use std::fs;
use std::path::{Component, Path};

use crate::commands::init_command::{ProjectAnalysis, ProjectInfo};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DoctorResult {
    pub success: bool,
    pub message: String,
    pub issues: Vec<DoctorIssue>,
    pub suggestions: Vec<ExecutableSuggestion>,
    pub checks: Vec<CheckResult>,
    pub summary: DoctorSummary,
    pub security_mode: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lan_secret: Option<LanSecretInfo>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LanSecretInfo {
    pub secret: String,
    pub was_generated: bool,
    pub config_path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct DoctorOptions {
    pub security: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DoctorSummary {
    pub critical_count: u8,
    pub warning_count: u8,
    pub suggestion_count: u8,
    pub health_score: u8,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DoctorIssue {
    pub level: IssueLevel,
    pub priority: u8,
    pub message: String,
    pub fix_hint: Option<String>,
    pub command: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum IssueLevel {
    Critical,
    Warning,
    Info,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutableSuggestion {
    pub message: String,
    pub command: Option<String>,
    pub auto_fixable: bool,
    pub category: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

pub struct DoctorCommand;

impl DoctorCommand {
    pub fn run(project_root: &Path, options: &DoctorOptions) -> DoctorResult {
        Self::run_with_config(project_root, options, None)
    }

    pub fn run_with_config(
        project_root: &Path,
        options: &DoctorOptions,
        config_path: Option<&Path>,
    ) -> DoctorResult {
        let info = super::init_command::scan_project(project_root);
        let _analysis = ProjectAnalysis::analyze(&info);
        let recommended_test_cmd = recommended_test_command(&info);
        let recommended_lint_cmd = recommended_lint_command(&info);

        let mut issues = Vec::new();
        let mut suggestions = Vec::new();
        let mut checks = Vec::new();
        let mut critical_count = 0u8;
        let mut warning_count = 0u8;

        checks.push(CheckResult {
            name: "Proyecto existe".to_string(),
            passed: project_root.is_dir(),
            message: if project_root.is_dir() {
                "✓".to_string()
            } else {
                "✗ Directorio no encontrado".to_string()
            },
        });

        if info.stack.is_empty() {
            critical_count += 1;
            issues.push(DoctorIssue {
                level: IssueLevel::Critical,
                priority: 10,
                message: "No se detectó stack tecnológico".to_string(),
                fix_hint: Some(
                    "Agregar Cargo.toml, package.json, o archivo de configuración".to_string(),
                ),
                command: None,
            });
        } else {
            checks.push(CheckResult {
                name: "Stack".to_string(),
                passed: true,
                message: format!(
                    "✓ {}",
                    info.stack.iter().cloned().collect::<Vec<_>>().join(", ")
                ),
            });
        }

        if info.has_tests {
            checks.push(CheckResult {
                name: "Tests".to_string(),
                passed: true,
                message: "✓ Tests detectados".to_string(),
            });
        } else {
            critical_count += 1;
            issues.push(DoctorIssue {
                level: IssueLevel::Critical,
                priority: 9,
                message: "No se detectaron tests".to_string(),
                fix_hint: Some(
                    "Agregar tests (ej. `test/`, `__tests__/`) y script/comando de ejecución"
                        .to_string(),
                ),
                command: recommended_test_cmd.clone(),
            });
            suggestions.push(ExecutableSuggestion {
                message: "Agregar tests unitarios".to_string(),
                command: recommended_test_cmd.clone(),
                auto_fixable: false,
                category: "testing".to_string(),
            });
        }

        if info.has_docs {
            checks.push(CheckResult {
                name: "Documentación".to_string(),
                passed: true,
                message: "✓ README.md encontrado".to_string(),
            });
        } else {
            warning_count += 1;
            issues.push(DoctorIssue {
                level: IssueLevel::Warning,
                priority: 6,
                message: "No se detectó documentación".to_string(),
                fix_hint: Some("Crear README.md con instrucciones".to_string()),
                command: None,
            });
            suggestions.push(ExecutableSuggestion {
                message: "Agregar README.md con documentación básica".to_string(),
                command: None,
                auto_fixable: false,
                category: "docs".to_string(),
            });
        }

        if info.has_docker {
            checks.push(CheckResult {
                name: "Docker".to_string(),
                passed: true,
                message: "✓ Dockerfile o docker-compose.yml detectado".to_string(),
            });
        }

        if info.has_linting {
            checks.push(CheckResult {
                name: "Linting".to_string(),
                passed: true,
                message: "✓ Configuración de linting detectada".to_string(),
            });
        } else {
            warning_count += 1;
            issues.push(DoctorIssue {
                level: IssueLevel::Warning,
                priority: 5,
                message: "No se detectó configuración de linting".to_string(),
                fix_hint: Some("Agregar .eslintrc, .rustfmt.toml, o similar".to_string()),
                command: recommended_lint_cmd.clone(),
            });
            suggestions.push(ExecutableSuggestion {
                message: "Agregar configuración de linting".to_string(),
                command: recommended_lint_cmd,
                auto_fixable: false,
                category: "quality".to_string(),
            });
        }

        if !info.validation_commands.is_empty() {
            checks.push(CheckResult {
                name: "Build commands".to_string(),
                passed: true,
                message: format!("✓ {} comandos disponibles", info.validation_commands.len()),
            });
        }

        if info.complexity == "large" && !info.has_linting {
            critical_count += 1;
            issues.push(DoctorIssue {
                level: IssueLevel::Critical,
                priority: 8,
                message: "Proyecto grande sin linting configurado".to_string(),
                fix_hint: Some("Configurar linting obligatorio para proyectos grandes".to_string()),
                command: None,
            });
        }

        if options.security {
            run_security_audit(
                project_root,
                &info,
                &mut checks,
                &mut issues,
                &mut suggestions,
                &mut critical_count,
                &mut warning_count,
            );
        }

        let suggestion_count = suggestions.len() as u8;

        let health_score =
            calculate_health_score(critical_count, warning_count, suggestion_count, &info);

        let message = if critical_count > 0 {
            format!(
                "{} crítico(s), {} warning(s)",
                critical_count, warning_count
            )
        } else if warning_count > 0 {
            format!("Proyecto okay ({} warning(s))", warning_count)
        } else {
            "Proyecto saludable".to_string()
        };

        let lan_secret = if options.security {
            check_or_generate_lan_secret(config_path, &mut checks, &mut issues, &mut critical_count)
        } else {
            None
        };

        DoctorResult {
            success: true,
            message,
            issues,
            suggestions,
            checks,
            summary: DoctorSummary {
                critical_count,
                warning_count,
                suggestion_count,
                health_score,
            },
            security_mode: options.security,
            lan_secret,
        }
    }
}

fn recommended_test_command(info: &ProjectInfo) -> Option<String> {
    if let Some(cmd) = info
        .validation_commands
        .iter()
        .find(|v| {
            let n = v.name.to_ascii_lowercase();
            let c = v.command.to_ascii_lowercase();
            n.contains("test") || c.contains(" test") || c.starts_with("test")
        })
        .map(|v| v.command.clone())
    {
        return Some(cmd);
    }

    if info.stack.contains("React")
        || info.stack.contains("Node.js")
        || info.stack.contains("TypeScript")
    {
        return Some("npm test".to_string());
    }
    if info.stack.contains("Flutter") {
        return Some("flutter test".to_string());
    }
    if info.stack.contains("Rust") {
        return Some("cargo test".to_string());
    }
    if info.stack.contains("Python") {
        return Some("pytest".to_string());
    }
    if info.stack.contains("Go") {
        return Some("go test ./...".to_string());
    }
    if info.stack.contains("C#") {
        return Some("dotnet test".to_string());
    }

    None
}

fn recommended_lint_command(info: &ProjectInfo) -> Option<String> {
    if let Some(cmd) = info
        .validation_commands
        .iter()
        .find(|v| {
            let n = v.name.to_ascii_lowercase();
            let c = v.command.to_ascii_lowercase();
            n.contains("lint")
                || n.contains("analyze")
                || c.contains("lint")
                || c.contains("clippy")
                || c.contains("analyze")
        })
        .map(|v| v.command.clone())
    {
        return Some(cmd);
    }

    if info.stack.contains("React")
        || info.stack.contains("Node.js")
        || info.stack.contains("TypeScript")
    {
        return Some("npm run lint".to_string());
    }
    if info.stack.contains("Flutter") {
        return Some("flutter analyze".to_string());
    }
    if info.stack.contains("Rust") {
        return Some("cargo clippy -- -D warnings".to_string());
    }
    if info.stack.contains("Python") {
        return Some("ruff check .".to_string());
    }
    if info.stack.contains("Go") {
        return Some("go vet ./...".to_string());
    }
    if info.stack.contains("C#") {
        return Some("dotnet format --verify-no-changes".to_string());
    }

    None
}

fn check_or_generate_lan_secret(
    config_path: Option<&Path>,
    checks: &mut Vec<CheckResult>,
    issues: &mut Vec<DoctorIssue>,
    critical_count: &mut u8,
) -> Option<LanSecretInfo> {
    use crate::config::generate_lan_secret;

    let path = config_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(crate::config::AppConfig::default_user_config_path);

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => {
            checks.push(CheckResult {
                name: "LAN Secret".to_string(),
                passed: false,
                message: format!("No se pudo leer config en {}", path.display()),
            });
            return None;
        }
    };

    #[derive(serde::Deserialize, Default)]
    struct NetworkOnly {
        #[serde(default)]
        network: NetworkSection,
    }
    #[derive(serde::Deserialize, Default)]
    struct NetworkSection {
        #[serde(default)]
        shared_secret: String,
    }

    let net: NetworkOnly = toml::from_str(&content).unwrap_or_default();
    let current_secret = net.network.shared_secret;

    if current_secret.is_empty() {
        let new_secret = generate_lan_secret();
        let updated = content.replace(
            "shared_secret = \"\"",
            &format!("shared_secret = \"{}\"", new_secret),
        );
        let saved = if updated != content {
            std::fs::write(&path, &updated).is_ok()
        } else {
            false
        };

        *critical_count = critical_count.saturating_add(1);
        issues.push(DoctorIssue {
            level: IssueLevel::Critical,
            priority: 15,
            message: "LAN Agent sin secret configurado — generando uno nuevo".to_string(),
            fix_hint: Some(format!(
                "Secret guardado en {}. Copia el codigo en tus otros dispositivos.",
                path.display()
            )),
            command: None,
        });
        checks.push(CheckResult {
            name: "LAN Secret".to_string(),
            passed: saved,
            message: if saved {
                "Secret generado y guardado automaticamente".to_string()
            } else {
                "Secret generado pero no se pudo guardar en config".to_string()
            },
        });

        Some(LanSecretInfo {
            secret: new_secret,
            was_generated: true,
            config_path: path.display().to_string(),
        })
    } else {
        checks.push(CheckResult {
            name: "LAN Secret".to_string(),
            passed: true,
            message: format!("LAN secret configurado ({} chars)", current_secret.len()),
        });

        Some(LanSecretInfo {
            secret: current_secret,
            was_generated: false,
            config_path: path.display().to_string(),
        })
    }
}

fn run_security_audit(
    project_root: &Path,
    info: &ProjectInfo,
    checks: &mut Vec<CheckResult>,
    issues: &mut Vec<DoctorIssue>,
    suggestions: &mut Vec<ExecutableSuggestion>,
    critical_count: &mut u8,
    warning_count: &mut u8,
) {
    let mut secret_hits: Vec<String> = Vec::new();
    let mut local_path_hits: Vec<String> = Vec::new();
    let mut localhost_hits: Vec<String> = Vec::new();

    let entries = walkdir::WalkDir::new(project_root)
        .max_depth(8)
        .into_iter()
        .filter_entry(|e| !is_ignored_path(e.path()))
        .filter_map(|e| e.ok());

    for entry in entries {
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        if is_ignored_security_file(path) {
            continue;
        }
        if !is_in_project_scope(path, project_root, &info.detected_paths) {
            continue;
        }
        if !is_security_relevant_file(path) {
            continue;
        }

        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };

        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if is_comment_line(trimmed) {
                continue;
            }

            let location = format!(
                "{}:{}",
                path.strip_prefix(project_root).unwrap_or(path).display(),
                idx + 1
            );

            if secret_hits.len() < 12 && looks_like_hardcoded_secret(trimmed) {
                secret_hits.push(location.clone());
            }
            if local_path_hits.len() < 12 && looks_like_absolute_local_path(trimmed) {
                local_path_hits.push(location.clone());
            }
            if localhost_hits.len() < 12 && looks_like_hardcoded_localhost(trimmed) {
                localhost_hits.push(location);
            }
        }
    }

    checks.push(CheckResult {
        name: "Security audit".to_string(),
        passed: secret_hits.is_empty() && local_path_hits.is_empty() && localhost_hits.is_empty(),
        message: if secret_hits.is_empty()
            && local_path_hits.is_empty()
            && localhost_hits.is_empty()
        {
            "✓ No se detectaron hardcodeos críticos (secretos/rutas/hosts locales)".to_string()
        } else {
            format!(
                "⚠️ hallazgos: secretos={}, rutas_locales={}, localhost={}",
                secret_hits.len(),
                local_path_hits.len(),
                localhost_hits.len()
            )
        },
    });

    if !secret_hits.is_empty() {
        *critical_count = critical_count.saturating_add(1);
        issues.push(DoctorIssue {
            level: IssueLevel::Critical,
            priority: 10,
            message: format!(
                "Posibles secretos hardcodeados detectados ({} hallazgos)",
                secret_hits.len()
            ),
            fix_hint: Some(
                "Mover credenciales a variables de entorno o secret manager".to_string(),
            ),
            command: None,
        });
        suggestions.push(ExecutableSuggestion {
            message: format!(
                "Revisar y limpiar secretos hardcodeados en: {}",
                secret_hits
                    .iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            command: None,
            auto_fixable: false,
            category: "security".to_string(),
        });
    }

    if !local_path_hits.is_empty() {
        *warning_count = warning_count.saturating_add(1);
        issues.push(DoctorIssue {
            level: IssueLevel::Warning,
            priority: 8,
            message: format!(
                "Rutas absolutas locales hardcodeadas detectadas ({} hallazgos)",
                local_path_hits.len()
            ),
            fix_hint: Some("Usar rutas relativas o variables de entorno".to_string()),
            command: None,
        });
    }

    if !localhost_hits.is_empty() {
        *warning_count = warning_count.saturating_add(1);
        issues.push(DoctorIssue {
            level: IssueLevel::Warning,
            priority: 7,
            message: format!(
                "Hosts locales hardcodeados detectados ({} hallazgos)",
                localhost_hits.len()
            ),
            fix_hint: Some("Parametrizar host/puerto por configuración".to_string()),
            command: None,
        });
    }
}

fn is_comment_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('#')
        || trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with("--")
}

fn is_security_relevant_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some(
            "rs" | "js"
                | "ts"
                | "jsx"
                | "tsx"
                | "py"
                | "java"
                | "kt"
                | "go"
                | "cpp"
                | "c"
                | "h"
                | "hpp"
                | "qml"
                | "json"
                | "yaml"
                | "yml"
                | "toml"
                | "env"
        )
    )
}

fn is_ignored_security_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };

    matches!(
        name,
        "package-lock.json"
            | "yarn.lock"
            | "pnpm-lock.yaml"
            | "Cargo.lock"
            | "composer.lock"
            | "go.sum"
            | "Podfile.lock"
    )
}

