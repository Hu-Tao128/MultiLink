use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::{LLMError, LLMProvider, LLMResponse, PromptOptions, ProviderId, TokenEvent, TokenStream};

#[derive(Clone)]
pub struct OllamaProvider {
    client: Client,
    base_url: String,
    default_model: String,
}

impl OllamaProvider {
    pub fn new(base_url: String, default_model: String) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(180))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            client,
            base_url,
            default_model,
        }
    }

    fn model_for(&self, options: &PromptOptions) -> String {
        options
            .model
            .clone()
            .unwrap_or_else(|| self.default_model.clone())
    }
}

#[derive(Serialize)]
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

#[derive(Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
}

#[derive(Deserialize)]
struct OllamaStreamChunk {
    done: bool,
    message: Option<OllamaMessage>,
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
        true
    }

    async fn send(&self, prompt: String, options: PromptOptions) -> Result<LLMResponse, LLMError> {
        let model = self.model_for(&options);
        let ollama_options = OllamaOptions {
            num_ctx: options.num_ctx,
            temperature: options.temperature,
        };
        
        let mut messages = Vec::new();
        if let Some(sys) = options.system_prompt.as_ref() {
            messages.push(OllamaMessage {
                role: "system".to_string(),
                content: sys.clone(),
            });
        }
        messages.push(OllamaMessage {
            role: "user".to_string(),
            content: prompt,
        });

        let body = OllamaRequest {
            model: model.clone(),
            stream: false,
            messages,
            options: Some(ollama_options),
        };

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| LLMError::Http(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let detail = if body.trim().is_empty() {
                status.to_string()
            } else {
                format!("{}: {}", status, body)
            };
            return Err(LLMError::Http(detail));
        }

        let parsed = response
            .json::<OllamaResponse>()
            .await
            .map_err(|e| LLMError::Serialization(e.to_string()))?;

        Ok(LLMResponse {
            text: parsed.message.content,
            provider: ProviderId::Ollama,
            model: Some(model),
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
        
        let mut messages = Vec::new();
        if let Some(sys) = options.system_prompt.as_ref() {
            messages.push(OllamaMessage {
                role: "system".to_string(),
                content: sys.clone(),
            });
        }
        messages.push(OllamaMessage {
            role: "user".to_string(),
            content: prompt,
        });

        let body = OllamaRequest {
            model,
            stream: true,
            messages,
            options: Some(ollama_options),
        };

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| LLMError::Http(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let detail = if body.trim().is_empty() {
                status.to_string()
            } else {
                format!("{}: {}", status, body)
            };
            return Err(LLMError::Http(detail));
        }

        let byte_stream = response.bytes_stream();
        let stream = tokio_stream::wrappers::ReceiverStream::new({
            let (tx, rx) = tokio::sync::mpsc::channel(64);
            tokio::spawn(async move {
                let _ = tx.send(Ok(TokenEvent::Started)).await;
                tokio::pin!(byte_stream);
                let mut pending = Vec::<u8>::new();
                let mut completed_sent = false;

                while let Some(item) = byte_stream.next().await {
                    match item {
                        Ok(bytes) => {
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
                                            let _ = tx.send(Ok(TokenEvent::Token(message.content))).await;
                                        }
                                        if chunk.done && !completed_sent {
                                            completed_sent = true;
                                            let _ = tx.send(Ok(TokenEvent::Completed)).await;
                                        }
                                    }
                                    Err(err) => {
                                        let _ = tx.send(Err(err)).await;
                                        return;
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            let _ = tx.send(Err(LLMError::Http(err.to_string()))).await;
                            return;
                        }
                    }
                }

                if !pending.is_empty() {
                    let line = String::from_utf8_lossy(&pending);
                    let line = line.trim();
                    if !line.is_empty() {
                        match serde_json::from_str::<OllamaStreamChunk>(line)
                            .map_err(|e| LLMError::Serialization(e.to_string()))
                        {
                            Ok(chunk) => {
                                if let Some(message) = chunk.message {
                                    let _ = tx.send(Ok(TokenEvent::Token(message.content))).await;
                                }
                                if chunk.done && !completed_sent {
                                    let _ = tx.send(Ok(TokenEvent::Completed)).await;
                                }
                            }
                            Err(err) => {
                                let _ = tx.send(Err(err)).await;
                            }
                        }
                    }
                }
            });
            rx
        });

        Ok(Box::pin(stream))
    }
}
