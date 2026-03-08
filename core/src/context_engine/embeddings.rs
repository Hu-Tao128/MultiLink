use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::context_engine::chunker::SemanticChunk;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct EmbeddingCacheFile {
    vectors: HashMap<String, Vec<f32>>,
}

pub struct EmbeddingBlendResult {
    pub scores: HashMap<String, f32>,
    pub used_embeddings: bool,
}

pub async fn blend_embedding_scores(
    prompt: &str,
    candidates: &[SemanticChunk],
    base_url: &str,
    model: &str,
    cache_dir: &Path,
) -> EmbeddingBlendResult {
    if candidates.is_empty() {
        return EmbeddingBlendResult {
            scores: HashMap::new(),
            used_embeddings: false,
        };
    }

    let cache_path = cache_dir.join("embeddings.json");
    let mut cache = load_cache(&cache_path);

    let query_vec = match embed_inputs(base_url, model, vec![prompt.to_string()]).await {
        Some(v) => v.into_iter().next(),
        None => None,
    };
    let Some(query_vec) = query_vec else {
        return EmbeddingBlendResult {
            scores: HashMap::new(),
            used_embeddings: false,
        };
    };

    let mut missing = Vec::new();
    let mut missing_ids = Vec::new();
    for chunk in candidates {
        if !cache.vectors.contains_key(&chunk.chunk_hash) {
            let text = format!(
                "File: {}\nSymbol: {}\nStart: {}\n{}",
                chunk.file, chunk.symbol, chunk.start_line, chunk.content
            );
            missing.push(text);
            missing_ids.push(chunk.chunk_hash.clone());
        }
    }

    if !missing.is_empty() {
        if let Some(vectors) = embed_inputs(base_url, model, missing).await {
            for (idx, vec) in vectors.into_iter().enumerate() {
                if let Some(hash) = missing_ids.get(idx) {
                    cache.vectors.insert(hash.clone(), vec);
                }
            }
            let _ = save_cache(&cache_path, &cache);
        }
    }

    let mut scores = HashMap::new();
    for chunk in candidates {
        if let Some(v) = cache.vectors.get(&chunk.chunk_hash) {
            scores.insert(chunk.chunk_hash.clone(), cosine_similarity(&query_vec, v));
        }
    }

    EmbeddingBlendResult {
        scores,
        used_embeddings: true,
    }
}

fn load_cache(path: &Path) -> EmbeddingCacheFile {
    let Ok(raw) = fs::read(path) else {
        return EmbeddingCacheFile::default();
    };
    serde_json::from_slice::<EmbeddingCacheFile>(&raw).unwrap_or_default()
}

fn save_cache(path: &Path, cache: &EmbeddingCacheFile) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec(cache).unwrap_or_default();
    fs::write(path, bytes)
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

async fn embed_inputs(base_url: &str, model: &str, input: Vec<String>) -> Option<Vec<Vec<f32>>> {
    if input.is_empty() {
        return Some(Vec::new());
    }

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(12))
        .build()
        .ok()?;

    let body = OllamaEmbedRequest {
        model: model.to_string(),
        input,
        truncate: true,
        keep_alive: "10m".to_string(),
    };
    let url = format!("{}/api/embed", base_url.trim_end_matches('/'));
    let resp = client.post(url).json(&body).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed = resp.json::<OllamaEmbedResponse>().await.ok()?;
    if parsed.embeddings.is_empty() {
        return None;
    }
    Some(parsed.embeddings)
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{load_cache, save_cache, EmbeddingCacheFile};

    #[test]
    fn embedding_cache_roundtrip() {
        let temp = tempfile::tempdir().expect("temp");
        let path = temp.path().join("embeddings.json");

        let mut cache = EmbeddingCacheFile::default();
        cache
            .vectors
            .insert("abc".to_string(), vec![0.1, 0.2, 0.3]);
        save_cache(&path, &cache).expect("save");
        assert!(fs::metadata(&path).is_ok());

        let loaded = load_cache(&path);
        assert_eq!(loaded.vectors.get("abc").map(|v| v.len()), Some(3));
    }
}
