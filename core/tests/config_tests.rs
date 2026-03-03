use std::path::Path;
use tokio::fs;

use multilink_core::{AppConfig, ProviderKind};

#[tokio::test]
async fn creates_default_config_when_missing() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file_path = temp.path().join("config.toml");

    let config = AppConfig::load_or_create(Path::new(&file_path))
        .await
        .expect("config must load");

    assert!(file_path.exists());
    assert_eq!(config.version, 2);
    assert!(!config.servers.is_empty());
    assert_eq!(config.servers[0].provider, ProviderKind::Ollama);
}

#[tokio::test]
async fn migrates_v1_config_to_v2_servers() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file_path = temp.path().join("config.toml");
    let legacy = r#"
version = 1
preferred_provider = "ollama"

[ollama]
base_url = "http://192.168.1.20:11434"
default_model = "qwen2.5-coder:3b"

[context]
embeddings_enabled = true
embed_model = "embeddinggemma"
project_top_k = 8
max_project_tokens = 2000
debug = false

[runtime]
max_context_tokens = 7000
summary_trigger_tokens = 6000
keep_last_messages = 6
max_summary_tokens = 1200
max_project_files = 30
max_project_bytes = 204800
max_project_file_bytes = 65536
max_project_context_tokens = 3500
max_parallel_streams = 4
context_embeddings_enabled = true
context_debug = false
context_embed_model = "embeddinggemma"
context_project_top_k = 8
context_ollama_base_url = "http://127.0.0.1:11434"
observability_json_logs = false

[storage]
models_dir = "~/.local/share/multilink/models"
"#;
    fs::write(&file_path, legacy).await.expect("write legacy config");

    let config = AppConfig::load_or_create(Path::new(&file_path))
        .await
        .expect("migrated config must load");

    assert_eq!(config.version, 2);
    assert!(!config.servers.is_empty());
    assert_eq!(config.servers[0].base_url, "http://192.168.1.20:11434");
}
