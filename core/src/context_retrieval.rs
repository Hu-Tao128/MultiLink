use std::cmp::Ordering;
use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::Instant;

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

#[derive(Clone)]
struct ProjectChunk {
    path: String,
    language: String,
    content: String,
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
    let (embedding, embedding_diag) = maybe_embedding_scores(prompt, &chunks, &config).await;
    let embedding_used = embedding_diag.used;
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
            for term in &terms {
                if path_l.contains(term) {
                    score += 6.0;
                }
                if lang_l.contains(term) {
                    score += 2.0;
                }
                if preview_l.contains(term) {
                    score += 1.0;
                }
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

        scores.push((idx, score));
    }

    scores
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

    let model = config.embed_model.as_str();
    let base_url = config.embed_base_url.as_str();
    let started = Instant::now();

    let chunk_inputs: Vec<String> = chunks
        .iter()
        .map(|chunk| {
            let mut text = String::new();
            text.push_str("Path: ");
            text.push_str(&chunk.path);
            text.push_str("\n");
            text.push_str(&chunk.content.chars().take(1800).collect::<String>());
            text
        })
        .collect();

    let query_vector = match embed_inputs(base_url, model, vec![prompt.to_string()], config).await {
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
            diag.reason = format!("query_failed:{}", reason);
            diag.latency_ms = started.elapsed().as_millis();
            return (None, diag);
        }
    };

    let mut chunk_vectors = Vec::new();
    for batch in chunk_inputs.chunks(config.embed_batch_size.max(1)) {
        match embed_inputs(base_url, model, batch.to_vec(), config).await {
            Ok(vectors) => {
                diag.attempts = diag.attempts.saturating_add(1);
                chunk_vectors.extend(vectors);
            }
            Err(reason) => {
                diag.reason = format!("chunks_failed:{}", reason);
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

#[cfg(test)]
mod tests {
    use super::*;

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
        input,
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
