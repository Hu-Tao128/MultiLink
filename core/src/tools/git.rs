use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::tools::{Tool, ToolInput, ToolResult};

const DEFAULT_MAX_BYTES: usize = 24_000;
const MAX_ALLOWED_BYTES: usize = 200_000;

fn normalize_optional_path(
    project_root: &Path,
    path: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let relative = Path::new(path);
    if relative.is_absolute() {
        return Err("Absolute paths are not allowed".to_string());
    }
    if relative.components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return Err("Path escape detected".to_string());
    }

    Ok(Some(project_root.join(relative)))
}

fn relative_path_for_git(project_root: &Path, full_path: &Path) -> Result<String, String> {
    let relative = full_path
        .strip_prefix(project_root)
        .map_err(|_| "Path escape detected".to_string())?;
    Ok(relative.to_string_lossy().to_string())
}

fn max_bytes(input: &ToolInput) -> usize {
    input
        .args
        .as_ref()
        .and_then(|args| args.get("max_bytes"))
        .and_then(|value| value.as_u64())
        .map(|value| value as usize)
        .unwrap_or(DEFAULT_MAX_BYTES)
        .min(MAX_ALLOWED_BYTES)
}

fn truncate_output(text: String, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text, false);
    }

    let mut boundary = max_bytes;
    while !text.is_char_boundary(boundary) {
        boundary = boundary.saturating_sub(1);
    }
    (text[..boundary].to_string(), true)
}

fn run_git(project_root: &Path, args: &[&str]) -> Result<(String, String), String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        let message = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        return Err(if message.is_empty() {
            "git command failed".to_string()
        } else {
            message
        });
    }

    Ok((stdout, stderr))
}

pub struct GitStatus;

#[async_trait]
impl Tool for GitStatus {
    fn name(&self) -> &'static str {
        "git_status"
    }

    fn description(&self) -> &'static str {
        "Read git status for the project without modifying the repository"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "max_bytes": {
                    "type": "integer",
                    "description": "Maximum output bytes to return (default 24000)"
                }
            }
        })
    }

    async fn execute(&self, input: ToolInput, project_root: &Path) -> ToolResult {
        let max_bytes = max_bytes(&input);
        let (stdout, _) = match run_git(
            project_root,
            &["status", "--short", "--branch", "--untracked-files=all"],
        ) {
            Ok(output) => output,
            Err(err) => return ToolResult::err(err),
        };
        let (status, truncated) = truncate_output(stdout, max_bytes);

        ToolResult::ok(json!({
            "status": status,
            "truncated": truncated,
            "max_bytes": max_bytes
        }))
    }
}

pub struct GitDiff;

#[async_trait]
impl Tool for GitDiff {
    fn name(&self) -> &'static str {
        "git_diff"
    }

    fn description(&self) -> &'static str {
        "Read git diff for the project or a relative path without modifying the repository"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Optional relative path to diff"
                },
                "max_bytes": {
                    "type": "integer",
                    "description": "Maximum output bytes to return (default 24000)"
                }
            }
        })
    }

    async fn execute(&self, input: ToolInput, project_root: &Path) -> ToolResult {
        let max_bytes = max_bytes(&input);
        let requested_path = input.path.as_deref().or_else(|| {
            input
                .args
                .as_ref()
                .and_then(|args| args.get("path"))
                .and_then(|value| value.as_str())
        });

        let normalized_path = match normalize_optional_path(project_root, requested_path) {
            Ok(path) => path,
            Err(err) => return ToolResult::err(err),
        };

        let mut owned_args = vec!["diff".to_string(), "--".to_string()];
        if let Some(path) = normalized_path.as_ref() {
            match relative_path_for_git(project_root, path) {
                Ok(relative) => owned_args.push(relative),
                Err(err) => return ToolResult::err(err),
            }
        }
        let arg_refs: Vec<&str> = owned_args.iter().map(String::as_str).collect();

        let (stdout, _) = match run_git(project_root, &arg_refs) {
            Ok(output) => output,
            Err(err) => return ToolResult::err(err),
        };
        let (diff, truncated) = truncate_output(stdout, max_bytes);

        ToolResult::ok(json!({
            "diff": diff,
            "path": requested_path,
            "truncated": truncated,
            "max_bytes": max_bytes
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn run_git_test(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_repo() -> tempfile::TempDir {
        let temp = tempfile::tempdir().expect("tempdir");
        run_git_test(temp.path(), &["init"]);
        run_git_test(
            temp.path(),
            &["config", "user.email", "test@example.invalid"],
        );
        run_git_test(temp.path(), &["config", "user.name", "Test User"]);
        fs::write(temp.path().join("tracked.txt"), "one\n").expect("write");
        run_git_test(temp.path(), &["add", "tracked.txt"]);
        run_git_test(temp.path(), &["commit", "-m", "initial"]);
        temp
    }

    #[tokio::test]
    async fn git_status_reports_untracked_files() {
        let temp = init_repo();
        fs::write(temp.path().join("new.txt"), "new\n").expect("write");

        let result = GitStatus.execute(ToolInput::default(), temp.path()).await;

        assert!(result.success, "{:?}", result.error);
        let status = result
            .output
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap();
        assert!(status.contains("?? new.txt"), "status was: {}", status);
    }

    #[tokio::test]
    async fn git_diff_returns_worktree_changes() {
        let temp = init_repo();
        fs::write(temp.path().join("tracked.txt"), "one\ntwo\n").expect("write");

        let result = GitDiff.execute(ToolInput::default(), temp.path()).await;

        assert!(result.success, "{:?}", result.error);
        let diff = result.output.get("diff").and_then(|v| v.as_str()).unwrap();
        assert!(diff.contains("+two"), "diff was: {}", diff);
    }

    #[tokio::test]
    async fn git_diff_rejects_path_traversal() {
        let temp = init_repo();
        let result = GitDiff
            .execute(
                ToolInput {
                    path: Some("../outside.txt".to_string()),
                    pattern: None,
                    args: None,
                },
                temp.path(),
            )
            .await;

        assert!(!result.success);
        assert!(result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("Path escape"));
    }
}
