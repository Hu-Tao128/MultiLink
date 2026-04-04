use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::time::sleep;

use super::{
    LLMError, LLMProvider, LLMResponse, PromptOptions, ProviderCapabilities, ProviderId,
    TokenEvent, TokenStream, TokenUsage,
};
use crate::session::ChatMessage;

#[derive(Clone)]
pub struct OllamaProvider {
    client: Client,
    base_url: String,
    default_model: String,
    model_cache: Arc<RwLock<HashMap<String, CachedModelInfo>>>,
}

#[derive(Clone)]
struct CachedModelInfo {
    capabilities: ProviderCapabilities,
    cached_at: Instant,
}

impl OllamaProvider {
    const RETRY_ATTEMPTS: usize = 3;
    const CACHE_TTL_SECS: u64 = 300;

    fn env_u64(name: &str, default: u64, min: u64, max: u64) -> u64 {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(|v| v.clamp(min, max))
            .unwrap_or(default)
    }

    fn connect_timeout_secs() -> u64 {
        Self::env_u64("MULTILINK_OLLAMA_CONNECT_TIMEOUT_SECS", 10, 1, 120)
    }

    fn http_timeout_secs() -> u64 {
        // End-to-end timeout for long generations.
        Self::env_u64("MULTILINK_OLLAMA_HTTP_TIMEOUT_SECS", 1800, 60, 7200)
    }

    fn stream_idle_timeout_secs() -> u64 {
        // Idle (no bytes) timeout, not total generation time.
        Self::env_u64("MULTILINK_OLLAMA_STREAM_IDLE_TIMEOUT_SECS", 180, 30, 3600)
    }

    fn stream_retries() -> usize {
        Self::env_u64("MULTILINK_OLLAMA_STREAM_RETRIES", 4, 0, 10) as usize
    }

