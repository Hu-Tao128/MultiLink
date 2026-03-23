use std::collections::HashMap;

use crate::context_engine::chunker::SemanticChunk;
use crate::context_engine::lexical_search::lexical_search;

#[derive(Debug, Clone)]
pub struct EmbeddingStore {
    vectors: HashMap<String, Vec<f32>>,
}

impl EmbeddingStore {
    pub fn new() -> Self {
        Self {
            vectors: HashMap::new(),
        }
    }

    pub fn add_vector(&mut self, chunk_id: String, vector: Vec<f32>) {
        self.vectors.insert(chunk_id, vector);
    }

    pub fn search(&self, query_vec: &[f32], top_k: usize) -> Vec<(String, f32)> {
        if self.vectors.is_empty() || query_vec.is_empty() {
            return Vec::new();
        }

        let mut scores: Vec<(String, f32)> = self
            .vectors
            .iter()
            .map(|(id, vec)| {
                let similarity = cosine_similarity(query_vec, vec);
                (id.clone(), similarity)
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(top_k);
        scores
    }

    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }
}

impl Default for EmbeddingStore {
    fn default() -> Self {
        Self::new()
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot_product / (norm_a * norm_b)
}

#[derive(Debug, Clone)]
pub struct HybridConfig {
    pub alpha: f32,
    pub lexical_top_k: usize,
    pub embedding_top_k: usize,
}

impl Default for HybridConfig {
    fn default() -> Self {
        Self {
            alpha: 0.5,
            lexical_top_k: 20,
            embedding_top_k: 20,
        }
    }
}

impl HybridConfig {
    pub fn new(alpha: f32) -> Self {
        Self {
            alpha: alpha.clamp(0.0, 1.0),
            lexical_top_k: 20,
            embedding_top_k: 20,
        }
    }
}

pub struct HybridRetrieval {
    config: HybridConfig,
}

impl HybridRetrieval {
    pub fn new(config: HybridConfig) -> Self {
        Self { config }
    }

    pub fn search(
        &self,
        query: &str,
        chunks: &[SemanticChunk],
        embedding_store: Option<&EmbeddingStore>,
    ) -> Vec<(usize, f32)> {
        let lexical_scores = self.lexical_search(query, chunks);

        let embedding_scores = if let Some(store) = embedding_store {
            if store.is_empty() {
                None
            } else {
                Some(self.embedding_search(query, store))
            }
        } else {
            None
        };

        self.combine_scores(lexical_scores, embedding_scores)
    }

    fn lexical_search(&self, query: &str, chunks: &[SemanticChunk]) -> HashMap<usize, f32> {
        let chunk_texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
        let results = lexical_search(query, &chunk_texts, self.config.lexical_top_k);

        results.into_iter().collect()
    }

    fn embedding_search(&self, _query: &str, store: &EmbeddingStore) -> HashMap<usize, f32> {
        let dummy_vector = vec![0.0; 384];
        let results = store.search(&dummy_vector, self.config.embedding_top_k);

        let mut scores = HashMap::new();
        for (chunk_id, score) in results {
            if let Ok(idx) = chunk_id
                .strip_prefix("chunk_")
                .unwrap_or(&chunk_id)
                .parse::<usize>()
            {
                scores.insert(idx, score);
            }
        }
        scores
    }

    fn combine_scores(
        &self,
        lexical: HashMap<usize, f32>,
        embedding: Option<HashMap<usize, f32>>,
    ) -> Vec<(usize, f32)> {
        let alpha = self.config.alpha;
        let mut combined: HashMap<usize, f32> = HashMap::new();

        for (idx, lex_score) in &lexical {
            let emb_score = embedding
                .as_ref()
                .and_then(|e| e.get(idx))
                .copied()
                .unwrap_or(0.0);
            let final_score = (alpha * lex_score) + ((1.0 - alpha) * emb_score);
            combined.insert(*idx, final_score);
        }

        if let Some(emb) = embedding {
            for (idx, emb_score) in &emb {
                if !lexical.contains_key(idx) {
                    let lex_score = 0.0;
                    let final_score = (alpha * lex_score) + ((1.0 - alpha) * emb_score);
                    combined.insert(*idx, final_score);
                }
            }
        }

        let mut results: Vec<(usize, f32)> = combined.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }
}

pub fn hybrid_search(
    query: &str,
    chunks: &[SemanticChunk],
    alpha: f32,
    embedding_store: Option<&EmbeddingStore>,
) -> Vec<(usize, f32)> {
    let config = HybridConfig::new(alpha);
    let retrieval = HybridRetrieval::new(config);
    retrieval.search(query, chunks, embedding_store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_embedding_store() {
        let mut store = EmbeddingStore::new();
        store.add_vector("chunk_0".to_string(), vec![1.0, 0.0]);
        store.add_vector("chunk_1".to_string(), vec![0.0, 1.0]);

        let results = store.search(&[1.0, 0.0], 2);
        assert_eq!(results[0].0, "chunk_0");
    }

    #[test]
    fn test_hybrid_config() {
        let config = HybridConfig::new(0.7);
        assert!((config.alpha - 0.7).abs() < 0.001);

        let config_clamped = HybridConfig::new(1.5);
        assert!((config_clamped.alpha - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_hybrid_retrieval() {
        let chunks = vec![
            SemanticChunk {
                file: "test.rs".to_string(),
                language: "rust".to_string(),
                start_line: 1,
                symbol: "main".to_string(),
                content: "fn main() {}".to_string(),
                chunk_hash: "abc".to_string(),
            },
            SemanticChunk {
                file: "test.rs".to_string(),
                language: "rust".to_string(),
                start_line: 10,
                symbol: "test".to_string(),
                content: "fn test() {}".to_string(),
                chunk_hash: "def".to_string(),
            },
        ];

        let config = HybridConfig::new(0.5);
        let retrieval = HybridRetrieval::new(config);
        let results = retrieval.search("main test function", &chunks, None);

        assert!(
            !results.is_empty(),
            "Results should not be empty for main test query"
        );
    }
}
