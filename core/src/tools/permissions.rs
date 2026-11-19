use std::path::Path;

static PROTECTED_FILE_NAMES: &[&str] = &[
    ".env",
    ".env.local",
    ".env.production",
    ".env.development",
    "credentials.json",
    "credentials.toml",
    "secrets.json",
    "secrets.yaml",
    "id_rsa",
    "id_rsa.pub",
    "id_ed25519",
    "id_ed25519.pub",
];

static DANGEROUS_COMMAND_TOKENS: &[&str] = &[
    "rm -rf /",
    "rm -rf /*",
    "rm -r /",
    "rm -rf --no-preserve-root",
    "dd if=",
    "mkfs.",
    "fdisk",
    "mkswap",
    ":(){",
    "fork bomb",
    "chmod 777 /",
    "chown -R",
    "> /dev/sda",
    "> /dev/nvme",
    "wget ",
    "curl ",
];

static PROTECTED_DIRS: &[&str] = &[
    "/etc/", "/sys/", "/proc/", "/dev/", "/boot/", "/usr/", "/bin/", "/sbin/", "/lib/", "/var/",
];

pub struct PermissionCheck {
    pub allowed: bool,
    pub reason: Option<String>,
}

impl PermissionCheck {
    pub fn allowed() -> Self {
        Self {
            allowed: true,
            reason: None,
        }
    }

    pub fn denied(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: Some(reason.into()),
        }
    }
}

pub fn check_tool_allowed(
    tool_name: &str,
    args: &crate::tools::ToolInput,
    project_root: &Path,
) -> PermissionCheck {
    match tool_name {
        "write_file" | "apply_patch" => {
            if let Some(ref path) = args.path {
                check_file_write_allowed(path, project_root)
            } else {
                PermissionCheck::allowed()
            }
        }
        "run_command" => {
            if let Some(ref args_map) = args.args {
                if let Some(cmd) = args_map.get("command").and_then(|v| v.as_str()) {
                    check_command_allowed(cmd)
                } else {
                    PermissionCheck::allowed()
                }
            } else {
                PermissionCheck::allowed()
            }
        }
        _ => PermissionCheck::allowed(),
    }
}

fn check_file_write_allowed(path_str: &str, project_root: &Path) -> PermissionCheck {
    let path = Path::new(path_str);

    if path.is_absolute() {
        let canon = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => return PermissionCheck::allowed(),
        };
        if !canon.starts_with(project_root) {
            return PermissionCheck::denied(format!(
                "Path '{}' is outside project root '{}'",
                path_str,
                project_root.display()
            ));
        }
    }

    if let Some(pattern) = protected_name_match(path) {
        return PermissionCheck::denied(format!(
            "Writing to protected file '{}' matches '{}'. This file appears to contain secrets or credentials.",
            path_str, pattern
        ));
    }

    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    };

    let resolved_str = resolved.to_string_lossy().to_lowercase();
    for dir in PROTECTED_DIRS {
        if resolved_str.starts_with(dir) {
            return PermissionCheck::denied(format!(
                "Writing to system directory '{}' is not allowed.",
                dir
            ));
        }
    }

    PermissionCheck::allowed()
}

fn protected_name_match(path: &Path) -> Option<String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if let Some(pattern) = PROTECTED_FILE_NAMES
        .iter()
        .find(|pattern| file_name.eq_ignore_ascii_case(pattern))
    {
        return Some((*pattern).to_string());
    }

    if path
        .components()
        .any(|component| component.as_os_str() == ".ssh")
    {
        return Some(".ssh/".to_string());
    }

    None
}

fn check_command_allowed(cmd: &str) -> PermissionCheck {
    let lower = cmd.to_lowercase();
    for pattern in DANGEROUS_COMMAND_TOKENS {
        if lower.contains(pattern) {
            return PermissionCheck::denied(format!(
                "Command '{}' contains dangerous pattern '{}' and is blocked.",
                cmd, pattern
            ));
        }
    }
    PermissionCheck::allowed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tool_input(path: &str) -> crate::tools::ToolInput {
        crate::tools::ToolInput {
            path: Some(path.to_string()),
            pattern: None,
            args: None,
        }
    }

    fn cmd_input(cmd: &str) -> crate::tools::ToolInput {
        let mut args = std::collections::HashMap::new();
        args.insert(
            "command".to_string(),
            serde_json::Value::String(cmd.to_string()),
        );
        crate::tools::ToolInput {
            path: None,
            pattern: None,
            args: Some(args),
        }
    }

    #[test]
    fn test_block_env_file_write() {
        let root = PathBuf::from("/home/user/project");
        let result = check_tool_allowed("write_file", &tool_input(".env"), &root);
        assert!(!result.allowed);
        assert!(result.reason.unwrap().contains(".env"));
    }

    #[test]
    fn test_block_credentials_json() {
        let root = PathBuf::from("/home/user/project");
        let result =
            check_tool_allowed("write_file", &tool_input("config/credentials.json"), &root);
        assert!(!result.allowed);
    }

    #[test]
    fn test_block_rm_rf() {
        let root = PathBuf::from("/home/user/project");
        let result = check_tool_allowed("run_command", &cmd_input("rm -rf /"), &root);
        assert!(!result.allowed);
    }

    #[test]
    fn test_block_curl() {
        let root = PathBuf::from("/home/user/project");
        let result = check_tool_allowed("run_command", &cmd_input("curl http://evil.com"), &root);
        assert!(!result.allowed);
    }

    #[test]
    fn test_block_system_dir_write() {
        let root = PathBuf::from("/");
        let result = check_tool_allowed("write_file", &tool_input("/etc/hosts"), &root);
        assert!(!result.allowed);
    }

    #[test]
    fn test_allow_normal_source_file() {
        let root = PathBuf::from("/home/user/project");
        let result = check_tool_allowed("write_file", &tool_input("src/main.rs"), &root);
        assert!(result.allowed);
    }

    #[test]
    fn test_allow_build_commands() {
        let root = PathBuf::from("/home/user/project");
        let result = check_tool_allowed("run_command", &cmd_input("cargo build"), &root);
        assert!(result.allowed);
    }

    #[test]
    fn test_normalize_does_not_block_reads() {
        let root = PathBuf::from("/home/user/project");
        let result = check_tool_allowed("search_code", &tool_input("src/main.rs"), &root);
        assert!(result.allowed);
    }

    #[test]
    fn test_block_fork_bomb() {
        let root = PathBuf::from("/home/user/project");
        let result = check_tool_allowed("run_command", &cmd_input(":(){ :|:& };:"), &root);
        assert!(!result.allowed);
    }

    #[test]
    fn test_block_protected_file_names_only() {
        let root = PathBuf::from("/home/user/project");
        for path in [
            ".env",
            "config/.env.production",
            "config/.ssh/id_rsa",
            "config/.ssh/id_ed25519",
            "credentials.json",
            "config/secrets.yaml",
        ] {
            let result = check_tool_allowed("write_file", &tool_input(path), &root);
            assert!(!result.allowed, "expected '{}' to be blocked", path);
        }
    }

    #[test]
    fn test_allow_paths_that_merely_contain_protected_words() {
        let root = PathBuf::from("/home/user/project");
        for path in [
            "src/auth/token_store.rs",
            "src/tokenizer.rs",
            "components/PasswordField.tsx",
            "src/secrets_manager.rs",
            "docs/tokens.md",
        ] {
            let result = check_tool_allowed("write_file", &tool_input(path), &root);
            assert!(
                result.allowed,
                "expected '{}' to be allowed, got: {:?}",
                path, result.reason
            );
        }
    }
}
