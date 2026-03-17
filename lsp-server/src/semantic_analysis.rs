#![allow(dead_code)]

use dashmap::DashMap;
use std::collections::HashMap;
use tree_sitter::Node;

use crate::ast_cache::SourceLanguage;

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
    pub container: Option<String>,
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Impl,
    Module,
    Class,
    Variable,
    Type,
    Constant,
}

impl SymbolKind {
    pub fn to_lsp_kind(&self) -> u32 {
        match self {
            SymbolKind::Function => 1,  // Function
            SymbolKind::Method => 2,    // Method
            SymbolKind::Struct => 5,    // Struct
            SymbolKind::Enum => 10,     // Enum
            SymbolKind::Trait => 7,     // Interface
            SymbolKind::Impl => 6,      // Class
            SymbolKind::Module => 2,    // Module
            SymbolKind::Class => 6,     // Class
            SymbolKind::Variable => 5,  // Variable
            SymbolKind::Type => 7,      // TypeParameter
            SymbolKind::Constant => 14, // Constant
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SemanticAnalysis {
    pub symbols: Vec<Symbol>,
    pub references: HashMap<String, Vec<SymbolReference>>,
}

#[derive(Debug, Clone)]
pub struct SymbolReference {
    pub name: String,
    pub file: String,
    pub line: usize,
    pub column: usize,
}

pub struct SemanticAnalyzer {
    language: SourceLanguage,
}

impl SemanticAnalyzer {
    pub fn new(language: SourceLanguage) -> Self {
        Self { language }
    }

    pub fn analyze(&self, source: &str, root_node: &Node, file_path: &str) -> SemanticAnalysis {
        let mut symbols = Vec::new();
        let mut references = HashMap::new();
        let mut current_container: Option<String> = None;

        self.walk_node(
            root_node,
            source,
            file_path,
            &mut symbols,
            &mut references,
            &mut current_container,
        );

        SemanticAnalysis {
            symbols,
            references,
        }
    }

    fn walk_node(
        &self,
        node: &Node,
        source: &str,
        file_path: &str,
        symbols: &mut Vec<Symbol>,
        references: &mut HashMap<String, Vec<SymbolReference>>,
        container: &mut Option<String>,
    ) {
        let kind = node.kind();

        if let Some(symbol) = self.extract_symbol(node, source, file_path, container) {
            if let Some(ref name) = symbol.container {
                *container = Some(name.clone());
            }
            symbols.push(symbol);
        }

        if self.is_reference(kind) {
            if let Some(name) = self.get_identifier_name(node, source) {
                references
                    .entry(name.clone())
                    .or_default()
                    .push(SymbolReference {
                        name,
                        file: file_path.to_string(),
                        line: node.start_position().row + 1,
                        column: node.start_position().column,
                    });
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_node(&child, source, file_path, symbols, references, container);
        }

        if self.ends_container(kind) {
            *container = None;
        }
    }

    fn extract_symbol(
        &self,
        node: &Node,
        source: &str,
        file_path: &str,
        container: &Option<String>,
    ) -> Option<Symbol> {
        let kind = node.kind();

        let (symbol_kind, name) = match self.language {
            SourceLanguage::Rust => self.rust_symbol_info(kind, node, source),
            SourceLanguage::Python => self.python_symbol_info(kind, node, source),
            SourceLanguage::JavaScript | SourceLanguage::TypeScript => {
                self.js_symbol_info(kind, node, source)
            }
            SourceLanguage::Unknown => return None,
        }?;

        let docstring = self.extract_docstring(node, source);

        Some(Symbol {
            name: name?,
            kind: symbol_kind,
            file: file_path.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            container: container.clone(),
            docstring,
        })
    }

    fn rust_symbol_info(
        &self,
        kind: &str,
        node: &Node,
        source: &str,
    ) -> Option<(SymbolKind, Option<String>)> {
        match kind {
            "function_item" | "function_declaration" => {
                Some((SymbolKind::Function, self.get_identifier_name(node, source)))
            }
            "struct_item" | "struct_declaration" => {
                Some((SymbolKind::Struct, self.get_identifier_name(node, source)))
            }
            "enum_item" => Some((SymbolKind::Enum, self.get_identifier_name(node, source))),
            "trait_item" => Some((SymbolKind::Trait, self.get_identifier_name(node, source))),
            "impl_item" => Some((SymbolKind::Impl, self.get_impl_name(node, source))),
            "mod_item" => Some((SymbolKind::Module, self.get_identifier_name(node, source))),
            "let_declaration" | "const_declaration" => {
                Some((SymbolKind::Variable, self.get_identifier_name(node, source)))
            }
            "type_item" => Some((SymbolKind::Type, self.get_identifier_name(node, source))),
            _ => None,
        }
    }

    fn python_symbol_info(
        &self,
        kind: &str,
        node: &Node,
        source: &str,
    ) -> Option<(SymbolKind, Option<String>)> {
        match kind {
            "function_definition" | "async_function_definition" => {
                Some((SymbolKind::Function, self.get_identifier_name(node, source)))
            }
            "class_definition" => Some((SymbolKind::Class, self.get_identifier_name(node, source))),
            _ => None,
        }
    }

    fn js_symbol_info(
        &self,
        kind: &str,
        node: &Node,
        source: &str,
    ) -> Option<(SymbolKind, Option<String>)> {
        match kind {
            "function_declaration" => {
                Some((SymbolKind::Function, self.get_identifier_name(node, source)))
            }
            "arrow_function" => {
                Some((SymbolKind::Function, self.get_identifier_name(node, source)))
            }
            "class_declaration" => {
                Some((SymbolKind::Class, self.get_identifier_name(node, source)))
            }
            "method_definition" => Some((SymbolKind::Method, self.get_method_name(node, source))),
            "lexical_declaration" | "variable_declaration" => {
                Some((SymbolKind::Variable, self.get_identifier_name(node, source)))
            }
            "interface_declaration" => {
                Some((SymbolKind::Trait, self.get_identifier_name(node, source)))
            }
            "type_alias_declaration" => {
                Some((SymbolKind::Type, self.get_identifier_name(node, source)))
            }
            _ => None,
        }
    }

    fn get_identifier_name(&self, node: &Node, source: &str) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let kind = child.kind();
            if kind == "identifier" || kind == "type_identifier" || kind == "attribute_identifier" {
                return child
                    .utf8_text(source.as_bytes())
                    .ok()
                    .map(|s| s.to_string());
            }
        }
        None
    }

    fn get_impl_name(&self, node: &Node, source: &str) -> Option<String> {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            if child.kind() == "type_identifier" {
                return child
                    .utf8_text(source.as_bytes())
                    .ok()
                    .map(|s| format!("impl {}", s));
            }
        }

        for child in &children {
            let mut cursor2 = child.walk();
            for grandchild in child.children(&mut cursor2) {
                if grandchild.kind() == "type_identifier" {
                    return grandchild
                        .utf8_text(source.as_bytes())
                        .ok()
                        .map(|s| format!("impl {}", s));
                }
            }
        }
        None
    }

    fn get_method_name(&self, node: &Node, source: &str) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "property_identifier" {
                return child
                    .utf8_text(source.as_bytes())
                    .ok()
                    .map(|s| s.to_string());
            }
        }
        None
    }

    fn is_reference(&self, kind: &str) -> bool {
        matches!(
            kind,
            "identifier" | "scoped_identifier" | "member_expression" | "call_expression"
        )
    }

    fn ends_container(&self, kind: &str) -> bool {
        matches!(
            kind,
            "function_item"
                | "function_declaration"
                | "struct_item"
                | "class_definition"
                | "impl_item"
                | "module_clause"
        )
    }

    fn extract_docstring(&self, node: &Node, source: &str) -> Option<String> {
        let _cursor = node.walk();

        if let Some(prev_sibling) = node.prev_sibling() {
            if prev_sibling.kind() == "line_comment" {
                return prev_sibling
                    .utf8_text(source.as_bytes())
                    .ok()
                    .map(|s| s.trim_start_matches("//").trim().to_string());
            }
            if prev_sibling.kind() == "block_comment" {
                return prev_sibling.utf8_text(source.as_bytes()).ok().map(|s| {
                    s.trim_start_matches("/*")
                        .trim_end_matches("*/")
                        .trim()
                        .to_string()
                });
            }
        }

        if let Some(first_child) = node.child(0) {
            if first_child.kind() == "attribute_item" {
                if let Some(attr_child) = first_child.child(1) {
                    if attr_child.kind() == "doc_comment" {
                        return attr_child
                            .utf8_text(source.as_bytes())
                            .ok()
                            .map(|s| s.trim().to_string());
                    }
                }
            }
        }

        None
    }
}

