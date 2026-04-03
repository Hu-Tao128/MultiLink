use std::fs;
use std::path::Path;

use multilink_core::context_engine::chunker::{semantic_chunks, SourceFile};
use multilink_core::context_engine::index::load_or_build;
use multilink_core::context_retrieval::{build_relevant_project_context, RetrievalConfig};
use multilink_core::{
    AppConfig, BenchmarkResult, BenchmarkRunner, ReleaseChecklist, ReleaseCriteria,
};

const CONTEXT_BUILD_THRESHOLD_MS: f64 = 1000.0;
const LEXICAL_FALLBACK_THRESHOLD_MS: f64 = 200.0;
const CONFIG_LOAD_VALIDATE_THRESHOLD_MS: f64 = 50.0;
const CHUNKER_THRESHOLD_MS: f64 = 100.0;

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directories");
    }
    fs::write(path, content).expect("write file");
}

fn language_for_path(path: &str) -> &'static str {
    if path.ends_with(".rs") {
        "rust"
    } else if path.ends_with(".toml") {
        "toml"
    } else if path.ends_with(".md") {
        "md"
    } else {
        "text"
    }
}

fn synthetic_project_files() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "Cargo.toml",
            "[package]\nname = \"bench-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "README.md",
            "# Bench Project\nSmall synthetic fixture for context benchmarks.\n",
        ),
        (
            "src/main.rs",
            "mod engine;\nmod config;\n\nfn main() {\n    println!(\"ok\");\n}\n",
        ),
        (
            "src/engine.rs",
            "pub fn score(query: &str) -> usize {\n    query.split_whitespace().count()\n}\n",
        ),
        (
            "src/config.rs",
            "pub struct Settings {\n    pub top_k: usize,\n    pub max_tokens: usize,\n}\n",
        ),
        (
            "src/router.rs",
            "pub fn route(input: &str) -> &'static str {\n    if input.contains(\"help\") { \"docs\" } else { \"chat\" }\n}\n",
        ),
        (
            "src/chunker.rs",
            "pub fn chunk_lines(content: &str) -> Vec<&str> {\n    content.lines().collect()\n}\n",
        ),
        (
            "src/index.rs",
            "pub fn build_index(paths: &[String]) -> usize {\n    paths.len()\n}\n",
        ),
        (
            "tests/smoke.rs",
            "#[test]\nfn smoke() {\n    assert_eq!(2 + 2, 4);\n}\n",
        ),
        (
            "docs/architecture.md",
            "## Architecture\nSynthetic project for benchmark tests.\n",
        ),
    ]
}

fn create_small_project_fixture() -> (tempfile::TempDir, String) {
    let temp = tempfile::tempdir().expect("create tempdir");
    let root = temp.path();
    let mut raw = format!("Project root: {}\n\nFiles:\n\n", root.display());

    for (relative, content) in synthetic_project_files() {
        write_file(&root.join(relative), content);
        raw.push_str(&format!(
            "File: {}\n```{}\n{}\n```\n\n",
            relative,
            language_for_path(relative),
            content
        ));
    }

    (temp, raw)
}

fn retrieval_config_without_embeddings() -> RetrievalConfig {
    RetrievalConfig {
        embeddings_enabled: false,
        embed_model: String::new(),
        embed_base_url: "http://127.0.0.1:1".to_string(),
        ollama_base_url: "http://127.0.0.1:1".to_string(),
        embed_connect_timeout_ms: 50,
        embed_request_timeout_ms: 50,
        embed_max_retries: 0,
        embed_batch_size: 8,
        top_k: 8,
    }
}

fn rust_source_200_lines() -> String {
    let mut out = String::from("pub struct BenchChunker;\n\nimpl BenchChunker {\n");
    for i in 0..65 {
        out.push_str(&format!(
            "    pub fn method_{i}(&self, input: i32) -> i32 {{\n        let base = input + {i};\n        base * 2\n    }}\n\n"
        ));
    }
    out.push_str("}\n");
    out
}

async fn benchmark_context_engine_build() -> BenchmarkResult {
    let (_fixture, base_raw) = create_small_project_fixture();
    let mut run_id = 0usize;
    let runner = BenchmarkRunner::new().with_warmup(1).with_iterations(20);

    runner
        .run_async("context_engine_build", || {
            run_id += 1;
            let raw = format!("{}\n# benchmark_run={}\n", base_raw, run_id);
            async move {
                let indexed = load_or_build(&raw).await;
                assert!(
                    indexed.is_some(),
                    "index should be built from synthetic project"
                );
            }
        })
        .await
}

async fn benchmark_lexical_fallback() -> BenchmarkResult {
    let (_fixture, raw_context) = create_small_project_fixture();
    let config = retrieval_config_without_embeddings();
    let runner = BenchmarkRunner::new().with_warmup(1).with_iterations(40);

    runner
        .run_async("lexical_fallback", || {
            let raw = raw_context.clone();
            let cfg = config.clone();
            async move {
                let result = build_relevant_project_context(
                    &raw,
                    "how does the chunker and index work in this project?",
                    1600,
                    Some("gpt-4o-mini"),
                    cfg,
                )
                .await;

                assert!(
                    !result.context.is_empty(),
                    "fallback context should not be empty"
                );
                assert!(
                    !result.embedding_used,
                    "embeddings should be disabled in fallback benchmark"
                );
            }
        })
        .await
}

