use std::collections::HashMap;
use std::sync::Arc;

use crate::providers::{LLMError, LLMProvider, LLMResponse, PromptOptions, ProviderId, TokenStream};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAvailability {
    Available,
    NotAvailable,
}

pub struct ProviderRouter {
    providers: HashMap<ProviderId, Arc<dyn LLMProvider>>,
    order: Vec<ProviderId>,
}

impl Default for ProviderRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderRouter {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            order: vec![ProviderId::Ollama, ProviderId::Gemini, ProviderId::Codex],
        }
    }

    pub fn register(&mut self, provider: Arc<dyn LLMProvider>) {
        self.providers.insert(provider.id(), provider);
    }

    pub fn available(&self) -> Vec<ProviderId> {
        self.order
            .iter()
            .filter(|id| {
                self.providers
                    .get(id)
                    .map(|provider| provider.is_available())
                    .unwrap_or(false)
            })
            .copied()
            .collect()
    }

    pub fn provider_status(&self) -> Vec<(ProviderId, ProviderAvailability)> {
        self.order
            .iter()
            .map(|id| {
                let availability = self
                    .providers
                    .get(id)
                    .map(|provider| {
                        if provider.is_available() {
                            ProviderAvailability::Available
                        } else {
                            ProviderAvailability::NotAvailable
                        }
                    })
                    .unwrap_or(ProviderAvailability::NotAvailable);
                (*id, availability)
            })
            .collect()
    }

    pub async fn send(
        &self,
        preferred: ProviderId,
        prompt: String,
        options: PromptOptions,
    ) -> Result<LLMResponse, LLMError> {
        if let Some(provider) = self.providers.get(&preferred) {
            if provider.is_available() {
                return provider.send(prompt.clone(), options.clone()).await;
            }
        }

        for provider_id in &self.order {
            if *provider_id == preferred {
                continue;
            }

            if let Some(provider) = self.providers.get(provider_id) {
                if provider.is_available() {
                    return provider.send(prompt.clone(), options.clone()).await;
                }
            }
        }

        Err(LLMError::Unavailable)
    }

    pub async fn stream_send(
        &self,
        preferred: ProviderId,
        prompt: String,
        options: PromptOptions,
    ) -> Result<TokenStream, LLMError> {
        if let Some(provider) = self.providers.get(&preferred) {
            if provider.is_available() {
                return provider.stream_send(prompt.clone(), options.clone()).await;
            }
        }

        for provider_id in &self.order {
            if *provider_id == preferred {
                continue;
            }

            if let Some(provider) = self.providers.get(provider_id) {
                if provider.is_available() {
                    return provider.stream_send(prompt.clone(), options.clone()).await;
                }
            }
        }

        Err(LLMError::Unavailable)
    }
}
