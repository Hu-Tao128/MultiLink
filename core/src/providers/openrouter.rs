//! OpenRouter provider.
//!
//! OpenRouter exposes an OpenAI-compatible Chat Completions API:
//!
//! * Base URL: `https://openrouter.ai/api/v1`
//! * Auth: `Authorization: Bearer <OPENROUTER_API_KEY>`
//! * Streaming: SSE, terminated by `data: [DONE]`
//! * Models: provider-prefixed IDs like `anthropic/claude-sonnet-latest`,
//!   `openai/gpt-4o`, etc.
//!
//! The model field is passed through verbatim, allowing users to select any
//! model from the OpenRouter catalog.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{
    LLMError, LLMProvider, LLMResponse, PromptOptions, ProviderCapabilities, ProviderId,
    TokenEvent, TokenStream, TokenUsage,
};

pub const OPENROUTER_DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
pub const OPENROUTER_CHAT_MODEL: &str = "anthropic/claude-sonnet-latest";

/// OpenRouter context window — conservative default. Actual limits depend on
/// the chosen downstream model.
const OPENROUTER_CONTEXT_TOKENS: usize = 32_000;

#[derive(Clone)]
pub struct OpenRouterProvider {
    client: Client,
    base_url: String,
    api_key: Arc<RwLock<Option<String>>>,
    default_model: String,
}

impl OpenRouterProvider {
    const CONNECT_TIMEOUT_SECS: u64 = 15;
    const HTTP_TIMEOUT_SECS: u64 = 180;
    const STREAM_IDLE_TIMEOUT_SECS: u64 = 120;
    const RETRY_ATTEMPTS: usize = 3;

    pub fn new(
        base_url: String,
        api_key: Option<String>,
        default_model: String,
        timeout_secs: u64,
    ) -> Result<Self, LLMError> {
        let http_timeout = if timeout_secs == 0 {
            Self::HTTP_TIMEOUT_SECS
        } else {
            timeout_secs
        };

        let client = Client::builder()
            .connect_timeout(Duration::from_secs(Self::CONNECT_TIMEOUT_SECS))
            .timeout(Duration::from_secs(http_timeout))
            .pool_idle_timeout(Duration::from_secs(10))
            .tcp_keepalive(Duration::from_secs(30))
            .tcp_nodelay(true)
            .build()
            .map_err(|e| LLMError::Http(e.to_string()))?;

        let base_url = normalize_base_url(&base_url);

        Ok(Self {
            client,
            base_url,
            api_key: Arc::new(RwLock::new(api_key.filter(|key| !key.trim().is_empty()))),
            default_model,
        })
    }

    fn current_api_key(&self) -> Option<String> {
        self.api_key
            .read()
            .ok()
            .and_then(|guard| guard.clone())
            .filter(|key| !key.trim().is_empty())
    }

