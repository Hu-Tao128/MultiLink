use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

use crate::tools::{Tool, ToolInput, ToolResult};

const DEFAULT_TIMEOUT_SECS: u64 = 120;

static ALWAYS_ALLOWED: &[&str] = &[
    "cargo build",
    "cargo test",
    "cargo clippy -- -D warnings",
    "cargo fmt --all",
    "cargo check",
    "npm test",
    "npm run build",
    "npm run lint",
    "npm install",
    "npm run typecheck",
    "pytest",
    "make",
    "cmake -S . -B build && cmake --build build",
    "flutter test",
    "flutter analyze",
    "go test",
    "go build",
    "python -m pytest",
    "cargo test --manifest-path core/Cargo.toml",
    "cargo clippy --manifest-path core/Cargo.toml -- -D warnings",
    "cargo fmt --manifest-path core/Cargo.toml --all",
    "cargo build --manifest-path core/Cargo.toml",
];

pub struct RunCommand;

#[async_trait]
impl Tool for RunCommand {
    fn name(&self) -> &'static str {
        "run_command"
    }
    fn description(&self) -> &'static str {
        "Execute an allowlisted validation or build command. Commands must be in the allowlist or detected by /init in MULTILINK.md."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {"type": "string", "description": "Command to execute (must be allowlisted)"},
                "timeout_secs": {"type": "integer", "description": "Timeout in seconds (default 120)"},
                "workdir": {"type": "string", "description": "Optional relative working directory"}
            }
        })
    }

    async fn execute(&self, input: ToolInput, project_root: &Path) -> ToolResult {
        let command = match input
            .args
            .as_ref()
            .and_then(|args| args.get("command"))
            .and_then(|v| v.as_str())
        {
            Some(c) => c.trim().to_string(),
            None => return ToolResult::err("command is required in args"),
        };

        if command.is_empty() {
            return ToolResult::err("command cannot be empty");
        }

        let timeout_secs = input
            .args
            .as_ref()
            .and_then(|args| args.get("timeout_secs"))
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_TIMEOUT_SECS);

        let workdir = match input
            .args
            .as_ref()
            .and_then(|args| args.get("workdir"))
            .and_then(|v| v.as_str())
        {
            Some(rel) if !rel.is_empty() => {
                let full = project_root.join(rel);
                if !full.exists() {
                    return ToolResult::err(format!("Working directory does not exist: {}", rel));
                }
                full
            }
            _ => project_root.to_path_buf(),
        };

        if !is_command_allowed(&command, project_root) {
            return ToolResult::err(format!(
                "Command not allowlisted: {}\n\nTo allow this command, add it to MULTILINK.md via /init, or use one of the built-in allowed commands.",
                command
            ));
        }

        let timeout = Duration::from_secs(timeout_secs);

        let shell_cmd = if cfg!(target_os = "windows") {
            "cmd"
        } else {
            "sh"
        };
        let shell_arg = if cfg!(target_os = "windows") {
            "/C"
        } else {
            "-c"
        };

        let result = tokio::time::timeout(timeout, async {
            Command::new(shell_cmd)
                .arg(shell_arg)
                .arg(&command)
                .current_dir(&workdir)
                .output()
                .await
        })
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);

                let response = json!({
                    "command": command,
                    "exit_code": exit_code,
                    "stdout": stdout,
                    "stderr": stderr,
                    "timed_out": false,
                });

                ToolResult::ok(response)
            }
            Ok(Err(e)) => ToolResult::err(format!("Failed to execute command: {}", e)),
            Err(_) => ToolResult::err(format!(
                "Command timed out after {} seconds: {}",
                timeout_secs, command
            )),
        }
    }
}

fn is_command_allowed(command: &str, project_root: &Path) -> bool {
    let trimmed = command.trim();

    if ALWAYS_ALLOWED.contains(&trimmed) {
        return true;
    }

    if let Some(project_allowed) = load_multilink_commands(project_root) {
        if project_allowed.iter().any(|ac| trimmed == ac.as_str()) {
            return true;
        }
    }

    false
}

