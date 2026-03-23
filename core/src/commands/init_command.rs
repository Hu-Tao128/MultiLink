use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

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
    pub guidance_files: Vec<String>,
    pub markdown_files: Vec<String>,
    pub roadmap_files: Vec<String>,
    pub module_readmes: Vec<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InitCommand {
    #[serde(default)]
    pub force: bool,
    #[serde(default)]
    pub smart: bool,
    #[serde(default)]
    pub merge: bool,
    #[serde(default)]
    pub strict: bool,
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
        return if command.smart {
            InitResult {
                success: true,
                message: "MULTILINK.md ya existe. Usa --force para sobrescribir o --merge para actualizar.".to_string(),
                file_path: Some(multilink_path),
                project_info: Some(info),
                analysis: Some(analysis),
                action: InitAction::Exists,
                health_score: Some(health_score),
                critical_issues: Some(critical_issues),
                warnings: Some(warnings),
            }
        } else {
            InitResult {
                success: false,
                message: "MULTILINK.md ya existe. Usa /init --force para sobrescribir.".to_string(),
                file_path: Some(multilink_path),
                project_info: None,
                analysis: None,
                action: InitAction::Exists,
                health_score: Some(health_score),
                critical_issues: Some(critical_issues),
                warnings: Some(warnings),
            }
        };
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

    let content = generate_multilink_md(&info, &analysis, project_root, command.strict);

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
        guidance_files: Vec::new(),
        markdown_files: Vec::new(),
        roadmap_files: Vec::new(),
        module_readmes: Vec::new(),
    };

    info.name = detect_project_name(project_root);
    info.detected_paths = detect_directories(project_root);

    detect_stack_files(project_root, &mut info);
    detect_test_files(project_root, &mut info);
    detect_docs(project_root, &mut info);
    detect_docker(project_root, &mut info);
    detect_linting(project_root, &mut info);

    info.validation_commands = detect_validation_commands(project_root, &info);
    info.guidance_files = detect_guidance_files(project_root);

    let (markdown_files, roadmap_files, module_readmes) = detect_markdown_files(project_root);
    info.markdown_files = markdown_files;
    info.roadmap_files = roadmap_files;
    info.module_readmes = module_readmes;
    if !info.markdown_files.is_empty() {
        info.has_docs = true;
    }

    info.project_type = infer_project_type(&info);
    info.complexity = infer_complexity(project_root);

    if let Some(readme_path) = find_readme_path(project_root) {
        if let Ok(readme) = fs::read_to_string(readme_path) {
            let first_lines: String = readme.lines().take(10).collect::<Vec<_>>().join("\n");
            info.readme_content = Some(first_lines);
        }
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
        .max_depth(8)
        .into_iter()
        .filter_entry(|e| !is_ignored_path(e.path()))
        .filter_map(|e| e.ok());

    let mut has_html = false;
    let mut has_css = false;
    let mut has_js = false;
    let mut has_ts = false;
    let mut has_csharp = false;
    let mut has_prisma = false;
    let mut has_sql = false;
    let mut web_signal_outside_flutter_web_dir = false;

    let is_flutter_project = root.join("pubspec.yaml").exists();

    for entry in entries {
        if let Some(name) = entry.file_name().to_str() {
            let lower = name.to_ascii_lowercase();

            if lower.ends_with(".qml") {
                info.stack.insert("Qt/QML".to_string());
            }
            if lower.ends_with(".csproj") {
                info.stack.insert("C#".to_string());
                has_csharp = true;
            }
            if lower.ends_with(".cs") {
                has_csharp = true;
            }
            if lower.ends_with(".cs") && lower.contains("xamarin") {
                info.stack.insert("Xamarin".to_string());
            }
            if lower.ends_with(".prisma") || lower == "schema.prisma" {
                has_prisma = true;
            }
            if lower.ends_with(".sql") {
                has_sql = true;
            }
            let is_flutter_scaffold_web =
                is_flutter_project && is_flutter_web_scaffold_file(root, entry.path());

            if (lower.ends_with(".html") || lower.ends_with(".htm")) && !is_flutter_scaffold_web {
                has_html = true;
                if is_flutter_project && !is_under_directory(root, entry.path(), "web") {
                    web_signal_outside_flutter_web_dir = true;
                }
            }
            if lower.ends_with(".css") && !is_flutter_scaffold_web {
                has_css = true;
                if is_flutter_project && !is_under_directory(root, entry.path(), "web") {
                    web_signal_outside_flutter_web_dir = true;
                }
            }
            if (lower.ends_with(".js") || lower.ends_with(".jsx")) && !is_flutter_scaffold_web {
                has_js = true;
                if is_flutter_project && !is_under_directory(root, entry.path(), "web") {
                    web_signal_outside_flutter_web_dir = true;
                }
            }
            if (lower.ends_with(".ts") || lower.ends_with(".tsx")) && !is_flutter_scaffold_web {
                has_ts = true;
                if is_flutter_project && !is_under_directory(root, entry.path(), "web") {
                    web_signal_outside_flutter_web_dir = true;
                }
            }
        }
    }

    if is_flutter_project && !web_signal_outside_flutter_web_dir {
        has_html = false;
        has_css = false;
        has_js = false;
        has_ts = false;
    }

    if has_html || has_css || has_js || has_ts {
        let mut web_stack = Vec::new();
        if has_html {
            web_stack.push("HTML");
        }
        if has_css {
            web_stack.push("CSS");
        }
        if has_js {
            web_stack.push("JavaScript");
        }
        if has_ts {
            web_stack.push("TypeScript");
        }
        for tech in web_stack {
            info.stack.insert(tech.to_string());
        }
    }

    if has_csharp {
        info.stack.insert("C#".to_string());
    }
    if has_prisma {
        info.stack.insert("Prisma".to_string());
    }
    if has_sql {
        info.stack.insert("SQL".to_string());
    }

    let package_json = root.join("package.json");
    if package_json.exists()
        && fs::read_to_string(&package_json)
            .map(|content| {
                let lower = content.to_ascii_lowercase();
                lower.contains("\"react\"") || lower.contains("\"react-dom\"")
            })
            .unwrap_or(false)
    {
        info.stack.insert("React".to_string());
    }
}

