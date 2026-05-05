use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

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

pub struct RealContextBridge {
    config: BridgeConfig,
    engine: Arc<dyn multilink_core::ContextEngine>,
    project_root: String,
    symbol_index: Arc<Mutex<multilink_core::context_engine::parser::symbol_index::SymbolIndex>>,
}

impl RealContextBridge {
    pub fn new(
        engine: Arc<dyn multilink_core::ContextEngine>,
        project_root: String,
    ) -> Self {
        Self {
            config: BridgeConfig::default(),
            engine,
            project_root,
            symbol_index: Arc::new(Mutex::new(
                multilink_core::context_engine::parser::symbol_index::SymbolIndex::new(),
            )),
        }
    }

    pub fn with_config(
        engine: Arc<dyn multilink_core::ContextEngine>,
        project_root: String,
        config: BridgeConfig,
    ) -> Self {
        Self {
            config,
            engine,
            project_root,
            symbol_index: Arc::new(Mutex::new(
                multilink_core::context_engine::parser::symbol_index::SymbolIndex::new(),
            )),
        }
    }

    fn core_language_from_str(
        lang: &str,
    ) -> multilink_core::context_engine::parser::tree_sitter_parser::SourceLanguage {
        match lang.to_lowercase().as_str() {
            "rust" => multilink_core::context_engine::parser::tree_sitter_parser::SourceLanguage::Rust,
            "python" => multilink_core::context_engine::parser::tree_sitter_parser::SourceLanguage::Python,
            "javascript" => {
                multilink_core::context_engine::parser::tree_sitter_parser::SourceLanguage::JavaScript
            }
            "typescript" => {
                multilink_core::context_engine::parser::tree_sitter_parser::SourceLanguage::TypeScript
            }
            _ => multilink_core::context_engine::parser::tree_sitter_parser::SourceLanguage::Unknown,
        }
    }

    fn lang_from_uri(uri: &str) -> &'static str {
        let ext = uri.rsplit('.').next().unwrap_or("");
        match ext {
            "rs" => "rust",
            "py" => "python",
            "js" | "jsx" => "javascript",
            "ts" | "tsx" => "typescript",
            _ => "text",
        }
    }
}

#[async_trait::async_trait]
impl ContextBridge for RealContextBridge {
    async fn retrieve_for_symbol(&self, symbol: &str, top_k: usize) -> Vec<ContextChunk> {
        let idx = self.symbol_index.lock().await;
        let chunks = idx.lookup_symbol(symbol);
        if chunks.is_empty() {
            return Vec::new();
        }

        let mut results: Vec<ContextChunk> = chunks
            .iter()
            .take(top_k)
            .map(|c| ContextChunk {
                file: c.file.clone(),
                language: c.language.to_string(),
                start_line: c.start_line,
                symbol: c.symbol.clone().unwrap_or_default(),
                content: c.text.chars().take(512).collect(),
                score: 1.0,
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    async fn retrieve_for_query(&self, query: &str, top_k: usize) -> Vec<ContextChunk> {
        let config = multilink_core::ContextRetrievalConfig::default();
        let result = self
            .engine
            .retrieve(&self.project_root, query, 4096, None, &config)
            .await;

        let mut chunks: Vec<ContextChunk> = result
            .selected_files
            .iter()
            .enumerate()
            .take(top_k)
            .map(|(i, file_path)| ContextChunk {
                file: file_path.clone(),
                language: file_path
                    .rsplit('.')
                    .next()
                    .unwrap_or("text")
                    .to_string(),
                start_line: 1,
                symbol: String::new(),
                content: file_path.clone(),
                score: 1.0 - (i as f32 * 0.01),
            })
            .collect();

        chunks.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        chunks
    }

    async fn index_document(&self, uri: &str, content: &str, language: &str) -> Result<(), BridgeError> {
        let core_lang =
            Self::core_language_from_str(language);
        if core_lang
            == multilink_core::context_engine::parser::tree_sitter_parser::SourceLanguage::Unknown
        {
            return Ok(());
        }

        let file_path = uri.strip_prefix("file://").unwrap_or(uri);

        let extractor =
            multilink_core::context_engine::parser::chunk_extractor::ChunkExtractor::new(
                core_lang,
            );
        let chunks = extractor.extract_chunks(content, file_path);

        if !chunks.is_empty() {
            let mut idx = self.symbol_index.lock().await;
            idx.add_chunks(chunks);
            tracing::debug!("Indexed document {}", uri);
        }

        Ok(())
    }

    fn config(&self) -> &BridgeConfig {
        &self.config
    }
}

pub struct BridgeState {
    bridge: Arc<dyn ContextBridge>,
    mode: ContextMode,
}

impl BridgeState {
    pub fn new(bridge: Arc<dyn ContextBridge>) -> Self {
        let mode = bridge.config().context_mode;
        Self { bridge, mode }
    }

    pub fn no_op() -> Self {
        Self {
            bridge: Arc::new(NoOpContextBridge::new()),
            mode: ContextMode::Hybrid,
        }
    }

    pub async fn get_context_for_symbol(&self, symbol: &str) -> Vec<ContextChunk> {
        match self.mode {
            ContextMode::Local | ContextMode::Hybrid => {
                let result = tokio::time::timeout(
                    self.bridge.config().local_timeout,
                    self.bridge.retrieve_for_symbol(symbol, 5),
                )
                .await;

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
                self.bridge.retrieve_for_symbol(symbol, 3),
            )
            .await;

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
                    self.bridge.retrieve_for_query(query, 8),
                )
                .await;

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
                self.bridge.retrieve_for_query(query, 5),
            )
            .await;

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

    #[test]
    fn test_lang_from_uri() {
        assert_eq!(RealContextBridge::lang_from_uri("file:///test.rs"), "rust");
        assert_eq!(RealContextBridge::lang_from_uri("file:///test.py"), "python");
        assert_eq!(RealContextBridge::lang_from_uri("file:///test.js"), "javascript");
        assert_eq!(RealContextBridge::lang_from_uri("file:///test.ts"), "typescript");
    }
}
