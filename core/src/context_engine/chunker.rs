use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: String,
    pub language: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct SemanticChunk {
    pub file: String,
    pub language: String,
    pub start_line: usize,
    pub symbol: String,
    pub content: String,
    pub chunk_hash: String,
}

pub fn semantic_chunks(files: &[SourceFile], fallback_block_lines: usize) -> Vec<SemanticChunk> {
    let mut out = Vec::new();
    for file in files {
        let chunks = chunk_single_file(file, fallback_block_lines.max(24));
        if chunks.is_empty() {
            out.extend(fallback_chunks(file, fallback_block_lines.max(24)));
        } else {
            out.extend(chunks);
        }
    }
    out
}

fn chunk_single_file(file: &SourceFile, fallback_block_lines: usize) -> Vec<SemanticChunk> {
    let path_l = file.path.to_ascii_lowercase();
    let mut markers = Vec::new();

    for (idx, line) in file.content.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim_start();
        if is_rust_file(&path_l) {
            if trimmed.starts_with("fn ")
                || trimmed.starts_with("impl ")
                || trimmed.starts_with("mod ")
            {
                markers.push((line_no, extract_symbol(trimmed, &path_l)));
            }
        } else if is_python_file(&path_l) {
            if trimmed.starts_with("def ") || trimmed.starts_with("class ") {
                markers.push((line_no, extract_symbol(trimmed, &path_l)));
            }
        } else if is_js_like_file(&path_l) {
            if trimmed.starts_with("function ")
                || trimmed.starts_with("class ")
                || trimmed.contains("=>")
                || trimmed.starts_with("export function ")
            {
                markers.push((line_no, extract_symbol(trimmed, &path_l)));
            }
        }
    }

    if markers.is_empty() {
        return fallback_chunks(file, fallback_block_lines);
    }

    let all_lines: Vec<&str> = file.content.lines().collect();
    let mut out = Vec::new();
    for i in 0..markers.len() {
        let (start_line, symbol) = &markers[i];
        let end_line = if i + 1 < markers.len() {
            markers[i + 1].0.saturating_sub(1)
        } else {
            all_lines.len()
        };
        if *start_line == 0 || end_line < *start_line {
            continue;
        }
        let content = all_lines
            .iter()
            .skip(start_line.saturating_sub(1))
            .take(end_line.saturating_sub(*start_line).saturating_add(1))
            .copied()
            .collect::<Vec<_>>()
            .join("\n");
        if content.trim().is_empty() {
            continue;
        }
        out.push(build_chunk(file, *start_line, symbol.clone(), content));
    }

    out
}

fn fallback_chunks(file: &SourceFile, block_lines: usize) -> Vec<SemanticChunk> {
    let lines: Vec<&str> = file.content.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut start = 0usize;
    while start < lines.len() {
        let end = (start + block_lines).min(lines.len());
        let content = lines[start..end].join("\n");
        if !content.trim().is_empty() {
            let start_line = start + 1;
            out.push(build_chunk(
                file,
                start_line,
                format!("block_{start_line}"),
                content,
            ));
        }
        start = end;
    }

    out
}

fn build_chunk(
    file: &SourceFile,
    start_line: usize,
    symbol: String,
    content: String,
) -> SemanticChunk {
    let mut hasher = DefaultHasher::new();
    file.path.hash(&mut hasher);
    start_line.hash(&mut hasher);
    symbol.hash(&mut hasher);
    content.hash(&mut hasher);

    SemanticChunk {
        file: file.path.clone(),
        language: if file.language.is_empty() {
            "text".to_string()
        } else {
            file.language.clone()
        },
        start_line,
        symbol,
        content,
        chunk_hash: format!("{:016x}", hasher.finish()),
    }
}

fn extract_symbol(line: &str, path_l: &str) -> String {
    let mut candidates = Vec::new();
    if line.starts_with("fn ") {
        candidates.push(line.trim_start_matches("fn "));
    }
    if line.starts_with("impl ") {
        candidates.push(line.trim_start_matches("impl "));
    }
    if line.starts_with("mod ") {
        candidates.push(line.trim_start_matches("mod "));
    }
    if line.starts_with("def ") {
        candidates.push(line.trim_start_matches("def "));
    }
    if line.starts_with("class ") {
        candidates.push(line.trim_start_matches("class "));
    }
    if line.starts_with("function ") {
        candidates.push(line.trim_start_matches("function "));
    }
    if line.starts_with("export function ") {
        candidates.push(line.trim_start_matches("export function "));
    }

    if let Some(raw) = candidates.first() {
        let symbol = raw
            .split(|c: char| c == '(' || c == '{' || c == ':' || c.is_whitespace())
            .next()
            .unwrap_or("symbol")
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
            .to_string();
        if !symbol.is_empty() {
            return symbol;
        }
    }

    if is_python_file(path_l) {
        return "python_symbol".to_string();
    }
    if is_js_like_file(path_l) {
        return "js_symbol".to_string();
    }
    if is_rust_file(path_l) {
        return "rust_symbol".to_string();
    }
    "block".to_string()
}

fn is_rust_file(path: &str) -> bool {
    path.ends_with(".rs")
}

fn is_python_file(path: &str) -> bool {
    path.ends_with(".py")
}

fn is_js_like_file(path: &str) -> bool {
    path.ends_with(".js")
        || path.ends_with(".ts")
        || path.ends_with(".tsx")
        || path.ends_with(".jsx")
}

#[cfg(test)]
mod tests {
    use super::{semantic_chunks, SourceFile};

    #[test]
    fn chunks_rust_by_symbols() {
        let files = vec![SourceFile {
            path: "src/lib.rs".to_string(),
            language: "rust".to_string(),
            content: "mod api;\nfn alpha() {}\nimpl Router {}\n".to_string(),
        }];
        let chunks = semantic_chunks(&files, 32);
        assert!(chunks.len() >= 2);
        assert_eq!(chunks[0].file, "src/lib.rs");
        assert!(chunks[0].start_line >= 1);
        assert!(!chunks[0].symbol.is_empty());
    }

    #[test]
    fn fallback_chunks_include_metadata() {
        let files = vec![SourceFile {
            path: "README.md".to_string(),
            language: "markdown".to_string(),
            content: "a\nb\nc\nd\ne\n".to_string(),
        }];
        let chunks = semantic_chunks(&files, 2);
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].start_line, 1);
        assert!(chunks[0].symbol.starts_with("block_"));
    }
}