fn is_under_directory(root: &Path, path: &Path, directory: &str) -> bool {
    path.strip_prefix(root)
        .ok()
        .and_then(|relative| relative.components().next())
        .map(|component| match component {
            Component::Normal(name) => name
                .to_str()
                .map(|s| s.eq_ignore_ascii_case(directory))
                .unwrap_or(false),
            _ => false,
        })
        .unwrap_or(false)
}

fn is_flutter_web_scaffold_file(root: &Path, path: &Path) -> bool {
    if !is_under_directory(root, path, "web") {
        return false;
    }

    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };

    matches!(
        name,
        "index.html" | "manifest.json" | "flutter_service_worker.js" | "favicon.png"
    )
}

fn detect_guidance_files(root: &Path) -> Vec<String> {
    let candidates = [
        "AGENTS.md",
        "agents.md",
        "GEMINI.md",
        "gemini.md",
        "CLAUDE.md",
        "claude.md",
        "COPILOT.md",
        "copilot.md",
        ".cursorrules",
        ".github/copilot-instructions.md",
    ];

    let mut files = candidates
        .iter()
        .filter_map(|relative| {
            let path = root.join(relative);
            if path.is_file() {
                Some(relative.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let cursor_rules_dir = root.join(".cursor").join("rules");
    if cursor_rules_dir.is_dir() {
        let entries = walkdir::WalkDir::new(&cursor_rules_dir)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| {
                e.path()
                    .strip_prefix(root)
                    .ok()
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
            })
            .take(10)
            .collect::<Vec<_>>();

        if entries.is_empty() {
            files.push(".cursor/rules/".to_string());
        } else {
            files.extend(entries);
        }
    }

    files.sort();
    files.dedup();
    files
}

fn detect_markdown_files(root: &Path) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut markdown_files = Vec::new();
    let mut roadmap_files = Vec::new();
    let mut module_readmes = Vec::new();

    let entries = walkdir::WalkDir::new(root)
        .max_depth(8)
        .into_iter()
        .filter_entry(|e| !is_ignored_path(e.path()))
        .filter_map(|e| e.ok());

    for entry in entries {
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };

        if !ext.eq_ignore_ascii_case("md") {
            continue;
        }

        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };

        let rel = relative.to_string_lossy().replace('\\', "/");
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        markdown_files.push(rel.clone());

        if file_name.contains("roadmap") {
            roadmap_files.push(rel.clone());
        }

        if file_name == "readme.md" && path.parent().map(|p| p != root).unwrap_or(false) {
            module_readmes.push(rel);
        }
    }

    markdown_files.sort();
    roadmap_files.sort();
    module_readmes.sort();

    (markdown_files, roadmap_files, module_readmes)
}

