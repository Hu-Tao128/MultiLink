use std::fmt;
use std::sync::OnceLock;
use tree_sitter::{Language, Parser, Tree};

static RUST_LANGUAGE: OnceLock<Language> = OnceLock::new();
static PYTHON_LANGUAGE: OnceLock<Language> = OnceLock::new();
static JAVASCRIPT_LANGUAGE: OnceLock<Language> = OnceLock::new();
static TYPESCRIPT_LANGUAGE: OnceLock<Language> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLanguage {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Unknown,
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
}

impl fmt::Display for SourceLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rust => write!(f, "rust"),
            Self::Python => write!(f, "python"),
            Self::JavaScript => write!(f, "javascript"),
            Self::TypeScript => write!(f, "typescript"),
            Self::Unknown => write!(f, "text"),
        }
    }
}

pub struct TreeSitterParser {
    parser: Parser,
    language: SourceLanguage,
}

impl TreeSitterParser {
    pub fn new(language: SourceLanguage) -> Result<Self, String> {
        let lang = match language {
            SourceLanguage::Rust => get_rust_language(),
            SourceLanguage::Python => get_python_language(),
            SourceLanguage::JavaScript => get_javascript_language(),
            SourceLanguage::TypeScript => get_typescript_language(),
            SourceLanguage::Unknown => return Err("Cannot parse unknown language".to_string()),
        };

        let mut parser = Parser::new();
        parser
            .set_language(&lang)
            .map_err(|e| format!("Failed to set language: {}", e))?;

        Ok(Self { parser, language })
    }

    pub fn parse(&mut self, source: &str) -> Result<Tree, String> {
        self.parser
            .parse(source, None)
            .ok_or_else(|| "Failed to parse source".to_string())
    }

    pub fn language(&self) -> SourceLanguage {
        self.language
    }
}

fn get_rust_language() -> Language {
    RUST_LANGUAGE
        .get_or_init(|| tree_sitter_rust::LANGUAGE.into())
        .clone()
}

fn get_python_language() -> Language {
    PYTHON_LANGUAGE
        .get_or_init(|| tree_sitter_python::LANGUAGE.into())
        .clone()
}

fn get_javascript_language() -> Language {
    JAVASCRIPT_LANGUAGE
        .get_or_init(|| tree_sitter_javascript::LANGUAGE.into())
        .clone()
}

fn get_typescript_language() -> Language {
    TYPESCRIPT_LANGUAGE
        .get_or_init(|| tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .clone()
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
    fn test_parse_rust() {
        let mut parser = TreeSitterParser::new(SourceLanguage::Rust).unwrap();
        let source = r#"
fn main() {
    println!("Hello");
}

struct MyStruct {
    field: i32,
}
"#;
        let tree = parser.parse(source).unwrap();
        assert!(tree.root_node().child_count() > 0);
    }
}
