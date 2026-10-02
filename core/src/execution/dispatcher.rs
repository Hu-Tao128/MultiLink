use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{RwLock, Semaphore};
use tokio::time::sleep;

use crate::config::ExecutionServerRuntime;
use crate::providers::ollama::OllamaProvider;
use crate::providers::{LLMError, LLMProvider, PromptOptions, ProviderId, TokenStream};
use crate::router::ProviderRouter;

#[derive(Debug, Clone)]
pub struct ServerStatus {
    pub online: bool,
    pub latency_ms: u128,
    pub concurrent_requests: usize,
}

pub struct ExecutionDispatchRequest {
    pub provider: ProviderId,
    pub prompt: String,
    pub options: PromptOptions,
    pub allow_remote_fallback: bool,
}

pub struct ExecutionDispatchResult {
    pub stream: TokenStream,
    pub server_used: String,
    pub fallback_used: bool,
    pub retries: usize,
    pub latency_ms: u128,
}

pub struct ExecutionDispatcher {
    router: Arc<ProviderRouter>,
    servers: Vec<ExecutionServerRuntime>,
    semaphores: Arc<RwLock<HashMap<String, Arc<Semaphore>>>>,
    status: Arc<RwLock<HashMap<String, ServerStatus>>>,
    circuits: Arc<RwLock<HashMap<String, CircuitState>>>,
}

const CIRCUIT_FAIL_THRESHOLD: u32 = 2;
const CIRCUIT_OPEN_SECS: u64 = 20;

#[derive(Debug, Clone, Default)]
struct CircuitState {
    consecutive_failures: u32,
    open_until: Option<Instant>,
}

impl ExecutionDispatcher {
    pub fn new(router: Arc<ProviderRouter>, servers: Vec<ExecutionServerRuntime>) -> Self {
        Self {
            router,
            servers,
            semaphores: Arc::new(RwLock::new(HashMap::new())),
            status: Arc::new(RwLock::new(HashMap::new())),
            circuits: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn dispatch(
        &self,
        request: ExecutionDispatchRequest,
    ) -> Result<ExecutionDispatchResult, LLMError> {
        let started = Instant::now();
        let requested_model = request.options.model.clone();
        let resolved_server_url = match (
            request.options.model_server_url.clone(),
            requested_model.as_deref(),
        ) {
            (Some(url), _) if !url.trim().is_empty() => Some(normalize_base_url(&url)),
            (None, Some(model)) => self.router.resolve_model_server(model).await,
            _ => None,
        };

        // Detect loopback primaries: if the registered Ollama server is on
        // 127.0.0.1 / ::1 / localhost, a failure must not trigger the remote
        // fallback loop — the problem is local and remote servers won't help.
        let primary_is_local = self
            .router
            .primary_ollama_url()
            .map(is_loopback_url)
            .unwrap_or(false);

        let primary_key = "primary-router".to_string();
        let primary_sem = self.semaphore_for(&primary_key, 1).await;
        let _primary_permit = primary_sem
            .acquire_owned()
            .await
            .map_err(|_| LLMError::Unexpected("primary semaphore unavailable".to_string()))?;

        let primary_result = self
            .router
            .stream_send(
                request.provider,
                request.prompt.clone(),
                request.options.clone(),
            )
            .await;

        match primary_result {
            Ok(stream) => {
                self.record_status(primary_key.clone(), true, started.elapsed().as_millis(), 1)
                    .await;
                Ok(ExecutionDispatchResult {
                    stream,
                    server_used: primary_key,
                    fallback_used: false,
                    retries: 0,
                    latency_ms: started.elapsed().as_millis(),
                })
            }
            Err(primary_err) => {
                // NOTE: We do NOT call record_status on the primary here.
                // The router's ProviderRouter.record_failure() already tracks
                // Ollama health via the circuit breaker in router.rs.
                // `ExecutionDispatcher.circuits` tracks only remote execution
                // servers, not the primary router path.

                if !request.allow_remote_fallback
                    || request.provider != ProviderId::Ollama
                    || self.servers.is_empty()
                    || primary_is_local
                {
                    return Err(primary_err);
                }

                // RETRY POLICY — Layer 4 (dispatcher, fallback loop):
                // Iterates remote execution_servers after the primary router
                // path fails. Each remote server is tried once; circuit-open
                // servers are skipped. 150ms base exponential backoff between
                // attempts (capped at 1200ms).
                // Scope: cross-server fallback. Only reached when primary is
                // non-loopback AND allow_remote_fallback = true.
                let mut retries = 0usize;
                let mut last_err = primary_err;
                for server in self.servers.iter().filter(|s| s.enabled) {
                    let key = format!("{}@{}", server.name, server.base_url);
                    let normalized_server_url = normalize_base_url(&server.base_url);
                    if let Some(expected_url) = resolved_server_url.as_deref() {
                        if normalized_server_url != expected_url {
                            continue;
                        }
                    }
                    if !self.server_available_for_attempt(&key).await {
                        continue;
                    }

                    retries += 1;
                    let sem = self
                        .semaphore_for(&key, server.max_concurrency.max(1))
                        .await;
                    let permit = sem.acquire_owned().await;
                    let Ok(_permit) = permit else {
                        self.record_status(key.clone(), false, 0, 0).await;
                        continue;
                    };

                    let provider = OllamaProvider::new(
                        normalized_server_url,
                        requested_model
                            .clone()
                            .unwrap_or_else(|| server.default_model.clone()),
                    );

                    let attempt_started = Instant::now();
                    let result = provider
                        .stream_send(request.prompt.clone(), request.options.clone())
                        .await;

                    match result {
                        Ok(stream) => {
                            self.mark_server_success(&key).await;
                            self.record_status(
                                key.clone(),
                                true,
                                attempt_started.elapsed().as_millis(),
                                1,
                            )
                            .await;
                            return Ok(ExecutionDispatchResult {
                                stream,
                                server_used: key,
                                fallback_used: true,
                                retries,
                                latency_ms: started.elapsed().as_millis(),
                            });
                        }
                        Err(err) => {
                            self.mark_server_failure(&key).await;
                            self.record_status(
                                key.clone(),
                                false,
                                attempt_started.elapsed().as_millis(),
                                0,
                            )
                            .await;
                            last_err = err;

                            let backoff_ms =
                                (150u64 * (1u64 << (retries.saturating_sub(1) as u32))).min(1200);
                            sleep(Duration::from_millis(backoff_ms)).await;
                        }
                    }
                }

                Err(last_err)
            }
        }
    }

    async fn semaphore_for(&self, key: &str, permits: usize) -> Arc<Semaphore> {
        if let Some(existing) = self.semaphores.read().await.get(key) {
            return existing.clone();
        }

        let mut guard = self.semaphores.write().await;
        guard
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(permits.max(1))))
            .clone()
    }

