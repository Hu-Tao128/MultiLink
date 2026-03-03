use std::collections::HashMap;
use std::fs as stdfs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::Arc;
use std::time::{Duration, Instant};

use directories::ProjectDirs;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, watch, Mutex, RwLock, Semaphore};
use walkdir::WalkDir;

use crate::config::{ModelTier, RuntimeConfig};
use crate::context_retrieval::{build_relevant_project_context, RetrievalConfig};
use crate::execution::{ExecutionDispatchRequest, ExecutionDispatcher};
use crate::hardware_profile::{HardwareCaps, HardwareProfile};
use crate::intent_budget::{budget_for_intent, detect_query_intent};
use crate::model_profile::{ModelClass, ModelProfile};
use crate::observability::ExecutionMetrics;
use crate::providers::{PromptOptions, ProviderId};
use crate::router::ProviderRouter;
use crate::session::{ChatMessage, ChatSession, SessionState};

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Started,
    Chunk(String),
    Usage {
        prompt_tokens: usize,
        completion_tokens: usize,
        total_tokens: usize,
        is_estimated: bool,
    },
    Finished,
    Error(String),
}

pub struct ChatResponse {
    pub events: mpsc::Receiver<StreamEvent>,
}

pub struct HandleUserMessageRequest {
    pub session_id: String,
    pub prompt: String,
}

pub struct ChatRuntime {
    router: Arc<ProviderRouter>,
    execution_dispatcher: Arc<ExecutionDispatcher>,
    sessions: Arc<RwLock<HashMap<String, ChatSession>>>,
    active_session_id: Arc<RwLock<Option<String>>>,
    cancellation: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
    stream_slots: Arc<Semaphore>,
    storage_dir: PathBuf,
    persist_interval: Duration,
    runtime_config: RuntimeConfig,
    system_context_dir: Option<PathBuf>,
}

impl ChatRuntime {
    pub fn new(
        router: Arc<ProviderRouter>,
        storage_dir: PathBuf,
        persist_interval: Duration,
    ) -> Self {
        Self::new_with_config(router, storage_dir, persist_interval, RuntimeConfig::default(), None)
    }

    pub fn new_with_config(
        router: Arc<ProviderRouter>,
        storage_dir: PathBuf,
        persist_interval: Duration,
        runtime_config: RuntimeConfig,
        system_context_dir: Option<PathBuf>,
    ) -> Self {
        let hardware_caps = cached_hardware_caps();
        let mut runtime_config = runtime_config;
        runtime_config.max_parallel_streams = runtime_config
            .max_parallel_streams
            .min(hardware_caps.max_parallel_streams)
            .max(1);
        runtime_config.max_context_tokens = runtime_config
            .max_context_tokens
            .min(hardware_caps.max_context_tokens)
            .max(1024);
        runtime_config.max_project_context_tokens = runtime_config
            .max_project_context_tokens
            .min(hardware_caps.max_project_tokens)
            .max(256);
        runtime_config.context_project_top_k = runtime_config
            .context_project_top_k
            .min(hardware_caps.max_project_top_k)
            .max(2);

        Self {
            router: router.clone(),
            execution_dispatcher: Arc::new(ExecutionDispatcher::new(
                router.clone(),
                runtime_config.execution_servers.clone(),
            )),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            active_session_id: Arc::new(RwLock::new(None)),
            cancellation: Arc::new(Mutex::new(HashMap::new())),
            stream_slots: Arc::new(Semaphore::new(runtime_config.max_parallel_streams.max(1))),
            storage_dir,
            persist_interval,
            runtime_config,
            system_context_dir,
        }
    }

    pub fn new_portable(
        router: Arc<ProviderRouter>,
        persist_interval: Duration,
    ) -> Result<Self, ChatRuntimeError> {
        Self::new_portable_with_config(router, persist_interval, RuntimeConfig::default())
    }

    pub fn new_portable_with_config(
        router: Arc<ProviderRouter>,
        persist_interval: Duration,
        runtime_config: RuntimeConfig,
    ) -> Result<Self, ChatRuntimeError> {
        Self::new_portable_with_settings(router, persist_interval, runtime_config, None)
    }

    pub fn new_portable_with_settings(
        router: Arc<ProviderRouter>,
        persist_interval: Duration,
        runtime_config: RuntimeConfig,
        system_context_dir: Option<PathBuf>,
    ) -> Result<Self, ChatRuntimeError> {
        let storage_dir = app_data_dir()?.join("sessions");
        Ok(Self::new_with_config(
            router,
            storage_dir,
            persist_interval,
            runtime_config,
            system_context_dir,
        ))
    }

    pub async fn create_session(&self, provider: ProviderId, model: Option<String>) -> String {
        let session = ChatSession::new(provider, model);
        let session_id = session.id.clone();
        self.sessions
            .write()
            .await
            .insert(session_id.clone(), session.clone());
        {
            let mut active_guard = self.active_session_id.write().await;
            *active_guard = Some(session_id.clone());
        }

        let ids = self.sessions.read().await.keys().cloned().collect();
        let _ = persist_session(&self.storage_dir, &session).await;
        let _ = persist_index_and_state(&self.storage_dir, ids, Some(session_id.clone())).await;

        session_id
    }

    pub async fn list_sessions(&self) -> Vec<ChatSession> {
        let mut sessions: Vec<ChatSession> = self.sessions.read().await.values().cloned().collect();
        sessions.sort_by(|a, b| b.id.cmp(&a.id));
        sessions
    }

    pub async fn list_messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<ChatMessage>, ChatRuntimeError> {
        let guard = self.sessions.read().await;
        let session = guard
            .get(session_id)
            .ok_or(ChatRuntimeError::SessionNotFound)?;
        Ok(session.messages.clone())
    }

    pub async fn select_session(&self, session_id: &str) -> Result<(), ChatRuntimeError> {
        let exists = self.sessions.read().await.contains_key(session_id);
        if !exists {
            return Err(ChatRuntimeError::SessionNotFound);
        }

        {
            let mut active_guard = self.active_session_id.write().await;
            *active_guard = Some(session_id.to_string());
        }

        let ids = self.sessions.read().await.keys().cloned().collect();
        persist_index_and_state(&self.storage_dir, ids, Some(session_id.to_string())).await
    }

    pub async fn get_session_messages(&self, session_id: &str) -> Option<Vec<ChatMessage>> {
        self.sessions
            .read()
            .await
            .get(session_id)
            .map(|s| s.messages.clone())
    }

