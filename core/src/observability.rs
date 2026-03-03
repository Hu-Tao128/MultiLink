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
