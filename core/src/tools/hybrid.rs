use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::context_engine::ContextEngine;
use super::{Tool, ToolInput, ToolResult};

pub struct SearchCode {
    context_engine: Arc<dyn ContextEngine>,
}

impl SearchCode {
    pub fn new(context_engine: Arc<dyn ContextEngine>) -> Self {
        Self { context_engine }
    }
}

#[async_trait]
impl Tool for SearchCode {
    fn name(&self) -> &'static str {
        "search_code"
    }

    fn description(&self) -> &'static str {
        "Search codebase using Context Engine (lexical search). Returns top_k results with path, score, and snippet."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "top_k": { "type": "number", "description": "Number of results (default: 5)" }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, input: ToolInput, project_root: &Path) -> ToolResult {
        let query = match input.args.as_ref().and_then(|a| a.get("query")).and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::err("query is required"),
        };

        let top_k = input.args
            .as_ref()
            .and_then(|a| a.get("top_k"))
            .and_then(|v| v.as_u64())
            .map(|k| k as usize)
            .unwrap_or(5);

        let config = crate::context_engine::ContextRetrievalConfig::default();
        
        let result = self.context_engine
            .retrieve(project_root.to_string_lossy().as_ref(), query, top_k * 500, None, &config)
            .await;

        let results: Vec<Value> = result.selected_files
            .iter()
            .enumerate()
            .map(|(i, path)| {
                let snippet = result.context
                    .lines()
                    .skip(i * 10)
                    .take(5)
                    .collect::<Vec<_>>()
                    .join("\n");
                
                json!({
                    "path": path,
                    "score": 1.0 - (i as f32 * 0.1),
                    "snippet": snippet
                })
            })
            .collect();

        ToolResult::ok(json!({
            "query": query,
            "results": results,
            "total": results.len()
        }))
    }
}

pub struct OpenFile;

#[async_trait]
impl Tool for OpenFile {
    fn name(&self) -> &'static str {
        "open_file"
    }

    fn description(&self) -> &'static str {
        "Read a file safely within project root. Returns file content."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Relative path to file" }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, input: ToolInput, project_root: &Path) -> ToolResult {
        let path = input.path.clone()
            .or_else(|| {
                input.args.as_ref().and_then(|a| a.get("path")).and_then(|v| v.as_str()).map(String::from)
            });

        let path = match path {
            Some(p) => p,
            None => return ToolResult::err("path is required"),
        };

        let safe_path = normalize_path(&path, project_root);
        if safe_path.is_none() {
            return ToolResult::err("Invalid path: must be relative and stay within project");
        }

        let full_path = project_root.join(&path);
        
        match tokio::fs::read_to_string(&full_path).await {
            Ok(content) => {
                let preview: String = content.lines().take(50).collect::<Vec<_>>().join("\n");
                ToolResult::ok(json!({
                    "path": path,
                    "content": content,
                    "content_preview": preview,
                    "size": content.len()
                }))
            }
            Err(e) => ToolResult::err(format!("Failed to read file: {}", e)),
        }
    }
}

pub struct SearchAndOpen {
    context_engine: Arc<dyn ContextEngine>,
}

impl SearchAndOpen {
    pub fn new(context_engine: Arc<dyn ContextEngine>) -> Self {
        Self { context_engine }
    }
}

#[async_trait]
impl Tool for SearchAndOpen {
    fn name(&self) -> &'static str {
        "search_and_open"
    }

    fn description(&self) -> &'static str {
        "Hybrid tool: search codebase then open top results. Combines retrieval + file reading."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "top_k": { "type": "number", "description": "Number of files to open (default: 3)" }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, input: ToolInput, project_root: &Path) -> ToolResult {
        let query = match input.args.as_ref().and_then(|a| a.get("query")).and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::err("query is required"),
        };

        let top_k = input.args
            .as_ref()
            .and_then(|a| a.get("top_k"))
            .and_then(|v| v.as_u64())
            .map(|k| k as usize)
            .unwrap_or(3);

        let config = crate::context_engine::ContextRetrievalConfig::default();
        
        let result = self.context_engine
            .retrieve(project_root.to_string_lossy().as_ref(), query, top_k * 500, None, &config)
            .await;

        let mut files = Vec::new();
        
        for path in result.selected_files.iter().take(top_k) {
            let full_path = project_root.join(path);
            let content_preview = match tokio::fs::read_to_string(&full_path).await {
                Ok(c) => c.lines().take(30).collect::<Vec<_>>().join("\n"),
                Err(_) => String::new(),
            };

            let snippet = result.context
                .lines()
                .filter(|l| l.contains(path))
                .take(3)
                .collect::<Vec<_>>()
                .join("\n");

            files.push(json!({
                "path": path,
                "snippet": if snippet.is_empty() { content_preview.lines().take(3).collect::<Vec<_>>().join("\n") } else { snippet },
                "content_preview": content_preview
            }));
        }

        ToolResult::ok(json!({
            "query": query,
            "files": files,
            "total": files.len()
        }))
    }
}

fn normalize_path(path: &str, project_root: &Path) -> Option<PathBuf> {
    if path.contains("..") || path.starts_with('/') || path.starts_with('\\') {
        return None;
    }
    
    let full = project_root.join(path);
    let canonical_root = project_root.canonicalize().ok()?;
    let canonical_full = full.canonicalize().ok()?;
    
    if canonical_full.starts_with(&canonical_root) {
        Some(full)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_file_schema() {
        let tool = OpenFile;
        let schema = tool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
    }

    #[test]
    fn test_search_code_schema() {
        let tool = SearchCode::new(Arc::new(crate::context_engine::ContextEngineV1) as Arc<dyn crate::context_engine::ContextEngine>);
        let schema = tool.input_schema();
        assert_eq!(schema["properties"]["query"]["type"], "string");
        assert_eq!(schema["properties"]["top_k"]["type"], "number");
    }
}