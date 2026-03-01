pub mod auth;
pub mod chat_runtime;
pub mod config;
pub mod context_retrieval;
pub mod model_manager;
pub mod providers;
pub mod router;
pub mod session;
pub mod system;

pub use auth::{AuthProvider, AuthService, OAuthConfig, OAuthFlow, StoredToken, TokenStore};
pub use chat_runtime::{ChatRuntime, ChatRuntimeError, StreamEvent};
pub use config::AppConfig;
pub use model_manager::{ModelInfo, ModelManager, ModelStatus, ProviderType};
pub use providers::{
    LLMError, LLMProvider, LLMResponse, PromptOptions, ProviderCapabilities, ProviderId,
    TokenEvent, TokenStream,
};
pub use router::{ProviderAvailability, ProviderRouter};
pub use session::{ChatMessage, ChatSession, SessionState};
pub use system::{OllamaInstallPlan, SystemService};
