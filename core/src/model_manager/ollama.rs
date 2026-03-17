use std::path::{Path, PathBuf};

use tokio::fs;

use super::{ModelInfo, ModelStatus, ProviderType};

pub struct OllamaModelManager {
    pub api_base_url: String,
    pub models_dir: PathBuf,
}

impl OllamaModelManager {
    pub fn new(api_base_url: String, models_dir: PathBuf) -> Self {
        Self {
            api_base_url,
            models_dir,
        }
    }

    pub fn detect_models_dir() -> PathBuf {
        if let Ok(custom) = std::env::var("OLLAMA_MODELS") {
            return PathBuf::from(custom);
        }

        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            if let Some(home) = dirs::home_dir() {
                let home_models = home.join(".ollama/models");
                if home_models.exists() || home.join(".ollama").exists() {
                    return home_models;
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            let system_path = PathBuf::from("/var/lib/ollama/models");
            if system_path.exists() {
                return system_path;
            }
        }

        PathBuf::from("/var/lib/ollama/models")
    }

    pub async fn ensure_models_dir(&self) -> Result<(), std::io::Error> {
        fs::create_dir_all(&self.models_dir).await
    }

    pub async fn list_installed_models(&self) -> Result<Vec<ModelInfo>, std::io::Error> {
        let mut out = Vec::new();
        if !self.models_dir.exists() {
            return Ok(out);
        }

        let mut entries = fs::read_dir(&self.models_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                let size_gb = dir_size_bytes(&path).await? as f32 / 1_000_000_000.0;
                out.push(ModelInfo {
                    name: entry.file_name().to_string_lossy().to_string(),
                    provider: ProviderType::Ollama,
                    size_gb,
                    path,
                    status: ModelStatus::Installed,
                    capabilities: None,
                    parameter_count: None,
                    quantization: None,
                });
            }
        }
        Ok(out)
    }

    pub async fn remove_model(&self, model_name: &str) -> Result<(), std::io::Error> {
        let model_path = self.models_dir.join(model_name);
        if model_path.exists() {
            fs::remove_dir_all(model_path).await?;
        }
        Ok(())
    }

    pub fn set_models_dir(&mut self, new_dir: PathBuf) {
        self.models_dir = new_dir;
    }

    pub async fn migrate_models_dir(&self, destination: &Path) -> Result<(), std::io::Error> {
        if !self.models_dir.exists() {
            return Ok(());
        }

        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::rename(&self.models_dir, destination).await?;
        create_symlink(destination, &self.models_dir).await
    }
}

async fn dir_size_bytes(path: &Path) -> Result<u64, std::io::Error> {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];

    while let Some(current) = stack.pop() {
        let mut entries = fs::read_dir(current).await?;
        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_dir() {
                stack.push(entry.path());
            } else if metadata.is_file() {
                total = total.saturating_add(metadata.len());
            }
        }
    }

    Ok(total)
}

#[cfg(target_family = "unix")]
async fn create_symlink(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
    std::os::unix::fs::symlink(src, dst)
}

#[cfg(target_family = "windows")]
async fn create_symlink(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
    std::os::windows::fs::symlink_dir(src, dst)
}
