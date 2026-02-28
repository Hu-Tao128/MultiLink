use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use multilink_core::config::RuntimeConfig;
use multilink_core::session::{ChatMessage, SessionState};
use multilink_core::{
    ChatRuntime, LLMError, LLMProvider, LLMResponse, PromptOptions, ProviderCapabilities,
    ProviderId, ProviderRouter, StreamEvent, TokenEvent, TokenStream,
};
use tokio::fs;

struct SlowMockProvider;

struct HoldingMockProvider;

#[async_trait]
impl LLMProvider for SlowMockProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Ollama
    }

    fn name(&self) -> &str {
        "slow-mock"
    }

    fn is_available(&self) -> bool {
        true
    }

    async fn send(
        &self,
        _prompt: String,
        _options: PromptOptions,
    ) -> Result<LLMResponse, LLMError> {
        Ok(LLMResponse {
            text: "ok".to_string(),
            provider: ProviderId::Ollama,
            model: Some("mock".to_string()),
            usage: None,
        })
    }

    async fn stream_send(
        &self,
        _prompt: String,
        _options: PromptOptions,
    ) -> Result<TokenStream, LLMError> {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            let _ = tx.send(Ok(TokenEvent::Started)).await;
            let _ = tx.send(Ok(TokenEvent::Token("Hel".to_string()))).await;
            tokio::time::sleep(Duration::from_millis(80)).await;
            let _ = tx.send(Ok(TokenEvent::Token("lo".to_string()))).await;
            tokio::time::sleep(Duration::from_millis(80)).await;
            let _ = tx.send(Ok(TokenEvent::Completed)).await;
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn get_model_info(&self, _model: &str) -> Result<ProviderCapabilities, LLMError> {
        Ok(ProviderCapabilities::default_with_context(4096))
    }
}

#[async_trait]
impl LLMProvider for HoldingMockProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Ollama
    }

    fn name(&self) -> &str {
        "holding-mock"
    }

    fn is_available(&self) -> bool {
        true
    }

    async fn send(
        &self,
        _prompt: String,
        _options: PromptOptions,
    ) -> Result<LLMResponse, LLMError> {
        Ok(LLMResponse {
            text: "ok".to_string(),
            provider: ProviderId::Ollama,
            model: Some("mock".to_string()),
            usage: None,
        })
    }

    async fn stream_send(
        &self,
        _prompt: String,
        _options: PromptOptions,
    ) -> Result<TokenStream, LLMError> {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            let _ = tx.send(Ok(TokenEvent::Started)).await;
            let _ = tx.send(Ok(TokenEvent::Token("busy".to_string()))).await;
            tokio::time::sleep(Duration::from_millis(220)).await;
            let _ = tx.send(Ok(TokenEvent::Completed)).await;
        });
        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn get_model_info(&self, _model: &str) -> Result<ProviderCapabilities, LLMError> {
        Ok(ProviderCapabilities::default_with_context(4096))
    }
}

#[tokio::test]
async fn runtime_streams_and_persists_session() {
    let mut router = ProviderRouter::new();
    router.register(Arc::new(SlowMockProvider));

    let temp = tempfile::tempdir().expect("temp");
    let runtime = ChatRuntime::new(
        Arc::new(router),
        temp.path().join("sessions"),
        Duration::from_millis(40),
    );

    let session_id = runtime
        .create_session(ProviderId::Ollama, Some("mock".to_string()))
        .await;

    let mut rx = runtime
        .send_message(&session_id, "hello".to_string())
        .await
        .expect("send");

    let mut saw_started = false;
    let mut saw_finished = false;
    let mut output = String::new();

    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::Started => saw_started = true,
            StreamEvent::Chunk(chunk) => output.push_str(&chunk),
            StreamEvent::Finished => {
                saw_finished = true;
                break;
            }
            StreamEvent::Error(message) => panic!("unexpected error: {message}"),
            StreamEvent::Usage { .. } => {}
        }
    }

    assert!(saw_started);
    assert!(saw_finished);
    assert_eq!(output, "Hello");
}

