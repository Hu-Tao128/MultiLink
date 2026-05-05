use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::Path;

use crate::tools::filesystem::normalize_path;
use crate::tools::{Tool, ToolInput, ToolResult};

pub struct ApplyPatch;

#[async_trait]
impl Tool for ApplyPatch {
    fn name(&self) -> &'static str {
        "apply_patch"
    }
    fn description(&self) -> &'static str {
        "Apply a unified diff (patch) to a file with path validation. Returns before/after summary."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path", "patch"],
            "properties": {
                "path": {"type": "string", "description": "Relative path from project root to the file to patch"},
                "patch": {"type": "string", "description": "Unified diff content to apply"}
            }
        })
    }

    async fn execute(&self, input: ToolInput, project_root: &Path) -> ToolResult {
        let path = match input.path {
            Some(p) => p,
            None => return ToolResult::err("path is required"),
        };

        let patch_str = match input
            .args
            .as_ref()
            .and_then(|args| args.get("patch"))
            .and_then(|v| v.as_str())
        {
            Some(p) => p.to_string(),
            None => return ToolResult::err("patch is required in args"),
        };

        let target_path = match normalize_path(project_root, &path) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };

        if !target_path.exists() {
            return ToolResult::err(format!("File does not exist: {}", path));
        }

        let original_content = match std::fs::read_to_string(&target_path) {
            Ok(c) => c,
            Err(e) => return ToolResult::err(format!("Failed to read file: {}", e)),
        };

        let parsed = match parse_unified_diff(&patch_str) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("Failed to parse patch: {}", e)),
        };

        let new_content = match apply_hunks(&original_content, &parsed.hunks) {
            Ok(c) => c,
            Err(e) => return ToolResult::err(format!("Failed to apply patch: {}", e)),
        };

        if let Err(e) = std::fs::write(&target_path, &new_content) {
            return ToolResult::err(format!("Failed to write patched file: {}", e));
        }

        let original_lines: Vec<&str> = original_content.lines().collect();
        let new_lines: Vec<&str> = new_content.lines().collect();

        ToolResult::ok(json!({
            "path": target_path.to_string_lossy(),
            "hunks_applied": parsed.hunks.len(),
            "lines_before": original_lines.len(),
            "lines_after": new_lines.len(),
            "lines_added": new_lines.len().saturating_sub(original_lines.len()),
            "lines_removed": original_lines.len().saturating_sub(new_lines.len()),
        }))
    }
}

struct ParsedDiff {
    hunks: Vec<Hunk>,
}

struct Hunk {
    old_start: usize,
    old_count: usize,
    new_start: usize,
    #[allow(dead_code)]
    new_count: usize,
    lines: Vec<HunkLine>,
}

#[derive(Debug, PartialEq, Eq)]
enum HunkLine {
    Context(String),
    Removal(String),
    Addition(String),
}

fn parse_unified_diff(input: &str) -> Result<ParsedDiff, String> {
    let mut hunks = Vec::new();
    let mut in_hunk = false;
    let mut current_hunk_lines: Vec<String> = Vec::new();
    let mut hunk_header: Option<(usize, usize, usize, usize)> = None;

    for line in input.lines() {
        if line.starts_with("@@ ") {
            if in_hunk {
                let hunk = build_hunk(&current_hunk_lines, hunk_header)?;
                hunks.push(hunk);
                current_hunk_lines.clear();
            }
            hunk_header = Some(parse_hunk_header(line)?);
            in_hunk = true;
        } else if in_hunk {
            if line.starts_with(' ') || line.starts_with('+') || line.starts_with('-') {
                current_hunk_lines.push(line.to_string());
            } else if line.is_empty() {
                current_hunk_lines.push(String::new());
            }
        }
    }

    if in_hunk {
        let hunk = build_hunk(&current_hunk_lines, hunk_header)?;
        hunks.push(hunk);
    }

    if hunks.is_empty() {
        return Err("No hunks found in patch. A valid unified diff must contain at least one @@ hunk.".to_string());
    }

    Ok(ParsedDiff { hunks })
}

fn parse_hunk_header(line: &str) -> Result<(usize, usize, usize, usize), String> {
    let line = line.trim();
    let after_at = line
        .strip_prefix("@@")
        .ok_or_else(|| format!("Invalid hunk header: {}", line))?;
    let before_at = after_at
        .rfind("@@")
        .ok_or_else(|| format!("Invalid hunk header: {}", line))?;
    let range_part = after_at[..before_at].trim();

    let parts: Vec<&str> = range_part.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(format!("Invalid hunk header ranges: {}", line));
    }

    let old_range = parts[0].trim_start_matches('-');
    let new_range = parts[1].trim_start_matches('+');

    let (old_start, old_count) = parse_range(old_range)?;
    let (new_start, new_count) = parse_range(new_range)?;

    Ok((old_start, old_count, new_start, new_count))
}

