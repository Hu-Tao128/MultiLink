use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Command;

use crate::tools::{Tool, ToolInput, ToolResult};

pub struct SystemVersion;

impl SystemVersion {
    fn get_version(&self, binary: &str, args: &[&str]) -> Option<Value> {
        let output = Command::new(binary).args(args).output().ok()?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if version.is_empty() {
                return Some(json!(String::from_utf8_lossy(&output.stderr)
                    .trim()
                    .to_string()));
            }
            Some(json!(version
                .lines()
                .next()
                .unwrap_or(&version)
                .to_string()))
        } else {
            None
        }
    }

    fn find_binary(&self, name: &str) -> Option<String> {
        which::which(name)
            .ok()
            .map(|p| p.to_string_lossy().to_string())
    }
}

#[async_trait]
impl Tool for SystemVersion {
    fn name(&self) -> &'static str {
        "system_version"
    }
    fn description(&self) -> &'static str {
        "Detect versions of Node.js, Java, Python, Rust, Cargo, Git"
    }

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
            .unwrap_or_else(|| {
                vec![
                    "node".to_string(),
                    "java".to_string(),
                    "python".to_string(),
                    "rust".to_string(),
                    "cargo".to_string(),
                    "git".to_string(),
                ]
            });

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

/// Read-only host inspection: OS, kernel, CPU, memory and root filesystem.
/// Unlike `system_version`, this does not depend on which developer tools are
/// installed, so it can answer questions like "what can this machine do?".
pub struct SystemInfo;

impl SystemInfo {
    fn command_output(binary: &str, args: &[&str]) -> Option<String> {
        let output = Command::new(binary).args(args).output().ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }

    fn uname(flag: &str) -> Option<String> {
        Self::command_output("uname", &[flag])
            .map(|value| value.lines().next().unwrap_or(&value).trim().to_string())
            .filter(|value| !value.is_empty())
    }

    #[cfg(target_os = "linux")]
    fn os_name() -> Option<String> {
        std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|content| {
                content.lines().find_map(|line| {
                    line.strip_prefix("PRETTY_NAME=")
                        .map(|value| value.trim_matches('"').trim().to_string())
                })
            })
            .filter(|value| !value.is_empty())
            .or_else(|| Self::uname("-s"))
    }

    #[cfg(not(target_os = "linux"))]
    fn os_name() -> Option<String> {
        Self::uname("-s")
    }

    #[cfg(target_os = "linux")]
    fn cpu_info() -> (Option<String>, Option<usize>) {
        let Ok(content) = std::fs::read_to_string("/proc/cpuinfo") else {
            return (None, None);
        };
        let model = content
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once(':')?;
                if key.trim() == "model name" {
                    Some(value.trim().to_string())
                } else {
                    None
                }
            })
            .filter(|value| !value.is_empty());
        let cores = content
            .lines()
            .filter(|line| line.trim_start().starts_with("processor"))
            .count();
        (model, (cores > 0).then_some(cores))
    }

    #[cfg(not(target_os = "linux"))]
    fn cpu_info() -> (Option<String>, Option<usize>) {
        let model = Self::command_output("sysctl", &["-n", "machdep.cpu.brand_string"]);
        let cores = Self::command_output("sysctl", &["-n", "hw.ncpu"])
            .and_then(|value| value.trim().parse::<usize>().ok());
        (model, cores)
    }

    #[cfg(target_os = "linux")]
    fn memory_info() -> Option<(u64, u64)> {
        let content = std::fs::read_to_string("/proc/meminfo").ok()?;
        let mut total_kb = 0u64;
        let mut available_kb = 0u64;
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                total_kb = parse_kb(rest)?;
            } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
                available_kb = parse_kb(rest)?;
            }
        }
        (total_kb > 0).then_some((total_kb, available_kb))
    }

    #[cfg(not(target_os = "linux"))]
    fn memory_info() -> Option<(u64, u64)> {
        let total = Self::command_output("sysctl", &["-n", "hw.memsize"])
            .and_then(|value| value.trim().parse::<u64>().ok())
            .map(|bytes| bytes / 1024)?;
        Some((total, 0))
    }

    fn root_disk() -> Option<String> {
        Self::command_output("df", &["-h", "/"])
            .and_then(|output| output.lines().nth(1).map(|line| line.trim().to_string()))
            .filter(|line| !line.is_empty())
    }

    fn hostname() -> Option<String> {
        std::env::var("HOSTNAME")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| Self::command_output("hostname", &[]))
            .map(|value| value.trim().to_string())
    }
}

fn parse_kb(rest: &str) -> Option<u64> {
    rest.split_whitespace().next()?.parse::<u64>().ok()
}

fn humanize_kb(kb: u64) -> String {
    let mut value = kb as f64;
    for unit in ["KiB", "MiB", "GiB", "TiB"] {
        if value < 1024.0 {
            return format!("{:.1} {}", value, unit);
        }
        value /= 1024.0;
    }
    format!("{:.1} PiB", value)
}

#[async_trait]
impl Tool for SystemInfo {
    fn name(&self) -> &'static str {
        "system_info"
    }

    fn description(&self) -> &'static str {
        "Inspect the host OS, CPU, memory and root filesystem (read-only)"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _: ToolInput, _: &std::path::Path) -> ToolResult {
        let mut info = serde_json::Map::new();

        if let Some(os) = Self::os_name() {
            info.insert("os".to_string(), json!(os));
        }
        if let Some(kernel) = Self::uname("-r") {
            info.insert("kernel".to_string(), json!(kernel));
        }
        if let Some(arch) = Self::uname("-m") {
            info.insert("arch".to_string(), json!(arch));
        }
        if let Some(hostname) = Self::hostname() {
            info.insert("hostname".to_string(), json!(hostname));
        }

        let (cpu_model, cpu_cores) = Self::cpu_info();
        let mut cpu = serde_json::Map::new();
        if let Some(model) = cpu_model {
            cpu.insert("model".to_string(), json!(model));
        }
        if let Some(cores) = cpu_cores {
            cpu.insert("cores".to_string(), json!(cores));
        }
        if !cpu.is_empty() {
            info.insert("cpu".to_string(), Value::Object(cpu));
        }

        if let Some((total_kb, available_kb)) = Self::memory_info() {
            let mut memory = serde_json::Map::new();
            memory.insert("total".to_string(), json!(humanize_kb(total_kb)));
            if available_kb > 0 {
                memory.insert("available".to_string(), json!(humanize_kb(available_kb)));
            }
            info.insert("memory".to_string(), Value::Object(memory));
        }

        if let Some(root_disk) = Self::root_disk() {
            info.insert("root_filesystem".to_string(), json!(root_disk));
        }

        ToolResult::ok(Value::Object(info))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_system_version() {
        let tool = SystemVersion;
        let result = tool
            .execute(ToolInput::default(), &std::path::PathBuf::from("/"))
            .await;
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_system_info_reports_host_data() {
        let tool = SystemInfo;
        let result = tool
            .execute(ToolInput::default(), &std::path::PathBuf::from("/"))
            .await;
        assert!(result.success);
        let output = result.output;
        assert!(output.is_object());
        #[cfg(target_os = "linux")]
        assert!(output.get("os").is_some());
    }
}
