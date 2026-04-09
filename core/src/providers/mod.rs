use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub id: ProviderId,
    pub capabilities: ProviderCapabilities,
    pub is_available: bool,
}
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
    pub num_ctx: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<super::session::ChatMessage>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_context_dir: Option<PathBuf>,
}

impl Default for PromptOptions {
    fn default() -> Self {
        Self {
            model: None,
            temperature: Some(0.7),
            system_prompt: None,
            num_ctx: None,
            messages: None,
            system_context_dir: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMResponse {
    pub text: String,
    pub provider: ProviderId,
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    #[serde(default)]
    pub is_estimated: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub chat: bool,
    pub tools: bool,
    pub fim: bool,
    #[serde(default)]
    pub supports_vision: bool,
    #[serde(default)]
    pub supports_thinking: bool,
    #[serde(default)]
    pub context_length: u32,
    #[serde(default)]
    pub vision: bool,
    pub supports_embedding: bool,
    pub max_context_tokens: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameter_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantization_level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_length: Option<usize>,
    #[serde(default)]
    pub capability_tags: Vec<String>,
    #[serde(default)]
    pub is_local: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_estimate_ms: Option<u32>,
}

impl TokenUsage {
    pub fn estimated(prompt_tokens: usize, completion_tokens: usize) -> Self {
        let total = prompt_tokens + completion_tokens;
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: total,
            is_estimated: true,
        }
    }

    pub fn exact(prompt_tokens: usize, completion_tokens: usize) -> Self {
        let total = prompt_tokens + completion_tokens;
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: total,
            is_estimated: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TokenEvent {
    Started,
    Token(String),
    Usage(TokenUsage),
    Completed,
}

#[derive(Debug, Error, Clone)]
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

    async fn get_model_info(&self, model: &str) -> Result<ProviderCapabilities, LLMError>;

    async fn capabilities(&self) -> Result<ProviderCapabilities, LLMError> {
        Err(LLMError::NotConfigured)
    }

    async fn health_check(&self) -> Result<bool, LLMError>;
    
    async fn warmup_model(&self, _model: &str) -> Result<(), LLMError> {
        Ok(())
    }
}

impl ProviderCapabilities {
    pub fn default_with_context(context_tokens: usize) -> Self {
        let context_length = context_tokens.min(u32::MAX as usize) as u32;
        Self {
            chat: true,
            tools: false,
            fim: false,
            supports_vision: false,
            supports_thinking: false,
            context_length,
            vision: false,
            supports_embedding: false,
            max_context_tokens: context_tokens,
            parameter_count: None,
            quantization_level: None,
            embedding_length: None,
            capability_tags: Vec::new(),
            is_local: false,
            latency_estimate_ms: None,
        }
    }
}
