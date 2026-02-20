use std::pin::Pin;

use async_trait::async_trait;
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod codex;
pub mod gemini;
pub mod ollama;

pub type TokenStream = Pin<Box<dyn Stream<Item = Result<TokenEvent, LLMError>> + Send>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Ollama,
    Gemini,
    Codex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptOptions {
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub system_prompt: Option<String>,
}

impl Default for PromptOptions {
    fn default() -> Self {
        Self {
            model: None,
            temperature: Some(0.7),
            system_prompt: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMResponse {
    pub text: String,
    pub provider: ProviderId,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TokenEvent {
    Started,
    Token(String),
    Completed,
}

#[derive(Debug, Error)]
pub enum LLMError {
    #[error("provider not configured")]
    NotConfigured,
    #[error("provider unavailable")]
    Unavailable,
    #[error("request timed out")]
    Timeout,
    #[error("remote rate limited")]
    RateLimited,
    #[error("http error: {0}")]
    Http(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("unexpected error: {0}")]
    Unexpected(String),
}

#[async_trait]
pub trait LLMProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;

    async fn send(&self, prompt: String, options: PromptOptions) -> Result<LLMResponse, LLMError>;

    async fn stream_send(
        &self,
        prompt: String,
        options: PromptOptions,
    ) -> Result<TokenStream, LLMError>;
}