fn is_in_project_scope(path: &Path, project_root: &Path, detected_paths: &[String]) -> bool {
    if detected_paths.is_empty() {
        return true;
    }

    let Ok(relative) = path.strip_prefix(project_root) else {
        return false;
    };

    let mut components = relative.components();
    let Some(first) = components.next() else {
        return true;
    };

    let Component::Normal(first) = first else {
        return true;
    };
    let Some(first_dir) = first.to_str() else {
        return true;
    };

    let is_root_file = relative.components().count() == 1;
    if is_root_file {
        return true;
    }

    detected_paths
        .iter()
        .any(|dir| dir.eq_ignore_ascii_case(first_dir))
}

fn looks_like_hardcoded_secret(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();

    if lower.contains("process.env")
        || lower.contains("std::env")
        || lower.contains("getenv(")
        || lower.contains("env(")
    {
        return false;
    }

    let has_key_name = lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("secret")
        || lower.contains("token")
        || lower.contains("password")
        || lower.contains("access_key");
    let looks_openai = line.contains("sk-") && line.len() > 20;

    if looks_openai {
        return true;
    }

    if !has_key_name {
        return false;
    }

    let assignment = if let Some((lhs, rhs)) = line.split_once('=') {
        Some((lhs.trim(), rhs.trim()))
    } else if let Some((lhs, rhs)) = line.split_once(':') {
        Some((lhs.trim(), rhs.trim()))
    } else {
        None
    };

    let Some((lhs, rhs)) = assignment else {
        return false;
    };

    if rhs.is_empty() {
        return false;
    }

    let is_quoted_literal = (rhs.starts_with('"') && rhs[1..].contains('"'))
        || (rhs.starts_with('\'') && rhs[1..].contains('\''));
    if !is_quoted_literal {
        return false;
    }

    let value = rhs
        .trim_matches(|c: char| c == '"' || c == '\'' || c == ',' || c == ';')
        .trim();

    let lhs_norm = lhs
        .trim_matches(|c: char| c == '"' || c == '\'' || c == '{' || c == '[')
        .trim()
        .to_ascii_lowercase();

    let lhs_matches_secret_key = lhs_norm.ends_with("api_key")
        || lhs_norm.ends_with("apikey")
        || lhs_norm.ends_with("secret")
        || lhs_norm.ends_with("token")
        || lhs_norm.ends_with("password")
        || lhs_norm.ends_with("access_key")
        || lhs_norm.ends_with("consumer_secret")
        || lhs_norm.ends_with("client_secret")
        || lhs_norm.ends_with("jwt_secret");

    if !lhs_matches_secret_key {
        return false;
    }

    if lhs_norm.contains("token_uri") || lhs_norm.contains("auth_uri") {
        return false;
    }

    if value.len() < 8 {
        return false;
    }

    if value.contains(char::is_whitespace) {
        return false;
    }

    if value.starts_with("http://") || value.starts_with("https://") {
        return false;
    }

    if value.eq_ignore_ascii_case("localhost")
        || value.eq_ignore_ascii_case("example")
        || value.eq_ignore_ascii_case("default")
        || value.eq_ignore_ascii_case("changeme")
    {
        return false;
    }

    true
}

