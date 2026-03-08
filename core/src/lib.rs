pub mod auth;
pub mod chat_runtime;
pub mod config;
pub mod context_retrieval;
pub mod execution;
pub mod hardware_profile;
pub mod intent_budget;
pub mod model_manager;
pub mod model_profile;
pub mod observability;
pub mod providers;
pub mod router;
pub mod session;
pub mod system;

pub use auth::{AuthProvider, AuthService, OAuthConfig, OAuthFlow, StoredToken, TokenStore};
pub use chat_runtime::{
    ChatResponse, ChatRuntime, ChatRuntimeError, HandleUserMessageRequest, StreamEvent,
};
pub use config::{AppConfig, ProviderKind, RemoteThreshold, TaskWeight};
pub use execution::{
    ExecutionDispatchRequest, ExecutionDispatchResult, ExecutionDispatcher, ServerStatus,
};
pub use hardware_profile::{HardwareCaps, HardwareProfile};
pub use intent_budget::{
    budget_for_intent, detect_query_intent, task_weight_for_prompt, IntentBudget, QueryIntent,
};
pub use model_manager::{ModelInfo, ModelManager, ModelStatus, ProviderType};
pub use model_profile::{ModelClass, ModelProfile, RetrievalBudget};
pub use observability::ExecutionMetrics;
pub use providers::{
    LLMError, LLMProvider, LLMResponse, PromptOptions, ProviderCapabilities, ProviderId,
    TokenEvent, TokenStream,
};
pub use router::{ProviderAvailability, ProviderRouter};
pub use session::{ChatMessage, ChatSession, SessionState};
pub use system::{OllamaInstallPlan, SystemService};