    async fn record_status(&self, key: String, online: bool, latency_ms: u128, concurrent: usize) {
        let mut guard = self.status.write().await;
        guard.insert(
            key,
            ServerStatus {
                online,
                latency_ms,
                concurrent_requests: concurrent,
            },
        );
    }

    async fn server_available_for_attempt(&self, key: &str) -> bool {
        let mut guard = self.circuits.write().await;
        let Some(circuit) = guard.get_mut(key) else {
            return true;
        };

        if let Some(open_until) = circuit.open_until {
            if Instant::now() < open_until {
                return false;
            }

            circuit.open_until = None;
            circuit.consecutive_failures = 0;
        }

        true
    }

    async fn mark_server_failure(&self, key: &str) {
        let mut guard = self.circuits.write().await;
        let circuit = guard.entry(key.to_string()).or_default();
        circuit.consecutive_failures = circuit.consecutive_failures.saturating_add(1);
        if circuit.consecutive_failures >= CIRCUIT_FAIL_THRESHOLD {
            circuit.open_until = Some(Instant::now() + Duration::from_secs(CIRCUIT_OPEN_SECS));
        }
    }

    async fn mark_server_success(&self, key: &str) {
        let mut guard = self.circuits.write().await;
        if let Some(circuit) = guard.get_mut(key) {
            circuit.consecutive_failures = 0;
            circuit.open_until = None;
        }
    }
}

fn normalize_base_url(input: &str) -> String {
    let trimmed = input.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return trimmed.to_string();
    }
    format!("http://{}", trimmed)
}

/// Returns true if the host part of `url` is a loopback address (127.0.0.1,
/// ::1, or the string "localhost"). Used to prevent remote fallback when the
/// primary Ollama target is local — a local failure will not be resolved by
/// trying a remote execution server.
pub(crate) fn is_loopback_url(url: &str) -> bool {
    // Strip scheme
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);

    // Extract host (drop path/port)
    let host = if rest.starts_with('[') {
        // IPv6 bracket form: [::1]:port/path
        rest.trim_start_matches('[')
            .split(']')
            .next()
            .unwrap_or(rest)
    } else {
        rest.split('/').next().unwrap_or(rest).split(':').next().unwrap_or(rest)
    };

    matches!(host, "localhost" | "127.0.0.1" | "::1")
}
