use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const CLUSTER_EMBED_SERVER_TIMEOUT_SECS: u64 = 2;

#[derive(Debug, Clone)]
pub struct RetrievalConfig {
    pub embeddings_enabled: bool,
    pub embed_model: String,
    pub embed_base_url: String,
    pub ollama_base_url: String,
    pub embed_connect_timeout_ms: u64,
    pub embed_request_timeout_ms: u64,
    pub embed_max_retries: u8,
    pub embed_batch_size: usize,
    pub top_k: usize,
}

#[derive(Debug, Clone, Default)]
pub struct EmbeddingDiagnostics {
    pub used: bool,
    pub latency_ms: u128,
    pub reason: String,
    pub attempts: u8,
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct RetrievalResult {
    pub context: String,
    pub selected_files: Vec<String>,
    pub used_tokens: usize,
    pub top_k: usize,
    pub embedding_used: bool,
    pub embedding_diag: EmbeddingDiagnostics,
}

pub async fn resolve_cluster_embedding_server(
    config: &RetrievalConfig,
    candidate_servers: &[String],
) -> Option<String> {
    let servers = unique_nonempty_servers(candidate_servers);
    if servers.is_empty() {
        return None;
    }

    if !config.embed_model.trim().is_empty() {
        for server in &servers {
            let found = tokio::time::timeout(
                Duration::from_secs(CLUSTER_EMBED_SERVER_TIMEOUT_SECS),
                model_exists_on_server(server, &config.embed_model, config),
            )
            .await
            .ok()
            .unwrap_or(false);
            if found {
                return Some(server.clone());
            }
        }
    }

    for server in &servers {
        let capable = tokio::time::timeout(
            Duration::from_secs(CLUSTER_EMBED_SERVER_TIMEOUT_SECS),
            server_has_embedding_capability(server, config),
        )
        .await
        .ok()
        .unwrap_or(false);
        if capable {
            return Some(server.clone());
        }
    }

    None
}

fn unique_nonempty_servers(candidates: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for candidate in candidates {
        let trimmed = candidate.trim().trim_end_matches('/');
        if trimmed.is_empty() {
            continue;
        }
        let normalized = trimmed.to_string();
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    out
}

async fn model_exists_on_server(base_url: &str, model: &str, config: &RetrievalConfig) -> bool {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(
            config.embed_connect_timeout_ms.min(2_000),
        ))
        .timeout(Duration::from_millis(
            config.embed_request_timeout_ms.min(2_000),
        ))
        .build();
    let Ok(client) = client else {
        return false;
    };

    let body = serde_json::json!({ "model": model });
    let response = client
        .post(format!("{}/api/show", base_url.trim_end_matches('/')))
        .json(&body)
        .send()
        .await;
    let Ok(response) = response else {
        return false;
    };
    response.status().is_success()
}

async fn server_has_embedding_capability(base_url: &str, config: &RetrievalConfig) -> bool {
    let mut probe_cfg = config.clone();
    probe_cfg.embed_base_url = base_url.to_string();
    probe_cfg.embed_max_retries = 0;
    probe_cfg.embed_connect_timeout_ms = probe_cfg.embed_connect_timeout_ms.min(2_000);
    probe_cfg.embed_request_timeout_ms = probe_cfg.embed_request_timeout_ms.min(2_000);

    match fetch_ollama_model_details(base_url, &probe_cfg).await {
        Ok(details) => details.iter().any(|(name, info)| {
            info.supports_embedding || name.to_ascii_lowercase().contains("embed")
        }),
        Err(_) => false,
    }
}

#[derive(Clone)]
struct ProjectChunk {
    path: String,
    language: String,
    content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectType {
    Rust,
    Node,
    Python,
    Java,
    Go,
    Web,
    CSharp,
    Php,
    Unknown,
}

impl ProjectType {
    fn as_str(self) -> &'static str {
        match self {
            ProjectType::Rust => "rust",
            ProjectType::Node => "node",
            ProjectType::Python => "python",
            ProjectType::Java => "java",
            ProjectType::Go => "go",
            ProjectType::Web => "web",
            ProjectType::CSharp => "csharp",
            ProjectType::Php => "php",
            ProjectType::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
struct ProjectDetection {
    project_type: ProjectType,
    detected_from: String,
}

pub async fn build_relevant_project_context(
    raw_context: &str,
    prompt: &str,
    token_budget: usize,
    model_hint: Option<&str>,
    config: RetrievalConfig,
) -> RetrievalResult {
    if raw_context.trim().is_empty() || token_budget == 0 {
        return RetrievalResult {
            context: String::new(),
            selected_files: Vec::new(),
            used_tokens: 0,
            top_k: config.top_k,
            embedding_used: false,
            embedding_diag: EmbeddingDiagnostics::default(),
        };
    }

    let chunks = parse_project_chunks(raw_context);
    if chunks.is_empty() {
        let context = truncate_to_token_budget(raw_context, token_budget, model_hint);
        let used_tokens = estimate_tokens_for_model(&context, model_hint);
        return RetrievalResult {
            context,
            selected_files: Vec::new(),
            used_tokens,
            top_k: config.top_k,
            embedding_used: false,
            embedding_diag: EmbeddingDiagnostics::default(),
        };
    }

    let lexical = lexical_scores(prompt, &chunks);
    let project_detection = detect_project_type(&chunks);
    let preferred_fallback = context_files_for_type(project_detection.project_type, &chunks);
    let (embedding, embedding_diag) =
        maybe_embedding_scores(prompt, &chunks, &config, model_hint).await;
    let mut embedding_diag = embedding_diag;
    let embedding_used = embedding_diag.used;

    if embedding_diag.reason.is_empty() {
        embedding_diag.reason = format!(
            "project_type={} detected_from={}",
            project_detection.project_type.as_str(),
            project_detection.detected_from
        );
    } else {
        embedding_diag.reason = format!(
            "{} project_type={} detected_from={}",
            embedding_diag.reason,
            project_detection.project_type.as_str(),
            project_detection.detected_from
        );
    }

    if !embedding_used {
        if let Some((context, selected_files, used_tokens)) = build_context_from_preferred_files(
            &chunks,
            &preferred_fallback,
            token_budget,
            model_hint,
        ) {
            return RetrievalResult {
                context,
                selected_files,
                used_tokens,
                top_k: config.top_k.clamp(2, 24),
                embedding_used,
                embedding_diag,
            };
        }
    }

    let mut ranked: Vec<(usize, f32)> = lexical
        .into_iter()
        .map(|(idx, score)| {
            let final_score = if let Some(embed_scores) = embedding.as_ref() {
                let emb = embed_scores.get(idx).copied().unwrap_or(0.0);
                score * 0.35 + emb * 0.65
            } else {
                score
            };
            (idx, final_score)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

    let top_k = config.top_k.clamp(2, 24);
    let mut selected_blocks = Vec::new();
    let mut selected_files = Vec::new();
    let mut used_tokens = 0usize;

    for (idx, _) in ranked.into_iter().take(top_k) {
        let Some(chunk) = chunks.get(idx) else {
            continue;
        };
        let block = render_chunk_block(chunk);
        let block_tokens = estimate_tokens_for_model(&block, model_hint);

        if used_tokens + block_tokens > token_budget {
            if used_tokens == 0 {
                let trimmed = truncate_to_token_budget(&block, token_budget, model_hint);
                if !trimmed.trim().is_empty() {
                    used_tokens = estimate_tokens_for_model(&trimmed, model_hint);
                    selected_blocks.push(trimmed);
                    selected_files.push(chunk.path.clone());
                }
            }
            break;
        }

        used_tokens += block_tokens;
        selected_blocks.push(block);
        selected_files.push(chunk.path.clone());
    }

    if selected_blocks.is_empty() {
        let fallback = truncate_to_token_budget(raw_context, token_budget, model_hint);
        let used_tokens = estimate_tokens_for_model(&fallback, model_hint);
        return RetrievalResult {
            context: fallback,
            selected_files,
            used_tokens,
            top_k,
            embedding_used,
            embedding_diag,
        };
    }

    let mut context = String::new();
    context.push_str("Project Context:\n");
    for block in selected_blocks {
        context.push_str(&block);
        context.push('\n');
    }

    if estimate_tokens_for_model(&context, model_hint) > token_budget {
        context = truncate_to_token_budget(&context, token_budget, model_hint);
        used_tokens = estimate_tokens_for_model(&context, model_hint);
    }

    RetrievalResult {
        context,
        selected_files,
        used_tokens,
        top_k,
        embedding_used,
        embedding_diag,
    }
}

fn build_context_from_preferred_files(
    chunks: &[ProjectChunk],
    preferred_paths: &[String],
    token_budget: usize,
    model_hint: Option<&str>,
) -> Option<(String, Vec<String>, usize)> {
    if preferred_paths.is_empty() || token_budget == 0 {
        return None;
    }

    let mut selected_blocks = Vec::new();
    let mut selected_files = Vec::new();
    let mut used_tokens = 0usize;

    for preferred in preferred_paths {
        let Some(chunk) = chunks.iter().find(|c| c.path == *preferred) else {
            continue;
        };

        let block = render_chunk_block(chunk);
        let block_tokens = estimate_tokens_for_model(&block, model_hint);
        if used_tokens + block_tokens > token_budget {
            if used_tokens == 0 {
                let trimmed = truncate_to_token_budget(&block, token_budget, model_hint);
                if !trimmed.trim().is_empty() {
                    used_tokens = estimate_tokens_for_model(&trimmed, model_hint);
                    selected_blocks.push(trimmed);
                    selected_files.push(chunk.path.clone());
                }
            }
            break;
        }

        used_tokens += block_tokens;
        selected_blocks.push(block);
        selected_files.push(chunk.path.clone());
    }

    if selected_blocks.is_empty() {
        return None;
    }

    let mut context = String::from("Project Context:\n");
    for block in selected_blocks {
        context.push_str(&block);
        context.push('\n');
    }

    if estimate_tokens_for_model(&context, model_hint) > token_budget {
        context = truncate_to_token_budget(&context, token_budget, model_hint);
        used_tokens = estimate_tokens_for_model(&context, model_hint);
    }

    Some((context, selected_files, used_tokens))
}

fn detect_project_type(chunks: &[ProjectChunk]) -> ProjectDetection {
    if chunks.is_empty() {
        return ProjectDetection {
            project_type: ProjectType::Unknown,
            detected_from: "none".to_string(),
        };
    }

    let has_root = |name: &str| chunks.iter().any(|c| c.path.eq_ignore_ascii_case(name));
    let has_src_java = chunks
        .iter()
        .any(|c| c.path.starts_with("src/") && c.path.ends_with(".java"));
    let has_root_html = chunks.iter().any(|c| {
        c.path.matches('/').count() == 0 && c.path.to_ascii_lowercase().ends_with(".html")
    });
    let has_csproj = chunks
        .iter()
        .any(|c| c.path.to_ascii_lowercase().ends_with(".csproj"));
    let has_sln = chunks
        .iter()
        .any(|c| c.path.to_ascii_lowercase().ends_with(".sln"));

    if has_root("Cargo.toml") {
        return ProjectDetection {
            project_type: ProjectType::Rust,
            detected_from: "Cargo.toml".to_string(),
        };
    }
    if has_root("package.json") {
        return ProjectDetection {
            project_type: ProjectType::Node,
            detected_from: "package.json".to_string(),
        };
    }
    if has_root("pyproject.toml") {
        return ProjectDetection {
            project_type: ProjectType::Python,
            detected_from: "pyproject.toml".to_string(),
        };
    }
    if has_root("requirements.txt") {
        return ProjectDetection {
            project_type: ProjectType::Python,
            detected_from: "requirements.txt".to_string(),
        };
    }
    if has_root("setup.py") {
        return ProjectDetection {
            project_type: ProjectType::Python,
            detected_from: "setup.py".to_string(),
        };
    }
    if has_root("pom.xml") {
        return ProjectDetection {
            project_type: ProjectType::Java,
            detected_from: "pom.xml".to_string(),
        };
    }
    if has_root("build.gradle") {
        return ProjectDetection {
            project_type: ProjectType::Java,
            detected_from: "build.gradle".to_string(),
        };
    }
    if has_src_java {
        return ProjectDetection {
            project_type: ProjectType::Java,
            detected_from: "src/**/*.java".to_string(),
        };
    }
    if has_root("go.mod") {
        return ProjectDetection {
            project_type: ProjectType::Go,
            detected_from: "go.mod".to_string(),
        };
    }
    if has_root("index.html") || has_root_html {
        return ProjectDetection {
            project_type: ProjectType::Web,
            detected_from: if has_root("index.html") {
                "index.html".to_string()
            } else {
                "*.html".to_string()
            },
        };
    }
    if has_csproj || has_sln {
        return ProjectDetection {
            project_type: ProjectType::CSharp,
            detected_from: if has_csproj {
                "*.csproj".to_string()
            } else {
                "*.sln".to_string()
            },
        };
    }
    if has_root("composer.json") || has_root("index.php") {
        return ProjectDetection {
            project_type: ProjectType::Php,
            detected_from: if has_root("composer.json") {
                "composer.json".to_string()
            } else {
                "index.php".to_string()
            },
        };
    }

    ProjectDetection {
        project_type: ProjectType::Unknown,
        detected_from: "none".to_string(),
    }
}

fn context_files_for_type(project_type: ProjectType, chunks: &[ProjectChunk]) -> Vec<String> {
    match project_type {
        ProjectType::Rust => vec_of_existing(
            chunks,
            &["Cargo.toml", "src/main.rs", "src/lib.rs", "README.md"],
        ),
        ProjectType::Node => vec_of_existing(
            chunks,
            &["package.json", "src/index.js", "index.js", "README.md"],
        ),
        ProjectType::Python => vec_of_existing(
            chunks,
            &[
                "pyproject.toml",
                "requirements.txt",
                "setup.py",
                "main.py",
                "app.py",
            ],
        ),
        ProjectType::Java => {
            let mut out = vec_of_existing(chunks, &["pom.xml", "build.gradle"]);
            if let Some(app) = chunks
                .iter()
                .find(|c| c.path.starts_with("src/main/") && c.path.ends_with("Application.java"))
            {
                out.push(app.path.clone());
            }
            out
        }
        ProjectType::Go => vec_of_existing(chunks, &["go.mod", "main.go"]),
        ProjectType::Web => {
            let mut out = vec_of_existing(chunks, &["index.html"]);
            if let Some(css) = chunks.iter().find(|c| {
                let p = c.path.to_ascii_lowercase();
                p.matches('/').count() == 0
                    && (p == "styles.css" || p == "main.css" || p.ends_with(".css"))
            }) {
                out.push(css.path.clone());
            }
            if let Some(js) = chunks.iter().find(|c| {
                let p = c.path.to_ascii_lowercase();
                p.matches('/').count() == 0
                    && (p == "main.js" || p == "app.js" || p == "index.js" || p.ends_with(".js"))
            }) {
                out.push(js.path.clone());
            }
            out
        }
        ProjectType::CSharp => {
            let mut out = Vec::new();
            if let Some(csproj) = chunks
                .iter()
                .find(|c| c.path.to_ascii_lowercase().ends_with(".csproj"))
            {
                out.push(csproj.path.clone());
            }
            out.extend(vec_of_existing(chunks, &["Program.cs"]));
            out
        }
        ProjectType::Php => {
            let mut out = vec_of_existing(chunks, &["composer.json", "index.php"]);
            out.dedup();
            out
        }
        ProjectType::Unknown => {
            let mut root_files: Vec<&ProjectChunk> = chunks
                .iter()
                .filter(|c| c.path.matches('/').count() == 0)
                .collect();
            root_files.sort_by(|a, b| b.content.len().cmp(&a.content.len()));
            root_files
                .into_iter()
                .take(3)
                .map(|c| c.path.clone())
                .collect()
        }
    }
}

fn vec_of_existing(chunks: &[ProjectChunk], candidates: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for candidate in candidates {
        if let Some(found) = chunks
            .iter()
            .find(|c| c.path.eq_ignore_ascii_case(candidate))
        {
            out.push(found.path.clone());
        }
    }
    out
}

fn parse_project_chunks(raw_context: &str) -> Vec<ProjectChunk> {
    let mut chunks = Vec::new();
    let mut cursor = 0usize;

    while let Some(rel_pos) = raw_context[cursor..].find("File: ") {
        let file_start = cursor + rel_pos;
        let path_end = match raw_context[file_start..].find('\n') {
            Some(v) => file_start + v,
            None => break,
        };

        let path = raw_context[file_start + 6..path_end].trim().to_string();
        if path.is_empty() {
            cursor = path_end.saturating_add(1);
            continue;
        }

        let fence_start = path_end.saturating_add(1);
        if fence_start >= raw_context.len() || !raw_context[fence_start..].starts_with("```") {
            cursor = fence_start;
            continue;
        }

        let fence_end = match raw_context[fence_start..].find('\n') {
            Some(v) => fence_start + v,
            None => break,
        };
        let language = raw_context[fence_start + 3..fence_end].trim().to_string();

        let content_start = fence_end.saturating_add(1);
        let content_rel_end = match raw_context[content_start..].find("\n```") {
            Some(v) => v,
            None => break,
        };
        let content_end = content_start + content_rel_end;
        let content = raw_context[content_start..content_end].to_string();

        chunks.push(ProjectChunk {
            path,
            language,
            content,
        });

        cursor = (content_end + "\n```".len()).min(raw_context.len());
    }

    chunks
}

fn render_chunk_block(chunk: &ProjectChunk) -> String {
    let excerpt: String = chunk.content.chars().take(1800).collect();
    format!(
        "- file: {}\n  language: {}\n  excerpt: |\n    {}\n",
        chunk.path,
        if chunk.language.is_empty() {
            "text"
        } else {
            chunk.language.as_str()
        },
        excerpt.replace('\n', "\n    ")
    )
}

fn lexical_scores(prompt: &str, chunks: &[ProjectChunk]) -> Vec<(usize, f32)> {
    let terms = query_terms(prompt);
    let path_hints = path_hints(prompt);
    let is_project_overview_prompt = looks_like_project_overview_prompt(prompt);
    let mut scores = Vec::with_capacity(chunks.len());

    for (idx, chunk) in chunks.iter().enumerate() {
        let mut score = 0.0f32;
        let path_l = chunk.path.to_ascii_lowercase();
        let lang_l = chunk.language.to_ascii_lowercase();
        let preview: String = chunk.content.chars().take(1600).collect();
        let preview_l = preview.to_ascii_lowercase();
        let depth = chunk.path.matches('/').count();
        let file_name = chunk
            .path
            .rsplit('/')
            .next()
            .unwrap_or(chunk.path.as_str())
            .to_ascii_lowercase();
        let is_readme =
            file_name == "readme.md" || file_name == "readme.txt" || file_name == "readme";
        let is_root_file = depth == 0;
        let ext = chunk
            .path
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let is_code_ext = matches!(
            ext.as_str(),
            "rs" | "py"
                | "js"
                | "jsx"
                | "ts"
                | "tsx"
                | "dart"
                | "java"
                | "kt"
                | "kts"
                | "cpp"
                | "c"
                | "cc"
                | "cxx"
                | "cs"
                | "go"
                | "swift"
                | "php"
                | "rb"
                | "scala"
                | "zig"
        );
        let hidden_or_vendor = path_l.starts_with('.')
            || path_l.contains("/.venv/")
            || path_l.contains("/venv/")
            || path_l.contains("site-packages")
            || path_l.contains("/dist/");
        let in_test_like_tree = path_l.starts_with("tests/")
            || path_l.starts_with("test/")
            || path_l.contains("/tests/")
            || path_l.contains("/test/")
            || path_l.contains("/fixtures/")
            || path_l.contains("/examples/")
            || path_l.contains("/sample/");

        if is_project_overview_prompt {
            if file_name == "readme.md" && is_root_file {
                score += 40.0;
            } else if is_readme {
                score -= 8.0;
            }
            if file_name == "agents.md" || file_name == "gemini.md" || file_name == "claude.md" {
                score -= 20.0;
            }
            if file_name == "package.json"
                || file_name == "cargo.toml"
                || file_name == "pyproject.toml"
                || file_name == "requirements.txt"
                || file_name == "go.mod"
                || file_name == "cmakelists.txt"
            {
                score += if is_root_file { 18.0 } else { 4.0 };
            }
            if in_test_like_tree {
                score -= 18.0;
            }
            if is_code_ext && !is_root_file {
                score -= 6.0;
            }
        }

        if terms.is_empty() {
            if depth <= 1 {
                score += 6.0;
            } else if depth <= 2 {
                score += 2.0;
            }
            if file_name == "main.py"
                || file_name == "main.rs"
                || file_name == "app.py"
                || file_name == "chrome.py"
                || file_name == "tts.py"
            {
                score += 3.0;
            }
            if is_readme {
                if depth == 0 {
                    score += 4.0;
                } else {
                    score -= 2.0;
                }
            }
            if is_code_ext {
                if depth <= 1 {
                    score += 4.0;
                } else {
                    score += 1.0;
                }
            }
            if !hidden_or_vendor {
                score += 2.0;
            }
            score += (1.0 / ((idx + 1) as f32)).max(0.01);
        } else {
            let file_name_only = file_name.split('.').next().unwrap_or(&file_name);
            let mut terms_matched_in_filename = 0;

            for term in &terms {
                if path_l.contains(term) {
                    score += 8.0;
                }
                if file_name_only.contains(term) {
                    score += 12.0;
                    terms_matched_in_filename += 1;
                }
                if lang_l.contains(term) {
                    score += 2.0;
                }
                if preview_l.contains(term) {
                    score += 1.0;
                }
            }

            if terms_matched_in_filename >= 2 || (terms.len() == 1 && terms_matched_in_filename == 1)
            {
                score += 50.0;
            }

            if path_l.ends_with("readme.md")
                || path_l.ends_with("main.rs")
                || path_l.ends_with("main.py")
            {
                score += 1.0;
            }
            if depth <= 1 {
                score += 1.0;
            }
            if hidden_or_vendor {
                score -= 4.0;
            }
        }

        for hint in &path_hints {
            if path_l == *hint {
                score += 100.0;
            } else if path_l.starts_with(hint) || path_l.contains(&format!("/{hint}")) {
                score += 25.0;
            }
        }

        scores.push((idx, score));
    }

    scores
}

fn path_hints(prompt: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in prompt.split_whitespace() {
        let t = token
            .trim_matches(|c: char| {
                c == '`'
                    || c == '"'
                    || c == '\''
                    || c == ','
                    || c == ';'
                    || c == ':'
                    || c == '('
                    || c == ')'
            })
            .trim_start_matches("./")
            .trim_start_matches('/')
            .to_ascii_lowercase();
        if t.contains('/') && t.len() >= 3 {
            out.push(t.trim_end_matches('/').to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

fn query_terms(prompt: &str) -> HashSet<String> {
    prompt
        .to_ascii_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .filter(|t| t.len() >= 3)
        .take(24)
        .map(|s| s.to_string())
        .collect()
}

fn looks_like_project_overview_prompt(prompt: &str) -> bool {
    let p = prompt.to_ascii_lowercase();
    p.contains("what is this project")
        || p.contains("what does this project do")
        || p.contains("what is this repo")
        || p.contains("what does this repo do")
        || p.contains("what is this repository")
        || p.contains("what does this repository do")
        || p.contains("project about")
        || p.contains("de que trata este proyecto")
        || p.contains("de qué trata este proyecto")
        || p.contains("de que trata el proyecto")
        || p.contains("de qué trata el proyecto")
        || p.contains("de que va este proyecto")
        || p.contains("de qué va este proyecto")
        || p.contains("resumen del proyecto")
}

async fn maybe_embedding_scores(
    prompt: &str,
    chunks: &[ProjectChunk],
    config: &RetrievalConfig,
    model_hint: Option<&str>,
) -> (Option<Vec<f32>>, EmbeddingDiagnostics) {
    let mut diag = EmbeddingDiagnostics {
        used: false,
        latency_ms: 0,
        reason: String::new(),
        attempts: 0,
        base_url: config.embed_base_url.clone(),
        model: config.embed_model.clone(),
    };

    if !config.embeddings_enabled || chunks.is_empty() {
        diag.reason = if !config.embeddings_enabled {
            "disabled_by_config".to_string()
        } else {
            "no_chunks".to_string()
        };
        return (None, diag);
    }

    let base_url = config.embed_base_url.as_str();
    let model = resolve_embed_model(base_url, &config.embed_model, model_hint, config).await;
    diag.model = model.clone();

    if model.is_empty() {
        diag.reason = if config.embed_model.trim().is_empty() {
            "optional_not_configured".to_string()
        } else {
            "configured_model_not_available".to_string()
        };
        return (None, diag);
    }

    let started = Instant::now();
    let chunk_inputs: Vec<String> = chunks
        .iter()
        .map(|chunk| {
            let mut text = String::new();
            text.push_str("Path: ");
            text.push_str(&chunk.path);
            text.push('\n');
            text.push_str(&chunk.content.chars().take(1800).collect::<String>());
            text
        })
        .collect();

    let query_vector = match embed_inputs(base_url, &model, vec![prompt.to_string()], config).await
    {
        Ok(v) => {
            diag.attempts = diag.attempts.saturating_add(1);
            if let Some(first) = v.into_iter().next() {
                first
            } else {
                diag.reason = "empty_query_embedding".to_string();
                diag.latency_ms = started.elapsed().as_millis();
                return (None, diag);
            }
        }
        Err(reason) => {
            let normalized = match reason.as_str() {
                "unsupported_endpoint" => "unsupported_endpoint",
                _ => "embed_api_error",
            };
            diag.reason = format!("{} model={}: {}", normalized, model, reason);
            diag.latency_ms = started.elapsed().as_millis();
            return (None, diag);
        }
    };

    let mut chunk_vectors = Vec::new();
    for batch in chunk_inputs.chunks(config.embed_batch_size.max(1)) {
        match embed_inputs(base_url, &model, batch.to_vec(), config).await {
            Ok(vectors) => {
                diag.attempts = diag.attempts.saturating_add(1);
                chunk_vectors.extend(vectors);
            }
            Err(reason) => {
                diag.reason = format!("chunks_failed model={}: {}", model, reason);
                diag.latency_ms = started.elapsed().as_millis();
                return (None, diag);
            }
        }
    }

    diag.used = true;
    diag.reason = "ok".to_string();
    diag.latency_ms = started.elapsed().as_millis();

    (
        Some(
            chunk_vectors
                .iter()
                .map(|v| cosine_similarity(&query_vector, v))
                .collect(),
        ),
        diag,
    )
}

#[derive(serde::Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaTagModel>,
}

#[derive(serde::Deserialize)]
struct OllamaTagModel {
    name: String,
}

#[derive(serde::Deserialize)]
struct OllamaShowResponse {
    capabilities: Option<Vec<String>>,
}

static EMBED_MODEL_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

async fn resolve_embed_model(
    base_url: &str,
    configured: &str,
    model_hint: Option<&str>,
    config: &RetrievalConfig,
) -> String {
    let cache = EMBED_MODEL_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    // 1. If configured and exists and supports embedding, use it.
    if !configured.is_empty() {
        if let Some(caps) = fetch_model_capabilities(base_url, configured, config).await {
            if caps.contains(&"embedding".to_string()) {
                return configured.to_string();
            }
        }
    }

    // Check cache
    {
        let lock = cache.lock().unwrap();
        if let Some(cached) = lock.get(base_url) {
            return cached.clone();
        }
    }

    // 2. Detect
    let detected = detect_best_embed_model(base_url, model_hint, config).await;

    // Save to cache
    {
        let mut lock = cache.lock().unwrap();
        lock.insert(base_url.to_string(), detected.clone());
    }

    detected
}

async fn fetch_model_capabilities(
    base_url: &str,
    model: &str,
    config: &RetrievalConfig,
) -> Option<Vec<String>> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(config.embed_connect_timeout_ms))
        .timeout(Duration::from_millis(config.embed_request_timeout_ms))
        .build()
        .ok()?;

    let url = format!("{}/api/show", base_url.trim_end_matches('/'));
    let body = serde_json::json!({"model": model});
    let resp = client.post(url).json(&body).send().await.ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let show = resp.json::<OllamaShowResponse>().await.ok()?;
    show.capabilities
}

async fn detect_best_embed_model(
    base_url: &str,
    model_hint: Option<&str>,
    config: &RetrievalConfig,
) -> String {
    let models = fetch_ollama_model_details(base_url, config)
        .await
        .unwrap_or_default();
    if models.is_empty() {
        return String::new();
    }

    let capable_models: Vec<String> = models
        .iter()
        .filter(|(_, info)| info.supports_embedding)
        .map(|(name, _)| name.clone())
        .collect();

    if capable_models.is_empty() {
        return String::new();
    }

    // Preference: nomic-embed-text
    if let Some(m) = capable_models
        .iter()
        .find(|m| m.contains("nomic-embed-text"))
    {
        return m.clone();
    }

    // Preference: mxbai-embed-large
    if let Some(m) = capable_models
        .iter()
        .find(|m| m.contains("mxbai-embed-large"))
    {
        return m.clone();
    }

    // Preference: any with "embed"
    if let Some(m) = capable_models
        .iter()
        .find(|m| m.to_lowercase().contains("embed"))
    {
        return m.clone();
    }

    // Fallback: model_hint if capable
    if let Some(hint) = model_hint {
        if capable_models
            .iter()
            .any(|m| m == hint || m.split(':').next() == Some(hint))
        {
            return hint.to_string();
        }
    }

    // Last resort: just the first capable one
    capable_models.first().cloned().unwrap_or_default()
}

struct SimpleModelInfo {
    supports_embedding: bool,
}

async fn fetch_ollama_model_details(
    base_url: &str,
    config: &RetrievalConfig,
) -> Result<Vec<(String, SimpleModelInfo)>, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(config.embed_connect_timeout_ms))
        .timeout(Duration::from_millis(config.embed_request_timeout_ms))
        .build()
        .map_err(|e| e.to_string())?;

    let url_tags = format!("{}/api/tags", base_url.trim_end_matches('/'));
    let resp_tags = client
        .get(url_tags)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp_tags.status().is_success() {
        return Err(format!("http_{}", resp_tags.status()));
    }

    let tags = resp_tags
        .json::<OllamaTagsResponse>()
        .await
        .map_err(|e| e.to_string())?;
    let mut results = Vec::new();

    for model in tags.models {
        let url_show = format!("{}/api/show", base_url.trim_end_matches('/'));
        let body = serde_json::json!({"model": model.name});
        if let Ok(resp_show) = client.post(url_show).json(&body).send().await {
            if resp_show.status().is_success() {
                if let Ok(show) = resp_show.json::<OllamaShowResponse>().await {
                    let supports_embedding = show
                        .capabilities
                        .is_some_and(|c| c.contains(&"embedding".to_string()));
                    results.push((model.name, SimpleModelInfo { supports_embedding }));
                }
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(path: &str) -> ProjectChunk {
        ProjectChunk {
            path: path.to_string(),
            language: "text".to_string(),
            content: "x".to_string(),
        }
    }

    #[test]
    fn lexical_scores_prefers_root_readme_for_project_overview() {
        let chunks = vec![
            ProjectChunk {
                path: "tests/README.md".to_string(),
                language: "markdown".to_string(),
                content: "test docs".to_string(),
            },
            ProjectChunk {
                path: "README.md".to_string(),
                language: "markdown".to_string(),
                content: "project overview".to_string(),
            },
        ];

        let scores = lexical_scores("de que trata este proyecto", &chunks);
        assert!(scores[1].1 > scores[0].1);
    }

    #[test]
    fn detect_project_type_rust() {
        let d = detect_project_type(&[mk("Cargo.toml"), mk("src/main.rs")]);
        assert_eq!(d.project_type, ProjectType::Rust);
    }

    #[test]
    fn detect_project_type_node() {
        let d = detect_project_type(&[mk("package.json"), mk("src/index.js")]);
        assert_eq!(d.project_type, ProjectType::Node);
    }

    #[test]
    fn detect_project_type_python() {
        let d = detect_project_type(&[mk("pyproject.toml"), mk("app.py")]);
        assert_eq!(d.project_type, ProjectType::Python);
    }

    #[test]
    fn detect_project_type_java() {
        let d = detect_project_type(&[mk("src/main/java/com/acme/Application.java")]);
        assert_eq!(d.project_type, ProjectType::Java);
    }

    #[test]
    fn detect_project_type_go() {
        let d = detect_project_type(&[mk("go.mod"), mk("main.go")]);
        assert_eq!(d.project_type, ProjectType::Go);
    }

    #[test]
    fn detect_project_type_web() {
        let d = detect_project_type(&[mk("index.html"), mk("main.js")]);
        assert_eq!(d.project_type, ProjectType::Web);
    }

    #[test]
    fn detect_project_type_csharp() {
        let d = detect_project_type(&[mk("Api.csproj"), mk("Program.cs")]);
        assert_eq!(d.project_type, ProjectType::CSharp);
    }

    #[test]
    fn detect_project_type_php() {
        let d = detect_project_type(&[mk("composer.json"), mk("index.php")]);
        assert_eq!(d.project_type, ProjectType::Php);
    }

    #[test]
    fn detect_project_type_unknown() {
        let d = detect_project_type(&[mk("docs/notes.txt")]);
        assert_eq!(d.project_type, ProjectType::Unknown);
    }
}

#[derive(serde::Serialize)]
struct OllamaEmbedRequest {
    model: String,
    input: Vec<String>,
    truncate: bool,
    keep_alive: String,
}

#[derive(serde::Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

#[derive(serde::Serialize)]
struct OllamaEmbeddingsRequest {
    model: String,
    prompt: String,
    keep_alive: String,
}

#[derive(serde::Deserialize)]
struct OllamaEmbeddingsResponse {
    embedding: Vec<f32>,
}

async fn embed_inputs_legacy_endpoint(
    client: &reqwest::Client,
    base_url: &str,
    model: &str,
    input: &[String],
    config: &RetrievalConfig,
) -> Result<Vec<Vec<f32>>, String> {
    let url = format!("{}/api/embeddings", base_url.trim_end_matches('/'));
    let max_attempts = config.embed_max_retries.saturating_add(1);
    let mut vectors = Vec::with_capacity(input.len());

    for prompt in input {
        let body = OllamaEmbeddingsRequest {
            model: model.to_string(),
            prompt: prompt.clone(),
            keep_alive: "10m".to_string(),
        };

        let mut attempt: u8 = 0;
        loop {
            attempt = attempt.saturating_add(1);
            match client.post(&url).json(&body).send().await {
                Ok(resp) => {
                    if !resp.status().is_success() {
                        if resp.status() == reqwest::StatusCode::NOT_FOUND {
                            return Err("unsupported_endpoint".to_string());
                        }
                        if attempt < max_attempts {
                            tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt)))
                                .await;
                            continue;
                        }
                        return Err(format!("http_status:{}", resp.status()));
                    }

                    match resp.json::<OllamaEmbeddingsResponse>().await {
                        Ok(parsed) => {
                            if parsed.embedding.is_empty() {
                                return Err("empty_embeddings".to_string());
                            }
                            vectors.push(parsed.embedding);
                            break;
                        }
                        Err(err) => {
                            if attempt < max_attempts {
                                tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt)))
                                    .await;
                                continue;
                            }
                            return Err(format!("invalid_json:{}", err));
                        }
                    }
                }
                Err(err) => {
                    if attempt < max_attempts {
                        tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt))).await;
                        continue;
                    }
                    return Err(format!("request_error:{}", err));
                }
            }
        }
    }

    Ok(vectors)
}