pub struct WorkspaceSymbolIndex {
    symbols: DashMap<String, Vec<Symbol>>,
}

impl WorkspaceSymbolIndex {
    pub fn new() -> Self {
        Self {
            symbols: DashMap::new(),
        }
    }

    pub fn update(&self, file_path: &str, analysis: SemanticAnalysis) {
        self.symbols
            .retain(|_, v| v.iter().any(|s| s.file != file_path));

        for symbol in analysis.symbols {
            let key = symbol.name.to_lowercase();
            self.symbols.entry(key).or_default().push(symbol);
        }
    }

    pub fn remove_file(&self, file_path: &str) {
        self.symbols.retain(|_, v| {
            v.retain(|s| s.file != file_path);
            !v.is_empty()
        });
    }

    pub fn search(&self, query: &str) -> Vec<Symbol> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for entry in self.symbols.iter() {
            for symbol in entry.value().iter() {
                if symbol.name.to_lowercase().contains(&query_lower) {
                    results.push(symbol.clone());
                }
            }
        }

        results.sort_by(|a, b| a.start_line.cmp(&b.start_line));
        results
    }

    pub fn find_symbol(&self, name: &str, file: &str, line: usize) -> Option<Symbol> {
        let key = name.to_lowercase();
        self.symbols.get(&key).and_then(|symbols| {
            symbols
                .iter()
                .find(|s| s.file == file && s.start_line <= line && s.end_line >= line)
                .cloned()
        })
    }
}

