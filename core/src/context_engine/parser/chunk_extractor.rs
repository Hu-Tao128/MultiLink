use tree_sitter::Node;

use super::tree_sitter_parser::{SourceLanguage, TreeSitterParser};

#[derive(Debug, Clone)]
pub struct CodeChunk {
    pub id: String,
    pub file: String,
    pub symbol: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
    pub language: SourceLanguage,
    pub chunk_type: ChunkType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkType {
    Function,
    Struct,
    Impl,
    Module,
    Class,
    Method,
    Enum,
    Trait,
    Unknown,
}

pub struct ChunkExtractor {
    language: SourceLanguage,
}

impl ChunkExtractor {
    pub fn new(language: SourceLanguage) -> Self {
        Self { language }
    }

    pub fn extract_chunks(&self, source: &str, file_path: &str) -> Vec<CodeChunk> {
        let mut parser = match TreeSitterParser::new(self.language) {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };

        let tree = match parser.parse(source) {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };

        let mut chunks = Vec::new();
        self.extract_from_node(tree.root_node(), source, file_path, &mut chunks);
        chunks
    }

    fn extract_from_node(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        chunks: &mut Vec<CodeChunk>,
    ) {
        let node_kind = node.kind();

        if let Some(chunk_type) = self.get_chunk_type(node_kind) {
            let start = node.start_position();
            let end = node.end_position();
            let text = node
                .utf8_text(source.as_bytes())
                .unwrap_or_default()
                .to_string();
            let symbol = self.extract_symbol(node, source);

            chunks.push(CodeChunk {
                id: format!(
                    "{}:{}:{}",
                    file_path,
                    start.row + 1,
                    symbol.as_deref().unwrap_or("unnamed")
                ),
                file: file_path.to_string(),
                symbol,
                start_line: start.row + 1,
                end_line: end.row + 1,
                text,
                language: self.language,
                chunk_type,
            });
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_from_node(child, source, file_path, chunks);
        }
    }

    fn get_chunk_type(&self, kind: &str) -> Option<ChunkType> {
        match self.language {
            SourceLanguage::Rust => match kind {
                "function_item" | "function_declaration" => Some(ChunkType::Function),
                "struct_item" | "struct_declaration" => Some(ChunkType::Struct),
                "impl_item" => Some(ChunkType::Impl),
                "mod_item" => Some(ChunkType::Module),
                "enum_item" => Some(ChunkType::Enum),
                "trait_item" => Some(ChunkType::Trait),
                _ => None,
            },
            SourceLanguage::Python => match kind {
                "function_definition" => Some(ChunkType::Function),
                "class_definition" => Some(ChunkType::Class),
                _ => None,
            },
            SourceLanguage::JavaScript | SourceLanguage::TypeScript => match kind {
                "function_declaration" | "arrow_function" | "function_expression" => {
                    Some(ChunkType::Function)
                }
                "class_declaration" => Some(ChunkType::Class),
                "method_definition" => Some(ChunkType::Method),
                _ => None,
            },
            SourceLanguage::Unknown => None,
        }
    }

    fn extract_symbol(&self, node: Node, source: &str) -> Option<String> {
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            let kind = child.kind();
            match kind {
                "identifier" | "type_identifier" | "attribute_identifier" => {
                    return child
                        .utf8_text(source.as_bytes())
                        .ok()
                        .map(|s| s.to_string());
                }
                _ => {}
            }
        }

        None
    }
}

pub fn extract_semantic_chunks(source: &str, file_path: &str) -> Vec<CodeChunk> {
    let language = SourceLanguage::from_filename(file_path);
    if language == SourceLanguage::Unknown {
        return Vec::new();
    }

    let extractor = ChunkExtractor::new(language);
    extractor.extract_chunks(source, file_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rust_chunks() {
        let source = r#"
fn main() {
    println!("Hello");
}

struct MyStruct {
    field: i32,
}

impl MyStruct {
    fn new() -> Self {
        Self { field: 0 }
    }
}
"#;
        let chunks = extract_semantic_chunks(source, "test.rs");

        assert!(chunks.len() >= 3);

        let has_function = chunks.iter().any(|c| c.chunk_type == ChunkType::Function);
        let has_struct = chunks.iter().any(|c| c.chunk_type == ChunkType::Struct);
        let has_impl = chunks.iter().any(|c| c.chunk_type == ChunkType::Impl);

        assert!(has_function, "Should have a function");
        assert!(has_struct, "Should have a struct");
        assert!(has_impl, "Should have an impl");
    }

    #[test]
    fn test_extract_python_chunks() {
        let source = r#"
def main():
    print("Hello")

class MyClass:
    def __init__(self):
        self.field = 0

def another_function():
    pass
"#;
        let chunks = extract_semantic_chunks(source, "test.py");

        let has_class = chunks.iter().any(|c| c.chunk_type == ChunkType::Class);
        let has_functions = chunks
            .iter()
            .filter(|c| c.chunk_type == ChunkType::Function)
            .count();

        assert!(has_class, "Should have a class");
        assert!(has_functions >= 2, "Should have at least 2 functions");
    }
}
