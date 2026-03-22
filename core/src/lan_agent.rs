use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;

use crate::chat_runtime::ChatRuntime;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanEnvelope {
    pub protocol_version: u8,
    pub request_id: String,
    pub timestamp_ms: u64,
    /// HMAC-SHA256 del payload serializado.
    /// Formato esperado: "sha256=<hex>".
    #[serde(default)]
    pub hmac_signature: String,
    pub payload: LanPayload,
}

impl LanEnvelope {
    const MAX_REQUEST_ID_LEN: usize = 64;
    const MAX_CLOCK_SKEW_MS: u64 = 300_000; // 5 minutes

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.protocol_version > 2 {
            return Err("unsupported protocol version");
        }
        if self.request_id.is_empty() {
            return Err("request_id cannot be empty");
        }
        if self.request_id.len() > Self::MAX_REQUEST_ID_LEN {
            return Err("request_id too long");
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if self.timestamp_ms > now.saturating_add(Self::MAX_CLOCK_SKEW_MS) {
            return Err("timestamp too far in future");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LanPayload {
    Ping,
    Dispatch {
        session_id: String,
        prompt: String,
        provider: Option<String>,
    },
    DispatchResponse {
        ok: bool,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        error: Option<String>,
    },
    Error {
        message: String,
    },
}

pub fn encode_messagepack<T: Serialize>(value: &T) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    rmp_serde::to_vec(value)
}

pub fn decode_messagepack<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
) -> Result<T, rmp_serde::decode::Error> {
    rmp_serde::from_slice(bytes)
}

pub fn shared_secret_matches(expected: &str, provided: &str) -> bool {
    if expected.is_empty() || provided.is_empty() {
        return false;
    }
    constant_time_compare(expected, provided)
}

fn constant_time_compare(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes()
        .iter()
        .zip(b.as_bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Valida la firma HMAC-SHA256 del payload.
/// `signature_header` debe tener formato `sha256=<hex>`.
pub fn verify_hmac(
    secret: &str,
    payload_bytes: &[u8],
    signature_header: &str,
) -> Result<(), &'static str> {
    let expected_hex = signature_header
        .strip_prefix("sha256=")
        .ok_or("invalid signature format")?;

    if expected_hex.is_empty() {
        return Err("empty signature");
    }

    let expected_bytes = hex::decode(expected_hex).map_err(|_| "invalid hex signature")?;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| "invalid secret")?;
    mac.update(payload_bytes);
    mac.verify_slice(&expected_bytes)
        .map_err(|_| "signature mismatch")
}

/// Firma un payload serializado en formato `sha256=<hex>`.
pub fn sign_payload(secret: &str, payload_bytes: &[u8]) -> Result<String, &'static str> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| "invalid secret")?;
    mac.update(payload_bytes);
    Ok(format!(
        "sha256={}",
        hex::encode(mac.finalize().into_bytes())
    ))
}

pub struct LanAgentServer {
    listener: TcpListener,
    runtime: Arc<ChatRuntime>,
    shared_secret: String,
    allowed_ips: Vec<IpAddr>,
    allow_remote: bool,
    shutdown: Arc<RwLock<bool>>,
}

impl LanAgentServer {
    pub async fn bind(
        addr: &str,
        runtime: Arc<ChatRuntime>,
        shared_secret: String,
        allowed_ips: Vec<String>,
        allow_remote: bool,
    ) -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind(addr).await?;

        let parsed_ips: Vec<IpAddr> = allowed_ips
            .iter()
            .filter_map(|ip| ip.parse().ok())
            .collect();

