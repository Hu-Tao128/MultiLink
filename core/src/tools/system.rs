use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Command;

use crate::tools::{ToolInput, ToolResult, Tool};

pub struct SystemVersion;

impl SystemVersion {
    fn get_version(&self, binary: &str, args: &[&str]) -> Option<Value> {
        let output = Command::new(binary)
            .args(args)
            .output()
            .ok()?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if version.is_empty() {
                return Some(json!(String::from_utf8_lossy(&output.stderr).trim().to_string()));
            }
            Some(json!(version.lines().next().unwrap_or(&version).to_string()))
        } else {
            None
        }
    }

    fn find_binary(&self, name: &str) -> Option<String> {
        which::which(name).ok().map(|p| p.to_string_lossy().to_string())
    }
}

#[async_trait]
impl Tool for SystemVersion {
    fn name(&self) -> &'static str { "system_version" }
    fn description(&self) -> &'static str { "Detect versions of Node.js, Java, Python, Rust, Cargo, Git" }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "tools": {
                    "type": "array",
                    "items": {"type": "string", "enum": ["node", "java", "python", "rust", "cargo", "git"]},
                    "description": "List of tools to check (default: all)"
                }
            }
        })
    }

    async fn execute(&self, input: ToolInput, _: &std::path::Path) -> ToolResult {
        let requested_tools: Vec<String> = input
            .args
            .as_ref()
            .and_then(|args| args.get("tools"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| vec![
                "node".to_string(),
                "java".to_string(),
                "python".to_string(),
                "rust".to_string(),
                "cargo".to_string(),
                "git".to_string(),
            ]);

        let mut results = Vec::new();

        for tool in requested_tools {
            let (binary, args, name) = match tool.as_str() {
                "node" => ("node", ["--version"], "Node.js"),
                "java" => ("java", ["-version"], "Java"),
                "python" => ("python3", ["--version"], "Python"),
                "rust" => ("rustc", ["--version"], "Rust"),
                "cargo" => ("cargo", ["--version"], "Cargo"),
                "git" => ("git", ["--version"], "Git"),
                _ => continue,
            };

            if let Some(version) = self.get_version(binary, &args) {
                let path = self.find_binary(binary);
                results.push(json!({
                    "name": name,
                    "version": version,
                    "path": path
                }));
            }
        }

        ToolResult::ok(json!({
            "tools": results,
            "count": results.len()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_system_version() {
        let tool = SystemVersion;
        let result = tool.execute(ToolInput::default(), &std::path::PathBuf::from("/")).await;
        assert!(result.success);
    }
}