    pub async fn delete_empty_sessions(&self) -> Result<(), ChatRuntimeError> {
        let mut ids_to_delete = Vec::new();
        {
            let guard = self.sessions.read().await;
            for (id, session) in guard.iter() {
                if session.messages.is_empty() {
                    ids_to_delete.push(id.clone());
                }
            }
        }

        if ids_to_delete.is_empty() {
            return Ok(());
        }

        {
            let mut guard = self.sessions.write().await;
            for id in &ids_to_delete {
                guard.remove(id);
            }
            let mut active = self.active_session_id.write().await;
            if let Some(ref current_active) = *active {
                if ids_to_delete.contains(current_active) {
                    *active = None;
                }
            }
        }

        for id in &ids_to_delete {
            let path = self.storage_dir.join(format!("{}.json", id));
            let _ = tokio::fs::remove_file(path).await;
        }

        let ids = self.sessions.read().await.keys().cloned().collect();
        let new_active = self.active_session_id.read().await.clone();
        persist_index_and_state(&self.storage_dir, ids, new_active).await
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<(), ChatRuntimeError> {
        if let Some(cancel) = self.cancellation.lock().await.remove(session_id) {
            let _ = cancel.send(true);
        }

        {
            let mut guard = self.sessions.write().await;
            guard
                .remove(session_id)
                .ok_or(ChatRuntimeError::SessionNotFound)?;

            let mut active = self.active_session_id.write().await;
            if active.as_deref() == Some(session_id) {
                *active = guard.keys().max().cloned();
            }
        }

        let path = self.storage_dir.join(format!("{}.json", session_id));
        let _ = fs::remove_file(path).await;
        remove_partial_file(&self.storage_dir, session_id).await;

        let ids = self.sessions.read().await.keys().cloned().collect();
        let new_active = self.active_session_id.read().await.clone();
        persist_index_and_state(&self.storage_dir, ids, new_active).await
    }

    pub async fn active_session(&self) -> Option<String> {
        self.active_session_id.read().await.clone()
    }

    pub async fn update_session_model(
        &self,
        session_id: &str,
        model: Option<String>,
    ) -> Result<(), ChatRuntimeError> {
        let snapshot = {
            let mut guard = self.sessions.write().await;
            let session = guard
                .get_mut(session_id)
                .ok_or(ChatRuntimeError::SessionNotFound)?;
            session.model = model;
            session.clone()
        };

        persist_session(&self.storage_dir, &snapshot).await
    }

    pub async fn set_session_project_root(
        &self,
        session_id: &str,
        project_root: Option<String>,
    ) -> Result<(), ChatRuntimeError> {
        let model_hint = {
            let guard = self.sessions.read().await;
            guard
                .get(session_id)
                .ok_or(ChatRuntimeError::SessionNotFound)?
                .model
                .clone()
        };
        let context_config = self
            .runtime_config
            .effective_for_model(model_hint.as_deref());

        let cached_context = if let Some(root) = project_root.as_ref() {
            match build_project_context_for_root(root, context_config).await {
                Ok(context) => {
                    if context_debug_enabled(&self.runtime_config) {
                        eprintln!(
                            "[context] session={} project_root_set files_context_tokens={}",
                            session_id,
                            estimate_tokens(&context)
                        );
                    }
                    Some(context)
                }
                Err(e) => {
                    eprintln!("[context error] session={} failed to build project context: {:?}", session_id, e);
                    return Err(e);
                }
            }
        } else {
            None
        };

        let snapshot = {
            let mut guard = self.sessions.write().await;
            let session = guard
                .get_mut(session_id)
                .ok_or(ChatRuntimeError::SessionNotFound)?;
            session.project_root = project_root;
            session.project_context = cached_context;
            session.clone()
        };

        persist_session(&self.storage_dir, &snapshot).await
    }

    pub async fn send_message(
        &self,
        session_id: &str,
        prompt: String,
    ) -> Result<mpsc::Receiver<StreamEvent>, ChatRuntimeError> {
        let response = self
            .handle_user_message(HandleUserMessageRequest {
                session_id: session_id.to_string(),
                prompt,
            })
            .await?;
        Ok(response.events)
    }

    pub async fn handle_user_message(
        &self,
        request: HandleUserMessageRequest,
    ) -> Result<ChatResponse, ChatRuntimeError> {
        const EMIT_INTERVAL: Duration = Duration::from_millis(120);
        let _persist_interval = self.persist_interval;
        let session_id = request.session_id.as_str();
        let prompt = request.prompt;

        let (provider, model) = {
            let mut guard = self.sessions.write().await;
            let session = guard
                .get_mut(session_id)
                .ok_or(ChatRuntimeError::SessionNotFound)?;
            session.add_user_message(prompt.clone());
            session.set_state(SessionState::Sending);
            let provider = session.provider;
            let model = session.model.clone();
            let snapshot = session.clone();
            drop(guard);
            persist_session(&self.storage_dir, &snapshot).await?;
            (provider, model)
        };

        self.ensure_project_context_cached(session_id).await;

        self.maybe_summarize_session(session_id, provider, model.clone())
            .await;

        let mut effective_runtime = self.runtime_config.effective_for_model(model.as_deref());
        let hardware_caps = cached_hardware_caps();
        effective_runtime.max_context_tokens = effective_runtime
            .max_context_tokens
            .min(hardware_caps.max_context_tokens)
            .max(1024);
        effective_runtime.max_project_context_tokens = effective_runtime
            .max_project_context_tokens
            .min(hardware_caps.max_project_tokens)
            .max(256);
        effective_runtime.context_project_top_k = effective_runtime
            .context_project_top_k
            .min(hardware_caps.max_project_top_k)
            .max(2);

        let mut model_profile: Option<ModelProfile> = None;
        if let Some(model_name) = model.as_ref() {
            if let Ok(caps) = self.router.get_model_info(provider, model_name).await {
                let profile = ModelProfile::from_capabilities(model_name.clone(), &caps);
                let budget = profile.retrieval_budget();
                effective_runtime.max_context_tokens = effective_runtime
                    .max_context_tokens
                    .min(budget.safe_budget.max(1024));
                effective_runtime.max_project_context_tokens = effective_runtime
                    .max_project_context_tokens
                    .min(budget.project_budget.max(256));
                effective_runtime.context_project_top_k = effective_runtime
                    .context_project_top_k
                    .min(budget.project_top_k.max(2));

                if model_class_rank(profile.class) > model_class_rank(hardware_caps.max_model_class) {
                    effective_runtime.max_project_context_tokens = effective_runtime
                        .max_project_context_tokens
                        .min(512);
                    effective_runtime.context_project_top_k = effective_runtime
                        .context_project_top_k
                        .min(3)
                        .max(2);
                }
                model_profile = Some(profile);
            }
        }

        let intent = detect_query_intent(&prompt);
        let intent_budget = budget_for_intent(intent);
        let intent_project_cap = effective_runtime
            .max_context_tokens
            .saturating_mul(intent_budget.project_budget_ratio as usize)
            / 100;
        effective_runtime.max_project_context_tokens = effective_runtime
            .max_project_context_tokens
            .min(intent_project_cap.max(128));
        effective_runtime.context_project_top_k = effective_runtime
            .context_project_top_k
            .min(intent_budget.top_k_cap)
            .max(2);

        if context_debug_enabled(&self.runtime_config) {
            eprintln!(
                "[context] session={} intent={:?} hw_caps(ctx={},project={},top_k={}) effective(ctx={},project={},top_k={})",
                session_id,
                intent,
                hardware_caps.max_context_tokens,
                hardware_caps.max_project_tokens,
                hardware_caps.max_project_top_k,
                effective_runtime.max_context_tokens,
                effective_runtime.max_project_context_tokens,
                effective_runtime.context_project_top_k,
            );
        }

        let messages = self
            .build_messages(
                session_id,
                prompt.clone(),
                true,
                &effective_runtime,
                model_profile.as_ref(),
            )
            .await?;
        let context_tokens: usize = messages.iter().map(|m| estimate_tokens(&m.content)).sum();
        if context_debug_enabled(&self.runtime_config) {
            let context_tokens: usize = messages.iter().map(|m| estimate_tokens(&m.content)).sum();
            eprintln!(
                "[context] session={} context_tokens={} max_tokens={} message_count={}",
                session_id, context_tokens, effective_runtime.max_context_tokens,
                messages.len()
            );
        }

        let options = PromptOptions {
            model,
            messages: Some(messages),
            system_context_dir: self.system_context_dir.clone(),
            ..PromptOptions::default()
        };

        let stream_permit = self
            .stream_slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| ChatRuntimeError::Provider("stream concurrency limiter unavailable".to_string()))?;

        let stream_result = self
            .execution_dispatcher
            .dispatch(ExecutionDispatchRequest {
                provider,
                prompt: prompt.clone(),
                options: options.clone(),
            })
            .await;

        let mut fallback_retry_used = false;
        let stream = match stream_result {
            Ok(result) => result.stream,
            Err(err) => {
                let err_text = err.to_string();
                eprintln!("[provider] stream error: {}", err_text);
                if likely_context_overflow(&err_text) {
                    fallback_retry_used = true;
                    eprintln!("[provider] retrying without project context (possible context overflow)");
                    let fallback_messages = self
                        .build_messages(
                            session_id,
                            prompt.clone(),
                            false,
                            &effective_runtime,
                            model_profile.as_ref(),
                        )
                        .await?;
                    let fallback_options = PromptOptions {
                        model: options.model.clone(),
                        messages: Some(fallback_messages),
                        num_ctx: None,
                        ..PromptOptions::default()
                    };
                    self.execution_dispatcher
                        .dispatch(ExecutionDispatchRequest {
                            provider,
                            prompt,
                            options: fallback_options,
                        })
                        .await
                        .map(|r| r.stream)
                        .map_err(|e| ChatRuntimeError::Provider(e.to_string()))?
                } else {
                    return Err(ChatRuntimeError::Provider(err_text));
                }
            }
        };

        let (event_tx, event_rx) = mpsc::channel(128);
        let (cancel_tx, mut cancel_rx) = watch::channel(false);
        self.cancellation
            .lock()
            .await
            .insert(session_id.to_string(), cancel_tx);

        let sessions = self.sessions.clone();
        let storage_dir = self.storage_dir.clone();
        let cancellation = self.cancellation.clone();
        let session_id_owned = session_id.to_string();
        let model_used = options
            .model
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let metrics_server = effective_runtime.context_ollama_base_url.clone();
        let metrics_top_k = effective_runtime.context_project_top_k;
        let metrics_json = effective_runtime.observability_json_logs;
        let metrics_context_tokens = context_tokens;
        let started_at = Instant::now();
        let retries: usize = if fallback_retry_used { 1 } else { 0 };

        tokio::spawn(async move {
            let _permit = stream_permit;

            let _ = event_tx.send(StreamEvent::Started).await;
            {
                let mut guard = sessions.write().await;
                if let Some(session) = guard.get_mut(&session_id_owned) {
                    session.set_state(SessionState::Streaming);
                }
            }

            let mut stream = stream;
            let mut full_output = String::new();
            let mut pending_emit = String::new();
            let mut last_emit = Instant::now();
            let mut usage_prompt_tokens = 0usize;
            let mut usage_completion_tokens = 0usize;

            loop {
                tokio::select! {
                    cancel_changed = cancel_rx.changed() => {
                        if cancel_changed.is_ok() && *cancel_rx.borrow() {
                            let _ = event_tx.send(StreamEvent::Error("Generation cancelled".to_string())).await;
                            let _ = finalize_error(&sessions, &storage_dir, &session_id_owned, "Generation cancelled").await;
                            break;
                        }
                    }
                    item = stream.next() => {
                        match item {
                            Some(Ok(crate::providers::TokenEvent::Token(token))) => {
                                full_output.push_str(&token);
                                pending_emit.push_str(&token);
                                let _ = append_partial_chunk(&storage_dir, &session_id_owned, &token).await;

                                if last_emit.elapsed() >= EMIT_INTERVAL {
                                    if !pending_emit.is_empty() {
                                        let out = pending_emit.clone();
                                        pending_emit.clear();
                                        let _ = event_tx.send(StreamEvent::Chunk(out)).await;
                                    }
                                    last_emit = Instant::now();
                                }
                            }
                            Some(Ok(crate::providers::TokenEvent::Completed)) => {
                                if !pending_emit.is_empty() {
                                    let out = pending_emit.clone();
                                    pending_emit.clear();
                                    let _ = event_tx.send(StreamEvent::Chunk(out)).await;
                                }
                                let _ = finalize_success(&sessions, &storage_dir, &session_id_owned, &full_output).await;
                                let _ = event_tx.send(StreamEvent::Finished).await;
                                break;
                            }
                            Some(Ok(crate::providers::TokenEvent::Usage(usage))) => {
                                usage_prompt_tokens = usage.prompt_tokens;
                                usage_completion_tokens = usage.completion_tokens;
                                let _ = event_tx.send(StreamEvent::Usage {
                                    prompt_tokens: usage.prompt_tokens,
                                    completion_tokens: usage.completion_tokens,
                                    total_tokens: usage.total_tokens,
                                    is_estimated: usage.is_estimated,
                                }).await;
                            }
                            Some(Ok(crate::providers::TokenEvent::Started)) => {}
                            Some(Err(err)) => {
                                let message = err.to_string();
                                if !pending_emit.is_empty() {
                                    let out = pending_emit.clone();
                                    pending_emit.clear();
                                    let _ = event_tx.send(StreamEvent::Chunk(out)).await;
                                }
                                let _ = event_tx.send(StreamEvent::Error(message.clone())).await;
                                let _ = finalize_error(&sessions, &storage_dir, &session_id_owned, &message).await;
                                break;
                            }
                            None => {
                                if !pending_emit.is_empty() {
                                    let out = pending_emit.clone();
                                    pending_emit.clear();
                                    let _ = event_tx.send(StreamEvent::Chunk(out)).await;
                                }
                                let _ = finalize_success(&sessions, &storage_dir, &session_id_owned, &full_output).await;
                                let _ = event_tx.send(StreamEvent::Finished).await;
                                break;
                            }
                        }
                    }
                }
            }

            let metrics = ExecutionMetrics {
                timestamp_ms: ExecutionMetrics::now_timestamp_ms(),
                model: model_used,
                server: metrics_server,
                tokens_in: usage_prompt_tokens,
                tokens_out: usage_completion_tokens,
                context_tokens: metrics_context_tokens,
                top_k_applied: metrics_top_k,
                latency_ms: started_at.elapsed().as_millis(),
                fallback_used: fallback_retry_used,
                retries,
            };
            metrics.emit(metrics_json);

            cancellation.lock().await.remove(&session_id_owned);
        });

        Ok(ChatResponse { events: event_rx })
    }

