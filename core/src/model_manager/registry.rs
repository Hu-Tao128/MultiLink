use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::providers::ProviderCapabilities;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderType {
    Ollama,
    Gemini,
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelStatus {
    Installed,
    NotInstalled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub provider: ProviderType,
    pub size_gb: f32,
    pub path: PathBuf,
    pub status: ModelStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ProviderCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameter_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
}

impl ModelInfo {
    pub fn max_context_tokens(&self) -> usize {
        self.capabilities
            .as_ref()
            .map(|c| c.max_context_tokens)
            .unwrap_or(4096)
    }

    pub fn supports_tools(&self) -> bool {
        self.capabilities.as_ref().map(|c| c.tools).unwrap_or(false)
    }

    pub fn supports_fim(&self) -> bool {
        self.capabilities.as_ref().map(|c| c.fim).unwrap_or(false)
    }

    pub fn supports_vision(&self) -> bool {
        self.capabilities
            .as_ref()
            .map(|c| c.vision)
            .unwrap_or(false)
    }
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

    pub fn get(&self, name: &str) -> Option<&ModelInfo> {
        self.models.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut ModelInfo> {
        self.models.get_mut(name)
    }

    pub fn set_capabilities(&mut self, name: &str, capabilities: ProviderCapabilities) {
        if let Some(model) = self.models.get_mut(name) {
            model.capabilities = Some(capabilities);
        }
    }
}
