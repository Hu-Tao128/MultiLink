use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tokio::time::sleep;

use crate::providers::{
    LLMError, LLMProvider, LLMResponse, PromptOptions, ProviderCapabilities, ProviderId,
    ProviderInfo, TokenStream,
};

const DEFAULT_MAX_RETRIES: u32 = 3;
const DEFAULT_INITIAL_BACKOFF_MS: u64 = 500;
const DEFAULT_MAX_BACKOFF_MS: u64 = 5000;

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
    model_server_map: Arc<RwLock<HashMap<String, String>>>,
    health_states: Arc<RwLock<HashMap<ProviderId, ProviderHealthState>>>,
    circuit_breaker_config: CircuitBreakerConfig,
    health_check_interval: Duration,
    shutdown_tx: Arc<RwLock<Option<tokio::sync::oneshot::Sender<()>>>>,
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
            model_server_map: Arc::new(RwLock::new(HashMap::new())),
            health_states: Arc::new(RwLock::new(HashMap::new())),
            circuit_breaker_config: CircuitBreakerConfig::default(),
            health_check_interval: Duration::from_secs(30),
            shutdown_tx: Arc::new(RwLock::new(None)),
        }
    }

    pub fn with_config(config: CircuitBreakerConfig) -> Self {
        Self {
            providers: HashMap::new(),
            order: vec![ProviderId::Ollama, ProviderId::Gemini, ProviderId::Codex],
            model_server_map: Arc::new(RwLock::new(HashMap::new())),
            health_states: Arc::new(RwLock::new(HashMap::new())),
            circuit_breaker_config: config,
            health_check_interval: Duration::from_secs(30),
            shutdown_tx: Arc::new(RwLock::new(None)),
        }
    }

    pub fn with_health_check_interval(mut self, interval: Duration) -> Self {
        self.health_check_interval = interval;
        self
    }

    pub fn register(&mut self, provider: Arc<dyn LLMProvider>) {
        let id = provider.id();
        self.providers.insert(id, provider);
    }

    pub fn get_provider(&self, id: ProviderId) -> Option<&Arc<dyn LLMProvider>> {
        self.providers.get(&id)
    }

    pub async fn register_model_server(
        &self,
        model: impl Into<String>,
        base_url: impl Into<String>,
    ) {
        let model = model.into();
        let base_url = normalize_base_url(&base_url.into());
        if model.trim().is_empty() || base_url.is_empty() {
            return;
        }
        self.model_server_map.write().await.insert(model, base_url);
    }

    pub async fn resolve_model_server(&self, model: &str) -> Option<String> {
        self.model_server_map.read().await.get(model).cloned()
    }

    async fn resolved_ollama_provider(
        &self,
        model: &str,
        explicit_base_url: Option<&str>,
    ) -> Option<Arc<dyn LLMProvider>> {
        let resolved_base_url = explicit_base_url
            .map(normalize_base_url)
            .filter(|value| !value.is_empty());
        let resolved_base_url = match resolved_base_url {
            Some(base_url) => Some(base_url),
            None => self.resolve_model_server(model).await,
        };

        match resolved_base_url {
            Some(base_url) => Some(Arc::new(crate::providers::ollama::OllamaProvider::new(
                base_url,
                model.to_string(),
            ))),
            None => self.providers.get(&ProviderId::Ollama).cloned(),
        }
    }

    pub async fn init_health_states(&self) {
        let mut states = self.health_states.write().await;
        for id in &self.order {
            states.entry(*id).or_default();
        }
    }

    pub async fn warmup_model(&self, provider_id: ProviderId, model: &str) {
        if provider_id == ProviderId::Ollama {
            if let Some(provider) = self.resolved_ollama_provider(model, None).await {
                eprintln!(
                    "[warmup] attempting to warmup model '{}' on provider {:?}",
                    model, provider_id
                );
                let warmup_options = PromptOptions {
                    model: Some(model.to_string()),
                    model_server_url: self.resolve_model_server(model).await,
                    ..Default::default()
                };

                let warmup_result = tokio::time::timeout(
                    Duration::from_secs(30),
                    provider.send("Hi".to_string(), warmup_options),
                )
                .await;

                match warmup_result {
                    Ok(Ok(_)) => eprintln!("[warmup] successfully warmed up model '{}'", model),
                    Ok(Err(e)) => eprintln!("[warmup] warmup failed for model '{}': {}", model, e),
                    Err(_) => eprintln!(
                        "[warmup] warmup timed out for model '{}' (model may still be loading)",
                        model
                    ),
                }
                return;
            }
        }

        if let Some(provider) = self.providers.get(&provider_id) {
            eprintln!(
                "[warmup] attempting to warmup model '{}' on provider {:?}",
                model, provider_id
            );
            // Try warmup using a simple minimal prompt to trigger model loading
            // This works for any provider type without needing downcasting
            let warmup_options = PromptOptions {
                model: Some(model.to_string()),
                ..Default::default()
            };

            // Use a longer timeout for warmup (30s) as large models take time to load
            let warmup_result = tokio::time::timeout(
                Duration::from_secs(30),
                provider.send("Hi".to_string(), warmup_options),
            )
            .await;

            match warmup_result {
                Ok(Ok(_)) => eprintln!("[warmup] successfully warmed up model '{}'", model),
                Ok(Err(e)) => eprintln!("[warmup] warmup failed for model '{}': {}", model, e),
                Err(_) => eprintln!(
                    "[warmup] warmup timed out for model '{}' (model may still be loading)",
                    model
                ),
            }
        } else {
            eprintln!("[warmup] no provider found for {:?}", provider_id);
        }
    }

    pub async fn health_state(&self, provider_id: ProviderId) -> Option<ProviderHealthState> {
        let states = self.health_states.read().await;
        states.get(&provider_id).cloned()
    }

    pub async fn record_success(&self, provider_id: ProviderId) {
        let mut states = self.health_states.write().await;
        if let Some(state) = states.get_mut(&provider_id) {
            state.failure_count = 0;
            state.consecutive_successes += 1;
            state.cooldown_until = None;
            state.is_healthy = true;
        }
    }

    pub async fn record_failure(&self, provider_id: ProviderId) {
        let mut states = self.health_states.write().await;
        if let Some(state) = states.get_mut(&provider_id) {
            state.failure_count += 1;
            state.last_failure_time = Some(Instant::now());
            state.consecutive_successes = 0;

            if state.failure_count >= self.circuit_breaker_config.failure_threshold {
                state.is_healthy = false;
                state.cooldown_until =
                    Some(Instant::now() + self.circuit_breaker_config.recovery_timeout);
            }
        }
    }

    pub async fn check_circuit_breaker(&self, provider_id: ProviderId) -> bool {
        let states = self.health_states.read().await;
        if let Some(state) = states.get(&provider_id) {
            if !state.is_healthy {
                if let Some(cooldown_end) = state.cooldown_until {
                    if Instant::now() >= cooldown_end {
                        return true;
                    }
                }
                return false;
            }
        }
        true
    }

    pub async fn start_health_monitor(&self) {
        let providers = self.providers.clone();
        let health_states = self.health_states.clone();
        let interval = self.health_check_interval;
        let config = self.circuit_breaker_config.clone();
        let shutdown_tx = self.shutdown_tx.clone();

        let (tx, rx) = tokio::sync::oneshot::channel();
        *shutdown_tx.write().await = Some(tx);

        tokio::spawn(async move {
            let mut rx = rx;
            let mut interval_timer = tokio::time::interval(interval);

            loop {
                tokio::select! {
                    _ = &mut rx => {
                        break;
                    }
                    _ = interval_timer.tick() => {
                        for (provider_id, provider) in &providers {
                            let is_healthy = provider.health_check().await.unwrap_or_default();

                            let mut states = health_states.write().await;
                            if let Some(state) = states.get_mut(provider_id) {
                                if is_healthy {
                                    state.consecutive_successes += 1;
                                    if state.consecutive_successes >= config.success_threshold {
                                        state.is_healthy = true;
                                        state.failure_count = 0;
                                        state.cooldown_until = None;
                                    }
                                } else {
                                    state.failure_count += 1;
                                    state.last_failure_time = Some(Instant::now());
                                    state.consecutive_successes = 0;

                                    if state.failure_count >= config.failure_threshold {
                                        state.is_healthy = false;
                                        state.cooldown_until = Some(Instant::now() + config.recovery_timeout);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    pub async fn stop_health_monitor(&self) {
        let mut tx = self.shutdown_tx.write().await;
        if let Some(sender) = tx.take() {
            let _ = sender.send(());
        }
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

    pub async fn get_available_providers(&self) -> Vec<ProviderInfo> {
        let mut providers = Vec::new();
        for id in &self.order {
            if let Some(provider) = self.providers.get(id) {
                let is_available = provider.is_available();
                let capabilities = if is_available {
                    provider.capabilities().await.unwrap_or_default()
                } else {
                    ProviderCapabilities::default()
                };
                providers.push(ProviderInfo {
                    id: *id,
                    capabilities,
                    is_available,
                });
            }
        }
        providers
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
        if preferred == ProviderId::Ollama {
            if let Some(model) = options.model.as_deref() {
                if let Some(provider) = self
                    .resolved_ollama_provider(model, options.model_server_url.as_deref())
                    .await
                {
                    return provider.send(prompt, options).await;
                }
            }
        }

        if let Some(provider) = self.providers.get(&preferred) {
            if provider.is_available_async().await && self.check_circuit_breaker(preferred).await {
                match provider.send(prompt.clone(), options.clone()).await {
                    Ok(response) => {
                        self.record_success(preferred).await;
                        return Ok(response);
                    }
                    Err(e) => {
                        self.record_failure(preferred).await;
                        if !is_retryable_error(&e) {
                            return Err(e);
                        }
                    }
                }
            }
        }

        for provider_id in &self.order {
            if *provider_id == preferred {
                continue;
            }

            if let Some(provider) = self.providers.get(provider_id) {
                if provider.is_available_async().await && self.check_circuit_breaker(*provider_id).await {
                    match self
                        .send_with_retry(provider, prompt.clone(), options.clone())
                        .await
                    {
                        Ok(response) => {
                            self.record_success(*provider_id).await;
                            return Ok(response);
                        }
                        Err(_e) => {
                            self.record_failure(*provider_id).await;
                            continue;
                        }
                    }
                }
            }
        }

        Err(LLMError::Unavailable)
    }

    async fn send_with_retry(
        &self,
        provider: &Arc<dyn LLMProvider>,
        prompt: String,
        options: PromptOptions,
    ) -> Result<LLMResponse, LLMError> {
        let max_retries = DEFAULT_MAX_RETRIES;
        let mut last_error = LLMError::Unavailable;

        for attempt in 0..=max_retries {
            if attempt > 0 {
                let backoff_ms =
                    calculate_backoff(attempt, DEFAULT_INITIAL_BACKOFF_MS, DEFAULT_MAX_BACKOFF_MS);
                sleep(Duration::from_millis(backoff_ms)).await;
            }

            match provider.send(prompt.clone(), options.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    last_error = e.clone();
                    if !is_retryable_error(&e) {
                        return Err(e);
                    }
                }
            }
        }

        Err(last_error)
    }

    pub async fn stream_send(
        &self,
        preferred: ProviderId,
        prompt: String,
        options: PromptOptions,
    ) -> Result<TokenStream, LLMError> {
        if preferred == ProviderId::Ollama {
            if let Some(model) = options.model.as_deref() {
                if let Some(provider) = self
                    .resolved_ollama_provider(model, options.model_server_url.as_deref())
                    .await
                {
                    return provider.stream_send(prompt, options).await;
                }
            }
        }

        if let Some(provider) = self.providers.get(&preferred) {
            if provider.is_available_async().await && self.check_circuit_breaker(preferred).await {
                return provider.stream_send(prompt.clone(), options.clone()).await;
            }
        }

        for provider_id in &self.order {
            if *provider_id == preferred {
                continue;
            }

            if let Some(provider) = self.providers.get(provider_id) {
                if provider.is_available_async().await && self.check_circuit_breaker(*provider_id).await {
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
        if preferred == ProviderId::Ollama {
            if let Some(provider) = self.resolved_ollama_provider(model, None).await {
                return provider.get_model_info(model).await;
            }
        }

        if let Some(provider) = self.providers.get(&preferred) {
            if provider.is_available_async().await {
                return provider.get_model_info(model).await;
            }
        }

        for provider_id in &self.order {
            if *provider_id == preferred {
                continue;
            }
            if let Some(provider) = self.providers.get(provider_id) {
                if provider.is_available_async().await {
                    return provider.get_model_info(model).await;
                }
            }
        }

        Err(LLMError::Unavailable)
    }
}

fn is_retryable_error(error: &LLMError) -> bool {
    matches!(
        error,
        LLMError::Timeout | LLMError::RateLimited | LLMError::Http(_)
    )
}

fn calculate_backoff(attempt: u32, initial_ms: u64, max_ms: u64) -> u64 {
    let backoff = initial_ms * 2u64.pow(attempt.saturating_sub(1));
    backoff.min(max_ms)
}

fn normalize_base_url(input: &str) -> String {
    let trimmed = input.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return trimmed.to_string();
    }
    format!("http://{}", trimmed)
}
