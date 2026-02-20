use std::sync::Arc;

use async_trait::async_trait;
use multilink_core::{
    LLMError, LLMProvider, LLMResponse, PromptOptions, ProviderId, ProviderRouter, TokenEvent,
    TokenStream,
};

struct MockProvider {
    id: ProviderId,
    available: bool,
}

#[async_trait]
impl LLMProvider for MockProvider {
    fn id(&self) -> ProviderId {
        self.id
    }

    fn name(&self) -> &str {
        "mock"
    }

    fn is_available(&self) -> bool {
        self.available
    }

    async fn send(&self, _prompt: String, _options: PromptOptions) -> Result<LLMResponse, LLMError> {
        Ok(LLMResponse {
            text: "ok".to_string(),
            provider: self.id,
            model: Some("mock".to_string()),
        })
    }

    async fn stream_send(
        &self,
        _prompt: String,
        _options: PromptOptions,
    ) -> Result<TokenStream, LLMError> {
        let stream = tokio_stream::iter(vec![
            Ok(TokenEvent::Started),
            Ok(TokenEvent::Token("ok".to_string())),
            Ok(TokenEvent::Completed),
        ]);
        Ok(Box::pin(stream))
    }
}

#[tokio::test]
async fn router_falls_back_when_preferred_unavailable() {
    let mut router = ProviderRouter::new();
    router.register(Arc::new(MockProvider {
        id: ProviderId::Gemini,
        available: false,
    }));
    router.register(Arc::new(MockProvider {
        id: ProviderId::Ollama,
        available: true,
    }));

    let response = router
        .send(
            ProviderId::Gemini,
            "hello".to_string(),
            PromptOptions::default(),
        )
        .await
        .expect("router should fallback");

    assert_eq!(response.provider, ProviderId::Ollama);
}