    pub fn new(base_url: String, default_model: String) -> Self {
        let connect_timeout_secs = Self::connect_timeout_secs();
        let http_timeout_secs = Self::http_timeout_secs();

        let client = Client::builder()
            .connect_timeout(Duration::from_secs(connect_timeout_secs))
            .timeout(Duration::from_secs(http_timeout_secs))
            .pool_idle_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(1)
            .tcp_keepalive(Duration::from_secs(30))
            .tcp_nodelay(true)
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            client,
            base_url,
            default_model,
            model_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn model_for(&self, options: &PromptOptions) -> String {
        options
            .model
            .clone()
            .unwrap_or_else(|| self.default_model.clone())
    }

    fn healthcheck_socket_addr(&self) -> Option<String> {
        let url = reqwest::Url::parse(&self.base_url).ok()?;
        if !matches!(url.scheme(), "http" | "https") {
            return None;
        }
        let host = url.host_str()?;
        let port = url.port_or_known_default()?;
        Some(format!("{}:{}", host, port))
    }

    async fn fetch_model_info(&self, model: &str) -> Result<OllamaShowResponse, LLMError> {
        let request = serde_json::json!({
            "name": model
        });

        let response = self
            .client
            .post(format!("{}/api/show", self.base_url))
            .json(&request)
            .send()
            .await
            .map_err(|e| LLMError::Http(e.to_string()))?;

        if !response.status().is_success() {
            return Err(LLMError::Http(format!(
                "failed to get model info: {}",
                response.status()
            )));
        }

        let body_bytes = response
            .bytes()
            .await
            .map_err(|e| LLMError::Http(e.to_string()))?;

        serde_json::from_slice::<OllamaShowResponse>(&body_bytes)
            .map_err(|e| LLMError::Serialization(e.to_string()))
    }

    fn extract_context_length(model_info: &Option<HashMap<String, serde_json::Value>>) -> usize {
        let info = match model_info {
            Some(i) => i,
            None => return 4096,
        };

        info.iter()
            .find(|(k, _)| k.ends_with(".context_length"))
            .and_then(|(_, v)| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(4096)
    }

    fn detect_capabilities(
        model_name: &str,
        response: &OllamaShowResponse,
        context_length: usize,
    ) -> ProviderCapabilities {
        let capabilities_raw = response.capabilities.as_deref().unwrap_or(&[]);
        let template = response.template.as_deref().unwrap_or("");
        let model_info = response.model_info.as_ref();
        let details = response.details.as_ref();

        let has_tools_in_capabilities = capabilities_raw.contains(&"tools".to_string());
        let supports_tools = has_tools_in_capabilities || template.contains(".Tools");

        let has_insert_in_capabilities = capabilities_raw.contains(&"insert".to_string());
        let supports_fim = has_insert_in_capabilities && template.contains("fim_prefix");

        let supports_embedding = capabilities_raw.contains(&"embedding".to_string());

        let mut supports_vision = capabilities_raw.contains(&"vision".to_string());
        let mut supports_audio = capabilities_raw.contains(&"audio".to_string());

        if let Some(info) = model_info {
            for key in info.keys() {
                let k = key.to_ascii_lowercase();
                if k.contains("vision") || k.contains("mm.") {
                    supports_vision = true;
                }
                if k.contains("audio") || k.contains("voice") {
                    supports_audio = true;
                }
            }
        }

        let model_name_l = model_name.to_ascii_lowercase();
        let family = details
            .and_then(|d| d.family.as_ref())
            .map(|f| f.to_lowercase())
            .unwrap_or_default();

        let is_thinking = capabilities_raw.contains(&"thinking".to_string())
            || family.contains("thinking")
            || family.contains("r1")
            || family.contains("qwq")
            || model_name_l.contains("thinking")
            || model_name_l.contains("r1")
            || model_name_l.contains("qwq");

        let mut parameter_count = model_info.and_then(extract_parameter_count);
        let mut quantization_level = model_info.and_then(extract_quantization_level);

        if let Some(d) = details {
            if parameter_count.is_none() {
                if let Some(p_size) = &d.parameter_size {
                    parameter_count = Some((parse_parameter_size(p_size) * 1_000_000_000.0) as u64);
                }
            }
            if quantization_level.is_none() {
                quantization_level = d.quantization_level.clone();
            }
        }

        let context_length_u32 = context_length.min(u32::MAX as usize) as u32;
        let embedding_length = model_info.and_then(extract_embedding_length);

        let mut final_tags = capabilities_raw.to_vec();
        if supports_audio && !final_tags.contains(&"audio".to_string()) {
            final_tags.push("audio".to_string());
        }

        ProviderCapabilities {
            chat: true,
            tools: supports_tools,
            fim: supports_fim,
            supports_vision,
            supports_thinking: is_thinking,
            context_length: context_length_u32,
            vision: supports_vision,
            supports_embedding,
            max_context_tokens: context_length,
            parameter_count,
            quantization_level,
            embedding_length,
            capability_tags: final_tags,
        }
    }

    async fn get_cached_or_fetch(&self, model: &str) -> Result<ProviderCapabilities, LLMError> {
        let cache = self.model_cache.read().await;
        if let Some(cached) = cache.get(model) {
            if cached.cached_at.elapsed() < Duration::from_secs(Self::CACHE_TTL_SECS) {
                return Ok(cached.capabilities.clone());
            }
        }
        drop(cache);

        let response = self.fetch_model_info(model).await?;
        let context_length = Self::extract_context_length(&response.model_info);
        let capabilities = Self::detect_capabilities(model, &response, context_length);

        let mut cache = self.model_cache.write().await;
        cache.insert(
            model.to_string(),
            CachedModelInfo {
                capabilities: capabilities.clone(),
                cached_at: Instant::now(),
            },
        );

        Ok(capabilities)
    }

    pub async fn invalidate_cache(&self, model: Option<&str>) {
        let mut cache = self.model_cache.write().await;
        match model {
            Some(m) => {
                cache.remove(m);
            }
            None => {
                cache.clear();
            }
        }
    }

    pub async fn get_model_info_full(&self, model: &str) -> Result<OllamaShowResponse, LLMError> {
        let model_name = if model.is_empty() {
            self.default_model.clone()
        } else {
            model.to_string()
        };
        self.fetch_model_info(&model_name).await
    }

    async fn post_chat_with_retry<T: Serialize>(
        &self,
        body: &T,
    ) -> Result<reqwest::Response, LLMError> {
        let mut backoff = Duration::from_millis(200);
        let mut last_error: Option<LLMError> = None;

        for attempt in 0..Self::RETRY_ATTEMPTS {
            let result = self
                .client
                .post(format!("{}/api/chat", self.base_url))
                .json(body)
                .send()
                .await;

            match result {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    let detail = if body.trim().is_empty() {
                        status.to_string()
                    } else {
                        format!("{}: {}", status, body)
                    };

                    if !is_transient_status(status) || attempt + 1 == Self::RETRY_ATTEMPTS {
                        return Err(LLMError::Http(detail));
                    }

                    last_error = Some(LLMError::Http(detail));
                }
                Err(err) => {
                    let error_str = err.to_string();
                    if error_str.contains("decoding response body")
                        || error_str.contains("connection closed")
                    {
                        if attempt + 1 == Self::RETRY_ATTEMPTS {
                            return Err(LLMError::Http(format!(
                                "server connection failed: the stream was interrupted. This may be due to server overload, timeout, or network issues. Original error: {}",
                                error_str
                            )));
                        }
                        last_error = Some(LLMError::Http(format!(
                            "stream interrupted (attempt {}/{}): {}",
                            attempt + 1,
                            Self::RETRY_ATTEMPTS,
                            error_str
                        )));
                    } else if err.is_timeout() {
                        if attempt + 1 == Self::RETRY_ATTEMPTS {
                            return Err(LLMError::Timeout);
                        }
                        last_error = Some(LLMError::Timeout);
                    } else if !is_transient_transport_error(&err)
                        || attempt + 1 == Self::RETRY_ATTEMPTS
                    {
                        return Err(LLMError::Http(err.to_string()));
                    } else {
                        last_error = Some(LLMError::Http(err.to_string()));
                    }
                }
            }

            let jitter = Duration::from_millis(jitter_millis(60));
            sleep(backoff + jitter).await;
            backoff = (backoff * 2).min(Duration::from_secs(2));
        }

        Err(last_error
            .unwrap_or_else(|| LLMError::Unexpected("request failed after retries".to_string())))
    }
}

fn jitter_millis(max: u64) -> u64 {
    if max == 0 {
        return 0;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d: std::time::Duration| d.subsec_nanos() as u64)
        .unwrap_or(0);
    nanos % (max + 1)
}

fn extract_parameter_count(model_info: &HashMap<String, serde_json::Value>) -> Option<u64> {
    model_info
        .iter()
        .find(|(k, _)| k.ends_with(".parameter_count"))
        .and_then(|(_, v)| v.as_u64())
}

fn extract_quantization_level(model_info: &HashMap<String, serde_json::Value>) -> Option<String> {
    model_info
        .iter()
        .find(|(k, _)| k.ends_with(".quantization_level"))
        .and_then(|(_, v)| v.as_str())
        .map(|v| v.to_string())
}

fn extract_embedding_length(model_info: &HashMap<String, serde_json::Value>) -> Option<usize> {
    model_info
        .iter()
        .find(|(k, _)| k.ends_with(".embedding_length"))
        .and_then(|(_, v)| v.as_u64())
        .map(|v| v as usize)
}

fn is_transient_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn is_transient_transport_error(err: &reqwest::Error) -> bool {
    err.is_connect() || err.is_timeout() || err.is_request()
}

#[derive(Serialize, Clone)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    num_ctx: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    stream: bool,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Serialize, Deserialize, Clone)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(default)]
    thinking: Option<String>,
}

