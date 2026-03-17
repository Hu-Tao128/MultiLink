#![allow(dead_code)]

use dashmap::DashMap;
use std::sync::OnceLock;
use tree_sitter::{Language, Parser, Tree};

static RUST_LANGUAGE: OnceLock<Language> = OnceLock::new();
static PYTHON_LANGUAGE: OnceLock<Language> = OnceLock::new();
static JAVASCRIPT_LANGUAGE: OnceLock<Language> = OnceLock::new();
static TYPESCRIPT_LANGUAGE: OnceLock<Language> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SourceLanguage {
    #[default]
    Unknown,
    Rust,
    Python,
    JavaScript,
    TypeScript,
}

impl SourceLanguage {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "rs" => Self::Rust,
            "py" => Self::Python,
            "js" | "jsx" => Self::JavaScript,
            "ts" | "tsx" => Self::TypeScript,
            _ => Self::Unknown,
        }
    }

    pub fn from_filename(filename: &str) -> Self {
        if let Some(ext) = filename.rsplit('.').next() {
            return Self::from_extension(ext);
        }
        Self::Unknown
    }

    pub fn is_supported(&self) -> bool {
        !matches!(self, Self::Unknown)
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Rust => "Rust",
            Self::Python => "Python",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
        }
    }
}

pub struct AstCache {
    cache: DashMap<String, CachedTree>,
}

#[derive(Clone)]
struct CachedTree {
    tree: Tree,
    version: usize,
}

impl AstCache {
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
        }
    }

    pub fn parse(&self, uri: &str, source: &str, language: SourceLanguage) -> Option<Tree> {
        let mut parser = self.get_parser(language)?;

        let existing = self.cache.get(uri);
        let old_tree = existing.as_ref().map(|e| e.tree.clone());

        let tree = parser.parse(source, old_tree.as_ref())?;

        self.cache.insert(
            uri.to_string(),
            CachedTree {
                tree: tree.clone(),
                version: existing.map(|e| e.version + 1).unwrap_or(1),
            },
        );

        Some(tree)
    }

    pub fn get(&self, uri: &str) -> Option<Tree> {
        self.cache.get(uri).map(|e| e.tree.clone())
    }

    pub fn remove(&self, uri: &str) {
        self.cache.remove(uri);
    }

    pub fn clear(&self) {
        self.cache.clear();
    }

    pub fn supported_languages() -> Vec<SourceLanguage> {
        vec![
            SourceLanguage::Rust,
            SourceLanguage::Python,
            SourceLanguage::JavaScript,
            SourceLanguage::TypeScript,
        ]
    }

    fn get_parser(&self, language: SourceLanguage) -> Option<Parser> {
        let lang = match language {
            SourceLanguage::Rust => get_rust_language(),
            SourceLanguage::Python => get_python_language(),
            SourceLanguage::JavaScript => get_javascript_language(),
            SourceLanguage::TypeScript => get_typescript_language(),
            SourceLanguage::Unknown => return None,
        }?;

        let mut parser = Parser::new();
        parser.set_language(&lang).ok()?;
        Some(parser)
    }
}

impl Default for AstCache {
    fn default() -> Self {
        Self::new()
    }
}

fn get_rust_language() -> Option<Language> {
    Some(
        RUST_LANGUAGE
            .get_or_init(|| tree_sitter_rust::LANGUAGE.into())
            .clone(),
    )
}

fn get_python_language() -> Option<Language> {
    Some(
        PYTHON_LANGUAGE
            .get_or_init(|| tree_sitter_python::LANGUAGE.into())
            .clone(),
    )
}

fn get_javascript_language() -> Option<Language> {
    Some(
        JAVASCRIPT_LANGUAGE
            .get_or_init(|| tree_sitter_javascript::LANGUAGE.into())
            .clone(),
    )
}

fn get_typescript_language() -> Option<Language> {
    Some(
        TYPESCRIPT_LANGUAGE
            .get_or_init(|| tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_detection() {
        assert_eq!(SourceLanguage::from_extension("rs"), SourceLanguage::Rust);
        assert_eq!(SourceLanguage::from_extension("py"), SourceLanguage::Python);
        assert_eq!(
            SourceLanguage::from_extension("js"),
            SourceLanguage::JavaScript
        );
        assert_eq!(
            SourceLanguage::from_extension("ts"),
            SourceLanguage::TypeScript
        );
        assert_eq!(
            SourceLanguage::from_extension("txt"),
            SourceLanguage::Unknown
        );
    }

    #[test]
    fn test_language_display_name() {
        assert_eq!(SourceLanguage::Rust.display_name(), "Rust");
    }

    #[test]
    fn test_is_supported() {
        assert!(SourceLanguage::Rust.is_supported());
        assert!(!SourceLanguage::Unknown.is_supported());
    }

    #[test]
    fn test_ast_cache_basic() {
        let cache = AstCache::new();
        let source = r#"
fn main() {
    println!("Hello");
}
"#;
        let tree = cache.parse("test.rs", source, SourceLanguage::Rust);
        assert!(tree.is_some());
        let tree = tree.unwrap();
        assert!(tree.root_node().child_count() > 0);
    }

    #[test]
    fn test_ast_cache_incremental() {
        let cache = AstCache::new();
        let source_v1 = r#"
fn main() {
    println!("Hello");
}
"#;
        let source_v2 = r#"
fn main() {
    println!("Hello World");
}
fn new_function() {}
"#;

        let tree1 = cache.parse("test.rs", source_v1, SourceLanguage::Rust);
        assert!(tree1.is_some());

        let tree2 = cache.parse("test.rs", source_v2, SourceLanguage::Rust);
        assert!(tree2.is_some());

        let cached = cache.get("test.rs");
        assert!(cached.is_some());
    }

    #[test]
    fn test_ast_cache_remove() {
        let cache = AstCache::new();
        let source = r#"fn main() {}"#;

        cache.parse("test.rs", source, SourceLanguage::Rust);
        assert!(cache.get("test.rs").is_some());

        cache.remove("test.rs");
        assert!(cache.get("test.rs").is_none());
    }

    #[test]
    fn test_supported_languages() {
        let langs = AstCache::supported_languages();
        assert!(langs.contains(&SourceLanguage::Rust));
        assert!(langs.contains(&SourceLanguage::Python));
    }
}
