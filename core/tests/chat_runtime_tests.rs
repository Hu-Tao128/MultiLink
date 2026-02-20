use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use multilink_core::{
    ChatRuntime, LLMError, LLMProvider, LLMResponse, PromptOptions, ProviderId, ProviderRouter,
    StreamEvent, TokenEvent, TokenStream,
};

struct SlowMockProvider;

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

    async fn send(&self, _prompt: String, _options: PromptOptions) -> Result<LLMResponse, LLMError> {
        Ok(LLMResponse {
            text: "ok".to_string(),
            provider: ProviderId::Ollama,
            model: Some("mock".to_string()),
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
        }
    }

    assert!(saw_started);
    assert!(saw_finished);
    assert_eq!(output, "Hello");
}
