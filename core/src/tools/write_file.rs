use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::Path;

use crate::tools::backup::save_backup;
use crate::tools::filesystem::normalize_path;
use crate::tools::{Tool, ToolInput, ToolResult};

const MAX_WRITE_SIZE: usize = 1_048_576;

pub struct WriteFile;

#[async_trait]
impl Tool for WriteFile {
    fn name(&self) -> &'static str {
        "write_file"
    }
    fn description(&self) -> &'static str {
        "Write content to a file with size limit and diff summary. Returns before/after metadata."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": {"type": "string", "description": "Relative path from project root"},
                "content": {"type": "string", "description": "Content to write to the file"}
            }
        })
    }

    async fn execute(&self, input: ToolInput, project_root: &Path) -> ToolResult {
        let path = match input.path {
            Some(p) => p,
            None => return ToolResult::err("path is required"),
        };

        let content = match input
            .args
            .as_ref()
            .and_then(|args| args.get("content"))
            .and_then(|v| v.as_str())
        {
            Some(c) => c.to_string(),
            None => return ToolResult::err("content is required in args"),
        };

        if content.len() > MAX_WRITE_SIZE {
            return ToolResult::err(format!(
                "Content exceeds maximum size of {} bytes (got {})",
                MAX_WRITE_SIZE,
                content.len()
            ));
        }

        let target_path = match normalize_path(project_root, &path) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };

        let (old_content, is_new) = if target_path.exists() {
            match std::fs::read_to_string(&target_path) {
                Ok(c) => (Some(c), false),
                Err(_) => (None, true),
            }
        } else {
            (None, true)
        };

        let backup_path = if !is_new {
            match save_backup(project_root, &path) {
                Ok(p) => Some(p),
                Err(e) => return ToolResult::err(e),
            }
        } else {
            None
        };

        if let Some(parent) = target_path.parent() {
            if !parent.exists() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return ToolResult::err(format!("Failed to create parent directory: {}", e));
                }
            }
        }

        if let Err(e) = std::fs::write(&target_path, &content) {
            return ToolResult::err(format!("Failed to write file: {}", e));
        }

        let diff_summary = if let Some(old) = old_content {
            compute_diff_summary(&old, &content)
        } else {
            DiffSummary {
                lines_before: 0,
                lines_after: content.lines().count(),
                added: content.lines().count(),
                removed: 0,
            }
        };

        ToolResult::ok(json!({
            "path": target_path.to_string_lossy(),
            "size": content.len(),
            "is_new": is_new,
            "lines_before": diff_summary.lines_before,
            "lines_after": diff_summary.lines_after,
            "lines_added": diff_summary.added,
            "lines_removed": diff_summary.removed,
            "backup_path": backup_path.map(|p| p.to_string_lossy().to_string())
        }))
    }
}

struct DiffSummary {
    lines_before: usize,
    lines_after: usize,
    added: usize,
    removed: usize,
}

fn compute_diff_summary(before: &str, after: &str) -> DiffSummary {
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let lines_before = before_lines.len();
    let lines_after = after_lines.len();

    let (added, removed) = naive_line_diff(&before_lines, &after_lines);

    DiffSummary {
        lines_before,
        lines_after,
        added,
        removed,
    }
}

