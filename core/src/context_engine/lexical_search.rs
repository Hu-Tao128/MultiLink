use std::collections::{HashMap, HashSet};

use super::bm25::BM25Scorer;
use super::tokenizer::Tokenizer;

#[derive(Debug, Clone)]
pub struct LexicalIndex {
    inverted_index: HashMap<String, HashSet<String>>,
    documents: HashMap<String, Vec<String>>,
    scorer: BM25Scorer,
}

impl LexicalIndex {
    pub fn new() -> Self {
        Self {
            inverted_index: HashMap::new(),
            documents: HashMap::new(),
            scorer: BM25Scorer::new(),
        }
    }

    pub fn add_document(&mut self, doc_id: String, content: String) {
        let tokens = Tokenizer::tokenize(&content);

        for token in &tokens {
            self.inverted_index
                .entry(token.clone())
                .or_default()
                .insert(doc_id.clone());
        }

        self.documents.insert(doc_id.clone(), tokens);
        self.scorer.index_doc(&doc_id, &self.documents[&doc_id]);
    }

    pub fn search(&self, query: &str, top_k: usize) -> Vec<(String, f32)> {
        let query_tokens = Tokenizer::tokenize(query);

        if query_tokens.is_empty() {
            return Vec::new();
        }

        let candidate_docs: HashSet<&String> = query_tokens
            .iter()
            .filter_map(|t| self.inverted_index.get(t))
            .flatten()
            .collect();

        if candidate_docs.is_empty() {
            return Vec::new();
        }

        let candidate_map: HashMap<String, Vec<String>> = candidate_docs
            .iter()
            .filter_map(|doc_id| {
                let key = (*doc_id).clone();
                self.documents.get(&key).map(|tokens| (key, tokens.clone()))
            })
            .collect();

        let mut results = self.scorer.score_batch(&query_tokens, &candidate_map);

        results.truncate(top_k);
        results
    }

    pub fn get_document(&self, doc_id: &str) -> Option<&Vec<String>> {
        self.documents.get(doc_id)
    }

    pub fn document_count(&self) -> usize {
        self.documents.len()
    }
}

impl Default for LexicalIndex {
    fn default() -> Self {
        Self::new()
    }
}

pub struct LexicalSearch {
    index: LexicalIndex,
}

impl LexicalSearch {
    pub fn new() -> Self {
        Self {
            index: LexicalIndex::new(),
        }
    }

    pub fn index(&mut self, doc_id: String, content: String) {
        self.index.add_document(doc_id, content);
    }

    pub fn retrieve(&self, query: &str, top_k: usize) -> Vec<(String, f32)> {
        self.index.search(query, top_k)
    }
}

impl Default for LexicalSearch {
    fn default() -> Self {
        Self::new()
    }
}

pub fn lexical_search(query: &str, chunks: &[String], top_k: usize) -> Vec<(usize, f32)> {
    let mut search = LexicalSearch::new();

    for (i, chunk) in chunks.iter().enumerate() {
        search.index(format!("chunk_{}", i), chunk.clone());
    }

    search
        .retrieve(query, top_k)
        .into_iter()
        .map(|(id, score)| {
            let idx = id
                .strip_prefix("chunk_")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);
            (idx, score)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexical_index() {
        let mut index = LexicalIndex::new();

        index.add_document("doc1".to_string(), "hello world foo".to_string());
        index.add_document("doc2".to_string(), "hello rust bar".to_string());
        index.add_document("doc3".to_string(), "world baz qux".to_string());

        let results = index.search("hello", 10);

        assert!(!results.is_empty());
        let min_score = results.last().map(|(_, s)| *s).unwrap_or(0.0);
        assert!(results[0].1 >= min_score);
    }

    #[test]
    fn test_lexical_search() {
        let chunks = vec![
            "fn main() {}".to_string(),
            "struct Test {}".to_string(),
            "fn test_function() {}".to_string(),
        ];

        let results = lexical_search("function", &chunks, 2);

        assert!(!results.is_empty());
    }

    #[test]
    fn test_camel_case_split() {
        let tokens = Tokenizer::tokenize("myFunctionName");
        assert!(tokens.iter().any(|t| t.contains("function")));
    }

    #[test]
    fn test_snake_case_split() {
        let tokens = Tokenizer::tokenize("my_function_name");
        assert!(tokens.contains(&"function".to_string()));
    }
}