#[derive(Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
    #[serde(default)]
    prompt_eval_count: Option<usize>,
    #[serde(default)]
    eval_count: Option<usize>,
}

#[derive(Deserialize)]
struct OllamaStreamChunk {
    done: bool,
    message: Option<OllamaMessage>,
    #[serde(default)]
    prompt_eval_count: Option<usize>,
    #[serde(default)]
    eval_count: Option<usize>,
}

#[derive(Deserialize, Debug, Clone)]
struct OllamaModelDetails {
    pub parent_model: Option<String>,
    pub format: Option<String>,
    pub family: Option<String>,
    pub families: Option<Vec<String>>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct OllamaShowResponse {
    #[serde(default)]
    template: Option<String>,
    #[serde(default)]
    capabilities: Option<Vec<String>>,
    #[serde(default)]
    details: Option<OllamaModelDetails>,
    #[serde(default)]
    model_info: Option<HashMap<String, serde_json::Value>>,
}

fn parse_parameter_size(size_str: &str) -> f32 {
    let lower = size_str.to_ascii_lowercase();
    let num_part = lower
        .chars()
        .take_while(|c| c.is_numeric() || *c == '.')
        .collect::<String>();
    let val = num_part.parse::<f32>().unwrap_or(0.0);

    if lower.contains('b') {
        val
    } else if lower.contains('m') {
        val / 1000.0
    } else {
        val
    }
}

#[async_trait]
impl LLMProvider for OllamaProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Ollama
    }

    fn name(&self) -> &str {
        "Ollama"
    }

    fn is_available(&self) -> bool {
        let Some(addr) = self.healthcheck_socket_addr() else {
            return false;
        };

        let mut iter = match addr.to_socket_addrs() {
            Ok(value) => value,
            Err(_) => return false,
        };
        let Some(socket_addr) = iter.next() else {
            return false;
        };

        TcpStream::connect_timeout(&socket_addr, Duration::from_millis(700)).is_ok()
    }

    async fn send(&self, prompt: String, options: PromptOptions) -> Result<LLMResponse, LLMError> {
        let model = self.model_for(&options);
        let ollama_options = OllamaOptions {
            num_ctx: options.num_ctx,
            temperature: options.temperature,
        };

        let messages = if let Some(chat_messages) = options.messages {
            chat_messages
                .into_iter()
                .map(|m| OllamaMessage {
                    role: m.role,
                    content: m.content,
                    thinking: None,
                })
                .collect()
        } else {
            let mut msgs = Vec::new();
            if let Some(sys) = options.system_prompt.as_ref() {
                msgs.push(OllamaMessage {
                    role: "system".to_string(),
                    content: sys.clone(),
                    thinking: None,
                });
            }
            msgs.push(OllamaMessage {
                role: "user".to_string(),
                content: prompt,
                thinking: None,
            });
            msgs
        };

        let body = OllamaRequest {
            model: model.clone(),
            stream: false,
            messages,
            options: Some(ollama_options),
        };

        let response = self.post_chat_with_retry(&body).await?;

        let parsed = response
            .json::<OllamaResponse>()
            .await
            .map_err(|e| LLMError::Serialization(e.to_string()))?;

        let usage = if parsed.prompt_eval_count.is_some() || parsed.eval_count.is_some() {
            Some(TokenUsage::exact(
                parsed.prompt_eval_count.unwrap_or(0),
                parsed.eval_count.unwrap_or(0),
            ))
        } else {
            None
        };

        Ok(LLMResponse {
            text: parsed.message.content,
            provider: ProviderId::Ollama,
            model: Some(model),
            usage,
        })
    }

    async fn stream_send(
        &self,
        prompt: String,
        options: PromptOptions,
    ) -> Result<TokenStream, LLMError> {
        let model = self.model_for(&options);
        let ollama_options = OllamaOptions {
            num_ctx: options.num_ctx,
            temperature: options.temperature,
        };

        let messages = if let Some(chat_messages) = options.messages {
            chat_messages
        } else {
            let mut msgs = Vec::new();
            if let Some(sys) = options.system_prompt.as_ref() {
                msgs.push(ChatMessage {
                    role: "system".to_string(),
                    content: sys.clone(),
                    timestamp: 0,
                });
            }
            msgs.push(ChatMessage {
                role: "user".to_string(),
                content: prompt,
                timestamp: 0,
            });
            msgs
        };

        let ollama_messages: Vec<OllamaMessage> = messages
            .into_iter()
            .map(|m| OllamaMessage {
                role: m.role,
                content: m.content,
                thinking: None,
            })
            .collect();

        let stream_retries = Self::stream_retries();
        let stream_timeout_secs = Self::stream_idle_timeout_secs();
        const MAX_PENDING_STREAM_BYTES: usize = 512 * 1024;

        for attempt in 0..=stream_retries {
            if attempt > 0 {
                let backoff_ms = 500u64 * 2u64.pow((attempt - 1) as u32);
                let backoff = Duration::from_millis(backoff_ms);
                sleep(backoff).await;
            }

            let body = OllamaRequest {
                model: model.clone(),
                stream: true,
                messages: ollama_messages.clone(),
                options: Some(ollama_options.clone()),
            };

            let response = match self.post_chat_with_retry(&body).await {
                Ok(r) => r,
                Err(e) => {
                    if attempt == stream_retries {
                        return Err(e);
                    }
                    let err_str = e.to_string();
                    if err_str.contains("connection")
                        || err_str.contains("timeout")
                        || err_str.contains("closed")
                    {
                        continue;
                    }
                    return Err(e);
                }
            };

            let byte_stream = response.bytes_stream();
            let attempt_num = attempt;

            let stream = tokio_stream::wrappers::ReceiverStream::new({
                let (tx, rx) = tokio::sync::mpsc::channel(64);
                tokio::spawn(async move {
                    let _ = tx.send(Ok(TokenEvent::Started)).await;
                    tokio::pin!(byte_stream);
                    let mut pending = Vec::<u8>::new();
                    let mut completed_sent = false;
                    let mut thinking_open = false;
                    let mut last_data_time = Instant::now();
                    let stream_timeout_secs = stream_timeout_secs;
                    let stream_retries = stream_retries;

                    loop {
                        tokio::select! {
                            _ = sleep(Duration::from_secs(stream_timeout_secs)) => {
                                let idle_secs = last_data_time.elapsed().as_secs();
                                if idle_secs >= stream_timeout_secs {
                                    let _ = tx.send(Err(LLMError::Timeout)).await;
                                    break;
                                }
                            }
                            item = byte_stream.next() => {
                                match item {
                                    Some(Ok(bytes)) => {
                                        last_data_time = Instant::now();

                                        if pending.len().saturating_add(bytes.len()) > MAX_PENDING_STREAM_BYTES {
                                            let _ = tx.send(Err(LLMError::Unexpected("stream buffer exceeded maximum size".to_string()))).await;
                                            return;
                                        }
                                        pending.extend_from_slice(&bytes);

                                        while let Some(newline_pos) = pending.iter().position(|b| *b == b'\n') {
                                            let line_bytes: Vec<u8> = pending.drain(..=newline_pos).collect();
                                            let line = String::from_utf8_lossy(&line_bytes);
                                            let line = line.trim();
                                            if line.is_empty() {
                                                continue;
                                            }

                                            let parsed = serde_json::from_str::<OllamaStreamChunk>(line)
                                                .map_err(|e| LLMError::Serialization(e.to_string()));

                                            match parsed {
                                                Ok(chunk) => {
                                                    if let Some(message) = chunk.message {
                                                        if let Some(thinking) = message.thinking {
                                                            if !thinking.is_empty() {
                                                                if !thinking_open {
                                                                    thinking_open = true;
                                                                    let _ = tx.send(Ok(TokenEvent::Token("<think>".to_string()))).await;
                                                                }
                                                                let _ = tx.send(Ok(TokenEvent::Token(thinking))).await;
                                                            }
                                                        }

                                                        if !message.content.is_empty() {
                                                            if thinking_open {
                                                                thinking_open = false;
                                                                let _ = tx.send(Ok(TokenEvent::Token("</think>".to_string()))).await;
                                                            }
                                                            let _ = tx.send(Ok(TokenEvent::Token(message.content))).await;
                                                        }
                                                    }
                                                    if chunk.done && !completed_sent {
                                                        if thinking_open {
                                                            thinking_open = false;
                                                            let _ = tx.send(Ok(TokenEvent::Token("</think>".to_string()))).await;
                                                        }
                                                        completed_sent = true;
                                                        let prompt_tokens = chunk.prompt_eval_count.unwrap_or(0);
                                                        let completion_tokens = chunk.eval_count.unwrap_or(0);
                                                        if prompt_tokens > 0 || completion_tokens > 0 {
                                                            let usage = TokenUsage::exact(prompt_tokens, completion_tokens);
                                                            let _ = tx.send(Ok(TokenEvent::Usage(usage))).await;
                                                        }
                                                        let _ = tx.send(Ok(TokenEvent::Completed)).await;
                                                    }
                                                }
                                                Err(err) => {
                                                    let _ = tx.send(Err(err)).await;
                                                    return;
                                                }
                                            }
                                        }

                                        if pending.len() > MAX_PENDING_STREAM_BYTES {
                                            let _ = tx.send(Err(LLMError::Unexpected("stream line exceeded maximum size".to_string()))).await;
                                            return;
                                        }
                                    }
                                    Some(Err(err)) => {
                                        let error_msg = err.to_string();
                                        let is_retryable = error_msg.contains("decoding response body")
                                            || error_msg.contains("connection closed")
                                            || error_msg.contains("reset");

                                        let enhanced_error = if is_retryable {
                                            if attempt_num < stream_retries {
                                                LLMError::Http(format!(
                                                    "stream interrupted (attempt {}/{}): server closed connection. You can retry to continue the generation.",
                                                    attempt_num + 1,
                                                    stream_retries + 1
                                                ))
                                            } else {
                                                LLMError::Http(format!(
                                                    "stream interrupted: server closed connection unexpectedly. Details: {}",
                                                    error_msg
                                                ))
                                            }
                                        } else {
                                            LLMError::Http(error_msg)
                                        };

                                        let _ = tx.send(Err(enhanced_error)).await;
                                        return;
                                    }
                                    None => {
                                        if !pending.is_empty() {
                                            let line = String::from_utf8_lossy(&pending);
                                            let line = line.trim();
                                            if !line.is_empty() {
                                                match serde_json::from_str::<OllamaStreamChunk>(line)
                                                    .map_err(|e| LLMError::Serialization(e.to_string()))
                                                {
                                                    Ok(chunk) => {
                                                        if let Some(message) = chunk.message {
                                                            if let Some(thinking) = message.thinking {
                                                                if !thinking.is_empty() {
                                                                    if !thinking_open {
                                                                        thinking_open = true;
                                                                        let _ = tx.send(Ok(TokenEvent::Token("<think>".to_string()))).await;
                                                                    }
                                                                    let _ = tx.send(Ok(TokenEvent::Token(thinking))).await;
                                                                }
                                                            }

                                                            if !message.content.is_empty() {
                                                                if thinking_open {
                                                                    thinking_open = false;
                                                                    let _ = tx.send(Ok(TokenEvent::Token("</think>".to_string()))).await;
                                                                }
                                                                let _ = tx.send(Ok(TokenEvent::Token(message.content))).await;
                                                            }
                                                        }
                                                        if chunk.done && !completed_sent {
                                                            if thinking_open {
                                                                thinking_open = false;
                                                                let _ = tx.send(Ok(TokenEvent::Token("</think>".to_string()))).await;
                                                            }
                                                            let prompt_tokens = chunk.prompt_eval_count.unwrap_or(0);
                                                            let completion_tokens = chunk.eval_count.unwrap_or(0);
                                                            if prompt_tokens > 0 || completion_tokens > 0 {
                                                                let usage = TokenUsage::exact(prompt_tokens, completion_tokens);
                                                                let _ = tx.send(Ok(TokenEvent::Usage(usage))).await;
                                                            }
                                                            let _ = tx.send(Ok(TokenEvent::Completed)).await;
                                                        }
                                                    }
                                                    Err(err) => {
                                                        let _ = tx.send(Err(err)).await;
                                                    }
                                                }
                                            }
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                    }
                });
                rx
            });

            return Ok(Box::pin(stream));
        }

        Err(LLMError::Http(
            "stream failed after all retries".to_string(),
        ))
    }

    async fn get_model_info(&self, model: &str) -> Result<ProviderCapabilities, LLMError> {
        let model_name = if model.is_empty() {
            self.default_model.clone()
        } else {
            model.to_string()
        };
        self.get_cached_or_fetch(&model_name).await
    }

    async fn health_check(&self) -> Result<bool, LLMError> {
        let url = format!("{}/api/tags", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}