    pub fn default_base_url() -> &'static str {
        OPENROUTER_DEFAULT_BASE_URL
    }

    fn chat_endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    fn models_endpoint(&self) -> String {
        format!("{}/models", self.base_url)
    }

    fn model_for(&self, options: &PromptOptions) -> String {
        options
            .model
            .clone()
            .filter(|model| !model.trim().is_empty())
            .unwrap_or_else(|| self.default_model.clone())
    }

    fn build_messages(prompt: String, options: &PromptOptions) -> Vec<ChatMessagePayload> {
        if let Some(messages) = options.messages.as_ref() {
            return messages
                .iter()
                .filter(|message| !message.content.trim().is_empty())
                .map(|message| ChatMessagePayload {
                    role: message.role.clone(),
                    content: message.content.clone(),
                })
                .collect();
        }

        let mut messages = Vec::new();
        if let Some(system) = options.system_prompt.as_ref() {
            if !system.trim().is_empty() {
                messages.push(ChatMessagePayload {
                    role: "system".to_string(),
                    content: system.clone(),
                });
            }
        }
        messages.push(ChatMessagePayload {
            role: "user".to_string(),
            content: prompt,
        });
        messages
    }

    fn build_request(
        &self,
        model: String,
        messages: Vec<ChatMessagePayload>,
        stream: bool,
        requested_temperature: Option<f32>,
    ) -> ChatRequest {
        ChatRequest {
            model,
            messages,
            temperature: Some(requested_temperature.unwrap_or(0.7).clamp(0.0, 2.0)),
            stream,
            stream_options: if stream {
                Some(StreamOptions {
                    include_usage: true,
                })
            } else {
                None
            },
            // OpenRouter supports tools; the router/runtime injects them
            // separately via the tool registry. For now, we keep the base
            // request simple — tools are handled upstream.
            tools: None,
            tool_choice: None,
        }
    }

    async fn post_chat_with_retry<T: Serialize>(
        &self,
        body: &T,
    ) -> Result<reqwest::Response, LLMError> {
        let api_key = self.current_api_key().ok_or(LLMError::NotConfigured)?;
        let endpoint = self.chat_endpoint();

        let mut last_error: Option<LLMError> = None;
        let mut backoff = Duration::from_millis(300);

        for attempt in 0..Self::RETRY_ATTEMPTS {
            let result = self
                .client
                .post(&endpoint)
                .bearer_auth(&api_key)
                .json(body)
                .send()
                .await;

            match result {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    let status = response.status();
                    let body_text = response.text().await.unwrap_or_default();
                    let detail = if body_text.trim().is_empty() {
                        status.to_string()
                    } else {
                        format!("{}: {}", status, truncate(&body_text, 400))
                    };

                    if status.as_u16() == 429 {
                        last_error = Some(LLMError::RateLimited);
                    } else if !is_transient_status(status) || attempt + 1 == Self::RETRY_ATTEMPTS {
                        return Err(LLMError::Http(detail));
                    } else {
                        last_error = Some(LLMError::Http(detail));
                    }
                }
                Err(err) => {
                    let mapped = map_reqwest(&err);
                    if err.is_timeout() {
                        last_error = Some(LLMError::Timeout);
                    } else if !is_transient_transport_error(&err)
                        || attempt + 1 == Self::RETRY_ATTEMPTS
                    {
                        return Err(mapped);
                    } else {
                        last_error = Some(mapped);
                    }
                }
            }

            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(3));
        }

        Err(last_error.unwrap_or_else(|| LLMError::Unexpected("request failed".to_string())))
    }
}

#[async_trait]
impl LLMProvider for OpenRouterProvider {
    fn id(&self) -> ProviderId {
        ProviderId::OpenRouter
    }

    fn name(&self) -> &str {
        "OpenRouter"
    }

    fn is_available(&self) -> bool {
        self.current_api_key().is_some()
    }

    async fn send(&self, prompt: String, options: PromptOptions) -> Result<LLMResponse, LLMError> {
        let model = self.model_for(&options);
        let messages = Self::build_messages(prompt, &options);
        let body = self.build_request(model.clone(), messages, false, options.temperature);

        let response = self.post_chat_with_retry(&body).await?;

        let payload = response
            .json::<ChatCompletionResponse>()
            .await
            .map_err(|e| LLMError::Serialization(e.to_string()))?;

        let content = payload
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .unwrap_or_default();

        let usage = payload.usage.map(|usage| {
            TokenUsage::exact(
                usage.prompt_tokens as usize,
                usage.completion_tokens as usize,
            )
        });

        Ok(LLMResponse {
            text: content,
            provider: ProviderId::OpenRouter,
            model: payload.model.or(Some(model)),
            usage,
        })
    }

