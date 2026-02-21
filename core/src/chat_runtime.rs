use std::collections::HashMap;
use std::fs as stdfs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use directories::ProjectDirs;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, watch, Mutex, RwLock};
use walkdir::WalkDir;

use crate::providers::{PromptOptions, ProviderId};
use crate::router::ProviderRouter;
use crate::session::{ChatMessage, ChatSession, SessionState};

const MAX_CONTEXT_TOKENS: usize = 32768;
const SUMMARY_TRIGGER_TOKENS: usize = 6000;
const KEEP_LAST_MESSAGES: usize = 6;
const MAX_SUMMARY_TOKENS: usize = 1200;
const MAX_PROJECT_FILES: usize = 30;
const MAX_PROJECT_BYTES: usize = 200 * 1024;
const MAX_PROJECT_CONTEXT_TOKENS: usize = 24000;

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Started,
    Chunk(String),
    Finished,
    Error(String),
}

pub struct ChatRuntime {
    router: Arc<ProviderRouter>,
    sessions: Arc<RwLock<HashMap<String, ChatSession>>>,
    active_session_id: Arc<RwLock<Option<String>>>,
    cancellation: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
    storage_dir: PathBuf,
    persist_interval: Duration,
}

impl ChatRuntime {
    pub fn new(
        router: Arc<ProviderRouter>,
        storage_dir: PathBuf,
        persist_interval: Duration,
    ) -> Self {
        Self {
            router,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            active_session_id: Arc::new(RwLock::new(None)),
            cancellation: Arc::new(Mutex::new(HashMap::new())),
            storage_dir,
            persist_interval,
        }
    }