fn looks_like_absolute_local_path(line: &str) -> bool {
    line.contains("/home/") || line.contains("/Users/") || line.contains("C:\\\\Users\\")
}

fn looks_like_hardcoded_localhost(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("localhost:") || lower.contains("127.0.0.1:") || lower.contains("0.0.0.0:")
}

fn is_ignored_path(path: &Path) -> bool {
    const IGNORED_DIRS: [&str; 12] = [
        ".git",
        "node_modules",
        "target",
        "build",
        "dist",
        ".venv",
        "venv",
        "__pycache__",
        ".idea",
        ".vscode",
        "coverage",
        "vendor",
    ];

    path.components().any(|component| {
        let Component::Normal(name) = component else {
            return false;
        };
        let Some(name) = name.to_str() else {
            return false;
        };
        IGNORED_DIRS
            .iter()
            .any(|ignored| name.eq_ignore_ascii_case(ignored))
    })
}

fn calculate_health_score(critical: u8, warnings: u8, suggestions: u8, info: &ProjectInfo) -> u8 {
    let base: i16 = 100;
    let critical_penalty = (critical as i16) * 20;
    let warning_penalty = (warnings as i16) * 5;
    let suggestion_penalty = (suggestions as i16) * 2;

    let has_basics = info.has_tests && info.has_docs;
    let bonus = if has_basics { 5 } else { 0 };

    let score = base - critical_penalty - warning_penalty - suggestion_penalty + bonus;
    score.clamp(0, 100) as u8
}