    pub async fn cancel_stream(&self, session_id: &str) -> Result<(), ChatRuntimeError> {
        let mut guard = self.cancellation.lock().await;
        let sender = guard
            .remove(session_id)
            .ok_or(ChatRuntimeError::NoActiveStream)?;
        sender
            .send(true)
            .map_err(|_| ChatRuntimeError::NoActiveStream)
    }

    pub async fn load_sessions_from_disk(&self) -> Result<(), ChatRuntimeError> {
        if !self.storage_dir.exists() {
            return Ok(());
        }

        let index_path = self.storage_dir.join("index.json");
        let mut loaded_ids = Vec::new();
        let mut index_needs_rebuild = false;

        if index_path.exists() {
            let raw = fs::read_to_string(&index_path)
                .await
                .map_err(ChatRuntimeError::Io)?;
            let index: SessionIndex = serde_json::from_str(&raw)
                .map_err(|e| ChatRuntimeError::Serialization(e.to_string()))?;
            loaded_ids = index.session_ids;
        }

        let disk_ids = collect_session_ids_from_storage(&self.storage_dir).await?;

        if loaded_ids.is_empty() {
            loaded_ids = disk_ids;
            index_needs_rebuild = true;
        } else {
            for disk_id in &disk_ids {
                if !loaded_ids.contains(disk_id) {
                    loaded_ids.push(disk_id.clone());
                    index_needs_rebuild = true;
                }
            }
            loaded_ids.retain(|id| {
                let exists_on_disk = disk_ids.contains(id);
                if !exists_on_disk {
                    index_needs_rebuild = true;
                }
                exists_on_disk
            });
        }

        for session_id in loaded_ids {
            let path = self.storage_dir.join(format!("{}.json", session_id));
            if !path.exists() {
                continue;
            }
            let data = fs::read_to_string(&path)
                .await
                .map_err(ChatRuntimeError::Io)?;
            let session: ChatSession = serde_json::from_str(&data)
                .map_err(|e| ChatRuntimeError::Serialization(e.to_string()))?;
            self.sessions
                .write()
                .await
                .insert(session.id.clone(), session);
        }

        let state_path = self.storage_dir.join("state.json");
        if state_path.exists() {
            let raw = fs::read_to_string(&state_path)
                .await
                .map_err(ChatRuntimeError::Io)?;
            let state: RuntimeState = serde_json::from_str(&raw)
                .map_err(|e| ChatRuntimeError::Serialization(e.to_string()))?;
            *self.active_session_id.write().await = state.active_session_id;
        }

        {
            let active = self.active_session_id.read().await.clone();
            if let Some(active_id) = active {
                let exists = self.sessions.read().await.contains_key(&active_id);
                if !exists {
                    *self.active_session_id.write().await = None;
                    index_needs_rebuild = true;
                }
            }
        }

        if index_needs_rebuild {
            let ids = self.sessions.read().await.keys().cloned().collect();
            let active = self.active_session_id.read().await.clone();
            let _ = persist_index_and_state(&self.storage_dir, ids, active).await;
        }

        let _ = recover_partials_from_wal(&self.sessions, &self.storage_dir).await;

        Ok(())
    }
}

