mod ollama;
mod registry;

pub use ollama::OllamaModelManager;
pub use registry::{ModelInfo, ModelManager, ModelStatus, ProviderType};