fn naive_line_diff(before: &[&str], after: &[&str]) -> (usize, usize) {
    let before_len = before.len();
    let after_len = after.len();

    let mut lcs_len = vec![vec![0usize; after_len + 1]; before_len + 1];
    for i in 1..=before_len {
        for j in 1..=after_len {
            if before[i - 1] == after[j - 1] {
                lcs_len[i][j] = lcs_len[i - 1][j - 1] + 1;
            } else {
                lcs_len[i][j] = lcs_len[i - 1][j].max(lcs_len[i][j - 1]);
            }
        }
    }

    let common = lcs_len[before_len][after_len];
    let added = after_len.saturating_sub(common);
    let removed = before_len.saturating_sub(common);

    (added, removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::OnceLock;
    use tempfile::TempDir;

    fn test_root() -> &'static TempDir {
        static ROOT: OnceLock<TempDir> = OnceLock::new();
        ROOT.get_or_init(|| tempfile::tempdir().expect("tempdir"))
    }

    #[tokio::test]
    async fn write_file_creates_new_file() {
        let root = test_root();
        let tool = WriteFile;
        let result = tool
            .execute(
                ToolInput {
                    path: Some("new_test.txt".to_string()),
                    pattern: None,
                    args: Some(
                        vec![("content".to_string(), Value::String("hello world".to_string()))]
                            .into_iter()
                            .collect(),
                    ),
                },
                root.path(),
            )
            .await;

        assert!(result.success, "{:?}", result.error);
        assert_eq!(
            result.output.get("is_new").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            result.output.get("size").and_then(|v| v.as_u64()),
            Some(11)
        );
        let content = fs::read_to_string(root.path().join("new_test.txt")).unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn write_file_overwrites_existing() {
        let root = test_root();
        let path = root.path().join("overwrite_test.txt");
        fs::write(&path, "old content").unwrap();

        let tool = WriteFile;
        let result = tool
            .execute(
                ToolInput {
                    path: Some("overwrite_test.txt".to_string()),
                    pattern: None,
                    args: Some(
                        vec![("content".to_string(), Value::String("new content".to_string()))]
                            .into_iter()
                            .collect(),
                    ),
                },
                root.path(),
            )
            .await;

        assert!(result.success, "{:?}", result.error);
        assert_eq!(
            result.output.get("is_new").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(
            result.output.get("lines_before").and_then(|v| v.as_u64()),
            Some(1)
        );
        assert_eq!(
            result.output.get("lines_after").and_then(|v| v.as_u64()),
            Some(1)
        );
    }

    #[tokio::test]
    async fn write_file_rejects_absolute_path() {
        let root = test_root();
        let tool = WriteFile;
        let result = tool
            .execute(
                ToolInput {
                    path: Some("/etc/passwd".to_string()),
                    pattern: None,
                    args: Some(
                        vec![("content".to_string(), Value::String("bad".to_string()))]
                            .into_iter()
                            .collect(),
                    ),
                },
                root.path(),
            )
            .await;

        assert!(!result.success);
    }

    #[tokio::test]
    async fn write_file_rejects_path_traversal() {
        let root = test_root();
        let tool = WriteFile;
        let result = tool
            .execute(
                ToolInput {
                    path: Some("../outside.txt".to_string()),
                    pattern: None,
                    args: Some(
                        vec![("content".to_string(), Value::String("bad".to_string()))]
                            .into_iter()
                            .collect(),
                    ),
                },
                root.path(),
            )
            .await;

        assert!(!result.success);
    }

    #[tokio::test]
    async fn write_file_rejects_oversized_content() {
        let root = test_root();
        let tool = WriteFile;
        let oversized = "x".repeat(MAX_WRITE_SIZE + 1);
        let result = tool
            .execute(
                ToolInput {
                    path: Some("big.txt".to_string()),
                    pattern: None,
                    args: Some(
                        vec![("content".to_string(), Value::String(oversized))]
                            .into_iter()
                            .collect(),
                    ),
                },
                root.path(),
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("exceeds maximum"));
    }

    #[tokio::test]
    async fn write_file_diff_summary_shows_added_lines() {
        let root = test_root();
        let path = root.path().join("diff_test.rs");
        fs::write(&path, "fn old() {}\nfn also_old() {}\n").unwrap();

        let tool = WriteFile;
        let result = tool
            .execute(
                ToolInput {
                    path: Some("diff_test.rs".to_string()),
                    pattern: None,
                    args: Some(
                        vec![(
                            "content".to_string(),
                            Value::String("fn old() {}\nfn new_func() {}\nfn also_old() {}\n".to_string()),
                        )]
                            .into_iter()
                            .collect(),
                    ),
                },
                root.path(),
            )
            .await;

        assert!(result.success, "{:?}", result.error);
        assert_eq!(
            result.output.get("lines_before").and_then(|v| v.as_u64()),
            Some(2)
        );
        assert_eq!(
            result.output.get("lines_after").and_then(|v| v.as_u64()),
            Some(3)
        );
        assert_eq!(
            result.output.get("lines_added").and_then(|v| v.as_u64()),
            Some(1)
        );
    }

    #[test]
    fn compute_diff_on_identical_lines() {
        let before = "a\nb\nc\n";
        let after = "a\nb\nc\n";
        let summary = compute_diff_summary(before, after);
        assert_eq!(summary.lines_before, 3);
        assert_eq!(summary.lines_after, 3);
        assert_eq!(summary.added, 0);
        assert_eq!(summary.removed, 0);
    }

    #[test]
    fn compute_diff_on_completely_different() {
        let before = "a\nb\n";
        let after = "c\nd\ne\n";
        let summary = compute_diff_summary(before, after);
        assert_eq!(summary.lines_before, 2);
        assert_eq!(summary.lines_after, 3);
        assert_eq!(summary.added, 3);
        assert_eq!(summary.removed, 2);
    }
}
