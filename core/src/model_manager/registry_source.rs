use std::path::PathBuf;

use reqwest::Client;
use serde::Deserialize;

use super::{ModelInfo, ModelStatus, ProviderType};

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("network error: {0}")]
    Network(String),
    #[error("parse error: {0}")]
    Parse(String),
}

pub trait ModelRegistrySource: Send + Sync {
    fn name(&self) -> &str;
    fn list_models<'a>(
        &'a self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<ModelInfo>, RegistryError>> + Send + 'a,
        >,
    >;
}

#[derive(Clone)]
pub struct HttpRegistrySource {
    pub endpoint: String,
}

#[derive(Deserialize)]
struct RegistryModel {
    name: String,
    size_gb: f32,
}

impl ModelRegistrySource for HttpRegistrySource {
    fn name(&self) -> &str {
        "http"
    }

    fn list_models<'a>(
        &'a self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<ModelInfo>, RegistryError>> + Send + 'a,
        >,
    > {
        Box::pin(async move {
            let client = Client::new();
            let response = client
                .get(&self.endpoint)
                .send()
                .await
                .map_err(|e| RegistryError::Network(e.to_string()))?;
            let payload = response
                .json::<Vec<RegistryModel>>()
                .await
                .map_err(|e| RegistryError::Parse(e.to_string()))?;
            Ok(payload
                .into_iter()
                .map(|item| ModelInfo {
                    name: item.name,
                    provider: ProviderType::Ollama,
                    size_gb: item.size_gb,
                    path: PathBuf::new(),
                    status: ModelStatus::NotInstalled,
                })
                .collect())
        })
    }
}

#[derive(Clone)]
pub struct NpmLikeRegistrySource {
    pub endpoint: String,
}

impl ModelRegistrySource for NpmLikeRegistrySource {
    fn name(&self) -> &str {
        "npm-like"
    }

    fn list_models<'a>(
        &'a self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<ModelInfo>, RegistryError>> + Send + 'a,
        >,
    > {
        Box::pin(async move {
            let client = Client::new();
            let response = client
                .get(&self.endpoint)
                .send()
                .await
                .map_err(|e| RegistryError::Network(e.to_string()))?;
            let payload = response
                .json::<Vec<RegistryModel>>()
                .await
                .map_err(|e| RegistryError::Parse(e.to_string()))?;
            Ok(payload
                .into_iter()
                .map(|item| ModelInfo {
                    name: item.name,
                    provider: ProviderType::Codex,
                    size_gb: item.size_gb,
                    path: PathBuf::new(),
                    status: ModelStatus::NotInstalled,
                })
                .collect())
        })
    }
}
