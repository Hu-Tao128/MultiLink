pub mod chunker;
pub mod compress;
pub mod embeddings;
pub mod index;
pub mod retrieval;

use std::path::PathBuf;

pub use retrieval::RetrievalResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextEngineVersion {
    V1,
    V2,
}

impl Default for ContextEngineVersion {
    fn default() -> Self {
        Self::V1
    }
}

impl ContextEngineVersion {
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "v2" | "2" => Self::V2,
            _ => Self::V1,
        }
    }
}

#[async_trait::async_trait]
pub trait ContextEngine: Send + Sync {
    async fn retrieve(
        &self,
        project_context: &str,
        prompt: &str,
        token_budget: usize,
        model_hint: Option<&str>,
        config: &ContextRetrievalConfig,
    ) -> RetrievalResult;
}

#[derive(Clone)]
pub struct ContextRetrievalConfig {
    pub embeddings_enabled: bool,
    pub embed_model: String,
    pub ollama_base_url: String,
    pub top_k: usize,
    pub version: ContextEngineVersion,
}

impl Default for ContextRetrievalConfig {
    fn default() -> Self {
        Self {
            embeddings_enabled: true,
            embed_model: "embeddinggemma".to_string(),
            ollama_base_url: "http://127.0.0.1:11434".to_string(),
            top_k: 8,
            version: ContextEngineVersion::V1,
        }
    }
}

pub struct ContextEngineV1;

#[async_trait::async_trait]
impl ContextEngine for ContextEngineV1 {
    async fn retrieve(
        &self,
        project_context: &str,
        prompt: &str,
        token_budget: usize,
        model_hint: Option<&str>,
        config: &ContextRetrievalConfig,
    ) -> RetrievalResult {
        crate::context_retrieval::build_relevant_project_context(
            project_context,
            prompt,
            token_budget,
            model_hint,
            crate::context_retrieval::RetrievalConfig {
                embeddings_enabled: config.embeddings_enabled,
                embed_model: config.embed_model.clone(),
                ollama_base_url: config.ollama_base_url.clone(),
                top_k: config.top_k,
            },
        )
        .await
        .into()
    }
}

pub struct ContextEngineV2 {
    index_dir: PathBuf,
}

impl ContextEngineV2 {
    pub fn new(index_dir: PathBuf) -> Self {
        Self { index_dir }
    }
}

#[async_trait::async_trait]
impl ContextEngine for ContextEngineV2 {
    async fn retrieve(
        &self,
        project_context: &str,
        prompt: &str,
        token_budget: usize,
        model_hint: Option<&str>,
        config: &ContextRetrievalConfig,
    ) -> RetrievalResult {
        let indexed = crate::context_engine::index::load_or_build(project_context).await;

        let Some(indexed) = indexed else {
            return RetrievalResult {
                context: String::new(),
                selected_files: Vec::new(),
                used_tokens: 0,
                top_k: config.top_k,
                embedding_used: false,
                is_truncated: false,
                budget_used: 0,
            };
        };

        let result = crate::context_engine::retrieval::hybrid_retrieval(
            prompt,
            &indexed.chunks,
            token_budget,
            model_hint,
            &config.ollama_base_url,
            &config.embed_model,
            config.embeddings_enabled,
            Some(&indexed.index_dir),
        )
        .await;

        result
    }
}

impl From<crate::context_retrieval::RetrievalResult> for RetrievalResult {
    fn from(v1: crate::context_retrieval::RetrievalResult) -> Self {
        RetrievalResult {
            context: v1.context,
            selected_files: v1.selected_files,
            used_tokens: v1.used_tokens,
            top_k: v1.top_k,
            embedding_used: v1.embedding_used,
            is_truncated: false,
            budget_used: v1.used_tokens,
        }
    }
}
