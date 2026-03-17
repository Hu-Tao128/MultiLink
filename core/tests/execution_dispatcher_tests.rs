use std::sync::Arc;

use async_trait::async_trait;
use multilink_core::config::ExecutionServerRuntime;
use multilink_core::{
    ExecutionDispatchRequest, ExecutionDispatcher, LLMError, LLMProvider, LLMResponse,
    PromptOptions, ProviderCapabilities, ProviderId, ProviderRouter, TokenStream,
};

struct FailingPrimaryProvider;

#[async_trait]
impl LLMProvider for FailingPrimaryProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Ollama
    }

    fn name(&self) -> &str {
        "failing-primary"
    }

    fn is_available(&self) -> bool {
        true
    }

    async fn send(
        &self,
        _prompt: String,
        _options: PromptOptions,
    ) -> Result<LLMResponse, LLMError> {
        Err(LLMError::Unexpected("primary fail".to_string()))
    }

    async fn stream_send(
        &self,
        _prompt: String,
        _options: PromptOptions,
    ) -> Result<TokenStream, LLMError> {
        Err(LLMError::Unexpected("primary fail".to_string()))
    }

    async fn get_model_info(&self, _model: &str) -> Result<ProviderCapabilities, LLMError> {
        Ok(ProviderCapabilities::default_with_context(4096))
    }

    async fn health_check(&self) -> Result<bool, LLMError> {
        Ok(true)
    }
}

fn dispatcher_with_unreachable_remote() -> ExecutionDispatcher {
    let mut router = ProviderRouter::new();
    router.register(Arc::new(FailingPrimaryProvider));

    ExecutionDispatcher::new(
        Arc::new(router),
        vec![ExecutionServerRuntime {
            name: "Remote fallback".to_string(),
            base_url: "http://127.0.0.1:9".to_string(),
            default_model: "qwen2.5-coder:3b".to_string(),
            priority: 1,
            enabled: true,
            max_concurrency: 1,
        }],
    )
}

#[tokio::test]
async fn dispatcher_keeps_primary_error_when_remote_fallback_disallowed() {
    let dispatcher = dispatcher_with_unreachable_remote();

    let err = match dispatcher
        .dispatch(ExecutionDispatchRequest {
            provider: ProviderId::Ollama,
            prompt: "hello".to_string(),
            options: PromptOptions::default(),
            allow_remote_fallback: false,
        })
        .await
    {
        Ok(_) => panic!("dispatch should fail"),
        Err(err) => err,
    };

    assert!(
        err.to_string().contains("primary fail"),
        "expected primary error when remote fallback is disabled"
    );
}

#[tokio::test]
async fn dispatcher_attempts_remote_fallback_when_allowed() {
    let dispatcher = dispatcher_with_unreachable_remote();

    let err = match dispatcher
        .dispatch(ExecutionDispatchRequest {
            provider: ProviderId::Ollama,
            prompt: "hello".to_string(),
            options: PromptOptions::default(),
            allow_remote_fallback: true,
        })
        .await
    {
        Ok(_) => panic!("dispatch should fail"),
        Err(err) => err,
    };

    assert!(
        !err.to_string().contains("primary fail"),
        "expected fallback attempt to replace primary error"
    );
}
