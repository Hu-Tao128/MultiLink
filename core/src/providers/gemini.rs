use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{
    LLMError, LLMProvider, LLMResponse, PromptOptions, ProviderCapabilities, ProviderId,
    TokenEvent, TokenStream, TokenUsage,
};

#[derive(Clone)]
pub struct GeminiProvider {
    client: Client,
    endpoint: String,
    access_token: Option<String>,
}

impl GeminiProvider {
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
struct GeminiRequest {
    prompt: String,
    model: Option<String>,
}

#[derive(Deserialize)]
struct GeminiResponse {
    text: String,
    model: Option<String>,
    #[serde(default)]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Deserialize)]
struct GeminiUsageMetadata {
    #[serde(rename = "promptTokenCount", default)]
    prompt_token_count: Option<usize>,
    #[serde(rename = "candidatesTokenCount", default)]
    candidates_token_count: Option<usize>,
}

#[async_trait]
impl LLMProvider for GeminiProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Gemini
    }

    fn name(&self) -> &str {
        "Gemini"
    }

    fn is_available(&self) -> bool {
        self.access_token.is_some()
    }

    async fn send(&self, prompt: String, options: PromptOptions) -> Result<LLMResponse, LLMError> {
        let token = self
            .access_token
            .as_deref()
            .ok_or(LLMError::NotConfigured)?;

        let final_prompt = if let Some(messages) = options.messages {
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
            .json(&GeminiRequest {
                prompt: final_prompt,
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
            .json::<GeminiResponse>()
            .await
            .map_err(|e| LLMError::Serialization(e.to_string()))?;

        let usage = payload.usage_metadata.map(|u| {
            TokenUsage::exact(
                u.prompt_token_count.unwrap_or(0),
                u.candidates_token_count.unwrap_or(0),
            )
        });

        Ok(LLMResponse {
            text: payload.text,
            provider: ProviderId::Gemini,
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
        Ok(ProviderCapabilities::default_with_context(128_000))
    }

    async fn health_check(&self) -> Result<bool, LLMError> {
        if self.access_token.is_none() {
            return Ok(false);
        }
        let token = self.access_token.as_deref().unwrap();
        let test_url = format!("{}/models", self.endpoint.replace("/generateContent", ""));
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