impl Default for WorkspaceSymbolIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_symbol_extraction() {
        let source = r#"
/// Docstring for function
fn main() {
    let x = 5;
}

struct MyStruct {
    field: i32,
}
"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();

        let analyzer = SemanticAnalyzer::new(SourceLanguage::Rust);
        let analysis = analyzer.analyze(source, &tree.root_node(), "test.rs");

        assert!(analysis.symbols.len() >= 2);

        let has_function = analysis
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Function);
        let has_struct = analysis
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Struct);

        assert!(has_function, "Should find a function");
        assert!(has_struct, "Should find a struct");
    }

    #[test]
    fn test_python_symbol_extraction() {
        let source = r#"
def main():
    pass

class MyClass:
    pass
"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();

        let analyzer = SemanticAnalyzer::new(SourceLanguage::Python);
        let analysis = analyzer.analyze(source, &tree.root_node(), "test.py");

        let has_function = analysis
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Function);
        let has_class = analysis.symbols.iter().any(|s| s.kind == SymbolKind::Class);

        assert!(has_function, "Should find a function");
        assert!(has_class, "Should find a class");
    }

    #[test]
    fn test_workspace_symbol_index() {
        let index = WorkspaceSymbolIndex::new();

        let symbols = vec![
            Symbol {
                name: "main".to_string(),
                kind: SymbolKind::Function,
                file: "test.rs".to_string(),
                start_line: 1,
                end_line: 10,
                container: None,
                docstring: None,
            },
            Symbol {
                name: "MyStruct".to_string(),
                kind: SymbolKind::Struct,
                file: "test.rs".to_string(),
                start_line: 12,
                end_line: 20,
                container: None,
                docstring: None,
            },
        ];

        let analysis = SemanticAnalysis {
            symbols,
            references: HashMap::new(),
        };

        index.update("test.rs", analysis);

        let results = index.search("main");
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "main");
    }
}