async fn embed_inputs(
    base_url: &str,
    model: &str,
    input: Vec<String>,
    config: &RetrievalConfig,
) -> Result<Vec<Vec<f32>>, String> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(config.embed_connect_timeout_ms))
        .timeout(Duration::from_millis(config.embed_request_timeout_ms))
        .build()
        .map_err(|e| format!("client_build:{}", e))?;

    let body = OllamaEmbedRequest {
        model: model.to_string(),
        input: input.clone(),
        truncate: true,
        keep_alive: "10m".to_string(),
    };
    let url = format!("{}/api/embed", base_url.trim_end_matches('/'));
    let mut attempt: u8 = 0;
    let max_attempts = config.embed_max_retries.saturating_add(1);

    while attempt < max_attempts {
        attempt = attempt.saturating_add(1);
        match client.post(&url).json(&body).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    if resp.status() == reqwest::StatusCode::NOT_FOUND {
                        return embed_inputs_legacy_endpoint(
                            &client, base_url, model, &input, config,
                        )
                        .await;
                    }
                    if attempt < max_attempts {
                        tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt))).await;
                        continue;
                    }
                    return Err(format!("http_status:{}", resp.status()));
                }
                match resp.json::<OllamaEmbedResponse>().await {
                    Ok(parsed) => {
                        if parsed.embeddings.is_empty() {
                            return Err("empty_embeddings".to_string());
                        }
                        return Ok(parsed.embeddings);
                    }
                    Err(err) => {
                        if attempt < max_attempts {
                            tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt)))
                                .await;
                            continue;
                        }
                        return Err(format!("invalid_json:{}", err));
                    }
                }
            }
            Err(err) => {
                if attempt < max_attempts {
                    tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt))).await;
                    continue;
                }
                return Err(format!("request_error:{}", err));
            }
        }
    }

    Err("unknown_error".to_string())
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }

    let mut dot = 0f32;
    let mut na = 0f32;
    let mut nb = 0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na <= f32::EPSILON || nb <= f32::EPSILON {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

fn estimate_tokens_for_model(s: &str, _model: Option<&str>) -> usize {
    if let Some(tokenizer) = cl100k_tokenizer() {
        return tokenizer.encode_with_special_tokens(s).len();
    }
    let graphemes = s.chars().count();
    let words = s.split_whitespace().count();
    graphemes / 4 + words / 2
}

fn truncate_to_token_budget(text: &str, max_tokens: usize, model: Option<&str>) -> String {
    if estimate_tokens_for_model(text, model) <= max_tokens {
        return text.to_string();
    }

    let mut boundaries: Vec<usize> = text.char_indices().map(|(idx, _)| idx).collect();
    boundaries.push(text.len());
    let mut left = 0usize;
    let mut right = boundaries.len().saturating_sub(1);
    let mut best = 0usize;

    while left <= right {
        let mid = left + (right - left) / 2;
        let end = boundaries[mid];
        let tokens = estimate_tokens_for_model(&text[..end], model);
        if tokens <= max_tokens {
            best = end;
            left = mid.saturating_add(1);
        } else if mid == 0 {
            break;
        } else {
            right = mid - 1;
        }
    }

    text[..best].to_string()
}

fn cl100k_tokenizer() -> Option<&'static tiktoken_rs::CoreBPE> {
    static TOKENIZER: OnceLock<Option<tiktoken_rs::CoreBPE>> = OnceLock::new();
    TOKENIZER
        .get_or_init(|| tiktoken_rs::cl100k_base().ok())
        .as_ref()
}
