#![allow(dead_code)]

use std::collections::HashMap;
use tower_lsp::lsp_types::Url;

pub struct DocumentCache {
    documents: HashMap<Url, String>,
}

impl DocumentCache {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
        }
    }

    pub fn put(&mut self, uri: Url, content: String) {
        self.documents.insert(uri, content);
    }

    pub fn get(&self, uri: &Url) -> Option<String> {
        self.documents.get(uri).cloned()
    }

    pub fn remove(&mut self, uri: &Url) {
        self.documents.remove(uri);
    }

    pub fn contains(&self, uri: &Url) -> bool {
        self.documents.contains_key(uri)
    }

    pub fn all(&self) -> Vec<(Url, String)> {
        self.documents
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

impl Default for DocumentCache {
    fn default() -> Self {
        Self::new()
    }
}
