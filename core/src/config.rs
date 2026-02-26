use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::providers::ProviderId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub preferred_provider: ProviderId,
    pub ollama: OllamaConfig,
    pub gemini: RemoteProviderConfig,
    pub codex: RemoteProviderConfig,
    pub storage: StorageConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub base_url: String,
    pub default_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteProviderConfig {
    pub enabled: bool,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub models_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeConfig {
    pub max_context_tokens: usize,
    pub summary_trigger_tokens: usize,
    pub keep_last_messages: usize,
    pub max_summary_tokens: usize,
    pub max_project_files: usize,
    pub max_project_bytes: usize,
    pub max_project_file_bytes: usize,
    pub max_project_context_tokens: usize,
    pub max_parallel_streams: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: 7000,
            summary_trigger_tokens: 6000,
            keep_last_messages: 6,
            max_summary_tokens: 1200,
            max_project_files: 30,
            max_project_bytes: 200 * 1024,
            max_project_file_bytes: 64 * 1024,
            max_project_context_tokens: 3500,
            max_parallel_streams: 4,
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            preferred_provider: ProviderId::Ollama,
            ollama: OllamaConfig {
                base_url: "http://127.0.0.1:11434".to_string(),
                default_model: "llama3.2".to_string(),
            },
            gemini: RemoteProviderConfig {
                enabled: true,
                timeout_secs: 20,
            },
            codex: RemoteProviderConfig {
                enabled: true,
                timeout_secs: 20,
            },
            storage: StorageConfig {
                models_dir: "~/.local/share/multilink/models".to_string(),
            },
            runtime: RuntimeConfig::default(),
        }
    }
}

impl AppConfig {
    pub async fn load_or_create(path: &Path) -> Result<Self, ConfigError> {
        if path.exists() {
            let content = fs::read_to_string(path).await?;
            let mut parsed = toml::from_str::<Self>(&content)?;
            parsed.apply_env_overrides();
            return Ok(parsed);
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let default = Self::default();
        let content = toml::to_string_pretty(&default)?;
        fs::write(path, content).await?;
        restrict_permissions(path)?;

        let mut loaded = default;
        loaded.apply_env_overrides();
        Ok(loaded)
    }

    pub fn default_user_config_path() -> PathBuf {
        if let Some(config_dir) = dirs::config_dir() {
            return config_dir.join("multilink").join("config.toml");
        }
        PathBuf::from("./config/default.toml")
    }

    pub fn apply_env_overrides(&mut self) {
        if let Ok(value) = std::env::var("MULTILINK_PROVIDER") {
            self.preferred_provider = match value.to_ascii_lowercase().as_str() {
                "gemini" => ProviderId::Gemini,
                "codex" => ProviderId::Codex,
                _ => ProviderId::Ollama,
            };
        }

        if let Ok(value) = std::env::var("MULTILINK_OLLAMA_BASE_URL") {
            self.ollama.base_url = value;
        }

        if let Ok(value) = std::env::var("MULTILINK_OLLAMA_MODEL") {
            self.ollama.default_model = value;
        }

        if let Ok(value) = std::env::var("MULTILINK_MODELS_DIR") {
            self.storage.models_dir = value;
        }
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml deserialize error: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("toml serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),
}
