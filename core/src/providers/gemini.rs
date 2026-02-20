use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{LLMError, LLMProvider, LLMResponse, PromptOptions, ProviderId, TokenEvent, TokenStream};

#[derive(Clone)]
pub struct GeminiProvider {
    client: Client,
    endpoint: String,
    access_token: Option<String>,
}

impl GeminiProvider {
    pub fn new(endpoint: String, access_token: Option<String>, timeout_secs: u64) -> Result<Self, LLMError> {
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
        let token = self.access_token.as_deref().ok_or(LLMError::NotConfigured)?;
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(token)
            .json(&GeminiRequest {
                prompt,
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

        Ok(LLMResponse {
            text: payload.text,
            provider: ProviderId::Gemini,
            model: payload.model,
        })
    }

    async fn stream_send(&self, prompt: String, options: PromptOptions) -> Result<TokenStream, LLMError> {
        let full = self.send(prompt, options).await?;
        let stream = tokio_stream::iter(vec![
            Ok(TokenEvent::Started),
            Ok(TokenEvent::Token(full.text)),
            Ok(TokenEvent::Completed),
        ]);
        Ok(Box::pin(stream))
    }
}

fn map_reqwest(error: reqwest::Error) -> LLMError {
    if error.is_timeout() {
        LLMError::Timeout
    } else {
        LLMError::Http(error.to_string())
    }
}
