use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub stack: HashSet<String>,
    pub project_type: String,
    pub has_tests: bool,
    pub has_docs: bool,
    pub has_docker: bool,
    pub has_linting: bool,
    pub complexity: String,
    pub detected_paths: Vec<String>,
    pub readme_content: Option<String>,
    pub validation_commands: Vec<ValidationCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationCommand {
    pub name: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectAnalysis {
    pub issues: Vec<String>,
    pub suggestions: Vec<String>,
    pub missing_parts: Vec<String>,
}

impl ProjectAnalysis {
    pub fn analyze(info: &ProjectInfo) -> Self {
        let mut issues = Vec::new();
        let mut suggestions = Vec::new();
        let mut missing_parts = Vec::new();

        if !info.has_tests {
            issues.push("⚠️ No se detectaron tests".to_string());
            missing_parts.push("tests/ o test/ directory".to_string());
            suggestions.push("Agregar tests unitarios o de integración".to_string());
        }

        if !info.has_docs {
            issues.push("⚠️ No se detectó documentación".to_string());
            missing_parts.push("README.md o docs/".to_string());
            suggestions.push("Agregar README.md con instrucciones".to_string());
        }

        if info.project_type == "software project" && info.complexity == "prototype" {
            suggestions
                .push("Proyecto en fase inicial - considerar agregar estructura".to_string());
        }

        if !info.has_linting {
            suggestions.push("Agregar configuración de linting (eslint, clippy, etc.)".to_string());
        }

        if info.detected_paths.is_empty() {
            issues.push("⚠️ Estructura de directorios no clara".to_string());
            suggestions.push("Considerar organizar en src/, tests/, docs/".to_string());
        }

        if info.stack.is_empty() {
            issues.push("⚠️ No se detectó stack tecnológico".to_string());
        }

        if info.complexity == "large" && !info.has_linting {
            issues.push("⚠️ Proyecto grande sin linting configurado".to_string());
        }

        Self {
            issues,
            suggestions,
            missing_parts,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitCommand {
    pub force: bool,
    pub smart: bool,
    pub merge: bool,
}

impl Default for InitCommand {
    fn default() -> Self {
        Self {
            force: false,
            smart: false,
            merge: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitResult {
    pub success: bool,
    pub message: String,
    pub file_path: Option<PathBuf>,
    pub project_info: Option<ProjectInfo>,
    pub analysis: Option<ProjectAnalysis>,
    pub action: InitAction,
    pub health_score: Option<u8>,
    pub critical_issues: Option<u8>,
    pub warnings: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InitAction {
    Created,
    Exists,
    Merged,
    Cancelled,
    Updated,
    Analyzed,
}

impl Default for InitResult {
    fn default() -> Self {
        Self {
            success: false,
            message: String::new(),
            file_path: None,
            project_info: None,
            analysis: None,
            action: InitAction::Cancelled,
            health_score: None,
            critical_issues: None,
            warnings: None,
        }
    }
}

pub fn run_init(project_root: &Path, command: &InitCommand) -> InitResult {
    let multilink_path = project_root.join("MULTILINK.md");

    let info = scan_project(project_root);
    let analysis = ProjectAnalysis::analyze(&info);
    let (health_score, critical_issues, warnings) = calculate_health_from_info(&info, &analysis);

    if multilink_path.exists() && !command.force && !command.merge {
        if command.smart {
            return InitResult {
                success: true,
                message: "MULTILINK.md ya existe. Usa --force para sobrescribir o --merge para actualizar.".to_string(),
                file_path: Some(multilink_path),
                project_info: Some(info),
                analysis: Some(analysis),
                action: InitAction::Exists,
                health_score: Some(health_score),
                critical_issues: Some(critical_issues),
                warnings: Some(warnings),
            };
        } else {
            return InitResult {
                success: false,
                message: "MULTILINK.md ya existe. Usa /init --force para sobrescribir.".to_string(),
                file_path: Some(multilink_path),
                project_info: None,
                analysis: None,
                action: InitAction::Exists,
                health_score: Some(health_score),
                critical_issues: Some(critical_issues),
                warnings: Some(warnings),
            };
        }
    }

    if command.smart {
        return InitResult {
            success: true,
            message: "Análisis completado (modo smart - no se creó archivo)".to_string(),
            file_path: Some(multilink_path),
            project_info: Some(info),
            analysis: Some(analysis),
            action: InitAction::Analyzed,
            health_score: Some(health_score),
            critical_issues: Some(critical_issues),
            warnings: Some(warnings),
        };
    }

    let content = generate_multilink_md(&info, &analysis, project_root);

    if command.merge && multilink_path.exists() {
        match merge_multilink(&multilink_path, &content) {
            Ok(_) => InitResult {
                success: true,
                message: format!(
                    "MULTILINK.md actualizado con merge en {}",
                    multilink_path.display()
                ),
                file_path: Some(multilink_path),
                project_info: Some(info),
                analysis: Some(analysis),
                action: InitAction::Merged,
                health_score: Some(health_score),
                critical_issues: Some(critical_issues),
                warnings: Some(warnings),
            },
            Err(e) => InitResult {
                success: false,
                message: format!("Error en merge: {}", e),
                file_path: None,
                project_info: None,
                analysis: None,
                action: InitAction::Cancelled,
                health_score: None,
                critical_issues: None,
                warnings: None,
            },
        }
    } else {
        match fs::write(&multilink_path, content) {
            Ok(_) => InitResult {
                success: true,
                message: format!("MULTILINK.md creado en {}", multilink_path.display()),
                file_path: Some(multilink_path),
                project_info: Some(info),
                analysis: Some(analysis),
                action: InitAction::Created,
                health_score: Some(health_score),
                critical_issues: Some(critical_issues),
                warnings: Some(warnings),
            },
            Err(e) => InitResult {
                success: false,
                message: format!("Error al escribir: {}", e),
                file_path: None,
                project_info: None,
                analysis: None,
                action: InitAction::Cancelled,
                health_score: None,
                critical_issues: None,
                warnings: None,
            },
        }
    }
}

fn merge_multilink(existing_path: &Path, new_content: &str) -> Result<(), String> {
    let existing = fs::read_to_string(existing_path).map_err(|e| e.to_string())?;

    let user_sections = extract_user_sections(&existing);

    let mut result = new_content.to_string();

    for (key, content) in user_sections {
        let marker = format!("<!-- multilink:user:{} -->", key);
        if !result.contains(&marker) {
            result.push_str(&format!("\n{}\n{}", marker, content));
        }
    }

    fs::write(existing_path, result).map_err(|e| e.to_string())
}

fn extract_user_sections(content: &str) -> Vec<(String, String)> {
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut current_key: Option<String> = None;
    let mut current_content = String::new();
    let mut in_user_section = false;

    for line in content.lines() {
        if line.contains("<!-- multilink:user:") {
            if let Some(key) = line.split("<!-- multilink:user:").nth(1) {
                if let Some(end) = key.find(" -->") {
                    if in_user_section {
                        if let Some(ref k) = current_key {
                            sections.push((k.clone(), current_content.trim().to_string()));
                        }
                    }
                    current_key = Some(key[..end].to_string());
                    current_content = String::new();
                    in_user_section = true;
                }
            }
        } else if in_user_section && line.contains("<!-- multilink:end -->") {
            if let Some(ref key) = current_key {
                sections.push((key.clone(), current_content.trim().to_string()));
            }
            current_key = None;
            current_content = String::new();
            in_user_section = false;
        } else if in_user_section {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    if in_user_section {
        if let Some(ref key) = current_key {
            sections.push((key.clone(), current_content.trim().to_string()));
        }
    }

    sections
}

pub fn scan_project(project_root: &Path) -> ProjectInfo {
    let mut info = ProjectInfo {
        name: String::new(),
        stack: HashSet::new(),
        project_type: String::new(),
        has_tests: false,
        has_docs: false,
        has_docker: false,
        has_linting: false,
        complexity: String::new(),
        detected_paths: Vec::new(),
        readme_content: None,
        validation_commands: Vec::new(),
    };

    info.name = detect_project_name(project_root);
    info.detected_paths = detect_directories(project_root);

    detect_stack_files(project_root, &mut info);
    detect_test_files(project_root, &mut info);
    detect_docs(project_root, &mut info);
    detect_docker(project_root, &mut info);
    detect_linting(project_root, &mut info);

    info.validation_commands = detect_validation_commands(project_root, &info);

    info.project_type = infer_project_type(&info);
    info.complexity = infer_complexity(project_root);

    if let Ok(readme) = fs::read_to_string(project_root.join("README.md")) {
        let first_lines: String = readme.lines().take(10).collect::<Vec<_>>().join("\n");
        info.readme_content = Some(first_lines);
    }

    info
}

fn detect_project_name(root: &Path) -> String {
    if let Ok(name) = fs::read_to_string(root.join("package.json")) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&name) {
            if let Some(n) = json.get("name").and_then(|v| v.as_str()) {
                return n.to_string();
            }
        }
    }

    if let Ok(name) = fs::read_to_string(root.join("Cargo.toml")) {
        if let Ok(toml_val) = toml::from_str::<toml::Value>(&name) {
            if let Some(n) = toml_val
                .get("package")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
            {
                return n.to_string();
            }
        }
    }

    root.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string()
}

fn detect_directories(root: &Path) -> Vec<String> {
    let important_dirs = [
        "src",
        "lib",
        "app",
        "core",
        "gui",
        "backend",
        "frontend",
        "api",
        "cmd",
        "internal",
        "pkg",
        "shared",
        "docs",
        "tests",
        "test",
        "__tests__",
        "spec",
        "features",
        "scripts",
        "tools",
        "config",
        "configs",
        "migrations",
        "db",
        "storage",
        "assets",
    ];

    let mut found = Vec::new();

    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if important_dirs
                        .iter()
                        .any(|&d| name == d || name.starts_with(d))
                    {
                        found.push(name.to_string());
                    }
                }
            }
        }
    }

    found
}

fn detect_stack_files(root: &Path, info: &mut ProjectInfo) {
    let stack_indicators = [
        ("Cargo.toml", "Rust"),
        ("package.json", "Node.js"),
        ("requirements.txt", "Python"),
        ("pyproject.toml", "Python"),
        ("go.mod", "Go"),
        ("pom.xml", "Java"),
        ("build.gradle", "Kotlin"),
        ("CMakeLists.txt", "C++"),
        ("pubspec.yaml", "Flutter"),
        ("composer.json", "PHP"),
    ];

    for (file, lang) in &stack_indicators {
        if root.join(file).exists() {
            info.stack.insert(lang.to_string());
        }
    }

    if root.join("pubspec.yaml").exists() {
        info.stack.insert("Flutter".to_string());
    }

    let entries = walkdir::WalkDir::new(root)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok());

    for entry in entries {
        if let Some(name) = entry.file_name().to_str() {
            if name.ends_with(".qml") {
                info.stack.insert("Qt/QML".to_string());
            }
            if name.ends_with(".csproj") {
                info.stack.insert("C#".to_string());
            }
            if name.ends_with(".cs") && name.to_lowercase().contains("xamarin") {
                info.stack.insert("Xamarin".to_string());
            }
        }
    }
}

fn detect_test_files(root: &Path, info: &mut ProjectInfo) {
    let test_patterns = ["tests", "test", "__tests__", "spec", "features"];

    for pattern in test_patterns {
        if root.join(pattern).is_dir() {
            info.has_tests = true;
            break;
        }
    }

    if root.join("Cargo.toml").exists() {
        if root.join("tests").is_dir() || root.join("src/tests").is_dir() {
            info.has_tests = true;
        }
    }

    if let Ok(content) = fs::read_to_string(root.join("package.json")) {
        if content.contains("\"test\"") {
            info.has_tests = true;
        }
    }
}

fn detect_docs(root: &Path, info: &mut ProjectInfo) {
    if root.join("README.md").exists() || root.join("readme.md").exists() {
        info.has_docs = true;
    }
    if root.join("docs").is_dir() {
        info.has_docs = true;
    }
}

fn detect_docker(root: &Path, info: &mut ProjectInfo) {
    info.has_docker = root.join("Dockerfile").exists()
        || root.join("docker-compose.yml").exists()
        || root.join("docker-compose.yaml").exists();
}

fn detect_linting(root: &Path, info: &mut ProjectInfo) {
    let linting_files = [
        ".eslintrc",
        ".eslintrc.js",
        ".eslintrc.json",
        ".pylintrc",
        "pyproject.toml",
        "setup.cfg",
        ".rustfmt.toml",
        "clippy.toml",
        ".prettierrc",
        "prettier.config.js",
        "tslint.json",
        ".editorconfig",
    ];

    for file in linting_files {
        if root.join(file).exists() {
            info.has_linting = true;
            break;
        }
    }

    if root.join("Cargo.toml").exists() {
        if let Ok(content) = fs::read_to_string(root.join("Cargo.toml")) {
            if content.contains("[profile") {
                info.has_linting = true;
            }
        }
    }
}

fn detect_validation_commands(root: &Path, _info: &ProjectInfo) -> Vec<ValidationCommand> {
    let mut commands = Vec::new();

    if root.join("Cargo.toml").exists() {
        commands.push(ValidationCommand {
            name: "Build".to_string(),
            command: "cargo build".to_string(),
        });
        commands.push(ValidationCommand {
            name: "Test".to_string(),
            command: "cargo test".to_string(),
        });
        commands.push(ValidationCommand {
            name: "Lint".to_string(),
            command: "cargo clippy -- -D warnings".to_string(),
        });
        commands.push(ValidationCommand {
            name: "Format".to_string(),
            command: "cargo fmt --all".to_string(),
        });
    }

    if root.join("package.json").exists() {
        commands.push(ValidationCommand {
            name: "Install".to_string(),
            command: "npm install".to_string(),
        });
        commands.push(ValidationCommand {
            name: "Test".to_string(),
            command: "npm test".to_string(),
        });
        commands.push(ValidationCommand {
            name: "Build".to_string(),
            command: "npm run build".to_string(),
        });
    }

    if root.join("Makefile").exists() {
        commands.push(ValidationCommand {
            name: "Make".to_string(),
            command: "make".to_string(),
        });
    }

    if root.join("pytest.ini").exists() || root.join("pyproject.toml").exists() {
        commands.push(ValidationCommand {
            name: "Python Test".to_string(),
            command: "pytest".to_string(),
        });
    }

    if root.join("CMakeLists.txt").exists() {
        commands.push(ValidationCommand {
            name: "CMake Build".to_string(),
            command: "cmake -S . -B build && cmake --build build".to_string(),
        });
    }

    if root.join("pubspec.yaml").exists() {
        commands.push(ValidationCommand {
            name: "Flutter Test".to_string(),
            command: "flutter test".to_string(),
        });
        commands.push(ValidationCommand {
            name: "Flutter Analyze".to_string(),
            command: "flutter analyze".to_string(),
        });
    }

    commands
}

fn infer_project_type(info: &ProjectInfo) -> String {
    if info.stack.contains("Flutter") {
        return "mobile app".to_string();
    }
    if info.stack.contains("Qt/QML") || info.detected_paths.contains(&"gui".to_string()) {
        return "desktop app".to_string();
    }
    if info.stack.contains("Node.js") {
        if info.detected_paths.contains(&"frontend".to_string())
            || info.detected_paths.contains(&"web".to_string())
        {
            return "web frontend".to_string();
        }
        if info.detected_paths.contains(&"backend".to_string())
            || info.detected_paths.contains(&"api".to_string())
        {
            return "backend API".to_string();
        }
        return "fullstack web app".to_string();
    }
    if info.stack.contains("Python") {
        return "backend API".to_string();
    }
    if info.stack.contains("Rust") {
        return "Rust project".to_string();
    }
    "software project".to_string()
}

fn infer_complexity(root: &Path) -> String {
    let file_count = count_source_files(root);

    if file_count < 10 {
        "prototype".to_string()
    } else if file_count < 100 {
        "small".to_string()
    } else if file_count < 500 {
        "medium".to_string()
    } else {
        "large".to_string()
    }
}

fn count_source_files(root: &Path) -> usize {
    let extensions = [
        "rs", "go", "py", "js", "ts", "jsx", "tsx", "java", "kt", "cpp", "c", "h", "hpp", "cs",
        "rb", "php", "swift", "dart",
    ];

    let entries = walkdir::WalkDir::new(root)
        .max_depth(5)
        .into_iter()
        .filter_map(|e| e.ok());

    entries
        .filter(|entry| {
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                    return extensions.contains(&ext);
                }
            }
            false
        })
        .count()
}

pub fn generate_multilink_md(
    info: &ProjectInfo,
    analysis: &ProjectAnalysis,
    project_root: &Path,
) -> String {
    let name = &info.name;
    let project_type = &info.project_type;
    let complexity = &info.complexity;
    let stack_str = if info.stack.is_empty() {
        "No stack detected".to_string()
    } else {
        info.stack.iter().cloned().collect::<Vec<_>>().join(", ")
    };

    let paths_str = info
        .detected_paths
        .iter()
        .map(|p| format!("  - `{}`", p))
        .collect::<Vec<_>>()
        .join("\n");

    let overview = info
        .readme_content
        .as_ref()
        .map(|s| {
            s.lines()
                .skip_while(|l| l.trim().is_empty() || l.trim().starts_with('#'))
                .take(5)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| format!("{} project with {} stack.", project_type, stack_str));

    let validation_str = if info.validation_commands.is_empty() {
        "- (No standard validation commands detected)".to_string()
    } else {
        info.validation_commands
            .iter()
            .map(|v| format!("- `{}`: {}", v.name, v.command))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let issues_str = if analysis.issues.is_empty() {
        "✓ No se detectaron problemas".to_string()
    } else {
        analysis.issues.join("\n")
    };

    let suggestions_str = if analysis.suggestions.is_empty() {
        "✓ Proyecto bien configurado".to_string()
    } else {
        analysis
            .suggestions
            .iter()
            .map(|s| format!("- {}", s))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let machine_readable = serde_json::json!({
        "stack": info.stack.iter().cloned().collect::<Vec<_>>(),
        "type": info.project_type,
        "has_tests": info.has_tests,
        "has_docs": info.has_docs,
        "has_docker": info.has_docker,
        "has_linting": info.has_linting,
        "complexity": info.complexity,
        "paths": info.detected_paths,
        "validation_commands": info.validation_commands,
    })
    .to_string();

    format!(
        r#"# {name} — Project Definition

> Este archivo fue generado automáticamente por MultiLink.
> Lee este archivo primero antes de cualquier tarea.

---

## 📌 Project Overview

{overview}

**Tipo de proyecto:** {project_type}  
**Complejidad:** {complexity}

---

## 🏗️ Architecture

**Stack detected:** {stack_str}

**Estructura detectada:**
{paths_str}

---

## 📊 Current State

- **Tests:** {has_tests}
- **Documentación:** {has_docs}
- **Docker:** {has_docker}
- **Linting:** {has_linting}

### ⚠️ Problemas Detectados

{issues}

### 💡 Sugerencias

{suggestions}

---

## 🗺️ Active Roadmaps

Lee los archivos en `docs/` para ver la planificación:

- `docs/ROADMAP_*.md` — Roadmap general
- `docs/*_ROADMAP.md` — Roadmaps específicos

---

## ⚙️ Execution Rules

1. Leer este archivo primero
2. Usar roadmap si existe en docs/
3. Ejecutar UNA tarea a la vez
4. Validar antes de completar (ver comandos abajo)
5. No asumir contexto faltante

### Comandos de Validación

{validation}

---

## 🔒 Constraints

- No romper código existente
- Mantener compatibilidad
- Preferir cambios pequeños
- No compartir credenciales

---

## 🏃 Execution Context

### Runtime Commands

- **Build (core):** `cargo build --manifest-path core/Cargo.toml`
- **Test (core):** `cargo test --manifest-path core/Cargo.toml`
- **Lint:** `cargo clippy --manifest-path core/Cargo.toml -- -D warnings`
- **Format:** `cargo fmt --all --manifest-path core/Cargo.toml`

### Quick Reference

```bash
# Run tests
cargo test --manifest-path core/Cargo.toml

# Build GUI
cmake -S gui -B build/gui -DCMAKE_BUILD_TYPE=Release
cmake --build build/gui --config Release

# Run app
./build/gui/multilink_gui
```

### Architecture Boundaries

- `core/` → Runtime, routing, providers, persistence, auth
- `gui/` → Presentation and user interaction
- C++/Qt shim stays thin; business logic belongs in Rust core

---

## 🧠 Agent Behavior

### Cómo pensar sobre este proyecto

- **Tipo:** {project_type}
- **Stack:** {stack_str}
- **Complejidad:** {complexity}

### Workflow recomendado

```
1. Entender la tarea → leer docs/ si existe
2. Identificar módulo afectado
3. Implementar cambio mínimo necesario
4. Validar con tests
5. Commit con mensaje descriptivo
```

### Cuando detenerse

- Tarea demasiado grande → dividir
- Falta de contexto → pedir clarificación
- Ambigüedad → confirmar antes de proceder

---

## 📂 Important Paths

{paths_str}

---

## 🤖 Machine Readable

```json
{machine_json}
```

<!-- multilink:user:custom_notes -->
<!-- multilink:end -->

---

## 🤖 Generated by MultiLink

Este archivo fue inicializado automáticamente el {date}.

Puedes editar la sección `custom_notes` para agregar notas específicas del proyecto.

---

## 🏷️ Tags

{tags}
"#,
        name = name,
        project_type = project_type,
        complexity = complexity,
        stack_str = stack_str,
        paths_str = if paths_str.is_empty() {
            "- (No estructura específica detectada)".to_string()
        } else {
            paths_str
        },
        has_tests = if info.has_tests { "✅" } else { "❌" },
        has_docs = if info.has_docs { "✅" } else { "❌" },
        has_docker = if info.has_docker { "✅" } else { "❌" },
        has_linting = if info.has_linting { "✅" } else { "❌" },
        validation = validation_str,
        issues = issues_str,
        suggestions = suggestions_str,
        overview = overview,
        machine_json = machine_readable,
        date = chrono::Local::now().format("%Y-%m-%d"),
        tags = info
            .stack
            .iter()
            .map(|s| format!("`{}`", s.to_lowercase()))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn calculate_health_from_info(info: &ProjectInfo, analysis: &ProjectAnalysis) -> (u8, u8, u8) {
    let mut critical_count = 0u8;
    let mut warning_count = 0u8;
    let suggestion_count = analysis.suggestions.len() as u8;

    if info.stack.is_empty() {
        critical_count += 1;
    }
    if !info.has_tests {
        critical_count += 1;
    }
    if !info.has_docs {
        warning_count += 1;
    }
    if !info.has_linting {
        warning_count += 1;
    }
    if info.complexity == "large" && !info.has_linting {
        critical_count += 1;
    }

    let base: i16 = 100;
    let critical_penalty = (critical_count as i16) * 20;
    let warning_penalty = (warning_count as i16) * 5;
    let suggestion_penalty = (suggestion_count as i16) * 2;

    let has_basics = info.has_tests && info.has_docs;
    let bonus = if has_basics { 5 } else { 0 };

    let health_score = base - critical_penalty - warning_penalty - suggestion_penalty + bonus;
    let health_score = health_score.clamp(0, 100) as u8;

    (health_score, critical_count, warning_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_analysis() {
        let info = ProjectInfo {
            name: "test".to_string(),
            stack: HashSet::new(),
            project_type: "prototype".to_string(),
            has_tests: false,
            has_docs: false,
            has_docker: false,
            has_linting: false,
            complexity: "prototype".to_string(),
            detected_paths: vec![],
            readme_content: None,
            validation_commands: vec![],
        };

        let analysis = ProjectAnalysis::analyze(&info);
        assert!(!analysis.issues.is_empty());
        assert!(analysis.suggestions.len() > 2);
    }
}
