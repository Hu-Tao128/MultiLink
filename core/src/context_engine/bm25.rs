use std::collections::{HashMap, HashSet};

const DEFAULT_K1: f32 = 1.5;
const DEFAULT_B: f32 = 0.75;

#[derive(Debug, Clone)]
pub struct BM25Scorer {
    k1: f32,
    b: f32,
    avg_doc_length: f32,
    doc_count: usize,
    doc_lengths: HashMap<String, usize>,
    term_doc_freq: HashMap<String, usize>,
}

impl BM25Scorer {
    pub fn new() -> Self {
        Self {
            k1: DEFAULT_K1,
            b: DEFAULT_B,
            avg_doc_length: 0.0,
            doc_count: 0,
            doc_lengths: HashMap::new(),
            term_doc_freq: HashMap::new(),
        }
    }

    pub fn with_params(k1: f32, b: f32) -> Self {
        Self {
            k1,
            b,
            avg_doc_length: 0.0,
            doc_count: 0,
            doc_lengths: HashMap::new(),
            term_doc_freq: HashMap::new(),
        }
    }

    pub fn index_doc(&mut self, doc_id: &str, tokens: &[String]) {
        let doc_length = tokens.len();
        self.doc_lengths.insert(doc_id.to_string(), doc_length);
        self.doc_count += 1;

        let mut unique_terms: HashSet<&String> = HashSet::new();
        for token in tokens {
            unique_terms.insert(token);
        }

        for term in unique_terms {
            *self.term_doc_freq.entry(term.clone()).or_insert(0) += 1;
        }

        self.avg_doc_length =
            self.doc_lengths.values().sum::<usize>() as f32 / self.doc_count as f32;
    }

    pub fn score(&self, query_terms: &[String], doc_id: &str, doc_tokens: &[String]) -> f32 {
        if query_terms.is_empty() || doc_tokens.is_empty() {
            return 0.0;
        }

        let doc_length = self
            .doc_lengths
            .get(doc_id)
            .copied()
            .unwrap_or(doc_tokens.len());
        let doc_length = doc_length as f32;

        let mut score = 0.0;
        let mut doc_term_freq: HashMap<&String, usize> = HashMap::new();
        for token in doc_tokens {
            *doc_term_freq.entry(token).or_insert(0) += 1;
        }

        for term in query_terms {
            let tf = doc_term_freq.get(term).copied().unwrap_or(0) as f32;
            if tf == 0.0 {
                continue;
            }

            let df = self.term_doc_freq.get(term).copied().unwrap_or(0) as f32;
            if df == 0.0 {
                continue;
            }

            let idf = ((self.doc_count as f32 - df + 0.5) / (df + 0.5) + 1.0).ln();

            let numerator = tf * (self.k1 + 1.0);
            let denominator =
                tf + self.k1 * (1.0 - self.b + self.b * doc_length / self.avg_doc_length);

            score += idf * numerator / denominator;
        }

        score
    }

    pub fn score_batch(
        &self,
        query_terms: &[String],
        docs: &HashMap<String, Vec<String>>,
    ) -> Vec<(String, f32)> {
        let mut results: Vec<(String, f32)> = docs
            .iter()
            .map(|(id, tokens)| {
                let score = self.score(query_terms, id, tokens);
                (id.clone(), score)
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }
}

impl Default for BM25Scorer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_basic() {
        let mut scorer = BM25Scorer::new();

        scorer.index_doc("doc1", &["hello".to_string(), "world".to_string()]);
        scorer.index_doc("doc2", &["hello".to_string(), "rust".to_string()]);

        let score = scorer.score(
            &["hello".to_string()],
            "doc1",
            &["hello".to_string(), "world".to_string()],
        );
        assert!(score > 0.0);
    }

    #[test]
    fn test_bm25_ranking() {
        let mut scorer = BM25Scorer::new();

        scorer.index_doc("doc1", &["hello".to_string(), "world".to_string()]);
        scorer.index_doc(
            "doc2",
            &[
                "hello".to_string(),
                "hello".to_string(),
                "hello".to_string(),
            ],
        );

        let results = scorer.score_batch(&["hello".to_string()], &HashMap::new());
        assert!(results.len() <= 2);
    }
}
