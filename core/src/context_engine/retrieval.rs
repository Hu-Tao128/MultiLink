use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::context_engine::chunker::SemanticChunk;

#[derive(Debug, Clone)]
pub struct RankedChunk {
    pub chunk: SemanticChunk,
    pub score: f32,
    pub lexical_score: f32,
    pub embedding_score: f32,
}

#[derive(Debug, Clone)]
pub struct RetrievalResult {
    pub context: String,
    pub selected_files: Vec<String>,
    pub used_tokens: usize,
    pub top_k: usize,
    pub embedding_used: bool,
    pub embedding_reason: String,
    pub embedding_latency_ms: u128,
    pub embedding_attempts: u8,
    pub embed_base_url: String,
    pub embed_model: String,
    pub is_truncated: bool,
    pub budget_used: usize,
}

pub async fn hybrid_retrieval(
    prompt: &str,
    chunks: &[SemanticChunk],
    token_budget: usize,
    model_hint: Option<&str>,
    embed_base_url: &str,
    embed_model: &str,
    embed_enabled: bool,
    embed_connect_timeout_ms: u64,
    embed_request_timeout_ms: u64,
    embed_max_retries: u8,
    embed_batch_size: usize,
    index_dir: Option<&PathBuf>,
) -> RetrievalResult {
    if chunks.is_empty() {
        return empty_result(token_budget);
    }

    let lexical = lexical_scores(prompt, chunks);
    let embed_result = if embed_enabled {
        if let Some(dir) = index_dir {
            super::embeddings::blend_embedding_scores(
                prompt,
                chunks,
                embed_base_url,
                embed_model,
                embed_connect_timeout_ms,
                embed_request_timeout_ms,
                embed_max_retries,
                embed_batch_size,
                dir,
            )
            .await
        } else {
            super::embeddings::EmbeddingBlendResult {
                scores: HashMap::new(),
                used_embeddings: false,
                reason: "index_dir_unavailable".to_string(),
                latency_ms: 0,
                attempts: 0,
                base_url: embed_base_url.to_string(),
                model: embed_model.to_string(),
            }
        }
    } else {
        super::embeddings::EmbeddingBlendResult {
            scores: HashMap::new(),
            used_embeddings: false,
            reason: "disabled_by_config".to_string(),
            latency_ms: 0,
            attempts: 0,
            base_url: embed_base_url.to_string(),
            model: embed_model.to_string(),
        }
    };

    let embed_used = embed_result.used_embeddings;
    let mut ranked: Vec<RankedChunk> = chunks
        .iter()
        .enumerate()
        .map(|(idx, chunk)| {
            let lex = lexical.get(idx).copied().unwrap_or(0.0);
            let emb = embed_result
                .scores
                .get(&chunk.chunk_hash)
                .copied()
                .unwrap_or(0.0);
            let final_score = if embed_used {
                lex * 0.35 + emb * 0.65
            } else {
                lex
            };
            RankedChunk {
                chunk: chunk.clone(),
                score: final_score,
                lexical_score: lex,
                embedding_score: emb,
            }
        })
        .collect();

    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let top_k = (token_budget / 150).max(2).min(24);
    let ranked_limited: Vec<RankedChunk> = ranked.into_iter().take(top_k).collect();

    let compression_result =
        super::compress::fit_to_budget(ranked_limited, token_budget, model_hint);

    let context = render_result(&compression_result.selected, model_hint);
    let used = estimate_tokens_for_model(&context, model_hint);

    let selected_files: Vec<String> = compression_result
        .selected
        .iter()
        .map(|c| c.chunk.file.clone())
        .collect();

    RetrievalResult {
        context,
        selected_files: dedupe_preserve_order(selected_files),
        used_tokens: used,
        top_k,
        embedding_used: embed_used,
        embedding_reason: embed_result.reason,
        embedding_latency_ms: embed_result.latency_ms,
        embedding_attempts: embed_result.attempts,
        embed_base_url: embed_result.base_url,
        embed_model: embed_result.model,
        is_truncated: compression_result.is_truncated,
        budget_used: compression_result.budget_used,
    }
}

