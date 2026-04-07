use std::collections::HashMap;

pub trait EmbeddingIndex: Send + Sync {
    fn search(&self, query: &[f32], top_k: usize) -> Vec<(String, f32)>;
    fn add(&mut self, id: String, vector: Vec<f32>);
    fn is_empty(&self) -> bool;
}

pub struct LinearIndex {
    vectors: HashMap<String, Vec<f32>>,
}

impl LinearIndex {
    pub fn new() -> Self {
        Self {
            vectors: HashMap::new(),
        }
    }

    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.is_empty() || b.is_empty() || a.len() != b.len() {
            return 0.0;
        }
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if na == 0.0 || nb == 0.0 {
            return 0.0;
        }
        dot / (na * nb)
    }
}

impl Default for LinearIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl EmbeddingIndex for LinearIndex {
    fn search(&self, query: &[f32], top_k: usize) -> Vec<(String, f32)> {
        let mut scores: Vec<(String, f32)> = self
            .vectors
            .iter()
            .map(|(id, vec)| (id.clone(), Self::cosine_similarity(query, vec)))
            .collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(top_k);
        scores
    }

    fn add(&mut self, id: String, vector: Vec<f32>) {
        self.vectors.insert(id, vector);
    }

    fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }
}