impl ChatRuntime {
    async fn ensure_project_context_cached(&self, session_id: &str) {
        let (root, model_hint) = {
            let guard = self.sessions.read().await;
            let Some(session) = guard.get(session_id) else {
                return;
            };
            if session
                .project_context
                .as_ref()
                .is_some_and(|ctx| !ctx.trim().is_empty())
            {
                return;
            }
            (session.project_root.clone(), session.model.clone())
        };

        let Some(project_root) = root else {
            return;
        };

        let context_config = self
            .runtime_config
            .effective_for_model(model_hint.as_deref());

        let context = match build_project_context_for_root(&project_root, context_config).await {
            Ok(value) => value,
            Err(err) => {
                if context_debug_enabled(&self.runtime_config) {
                    eprintln!(
                        "[context] session={} project_context=error error={}",
                        session_id, err
                    );
                }
                return;
            }
        };

        let mut guard = self.sessions.write().await;
        if let Some(session) = guard.get_mut(session_id) {
            session.project_context = Some(context);
        }
    }

    async fn build_messages(
        &self,
        session_id: &str,
        current_prompt: String,
        include_project_context: bool,
        effective_runtime: &RuntimeConfig,
        model_profile: Option<&ModelProfile>,
    ) -> Result<Vec<ChatMessage>, ChatRuntimeError> {
        let session = {
            let guard = self.sessions.read().await;
            guard
                .get(session_id)
                .cloned()
                .ok_or(ChatRuntimeError::SessionNotFound)?
        };

        let mut messages = Vec::new();
        let mut tokens = 0usize;
        let model_hint = session.model.as_deref();
        let model_tier = match model_profile.map(|p| p.class) {
            Some(ModelClass::Tiny | ModelClass::Small) => ModelTier::Small,
            Some(ModelClass::Medium) => ModelTier::Medium,
            Some(ModelClass::Large) => ModelTier::Large,
            None => self.runtime_config.tier_for_model(model_hint),
        };
        let prompt_intent = detect_query_intent(&current_prompt);

        let mut system_content = String::new();

        system_content.push_str("You are a senior software engineer.\n");
        if model_tier == ModelTier::Small {
            system_content.push_str(
                "Use this exact response structure:\n1) Brief answer\n2) Evidence (at least 2 files)\n3) Uncertainties.\n",
            );
            system_content.push_str(
                "Every important claim must reference at least one file from Project Context. If evidence is missing, respond exactly: No tengo suficiente contexto. Do not guess. Do not suggest external tools/libraries unless they already appear in Project Context files.\n\n",
            );
        } else {
            system_content.push_str(
                "Every important claim must reference at least one file from Project Context. If evidence is missing, respond exactly: No tengo suficiente contexto.\n\n",
            );
        }

        match prompt_intent {
            crate::intent_budget::QueryIntent::FileScoped => {
                system_content.push_str(
                    "The user asked about specific file-level changes. Provide concrete edits for the referenced files only, with short actionable bullets. Avoid generic recommendations.\n\n",
                );
            }
            crate::intent_budget::QueryIntent::SymbolScoped => {
                system_content.push_str(
                    "The user asked about function/class-level details. Keep the answer tightly scoped to symbols found in context.\n\n",
                );
            }
            _ => {}
        }

        if let Some(system_context_path) = &self.system_context_dir {
            match build_system_context(system_context_path, effective_runtime.clone()).await {
                Ok(context) => {
                    if context_debug_enabled(&self.runtime_config) {
                        eprintln!(
                            "[context] session={} system_context_dir={} files_context_tokens={}",
                            session_id,
                            system_context_path.display(),
                            estimate_tokens(&context)
                        );
                    }
                    system_content.push_str("This conversation is about a project in the following system directory:\n");
                    system_content.push_str(&context);
                    system_content.push('\n');
                }
                Err(e) => {
                    eprintln!("[context error] session={} failed to build system context: {:?}", session_id, e);
                }
            }
        }

        if include_project_context {
            if let Some(project_context) = session
                .project_context
                .as_ref()
                .filter(|v| !v.trim().is_empty())
            {
                let context_budget =
                    effective_runtime
                        .max_project_context_tokens
                        .min(effective_runtime.max_context_tokens.saturating_sub(tokens));
                if context_budget > 0 {
                    let retrieval = build_relevant_project_context(
                        project_context,
                        &current_prompt,
                        context_budget,
                        model_hint,
                        RetrievalConfig {
                            embeddings_enabled: effective_runtime.context_embeddings_enabled,
                            embed_model: effective_runtime.context_embed_model.clone(),
                            ollama_base_url: effective_runtime.context_ollama_base_url.clone(),
                            top_k: effective_runtime.context_project_top_k,
                        },
                    )
                    .await;
                    if !retrieval.context.trim().is_empty() {
                        system_content.push_str("This conversation is about the following project:\n");
                        system_content.push_str(&retrieval.context);
                        system_content.push('\n');
                        if context_debug_enabled(&self.runtime_config) {
                            eprintln!(
                                "[context] session={} model={:?} top_k={} embeddings={} selected_files={:?} context_tokens={}",
                                session_id,
                                model_hint,
                                retrieval.top_k,
                                retrieval.embedding_used,
                                retrieval.selected_files,
                                retrieval.used_tokens
                            );
                        }
                    }
                }
            }
        }

        if let Some(summary) = session.summary.as_ref().filter(|v| !v.trim().is_empty()) {
            system_content.push_str("Conversation summary:\n");
            system_content.push_str(summary);
            system_content.push('\n');
        }

        if !system_content.is_empty() {
            tokens += estimate_tokens_for_model(&system_content, model_hint);
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: system_content,
                timestamp: 0,
            });
        }

        let base_index = session.summarized_messages.min(session.messages.len());
        for msg in session.messages.iter().skip(base_index) {
            let t = estimate_tokens_for_model(&msg.content, model_hint);
            if tokens + t > effective_runtime.max_context_tokens {
                break;
            }
            tokens += t;
            messages.push(msg.clone());
        }

        messages.push(ChatMessage {
            role: "user".to_string(),
            content: current_prompt,
            timestamp: 0,
        });

        Ok(messages)
    }

    async fn maybe_summarize_session(
        &self,
        session_id: &str,
        provider: ProviderId,
        model: Option<String>,
    ) {
        let session = {
            let guard = self.sessions.read().await;
            match guard.get(session_id) {
                Some(s) => s.clone(),
                None => return,
            }
        };

        if estimate_session_tokens(&session) <= self.runtime_config.summary_trigger_tokens {
            if context_debug_enabled(&self.runtime_config) {
                eprintln!(
                    "[context] session={} summarize=no estimated_tokens={} trigger={}",
                    session_id,
                    estimate_session_tokens(&session),
                    self.runtime_config.summary_trigger_tokens
                );
            }
            return;
        }

        let end_index = session
            .messages
            .len()
            .saturating_sub(self.runtime_config.keep_last_messages);
        if end_index <= session.summarized_messages {
            return;
        }

        let mut history = String::new();
        for msg in session
            .messages
            .iter()
            .skip(session.summarized_messages)
            .take(end_index.saturating_sub(session.summarized_messages))
        {
            history.push_str(&format!("{}: {}\n", role_label(&msg.role), msg.content));
        }
        if history.trim().is_empty() {
            return;
        }

        let mut summary_prompt = String::from(
            "Resume the following conversation history.\nKeep:\n- user goals\n- technical decisions\n- constraints\n- open questions\n\nBe concise, factual, and neutral.\nDo NOT include greetings or filler.\n",
        );
        if let Some(prev) = session.summary.as_ref().filter(|v| !v.trim().is_empty()) {
            summary_prompt.push_str("\nCurrent summary:\n");
            summary_prompt.push_str(prev);
            summary_prompt.push('\n');
        }
        summary_prompt.push_str("\nConversation:\n");
        summary_prompt.push_str(&history);

        let options = PromptOptions {
            model,
            temperature: Some(0.2),
            system_context_dir: self.system_context_dir.clone(),
            ..PromptOptions::default()
        };

        let new_summary = match self.router.send(provider, summary_prompt, options).await {
            Ok(response) => response.text.trim().to_string(),
            Err(err) => {
                if context_debug_enabled(&self.runtime_config) {
                    eprintln!(
                        "[context] session={} summarize=error error={}",
                        session_id, err
                    );
                }
                return;
            }
        };
        if new_summary.is_empty() {
            if context_debug_enabled(&self.runtime_config) {
                eprintln!("[context] session={} summarize=empty", session_id);
            }
            return;
        }

        let capped_summary = if estimate_tokens_for_model(&new_summary, session.model.as_deref())
            > self.runtime_config.max_summary_tokens
        {
            truncate_to_token_budget(
                &new_summary,
                self.runtime_config.max_summary_tokens,
                session.model.as_deref(),
            )
        } else {
            new_summary
        };

        let snapshot = {
            let mut guard = self.sessions.write().await;
            let Some(live) = guard.get_mut(session_id) else {
                return;
            };
            live.summary = Some(capped_summary);
            live.summarized_messages = end_index.min(live.messages.len());
            live.clone()
        };

        let _ = persist_session(&self.storage_dir, &snapshot).await;
        if context_debug_enabled(&self.runtime_config) {
            eprintln!(
                "[context] session={} summarize=yes summarized_messages={} summary_tokens={}",
                session_id,
                snapshot.summarized_messages,
                snapshot
                    .summary
                    .as_ref()
                    .map(|s| estimate_tokens(s))
                    .unwrap_or(0)
            );
        }
    }
}

