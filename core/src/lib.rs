pub mod auth;
pub mod benchmark;
pub mod chat_runtime;
pub mod commands;
pub mod config;
pub mod context_engine;
pub mod context_retrieval;
pub mod execution;
pub mod hardware_profile;
pub mod intent_budget;
pub mod lan_agent;
pub mod mcp_adapter;
pub mod model_manager;
pub mod model_profile;
pub mod observability;
pub mod providers;
pub mod router;
pub mod session;
pub mod skills;
pub mod system;

pub use context_engine::{
    ContextEngine, ContextEngineV1, ContextEngineV2, ContextEngineV2Plus, ContextEngineVersion,
    ContextRetrievalConfig, RetrievalResult,
};

pub use auth::{AuthProvider, AuthService, OAuthConfig, OAuthFlow, StoredToken, TokenStore};
pub use benchmark::{BenchmarkResult, BenchmarkRunner, ReleaseChecklist, ReleaseCriteria};
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
pub use lan_agent::{
    decode_messagepack, encode_messagepack, shared_secret_matches, LanAgentServer, LanEnvelope,
    LanPayload,
};
pub use mcp_adapter::{
    McpContent, McpError, McpResult, McpToolCall, McpToolResponse, ThinMcpAdapter,
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
pub use skills::{
    Skill, SkillLoader, SkillManifest, SkillOrchestrator, SkillParameter, SkillShare, SkillSharer,
};
pub use system::{OllamaInstallPlan, SystemService};
