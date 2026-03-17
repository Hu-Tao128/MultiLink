use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tokio::sync::RwLock;

use crate::context_engine::chunker::{semantic_chunks, SemanticChunk, SourceFile};
use crate::context_engine::parser::chunk_extractor::{extract_semantic_chunks, CodeChunk};

#[derive(Debug, Clone)]
pub struct IndexedProject {
    pub project_hash: String,
    pub source_fingerprint: String,
    pub chunks: Vec<SemanticChunk>,
    pub index_dir: PathBuf,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PersistedIndex {
    #[serde(default)]
    source_fingerprint: String,
    #[serde(default)]
    project_hash: String,
    #[serde(default)]
    files: Vec<PersistedFileState>,
    #[serde(default)]
    chunks: Vec<PersistedChunk>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct PersistedFileState {
    path: String,
    content_hash: String,
    chunk_hashes: Vec<String>,
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

    let previous_index = if let Ok(existing) = fs::read_to_string(&manifest) {
        serde_json::from_str::<PersistedIndex>(&existing).ok()
    } else {
        None
    };

    if let Some(parsed_manifest) = previous_index.as_ref() {
        if parsed_manifest.source_fingerprint == source_fingerprint {
            let out = IndexedProject {
                project_hash,
                source_fingerprint: source_fingerprint.clone(),
                chunks: parsed_manifest
                    .chunks
                    .iter()
                    .cloned()
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

    let _file_hashes = file_signatures(&parsed);
    let chunks = build_incremental_chunks(&parsed.files, previous_index.as_ref());
    if chunks.is_empty() {
        return None;
    }

    let files = build_file_states(&parsed.files, &chunks);

    let persisted = PersistedIndex {
        source_fingerprint: source_fingerprint.clone(),
        project_hash: project_hash.clone(),
        files,
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

pub async fn load_best_effort(raw_context: &str) -> Option<IndexedProject> {
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
    let persisted = fs::read_to_string(&manifest)
        .ok()
        .and_then(|raw| serde_json::from_str::<PersistedIndex>(&raw).ok())?;

    if persisted.source_fingerprint == source_fingerprint {
        let exact = indexed_from_persisted(
            project_hash,
            source_fingerprint.clone(),
            index_dir,
            persisted,
        );
        in_memory_cache()
            .write()
            .await
            .insert(source_fingerprint, exact.clone());
        return Some(exact);
    }

    // Fallback to the latest consistent index for this project while a refresh may be in-flight.
    Some(indexed_from_persisted(
        project_hash,
        persisted.source_fingerprint.clone(),
        index_dir,
        persisted,
    ))
}

pub fn refresh_in_background(raw_context: String) {
    if raw_context.trim().is_empty() {
        return;
    }
    let refresh_key = refresh_key_for_raw_context(&raw_context);
    tokio::spawn(async move {
        let can_run = {
            let mut guard = refresh_inflight().write().await;
            if guard.contains(&refresh_key) {
                false
            } else {
                guard.insert(refresh_key.clone());
                true
            }
        };

        if !can_run {
            return;
        }

        let _ = load_or_build(&raw_context).await;

        let mut guard = refresh_inflight().write().await;
        guard.remove(&refresh_key);
    });
}

fn parse_project(raw_context: &str) -> ParsedProject {
    let root = parse_project_root(raw_context);
    let files = parse_source_files(raw_context);
    ParsedProject { root, files }
}

fn indexed_from_persisted(
    project_hash: String,
    source_fingerprint: String,
    index_dir: PathBuf,
    persisted: PersistedIndex,
) -> IndexedProject {
    IndexedProject {
        project_hash,
        source_fingerprint,
        chunks: persisted
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
    }
}

fn refresh_key_for_raw_context(raw_context: &str) -> String {
    if let Some(root) = parse_project_root(raw_context) {
        return format!("root:{}", root.display());
    }
    format!("raw:{}", short_hash(raw_context))
}

fn build_incremental_chunks(
    files: &[SourceFile],
    previous: Option<&PersistedIndex>,
) -> Vec<SemanticChunk> {
    let mut out = Vec::new();

    let (prev_files, prev_chunks) = if let Some(prev) = previous {
        let file_map = prev
            .files
            .iter()
            .map(|state| (state.path.clone(), state.clone()))
            .collect::<HashMap<_, _>>();
        let chunk_map = prev
            .chunks
            .iter()
            .map(|chunk| {
                (
                    chunk.chunk_hash.clone(),
                    SemanticChunk {
                        file: chunk.file.clone(),
                        language: chunk.language.clone(),
                        start_line: chunk.start_line,
                        symbol: chunk.symbol.clone(),
                        content: chunk.content.clone(),
                        chunk_hash: chunk.chunk_hash.clone(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        (Some(file_map), Some(chunk_map))
    } else {
        (None, None)
    };

    for file in files {
        let content_hash = short_hash(&file.content);
        let reused = prev_files
            .as_ref()
            .and_then(|map| map.get(&file.path))
            .filter(|state| state.content_hash == content_hash)
            .and_then(|state| {
                let mut rebuilt = Vec::with_capacity(state.chunk_hashes.len());
                let chunk_map = prev_chunks.as_ref()?;
                for hash in &state.chunk_hashes {
                    let chunk = chunk_map.get(hash)?;
                    rebuilt.push(chunk.clone());
                }
                Some(rebuilt)
            });

        if let Some(chunks) = reused {
            out.extend(chunks);
        } else {
            out.extend(extract_chunks_from_file(file));
        }
    }

    out
}

fn extract_chunks_from_file(file: &SourceFile) -> Vec<SemanticChunk> {
    let code_chunks = extract_semantic_chunks(&file.content, &file.path);

    if code_chunks.is_empty() {
        return semantic_chunks(std::slice::from_ref(file), 32);
    }

    code_chunks
        .into_iter()
        .map(convert_code_chunk_to_semantic)
        .collect()
}

fn convert_code_chunk_to_semantic(chunk: CodeChunk) -> SemanticChunk {
    let mut hasher = DefaultHasher::new();
    chunk.file.hash(&mut hasher);
    chunk.start_line.hash(&mut hasher);
    if let Some(ref sym) = chunk.symbol {
        sym.hash(&mut hasher);
    }
    chunk.text.hash(&mut hasher);

    SemanticChunk {
        file: chunk.file,
        language: chunk.language.to_string(),
        start_line: chunk.start_line,
        symbol: chunk.symbol.unwrap_or_else(|| "unnamed".to_string()),
        content: chunk.text,
        chunk_hash: format!("{:016x}", hasher.finish()),
    }
}

fn build_file_states(files: &[SourceFile], chunks: &[SemanticChunk]) -> Vec<PersistedFileState> {
    let mut chunk_hashes_by_file: HashMap<&str, Vec<String>> = HashMap::new();
    for chunk in chunks {
        chunk_hashes_by_file
            .entry(chunk.file.as_str())
            .or_default()
            .push(chunk.chunk_hash.clone());
    }

    files
        .iter()
        .map(|file| PersistedFileState {
            path: file.path.clone(),
            content_hash: short_hash(&file.content),
            chunk_hashes: chunk_hashes_by_file
                .get(file.path.as_str())
                .cloned()
                .unwrap_or_default(),
        })
        .collect()
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

fn refresh_inflight() -> &'static RwLock<HashSet<String>> {
    static REFRESH_INFLIGHT: OnceLock<RwLock<HashSet<String>>> = OnceLock::new();
    REFRESH_INFLIGHT.get_or_init(|| RwLock::new(HashSet::new()))
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

    #[tokio::test]
    async fn benchmark_indexing_100_chunks() {
        use std::time::Instant;

        let mut files_content = String::new();
        for i in 0..100 {
            files_content.push_str(&format!(
                "File: module_{}.rs\n```rust\npub fn function_{}() -> i32 {{\n    let x = {};\n    x + 1\n}}\n\n```\n",
                i, i, i
            ));
        }

        let raw = format!("Project root: /tmp/test\n\nFiles:\n\n{}", files_content);

        let start = Instant::now();
        let result = load_or_build(&raw).await;
        let duration = start.elapsed();

        assert!(result.is_some(), "Index should be built");
        let chunks = result.unwrap().chunks;
        assert!(!chunks.is_empty(), "Should have extracted chunks");

        println!("Indexed 100 files in {}ms", duration.as_millis());
        assert!(
            duration.as_millis() < 500,
            "Indexing should complete in < 500ms"
        );
    }

    #[tokio::test]
    async fn benchmark_indexing_1000_chunks() {
        use std::time::Instant;

        let mut files_content = String::new();
        for i in 0..1000 {
            files_content.push_str(&format!(
                "File: file_{}.rs\n```rust\nfn func_{}() {{\n    let val = {};\n}}\n```\n",
                i, i, i
            ));
        }

        let raw = format!("Project root: /tmp/test\n\nFiles:\n\n{}", files_content);

        let start = Instant::now();
        let result = load_or_build(&raw).await;
        let duration = start.elapsed();

        assert!(result.is_some(), "Index should be built");

        println!("Indexed 1000 files in {}ms", duration.as_millis());
        assert!(
            duration.as_millis() < 3000,
            "Indexing 1k files should complete in < 3s"
        );
    }
}