    pub fn new_portable(
        router: Arc<ProviderRouter>,
        persist_interval: Duration,
    ) -> Result<Self, ChatRuntimeError> {
        let storage_dir = app_data_dir()?.join("sessions");
        Ok(Self::new(router, storage_dir, persist_interval))
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
        let cached_context = if let Some(root) = project_root.as_ref() {
            match build_project_context_for_root(root).await {
                Ok(context) => {
                    if context_debug_enabled() {
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
        const EMIT_INTERVAL: Duration = Duration::from_millis(120);
        let _persist_interval = self.persist_interval;

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

        let (system_prompt, routed_prompt) = self.build_context(session_id).await?;
        if context_debug_enabled() {
            let context_tokens = estimate_tokens(&routed_prompt) + estimate_tokens(&system_prompt);
            eprintln!(
                "[context] session={} context_tokens={} max_tokens={}",
                session_id, context_tokens, MAX_CONTEXT_TOKENS
            );
        }

        let options = PromptOptions {
            model,
            system_prompt: Some(system_prompt).filter(|s| !s.is_empty()),
            ..PromptOptions::default()
        };

        let stream = self
            .router
            .stream_send(provider, routed_prompt, options)
            .await
            .map_err(|e| ChatRuntimeError::Provider(e.to_string()))?;

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

        tokio::spawn(async move {
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

            cancellation.lock().await.remove(&session_id_owned);
        });

        Ok(event_rx)
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

        if index_path.exists() {
            let raw = fs::read_to_string(&index_path)
                .await
                .map_err(ChatRuntimeError::Io)?;
            let index: SessionIndex = serde_json::from_str(&raw)
                .map_err(|e| ChatRuntimeError::Serialization(e.to_string()))?;
            loaded_ids = index.session_ids;
        }

        if loaded_ids.is_empty() {
            let mut entries = fs::read_dir(&self.storage_dir)
                .await
                .map_err(ChatRuntimeError::Io)?;
            while let Some(entry) = entries.next_entry().await.map_err(ChatRuntimeError::Io)? {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                if file_stem != "index" && file_stem != "state" {
                    loaded_ids.push(file_stem.to_string());
                }
            }
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
                }
            }
        }

        let _ = recover_partials_from_wal(&self.sessions, &self.storage_dir).await;

        Ok(())
    }
}

impl ChatRuntime {
    async fn ensure_project_context_cached(&self, session_id: &str) {
        let root = {
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
            session.project_root.clone()
        };

        let Some(project_root) = root else {
            return;
        };

        let context = match build_project_context_for_root(&project_root).await {
            Ok(value) => value,
            Err(err) => {
                if context_debug_enabled() {
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

    async fn build_context(&self, session_id: &str) -> Result<(String, String), ChatRuntimeError> {
        let session = {
            let guard = self.sessions.read().await;
            guard
                .get(session_id)
                .cloned()
                .ok_or(ChatRuntimeError::SessionNotFound)?
        };

        let mut tokens = 0usize;
        let mut system_chunks = Vec::new();

        let model_supported = session
            .model
            .as_deref()
            .map(supports_code_context)
            .unwrap_or(true);

        if model_supported {
            if let Some(project_context) = session
                .project_context
                .as_ref()
                .filter(|v| !v.trim().is_empty())
            {
                let context_budget =
                    MAX_PROJECT_CONTEXT_TOKENS.min(MAX_CONTEXT_TOKENS.saturating_sub(tokens));
                if context_budget > 0 {
                    let capped_context = if estimate_tokens(project_context) > context_budget {
                        truncate_to_token_budget(project_context, context_budget)
                    } else {
                        project_context.clone()
                    };
                    let prelude = format!(
                        "You are a senior software engineer.\n\nThis conversation is about the following project:\n{}\n",
                        capped_context
                    );
                    tokens += estimate_tokens(&prelude);
                    system_chunks.push(prelude);
                }
            }
        }

        if let Some(summary) = session.summary.as_ref().filter(|v| !v.trim().is_empty()) {
            let summary_block = format!("Conversation summary:\n{}\n", summary);
            tokens += estimate_tokens(&summary_block);
            system_chunks.push(summary_block);
        }

        let base_index = session.summarized_messages.min(session.messages.len());
        let mut recent_chunks = Vec::new();
        for msg in session.messages.iter().skip(base_index).rev() {
            let text = format!("{}: {}\n", role_label(&msg.role), msg.content);
            let t = estimate_tokens(&text);
            if tokens + t > MAX_CONTEXT_TOKENS {
                break;
            }
            tokens += t;
            recent_chunks.push(text);
        }

        recent_chunks.reverse();

        Ok((system_chunks.join("\n"), recent_chunks.join("")))
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

        if estimate_session_tokens(&session) <= SUMMARY_TRIGGER_TOKENS {
            if context_debug_enabled() {
                eprintln!(
                    "[context] session={} summarize=no estimated_tokens={} trigger={}",
                    session_id,
                    estimate_session_tokens(&session),
                    SUMMARY_TRIGGER_TOKENS
                );
            }
            return;
        }

        let end_index = session.messages.len().saturating_sub(KEEP_LAST_MESSAGES);
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
            ..PromptOptions::default()
        };

        let new_summary = match self.router.send(provider, summary_prompt, options).await {
            Ok(response) => response.text.trim().to_string(),
            Err(err) => {
                if context_debug_enabled() {
                    eprintln!(
                        "[context] session={} summarize=error error={}",
                        session_id, err
                    );
                }
                return;
            }
        };
        if new_summary.is_empty() {
            if context_debug_enabled() {
                eprintln!("[context] session={} summarize=empty", session_id);
            }
            return;
        }

        let capped_summary = if estimate_tokens(&new_summary) > MAX_SUMMARY_TOKENS {
            truncate_to_token_budget(&new_summary, MAX_SUMMARY_TOKENS)
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
        if context_debug_enabled() {
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

fn role_label(role: &str) -> &str {
    match role {
        "user" => "User",
        "assistant" => "Assistant",
        "system" => "System",
        other => other,
    }
}

fn estimate_tokens(s: &str) -> usize {
    s.len() / 4
}

fn estimate_session_tokens(session: &ChatSession) -> usize {
    let mut total = 0usize;
    if let Some(summary) = session.summary.as_ref() {
        total += estimate_tokens(summary);
    }
    let from = session.summarized_messages.min(session.messages.len());
    for msg in session.messages.iter().skip(from) {
        total += estimate_tokens(&msg.content);
    }
    total
}

fn truncate_to_token_budget(text: &str, max_tokens: usize) -> String {
    let max_bytes = max_tokens.saturating_mul(4);
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

async fn build_project_context_for_root(project_root: &str) -> Result<String, ChatRuntimeError> {
    let root = PathBuf::from(project_root);
    let root_for_task = root.clone();
    tokio::task::spawn_blocking(move || collect_project_snapshot(&root_for_task))
        .await
        .map_err(|e| ChatRuntimeError::Path(format!("project scan task failed: {}", e)))
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

fn collect_project_snapshot(root: &Path) -> Result<ProjectSnapshot, ChatRuntimeError> {
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

    candidate_paths.sort();

    let mut files = Vec::new();
    let mut total_bytes = 0usize;
    for path in candidate_paths {
        if files.len() >= MAX_PROJECT_FILES || total_bytes >= MAX_PROJECT_BYTES {
            break;
        }

        let content_bytes = match stdfs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let content = match String::from_utf8(content_bytes) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if total_bytes + content.len() > MAX_PROJECT_BYTES {
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
    matches!(name, ".git" | "node_modules" | "target" | "build")
}

fn is_allowed_project_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some(
            "rs" | "py"
                | "js"
                | "ts"
                | "tsx"
                | "java"
                | "kt"
                | "cpp"
                | "h"
                | "qml"
                | "json"
                | "toml"
                | "md"
        )
    )
}

fn code_fence_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => "rust",
        Some("py") => "python",
        Some("js") => "javascript",
        Some("ts") => "typescript",
        Some("tsx") => "tsx",
        Some("java") => "java",
        Some("kt") => "kotlin",
        Some("cpp") => "cpp",
        Some("h") => "c",
        Some("qml") => "qml",
        Some("json") => "json",
        Some("toml") => "toml",
        Some("md") => "markdown",
        _ => "text",
    }
}

fn context_debug_enabled() -> bool {
    std::env::var("MULTILINK_DEBUG_CONTEXT")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn supports_code_context(model: &str) -> bool {
    let lower = model.to_lowercase();
    lower.contains("coder")
        || lower.contains("qwen")
        || lower.contains("mistral")
        || lower.contains("llama3")
        || lower.contains("phind")
        || lower.contains("deepseek")
        || lower.contains("codellama")
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
    fs::write(&temp_path, data)
        .await
        .map_err(ChatRuntimeError::Io)?;
    fs::rename(&temp_path, path)
        .await
        .map_err(ChatRuntimeError::Io)
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