#[tokio::test]
async fn runtime_recovers_partial_wal_on_load() {
    let mut router = ProviderRouter::new();
    router.register(Arc::new(SlowMockProvider));

    let temp = tempfile::tempdir().expect("temp");
    let storage_dir = temp.path().join("sessions");

    let runtime = ChatRuntime::new(
        Arc::new(router),
        storage_dir.clone(),
        Duration::from_millis(40),
    );

    fs::create_dir_all(&storage_dir)
        .await
        .expect("create storage dir");

    let session_id = runtime
        .create_session(ProviderId::Ollama, Some("mock".to_string()))
        .await;

    let mut sessions = runtime.list_sessions().await;
    let session = sessions
        .iter_mut()
        .find(|s| s.id == session_id)
        .expect("session exists");
    session.messages.push(ChatMessage::user("hola".to_string()));
    session.state = SessionState::Streaming;

    let session_path = storage_dir.join(format!("{}.json", session_id));
    let bytes = serde_json::to_vec_pretty(session).expect("serialize session");
    fs::write(&session_path, bytes)
        .await
        .expect("write session file");

    let wal_path = storage_dir.join(format!("{}.partial.log", session_id));
    fs::write(&wal_path, "respuesta parcial")
        .await
        .expect("write partial wal");

    let mut router2 = ProviderRouter::new();
    router2.register(Arc::new(SlowMockProvider));
    let runtime_reloaded = ChatRuntime::new(
        Arc::new(router2),
        storage_dir.clone(),
        Duration::from_millis(40),
    );

    runtime_reloaded
        .load_sessions_from_disk()
        .await
        .expect("load sessions");

    let messages = runtime_reloaded
        .list_messages(&session_id)
        .await
        .expect("messages loaded");
    let last = messages.last().expect("last message exists");
    assert_eq!(last.role, "assistant");
    assert_eq!(last.content, "respuesta parcial");

    assert!(
        !wal_path.exists(),
        "partial wal should be removed after recovery"
    );
}

#[tokio::test]
async fn runtime_project_context_skips_oversized_files() {
    let mut router = ProviderRouter::new();
    router.register(Arc::new(SlowMockProvider));

    let temp = tempfile::tempdir().expect("temp");
    let storage_dir = temp.path().join("sessions");
    let project_root = temp.path().join("project");
    fs::create_dir_all(&project_root)
        .await
        .expect("create project root");

    fs::write(project_root.join("main.rs"), "fn main() {}")
        .await
        .expect("write small file");
    fs::write(project_root.join("big.rs"), "x".repeat(2048))
        .await
        .expect("write large file");

    let runtime = ChatRuntime::new_with_config(
        Arc::new(router),
        storage_dir,
        Duration::from_millis(40),
        RuntimeConfig {
            max_project_file_bytes: 256,
            ..RuntimeConfig::default()
        },
        None,
    );

    let session_id = runtime
        .create_session(ProviderId::Ollama, Some("mock".to_string()))
        .await;

    runtime
        .set_session_project_root(&session_id, Some(project_root.to_string_lossy().to_string()))
        .await
        .expect("set project root");

    let sessions = runtime.list_sessions().await;
    let session = sessions
        .iter()
        .find(|s| s.id == session_id)
        .expect("session exists");
    let context = session.project_context.clone().unwrap_or_default();

    assert!(context.contains("main.rs"));
    assert!(!context.contains("big.rs"));
}

#[tokio::test]
async fn runtime_limits_parallel_streams() {
    let mut router = ProviderRouter::new();
    router.register(Arc::new(HoldingMockProvider));

    let temp = tempfile::tempdir().expect("temp");
    let runtime = ChatRuntime::new_with_config(
        Arc::new(router),
        temp.path().join("sessions"),
        Duration::from_millis(40),
        RuntimeConfig {
            max_parallel_streams: 1,
            ..RuntimeConfig::default()
        },
        None,
    );

    let first = runtime
        .create_session(ProviderId::Ollama, Some("mock".to_string()))
        .await;
    let second = runtime
        .create_session(ProviderId::Ollama, Some("mock".to_string()))
        .await;

    let mut first_rx = runtime
        .send_message(&first, "one".to_string())
        .await
        .expect("first send");

    let blocked = tokio::time::timeout(
        Duration::from_millis(60),
        runtime.send_message(&second, "two".to_string()),
    )
    .await;
    assert!(blocked.is_err(), "second send should wait for slot");

    while let Some(event) = first_rx.recv().await {
        if matches!(event, StreamEvent::Finished) {
            break;
        }
    }

    let second_start = tokio::time::timeout(
        Duration::from_millis(300),
        runtime.send_message(&second, "two".to_string()),
    )
    .await;
    assert!(second_start.is_ok(), "second send should proceed after slot frees");
}