fn likely_context_overflow(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("context length")
        || lower.contains("context window")
        || lower.contains("too many tokens")
        || lower.contains("maximum context")
        || lower.contains("prompt is too long")
        || lower.contains("token limit")
}

fn role_label(role: &str) -> &str {
    match role {
        "user" => "User",
        "assistant" => "Assistant",
        "system" => "System",
        other => other,
    }
}

fn estimate_tokens(s: &str) -> usize {
    estimate_tokens_for_model(s, None)
}

fn estimate_tokens_for_model(s: &str, _model: Option<&str>) -> usize {
    if let Some(tokenizer) = cl100k_tokenizer() {
        return tokenizer.encode_with_special_tokens(s).len();
    }
    let graphemes = s.chars().count();
    let words = s.split_whitespace().count();
    graphemes / 4 + words / 2
}

fn estimate_session_tokens(session: &ChatSession) -> usize {
    let mut total = 0usize;
    let model_hint = session.model.as_deref();
    if let Some(summary) = session.summary.as_ref() {
        total += estimate_tokens_for_model(summary, model_hint);
    }
    let from = session.summarized_messages.min(session.messages.len());
    for msg in session.messages.iter().skip(from) {
        total += estimate_tokens_for_model(&msg.content, model_hint);
    }
    total
}

