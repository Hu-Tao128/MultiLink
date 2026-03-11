use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tokio::sync::RwLock;

use crate::context_engine::chunker::{semantic_chunks, SemanticChunk, SourceFile};

#[derive(Debug, Clone)]
pub struct IndexedProject {
    pub project_hash: String,
    pub source_fingerprint: String,
    pub chunks: Vec<SemanticChunk>,
    pub index_dir: PathBuf,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PersistedIndex {
    source_fingerprint: String,
    project_hash: String,
    chunks: Vec<PersistedChunk>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PersistedChunk {
    file: String,
    language: String,
    start_line: usize,
    symbol: String,
    content: String,
    chunk_hash: String,
}

#[derive(Debug)]
struct ParsedProject {
    root: Option<PathBuf>,
    files: Vec<SourceFile>,
}

#[cfg(test)]
static DISK_SCAN_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub async fn load_or_build(raw_context: &str) -> Option<IndexedProject> {
    if raw_context.trim().is_empty() {
        return None;
    }

    let source_fingerprint = short_hash(raw_context);
    if let Some(cached) = in_memory_cache().read().await.get(&source_fingerprint) {
        return Some(cached.clone());
    }

    let parsed = parse_project(raw_context);
    if parsed.files.is_empty() {
        return None;
    }

    let project_hash = project_hash(&parsed);
    let index_dir = index_root_dir().join(&project_hash);
    let manifest = index_dir.join("manifest.json");

    if let Ok(existing) = fs::read_to_string(&manifest) {
        if let Ok(parsed_manifest) = serde_json::from_str::<PersistedIndex>(&existing) {
            if parsed_manifest.source_fingerprint == source_fingerprint {
                let out = IndexedProject {
                    project_hash,
                    source_fingerprint: source_fingerprint.clone(),
                    chunks: parsed_manifest
                        .chunks
                        .into_iter()
                        .map(|c| SemanticChunk {
                            file: c.file,
                            language: c.language,
                            start_line: c.start_line,
                            symbol: c.symbol,
                            content: c.content,
                            chunk_hash: c.chunk_hash,
                        })
                        .collect(),
                    index_dir,
                };
                in_memory_cache()
                    .write()
                    .await
                    .insert(source_fingerprint, out.clone());
                return Some(out);
            }
        }
    }

    let _file_hashes = file_signatures(&parsed);
    let chunks = semantic_chunks(&parsed.files, 32);
    if chunks.is_empty() {
        return None;
    }

    let persisted = PersistedIndex {
        source_fingerprint: source_fingerprint.clone(),
        project_hash: project_hash.clone(),
        chunks: chunks
            .iter()
            .map(|chunk| PersistedChunk {
                file: chunk.file.clone(),
                language: chunk.language.clone(),
                start_line: chunk.start_line,
                symbol: chunk.symbol.clone(),
                content: chunk.content.clone(),
                chunk_hash: chunk.chunk_hash.clone(),
            })
            .collect(),
    };

    if fs::create_dir_all(&index_dir).is_ok() {
        let _ = fs::write(
            &manifest,
            serde_json::to_vec_pretty(&persisted).unwrap_or_default(),
        );
    }

    let out = IndexedProject {
        project_hash,
        source_fingerprint: source_fingerprint.clone(),
        chunks,
        index_dir,
    };
    in_memory_cache()
        .write()
        .await
        .insert(source_fingerprint, out.clone());
    Some(out)
}

fn parse_project(raw_context: &str) -> ParsedProject {
    let root = parse_project_root(raw_context);
    let files = parse_source_files(raw_context);
    ParsedProject { root, files }
}

fn parse_project_root(raw_context: &str) -> Option<PathBuf> {
    let line = raw_context
        .lines()
        .find(|line| line.starts_with("Project root: "))?;
    let root = line.trim_start_matches("Project root: ").trim();
    if root.is_empty() {
        return None;
    }
    Some(PathBuf::from(root))
}

fn parse_source_files(raw_context: &str) -> Vec<SourceFile> {
    let mut files = Vec::new();
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

        files.push(SourceFile {
            path,
            language,
            content,
        });
        cursor = (content_end + "\n```".len()).min(raw_context.len());
    }

    files
}

fn project_hash(project: &ParsedProject) -> String {
    let mut hasher = DefaultHasher::new();
    if let Some(root) = project.root.as_ref() {
        root.to_string_lossy().hash(&mut hasher);
    }
    for file in &project.files {
        file.path.hash(&mut hasher);
        file.language.hash(&mut hasher);
        file.content.len().hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn file_signatures(project: &ParsedProject) -> Vec<String> {
    #[cfg(test)]
    {
        DISK_SCAN_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    let Some(root) = project.root.as_ref() else {
        return project
            .files
            .iter()
            .map(|f| short_hash(&f.path))
            .collect::<Vec<_>>();
    };

    project
        .files
        .iter()
        .map(|file| {
            let full = root.join(&file.path);
            signature_for_file(&full, &file.path)
        })
        .collect()
}

fn signature_for_file(path: &Path, rel_path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    rel_path.hash(&mut hasher);
    if let Ok(meta) = fs::metadata(path) {
        if let Ok(modified) = meta.modified() {
            if let Ok(since_epoch) = modified.duration_since(std::time::UNIX_EPOCH) {
                since_epoch.as_nanos().hash(&mut hasher);
            }
        }
    }
    format!("{:016x}", hasher.finish())
}

fn index_root_dir() -> PathBuf {
    if let Some(base) = dirs::data_local_dir() {
        return base.join("multilink").join("index");
    }
    PathBuf::from(".multilink/index")
}

fn short_hash(input: &str) -> String {
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn in_memory_cache() -> &'static RwLock<HashMap<String, IndexedProject>> {
    static CACHE: OnceLock<RwLock<HashMap<String, IndexedProject>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

#[cfg(test)]
pub(crate) fn debug_disk_scan_count() -> usize {
    DISK_SCAN_COUNT.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
pub(crate) fn debug_reset_disk_scan_count() {
    DISK_SCAN_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::{debug_disk_scan_count, debug_reset_disk_scan_count, load_or_build};

    #[tokio::test]
    async fn second_request_uses_memory_index_without_new_disk_scan() {
        debug_reset_disk_scan_count();
        let temp = tempfile::tempdir().expect("temp");
        let root = temp.path();
        std::fs::write(root.join("main.rs"), "fn main() {}\n").expect("write file");

        let raw = format!(
            "Project root: {}\n\nFiles:\n\nFile: main.rs\n```rust\nfn main() {{}}\n\n```\n",
            root.display()
        );

        let _ = load_or_build(&raw).await;
        let first = debug_disk_scan_count();
        let _ = load_or_build(&raw).await;
        let second = debug_disk_scan_count();
        assert!(first >= 1);
        assert_eq!(first, second);
    }
}
