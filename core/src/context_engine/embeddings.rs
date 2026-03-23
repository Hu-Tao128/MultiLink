use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;
use std::time::Instant;

use crate::context_engine::chunker::SemanticChunk;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct EmbeddingCacheFile {
    vectors: HashMap<String, Vec<f32>>,
}

pub struct EmbeddingBlendResult {
    pub scores: HashMap<String, f32>,
    pub used_embeddings: bool,
    pub reason: String,
    pub latency_ms: u128,
    pub attempts: u8,
    pub base_url: String,
    pub model: String,
}

#[allow(clippy::too_many_arguments)]
pub async fn blend_embedding_scores(
    prompt: &str,
    candidates: &[SemanticChunk],
    base_url: &str,
    model: &str,
    connect_timeout_ms: u64,
    request_timeout_ms: u64,
    max_retries: u8,
    batch_size: usize,
    cache_dir: &Path,
) -> EmbeddingBlendResult {
    let started = Instant::now();
    if model.trim().is_empty() {
        return EmbeddingBlendResult {
            scores: HashMap::new(),
            used_embeddings: false,
            reason: "optional_not_configured".to_string(),
            latency_ms: 0,
            attempts: 0,
            base_url: base_url.to_string(),
            model: String::new(),
        };
    }

    if candidates.is_empty() {
        return EmbeddingBlendResult {
            scores: HashMap::new(),
            used_embeddings: false,
            reason: "no_candidates".to_string(),
            latency_ms: 0,
            attempts: 0,
            base_url: base_url.to_string(),
            model: model.to_string(),
        };
    }

    let cache_path = cache_dir.join("embeddings.json");
    let mut cache = load_cache(&cache_path);

    let mut attempts = 0u8;
    let query_vec = match embed_inputs(
        base_url,
        model,
        vec![prompt.to_string()],
        connect_timeout_ms,
        request_timeout_ms,
        max_retries,
    )
    .await
    {
        Some(v) => v.into_iter().next(),
        None => None,
    };
    attempts = attempts.saturating_add(1);
    let Some(query_vec) = query_vec else {
        return EmbeddingBlendResult {
            scores: HashMap::new(),
            used_embeddings: false,
            reason: "query_embedding_failed".to_string(),
            latency_ms: started.elapsed().as_millis(),
            attempts,
            base_url: base_url.to_string(),
            model: model.to_string(),
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
        let safe_batch_size = batch_size.max(1);
        let mut offset = 0usize;
        let mut failed_batches = 0usize;
        while offset < missing.len() {
            let end = (offset + safe_batch_size).min(missing.len());
            let batch = missing[offset..end].to_vec();
            if let Some(vectors) = embed_inputs(
                base_url,
                model,
                batch,
                connect_timeout_ms,
                request_timeout_ms,
                max_retries,
            )
            .await
            {
                attempts = attempts.saturating_add(1);
                for (idx, vec) in vectors.into_iter().enumerate() {
                    if let Some(hash) = missing_ids.get(offset + idx) {
                        cache.vectors.insert(hash.clone(), vec);
                    }
                }
            } else {
                failed_batches += 1;
            }
            offset = end;
        }
        let _ = save_cache(&cache_path, &cache);

        if failed_batches > 0 {
            // Do not fail hard when some chunk batches timeout/fail.
            // Use partial cached/new vectors and continue with hybrid ranking.
            // This keeps context quality acceptable instead of collapsing to lexical-only.
        }
    }

    let mut scores = HashMap::new();
    for chunk in candidates {
        if let Some(v) = cache.vectors.get(&chunk.chunk_hash) {
            scores.insert(chunk.chunk_hash.clone(), cosine_similarity(&query_vec, v));
        }
    }

    let used_embeddings = !scores.is_empty();
    EmbeddingBlendResult {
        scores,
        used_embeddings,
        reason: if used_embeddings {
            "ok".to_string()
        } else {
            "chunk_embedding_failed".to_string()
        },
        latency_ms: started.elapsed().as_millis(),
        attempts,
        base_url: base_url.to_string(),
        model: model.to_string(),
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
    max_retries: u8,
) -> Option<Vec<Vec<f32>>> {
    let url = format!("{}/api/embeddings", base_url.trim_end_matches('/'));
    let max_attempts = max_retries.saturating_add(1);
    let mut vectors = Vec::with_capacity(input.len());

    for prompt in input {
        let body = OllamaEmbeddingsRequest {
            model: model.to_string(),
            prompt: prompt.clone(),
            keep_alive: "10m".to_string(),
        };

        let mut attempt = 0u8;
        loop {
            attempt = attempt.saturating_add(1);
            let resp = match client.post(&url).json(&body).send().await {
                Ok(v) => v,
                Err(_) => {
                    if attempt < max_attempts {
                        tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt))).await;
                        continue;
                    }
                    return None;
                }
            };

            if !resp.status().is_success() {
                if attempt < max_attempts {
                    tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt))).await;
                    continue;
                }
                return None;
            }

            let parsed = match resp.json::<OllamaEmbeddingsResponse>().await {
                Ok(v) => v,
                Err(_) => {
                    if attempt < max_attempts {
                        tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt))).await;
                        continue;
                    }
                    return None;
                }
            };

            if parsed.embedding.is_empty() {
                return None;
            }

            vectors.push(parsed.embedding);
            break;
        }
    }

    Some(vectors)
}

async fn embed_inputs(
    base_url: &str,
    model: &str,
    input: Vec<String>,
    connect_timeout_ms: u64,
    request_timeout_ms: u64,
    max_retries: u8,
) -> Option<Vec<Vec<f32>>> {
    if input.is_empty() {
        return Some(Vec::new());
    }

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(connect_timeout_ms))
        .timeout(Duration::from_millis(request_timeout_ms))
        .build()
        .ok()?;

    let body = OllamaEmbedRequest {
        model: model.to_string(),
        input: input.clone(),
        truncate: true,
        keep_alive: "10m".to_string(),
    };
    let url = format!("{}/api/embed", base_url.trim_end_matches('/'));
    let mut attempt = 0u8;
    let max_attempts = max_retries.saturating_add(1);
    while attempt < max_attempts {
        attempt = attempt.saturating_add(1);
        let resp = match client.post(&url).json(&body).send().await {
            Ok(v) => v,
            Err(_) => {
                if attempt < max_attempts {
                    tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt))).await;
                    continue;
                }
                return None;
            }
        };
        if !resp.status().is_success() {
            if resp.status() == reqwest::StatusCode::NOT_FOUND {
                return embed_inputs_legacy_endpoint(&client, base_url, model, &input, max_retries)
                    .await;
            }
            if attempt < max_attempts {
                tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt))).await;
                continue;
            }
            return None;
        }
        let parsed = match resp.json::<OllamaEmbedResponse>().await {
            Ok(v) => v,
            Err(_) => {
                if attempt < max_attempts {
                    tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt))).await;
                    continue;
                }
                return None;
            }
        };
        if parsed.embeddings.is_empty() {
            return None;
        }
        return Some(parsed.embeddings);
    }
    None
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
        cache.vectors.insert("abc".to_string(), vec![0.1, 0.2, 0.3]);
        save_cache(&path, &cache).expect("save");
        assert!(fs::metadata(&path).is_ok());

        let loaded = load_cache(&path);
        assert_eq!(loaded.vectors.get("abc").map(|v| v.len()), Some(3));
    }
}
