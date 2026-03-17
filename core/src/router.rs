use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::providers::{
    LLMError, LLMProvider, LLMResponse, PromptOptions, ProviderCapabilities, ProviderId,
    TokenStream,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAvailability {
    Available,
    NotAvailable,
}

#[derive(Debug, Clone)]
pub struct ProviderHealthState {
    pub failure_count: u32,
    pub last_failure_time: Option<Instant>,
    pub cooldown_until: Option<Instant>,
    pub is_healthy: bool,
    pub consecutive_successes: u32,
}

impl Default for ProviderHealthState {
    fn default() -> Self {
        Self {
            failure_count: 0,
            last_failure_time: None,
            cooldown_until: None,
            is_healthy: true,
            consecutive_successes: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub recovery_timeout: Duration,
    pub half_open_max_calls: u32,
    pub success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 3,
            recovery_timeout: Duration::from_secs(30),
            half_open_max_calls: 1,
            success_threshold: 2,
        }
    }
}

pub struct ProviderRouter {
    providers: HashMap<ProviderId, Arc<dyn LLMProvider>>,
    order: Vec<ProviderId>,
    health_states: HashMap<ProviderId, ProviderHealthState>,
    circuit_breaker_config: CircuitBreakerConfig,
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
            health_states: HashMap::new(),
            circuit_breaker_config: CircuitBreakerConfig::default(),
        }
    }

    pub fn with_config(config: CircuitBreakerConfig) -> Self {
        Self {
            providers: HashMap::new(),
            order: vec![ProviderId::Ollama, ProviderId::Gemini, ProviderId::Codex],
            health_states: HashMap::new(),
            circuit_breaker_config: config,
        }
    }

    pub fn register(&mut self, provider: Arc<dyn LLMProvider>) {
        let id = provider.id();
        self.providers.insert(id, provider);
        self.health_states.entry(id).or_default();
    }

    pub fn health_state(&self, provider_id: ProviderId) -> Option<&ProviderHealthState> {
        self.health_states.get(&provider_id)
    }

    pub fn record_success(&mut self, provider_id: ProviderId) {
        if let Some(state) = self.health_states.get_mut(&provider_id) {
            state.failure_count = 0;
            state.consecutive_successes += 1;
            state.cooldown_until = None;
            state.is_healthy = true;
        }
    }

    pub fn record_failure(&mut self, provider_id: ProviderId) {
        if let Some(state) = self.health_states.get_mut(&provider_id) {
            state.failure_count += 1;
            state.last_failure_time = Some(Instant::now());
            state.consecutive_successes = 0;
            
            if state.failure_count >= self.circuit_breaker_config.failure_threshold {
                state.is_healthy = false;
                state.cooldown_until = Some(Instant::now() + self.circuit_breaker_config.recovery_timeout);
            }
        }
    }

    pub fn check_circuit_breaker(&self, provider_id: ProviderId) -> bool {
        if let Some(state) = self.health_states.get(&provider_id) {
            if !state.is_healthy {
                if let Some(cooldown_end) = state.cooldown_until {
                    if Instant::now() >= cooldown_end {
                        return true;
                        // Would transition to half-open state here
                    }
                }
                return false;
            }
        }
        true
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

    pub async fn get_model_info(
        &self,
        preferred: ProviderId,
        model: &str,
    ) -> Result<ProviderCapabilities, LLMError> {
        if let Some(provider) = self.providers.get(&preferred) {
            if provider.is_available() {
                return provider.get_model_info(model).await;
            }
        }

        for provider_id in &self.order {
            if *provider_id == preferred {
                continue;
            }
            if let Some(provider) = self.providers.get(provider_id) {
                if provider.is_available() {
                    return provider.get_model_info(model).await;
                }
            }
        }

        Err(LLMError::Unavailable)
    }
}
