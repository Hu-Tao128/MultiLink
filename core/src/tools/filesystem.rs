use std::path::{Path, PathBuf};
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::{ToolInput, ToolResult, Tool};

pub fn normalize_path(base: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative_path = Path::new(relative);

    if relative_path.is_absolute() {
        return Err("Absolute paths are not allowed".to_string());
    }

    let clean = relative_path
        .components()
        .filter(|c| !matches!(c, std::path::Component::CurDir))
        .collect::<PathBuf>();

    let full_path = base.join(&clean);

    if !full_path.starts_with(base) {
        return Err("Path escape detected".to_string());
    }

    Ok(full_path)
}

pub struct FsLs;

#[async_trait]
impl Tool for FsLs {
    fn name(&self) -> &'static str { "fs_ls" }
    fn description(&self) -> &'static str { "List files and directories in a path" }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Relative path from project root"}
            }
        })
    }

    async fn execute(&self, input: ToolInput, project_root: &Path) -> ToolResult {
        let target_path = match input.path {
            Some(p) => match normalize_path(project_root, &p) {
                Ok(path) => path,
                Err(e) => return ToolResult::err(e),
            },
            None => project_root.to_path_buf(),
        };

        if !target_path.exists() {
            return ToolResult::err("Path does not exist");
        }

        if !target_path.is_dir() {
            return ToolResult::err("Path is not a directory");
        }

        match std::fs::read_dir(&target_path) {
            Ok(entries) => {
                let mut result: Vec<Value> = Vec::new();
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = path.is_dir();
                    result.push(json!({
                        "name": name,
                        "type": if is_dir { "directory" } else { "file" }
                    }));
                }

                result.sort_by(|a, b| {
                    let a_is_dir = a.get("type").and_then(|t| t.as_str()) == Some("directory");
                    let b_is_dir = b.get("type").and_then(|t| t.as_str()) == Some("directory");
                    match (a_is_dir, b_is_dir) {
                        (true, false) => std::cmp::Ordering::Less,
                        (false, true) => std::cmp::Ordering::Greater,
                        _ => a.get("name").and_then(|n| n.as_str()).cmp(&b.get("name").and_then(|n| n.as_str())),
                    }
                });

                ToolResult::ok(json!({
                    "entries": result,
                    "path": target_path.to_string_lossy()
                }))
            }
            Err(e) => ToolResult::err(format!("Failed to read directory: {}", e)),
        }
    }
}

pub struct FsCat;

#[async_trait]
impl Tool for FsCat {
    fn name(&self) -> &'static str { "fs_cat" }
    fn description(&self) -> &'static str { "Read file contents" }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {"type": "string", "description": "Relative path from project root"}
            }
        })
    }

    async fn execute(&self, input: ToolInput, project_root: &Path) -> ToolResult {
        let path = match input.path {
            Some(p) => p,
            None => return ToolResult::err("path is required"),
        };

        let target_path = match normalize_path(project_root, &path) {
            Ok(path) => path,
            Err(e) => return ToolResult::err(e),
        };

        if !target_path.exists() {
            return ToolResult::err("File does not exist");
        }

        if !target_path.is_file() {
            return ToolResult::err("Path is not a file");
        }

        match std::fs::read_to_string(&target_path) {
            Ok(content) => ToolResult::ok(json!({
                "content": content,
                "path": target_path.to_string_lossy(),
                "size": content.len()
            })),
            Err(e) => ToolResult::err(format!("Failed to read file: {}", e)),
        }
    }
}

pub struct FsGrep;

#[async_trait]
impl Tool for FsGrep {
    fn name(&self) -> &'static str { "fs_grep" }
    fn description(&self) -> &'static str { "Search for pattern in files (pure Rust, no shell)" }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": {"type": "string", "description": "Pattern to search for"},
                "path": {"type": "string", "description": "Optional path to search in"},
                "limit": {"type": "integer", "description": "Maximum results (default 50)"}
            }
        })
    }

    async fn execute(&self, input: ToolInput, project_root: &Path) -> ToolResult {
        let pattern = match input.pattern {
            Some(p) => p,
            None => return ToolResult::err("pattern is required"),
        };

        let base_path = match input.path {
            Some(p) => match normalize_path(project_root, &p) {
                Ok(path) => path,
                Err(e) => return ToolResult::err(e),
            },
            None => project_root.to_path_buf(),
        };

        let limit = input.args
            .as_ref()
            .and_then(|args| args.get("limit"))
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;

        let pattern_lower = pattern.to_lowercase();
        let mut results = Vec::new();

        if base_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&base_path) {
                for (line_num, line) in content.lines().enumerate() {
                    if line.to_lowercase().contains(&pattern_lower) {
                        results.push(json!({
                            "file": base_path.to_string_lossy(),
                            "line": line_num + 1,
                            "content": line
                        }));
                        if results.len() >= limit {
                            break;
                        }
                    }
                }
            }
        } else if base_path.is_dir() {
            let entries = walkdir::WalkDir::new(&base_path)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .take(100);

            for entry in entries {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !matches!(ext, "rs" | "toml" | "json" | "md" | "yaml" | "yml" | "txt" | "sh" | "py" | "js" | "ts") {
                    continue;
                }

                if let Ok(content) = std::fs::read_to_string(path) {
                    for (line_num, line) in content.lines().enumerate() {
                        if line.to_lowercase().contains(&pattern_lower) {
                            results.push(json!({
                                "file": path.to_string_lossy(),
                                "line": line_num + 1,
                                "content": line
                            }));
                            if results.len() >= limit {
                                break;
                            }
                        }
                    }
                }

                if results.len() >= limit {
                    break;
                }
            }
        }

        ToolResult::ok(json!({
            "results": results,
            "count": results.len(),
            "pattern": pattern
        }))
    }
}