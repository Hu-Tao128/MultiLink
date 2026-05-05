use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{Tool, ToolInput, ToolResult};
use crate::context_engine::ContextEngine;

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
        let query = match input
            .args
            .as_ref()
            .and_then(|a| a.get("query"))
            .and_then(|v| v.as_str())
        {
            Some(q) => q,
            None => return ToolResult::err("query is required"),
        };

        let top_k = input
            .args
            .as_ref()
            .and_then(|a| a.get("top_k"))
            .and_then(|v| v.as_u64())
            .map(|k| k as usize)
            .unwrap_or(5);

        let config = crate::context_engine::ContextRetrievalConfig::default();

        let result = self
            .context_engine
            .retrieve(
                project_root.to_string_lossy().as_ref(),
                query,
                top_k * 500,
                None,
                &config,
            )
            .await;

        let results: Vec<Value> = result
            .selected_files
            .iter()
            .enumerate()
            .map(|(i, path)| {
                let snippet = result
                    .context
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
        let path = input.path.clone().or_else(|| {
            input
                .args
                .as_ref()
                .and_then(|a| a.get("path"))
                .and_then(|v| v.as_str())
                .map(String::from)
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

        const DEFAULT_MAX_FILE_LINES: usize = 2000;
        const DEFAULT_TRUNCATE_LINES: usize = 500;
        const DEFAULT_LINES_PER_CHUNK: usize = 500;
        const TOKENS_PER_LINE: usize = 4;
        const RESERVED_TOKENS: usize = 800;
        const CHUNK_SAFE_PERCENT: f32 = 0.6;

        let max_lines = input
            .args
            .as_ref()
            .and_then(|a| a.get("max_lines"))
            .and_then(|v| v.as_u64())
            .map(|c| c as usize)
            .unwrap_or(DEFAULT_MAX_FILE_LINES);

        let truncate_lines = input
            .args
            .as_ref()
            .and_then(|a| a.get("truncate_lines"))
            .and_then(|v| v.as_u64())
            .map(|c| c as usize)
            .unwrap_or(DEFAULT_TRUNCATE_LINES);

        let lines_per_chunk = input
            .args
            .as_ref()
            .and_then(|a| a.get("lines_per_chunk"))
            .and_then(|v| v.as_u64())
            .map(|c| c as usize)
            .unwrap_or(DEFAULT_LINES_PER_CHUNK);

        let chunk_index = input
            .args
            .as_ref()
            .and_then(|a| a.get("chunk"))
            .and_then(|v| v.as_u64())
            .map(|c| c as usize);

        let search_pattern = input
            .args
            .as_ref()
            .and_then(|a| a.get("search"))
            .and_then(|v| v.as_str());

        let available_tokens = input
            .args
            .as_ref()
            .and_then(|a| a.get("available_tokens"))
            .and_then(|v| v.as_u64())
            .map(|c| c as usize);

        let (dynamic_max_lines, dynamic_truncate, dynamic_chunk_size) =
            if let Some(tokens) = available_tokens {
                let reserved = RESERVED_TOKENS.max(tokens / 4);
                let safe_for_content =
                    (tokens.saturating_sub(reserved) as f32 * CHUNK_SAFE_PERCENT) as usize;
                let calculated = safe_for_content / TOKENS_PER_LINE;
                (
                    calculated.saturating_add(300),
                    calculated.saturating_sub(150),
                    calculated.saturating_sub(150),
                )
            } else {
                (max_lines, truncate_lines, lines_per_chunk)
            };

        match tokio::fs::read_to_string(&full_path).await {
            Ok(content) => {
                let total_lines = content.lines().count();
                let effective_chunk_size = if dynamic_chunk_size > 0 {
                    dynamic_chunk_size
                } else {
                    DEFAULT_LINES_PER_CHUNK
                };
                let total_chunks = total_lines.div_ceil(effective_chunk_size);

                let (display_content, chunk_info) = if total_lines > dynamic_max_lines {
                    if let Some(chunk_idx) = chunk_index {
                        let start = chunk_idx * effective_chunk_size;
                        let end = std::cmp::min(start + effective_chunk_size, total_lines);
                        let chunk_content: String = content
                            .lines()
                            .skip(start)
                            .take(end - start)
                            .collect::<Vec<_>>()
                            .join("\n");
                        let info = json!({
                            "chunk_index": chunk_idx,
                            "total_chunks": total_chunks,
                            "lines_in_chunk": end - start,
                            "message": format!("[Chunk {}/{}]", chunk_idx + 1, total_chunks)
                        });
                        (chunk_content, Some(info))
                    } else {
                        let truncated: String = content
                            .lines()
                            .take(dynamic_truncate)
                            .collect::<Vec<_>>()
                            .join("\n");
                        let info = json!({
                            "chunk_index": 0,
                            "total_chunks": total_chunks,
                            "lines_in_chunk": dynamic_truncate,
                            "message": format!("[truncated] Showing first {} of {} lines. Total chunks: {}. Use 'chunk' param to access other chunks.", dynamic_truncate, total_lines, total_chunks)
                        });
                        (truncated, Some(info))
                    }
                } else {
                    (content.clone(), None)
                };

                let mut result = json!({
                    "path": path,
                    "content": display_content,
                    "size": display_content.len(),
                    "total_lines": total_lines
                });

                if let Some(info) = chunk_info {
                    result["chunk_info"] = info;
                }

                if let Some(pattern) = search_pattern {
                    let pattern_lower = pattern.to_lowercase();
                    let matching_lines: Vec<String> = display_content
                        .lines()
                        .enumerate()
                        .filter(|(_, line)| line.to_lowercase().contains(&pattern_lower))
                        .map(|(idx, line)| format!("{}: {}", idx + 1, line))
                        .take(20)
                        .collect();

                    result["matching_lines"] = json!(matching_lines);
                    result["search_pattern"] = json!(pattern);
                }

                ToolResult::ok(result)
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
        let query = match input
            .args
            .as_ref()
            .and_then(|a| a.get("query"))
            .and_then(|v| v.as_str())
        {
            Some(q) => q,
            None => return ToolResult::err("query is required"),
        };

        let top_k = input
            .args
            .as_ref()
            .and_then(|a| a.get("top_k"))
            .and_then(|v| v.as_u64())
            .map(|k| k as usize)
            .unwrap_or(3);

        let config = crate::context_engine::ContextRetrievalConfig::default();

        let result = self
            .context_engine
            .retrieve(
                project_root.to_string_lossy().as_ref(),
                query,
                top_k * 500,
                None,
                &config,
            )
            .await;

        let mut files = Vec::new();

        for path in result.selected_files.iter().take(top_k) {
            let full_path = project_root.join(path);
            let content_preview = match tokio::fs::read_to_string(&full_path).await {
                Ok(c) => c.lines().take(30).collect::<Vec<_>>().join("\n"),
                Err(_) => String::new(),
            };

            let snippet = result
                .context
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
        let tool = SearchCode::new(Arc::new(crate::context_engine::ContextEngineV1)
            as Arc<dyn crate::context_engine::ContextEngine>);
        let schema = tool.input_schema();
        assert_eq!(schema["properties"]["query"]["type"], "string");
        assert_eq!(schema["properties"]["top_k"]["type"], "number");
    }
}
