use std::path::Path;

use multilink_core::{AppConfig, ProviderId};

#[tokio::test]
async fn creates_default_config_when_missing() {
    let temp = tempfile::tempdir().expect("temp dir");
    let file_path = temp.path().join("config.toml");

    let config = AppConfig::load_or_create(Path::new(&file_path))
        .await
        .expect("config must load");

    assert!(file_path.exists());
    assert_eq!(config.preferred_provider, ProviderId::Ollama);
}
