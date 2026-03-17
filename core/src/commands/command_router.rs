use std::path::PathBuf;

use crate::commands::init_command::InitCommand;

pub enum ChatCommand {
    Init(InitCommand),
    Doctor,
    WriteFile {
        relative_path: String,
        content: String,
    },
}

pub fn route_command(prompt: &str) -> Option<ChatCommand> {
    let first = prompt.lines().next()?.trim();
    let parts: Vec<&str> = first.split_whitespace().collect();
    let command = *parts.first()?;

    match command {
        "/init" => {
            let smart = prompt.contains("smart") || prompt.contains("--smart");
            let force = prompt.contains("--force") || prompt.contains("-f");
            let merge = prompt.contains("--merge");

            Some(ChatCommand::Init(InitCommand {
                force,
                smart,
                merge,
            }))
        }
        "/doctor" => Some(ChatCommand::Doctor),
        "/write-file" => {
            let relative_path = parts.get(1)?.trim().to_string();
            if relative_path.is_empty() {
                return None;
            }

            let mut content = prompt.lines().skip(1).collect::<Vec<_>>().join("\n");
            if let Some(stripped) = strip_fence(&content) {
                content = stripped;
            }

            Some(ChatCommand::WriteFile {
                relative_path,
                content,
            })
        }
        _ => None,
    }
}

fn strip_fence(content: &str) -> Option<String> {
    let trimmed = content.trim();
    if trimmed.starts_with("```") {
        let first_newline = trimmed.find('\n')?;
        let last_fence = trimmed.rfind("```")?;
        if last_fence > first_newline {
            return Some(trimmed[first_newline + 1..last_fence].trim().to_string());
        }
    }
    None
}

pub fn get_project_root(session_root: Option<String>, session_id: &str) -> PathBuf {
    if let Some(root) = session_root {
        let path = PathBuf::from(&root);
        if path.is_dir() {
            return path;
        }
    }

    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