fn load_multilink_commands(project_root: &Path) -> Option<Vec<String>> {
    let multilink_path = project_root.join("MULTILINK.md");
    if !multilink_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(multilink_path).ok()?;
    extract_json_block_commands(&content)
}

pub fn extract_json_block_commands(content: &str) -> Option<Vec<String>> {
    let start_marker = "```json";
    let end_marker = "```";

    let start = content.find(start_marker)?;
    let after_start = content[start + start_marker.len()..].trim_start();
    let end = after_start.find(end_marker)?;
    let json_str = &after_start[..end];

    let parsed: Value = serde_json::from_str(json_str).ok()?;

    let cmds = parsed
        .get("validation_commands")?
        .as_array()?
        .iter()
        .filter_map(|v| {
            v.as_object()
                .and_then(|obj| obj.get("command"))
                .and_then(|c| c.as_str())
                .map(|s| s.to_string())
        })
        .collect::<Vec<_>>();

    if cmds.is_empty() {
        None
    } else {
        Some(cmds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn test_root() -> &'static tempfile::TempDir {
        static ROOT: OnceLock<tempfile::TempDir> = OnceLock::new();
        ROOT.get_or_init(|| tempfile::tempdir().expect("tempdir"))
    }

    #[tokio::test]
    async fn run_command_rejects_empty_command() {
        let root = test_root();
        let tool = RunCommand;
        let result = tool
            .execute(
                ToolInput {
                    path: None,
                    pattern: None,
                    args: Some(
                        vec![("command".to_string(), Value::String("".to_string()))]
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
    async fn run_command_rejects_disallowed_command() {
        let root = test_root();
        let tool = RunCommand;
        let result = tool
            .execute(
                ToolInput {
                    path: None,
                    pattern: None,
                    args: Some(
                        vec![("command".to_string(), Value::String("rm -rf /".to_string()))]
                            .into_iter()
                            .collect(),
                    ),
                },
                root.path(),
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("not allowlisted"));
    }

    #[tokio::test]
    async fn run_command_allows_builtin_commands() {
        let root = test_root();
        let tool = RunCommand;
        let result = tool
            .execute(
                ToolInput {
                    path: None,
                    pattern: None,
                    args: Some(
                        vec![("command".to_string(), Value::String("cargo build".to_string()))]
                            .into_iter()
                            .collect(),
                    ),
                },
                root.path(),
            )
            .await;

        assert!(result.success, "{:?}", result.error);
        // cargo build in an empty dir may fail (exit code != 0), but the tool itself ran
        assert!(
            result.output.get("exit_code").and_then(|v| v.as_i64()).is_some(),
            "exit_code should be present"
        );
    }

    #[test]
    fn extract_json_block_finds_commands() {
        let content = r#"
Some text

```json
{
    "validation_commands": [
        {"name": "Build", "command": "cargo build"},
        {"name": "Test", "command": "cargo test"}
    ]
}
```

More text
"#;
        let commands = extract_json_block_commands(content).unwrap();
        assert!(commands.contains(&"cargo build".to_string()));
        assert!(commands.contains(&"cargo test".to_string()));
    }

    #[test]
    fn extract_json_block_none_when_missing() {
        let content = "no json block here";
        assert!(extract_json_block_commands(content).is_none());
    }

    #[test]
    fn extract_json_block_none_when_empty_commands() {
        let content = r#"
```json
{
    "validation_commands": []
}
```
"#;
        assert!(extract_json_block_commands(content).is_none());
    }

    #[test]
    fn allowlist_matches_builtin_commands() {
        assert!(is_command_str_allowed("cargo test"));
        assert!(is_command_str_allowed("cargo build"));
        assert!(is_command_str_allowed("npm test"));
        assert!(!is_command_str_allowed("rm -rf /"));
        assert!(!is_command_str_allowed("sudo rm -rf"));
    }

    fn is_command_str_allowed(cmd: &str) -> bool {
        let temp = tempfile::tempdir().unwrap();
        is_command_allowed(cmd, temp.path())
    }
}
