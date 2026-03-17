#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextChunk {
    pub file: String,
    pub language: String,
    pub start_line: usize,
    pub symbol: String,
    pub content: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextQuery {
    pub prompt: String,
    pub top_k: usize,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContextMode {
    Local,
    Remote,
    #[default]
    Hybrid,
}

#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub context_mode: ContextMode,
    pub local_timeout: Duration,
    pub remote_timeout: Duration,
    pub fallback_enabled: bool,
    pub max_tokens_per_chunk: usize,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            context_mode: ContextMode::Hybrid,
            local_timeout: Duration::from_millis(500),
            remote_timeout: Duration::from_millis(2000),
            fallback_enabled: true,
            max_tokens_per_chunk: 512,
        }
    }
}

#[async_trait::async_trait]
pub trait ContextBridge: Send + Sync {
    async fn retrieve_for_symbol(&self, symbol: &str, top_k: usize) -> Vec<ContextChunk>;
    async fn retrieve_for_query(&self, query: &str, top_k: usize) -> Vec<ContextChunk>;
    async fn index_document(&self, uri: &str, content: &str, language: &str) -> Result<(), BridgeError>;
    fn config(&self) -> &BridgeConfig;
}

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("Context engine not available: {0}")]
    NotAvailable(String),
    
    #[error("Timeout waiting for context: {0}")]
    Timeout(String),
    
    #[error("Invalid response from context engine: {0}")]
    InvalidResponse(String),
    
    #[error("Network error: {0}")]
    Network(String),
}

pub struct NoOpContextBridge {
    config: BridgeConfig,
}

impl NoOpContextBridge {
    pub fn new() -> Self {
        Self {
            config: BridgeConfig::default(),
        }
    }

    pub fn with_config(config: BridgeConfig) -> Self {
        Self { config }
    }
}

impl Default for NoOpContextBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ContextBridge for NoOpContextBridge {
    async fn retrieve_for_symbol(&self, symbol: &str, top_k: usize) -> Vec<ContextChunk> {
        tracing::debug!("NoOpContextBridge: retrieve_for_symbol({}, {})", symbol, top_k);
        Vec::new()
    }

    async fn retrieve_for_query(&self, query: &str, top_k: usize) -> Vec<ContextChunk> {
        tracing::debug!("NoOpContextBridge: retrieve_for_query({}, {})", query, top_k);
        Vec::new()
    }

    async fn index_document(&self, uri: &str, content: &str, language: &str) -> Result<(), BridgeError> {
        tracing::debug!("NoOpContextBridge: index_document({}, {}, {})", uri, content.len(), language);
        Ok(())
    }

    fn config(&self) -> &BridgeConfig {
        &self.config
    }
}

pub struct BridgeState {
    bridge: Box<dyn ContextBridge>,
    mode: ContextMode,
}

impl BridgeState {
    pub fn new(bridge: Box<dyn ContextBridge>) -> Self {
        let mode = bridge.config().context_mode;
        Self { bridge, mode }
    }

    pub fn no_op() -> Self {
        Self {
            bridge: Box::new(NoOpContextBridge::new()),
            mode: ContextMode::Hybrid,
        }
    }

    pub async fn get_context_for_symbol(&self, symbol: &str) -> Vec<ContextChunk> {
        match self.mode {
            ContextMode::Local | ContextMode::Hybrid => {
                let result = tokio::time::timeout(
                    self.bridge.config().local_timeout,
                    self.bridge.retrieve_for_symbol(symbol, 5)
                ).await;

                match result {
                    Ok(chunks) => return chunks,
                    Err(_) => {
                        tracing::warn!("Local context lookup timed out for symbol: {}", symbol);
                    }
                }
            }
            ContextMode::Remote => {}
        }

        if self.bridge.config().fallback_enabled {
            let result = tokio::time::timeout(
                self.bridge.config().remote_timeout,
                self.bridge.retrieve_for_symbol(symbol, 3)
            ).await;

            match result {
                Ok(chunks) => return chunks,
                Err(_) => {
                    tracing::error!("Remote context lookup timed out for symbol: {}", symbol);
                }
            }
        }

        Vec::new()
    }

    pub async fn get_context_for_query(&self, query: &str) -> Vec<ContextChunk> {
        match self.mode {
            ContextMode::Local | ContextMode::Hybrid => {
                let result = tokio::time::timeout(
                    self.bridge.config().local_timeout,
                    self.bridge.retrieve_for_query(query, 8)
                ).await;

                match result {
                    Ok(chunks) => return chunks,
                    Err(_) => {
                        tracing::warn!("Local context lookup timed out for query: {}", query);
                    }
                }
            }
            ContextMode::Remote => {}
        }

        if self.bridge.config().fallback_enabled {
            let result = tokio::time::timeout(
                self.bridge.config().remote_timeout,
                self.bridge.retrieve_for_query(query, 5)
            ).await;

            match result {
                Ok(chunks) => return chunks,
                Err(_) => {
                    tracing::error!("Remote context lookup timed out for query: {}", query);
                }
            }
        }

        Vec::new()
    }

    pub async fn index_document(&self, uri: &str, content: &str, language: &str) {
        if let Err(e) = self.bridge.index_document(uri, content, language).await {
            tracing::warn!("Failed to index document {}: {}", uri, e);
        }
    }

    pub fn mode(&self) -> ContextMode {
        self.mode
    }
}

impl Default for BridgeState {
    fn default() -> Self {
        Self::no_op()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_no_op_bridge_returns_empty() {
        let bridge = NoOpContextBridge::new();
        let chunks = bridge.retrieve_for_symbol("test", 5).await;
        assert!(chunks.is_empty());
    }

    #[tokio::test]
    async fn test_bridge_state_no_op_mode() {
        let state = BridgeState::no_op();
        let chunks = state.get_context_for_symbol("test").await;
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_default_config() {
        let config = BridgeConfig::default();
        assert_eq!(config.context_mode, ContextMode::Hybrid);
        assert!(config.fallback_enabled);
    }
}
