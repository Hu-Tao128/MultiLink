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
    pub messages: Vec<ChatMessage>,
    pub state: SessionState,
}

impl ChatSession {
    pub fn new(provider: ProviderId, model: Option<String>) -> Self {
        Self {
            id: format!("session-{}", now_unix()),
            provider,
            model,
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