fn truncate_to_token_budget(text: &str, max_tokens: usize, model: Option<&str>) -> String {
    if estimate_tokens_for_model(text, model) <= max_tokens {
        return text.to_string();
    }

    let mut boundaries: Vec<usize> = text.char_indices().map(|(idx, _)| idx).collect();
    boundaries.push(text.len());
    let mut left = 0usize;
    let mut right = boundaries.len().saturating_sub(1);
    let mut best = 0usize;

    while left <= right {
        let mid = left + (right - left) / 2;
        let end = boundaries[mid];
        let tokens = estimate_tokens_for_model(&text[..end], model);
        if tokens <= max_tokens {
            best = end;
            left = mid.saturating_add(1);
        } else if mid == 0 {
            break;
        } else {
            right = mid - 1;
        }
    }

    text[..best].to_string()
}

fn cl100k_tokenizer() -> Option<&'static tiktoken_rs::CoreBPE> {
    static TOKENIZER: OnceLock<Option<tiktoken_rs::CoreBPE>> = OnceLock::new();
    TOKENIZER
        .get_or_init(|| tiktoken_rs::cl100k_base().ok())
        .as_ref()
}

async fn build_project_context_for_root(
    project_root: &str,
    runtime_config: RuntimeConfig,
) -> Result<String, ChatRuntimeError> {
    let root = PathBuf::from(project_root);
    let root_for_task = root.clone();
    tokio::task::spawn_blocking(move || collect_project_snapshot(&root_for_task, &runtime_config))
        .await
        .map_err(|e| ChatRuntimeError::Path(format!("project scan task failed: {}", e)))
        .and_then(|r| r)
        .map(|snapshot| snapshot.render())
}

async fn build_system_context(
    system_context_dir: &PathBuf,
    runtime_config: RuntimeConfig,
) -> Result<String, ChatRuntimeError> {
    let root_for_task = system_context_dir.clone();
    tokio::task::spawn_blocking(move || collect_project_snapshot(&root_for_task, &runtime_config))
        .await
        .map_err(|e| ChatRuntimeError::Path(format!("system context scan task failed: {}", e)))
        .and_then(|r| r)
        .map(|snapshot| snapshot.render())
}

struct ProjectSnapshot {
    root: PathBuf,
    files: Vec<(PathBuf, String)>,
}

impl ProjectSnapshot {
    fn render(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!("Project root: {}\n\n", self.root.display()));
        output.push_str("Project structure:\n");
        for (path, _) in &self.files {
            output.push_str(&format!("- {}\n", path.display()));
        }
        output.push_str("\nFiles:\n\n");

        for (path, content) in &self.files {
            let fence = code_fence_for_path(path);
            output.push_str(&format!(
                "File: {}\n```{}\n{}\n```\n\n",
                path.display(),
                fence,
                content
            ));
        }

        output
    }
}

