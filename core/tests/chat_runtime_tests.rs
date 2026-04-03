use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use multilink_core::config::{RuntimeConfig, RuntimeProfile, RuntimeProfiles};
use multilink_core::session::{ChatMessage, SessionState};
use multilink_core::{
    ChatRuntime, LLMError, LLMProvider, LLMResponse, PromptOptions, ProviderCapabilities,
    ProviderId, ProviderRouter, StreamEvent, TokenEvent, TokenStream,
};
use tokio::fs;

struct SlowMockProvider;

struct HoldingMockProvider;

struct DelayedFirstTokenProvider;

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

    async fn health_check(&self) -> Result<bool, LLMError> {
        Ok(true)
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

    async fn health_check(&self) -> Result<bool, LLMError> {
        Ok(true)
    }
}

#[async_trait]
impl LLMProvider for DelayedFirstTokenProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Ollama
    }

    fn name(&self) -> &str {
        "delayed-first-token-mock"
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
            tokio::time::sleep(Duration::from_secs(2)).await;
            let _ = tx.send(Ok(TokenEvent::Token("late".to_string()))).await;
            let _ = tx.send(Ok(TokenEvent::Completed)).await;
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn get_model_info(&self, _model: &str) -> Result<ProviderCapabilities, LLMError> {
        Ok(ProviderCapabilities::default_with_context(4096))
    }

    async fn health_check(&self) -> Result<bool, LLMError> {
        Ok(true)
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
async fn runtime_persists_new_session_metadata_immediately() {
    let mut router = ProviderRouter::new();
    router.register(Arc::new(SlowMockProvider));

    let temp = tempfile::tempdir().expect("temp");
    let storage_dir = temp.path().join("sessions");

    let runtime = ChatRuntime::new(
        Arc::new(router),
        storage_dir.clone(),
        Duration::from_millis(40),
    );

    let session_id = runtime
        .create_session(ProviderId::Ollama, Some("mock".to_string()))
        .await;

    let session_path = storage_dir.join(format!("{}.json", session_id));
    let index_path = storage_dir.join("index.json");
    assert!(
        session_path.exists(),
        "new session file should be persisted"
    );
    assert!(index_path.exists(), "session index should be persisted");

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

    let sessions = runtime_reloaded.list_sessions().await;
    assert!(
        sessions.iter().any(|s| s.id == session_id),
        "reloaded runtime should include newly created session"
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
        .set_session_project_root(
            &session_id,
            Some(project_root.to_string_lossy().to_string()),
        )
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
async fn runtime_project_context_ignores_venv_and_prefers_root_files() {
    let mut router = ProviderRouter::new();
    router.register(Arc::new(SlowMockProvider));

    let temp = tempfile::tempdir().expect("temp");
    let storage_dir = temp.path().join("sessions");
    let project_root = temp.path().join("project");
    fs::create_dir_all(&project_root)
        .await
        .expect("create project root");

    fs::write(project_root.join("README.md"), "# Cast project")
        .await
        .expect("write readme");
    fs::write(project_root.join("chrome.py"), "print('chrome')")
        .await
        .expect("write chrome.py");
    fs::write(project_root.join("tts.py"), "print('tts')")
        .await
        .expect("write tts.py");

    let venv_pkg = project_root.join(".venv/lib/python3.13/site-packages/certifi");
    fs::create_dir_all(&venv_pkg)
        .await
        .expect("create venv folder");
    fs::write(venv_pkg.join("core.py"), "def where(): pass")
        .await
        .expect("write venv file");

    let runtime = ChatRuntime::new_with_config(
        Arc::new(router),
        storage_dir,
        Duration::from_millis(40),
        RuntimeConfig {
            max_project_files: 2,
            profiles: RuntimeProfiles {
                small: RuntimeProfile {
                    max_project_context_tokens: 800,
                    max_project_files: 2,
                },
                medium: RuntimeProfile {
                    max_project_context_tokens: 1200,
                    max_project_files: 2,
                },
                large: RuntimeProfile {
                    max_project_context_tokens: 2000,
                    max_project_files: 2,
                },
            },
            ..RuntimeConfig::default()
        },
        None,
    );

    let session_id = runtime
        .create_session(ProviderId::Ollama, Some("deepseek-coder:1.3b".to_string()))
        .await;

    runtime
        .set_session_project_root(
            &session_id,
            Some(project_root.to_string_lossy().to_string()),
        )
        .await
        .expect("set project root");

    let sessions = runtime.list_sessions().await;
    let session = sessions
        .iter()
        .find(|s| s.id == session_id)
        .expect("session exists");
    let context = session.project_context.clone().unwrap_or_default();

    assert!(context.contains("README.md"), "should include root readme");
    assert!(
        context.contains("chrome.py") || context.contains("tts.py"),
        "should include root code file"
    );
    assert!(
        !context.contains(".venv/") && !context.contains("site-packages"),
        "should exclude virtualenv/vendor files"
    );
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
    assert!(
        second_start.is_ok(),
        "second send should proceed after slot frees"
    );
}

#[tokio::test]
async fn runtime_times_out_when_first_token_is_too_slow() {
    let mut router = ProviderRouter::new();
    router.register(Arc::new(DelayedFirstTokenProvider));

    let temp = tempfile::tempdir().expect("temp");
    let runtime = ChatRuntime::new_with_config(
        Arc::new(router),
        temp.path().join("sessions"),
        Duration::from_millis(40),
        RuntimeConfig {
            stream_first_token_timeout_secs: 1,
            ..RuntimeConfig::default()
        },
        None,
    );

    let session_id = runtime
        .create_session(ProviderId::Ollama, Some("mock".to_string()))
        .await;

    let mut rx = runtime
        .send_message(&session_id, "hello".to_string())
        .await
        .expect("send");

    let mut saw_started = false;
    let mut saw_timeout_error = false;

    while let Ok(Some(event)) = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await {
        match event {
            StreamEvent::Started => saw_started = true,
            StreamEvent::Error(message) => {
                if message.contains("primer token") {
                    saw_timeout_error = true;
                }
                break;
            }
            StreamEvent::Finished => break,
            StreamEvent::Chunk(_) | StreamEvent::Usage { .. } => {}
        }
    }

    assert!(
        saw_started,
        "stream should start before first-token timeout"
    );
    assert!(
        saw_timeout_error,
        "runtime should emit first-token-timeout error when no chunk arrives in time"
    );
}

#[tokio::test]
async fn runtime_extends_first_token_timeout_for_thinking_models() {
    let mut router = ProviderRouter::new();
    router.register(Arc::new(DelayedFirstTokenProvider));

    let temp = tempfile::tempdir().expect("temp");
    let runtime = ChatRuntime::new_with_config(
        Arc::new(router),
        temp.path().join("sessions"),
        Duration::from_millis(40),
        RuntimeConfig {
            stream_first_token_timeout_secs: 1,
            thinking_model_timeout_multiplier: 5,
            ..RuntimeConfig::default()
        },
        None,
    );

    let session_id = runtime
        .create_session(ProviderId::Ollama, Some("deepseek-r1:14b".to_string()))
        .await;

    let mut rx = runtime
        .send_message(&session_id, "hello".to_string())
        .await
        .expect("send");

    let mut saw_chunk = false;
    let mut saw_finished = false;
    let mut saw_timeout_error = false;

    while let Ok(Some(event)) = tokio::time::timeout(Duration::from_secs(7), rx.recv()).await {
        match event {
            StreamEvent::Chunk(chunk) => {
                if chunk.contains("late") {
                    saw_chunk = true;
                }
            }
            StreamEvent::Error(message) => {
                if message.contains("primer token") {
                    saw_timeout_error = true;
                }
                break;
            }
            StreamEvent::Finished => {
                saw_finished = true;
                break;
            }
            StreamEvent::Started | StreamEvent::Usage { .. } => {}
        }
    }

    assert!(
        !saw_timeout_error,
        "thinking model should not hit first-token timeout with multiplier"
    );
    assert!(
        saw_chunk,
        "thinking model stream should receive delayed chunk"
    );
    assert!(
        saw_finished,
        "thinking model stream should finish when timeout is extended"
    );
}

#[tokio::test]
async fn runtime_start_bootstraps_router_health_states() {
    let mut router = ProviderRouter::new();
    router.register(Arc::new(SlowMockProvider));
    let router = Arc::new(router);

    let temp = tempfile::tempdir().expect("temp");
    let runtime = ChatRuntime::new(
        router.clone(),
        temp.path().join("sessions"),
        Duration::from_millis(40),
    );

    runtime.start().await.expect("runtime start");

    assert!(router.health_state(ProviderId::Ollama).await.is_some());
    assert!(router.health_state(ProviderId::Gemini).await.is_some());
    assert!(router.health_state(ProviderId::Codex).await.is_some());

    runtime.stop().await;
}