fn parse_range(s: &str) -> Result<(usize, usize), String> {
    let parts: Vec<&str> = s.split(',').collect();
    let start: usize = parts[0]
        .parse()
        .map_err(|_| format!("Invalid range start: {}", parts[0]))?;
    let count: usize = if parts.len() > 1 {
        parts[1]
            .parse()
            .map_err(|_| format!("Invalid range count: {}", parts[1]))?
    } else {
        1
    };
    Ok((start, count))
}

fn build_hunk(lines: &[String], header: Option<(usize, usize, usize, usize)>) -> Result<Hunk, String> {
    let (old_start, old_count, new_start, new_count) =
        header.unwrap_or((1, 0, 1, 0));
    let mut parsed_lines = Vec::new();

    for line in lines {
        if let Some(rest) = line.strip_prefix(' ') {
            parsed_lines.push(HunkLine::Context(rest.to_string()));
        } else if let Some(rest) = line.strip_prefix('+') {
            parsed_lines.push(HunkLine::Addition(rest.to_string()));
        } else if let Some(rest) = line.strip_prefix('-') {
            parsed_lines.push(HunkLine::Removal(rest.to_string()));
        } else {
            parsed_lines.push(HunkLine::Context(line.clone()));
        }
    }

    Ok(Hunk {
        old_start,
        old_count,
        new_start,
        new_count,
        lines: parsed_lines,
    })
}