fn collect_project_snapshot(
    root: &Path,
    runtime_config: &RuntimeConfig,
) -> Result<ProjectSnapshot, ChatRuntimeError> {
    if !root.exists() {
        return Err(ChatRuntimeError::Path(format!(
            "project root does not exist: {}",
            root.display()
        )));
    }
    if !root.is_dir() {
        return Err(ChatRuntimeError::Path(format!(
            "project root is not a directory: {}",
            root.display()
        )));
    }

    let mut candidate_paths = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !should_skip_entry(e.path(), e.file_type().is_dir()))
    {
        let entry = entry.map_err(|e| ChatRuntimeError::Path(format!("walkdir error: {}", e)))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !is_allowed_project_file(path) {
            continue;
        }
        candidate_paths.push(path.to_path_buf());
    }

    candidate_paths.sort_by(|a, b| {
        let pa = project_file_priority(root, a);
        let pb = project_file_priority(root, b);
        pb.cmp(&pa).then_with(|| a.cmp(b))
    });

    let mut files = Vec::new();
    let mut total_bytes = 0usize;
    for path in candidate_paths {
        if files.len() >= runtime_config.max_project_files
            || total_bytes >= runtime_config.max_project_bytes
        {
            break;
        }

        let metadata = match stdfs::metadata(&path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if metadata.len() as usize > runtime_config.max_project_file_bytes {
            continue;
        }

        let content_bytes = match stdfs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let content = match String::from_utf8(content_bytes) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if total_bytes + content.len() > runtime_config.max_project_bytes {
            break;
        }
        total_bytes += content.len();

        let relative = path
            .strip_prefix(root)
            .map_err(|e| ChatRuntimeError::Path(format!("path strip error: {}", e)))?
            .to_path_buf();
        files.push((relative, content));
    }

    Ok(ProjectSnapshot {
        root: root.to_path_buf(),
        files,
    })
}

fn should_skip_entry(path: &Path, is_dir: bool) -> bool {
    if !is_dir {
        return false;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    matches!(
        name,
        ".git"
            | "node_modules"
            | "target"
            | "build"
            | ".venv"
            | "venv"
            | "env"
            | "__pycache__"
            | ".pytest_cache"
            | ".mypy_cache"
            | ".idea"
            | ".vscode"
            | "dist"
    )
}

fn is_allowed_project_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some(
            "rs" | "py"
                | "js"
                | "mjs"
                | "cjs"
                | "jsx"
                | "ts"
                | "tsx"
                | "dart"
                | "java"
                | "kt"
                | "kts"
                | "scala"
                | "cpp"
                | "c"
                | "cc"
                | "cxx"
                | "h"
                | "hpp"
                | "hh"
                | "cs"
                | "go"
                | "swift"
                | "php"
                | "rb"
                | "lua"
                | "zig"
                | "sh"
                | "bash"
                | "qml"
                | "json"
                | "toml"
                | "md"
                | "txt"
                | "yml"
                | "yaml"
        )
    )
}

fn code_fence_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => "rust",
        Some("py") => "python",
        Some("js") => "javascript",
        Some("mjs") => "javascript",
        Some("cjs") => "javascript",
        Some("jsx") => "jsx",
        Some("ts") => "typescript",
        Some("tsx") => "tsx",
        Some("dart") => "dart",
        Some("java") => "java",
        Some("kt") => "kotlin",
        Some("kts") => "kotlin",
        Some("scala") => "scala",
        Some("cpp") => "cpp",
        Some("c") => "c",
        Some("cc") => "cpp",
        Some("cxx") => "cpp",
        Some("h") => "c",
        Some("hpp") => "cpp",
        Some("hh") => "cpp",
        Some("cs") => "csharp",
        Some("go") => "go",
        Some("swift") => "swift",
        Some("php") => "php",
        Some("rb") => "ruby",
        Some("lua") => "lua",
        Some("zig") => "zig",
        Some("sh") => "bash",
        Some("bash") => "bash",
        Some("qml") => "qml",
        Some("json") => "json",
        Some("toml") => "toml",
        Some("md") => "markdown",
        _ => "text",
    }
}

fn project_file_priority(root: &Path, file_path: &Path) -> i32 {
    let mut score = 0i32;

    let rel = file_path.strip_prefix(root).unwrap_or(file_path);
    let depth = rel.components().count().saturating_sub(1);
    if depth == 0 {
        score += 30;
    } else if depth == 1 {
        score += 16;
    } else if depth == 2 {
        score += 8;
    }

    let file_name = rel
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let is_readme = file_name == "readme.md" || file_name == "readme.txt" || file_name == "readme";
    if is_readme {
        if depth == 0 {
            score += 50;
        } else if depth == 1 {
            score += 8;
        } else {
            score -= 8;
        }
    }
    if matches!(
        file_name.as_str(),
        "main.py"
            | "main.rs"
            | "main.ts"
            | "main.js"
            | "app.py"
            | "app.ts"
            | "app.js"
            | "chrome.py"
            | "tts.py"
    ) {
        score += 24;
    }

    let ext = rel.extension().and_then(|e| e.to_str()).unwrap_or_default();
    let lang_weight = match ext {
        "rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "java" | "dart" | "cpp" | "c"
        | "cc" | "cxx" | "cs" | "go" | "swift" | "kt" | "kts" | "php" | "rb"
        | "scala" | "zig" => 14,
        "md" => {
            if depth == 0 {
                8
            } else {
                1
            }
        }
        "toml" | "json" | "yaml" | "yml" => 5,
        _ => 2,
    };
    score += lang_weight;

    score
}

fn context_debug_enabled(runtime_config: &RuntimeConfig) -> bool {
    if let Ok(v) = std::env::var("MULTILINK_DEBUG_CONTEXT") {
        return v == "1" || v.eq_ignore_ascii_case("true");
    }
    runtime_config.context_debug
}

fn cached_hardware_caps() -> HardwareCaps {
    static CAPS: OnceLock<HardwareCaps> = OnceLock::new();
    *CAPS.get_or_init(|| {
        let profile = HardwareProfile::detect();
        profile.derive_caps()
    })
}

fn model_class_rank(class: ModelClass) -> u8 {
    match class {
        ModelClass::Tiny => 0,
        ModelClass::Small => 1,
        ModelClass::Medium => 2,
        ModelClass::Large => 3,
    }
}

fn app_data_dir() -> Result<PathBuf, ChatRuntimeError> {
    let dirs = ProjectDirs::from("com", "multilink", "multilink").ok_or_else(|| {
        ChatRuntimeError::Path("cannot resolve project data directory".to_string())
    })?;
    Ok(dirs.data_dir().to_path_buf())
}

