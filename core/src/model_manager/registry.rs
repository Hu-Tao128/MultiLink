use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderType {
    Ollama,
    Gemini,
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelStatus {
    Installed,
    NotInstalled,
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub provider: ProviderType,
    pub size_gb: f32,
    pub path: PathBuf,
    pub status: ModelStatus,
}

pub struct ModelManager {
    models: HashMap<String, ModelInfo>,
    active_local_model: Option<String>,
}

impl Default for ModelManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelManager {
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
            active_local_model: None,
        }
    }

    pub fn upsert(&mut self, model: ModelInfo) {
        self.models.insert(model.name.clone(), model);
    }

    pub fn list(&self) -> Vec<ModelInfo> {
        self.models.values().cloned().collect()
    }

    pub fn total_installed_size_gb(&self) -> f32 {
        self.models
            .values()
            .filter(|m| m.status == ModelStatus::Installed)
            .map(|m| m.size_gb)
            .sum()
    }

    pub fn set_active_local_model(&mut self, model_name: String) {
        self.active_local_model = Some(model_name);
    }

    pub fn active_local_model(&self) -> Option<&str> {
        self.active_local_model.as_deref()
    }
}