    async fn stream_send(
        &self,
        prompt: String,
        options: PromptOptions,
    ) -> Result<TokenStream, LLMError> {
        let model = self.model_for(&options);
        let messages = Self::build_messages(prompt, &options);
        let body = self.build_request(model, messages, true, options.temperature);

        let response = self.post_chat_with_retry(&body).await?;
        let byte_stream = response.bytes_stream();
        let idle_timeout = Duration::from_secs(Self::STREAM_IDLE_TIMEOUT_SECS);

        let stream = tokio_stream::wrappers::ReceiverStream::new({
            let (tx, rx) = tokio::sync::mpsc::channel(64);
            tokio::spawn(async move {
                let _ = tx.send(Ok(TokenEvent::Started)).await;

                tokio::pin!(byte_stream);
                let mut pending = Vec::<u8>::new();
                let mut pending_usage: Option<TokenUsage> = None;

                loop {
                    let next = tokio::time::timeout(idle_timeout, byte_stream.next()).await;

                    let bytes = match next {
                        Err(_) => {
                            let _ = tx.send(Err(LLMError::Timeout)).await;
                            return;
                        }
                        Ok(None) => break,
                        Ok(Some(Err(err))) => {
                            let _ = tx.send(Err(map_reqwest(&err))).await;
                            return;
                        }
                        Ok(Some(Ok(bytes))) => bytes,
                    };

                    pending.extend_from_slice(&bytes);

                    while let Some(newline_pos) = pending.iter().position(|byte| *byte == b'\n') {
                        let line_bytes: Vec<u8> = pending.drain(..=newline_pos).collect();
                        let line = String::from_utf8_lossy(&line_bytes);
                        let Some(event) = parse_sse_line(&line) else {
                            continue;
                        };

                        match event {
                            SseEvent::Done => {
                                if let Some(usage) = pending_usage.take() {
                                    let _ = tx.send(Ok(TokenEvent::Usage(usage))).await;
                                }
                                let _ = tx.send(Ok(TokenEvent::Completed)).await;
                                return;
                            }
                            SseEvent::Usage(usage) => {
                                pending_usage = Some(usage);
                            }
                            SseEvent::Delta { content, .. } if !content.is_empty() => {
                                let _ = tx.send(Ok(TokenEvent::Token(content))).await;
                            }
                            SseEvent::Delta { .. } | SseEvent::Ignore => {}
                        }
                    }
                }

                if !pending.is_empty() {
                    let line = String::from_utf8_lossy(&pending);
                    if let Some(event) = parse_sse_line(&line) {
                        match event {
                            SseEvent::Usage(usage) => pending_usage = Some(usage),
                            SseEvent::Delta { content, .. } if !content.is_empty() => {
                                let _ = tx.send(Ok(TokenEvent::Token(content))).await;
                            }
                            _ => {}
                        }
                    }
                }

                if let Some(usage) = pending_usage.take() {
                    let _ = tx.send(Ok(TokenEvent::Usage(usage))).await;
                }
                let _ = tx.send(Ok(TokenEvent::Completed)).await;
            });
            rx
        });

        Ok(Box::pin(stream))
    }

    async fn get_model_info(&self, model: &str) -> Result<ProviderCapabilities, LLMError> {
        let model = if model.trim().is_empty() {
            self.default_model.as_str()
        } else {
            model
        };
        Ok(model_capabilities(model))
    }

    async fn capabilities(&self) -> Result<ProviderCapabilities, LLMError> {
        Ok(model_capabilities(&self.default_model))
    }

    async fn health_check(&self) -> Result<bool, LLMError> {
        let Some(api_key) = self.current_api_key() else {
            return Ok(false);
        };

        match self
            .client
            .get(self.models_endpoint())
            .bearer_auth(&api_key)
            .send()
            .await
        {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    fn set_credential(&self, credential: Option<String>) {
        if let Ok(mut guard) = self.api_key.write() {
            *guard = credential.filter(|value| !value.trim().is_empty());
        }
    }
}

/// Static capability profile for an OpenRouter model.
pub fn model_capabilities(model: &str) -> ProviderCapabilities {
    // OpenRouter models vary widely; default to a safe middle ground.
    // Strong reasoning models may have 128k+ context.
    let is_reasoning = model.to_ascii_lowercase().contains("reason")
        || model.to_ascii_lowercase().contains("r1")
        || model.to_ascii_lowercase().contains("o1")
        || model.to_ascii_lowercase().contains("o3");

    let mut capability_tags = vec!["coding".to_string()];
    if is_reasoning {
        capability_tags.push("reasoning".to_string());
    }

    ProviderCapabilities {
        chat: true,
        tools: true,
        fim: false,
        supports_vision: model.to_ascii_lowercase().contains("vision")
            || model.to_ascii_lowercase().contains("claude"),
        supports_thinking: is_reasoning,
        context_length: OPENROUTER_CONTEXT_TOKENS as u32,
        vision: false,
        supports_embedding: false,
        max_context_tokens: OPENROUTER_CONTEXT_TOKENS,
        parameter_count: None,
        quantization_level: None,
        embedding_length: None,
        capability_tags,
        is_local: false,
        latency_estimate_ms: Some(800),
    }
}

#[derive(Debug, Clone, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessagePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessagePayload {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<UsagePayload>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    #[serde(default)]
    message: ChatMessageResponse,
}

#[derive(Debug, Default, Deserialize)]
struct ChatMessageResponse {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UsagePayload {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
    #[serde(default)]
    #[allow(dead_code)]
    total_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<UsagePayload>,
}

#[derive(Debug, Default, Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamDelta,
}

#[derive(Debug, Default, Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
}