async fn finalize_success(
    sessions: &Arc<RwLock<HashMap<String, ChatSession>>>,
    storage_dir: &Path,
    session_id: &str,
    text: &str,
) -> Result<(), ChatRuntimeError> {
    let snapshot = {
        let mut guard = sessions.write().await;
        let session = guard
            .get_mut(session_id)
            .ok_or(ChatRuntimeError::SessionNotFound)?;
        session.add_assistant_message(text.to_string());
        session.set_state(SessionState::Done);
        session.clone()
    };
    persist_session(storage_dir, &snapshot).await?;
    remove_partial_file(storage_dir, session_id).await;
    Ok(())
}

async fn finalize_error(
    sessions: &Arc<RwLock<HashMap<String, ChatSession>>>,
    storage_dir: &Path,
    session_id: &str,
    error: &str,
) -> Result<(), ChatRuntimeError> {
    let snapshot = {
        let mut guard = sessions.write().await;
        let session = guard
            .get_mut(session_id)
            .ok_or(ChatRuntimeError::SessionNotFound)?;
        session.set_state(SessionState::Error(error.to_string()));
        session.clone()
    };
    persist_session(storage_dir, &snapshot).await?;
    remove_partial_file(storage_dir, session_id).await;
    Ok(())
}

async fn append_partial_chunk(
    storage_dir: &Path,
    session_id: &str,
    chunk: &str,
) -> Result<(), ChatRuntimeError> {
    if chunk.is_empty() {
        return Ok(());
    }

    let partial_path = storage_dir.join(format!("{}.partial.log", session_id));
    if !storage_dir.exists() {
        fs::create_dir_all(storage_dir)
            .await
            .map_err(ChatRuntimeError::Io)?;
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&partial_path)
        .await
        .map_err(ChatRuntimeError::Io)?;
    file.write_all(chunk.as_bytes())
        .await
        .map_err(ChatRuntimeError::Io)
}

async fn persist_session(
    storage_dir: &Path,
    session: &ChatSession,
) -> Result<(), ChatRuntimeError> {
    if !storage_dir.exists() {
        fs::create_dir_all(storage_dir)
            .await
            .map_err(ChatRuntimeError::Io)?;
    }
    let path = storage_dir.join(format!("{}.json", session.id));
    write_atomic_json(&path, session).await
}

async fn persist_index_and_state(
    storage_dir: &Path,
    mut session_ids: Vec<String>,
    active_session_id: Option<String>,
) -> Result<(), ChatRuntimeError> {
    if !storage_dir.exists() {
        fs::create_dir_all(storage_dir)
            .await
            .map_err(ChatRuntimeError::Io)?;
    }

    session_ids.sort();
    session_ids.dedup();

    let index = SessionIndex { session_ids };
    write_atomic_json(&storage_dir.join("index.json"), &index).await?;

    let state = RuntimeState { active_session_id };
    write_atomic_json(&storage_dir.join("state.json"), &state).await
}

async fn write_atomic_json<T: serde::Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), ChatRuntimeError> {
    let data = serde_json::to_vec_pretty(value)
        .map_err(|e| ChatRuntimeError::Serialization(e.to_string()))?;
    let temp_path = path.with_extension("tmp");
    let mut file = fs::File::create(&temp_path)
        .await
        .map_err(ChatRuntimeError::Io)?;
    file.write_all(&data)
        .await
        .map_err(ChatRuntimeError::Io)?;
    file.sync_all()
        .await
        .map_err(ChatRuntimeError::Io)?;
    drop(file);

    fs::rename(&temp_path, path)
        .await
        .map_err(ChatRuntimeError::Io)?;

    if let Some(parent) = path.parent() {
        sync_directory(parent).await?;
    }

    Ok(())
}

async fn sync_directory(path: &Path) -> Result<(), ChatRuntimeError> {
    let dir = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let handle = std::fs::File::open(&dir)?;
        handle.sync_all()
    })
    .await
    .map_err(|e| ChatRuntimeError::Path(format!("directory sync task failed: {}", e)))?
    .map_err(ChatRuntimeError::Io)
}

async fn collect_session_ids_from_storage(storage_dir: &Path) -> Result<Vec<String>, ChatRuntimeError> {
    let mut entries = fs::read_dir(storage_dir)
        .await
        .map_err(ChatRuntimeError::Io)?;
    let mut ids = Vec::new();

    while let Some(entry) = entries.next_entry().await.map_err(ChatRuntimeError::Io)? {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if file_stem != "index" && file_stem != "state" {
            ids.push(file_stem.to_string());
        }
    }

    ids.sort();
    ids.dedup();
    Ok(ids)
}

async fn remove_partial_file(storage_dir: &Path, session_id: &str) {
    let partial_path = storage_dir.join(format!("{}.partial.log", session_id));
    if partial_path.exists() {
        let _ = fs::remove_file(partial_path).await;
    }
}

async fn recover_partials_from_wal(
    sessions: &Arc<RwLock<HashMap<String, ChatSession>>>,
    storage_dir: &Path,
) -> Result<(), ChatRuntimeError> {
    if !storage_dir.exists() {
        return Ok(());
    }

    let mut entries = fs::read_dir(storage_dir)
        .await
        .map_err(ChatRuntimeError::Io)?;
    while let Some(entry) = entries.next_entry().await.map_err(ChatRuntimeError::Io)? {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("log") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !name.ends_with(".partial.log") {
            continue;
        }

        let session_id = name.trim_end_matches(".partial.log").to_string();
        let partial_text = fs::read_to_string(&path).await.unwrap_or_default();

        let snapshot = if partial_text.is_empty() {
            None
        } else {
            let mut guard = sessions.write().await;
            if let Some(session) = guard.get_mut(&session_id) {
                let should_recover = matches!(
                    session.state,
                    SessionState::Streaming | SessionState::Sending
                ) || !matches!(session.messages.last(), Some(last) if last.role == "assistant");

                if should_recover {
                    session.add_assistant_message(partial_text);
                    session.set_state(SessionState::Error(
                        "Recovered unfinished response".to_string(),
                    ));
                    Some(session.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(recovered) = snapshot {
            let _ = persist_session(storage_dir, &recovered).await;
        }

        let _ = fs::remove_file(&path).await;
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum ChatRuntimeError {
    #[error("session not found")]
    SessionNotFound,
    #[error("provider error: {0}")]
    Provider(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("no active stream for this session")]
    NoActiveStream,
    #[error("path resolution error: {0}")]
    Path(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionIndex {
    session_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RuntimeState {
    active_session_id: Option<String>,
}