pub fn format_doctor_report(result: &DoctorResult) -> String {
    let mut output = String::new();

    output.push_str("# 🩺 Doctor Report\n\n");
    output.push_str(&format!(
        "## 📊 Health Score: {}/100\n\n",
        result.summary.health_score
    ));
    output.push_str(&format!(
        "{}{}\n\n",
        if result.summary.critical_count > 0 {
            "⚠️ "
        } else {
            "✅ "
        },
        result.message
    ));

    if result.security_mode {
        output.push_str("🔐 Security mode: habilitado (`/doctor --security`)\n\n");
    }

    output.push_str("## ✓ Checks\n\n");
    for check in &result.checks {
        output.push_str(&format!("- **{}**: {}\n", check.name, check.message));
    }

    if !result.issues.is_empty() {
        output.push_str("\n## 🚨 Issues (by Priority)\n\n");

        let mut sorted_issues = result.issues.clone();
        sorted_issues.sort_by(|a, b| b.priority.cmp(&a.priority));

        for issue in sorted_issues {
            let icon = match issue.level {
                IssueLevel::Critical => "❗",
                IssueLevel::Warning => "⚠️",
                IssueLevel::Info => "ℹ️",
            };
            output.push_str(&format!(
                "{} **[{}]** [P{}] {}\n",
                icon,
                format!("{:?}", issue.level).to_uppercase(),
                issue.priority,
                issue.message
            ));
            if let Some(fix) = &issue.fix_hint {
                output.push_str(&format!("   → Fix: {}\n", fix));
            }
            if let Some(cmd) = &issue.command {
                output.push_str(&format!("   → Command: `{}`\n", cmd));
            }
        }
    }

    if !result.suggestions.is_empty() {
        output.push_str("\n## 💡 Suggestions\n\n");
        for suggestion in &result.suggestions {
            output.push_str(&format!(
                "- **{}**: {}\n",
                suggestion.category, suggestion.message
            ));
            if let Some(cmd) = &suggestion.command {
                output.push_str(&format!("  → `{}`\n", cmd));
            }
            if suggestion.auto_fixable {
                output.push_str("  → Auto-fixable\n");
            }
        }
    }

    if let Some(lan) = &result.lan_secret {
        output.push_str("\n## 🔑 LAN Agent Secret\n\n");
        if lan.was_generated {
            output.push_str("**Secret generado ahora** — copialo en tus otros dispositivos:\n\n");
        } else {
            output.push_str("**Secret activo** — mismo codigo para todos tus dispositivos:\n\n");
        }
        output.push_str(&format!("```\nshared_secret = \"{}\"\n```\n\n", lan.secret));
        output.push_str(&format!("📁 Config: `{}`\n\n", lan.config_path));
        output.push_str(
            "En cada dispositivo agrega el mismo `shared_secret` en la seccion `[network]`\nde su archivo de configuracion y reinicia Multilink.\n",
        );
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_detector_ignores_env_access_patterns() {
        assert!(!looks_like_hardcoded_secret(
            "const jwtSecret = process.env.JWT_SECRET || 'fallback';"
        ));
        assert!(!looks_like_hardcoded_secret(
            "let api_key = std::env::var(\"API_KEY\").unwrap_or_default();"
        ));
    }

    #[test]
    fn secret_detector_detects_literal_secret_assignments() {
        assert!(looks_like_hardcoded_secret("API_KEY = \"abc123456789XYZ\""));
        assert!(looks_like_hardcoded_secret("token: 'abcd1234secret'"));
    }

    #[test]
    fn secret_detector_ignores_non_secret_ui_labels() {
        assert!(!looks_like_hardcoded_secret("title: 'Change Password'"));
        assert!(!looks_like_hardcoded_secret("headerTintColor: '#fff'"));
    }

    #[test]
    fn security_audit_ignores_lockfiles() {
        assert!(is_ignored_security_file(Path::new("package-lock.json")));
        assert!(is_ignored_security_file(Path::new("yarn.lock")));
        assert!(!is_ignored_security_file(Path::new("src/config/env.ts")));
    }

    #[test]
    fn project_scope_filters_unrelated_top_level_dirs() {
        let root = Path::new("/repo");
        let paths = vec!["src".to_string(), "test".to_string()];

        assert!(is_in_project_scope(
            Path::new("/repo/src/navigation/AppNavigator.tsx"),
            root,
            &paths
        ));
        assert!(!is_in_project_scope(
            Path::new("/repo/fitbalance-backend/index.ts"),
            root,
            &paths
        ));
        assert!(is_in_project_scope(
            Path::new("/repo/README.md"),
            root,
            &paths
        ));
    }

    #[test]
    fn generate_lan_secret_produces_64_char_hex() {
        use crate::config::generate_lan_secret;
        let secret = generate_lan_secret();
        assert_eq!(secret.len(), 64, "debe ser 32 bytes = 64 chars hex");
        assert!(
            secret.chars().all(|c| c.is_ascii_hexdigit()),
            "debe ser hex valido"
        );
    }

    #[test]
    fn generate_lan_secret_not_all_zeros() {
        use crate::config::generate_lan_secret;
        let secret = generate_lan_secret();
        assert_ne!(secret, "0".repeat(64), "no debe ser todo ceros");
    }

    #[test]
    fn lan_secret_info_serializes_correctly() {
        let info = LanSecretInfo {
            secret: "abc123".to_string(),
            was_generated: true,
            config_path: "/home/user/.config/multilink/multilink.toml".to_string(),
        };
        let json = serde_json::to_string(&info).expect("serialize");
        assert!(json.contains("abc123"));
        assert!(json.contains("was_generated"));
    }

    #[test]
    fn doctor_result_lan_secret_none_by_default() {
        let result = DoctorResult {
            success: true,
            message: "ok".to_string(),
            issues: vec![],
            suggestions: vec![],
            checks: vec![],
            summary: DoctorSummary {
                critical_count: 0,
                warning_count: 0,
                suggestion_count: 0,
                health_score: 100,
            },
            security_mode: false,
            lan_secret: None,
        };
        assert!(result.lan_secret.is_none());
        let report = format_doctor_report(&result);
        assert!(!report.contains("LAN Agent Secret"));
    }
}
