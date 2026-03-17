use std::path::PathBuf;

use crate::commands::init_command::{ProjectAnalysis, ProjectInfo};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DoctorResult {
    pub success: bool,
    pub message: String,
    pub issues: Vec<DoctorIssue>,
    pub suggestions: Vec<ExecutableSuggestion>,
    pub checks: Vec<CheckResult>,
    pub summary: DoctorSummary,
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
    pub fn run(project_root: &PathBuf) -> DoctorResult {
        let info = super::init_command::scan_project(project_root);
        let analysis = ProjectAnalysis::analyze(&info);

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
                fix_hint: Some("Crear directorio tests/ o agregar scripts de test".to_string()),
                command: Some("mkdir -p tests".to_string()),
            });
            suggestions.push(ExecutableSuggestion {
                message: "Agregar tests unitarios".to_string(),
                command: Some("cargo test".to_string()),
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
                command: None,
            });
            suggestions.push(ExecutableSuggestion {
                message: "Agregar configuración de linting".to_string(),
                command: Some("cargo clippy --fix".to_string()),
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

        let suggestion_count = suggestions.len() as u8;

        let health_score =
            calculate_health_score(critical_count, warning_count, suggestion_count, &info);

        let message = if critical_count > 0 {
            format!(
                "⚠️ {} crítico(s), {} warning(s)",
                critical_count, warning_count
            )
        } else if warning_count > 0 {
            format!("✅ Proyecto okay ({} warnings)", warning_count)
        } else {
            "✅ Proyecto saludable".to_string()
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
        }
    }
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

    output.push_str(&format!("# 🩺 Doctor Report\n\n"));
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

    output
}