        Ok(Self {
            listener,
            runtime,
            shared_secret,
            allowed_ips: parsed_ips,
            allow_remote,
            shutdown: Arc::new(RwLock::new(false)),
        })
    }

    pub async fn run(&self) -> Result<(), std::io::Error> {
        loop {
            {
                let should_stop = self.shutdown.read().await;
                if *should_stop {
                    break;
                }
            }

            tokio::select! {
                result = self.listener.accept() => {
                    match result {
                        Ok((stream, client_addr)) => {
                            if !self.is_ip_allowed(&client_addr.ip()) {
                                eprintln!("LAN Agent: connection rejected from {}", client_addr.ip());
                                continue;
                            }

                            let runtime = self.runtime.clone();
                            let secret = self.shared_secret.clone();
                            let allow_remote = self.allow_remote;
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_connection(stream, runtime, secret, allow_remote).await {
                                    eprintln!("LAN Agent connection error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            eprintln!("LAN Agent accept error: {}", e);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn is_ip_allowed(&self, client_ip: &IpAddr) -> bool {
        is_ip_allowed(self.allowed_ips.as_slice(), self.allow_remote, client_ip)
    }

    async fn handle_connection(
        mut stream: TcpStream,
        runtime: Arc<ChatRuntime>,
        shared_secret: String,
        _allow_remote: bool,
    ) -> Result<(), std::io::Error> {
        let mut buffer = vec![0u8; 65536];

        let n = stream.read(&mut buffer).await?;
        if n == 0 {
            return Ok(());
        }

        let request: LanEnvelope = match decode_messagepack(&buffer[..n]) {
            Ok(req) => req,
            Err(e) => {
                let response = LanEnvelope {
                    protocol_version: 1,
                    request_id: String::new(),
                    timestamp_ms: unix_timestamp_ms(),
                    hmac_signature: String::new(),
                    payload: LanPayload::Error {
                        message: format!("decode error: {}", e),
                    },
                };
                Self::write_response(&mut stream, &response).await?;
                return Ok(());
            }
        };

        if let Err(e) = request.validate() {
            let response = LanEnvelope {
                protocol_version: request.protocol_version,
                request_id: request.request_id,
                timestamp_ms: unix_timestamp_ms(),
                hmac_signature: String::new(),
                payload: LanPayload::Error {
                    message: format!("validation error: {}", e),
                },
            };
            Self::write_response(&mut stream, &response).await?;
            return Ok(());
        }

        if shared_secret.is_empty() {
            let response = LanEnvelope {
                protocol_version: request.protocol_version,
                request_id: request.request_id,
                timestamp_ms: unix_timestamp_ms(),
                hmac_signature: String::new(),
                payload: LanPayload::Error {
                    message: "server not configured with shared secret".to_string(),
                },
            };
            Self::write_response(&mut stream, &response).await?;
            return Ok(());
        }

        let payload_bytes = match encode_messagepack(&request.payload) {
            Ok(bytes) => bytes,
            Err(_) => {
                let response = LanEnvelope {
                    protocol_version: request.protocol_version,
                    request_id: request.request_id,
                    timestamp_ms: unix_timestamp_ms(),
                    hmac_signature: String::new(),
                    payload: LanPayload::Error {
                        message: "internal error serializing payload for verification".to_string(),
                    },
                };
                Self::write_response(&mut stream, &response).await?;
                return Ok(());
            }
        };

        if verify_hmac(&shared_secret, &payload_bytes, &request.hmac_signature).is_err() {
            let response = LanEnvelope {
                protocol_version: request.protocol_version,
                request_id: request.request_id,
                timestamp_ms: unix_timestamp_ms(),
                hmac_signature: String::new(),
                payload: LanPayload::Error {
                    message: "unauthorized".to_string(),
                },
            };
            Self::write_response(&mut stream, &response).await?;
            return Ok(());
        }

        let response = Self::process_request(request, &runtime, &shared_secret).await;
        Self::write_response(&mut stream, &response).await?;

        Ok(())
    }

    async fn process_request(
        request: LanEnvelope,
        runtime: &Arc<ChatRuntime>,
        shared_secret: &str,
    ) -> LanEnvelope {
        if shared_secret.is_empty() {
            return LanEnvelope {
                protocol_version: request.protocol_version,
                request_id: request.request_id,
                timestamp_ms: unix_timestamp_ms(),
                hmac_signature: String::new(),
                payload: LanPayload::Error {
                    message: "server not configured with shared secret".to_string(),
                },
            };
        }

        let response_payload = match request.payload {
            LanPayload::Ping => LanPayload::DispatchResponse {
                ok: true,
                text: Some("pong".to_string()),
                error: None,
            },
            LanPayload::Dispatch {
                session_id,
                prompt,
                provider: _,
            } => match runtime.send_message(&session_id, prompt).await {
                Ok(_response) => LanPayload::DispatchResponse {
                    ok: true,
                    text: Some("message queued".to_string()),
                    error: None,
                },
                Err(e) => LanPayload::DispatchResponse {
                    ok: false,
                    text: None,
                    error: Some(e.to_string()),
                },
            },
            LanPayload::Error { message } => LanPayload::Error { message },
            LanPayload::DispatchResponse { .. } => LanPayload::Error {
                message: "unexpected response payload in request".to_string(),
            },
        };

        LanEnvelope {
            protocol_version: request.protocol_version,
            request_id: request.request_id,
            timestamp_ms: unix_timestamp_ms(),
            hmac_signature: String::new(),
            payload: response_payload,
        }
    }

    async fn write_response(
        stream: &mut TcpStream,
        response: &LanEnvelope,
    ) -> Result<(), std::io::Error> {
        let encoded = encode_messagepack(response)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        stream.write_all(&encoded).await?;
        stream.flush().await?;
        Ok(())
    }

    /// Devuelve la dirección local donde el servidor está escuchando.
    /// Útil en tests para obtener el puerto asignado automáticamente.
    pub fn local_addr(&self) -> Result<std::net::SocketAddr, std::io::Error> {
        self.listener.local_addr()
    }

    pub async fn shutdown(&self) {
        let mut guard = self.shutdown.write().await;
        *guard = true;
    }
}

fn unix_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn is_ip_allowed(allowed_ips: &[IpAddr], allow_remote: bool, client_ip: &IpAddr) -> bool {
    if client_ip.is_loopback() {
        return true;
    }
    if allowed_ips.is_empty() {
        return allow_remote;
    }
    allowed_ips.contains(client_ip)
}

/// Expuesto para integration tests — permite verificar la lógica de allowlist
/// sin necesitar una conexión TCP real desde IPs remotas.
pub fn ip_allowed_for_test(
    allowed_ips: &[IpAddr],
    allow_remote: bool,
    client_ip: &IpAddr,
) -> bool {
    is_ip_allowed(allowed_ips, allow_remote, client_ip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messagepack_roundtrip_works() {
        let env = LanEnvelope {
            protocol_version: 1,
            request_id: "req-1".to_string(),
            timestamp_ms: 123,
            hmac_signature: String::new(),
            payload: LanPayload::Ping,
        };

        let encoded = encode_messagepack(&env).expect("encode");
        let decoded: LanEnvelope = decode_messagepack(&encoded).expect("decode");
        assert_eq!(decoded.protocol_version, 1);
        assert_eq!(decoded.request_id, "req-1");
        matches!(decoded.payload, LanPayload::Ping);
    }

    #[test]
    fn shared_secret_compare_is_exact() {
        assert!(shared_secret_matches("abc", "abc"));
        assert!(!shared_secret_matches("abc", "abd"));
        assert!(!shared_secret_matches("", "abc"));
    }

    fn ping_payload_bytes() -> Vec<u8> {
        encode_messagepack(&LanPayload::Ping).expect("encode ping")
    }

    #[test]
    fn hmac_valido_acepta() {
        let secret = "test-secret-123";
        let payload = ping_payload_bytes();
        let sig = sign_payload(secret, &payload).expect("sign payload");
        assert!(verify_hmac(secret, &payload, &sig).is_ok());
    }

    #[test]
    fn hmac_sin_firma_rechaza() {
        let payload = ping_payload_bytes();
        assert!(verify_hmac("secret", &payload, "").is_err());
    }

    #[test]
    fn hmac_secret_incorrecto_rechaza() {
        let payload = ping_payload_bytes();
        let sig = sign_payload("other-secret", &payload).expect("sign payload");
        assert!(verify_hmac("secret", &payload, &sig).is_err());
    }

    #[test]
    fn hmac_payload_modificado_rechaza() {
        let secret = "mi-secret";
        let payload_original = ping_payload_bytes();
        let sig = sign_payload(secret, &payload_original).expect("sign payload");

        let payload_distinto = encode_messagepack(&LanPayload::Error {
            message: "tampered".to_string(),
        })
        .expect("encode payload");
        assert!(verify_hmac(secret, &payload_distinto, &sig).is_err());
    }

    #[test]
    fn allowlist_vacia_deniega_no_loopback() {
        let ip: IpAddr = "192.168.1.50".parse().expect("parse ip");
        assert!(!is_ip_allowed(&[], false, &ip));
    }

    #[test]
    fn allowlist_ip_permitida() {
        let ip: IpAddr = "192.168.1.50".parse().expect("parse ip");
        let list = vec![ip];
        assert!(is_ip_allowed(&list, false, &ip));
    }

    #[test]
    fn allowlist_ip_no_en_lista() {
        let allowed: IpAddr = "192.168.1.50".parse().expect("parse ip");
        let stranger: IpAddr = "192.168.1.99".parse().expect("parse ip");
        let list = vec![allowed];
        assert!(!is_ip_allowed(&list, true, &stranger));
    }
}
