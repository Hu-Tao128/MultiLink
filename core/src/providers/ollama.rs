use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{LLMError, LLMProvider, LLMResponse, PromptOptions, ProviderId, TokenEvent, TokenStream};

#[derive(Clone)]
pub struct OllamaProvider {
    client: Client,
    base_url: String,
    default_model: String,
}

impl OllamaProvider {
    pub fn new(base_url: String, default_model: String) -> Self {
        Self {
            client: Client::new(),
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
struct OllamaRequest {
    model: String,
    stream: bool,
    messages: Vec<OllamaMessage>,
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
        let body = OllamaRequest {
            model: model.clone(),
            stream: false,
            messages: vec![OllamaMessage {
                role: "user".to_string(),
                content: prompt,
            }],
        };

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| LLMError::Http(e.to_string()))?;

        if !response.status().is_success() {
            return Err(LLMError::Http(response.status().to_string()));
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
        let body = OllamaRequest {
            model,
            stream: true,
            messages: vec![OllamaMessage {
                role: "user".to_string(),
                content: prompt,
            }],
        };

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| LLMError::Http(e.to_string()))?;

        if !response.status().is_success() {
            return Err(LLMError::Http(response.status().to_string()));
        }

        let byte_stream = response.bytes_stream();
        let stream = tokio_stream::wrappers::ReceiverStream::new({
            let (tx, rx) = tokio::sync::mpsc::channel(64);
            tokio::spawn(async move {
                let _ = tx.send(Ok(TokenEvent::Started)).await;
                tokio::pin!(byte_stream);

                while let Some(item) = byte_stream.next().await {
                    match item {
                        Ok(bytes) => {
                            let text = String::from_utf8_lossy(&bytes);
                            for line in text.lines().filter(|line| !line.trim().is_empty()) {
                                let parsed = serde_json::from_str::<OllamaStreamChunk>(line)
                                    .map_err(|e| LLMError::Serialization(e.to_string()));

                                match parsed {
                                    Ok(chunk) => {
                                        if let Some(message) = chunk.message {
                                            let _ = tx.send(Ok(TokenEvent::Token(message.content))).await;
                                        }
                                        if chunk.done {
                                            let _ = tx.send(Ok(TokenEvent::Completed)).await;
                                        }
                                    }
                                    Err(err) => {
                                        let _ = tx.send(Err(err)).await;
                                        break;
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            let _ = tx.send(Err(LLMError::Http(err.to_string()))).await;
                            break;
                        }
                    }
                }
            });
            rx
        });

        Ok(Box::pin(stream))
    }
}
