//! Integration tests de seguridad para LanAgentServer.
//!
//! Cubren:
//! - Rechazo de conexiones de IPs no autorizadas
//! - Rechazo de requests sin firma HMAC
//! - Rechazo de requests con firma incorrecta
//! - Rechazo de requests con payload manipulado post-firma
//! - Rechazo cuando el servidor no tiene secret configurado
//! - Aceptación del flujo legítimo (Ping con firma válida)
//! - Loopback siempre permitido independientemente de allowlist
//! - allow_remote=true no bypasea la allowlist

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use multilink_core::lan_agent::{
    decode_messagepack, encode_messagepack, sign_payload, LanAgentServer, LanEnvelope, LanPayload,
};
use multilink_core::{
    ChatRuntime, LLMError, LLMProvider, LLMResponse, PromptOptions, ProviderCapabilities,
    ProviderId, ProviderRouter, TokenEvent, TokenStream,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

// ── Mock provider mínimo para construir un ChatRuntime en tests ──────────────

struct MinimalMockProvider;

#[async_trait]
impl LLMProvider for MinimalMockProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Ollama
    }
    fn name(&self) -> &str {
        "minimal-mock"
    }
    fn is_available(&self) -> bool {
        true
    }
    async fn send(&self, _: String, _: PromptOptions) -> Result<LLMResponse, LLMError> {
        Ok(LLMResponse {
            text: "ok".to_string(),
            provider: ProviderId::Ollama,
            model: None,
            usage: None,
        })
    }
    async fn stream_send(&self, _: String, _: PromptOptions) -> Result<TokenStream, LLMError> {
        let stream = tokio_stream::iter(vec![
            Ok(TokenEvent::Started),
            Ok(TokenEvent::Token("ok".to_string())),
            Ok(TokenEvent::Completed),
        ]);
        Ok(Box::pin(stream))
    }
    async fn get_model_info(&self, _: &str) -> Result<ProviderCapabilities, LLMError> {
        Ok(ProviderCapabilities::default_with_context(4096))
    }
    async fn health_check(&self) -> Result<bool, LLMError> {
        Ok(true)
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn make_runtime() -> Arc<ChatRuntime> {
    let mut router = ProviderRouter::new();
    router.register(Arc::new(MinimalMockProvider));
    let temp = std::env::temp_dir().join(format!(
        "multilink_lan_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    Arc::new(ChatRuntime::new(
        Arc::new(router),
        temp.join("sessions"),
        Duration::from_millis(100),
    ))
}

/// Arranca un LanAgentServer en un puerto libre y devuelve (server, addr).
/// El servidor corre en un task separado y se detiene al hacer `.shutdown()`.
async fn start_server(
    secret: &str,
    allowed_ips: Vec<&str>,
    allow_remote: bool,
) -> (Arc<LanAgentServer>, SocketAddr) {
    let runtime = make_runtime();
    let server = LanAgentServer::bind(
        "127.0.0.1:0",
        runtime,
        secret.to_string(),
        allowed_ips.iter().map(|s| s.to_string()).collect(),
        allow_remote,
    )
    .await
    .expect("bind server");

    let addr = server.local_addr().expect("local_addr");
    let server = Arc::new(server);
    let server_clone = server.clone();

    tokio::spawn(async move {
        let _ = server_clone.run().await;
    });

    // Dar un tick al runtime para que el accept loop arranque
    tokio::time::sleep(Duration::from_millis(5)).await;

    (server, addr)
}

/// Construye y envía un LanEnvelope firmado, devuelve el envelope de respuesta.
async fn send_signed(addr: SocketAddr, secret: &str, payload: LanPayload) -> LanEnvelope {
    let payload_bytes = encode_messagepack(&payload).expect("encode payload");
    let signature = sign_payload(secret, &payload_bytes).expect("sign");

    let envelope = LanEnvelope {
        protocol_version: 1,
        request_id: "test-req".to_string(),
        timestamp_ms: 0,
        hmac_signature: signature,
        payload,
    };

    let encoded = encode_messagepack(&envelope).expect("encode envelope");
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    stream.write_all(&encoded).await.expect("write");
    stream.flush().await.expect("flush");

    let mut buf = vec![0u8; 65536];
    let n = stream.read(&mut buf).await.expect("read");
    decode_messagepack(&buf[..n]).expect("decode response")
}

/// Envía un envelope con firma incorrecta (secret diferente).
async fn send_wrong_signature(addr: SocketAddr, payload: LanPayload) -> LanEnvelope {
    let payload_bytes = encode_messagepack(&payload).expect("encode payload");
    let signature = sign_payload("wrong-secret", &payload_bytes).expect("sign");

    let envelope = LanEnvelope {
        protocol_version: 1,
        request_id: "test-req".to_string(),
        timestamp_ms: 0,
        hmac_signature: signature,
        payload,
    };

    let encoded = encode_messagepack(&envelope).expect("encode envelope");
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    stream.write_all(&encoded).await.expect("write");
    stream.flush().await.expect("flush");

    let mut buf = vec![0u8; 65536];
    let n = stream.read(&mut buf).await.expect("read");
    decode_messagepack(&buf[..n]).expect("decode response")
}

/// Envía un envelope sin firma (hmac_signature vacío).
async fn send_unsigned(addr: SocketAddr, payload: LanPayload) -> LanEnvelope {
    let envelope = LanEnvelope {
        protocol_version: 1,
        request_id: "test-req".to_string(),
        timestamp_ms: 0,
        hmac_signature: String::new(),
        payload,
    };

    let encoded = encode_messagepack(&envelope).expect("encode envelope");
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    stream.write_all(&encoded).await.expect("write");
    stream.flush().await.expect("flush");

    let mut buf = vec![0u8; 65536];
    let n = stream.read(&mut buf).await.expect("read");
    decode_messagepack(&buf[..n]).expect("decode response")
}

fn is_unauthorized(envelope: &LanEnvelope) -> bool {
    matches!(&envelope.payload, LanPayload::Error { message } if message == "unauthorized")
}

fn is_error_containing(envelope: &LanEnvelope, substr: &str) -> bool {
    matches!(&envelope.payload, LanPayload::Error { message } if message.contains(substr))
}

fn is_pong(envelope: &LanEnvelope) -> bool {
    matches!(
        &envelope.payload,
        LanPayload::DispatchResponse { ok: true, text, .. } if text.as_deref() == Some("pong")
    )
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// Flujo legítimo: Ping firmado correctamente → pong.
#[tokio::test]
async fn lan_signed_ping_returns_pong() {
    let (server, addr) = start_server("super-secret", vec!["127.0.0.1"], false).await;

    let resp = send_signed(addr, "super-secret", LanPayload::Ping).await;

    assert!(is_pong(&resp), "respuesta inesperada: {:?}", resp.payload);
    server.shutdown().await;
}

/// Request sin firma → unauthorized.
#[tokio::test]
async fn lan_unsigned_request_is_rejected() {
    let (server, addr) = start_server("mi-secret", vec!["127.0.0.1"], false).await;

    let resp = send_unsigned(addr, LanPayload::Ping).await;

    assert!(
        is_unauthorized(&resp),
        "request sin firma debería ser rechazado, got: {:?}",
        resp.payload
    );
    server.shutdown().await;
}

/// Request con firma de secret incorrecto → unauthorized.
#[tokio::test]
async fn lan_wrong_secret_is_rejected() {
    let (server, addr) = start_server("correct-secret", vec!["127.0.0.1"], false).await;

    let resp = send_wrong_signature(addr, LanPayload::Ping).await;

    assert!(
        is_unauthorized(&resp),
        "request con secret incorrecto debería ser rechazado, got: {:?}",
        resp.payload
    );
    server.shutdown().await;
}

/// Payload manipulado después de firmar → unauthorized.
/// El cliente firma el payload original, pero envía uno modificado.
#[tokio::test]
async fn lan_tampered_payload_is_rejected() {
    let (server, addr) = start_server("tamper-secret", vec!["127.0.0.1"], false).await;

    // Firma el payload original (Ping)
    let original = LanPayload::Ping;
    let original_bytes = encode_messagepack(&original).expect("encode");
    let signature = sign_payload("tamper-secret", &original_bytes).expect("sign");

    // Pero envía un payload diferente con esa firma
    let tampered = LanPayload::Error {
        message: "injected".to_string(),
    };
    let envelope = LanEnvelope {
        protocol_version: 1,
        request_id: "tamper".to_string(),
        timestamp_ms: 0,
        hmac_signature: signature, // firma del original
        payload: tampered,         // payload diferente
    };

    let encoded = encode_messagepack(&envelope).expect("encode");
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    stream.write_all(&encoded).await.expect("write");
    stream.flush().await.expect("flush");

    let mut buf = vec![0u8; 65536];
    let n = stream.read(&mut buf).await.expect("read");
    let resp: LanEnvelope = decode_messagepack(&buf[..n]).expect("decode");

    assert!(
        is_unauthorized(&resp),
        "payload manipulado debería ser rechazado, got: {:?}",
        resp.payload
    );
    server.shutdown().await;
}

/// Servidor sin secret configurado → error de configuración, no cuelgue.
#[tokio::test]
async fn lan_server_without_secret_returns_config_error() {
    // Secret vacío — el servidor debe rechazar todos los requests
    let (server, addr) = start_server("", vec!["127.0.0.1"], false).await;

    // Intentamos conectar y enviar cualquier cosa
    let envelope = LanEnvelope {
        protocol_version: 1,
        request_id: "req".to_string(),
        timestamp_ms: 0,
        hmac_signature: String::new(),
        payload: LanPayload::Ping,
    };
    let encoded = encode_messagepack(&envelope).expect("encode");
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    stream.write_all(&encoded).await.expect("write");
    stream.flush().await.expect("flush");

    let mut buf = vec![0u8; 65536];
    let n = stream.read(&mut buf).await.expect("read");
    let resp: LanEnvelope = decode_messagepack(&buf[..n]).expect("decode");

    assert!(
        is_error_containing(&resp, "secret"),
        "servidor sin secret debe retornar error de configuración, got: {:?}",
        resp.payload
    );
    server.shutdown().await;
}

/// Allowlist vacía deniega IPs no-loopback solo cuando allow_remote=false.
#[tokio::test]
async fn lan_empty_allowlist_denies_non_loopback() {
    use std::net::IpAddr;
    let loopback: IpAddr = "127.0.0.1".parse().unwrap();
    let remote: IpAddr = "192.168.1.50".parse().unwrap();

    assert!(
        multilink_core::lan_agent::ip_allowed_for_test(&[], false, &loopback),
        "loopback siempre permitido"
    );
    assert!(
        !multilink_core::lan_agent::ip_allowed_for_test(&[], false, &remote),
        "allow_remote=false + lista vacía debe denegar IPs no-loopback"
    );
}

/// allow_remote=true NO bypasea la allowlist.
#[tokio::test]
async fn lan_allow_remote_does_not_bypass_allowlist() {
    use std::net::IpAddr;

    let allowed: IpAddr = "192.168.1.10".parse().unwrap();
    let not_allowed: IpAddr = "192.168.1.99".parse().unwrap();
    let list = vec![allowed];

    // IP en lista con allow_remote=true → permitida
    assert!(multilink_core::lan_agent::ip_allowed_for_test(
        &list, true, &allowed
    ));
    // IP fuera de lista con allow_remote=true → denegada
    assert!(!multilink_core::lan_agent::ip_allowed_for_test(
        &list,
        true,
        &not_allowed
    ));
}

/// allow_remote=true con allowlist vacía permite IPs no-loopback.
#[tokio::test]
async fn lan_allow_remote_with_empty_allowlist_allows_non_loopback() {
    use std::net::IpAddr;
    let remote: IpAddr = "192.168.1.50".parse().unwrap();
    assert!(
        multilink_core::lan_agent::ip_allowed_for_test(&[], true, &remote),
        "allow_remote=true con lista vacía debe permitir IPs remotas"
    );
}

/// Múltiples requests con firma válida en la misma sesión del servidor.
#[tokio::test]
async fn lan_multiple_valid_requests_all_succeed() {
    let (server, addr) = start_server("multi-secret", vec!["127.0.0.1"], false).await;

    for i in 0..3 {
        let resp = send_signed(addr, "multi-secret", LanPayload::Ping).await;
        assert!(
            is_pong(&resp),
            "request {} debería ser pong, got: {:?}",
            i,
            resp.payload
        );
    }

    server.shutdown().await;
}

/// Mezcla de requests válidos e inválidos: los inválidos son rechazados
/// sin afectar los siguientes válidos.
#[tokio::test]
async fn lan_invalid_requests_do_not_poison_server() {
    let (server, addr) = start_server("poison-secret", vec!["127.0.0.1"], false).await;

    // Request inválido
    let bad = send_wrong_signature(addr, LanPayload::Ping).await;
    assert!(is_unauthorized(&bad));

    // Request válido después del inválido — el servidor sigue funcionando
    let good = send_signed(addr, "poison-secret", LanPayload::Ping).await;
    assert!(
        is_pong(&good),
        "servidor debe seguir funcionando tras rechazar request inválido"
    );

    server.shutdown().await;
}
