pub mod bm25;
pub mod chunker;
pub mod compress;
pub mod embeddings;
pub mod embedding_index;
pub mod index;
pub mod lexical_search;
pub mod parser;
pub mod project_context;
pub mod retrieval;
pub mod scoring;
pub mod tokenizer;

use std::path::PathBuf;
use std::str::FromStr;

pub use retrieval::RetrievalResult;

use crate::config::detect_ollama_base_url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContextEngineVersion {
    #[default]
    V1,
    V2,
    V2Plus,
}

impl FromStr for ContextEngineVersion {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_lowercase().as_str() {
            "v2plus" | "v2+" => Self::V2Plus,
            "v2" | "2" => Self::V2,
            _ => Self::V1,
        })
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
    pub embed_base_url: String,
    pub ollama_base_url: String,
    pub embed_connect_timeout_ms: u64,
    pub embed_request_timeout_ms: u64,
    pub embed_max_retries: u8,
    pub embed_batch_size: usize,
    pub top_k: usize,
    pub index_refresh_on_query: bool,
    pub retrieval_enable_filters: bool,
    pub version: ContextEngineVersion,
}

impl Default for ContextRetrievalConfig {
    fn default() -> Self {
        let ollama_url = detect_ollama_base_url();
        Self {
            embeddings_enabled: true,
            embed_model: String::new(),
            embed_base_url: ollama_url.clone(),
            ollama_base_url: ollama_url,
            embed_connect_timeout_ms: 2_000,
            embed_request_timeout_ms: 12_000,
            embed_max_retries: 1,
            embed_batch_size: 24,
            top_k: 8,
            index_refresh_on_query: true,
            retrieval_enable_filters: false,
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
                embed_base_url: config.embed_base_url.clone(),
                ollama_base_url: config.ollama_base_url.clone(),
                embed_connect_timeout_ms: config.embed_connect_timeout_ms,
                embed_request_timeout_ms: config.embed_request_timeout_ms,
                embed_max_retries: config.embed_max_retries,
                embed_batch_size: config.embed_batch_size,
                top_k: config.top_k,
            },
        )
        .await
        .into()
    }
}

pub struct ContextEngineV2;

impl ContextEngineV2 {
    pub fn new(_index_dir: PathBuf) -> Self {
        Self
    }
}

pub struct ContextEngineV2Plus;

impl ContextEngineV2Plus {
    pub fn new(_index_dir: PathBuf) -> Self {
        Self
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
                embedding_reason: "index_build_failed".to_string(),
                embedding_latency_ms: 0,
                embedding_attempts: 0,
                embed_base_url: config.embed_base_url.clone(),
                embed_model: config.embed_model.clone(),
                is_truncated: false,
                budget_used: 0,
            };
        };

        let result = crate::context_engine::retrieval::hybrid_retrieval(
            prompt,
            &indexed.chunks,
            token_budget,
            model_hint,
            &config.embed_base_url,
            &config.embed_model,
            config.embeddings_enabled,
            config.embed_connect_timeout_ms,
            config.embed_request_timeout_ms,
            config.embed_max_retries,
            config.embed_batch_size,
            config.retrieval_enable_filters,
            Some(&indexed.index_dir),
        )
        .await;

        result
    }
}

#[async_trait::async_trait]
impl ContextEngine for ContextEngineV2Plus {
    async fn retrieve(
        &self,
        project_context: &str,
        prompt: &str,
        token_budget: usize,
        model_hint: Option<&str>,
        config: &ContextRetrievalConfig,
    ) -> RetrievalResult {
        if config.index_refresh_on_query {
            crate::context_engine::index::refresh_in_background(project_context.to_string());
        }

        let mut indexed = crate::context_engine::index::load_best_effort(project_context).await;
        if indexed.is_none() {
            indexed = crate::context_engine::index::load_or_build(project_context).await;
        }

        let Some(indexed) = indexed else {
            return RetrievalResult {
                context: String::new(),
                selected_files: Vec::new(),
                used_tokens: 0,
                top_k: config.top_k,
                embedding_used: false,
                embedding_reason: "index_build_failed".to_string(),
                embedding_latency_ms: 0,
                embedding_attempts: 0,
                embed_base_url: config.embed_base_url.clone(),
                embed_model: config.embed_model.clone(),
                is_truncated: false,
                budget_used: 0,
            };
        };

        crate::context_engine::retrieval::hybrid_retrieval(
            prompt,
            &indexed.chunks,
            token_budget,
            model_hint,
            &config.embed_base_url,
            &config.embed_model,
            config.embeddings_enabled,
            config.embed_connect_timeout_ms,
            config.embed_request_timeout_ms,
            config.embed_max_retries,
            config.embed_batch_size,
            config.retrieval_enable_filters,
            Some(&indexed.index_dir),
        )
        .await
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
            embedding_reason: v1.embedding_diag.reason,
            embedding_latency_ms: v1.embedding_diag.latency_ms,
            embedding_attempts: v1.embedding_diag.attempts,
            embed_base_url: v1.embedding_diag.base_url,
            embed_model: v1.embedding_diag.model,
            is_truncated: false,
            budget_used: v1.used_tokens,
        }
    }
}
