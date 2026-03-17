use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::ast_cache::{AstCache, SourceLanguage};
use crate::document_cache::DocumentCache;
use crate::semantic_analysis::{SemanticAnalyzer, WorkspaceSymbolIndex};

const DEBOUNCE_MS: u64 = 300;

pub struct Backend {
    client: Client,
    documents: Arc<Mutex<DocumentCache>>,
    ast_cache: Arc<AstCache>,
    symbol_index: Arc<WorkspaceSymbolIndex>,
    pending_parse: Arc<Mutex<HashMap<String, bool>>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(Mutex::new(DocumentCache::new())),
            ast_cache: Arc::new(AstCache::new()),
            symbol_index: Arc::new(WorkspaceSymbolIndex::new()),
            pending_parse: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn get_language(uri: &Url) -> SourceLanguage {
        uri.path_segments()
            .and_then(|mut segments| segments.next_back())
            .map(SourceLanguage::from_filename)
            .unwrap_or(SourceLanguage::Unknown)
    }

    async fn analyze_and_update_index(&self, uri: &Url, content: &str) {
        let language = Self::get_language(uri);
        if language == SourceLanguage::Unknown {
            return;
        }

        if let Some(tree) = self.ast_cache.get(uri.as_str()) {
            let analyzer = SemanticAnalyzer::new(language);
            let analysis = analyzer.analyze(content, &tree.root_node(), uri.as_str());
            self.symbol_index.update(uri.as_str(), analysis);
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> tower_lsp::jsonrpc::Result<InitializeResult> {
        self.client
            .log_message(MessageType::INFO, "MultiLink LSP initialized")
            .await;

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "MultiLink LSP".to_string(),
                version: Some("0.1.0".to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "MultiLink Language Server initialized")
            .await;
    }

    async fn shutdown(&self) -> tower_lsp::jsonrpc::Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let content = params.text_document.text;
        let language = Self::get_language(&uri);

        {
            let mut docs = self.documents.lock().await;
            docs.put(uri.clone(), content.clone());
        }

        if language != SourceLanguage::Unknown {
            self.ast_cache.parse(uri.as_str(), &content, language);
            self.analyze_and_update_index(&uri, &content).await;
            self.client
                .log_message(MessageType::INFO, &format!("Parsed {:?} document", language))
                .await;
        } else {
            self.client
                .log_message(MessageType::INFO, "Document opened (unsupported language for AST)")
                .await;
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let content = params.content_changes[0].text.clone();

        {
            let mut docs = self.documents.lock().await;
            docs.put(uri.clone(), content.clone());
        }

        let language = Self::get_language(&uri);
        if language != SourceLanguage::Unknown {
            self.ast_cache.parse(uri.as_str(), &content, language);
            
            let uri_for_debounce = uri.clone();
            let pending_clone = self.pending_parse.clone();
            
            tokio::spawn(async move {
                let uri_str = uri_for_debounce.to_string();
                {
                    let mut p = pending_clone.lock().await;
                    p.insert(uri_str.clone(), true);
                }
                
                tokio::time::sleep(Duration::from_millis(DEBOUNCE_MS)).await;
                
                {
                    let mut p = pending_clone.lock().await;
                    p.remove(&uri_str);
                }
            });
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        
        let content = {
            let docs = self.documents.lock().await;
            docs.get(&uri)
        };

        if let Some(content) = content {
            self.analyze_and_update_index(&uri, &content).await;
            self.run_diagnostics(&uri, &content).await;
        }

        self.client
            .log_message(MessageType::INFO, "Document saved and analyzed")
            .await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        
        {
            let mut docs = self.documents.lock().await;
            docs.remove(&uri);
        }

        self.ast_cache.remove(uri.as_str());
        self.symbol_index.remove_file(uri.as_str());
    }

    async fn hover(&self, params: HoverParams) -> tower_lsp::jsonrpc::Result<Option<Hover>> {
        let docs = self.documents.lock().await;
        let uri = params.text_document_position_params.text_document.uri.clone();

        if let Some(content) = docs.get(&uri) {
            let lines: Vec<&str> = content.lines().collect();
            let line = params.text_document_position_params.position.line as usize;

            if line < lines.len() {
                let line_content = lines[line];
                let character = params.text_document_position_params.position.character as usize;
                let word = extract_word_at_position(line_content, character);

                if !word.is_empty() {
                    let mut symbol_info = format!("**{}** - Symbol from MultiLink Context", word);

                    if let Some(tree) = self.ast_cache.get(uri.as_str()) {
                        if let Some(symbol_detail) = find_symbol_in_tree(&tree.root_node(), &content, &word, line + 1) {
                            symbol_info = symbol_detail;
                        }
                    }

                    if let Some(symbol) = self.symbol_index.find_symbol(&word, uri.as_str(), line + 1) {
                        if let Some(doc) = &symbol.docstring {
                            symbol_info = format!("**{}**\n\n{}\n\nLines: {}-{}", 
                                symbol.name, doc, symbol.start_line, symbol.end_line);
                        } else {
                            symbol_info = format!("**{}** ({:?})\nLines: {}-{}", 
                                symbol.name, symbol.kind, symbol.start_line, symbol.end_line);
                        }
                    }

                    return Ok(Some(Hover {
                        contents: HoverContents::Scalar(MarkedString::String(symbol_info)),
                        range: Some(Range {
                            start: params.text_document_position_params.position,
                            end: params.text_document_position_params.position,
                        }),
                    }));
                }
            }
        }

        Ok(None)
    }
}

impl Backend {
    async fn run_diagnostics(&self, uri: &Url, content: &str) {
        let language = Self::get_language(uri);
        if language == SourceLanguage::Unknown {
            return;
        }

        let mut diagnostics = Vec::new();

        if let Some(tree) = self.ast_cache.get(uri.as_str()) {
            let root = &tree.root_node();
            self.find_diagnostics(root, content, uri.as_str(), &mut diagnostics);
        }

        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
    }

    fn find_diagnostics(&self, node: &tree_sitter::Node, source: &str, uri: &str, diagnostics: &mut Vec<Diagnostic>) {
        let _kind = node.kind();

        if let Some(diag) = self.check_syntax_error(node, source, uri) {
            diagnostics.push(diag);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.find_diagnostics(&child, source, uri, diagnostics);
        }
    }

    fn check_syntax_error(&self, node: &tree_sitter::Node, _source: &str, _uri: &str) -> Option<Diagnostic> {
        if node.is_error() || node.kind() == "ERROR" {
            let start = node.start_position();
            let end = node.end_position();

            return Some(Diagnostic {
                range: Range {
                    start: Position {
                        line: start.row as u32,
                        character: start.column as u32,
                    },
                    end: Position {
                        line: end.row as u32,
                        character: end.column as u32,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("syntax-error".to_string())),
                source: Some("MultiLink LSP".to_string()),
                message: "Syntax error".to_string(),
                ..Default::default()
            });
        }
        None
    }
}

fn extract_word_at_position(line: &str, character: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    if character >= chars.len() {
        return String::new();
    }

    let mut start = character;
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }

    let mut end = character;
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
        end += 1;
    }

    chars[start..end].iter().collect()
}

fn find_symbol_in_tree(node: &tree_sitter::Node, source: &str, symbol_name: &str, line: usize) -> Option<String> {
    let mut cursor = node.walk();
    
    for child in node.children(&mut cursor) {
        let kind = child.kind();
        
        if kind == "identifier" || kind == "type_identifier" {
            if let Ok(text) = child.utf8_text(source.as_bytes()) {
                if text == symbol_name && child.start_position().row + 1 == line {
                    if let Some(parent) = child.parent() {
                        return Some(format!("**{}** (`{}`) - line {}", text, parent.kind(), child.start_position().row + 1));
                    }
                    return Some(format!("**{}** - line {}", text, child.start_position().row + 1));
                }
            }
        }
        
        if let Some(result) = find_symbol_in_tree(&child, source, symbol_name, line) {
            return Some(result);
        }
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Url;

    #[test]
    fn test_language_detection_rust() {
        let url = Url::parse("file:///src/main.rs").unwrap();
        let lang = Backend::get_language(&url);
        assert_eq!(lang, SourceLanguage::Rust);
    }

    #[test]
    fn test_language_detection_python() {
        let url = Url::parse("file:///src/main.py").unwrap();
        let lang = Backend::get_language(&url);
        assert_eq!(lang, SourceLanguage::Python);
    }

    #[test]
    fn test_language_detection_typescript() {
        let url = Url::parse("file:///src/app.ts").unwrap();
        let lang = Backend::get_language(&url);
        assert_eq!(lang, SourceLanguage::TypeScript);
    }

    #[test]
    fn test_language_detection_javascript() {
        let url = Url::parse("file:///src/app.js").unwrap();
        let lang = Backend::get_language(&url);
        assert_eq!(lang, SourceLanguage::JavaScript);
    }

    #[test]
    fn test_language_detection_unknown() {
        let url = Url::parse("file:///src/main.xyz").unwrap();
        let lang = Backend::get_language(&url);
        assert_eq!(lang, SourceLanguage::Unknown);
    }

    #[tokio::test]
    async fn test_document_cache_operations() {
        use crate::document_cache::DocumentCache;

        let mut cache = DocumentCache::new();
        let url = Url::parse("file:///test.rs").unwrap();
        
        let content = "fn main() {}".to_string();
        cache.put(url.clone(), content.clone());
        
        let retrieved = cache.get(&url);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), content);
    }

    #[test]
    fn test_ast_cache_rust_parsing() {
        use crate::ast_cache::AstCache;

        let cache = AstCache::new();
        let content = r#"
pub fn hello() {
    println!("Hello, world!");
}

struct MyStruct {
    field: i32,
}
"#;
        
        cache.parse("file:///test.rs", content, SourceLanguage::Rust);
        
        let tree = cache.get("file:///test.rs");
        assert!(tree.is_some());
        
        let tree = tree.unwrap();
        let root = tree.root_node();
        assert!(root.child_count() > 0);
    }
}
