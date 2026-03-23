use std::path::PathBuf;

use crate::commands::init_command::InitCommand;

pub enum ChatCommand {
    Init(InitCommand),
    Doctor {
        security: bool,
    },
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
            let strict = prompt.contains("--strict");

            Some(ChatCommand::Init(InitCommand {
                force,
                smart,
                merge,
                strict,
            }))
        }
        "/doctor" => {
            let security = prompt.contains("--security") || prompt.contains("security");
            Some(ChatCommand::Doctor { security })
        }
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
    resolve_project_root(session_root, session_id, RootPolicy::PreferSession)
}

pub fn get_project_root_for_init(session_root: Option<String>, session_id: &str) -> PathBuf {
    resolve_project_root(session_root, session_id, RootPolicy::PreferCwdOnMismatch)
}

#[derive(Clone, Copy)]
enum RootPolicy {
    PreferSession,
    PreferCwdOnMismatch,
}

fn resolve_project_root(
    session_root: Option<String>,
    session_id: &str,
    policy: RootPolicy,
) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    resolve_project_root_with_cwd(session_root, session_id, policy, cwd)
}

fn resolve_project_root_with_cwd(
    session_root: Option<String>,
    session_id: &str,
    policy: RootPolicy,
    cwd: PathBuf,
) -> PathBuf {
    let cwd_canonical = canonical_or_raw(cwd);

    if let Some(root) = session_root {
        let session_path = PathBuf::from(&root);
        if session_path.is_dir() {
            let session_canonical = canonical_or_raw(session_path);
            if matches!(policy, RootPolicy::PreferCwdOnMismatch)
                && session_canonical != cwd_canonical
            {
                let cwd_score = project_marker_score(&cwd_canonical);
                let session_score = project_marker_score(&session_canonical);

                // Guard conservador: solo usar CWD cuando el root de sesión no parece
                // un proyecto válido y el CWD sí tiene señales de proyecto.
                if session_score == 0 && cwd_score > 0 {
                    eprintln!(
                        "[init root guard] session={} action=prefer_cwd reason=mismatch cwd={} session={} cwd_score={} session_score={}",
                        session_id,
                        cwd_canonical.display(),
                        session_canonical.display(),
                        cwd_score,
                        session_score
                    );
                    return cwd_canonical;
                }
            }

            eprintln!(
                "[init root] session={} source=session canonical={}",
                session_id,
                session_canonical.display()
            );
            return session_canonical;
        }
    }

    eprintln!(
        "[init root] session={} source=cwd canonical={}",
        session_id,
        cwd_canonical.display()
    );
    cwd_canonical
}

fn canonical_or_raw(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

fn project_marker_score(path: &std::path::Path) -> u8 {
    let markers = [
        ".git",
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "requirements.txt",
        "go.mod",
        "CMakeLists.txt",
        "pubspec.yaml",
    ];

    markers
        .iter()
        .filter(|marker| path.join(marker).exists())
        .count() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_root(name: &str) -> PathBuf {
        let base = std::env::temp_dir()
            .join("multilink_root_guard_tests")
            .join(name);
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("create temp root");
        base
    }

    #[test]
    fn parses_init_strict_flag() {
        let cmd = route_command("/init --strict --force");
        match cmd {
            Some(ChatCommand::Init(init)) => {
                assert!(init.strict);
                assert!(init.force);
            }
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn parses_doctor_security_flag() {
        let cmd = route_command("/doctor --security");
        match cmd {
            Some(ChatCommand::Doctor { security }) => assert!(security),
            _ => panic!("expected doctor command with security"),
        }
    }

    #[test]
    fn init_root_guard_keeps_session_when_both_paths_look_like_projects() {
        let cwd = temp_root("cwd_project");
        let session = temp_root("session_project");

        fs::write(cwd.join("package.json"), "{}\n").expect("write package json");
        fs::write(
            session.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .expect("write cargo toml");

        let selected = resolve_project_root_with_cwd(
            Some(session.to_string_lossy().to_string()),
            "s1",
            RootPolicy::PreferCwdOnMismatch,
            cwd.clone(),
        );

        assert_eq!(selected, canonical_or_raw(session));
    }

    #[test]
    fn init_root_guard_prefers_cwd_when_session_has_no_project_markers() {
        let cwd = temp_root("cwd_with_markers");
        let session = temp_root("session_without_markers");

        fs::write(cwd.join("package.json"), "{}\n").expect("write package json");

        let selected = resolve_project_root_with_cwd(
            Some(session.to_string_lossy().to_string()),
            "s3",
            RootPolicy::PreferCwdOnMismatch,
            cwd.clone(),
        );

        assert_eq!(selected, canonical_or_raw(cwd));
    }

    #[test]
    fn default_root_policy_keeps_session_root_even_on_mismatch() {
        let cwd = temp_root("cwd_plain");
        let session = temp_root("session_plain");

        fs::write(
            session.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .expect("write cargo toml");

        let selected = resolve_project_root_with_cwd(
            Some(session.to_string_lossy().to_string()),
            "s2",
            RootPolicy::PreferSession,
            cwd,
        );

        assert_eq!(selected, canonical_or_raw(session));
    }
}