fn apply_hunks(original: &str, hunks: &[Hunk]) -> Result<String, String> {
    let mut lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();
    let mut total_added: isize = 0;

    for hunk in hunks {
        let old_start = hunk.old_start.saturating_sub(1);
        let mut expected_old_idx = old_start.wrapping_add(total_added as usize);
        let mut new_lines: Vec<String> = Vec::new();
        let mut removed_in_hunk = 0usize;
        let mut added_in_hunk = 0usize;
        let mut ok = true;

        for hl in &hunk.lines {
            match hl {
                HunkLine::Context(text) => {
                    if expected_old_idx < lines.len() && lines[expected_old_idx] == *text {
                        new_lines.push(text.clone());
                        expected_old_idx += 1;
                    } else {
                        ok = false;
                        break;
                    }
                }
                HunkLine::Removal(text) => {
                    if expected_old_idx < lines.len() && lines[expected_old_idx] == *text {
                        removed_in_hunk += 1;
                        expected_old_idx += 1;
                    } else {
                        ok = false;
                        break;
                    }
                }
                HunkLine::Addition(text) => {
                    new_lines.push(text.clone());
                    added_in_hunk += 1;
                }
            }
        }

        if !ok {
            return Err(format!(
                "Hunk at line {} does not match file content",
                hunk.old_start
            ));
        }

        let remove_start = old_start.wrapping_add(total_added as usize);
        let remove_end = remove_start + removed_in_hunk;
        if remove_end > lines.len() {
            return Err(format!(
                "Hunk removal range out of bounds: {}-{} but file has {} lines",
                remove_start,
                remove_end,
                lines.len()
            ));
        }

        let new_range_start = hunk.new_start.saturating_sub(1);
        let insert_at = if hunk.old_count == 0 && removed_in_hunk == 0 {
            new_range_start.saturating_sub(total_added as usize)
        } else {
            remove_start
        }
        .min(lines.len());

        lines.splice(insert_at..remove_end, new_lines.iter().cloned());

        let delta = (added_in_hunk as isize) - (removed_in_hunk as isize);
        total_added += delta;
    }

    Ok(lines.join("\n"))
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

    fn write_test_file(root: &Path, name: &str, content: &str) {
        fs::write(root.join(name), content).unwrap();
    }

    #[tokio::test]
    async fn apply_patch_adds_new_function() {
        let root = test_root();
        write_test_file(
            root.path(),
            "lib.rs",
            "fn existing() {\n    println!(\"hi\");\n}\n",
        );

        let patch = "\
@@ -3,1 +3,4 @@\n\
 }\n\
+\n\
+fn new_func() {\n\
+    println!(\"hello\");\n\
+}\n\
";

        let tool = ApplyPatch;
        let result = tool
            .execute(
                ToolInput {
                    path: Some("lib.rs".to_string()),
                    pattern: None,
                    args: Some(
                        vec![("patch".to_string(), Value::String(patch.to_string()))]
                            .into_iter()
                            .collect(),
                    ),
                },
                root.path(),
            )
            .await;

        assert!(result.success, "{:?}", result.error);
        let content = fs::read_to_string(root.path().join("lib.rs")).unwrap();
        assert!(content.contains("fn new_func()"));
        assert!(content.contains("fn existing()"));
    }

    #[tokio::test]
    async fn apply_patch_rejects_path_traversal() {
        let root = test_root();
        let tool = ApplyPatch;
        let result = tool
            .execute(
                ToolInput {
                    path: Some("../outside.txt".to_string()),
                    pattern: None,
                    args: Some(
                        vec![("patch".to_string(), Value::String("@@ -1 +1 @@\n-a\n+b\n".to_string()))]
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
    async fn apply_patch_rejects_absolute_path() {
        let root = test_root();
        let tool = ApplyPatch;
        let result = tool
            .execute(
                ToolInput {
                    path: Some("/etc/passwd".to_string()),
                    pattern: None,
                    args: Some(
                        vec![("patch".to_string(), Value::String("@@ -1 +1 @@\n-a\n+b\n".to_string()))]
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
    async fn apply_patch_rejects_nonexistent_file() {
        let root = test_root();
        let tool = ApplyPatch;
        let result = tool
            .execute(
                ToolInput {
                    path: Some("nope.rs".to_string()),
                    pattern: None,
                    args: Some(
                        vec![("patch".to_string(), Value::String("@@ -1 +1 @@\n-a\n+b\n".to_string()))]
                            .into_iter()
                            .collect(),
                    ),
                },
                root.path(),
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("does not exist"));
    }

    #[tokio::test]
    async fn apply_patch_rejects_empty_patch() {
        let root = test_root();
        write_test_file(root.path(), "foo.rs", "fn a() {}\n");
        let tool = ApplyPatch;
        let result = tool
            .execute(
                ToolInput {
                    path: Some("foo.rs".to_string()),
                    pattern: None,
                    args: Some(
                        vec![("patch".to_string(), Value::String("".to_string()))]
                            .into_iter()
                            .collect(),
                    ),
                },
                root.path(),
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("No hunks"));
    }

    #[tokio::test]
    async fn apply_patch_replaces_word_in_line() {
        let root = test_root();
        write_test_file(root.path(), "greeting.rs", "fn hello() {\n    println!(\"hi\");\n}\n");
        let patch = "\
@@ -2,1 +2,1 @@\n\
-    println!(\"hi\");\n\
+    println!(\"hello\");\n\
";
        let tool = ApplyPatch;
        let result = tool
            .execute(
                ToolInput {
                    path: Some("greeting.rs".to_string()),
                    pattern: None,
                    args: Some(
                        vec![("patch".to_string(), Value::String(patch.to_string()))]
                            .into_iter()
                            .collect(),
                    ),
                },
                root.path(),
            )
            .await;

        assert!(result.success, "{:?}", result.error);
        let content = fs::read_to_string(root.path().join("greeting.rs")).unwrap();
        assert!(content.contains("println!(\"hello\")"));
        assert!(!content.contains("println!(\"hi\")"));
    }

    #[tokio::test]
    async fn apply_patch_multiple_hunks() {
        let root = test_root();
        write_test_file(
            root.path(),
            "multi.rs",
            "fn first() {\n    println!(\"1\");\n}\n\nfn second() {\n    println!(\"2\");\n}\n",
        );
        let patch = "\
@@ -2,1 +2,1 @@\n\
-    println!(\"1\");\n\
+    println!(\"one\");\n\
@@ -6,1 +6,1 @@\n\
-    println!(\"2\");\n\
+    println!(\"two\");\n\
";
        let tool = ApplyPatch;
        let result = tool
            .execute(
                ToolInput {
                    path: Some("multi.rs".to_string()),
                    pattern: None,
                    args: Some(
                        vec![("patch".to_string(), Value::String(patch.to_string()))]
                            .into_iter()
                            .collect(),
                    ),
                },
                root.path(),
            )
            .await;

        assert!(result.success, "{:?}", result.error);
        let content = fs::read_to_string(root.path().join("multi.rs")).unwrap();
        assert!(content.contains("println!(\"one\")"));
        assert!(content.contains("println!(\"two\")"));
        assert!(!content.contains("println!(\"1\")"));
        assert!(!content.contains("println!(\"2\")"));
    }

    #[test]
    fn parse_unified_diff_single_hunk() {
        let input = "\
@@ -1,3 +1,4 @@\n\
 a\n\
-b\n\
+c\n\
 d\n\
";
        let parsed = parse_unified_diff(input).unwrap();
        assert_eq!(parsed.hunks.len(), 1);
        assert_eq!(parsed.hunks[0].old_start, 1);
        assert_eq!(parsed.hunks[0].new_start, 1);
    }

    #[test]
    fn parse_unified_diff_empty_fails() {
        let result = parse_unified_diff("");
        assert!(result.is_err());
    }
}
