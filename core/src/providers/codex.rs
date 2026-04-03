use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{
    LLMError, LLMProvider, LLMResponse, PromptOptions, ProviderCapabilities, ProviderId,
    TokenEvent, TokenStream, TokenUsage,
};

#[derive(Clone)]
pub struct CodexProvider {
    client: Client,
    endpoint: String,
    access_token: Option<String>,
}

impl CodexProvider {
    pub fn new(
        endpoint: String,
        access_token: Option<String>,
        timeout_secs: u64,
    ) -> Result<Self, LLMError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .map_err(|e| LLMError::Http(e.to_string()))?;

        Ok(Self {
            client,
            endpoint,
            access_token,
        })
    }

    pub fn with_access_token(mut self, access_token: Option<String>) -> Self {
        self.access_token = access_token;
        self
    }
}

#[derive(Serialize)]
struct CodexRequest {
    input: String,
    model: Option<String>,
}

#[derive(Deserialize)]
struct CodexResponse {
    output_text: String,
    model: Option<String>,
    #[serde(default)]
    usage: Option<CodexUsage>,
}

#[derive(Deserialize)]
struct CodexUsage {
    #[serde(rename = "input_tokens", default)]
    input_tokens: Option<usize>,
    #[serde(rename = "output_tokens", default)]
    output_tokens: Option<usize>,
}

#[async_trait]
impl LLMProvider for CodexProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Codex
    }

    fn name(&self) -> &str {
        "Codex"
    }

    fn is_available(&self) -> bool {
        self.access_token.is_some()
    }

    async fn send(&self, prompt: String, options: PromptOptions) -> Result<LLMResponse, LLMError> {
        let token = self
            .access_token
            .as_deref()
            .ok_or(LLMError::NotConfigured)?;

        let final_input = if let Some(messages) = options.messages {
            let mut content = String::new();
            for msg in messages {
                let role_str = match msg.role.as_str() {
                    "system" => "System",
                    "assistant" => "Assistant",
                    _ => "User",
                };
                content.push_str(&format!("{}: {}\n\n", role_str, msg.content));
            }
            content
        } else {
            let mut p = String::new();
            if let Some(sys) = options.system_prompt {
                p.push_str(&sys);
                p.push_str("\n\n");
            }
            p.push_str(&prompt);
            p
        };

        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(token)
            .json(&CodexRequest {
                input: final_input,
                model: options.model,
            })
            .send()
            .await
            .map_err(map_reqwest)?;

        if response.status().as_u16() == 429 {
            return Err(LLMError::RateLimited);
        }
        if !response.status().is_success() {
            return Err(LLMError::Http(response.status().to_string()));
        }

        let payload = response
            .json::<CodexResponse>()
            .await
            .map_err(|e| LLMError::Serialization(e.to_string()))?;

        let usage = payload
            .usage
            .map(|u| TokenUsage::exact(u.input_tokens.unwrap_or(0), u.output_tokens.unwrap_or(0)));

        Ok(LLMResponse {
            text: payload.output_text,
            provider: ProviderId::Codex,
            model: payload.model,
            usage,
        })
    }

    async fn stream_send(
        &self,
        prompt: String,
        options: PromptOptions,
    ) -> Result<TokenStream, LLMError> {
        let full = self.send(prompt, options).await?;
        let mut events = vec![
            Ok(TokenEvent::Started),
            Ok(TokenEvent::Token(full.text.clone())),
        ];
        if let Some(usage) = full.usage {
            events.push(Ok(TokenEvent::Usage(usage)));
        }
        events.push(Ok(TokenEvent::Completed));
        let stream = tokio_stream::iter(events);
        Ok(Box::pin(stream))
    }

    async fn get_model_info(&self, _model: &str) -> Result<ProviderCapabilities, LLMError> {
        Ok(ProviderCapabilities {
            chat: true,
            tools: false,
            fim: true,
            supports_vision: false,
            supports_thinking: false,
            context_length: 128_000,
            vision: false,
            supports_embedding: false,
            max_context_tokens: 128_000,
            parameter_count: None,
            quantization_level: None,
            embedding_length: None,
            capability_tags: Vec::new(),
        })
    }

    async fn health_check(&self) -> Result<bool, LLMError> {
        if self.access_token.is_none() {
            return Ok(false);
        }
        let token = self.access_token.as_deref().unwrap();
        let test_url = self.endpoint.replace("/completions", "/models");
        match self.client.get(&test_url).bearer_auth(token).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}

fn map_reqwest(error: reqwest::Error) -> LLMError {
    if error.is_timeout() {
        LLMError::Timeout
    } else {
        LLMError::Http(error.to_string())
    }
}