fn detect_test_files(root: &Path, info: &mut ProjectInfo) {
    let test_patterns = ["tests", "test", "__tests__", "spec", "features"];

    for pattern in test_patterns {
        if root.join(pattern).is_dir() {
            info.has_tests = true;
            break;
        }
    }

    if root.join("Cargo.toml").exists()
        && (root.join("tests").is_dir() || root.join("src/tests").is_dir())
    {
        info.has_tests = true;
    }

    if let Ok(content) = fs::read_to_string(root.join("package.json")) {
        if content.contains("\"test\"") {
            info.has_tests = true;
        }
    }
}

fn detect_docs(root: &Path, info: &mut ProjectInfo) {
    if find_readme_path(root).is_some() {
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
        "analysis_options.yaml",
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
    if info.stack.contains("HTML")
        || info.stack.contains("CSS")
        || info.stack.contains("JavaScript")
    {
        return "web frontend".to_string();
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
        .filter_entry(|e| !is_ignored_path(e.path()))
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

fn is_ignored_path(path: &Path) -> bool {
    const IGNORED_DIRS: [&str; 14] = [
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
        "out",
        "bin",
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

fn format_markdown_paths(paths: &[String], limit: usize, empty_message: &str) -> String {
    if paths.is_empty() {
        return format!("- ({})", empty_message);
    }

    let mut rows = paths
        .iter()
        .take(limit)
        .map(|p| format!("- `{}`", p))
        .collect::<Vec<_>>();

    if paths.len() > limit {
        rows.push(format!("- ... y {} archivos más", paths.len() - limit));
    }

    rows.join("\n")
}

fn build_markdown_highlights(
    project_root: &Path,
    markdown_files: &[String],
    max_files: usize,
    max_highlights: usize,
) -> Vec<(String, String)> {
    let mut highlights = Vec::new();

    for relative in markdown_files.iter().take(max_files) {
        let path = project_root.join(relative);
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };

        if let Some(line) = extract_first_meaningful_markdown_line(&content) {
            highlights.push((relative.clone(), line));
        }

        if highlights.len() >= max_highlights {
            break;
        }
    }

    highlights
}

fn extract_first_meaningful_markdown_line(content: &str) -> Option<String> {
    let mut in_code_fence = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.starts_with("```") {
            in_code_fence = !in_code_fence;
            continue;
        }
        if in_code_fence || line.is_empty() || line.starts_with('#') {
            continue;
        }

        let cleaned = line
            .trim_start_matches(['-', '*', '>'])
            .trim()
            .replace('`', "");
        if cleaned.len() >= 24 {
            return Some(cleaned);
        }
    }

    None
}

pub fn generate_multilink_md(
    info: &ProjectInfo,
    analysis: &ProjectAnalysis,
    project_root: &Path,
    strict: bool,
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

    let markdown_highlights = build_markdown_highlights(project_root, &info.markdown_files, 8, 4);

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
        .or_else(|| {
            if markdown_highlights.is_empty() {
                None
            } else {
                Some(
                    markdown_highlights
                        .iter()
                        .take(2)
                        .map(|(_, line)| line.clone())
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
            }
        })
        .unwrap_or_else(|| {
            if strict {
                "No se detectó resumen desde archivos .md".to_string()
            } else {
                format!("{} project with {} stack.", project_type, stack_str)
            }
        });

    let validation_str = if info.validation_commands.is_empty() {
        if strict {
            "- (No se detectaron comandos de validación)".to_string()
        } else {
            "- (No standard validation commands detected)".to_string()
        }
    } else {
        info.validation_commands
            .iter()
            .map(|v| format!("- `{}`: {}", v.name, v.command))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let runtime_commands = generate_runtime_commands(info);
    let quick_reference = generate_quick_reference(info);
    let architecture_boundaries = generate_architecture_boundaries(info);
    let roadmaps_str =
        format_markdown_paths(&info.roadmap_files, 8, "No se detectaron roadmaps en docs/");
    let module_docs_str = format_markdown_paths(
        &info.module_readmes,
        10,
        "No se detectaron README.md por módulo",
    );
    let markdown_highlights_str = if markdown_highlights.is_empty() {
        "- (No se detectaron highlights en documentación markdown)".to_string()
    } else {
        markdown_highlights
            .iter()
            .map(|(path, line)| format!("- `{}`: {}", path, line))
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
        "guidance_files": info.guidance_files,
        "roadmap_files": info.roadmap_files,
        "module_readmes": info.module_readmes,
        "markdown_files": info.markdown_files,
    })
    .to_string();

    let guidance_str = if info.guidance_files.is_empty() {
        "- (No se detectaron archivos de guía de agentes)".to_string()
    } else {
        info.guidance_files
            .iter()
            .map(|p| format!("- `{}`", p))
            .collect::<Vec<_>>()
            .join("\n")
    };

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

{roadmaps}

### Module Docs

{module_docs}

### Markdown Highlights

{markdown_highlights}

---

## ⚙️ Execution Rules

1. Leer este archivo primero
2. Usar roadmap si existe en docs/
3. Ejecutar UNA tarea a la vez
4. Validar antes de completar (ver comandos abajo)
5. No asumir contexto faltante

### Comandos de Validación

{validation}

### Agent Guidance Files

{guidance_files}

---

## 🔒 Constraints

- No romper código existente
- Mantener compatibilidad
- Preferir cambios pequeños
- No compartir credenciales

---

## 🏃 Execution Context

### Runtime Commands

{runtime_commands}

### Quick Reference

```bash
{quick_reference}
```

### Architecture Boundaries

{architecture_boundaries}

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
        guidance_files = guidance_str,
        issues = issues_str,
        suggestions = suggestions_str,
        overview = overview,
        runtime_commands = runtime_commands,
        quick_reference = quick_reference,
        architecture_boundaries = architecture_boundaries,
        roadmaps = roadmaps_str,
        module_docs = module_docs_str,
        markdown_highlights = markdown_highlights_str,
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

fn find_readme_path(project_root: &Path) -> Option<PathBuf> {
    let root_canonical = project_root.canonicalize().ok();
    ["README.md", "readme.md", "Readme.md"]
        .iter()
        .map(|name| project_root.join(name))
        .find(|candidate| {
            if !candidate.is_file() {
                return false;
            }

            if let (Some(root), Ok(candidate_canonical)) =
                (root_canonical.as_ref(), candidate.canonicalize())
            {
                return candidate_canonical.starts_with(root);
            }

            true
        })
}

fn generate_runtime_commands(info: &ProjectInfo) -> String {
    let mut commands: Vec<String> = Vec::new();

    if info.stack.contains("Rust") {
        commands.push("- **Build:** `cargo build`".to_string());
        commands.push("- **Test:** `cargo test`".to_string());
        commands.push("- **Single test:** `cargo test <test_name_substring>`".to_string());
        commands.push("- **Lint:** `cargo clippy -- -D warnings`".to_string());
        commands.push("- **Format:** `cargo fmt --all`".to_string());
    }

    if info.stack.contains("Node.js") {
        commands.push("- **Install:** `npm install`".to_string());
        commands.push("- **Test:** `npm test`".to_string());
        commands.push("- **Single test:** `npm test -- <path-or-pattern>`".to_string());
        commands.push("- **Build:** `npm run build`".to_string());
    }

    if info.stack.contains("TypeScript") {
        commands.push("- **Typecheck:** `npx tsc --noEmit`".to_string());
    }

    if info.stack.contains("React") {
        commands.push("- **Dev server:** `npm run dev`".to_string());
        commands.push("- **React test (single):** `npm test -- <component-or-spec>`".to_string());
    }

    if info.stack.contains("Python") {
        commands.push("- **Install:** `python -m pip install -r requirements.txt`".to_string());
        commands.push("- **Test:** `pytest`".to_string());
        commands.push("- **Single test:** `pytest path/to/test_file.py::test_name`".to_string());
    }

    if info.stack.contains("C++") {
        commands.push("- **Configure:** `cmake -S . -B build`".to_string());
        commands.push("- **Build:** `cmake --build build`".to_string());
    }

    if info.stack.contains("Go") {
        commands.push("- **Build:** `go build ./...`".to_string());
        commands.push("- **Test:** `go test ./...`".to_string());
        commands.push("- **Single test:** `go test ./path/to/pkg -run TestName`".to_string());
    }

    if info.stack.contains("C#") {
        commands.push("- **Restore:** `dotnet restore`".to_string());
        commands.push("- **Build:** `dotnet build`".to_string());
        commands.push("- **Test:** `dotnet test`".to_string());
        commands.push(
            "- **Single test:** `dotnet test --filter FullyQualifiedName~TestName`".to_string(),
        );
    }

    if info.stack.contains("Prisma") {
        commands.push("- **Prisma validate:** `npx prisma validate`".to_string());
        commands.push("- **Prisma migrate status:** `npx prisma migrate status`".to_string());
        commands.push("- **Prisma migrate dev:** `npx prisma migrate dev`".to_string());
    }

    if info.stack.contains("Flutter") {
        commands.push("- **Install:** `flutter pub get`".to_string());
        commands.push("- **Analyze:** `flutter analyze`".to_string());
        commands.push("- **Test:** `flutter test`".to_string());
        commands.push("- **Single file test:** `flutter test test/widget_test.dart`".to_string());
        commands
            .push("- **Single named test:** `flutter test --plain-name \"test name\"`".to_string());
    }

    if commands.is_empty() {
        "- (No runtime commands detected)".to_string()
    } else {
        commands.join("\n")
    }
}

fn generate_quick_reference(info: &ProjectInfo) -> String {
    let mut commands = Vec::new();

    if info.stack.contains("Rust") {
        commands.push("cargo build");
        commands.push("cargo test");
        commands.push("cargo test <test_name_substring>");
    }
    if info.stack.contains("Node.js") {
        commands.push("npm install");
        commands.push("npm run build");
        commands.push("npm test -- <path-or-pattern>");
    }
    if info.stack.contains("TypeScript") {
        commands.push("npx tsc --noEmit");
    }
    if info.stack.contains("React") {
        commands.push("npm run dev");
    }
    if info.stack.contains("Python") {
        commands.push("python -m pip install -r requirements.txt");
        commands.push("pytest");
        commands.push("pytest path/to/test_file.py::test_name");
    }
    if info.stack.contains("C++") {
        commands.push("cmake -S . -B build");
        commands.push("cmake --build build");
    }
    if info.stack.contains("Go") {
        commands.push("go test ./...");
        commands.push("go test ./path/to/pkg -run TestName");
    }
    if info.stack.contains("C#") {
        commands.push("dotnet test");
        commands.push("dotnet test --filter FullyQualifiedName~TestName");
    }
    if info.stack.contains("Prisma") {
        commands.push("npx prisma validate");
        commands.push("npx prisma migrate status");
    }
    if info.stack.contains("Flutter") {
        commands.push("flutter pub get");
        commands.push("flutter test");
        commands.push("flutter test test/widget_test.dart");
        commands.push("flutter test --plain-name \"test name\"");
    }

    if commands.is_empty() {
        "# No quick reference commands detected".to_string()
    } else {
        commands.join("\n")
    }
}

fn generate_architecture_boundaries(info: &ProjectInfo) -> String {
    let mut boundaries = Vec::new();

    if info
        .detected_paths
        .iter()
        .any(|p| p == "core" || p == "lib" || p == "shared")
    {
        boundaries.push("- `core/` o `lib/` → lógica de dominio y componentes reutilizables");
    }

    if info
        .detected_paths
        .iter()
        .any(|p| p == "gui" || p == "frontend" || p == "app")
    {
        boundaries.push("- `gui/`, `frontend/` o `app/` → capa de UI e interacción");
    }

    if info
        .detected_paths
        .iter()
        .any(|p| p == "backend" || p == "api")
    {
        boundaries.push("- `backend/` o `api/` → servicios, endpoints e integración externa");
    }

    if boundaries.is_empty() {
        "- (No se detectaron límites de arquitectura explícitos)".to_string()
    } else {
        boundaries.join("\n")
    }
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
    use std::iter::FromIterator;
    use tempfile::tempdir;

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
            guidance_files: vec![],
            markdown_files: vec![],
            roadmap_files: vec![],
            module_readmes: vec![],
        };

        let analysis = ProjectAnalysis::analyze(&info);
        assert!(!analysis.issues.is_empty());
        assert!(analysis.suggestions.len() > 2);
    }

    #[test]
    fn test_generate_multilink_md_runtime_commands_are_dynamic() {
        let info = ProjectInfo {
            name: "fitbalance-backend".to_string(),
            stack: HashSet::from_iter(["Node.js".to_string()]),
            project_type: "backend API".to_string(),
            has_tests: true,
            has_docs: true,
            has_docker: false,
            has_linting: true,
            complexity: "small".to_string(),
            detected_paths: vec!["backend".to_string(), "api".to_string()],
            readme_content: Some("API backend para una app de nutricion".to_string()),
            validation_commands: vec![ValidationCommand {
                name: "Build".to_string(),
                command: "npm run build".to_string(),
            }],
            guidance_files: vec!["AGENTS.md".to_string()],
            markdown_files: vec!["README.md".to_string()],
            roadmap_files: vec![],
            module_readmes: vec![],
        };

        let analysis = ProjectAnalysis::analyze(&info);
        let content = generate_multilink_md(&info, &analysis, Path::new("."), false);

        assert!(content.contains("npm run build"));
        assert!(!content.contains("cargo build --manifest-path core/Cargo.toml"));
        assert!(!content.contains("./build/gui/multilink_gui"));
    }

    #[test]
    fn test_generate_multilink_md_strict_does_not_use_generic_overview_fallback() {
        let info = ProjectInfo {
            name: "sample".to_string(),
            stack: HashSet::new(),
            project_type: "software project".to_string(),
            has_tests: false,
            has_docs: false,
            has_docker: false,
            has_linting: false,
            complexity: "prototype".to_string(),
            detected_paths: vec![],
            readme_content: None,
            validation_commands: vec![],
            guidance_files: vec![],
            markdown_files: vec![],
            roadmap_files: vec![],
            module_readmes: vec![],
        };

        let analysis = ProjectAnalysis::analyze(&info);
        let content = generate_multilink_md(&info, &analysis, Path::new("."), true);

        assert!(content.contains("No se detectó resumen desde archivos .md"));
        assert!(!content.contains("project with"));
    }

    #[test]
    fn flutter_project_ignores_default_web_scaffold_html() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();

        fs::write(
            root.join("pubspec.yaml"),
            "name: study_medical\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\n",
        )
        .expect("write pubspec");
        fs::create_dir_all(root.join("lib")).expect("create lib");
        fs::write(root.join("lib/main.dart"), "void main() {}\n").expect("write dart");
        fs::create_dir_all(root.join("web")).expect("create web");
        fs::write(root.join("web/index.html"), "<html></html>\n").expect("write index html");

        let info = scan_project(root);
        assert!(info.stack.contains("Flutter"));
        assert!(!info.stack.contains("HTML"));
    }

    #[test]
    fn flutter_project_detects_html_when_outside_web_scaffold() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();

        fs::write(
            root.join("pubspec.yaml"),
            "name: study_medical\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\n",
        )
        .expect("write pubspec");
        fs::create_dir_all(root.join("lib")).expect("create lib");
        fs::write(root.join("lib/main.dart"), "void main() {}\n").expect("write dart");
        fs::create_dir_all(root.join("docs")).expect("create docs");
        fs::write(root.join("docs/landing.html"), "<html></html>\n").expect("write html");

        let info = scan_project(root);
        assert!(info.stack.contains("Flutter"));
        assert!(info.stack.contains("HTML"));
    }
}
