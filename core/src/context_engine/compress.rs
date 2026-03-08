use crate::context_engine::retrieval::RankedChunk;

pub struct CompressionResult {
    pub selected: Vec<RankedChunk>,
    pub is_truncated: bool,
    pub budget_used: usize,
}

pub fn fit_to_budget(
    ranked: Vec<RankedChunk>,
    token_budget: usize,
    model_hint: Option<&str>,
) -> CompressionResult {
    let mut selected = Vec::new();
    let mut used = 0usize;
    let mut truncated = false;

    for chunk in ranked {
        let rendered = render_chunk(&chunk);
        let tokens = estimate_tokens_for_model(&rendered, model_hint);
        if used + tokens <= token_budget {
            used += tokens;
            selected.push(chunk);
            continue;
        }

        truncated = true;
        if used >= token_budget {
            continue;
        }

        let remain = token_budget.saturating_sub(used);
        if remain < 48 {
            continue;
        }

        let mut compressed = chunk.clone();
        compressed.chunk.content = summarize_chunk(&compressed.chunk.content, remain, model_hint);
        if compressed.chunk.content.trim().is_empty() {
            continue;
        }
        let compressed_tokens = estimate_tokens_for_model(&render_chunk(&compressed), model_hint);
        if used + compressed_tokens <= token_budget {
            used += compressed_tokens;
            selected.push(compressed);
        }
    }

    CompressionResult {
        selected,
        is_truncated: truncated,
        budget_used: used,
    }
}

fn render_chunk(chunk: &RankedChunk) -> String {
    format!(
        "- file: {}\n  start_line: {}\n  symbol: {}\n  score: {:.3}\n  excerpt: |\n    {}\n",
        chunk.chunk.file,
        chunk.chunk.start_line,
        chunk.chunk.symbol,
        chunk.score,
        chunk.chunk.content.replace('\n', "\n    ")
    )
}

fn summarize_chunk(input: &str, budget_tokens: usize, model_hint: Option<&str>) -> String {
    let lines: Vec<&str> = input.lines().collect();
    if lines.is_empty() {
        return String::new();
    }

    let mut out = Vec::new();
    for line in &lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("fn ")
            || trimmed.starts_with("def ")
            || trimmed.starts_with("class ")
            || trimmed.starts_with("impl ")
            || trimmed.starts_with("pub ")
            || trimmed.starts_with("export ")
            || trimmed.contains("TODO")
        {
            out.push(trimmed.to_string());
        }
        if out.len() >= 14 {
            break;
        }
    }

    if out.is_empty() {
        out = lines
            .iter()
            .take(16)
            .map(|l| l.trim().to_string())
            .collect();
    }

    let text = out.join("\n");
    truncate_to_token_budget(&text, budget_tokens, model_hint)
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
