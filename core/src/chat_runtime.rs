use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use directories::ProjectDirs;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::sync::{mpsc, watch, Mutex, RwLock};

use crate::providers::{PromptOptions, ProviderId};
use crate::router::ProviderRouter;
use crate::session::{ChatSession, SessionState};

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
    pub fn new(router: Arc<ProviderRouter>, storage_dir: PathBuf, persist_interval: Duration) -> Self {
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
        let _ = persist_session(&self.storage_dir, &session).await;
        {
            let mut active_guard = self.active_session_id.write().await;
            *active_guard = Some(session_id.clone());
        }
        let _ = persist_index_and_state(
            &self.storage_dir,
            self.sessions.read().await.keys().cloned().collect(),
            Some(session_id.clone()),
        )
        .await;
        session_id
    }

    pub async fn list_sessions(&self) -> Vec<ChatSession> {
        let mut sessions: Vec<ChatSession> = self.sessions.read().await.values().cloned().collect();
        sessions.sort_by(|a, b| b.id.cmp(&a.id));
        sessions
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

    pub async fn send_message(
        &self,
        session_id: &str,
        prompt: String,
    ) -> Result<mpsc::Receiver<StreamEvent>, ChatRuntimeError> {
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

        let options = PromptOptions {
            model,
            ..PromptOptions::default()
        };

        let stream = self
            .router
            .stream_send(provider, prompt, options)
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
        let persist_interval = self.persist_interval;
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
            let mut pending_chunk = String::new();
            let mut last_persist = Instant::now();

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
                                pending_chunk.push_str(&token);

                                if last_persist.elapsed() >= persist_interval {
                                    if !pending_chunk.is_empty() {
                                        let out = pending_chunk.clone();
                                        pending_chunk.clear();
                                        let _ = event_tx.send(StreamEvent::Chunk(out)).await;
                                    }

                                    let _ = persist_partial(&sessions, &storage_dir, &session_id_owned, &full_output).await;
                                    last_persist = Instant::now();
                                }
                            }
                            Some(Ok(crate::providers::TokenEvent::Completed)) => {
                                if !pending_chunk.is_empty() {
                                    let out = pending_chunk.clone();
                                    pending_chunk.clear();
                                    let _ = event_tx.send(StreamEvent::Chunk(out)).await;
                                }
                                let _ = finalize_success(&sessions, &storage_dir, &session_id_owned, &full_output).await;
                                let _ = event_tx.send(StreamEvent::Finished).await;
                                break;
                            }
                            Some(Ok(crate::providers::TokenEvent::Started)) => {}
                            Some(Err(err)) => {
                                let message = err.to_string();
                                if !pending_chunk.is_empty() {
                                    let out = pending_chunk.clone();
                                    pending_chunk.clear();
                                    let _ = event_tx.send(StreamEvent::Chunk(out)).await;
                                }
                                let _ = event_tx.send(StreamEvent::Error(message.clone())).await;
                                let _ = finalize_error(&sessions, &storage_dir, &session_id_owned, &message).await;
                                break;
                            }
                            None => {
                                if !pending_chunk.is_empty() {
                                    let out = pending_chunk.clone();
                                    pending_chunk.clear();
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
            let raw = fs::read_to_string(&index_path).await.map_err(ChatRuntimeError::Io)?;
            let index: SessionIndex =
                serde_json::from_str(&raw).map_err(|e| ChatRuntimeError::Serialization(e.to_string()))?;
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
            let data = fs::read_to_string(&path).await.map_err(ChatRuntimeError::Io)?;
            let session: ChatSession =
                serde_json::from_str(&data).map_err(|e| ChatRuntimeError::Serialization(e.to_string()))?;
            self.sessions.write().await.insert(session.id.clone(), session);
        }

        let state_path = self.storage_dir.join("state.json");
        if state_path.exists() {
            let raw = fs::read_to_string(&state_path).await.map_err(ChatRuntimeError::Io)?;
            let state: RuntimeState =
                serde_json::from_str(&raw).map_err(|e| ChatRuntimeError::Serialization(e.to_string()))?;
            *self.active_session_id.write().await = state.active_session_id;
        }

        Ok(())
    }
}

fn app_data_dir() -> Result<PathBuf, ChatRuntimeError> {
    let dirs = ProjectDirs::from("com", "multilink", "multilink")
        .ok_or_else(|| ChatRuntimeError::Path("cannot resolve project data directory".to_string()))?;
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

async fn persist_partial(
    sessions: &Arc<RwLock<HashMap<String, ChatSession>>>,
    storage_dir: &Path,
    session_id: &str,
    partial_text: &str,
) -> Result<(), ChatRuntimeError> {
    let snapshot = {
        let guard = sessions.read().await;
        guard
            .get(session_id)
            .cloned()
            .ok_or(ChatRuntimeError::SessionNotFound)?
    };

    let partial_path = storage_dir.join(format!("{}.partial.tmp", session_id));
    if !storage_dir.exists() {
        fs::create_dir_all(storage_dir).await.map_err(ChatRuntimeError::Io)?;
    }

    let payload = serde_json::json!({
        "session": snapshot,
        "partial": partial_text,
    });

    write_atomic_json(&partial_path, &payload).await
}

async fn persist_session(storage_dir: &Path, session: &ChatSession) -> Result<(), ChatRuntimeError> {
    if !storage_dir.exists() {
        fs::create_dir_all(storage_dir).await.map_err(ChatRuntimeError::Io)?;
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
        fs::create_dir_all(storage_dir).await.map_err(ChatRuntimeError::Io)?;
    }

    session_ids.sort();
    session_ids.dedup();

    let index = SessionIndex { session_ids };
    write_atomic_json(&storage_dir.join("index.json"), &index).await?;

    let state = RuntimeState { active_session_id };
    write_atomic_json(&storage_dir.join("state.json"), &state).await
}

async fn write_atomic_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), ChatRuntimeError> {
    let data = serde_json::to_vec_pretty(value)
        .map_err(|e| ChatRuntimeError::Serialization(e.to_string()))?;
    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, data).await.map_err(ChatRuntimeError::Io)?;
    fs::rename(&temp_path, path).await.map_err(ChatRuntimeError::Io)
}

async fn remove_partial_file(storage_dir: &Path, session_id: &str) {
    let partial_path = storage_dir.join(format!("{}.partial.tmp", session_id));
    if partial_path.exists() {
        let _ = fs::remove_file(partial_path).await;
    }
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