/// Parsed event from a single SSE line.
#[derive(Debug, PartialEq)]
enum SseEvent {
    Done,
    Usage(TokenUsage),
    Delta { content: String, reasoning: String },
    Ignore,
}

fn parse_sse_line(line: &str) -> Option<SseEvent> {
    let line = line.trim_end_matches(['\r', '\n']).trim();
    if line.is_empty() || line.starts_with(':') {
        return None;
    }

    let payload = line.strip_prefix("data:")?.trim();
    if payload == "[DONE]" {
        return Some(SseEvent::Done);
    }

    let chunk: OpenRouterStreamChunk = serde_json::from_str(payload).ok()?;

    if let Some(usage) = chunk.usage {
        return Some(SseEvent::Usage(TokenUsage::exact(
            usage.prompt_tokens as usize,
            usage.completion_tokens as usize,
        )));
    }

    let content: String = chunk
        .choices
        .iter()
        .filter_map(|choice| choice.delta.content.clone())
        .collect();

    if content.is_empty() {
        return Some(SseEvent::Ignore);
    }

    Some(SseEvent::Delta {
        content,
        reasoning: String::new(),
    })
}

fn normalize_base_url(input: &str) -> String {
    let trimmed = input.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return OPENROUTER_DEFAULT_BASE_URL.to_string();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{}", trimmed)
    }
}

fn is_transient_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn is_transient_transport_error(err: &reqwest::Error) -> bool {
    err.is_connect() || err.is_timeout() || err.is_request()
}

fn map_reqwest(error: &reqwest::Error) -> LLMError {
    if error.is_timeout() {
        LLMError::Timeout
    } else {
        LLMError::Http(error.to_string())
    }
}

fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        value.to_string()
    } else {
        format!("{}...", &value[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openrouter_model_capabilities() {
        let caps = model_capabilities("anthropic/claude-sonnet-latest");
        assert!(caps.chat);
        assert!(caps.tools);
        assert!(!caps.is_local);

        let reasoning = model_capabilities("openai/o3-mini");
        assert!(reasoning.supports_thinking);
        assert!(reasoning.capability_tags.contains(&"reasoning".to_string()));
    }

    #[test]
    fn set_credential_toggles_availability() {
        let provider = OpenRouterProvider::new(
            OPENROUTER_DEFAULT_BASE_URL.to_string(),
            None,
            OPENROUTER_CHAT_MODEL.to_string(),
            0,
        )
        .expect("provider should build");

        assert!(!LLMProvider::is_available(&provider));
        provider.set_credential(Some("sk-or-test".to_string()));
        assert!(LLMProvider::is_available(&provider));
        provider.set_credential(Some("   ".to_string()));
        assert!(!LLMProvider::is_available(&provider));
    }

    #[test]
    fn parse_sse_done() {
        assert_eq!(parse_sse_line("data: [DONE]"), Some(SseEvent::Done));
    }

    #[test]
    fn build_messages_uses_system_prompt_then_user() {
        let options = PromptOptions {
            system_prompt: Some("Be concise".to_string()),
            ..PromptOptions::default()
        };
        let messages = OpenRouterProvider::build_messages("hello".to_string(), &options);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
    }
}
