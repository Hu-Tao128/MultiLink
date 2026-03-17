use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    pub timestamp_ms: u128,
    pub model: String,
    pub server: String,
    pub tokens_in: usize,
    pub tokens_out: usize,
    pub context_tokens: usize,
    pub top_k_applied: usize,
    pub latency_ms: u128,
    pub fallback_used: bool,
    pub retries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextRetrievalMetrics {
    pub session_id: String,
    pub engine: String,
    pub context_latency_ms: u64,
    pub embedding_latency_ms: u64,
    pub index_refresh_ms: Option<u64>,
    pub retrieval_hit_rate: f32,
    pub truncation_rate: f32,
    pub error_rate: f32,
    pub selected_files: usize,
    pub used_tokens: usize,
    pub budget_used: usize,
    pub embedding_used: bool,
    pub top_k: usize,
}

impl ContextRetrievalMetrics {
    pub fn new(engine: &str, session_id: &str) -> Self {
        Self {
            engine: engine.to_string(),
            session_id: session_id.to_string(),
            ..Default::default()
        }
    }

    pub fn emit(&self, json_logs: bool) {
        if json_logs {
            if let Ok(line) = serde_json::to_string(self) {
                eprintln!("[context.metrics] {}", line);
                return;
            }
        }

        eprintln!(
            "[context.metrics] session={} engine={} latency_ms={} embed_latency_ms={} hit_rate={:.2} truncation_rate={:.2} error_rate={:.2} selected={} tokens={}",
            self.session_id,
            self.engine,
            self.context_latency_ms,
            self.embedding_latency_ms,
            self.retrieval_hit_rate,
            self.truncation_rate,
            self.error_rate,
            self.selected_files,
            self.used_tokens,
        );
    }
}

impl ExecutionMetrics {
    pub fn now_timestamp_ms() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    }

    pub fn emit(&self, json_logs: bool) {
        if json_logs {
            if let Ok(line) = serde_json::to_string(self) {
                eprintln!("[metrics] {}", line);
                return;
            }
        }

        eprintln!(
            "[metrics] model={} server={} tokens_in={} tokens_out={} context_tokens={} top_k={} latency_ms={} fallback={} retries={}",
            self.model,
            self.server,
            self.tokens_in,
            self.tokens_out,
            self.context_tokens,
            self.top_k_applied,
            self.latency_ms,
            self.fallback_used,
            self.retries,
        );
    }
}
