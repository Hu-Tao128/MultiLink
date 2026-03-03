use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{RwLock, Semaphore};

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
}

impl ExecutionDispatcher {
    pub fn new(router: Arc<ProviderRouter>, servers: Vec<ExecutionServerRuntime>) -> Self {
        Self {
            router,
            servers,
            semaphores: Arc::new(RwLock::new(HashMap::new())),
            status: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn dispatch(
        &self,
        request: ExecutionDispatchRequest,
    ) -> Result<ExecutionDispatchResult, LLMError> {
        let started = Instant::now();

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
                return Ok(ExecutionDispatchResult {
                    stream,
                    server_used: primary_key,
                    fallback_used: false,
                    retries: 0,
                    latency_ms: started.elapsed().as_millis(),
                });
            }
            Err(primary_err) => {
                self.record_status(primary_key.clone(), false, started.elapsed().as_millis(), 0)
                    .await;

                if request.provider != ProviderId::Ollama || self.servers.is_empty() {
                    return Err(primary_err);
                }

                let mut retries = 0usize;
                let mut last_err = primary_err;
                for server in self.servers.iter().filter(|s| s.enabled) {
                    retries += 1;
                    let key = format!("{}@{}", server.name, server.base_url);
                    let sem = self
                        .semaphore_for(&key, server.max_concurrency.max(1))
                        .await;
                    let permit = sem.acquire_owned().await;
                    let Ok(_permit) = permit else {
                        self.record_status(key.clone(), false, 0, 0).await;
                        continue;
                    };

                    let provider = OllamaProvider::new(
                        normalize_base_url(&server.base_url),
                        server.default_model.clone(),
                    );

                    let attempt_started = Instant::now();
                    let result = provider
                        .stream_send(request.prompt.clone(), request.options.clone())
                        .await;

                    match result {
                        Ok(stream) => {
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
                            self.record_status(
                                key.clone(),
                                false,
                                attempt_started.elapsed().as_millis(),
                                0,
                            )
                            .await;
                            last_err = err;
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
}

fn normalize_base_url(input: &str) -> String {
    let trimmed = input.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return trimmed.to_string();
    }
    format!("http://{}", trimmed)
}
