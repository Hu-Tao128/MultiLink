use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::providers::ProviderId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionState {
    Idle,
    Sending,
    Streaming,
    Done,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub timestamp: u64,
}

impl ChatMessage {
    pub fn user(content: String) -> Self {
        Self {
            role: "user".to_string(),
            content,
            timestamp: now_unix(),
        }
    }

    pub fn assistant(content: String) -> Self {
        Self {
            role: "assistant".to_string(),
            content,
            timestamp: now_unix(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub provider: ProviderId,
    pub model: Option<String>,
    #[serde(default)]
    pub project_root: Option<String>,
    #[serde(default)]
    pub project_context: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub summarized_messages: usize,
    pub messages: Vec<ChatMessage>,
    pub state: SessionState,
}

impl ChatSession {
    pub fn new(provider: ProviderId, model: Option<String>) -> Self {
        Self {
            id: next_session_id(),
            provider,
            model,
            project_root: None,
            project_context: None,
            summary: None,
            summarized_messages: 0,
            messages: Vec::new(),
            state: SessionState::Idle,
        }
    }

    pub fn add_user_message(&mut self, text: String) {
        self.messages.push(ChatMessage::user(text));
    }

    pub fn add_assistant_message(&mut self, text: String) {
        self.messages.push(ChatMessage::assistant(text));
    }

    pub fn set_state(&mut self, next: SessionState) {
        self.state = next;
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn next_session_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let millis = now_unix_millis();
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("session-{}-{}", millis, seq)
}
