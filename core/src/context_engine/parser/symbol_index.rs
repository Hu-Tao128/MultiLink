use std::collections::HashMap;

use super::chunk_extractor::CodeChunk;

#[derive(Debug, Clone, Default)]
pub struct SymbolIndex {
    symbol_to_chunks: HashMap<String, Vec<ChunkId>>,
    file_to_chunks: HashMap<String, Vec<ChunkId>>,
    chunks: HashMap<ChunkId, CodeChunk>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChunkId {
    pub file: String,
    pub start_line: usize,
    pub symbol: String,
}

impl ChunkId {
    pub fn new(file: &str, start_line: usize, symbol: &str) -> Self {
        Self {
            file: file.to_string(),
            start_line,
            symbol: symbol.to_string(),
        }
    }

    pub fn from_string(id: &str) -> Option<Self> {
        let parts: Vec<&str> = id.splitn(3, ':').collect();
        if parts.len() >= 3 {
            Some(Self {
                file: parts[0].to_string(),
                start_line: parts[1].parse().unwrap_or(1),
                symbol: parts[2].to_string(),
            })
        } else {
            None
        }
    }
}

impl SymbolIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_chunk(&mut self, chunk: CodeChunk) {
        let id = ChunkId::new(
            &chunk.file,
            chunk.start_line,
            chunk.symbol.as_deref().unwrap_or("unnamed"),
        );

        self.chunks.insert(id.clone(), chunk.clone());

        if let Some(ref symbol) = chunk.symbol {
            self.symbol_to_chunks
                .entry(symbol.clone())
                .or_default()
                .push(id.clone());
        }

        self.file_to_chunks
            .entry(chunk.file.clone())
            .or_default()
            .push(id);
    }

    pub fn add_chunks(&mut self, chunks: Vec<CodeChunk>) {
        for chunk in chunks {
            self.add_chunk(chunk);
        }
    }

    pub fn lookup_symbol(&self, symbol: &str) -> Vec<&CodeChunk> {
        self.symbol_to_chunks
            .get(symbol)
            .map(|ids| ids.iter().filter_map(|id| self.chunks.get(id)).collect())
            .unwrap_or_default()
    }

    pub fn lookup_file(&self, file: &str) -> Vec<&CodeChunk> {
        self.file_to_chunks
            .get(file)
            .map(|ids| ids.iter().filter_map(|id| self.chunks.get(id)).collect())
            .unwrap_or_default()
    }

    pub fn get_chunk(&self, id: &ChunkId) -> Option<&CodeChunk> {
        self.chunks.get(id)
    }

    pub fn all_chunks(&self) -> Vec<&CodeChunk> {
        self.chunks.values().collect()
    }

    pub fn clear(&mut self) {
        self.symbol_to_chunks.clear();
        self.file_to_chunks.clear();
        self.chunks.clear();
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::parser::tree_sitter_parser::SourceLanguage;

    #[test]
    fn test_symbol_index() {
        let mut index = SymbolIndex::new();

        let chunk = CodeChunk {
            id: "test.rs:1:main".to_string(),
            file: "test.rs".to_string(),
            symbol: Some("main".to_string()),
            start_line: 1,
            end_line: 3,
            text: "fn main() {}".to_string(),
            language: SourceLanguage::Rust,
            chunk_type: crate::context_engine::parser::chunk_extractor::ChunkType::Function,
        };

        index.add_chunk(chunk.clone());

        let found = index.lookup_symbol("main");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file, "test.rs");

        let by_file = index.lookup_file("test.rs");
        assert_eq!(by_file.len(), 1);
    }
}