async fn benchmark_config_load_validate() -> BenchmarkResult {
    let temp = tempfile::tempdir().expect("create tempdir");
    let config_path = temp.path().join("multilink.toml");

    // Create once, benchmark hot path: load + validate existing file.
    let _ = AppConfig::load_or_create(&config_path)
        .await
        .expect("create config once before benchmark");

    let runner = BenchmarkRunner::new().with_warmup(2).with_iterations(40);
    runner
        .run_async("config_load_validate", || {
            let path = config_path.clone();
            async move {
                let loaded = AppConfig::load_or_create(&path)
                    .await
                    .expect("config load should succeed");
                assert!(
                    !loaded.servers.is_empty(),
                    "loaded config should contain servers"
                );
            }
        })
        .await
}

fn benchmark_chunker() -> BenchmarkResult {
    let source = SourceFile {
        path: "src/bench_chunker.rs".to_string(),
        language: "rust".to_string(),
        content: rust_source_200_lines(),
    };

    let runner = BenchmarkRunner::new().with_warmup(5).with_iterations(120);
    runner.run("chunker", || {
        let chunks = semantic_chunks(std::slice::from_ref(&source), 24);
        assert!(
            !chunks.is_empty(),
            "chunker should produce at least one chunk"
        );
    })
}

#[tokio::test]
async fn benchmark_context_engine_build_under_threshold() {
    let result = benchmark_context_engine_build().await;
    println!(
        "{}: avg={:.2}ms throughput={:.2}ops/s",
        result.name, result.avg_latency_ms, result.throughput
    );
    assert!(
        result.avg_latency_ms < CONTEXT_BUILD_THRESHOLD_MS,
        "context build avg latency {}ms exceeded {}ms",
        result.avg_latency_ms,
        CONTEXT_BUILD_THRESHOLD_MS
    );
}

#[tokio::test]
async fn benchmark_lexical_fallback_under_threshold() {
    let result = benchmark_lexical_fallback().await;
    println!(
        "{}: avg={:.2}ms throughput={:.2}ops/s",
        result.name, result.avg_latency_ms, result.throughput
    );
    assert!(
        result.avg_latency_ms < LEXICAL_FALLBACK_THRESHOLD_MS,
        "lexical fallback avg latency {}ms exceeded {}ms",
        result.avg_latency_ms,
        LEXICAL_FALLBACK_THRESHOLD_MS
    );
}

#[tokio::test]
async fn benchmark_config_load_validate_under_threshold() {
    let result = benchmark_config_load_validate().await;
    println!(
        "{}: avg={:.2}ms throughput={:.2}ops/s",
        result.name, result.avg_latency_ms, result.throughput
    );
    assert!(
        result.avg_latency_ms < CONFIG_LOAD_VALIDATE_THRESHOLD_MS,
        "config load+validate avg latency {}ms exceeded {}ms",
        result.avg_latency_ms,
        CONFIG_LOAD_VALIDATE_THRESHOLD_MS
    );
}

#[test]
fn benchmark_chunker_under_threshold() {
    let result = benchmark_chunker();
    println!(
        "{}: avg={:.2}ms throughput={:.2}ops/s",
        result.name, result.avg_latency_ms, result.throughput
    );
    assert!(
        result.avg_latency_ms < CHUNKER_THRESHOLD_MS,
        "chunker avg latency {}ms exceeded {}ms",
        result.avg_latency_ms,
        CHUNKER_THRESHOLD_MS
    );
}

#[tokio::test]
async fn release_gate_passes() {
    let context_build = benchmark_context_engine_build().await;
    let lexical_fallback = benchmark_lexical_fallback().await;
    let config_load_validate = benchmark_config_load_validate().await;
    let chunker = benchmark_chunker();

    let release_criteria = ReleaseCriteria::default();
    let mut failures = Vec::new();

    if !release_criteria.check(&context_build) {
        failures.extend(
            release_criteria
                .report(&context_build)
                .into_iter()
                .map(|issue| format!("{}: {}", context_build.name, issue)),
        );
    }

    let benchmarks = [
        (&lexical_fallback, LEXICAL_FALLBACK_THRESHOLD_MS),
        (&config_load_validate, CONFIG_LOAD_VALIDATE_THRESHOLD_MS),
        (&chunker, CHUNKER_THRESHOLD_MS),
    ];

    for (result, max_latency) in benchmarks {
        if result.avg_latency_ms > max_latency {
            failures.push(format!(
                "{}: latency {}ms exceeds threshold {}ms",
                result.name, result.avg_latency_ms, max_latency
            ));
        }
        if result.throughput < release_criteria.min_throughput {
            failures.push(format!(
                "{}: throughput {} ops/s below threshold {} ops/s",
                result.name, result.throughput, release_criteria.min_throughput
            ));
        }
    }

    println!(
        "Benchmark report:\n- {} avg={:.2}ms throughput={:.2}ops/s\n- {} avg={:.2}ms throughput={:.2}ops/s\n- {} avg={:.2}ms throughput={:.2}ops/s\n- {} avg={:.2}ms throughput={:.2}ops/s",
        context_build.name,
        context_build.avg_latency_ms,
        context_build.throughput,
        lexical_fallback.name,
        lexical_fallback.avg_latency_ms,
        lexical_fallback.throughput,
        config_load_validate.name,
        config_load_validate.avg_latency_ms,
        config_load_validate.throughput,
        chunker.name,
        chunker.avg_latency_ms,
        chunker.throughput
    );

    let checklist = ReleaseChecklist {
        core_tests_pass: true,
        gui_builds: vec!["linux".to_string()],
        benchmarks_pass: failures.is_empty(),
        documentation_updated: true,
    };
    println!("{}", checklist.summary());

    if !failures.is_empty() {
        panic!("release benchmark gate failed:\n{}", failures.join("\n"));
    }
}