fn lexical_scores(prompt: &str, chunks: &[SemanticChunk]) -> Vec<f32> {
    let terms = query_terms(prompt);
    let path_hints = path_hints(prompt);
    let is_project_overview_prompt = looks_like_project_overview_prompt(prompt);
    let mut scores = Vec::with_capacity(chunks.len());

    for chunk in chunks {
        let mut score = 0.0f32;
        let path_l = chunk.file.to_ascii_lowercase();
        let lang_l = chunk.language.to_ascii_lowercase();
        let preview_l = chunk.content.to_ascii_lowercase();
        let depth = chunk.file.matches('/').count();
        let file_name = chunk
            .file
            .rsplit('/')
            .next()
            .unwrap_or(chunk.file.as_str())
            .to_ascii_lowercase();
        let ext = chunk
            .file
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
        let is_root_file = depth == 0;
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
            } else if file_name == "readme.md" {
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
            if depth <= 1 {
                score += 3.0;
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
            if file_name == "main.rs"
                || file_name == "main.py"
                || file_name == "lib.rs"
                || file_name == "app.py"
            {
                score += 3.0;
            }
            if chunk.symbol != "block_1" && !chunk.symbol.starts_with("block_") {
                score += 2.0;
            }
        } else {
            let symbol_lower = chunk.symbol.to_lowercase();
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
                if symbol_lower.contains(term) {
                    score += 3.0;
                }
            }
        }

        for hint in &path_hints {
            if path_l.starts_with(hint) || path_l.contains(&format!("/{hint}")) {
                score += 12.0;
            }
        }

        scores.push(score);
    }

    scores
}

fn path_hints(prompt: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in prompt.split_whitespace() {
        let t = token
            .trim_matches(|c: char| {
                c == '`' || c == '"' || c == '\'' || c == ',' || c == ';' || c == ':' || c == '(' || c == ')'
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

fn query_terms(prompt: &str) -> Vec<String> {
    prompt
        .to_ascii_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .filter(|t| t.len() >= 3)
        .take(24)
        .map(|s| s.to_string())
        .collect()
}

fn dedupe_preserve_order(paths: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for p in paths {
        if seen.insert(p.clone()) {
            out.push(p);
        }
    }
    out
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

fn render_result(chunks: &[RankedChunk], model_hint: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str("Project Context:\n");
    for rc in chunks {
        let content = &rc.chunk.content;
        let truncated = truncate_to_token_budget(content, 1800, model_hint);
        out.push_str(&format!(
            "- file: {}\n  start_line: {}\n  symbol: {}\n  excerpt: |\n    {}\n",
            rc.chunk.file,
            rc.chunk.start_line,
            rc.chunk.symbol,
            truncated.replace('\n', "\n    ")
        ));
    }
    out
}

fn empty_result(_token_budget: usize) -> RetrievalResult {
    RetrievalResult {
        context: String::new(),
        selected_files: Vec::new(),
        used_tokens: 0,
        top_k: 0,
        embedding_used: false,
        embedding_reason: "no_chunks".to_string(),
        embedding_latency_ms: 0,
        embedding_attempts: 0,
        embed_base_url: String::new(),
        embed_model: String::new(),
        is_truncated: false,
        budget_used: 0,
    }
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
    use std::sync::OnceLock;
    static TOKENIZER: OnceLock<Option<tiktoken_rs::CoreBPE>> = OnceLock::new();
    TOKENIZER
        .get_or_init(|| tiktoken_rs::cl100k_base().ok())
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexical_scores_favors_symbol_matches() {
        let chunks = vec![super::super::chunker::SemanticChunk {
            file: "src/main.rs".to_string(),
            language: "rust".to_string(),
            start_line: 1,
            symbol: "main".to_string(),
            content: "fn main() {}".to_string(),
            chunk_hash: "abc".to_string(),
        }];
        let scores = lexical_scores("where is main function", &chunks);
        assert!(scores[0] > 0.0);
    }

    #[test]
    fn lexical_scores_prefers_root_readme_for_project_overview() {
        let chunks = vec![
            super::super::chunker::SemanticChunk {
                file: "tests/README.md".to_string(),
                language: "markdown".to_string(),
                start_line: 1,
                symbol: "block_1".to_string(),
                content: "test docs".to_string(),
                chunk_hash: "a".to_string(),
            },
            super::super::chunker::SemanticChunk {
                file: "README.md".to_string(),
                language: "markdown".to_string(),
                start_line: 1,
                symbol: "block_1".to_string(),
                content: "project overview".to_string(),
                chunk_hash: "b".to_string(),
            },
        ];
        let scores = lexical_scores("de que trata este proyecto", &chunks);
        assert!(scores[1] > scores[0]);
    }
}
