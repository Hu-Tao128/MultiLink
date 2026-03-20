use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::fs;

pub const CURRENT_CONFIG_VERSION: u32 = 2;

const OLLAMA_COMMON_PORTS: &[u16] = &[11434, 10101];

pub fn detect_ollama_base_url() -> String {
    if let Ok(env_url) = std::env::var("OLLAMA_HOST") {
        if !env_url.is_empty() {
            return normalize_ollama_url(&env_url);
        }
    }

    if let Ok(env_url) = std::env::var("MULTILINK_OLLAMA_BASE_URL") {
        if !env_url.is_empty() {
            return normalize_ollama_url(&env_url);
        }
    }

    for &port in OLLAMA_COMMON_PORTS {
        let url = format!("http://127.0.0.1:{}", port);
        if is_port_open("127.0.0.1", port) {
            return url;
        }
        let url_ipv6 = format!("http://[::1]:{}", port);
        if is_port_open("::1", port) {
            return url_ipv6;
        }
    }

    if let Ok(output) = std::process::Command::new("ollama").arg("list").output() {
        if output.status.success() {
            return "http://127.0.0.1:11434".to_string();
        }
    }

    eprintln!(
        "Warning: Could not detect Ollama server, using default http://127.0.0.1:11434. \
         Set OLLAMA_HOST or MULTILINK_OLLAMA_BASE_URL to override."
    );
    "http://127.0.0.1:11434".to_string()
}

fn normalize_ollama_url(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{}", trimmed)
    }
}

fn is_port_open(host: &str, port: u16) -> bool {
    let addr = format!("{}:{}", host, port);
    TcpStream::connect_timeout(
        &addr
            .parse()
            .unwrap_or_else(|_| "127.0.0.1:0".parse().unwrap()),
        Duration::from_millis(500),
    )
    .is_ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Ollama,
    Gemini,
    Codex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub provider: ProviderKind,
    pub base_url: String,
    pub default_model: String,
    pub priority: u8,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub models_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextConfig {
    pub embeddings_enabled: bool,
    pub embed_model: String,
    pub embed_base_url: Option<String>,
    pub embed_connect_timeout_ms: Option<u64>,
    pub embed_request_timeout_ms: Option<u64>,
    pub embed_max_retries: Option<u8>,
    pub embed_batch_size: Option<usize>,
    pub project_top_k: usize,
    pub max_project_tokens: usize,
    pub debug: bool,
    pub engine: String,
    pub index_refresh_on_query: bool,
    pub retrieval_enable_filters: bool,
    pub v2plus_metrics: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PerformanceConfig {
    pub profile: String,
    pub max_parallel_streams: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NetworkConfig {
    pub allow_remote: bool,
    pub shared_secret: String,
    pub allowed_ips: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub streaming: bool,
    pub json_logs: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskWeight {
    Trivial,
    Light,
    Medium,
    Heavy,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RemoteThreshold {
    Auto,
    Light,
    Medium,
    Heavy,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RoutingConfig {
    pub remote_threshold: RemoteThreshold,
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
    pub profiles: RuntimeProfiles,
    pub context_embeddings_enabled: bool,
    pub context_debug: bool,
    pub context_embed_model: String,
    pub embed_base_url: String,
    pub embed_connect_timeout_ms: u64,
    pub embed_request_timeout_ms: u64,
    pub embed_max_retries: u8,
    pub embed_batch_size: usize,
    pub context_project_top_k: usize,
    pub context_ollama_base_url: String,
    pub context_engine: String,
    pub context_index_refresh_on_query: bool,
    pub context_retrieval_enable_filters: bool,
    pub context_v2plus_metrics: bool,
    pub observability_json_logs: bool,
    #[serde(default)]
    pub execution_servers: Vec<ExecutionServerRuntime>,
    pub remote_threshold: RemoteThreshold,
    pub network_allow_remote: bool,
    pub network_shared_secret: String,
    pub network_allowed_ips: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecutionServerRuntime {
    pub name: String,
    pub base_url: String,
    pub default_model: String,
    pub priority: u8,
    pub enabled: bool,
    pub max_concurrency: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeProfiles {
    pub small: RuntimeProfile,
    pub medium: RuntimeProfile,
    pub large: RuntimeProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeProfile {
    pub max_project_context_tokens: usize,
    pub max_project_files: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTier {
    Small,
    Medium,
    Large,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_config_version")]
    pub version: u32,
    #[serde(default)]
    pub servers: Vec<ServerConfig>,
    #[serde(default)]
    pub context: ContextConfig,
    #[serde(default)]
    pub performance: PerformanceConfig,
    #[serde(default)]
    pub network: NetworkConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub routing: RoutingConfig,
    pub storage: StorageConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_context_dir: Option<PathBuf>,

    // legacy v1 fields, kept for migration only
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ollama: Option<LegacyOllamaConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyOllamaConfig {
    pub base_url: String,
    pub default_model: String,
}

fn default_config_version() -> u32 {
    CURRENT_CONFIG_VERSION
}

impl Default for RuntimeProfiles {
    fn default() -> Self {
        Self {
            small: RuntimeProfile {
                max_project_context_tokens: 800,
                max_project_files: 6,
            },
            medium: RuntimeProfile {
                max_project_context_tokens: 2000,
                max_project_files: 15,
            },
            large: RuntimeProfile {
                max_project_context_tokens: 3500,
                max_project_files: 30,
            },
        }
    }
}

impl Default for RuntimeProfile {
    fn default() -> Self {
        Self {
            max_project_context_tokens: 3500,
            max_project_files: 30,
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let ollama_url = detect_ollama_base_url();
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
            profiles: RuntimeProfiles::default(),
            context_embeddings_enabled: true,
            context_debug: false,
            context_embed_model: String::new(),
            embed_base_url: ollama_url.clone(),
            embed_connect_timeout_ms: 2_000,
            embed_request_timeout_ms: 12_000,
            embed_max_retries: 1,
            embed_batch_size: 24,
            context_project_top_k: 8,
            context_ollama_base_url: ollama_url,
            context_engine: "v1".to_string(),
            context_index_refresh_on_query: true,
            context_retrieval_enable_filters: false,
            context_v2plus_metrics: true,
            observability_json_logs: false,
            execution_servers: Vec::new(),
            remote_threshold: RemoteThreshold::Heavy,
            network_allow_remote: false,
            network_shared_secret: String::new(),
            network_allowed_ips: Vec::new(),
        }
    }
}

impl Default for ExecutionServerRuntime {
    fn default() -> Self {
        Self {
            name: "Local Ollama".to_string(),
            base_url: detect_ollama_base_url(),
            default_model: "qwen2.5-coder:3b".to_string(),
            priority: 1,
            enabled: true,
            max_concurrency: 1,
        }
    }
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            embeddings_enabled: true,
            embed_model: String::new(),
            embed_base_url: None,
            embed_connect_timeout_ms: None,
            embed_request_timeout_ms: None,
            embed_max_retries: None,
            embed_batch_size: None,
            project_top_k: 8,
            max_project_tokens: 2000,
            debug: false,
            engine: "v1".to_string(),
            index_refresh_on_query: true,
            retrieval_enable_filters: false,
            v2plus_metrics: true,
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            profile: "auto".to_string(),
            max_parallel_streams: 4,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            streaming: true,
            json_logs: false,
        }
    }
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            remote_threshold: RemoteThreshold::Heavy,
        }
    }
}

impl RuntimeConfig {
    pub fn tier_for_model(&self, model: Option<&str>) -> ModelTier {
        let Some(size_b) = model.and_then(extract_model_size_billions) else {
            return ModelTier::Medium;
        };

        if size_b < 4.0 {
            ModelTier::Small
        } else if size_b <= 8.0 {
            ModelTier::Medium
        } else {
            ModelTier::Large
        }
    }

    pub fn profile_for_model(&self, model: Option<&str>) -> RuntimeProfile {
        match self.tier_for_model(model) {
            ModelTier::Small => self.profiles.small.clone(),
            ModelTier::Medium => self.profiles.medium.clone(),
            ModelTier::Large => self.profiles.large.clone(),
        }
    }

    pub fn effective_for_model(&self, model: Option<&str>) -> Self {
        let mut next = self.clone();
        let profile = self.profile_for_model(model);
        next.max_project_context_tokens = profile.max_project_context_tokens;
        next.max_project_files = profile.max_project_files;
        next
    }
}

fn extract_model_size_billions(model: &str) -> Option<f32> {
    let lower = model.to_ascii_lowercase();
    let bytes = lower.as_bytes();

    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            let mut seen_dot = false;
            i += 1;
            while i < bytes.len() {
                if bytes[i].is_ascii_digit() {
                    i += 1;
                    continue;
                }
                if bytes[i] == b'.' && !seen_dot {
                    seen_dot = true;
                    i += 1;
                    continue;
                }
                break;
            }

            if i < bytes.len() && bytes[i] == b'b' {
                let parsed = lower[start..i].parse::<f32>().ok();
                if parsed.is_some() {
                    return parsed;
                }
            }
        } else {
            i += 1;
        }
    }

    None
}

impl Default for AppConfig {
    fn default() -> Self {
        let ollama_url = detect_ollama_base_url();
        Self {
            version: CURRENT_CONFIG_VERSION,
            servers: vec![ServerConfig {
                name: "Local Ollama".to_string(),
                provider: ProviderKind::Ollama,
                base_url: ollama_url.clone(),
                default_model: "qwen2.5-coder:3b".to_string(),
                priority: 1,
                enabled: true,
            }],
            context: ContextConfig::default(),
            performance: PerformanceConfig::default(),
            network: NetworkConfig::default(),
            ui: UiConfig::default(),
            routing: RoutingConfig::default(),
            storage: StorageConfig {
                models_dir: "~/.local/share/multilink/models".to_string(),
            },
            runtime: RuntimeConfig::default(),
            system_context_dir: None,
            preferred_provider: None,
            ollama: None,
        }
    }
}

impl AppConfig {
    pub async fn load_or_create(path: &Path) -> Result<Self, ConfigError> {
        if path.exists() {
            let content = fs::read_to_string(path).await?;
            let mut parsed = toml::from_str::<Self>(&content)?;
            let mut needs_write = parsed.migrate_to_current()?;

            if parsed.network.shared_secret.is_empty() {
                parsed.network.shared_secret = generate_lan_secret();
                needs_write = true;
                eprintln!(
                    "[multilink] LAN secret generado automaticamente.\n\n  shared_secret = \"{}\"\n\nCopia este codigo en tus otros dispositivos.\nEjecuta `/doctor --security` para verlo en cualquier momento.",
                    parsed.network.shared_secret
                );
            }

            if needs_write {
                parsed.sync_runtime_from_sections();
                let rewritten = toml::to_string_pretty(&parsed)?;
                fs::write(path, rewritten).await?;
                restrict_permissions(path)?;
            }
            parsed.sync_runtime_from_sections();
            parsed.apply_env_overrides();
            parsed.validate()?;
            return Ok(parsed);
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut default = Self::default();
        default.network.shared_secret = generate_lan_secret();
        eprintln!(
            "[multilink] Configuracion creada. LAN secret generado:\n\n  shared_secret = \"{}\"\n\nGuarda este codigo para configurar tus otros dispositivos.\nEjecuta `/doctor --security` para verlo en cualquier momento.",
            default.network.shared_secret
        );
        default.sync_runtime_from_sections();
        let content = toml::to_string_pretty(&default)?;
        fs::write(path, content).await?;
        restrict_permissions(path)?;

        let mut loaded = default;
        loaded.apply_env_overrides();
        loaded.validate()?;
        Ok(loaded)
    }

    pub fn default_user_config_path() -> PathBuf {
        if let Some(config_dir) = dirs::config_dir() {
            let folder = config_dir.join("multilink");
            let preferred = folder.join("multilink.toml");
            let legacy = folder.join("config.toml");
            if preferred.exists() {
                return preferred;
            }
            if legacy.exists() {
                return legacy;
            }
            return preferred;
        }
        PathBuf::from("./config/default.toml")
    }

    pub fn primary_server(&self) -> Option<&ServerConfig> {
        self.servers
            .iter()
            .filter(|s| s.enabled)
            .min_by_key(|s| s.priority)
    }

    pub fn apply_env_overrides(&mut self) {
        if let Ok(value) = std::env::var("MULTILINK_OLLAMA_BASE_URL") {
            if let Some(server) = self
                .servers
                .iter_mut()
                .find(|s| s.provider == ProviderKind::Ollama)
            {
                server.base_url = value;
            }
        }

        if let Ok(value) = std::env::var("MULTILINK_OLLAMA_MODEL") {
            if let Some(server) = self
                .servers
                .iter_mut()
                .find(|s| s.provider == ProviderKind::Ollama)
            {
                server.default_model = value;
            }
        }

        if let Ok(value) = std::env::var("MULTILINK_MODELS_DIR") {
            self.storage.models_dir = value;
        }

        if let Ok(value) = std::env::var("MULTILINK_CONTEXT_EMBEDDINGS") {
            self.context.embeddings_enabled = value == "1" || value.eq_ignore_ascii_case("true");
        }

        if let Ok(value) = std::env::var("MULTILINK_EMBED_MODEL") {
            self.context.embed_model = value;
        }

        if let Ok(value) = std::env::var("MULTILINK_EMBED_BASE_URL") {
            self.context.embed_base_url = Some(value);
        }

        if let Ok(value) = std::env::var("MULTILINK_EMBED_CONNECT_TIMEOUT_MS") {
            if let Ok(parsed) = value.parse::<u64>() {
                self.context.embed_connect_timeout_ms = Some(parsed);
            }
        }

        if let Ok(value) = std::env::var("MULTILINK_EMBED_REQUEST_TIMEOUT_MS") {
            if let Ok(parsed) = value.parse::<u64>() {
                self.context.embed_request_timeout_ms = Some(parsed);
            }
        }

        if let Ok(value) = std::env::var("MULTILINK_EMBED_MAX_RETRIES") {
            if let Ok(parsed) = value.parse::<u8>() {
                self.context.embed_max_retries = Some(parsed);
            }
        }

        if let Ok(value) = std::env::var("MULTILINK_EMBED_BATCH_SIZE") {
            if let Ok(parsed) = value.parse::<usize>() {
                self.context.embed_batch_size = Some(parsed);
            }
        }

        if let Ok(value) = std::env::var("MULTILINK_PROJECT_TOPK") {
            if let Ok(parsed) = value.parse::<usize>() {
                self.context.project_top_k = parsed.clamp(2, 24);
            }
        }

        if let Ok(value) = std::env::var("MULTILINK_DEBUG_CONTEXT") {
            self.context.debug = value == "1" || value.eq_ignore_ascii_case("true");
        }

        if let Ok(value) = std::env::var("MULTILINK_CONTEXT_INDEX_REFRESH") {
            self.context.index_refresh_on_query =
                value == "1" || value.eq_ignore_ascii_case("true");
        }

        if let Ok(value) = std::env::var("MULTILINK_CONTEXT_ENABLE_FILTERS") {
            self.context.retrieval_enable_filters =
                value == "1" || value.eq_ignore_ascii_case("true");
        }

        if let Ok(value) = std::env::var("MULTILINK_CONTEXT_V2PLUS_METRICS") {
            self.context.v2plus_metrics = value == "1" || value.eq_ignore_ascii_case("true");
        }

        if let Ok(value) = std::env::var("MULTILINK_CONTEXT_ENGINE") {
            self.context.engine = value;
        }

        if let Ok(value) = std::env::var("MULTILINK_SYSTEM_CONTEXT_DIR") {
            self.system_context_dir = Some(PathBuf::from(value));
        }

        if let Ok(value) = std::env::var("MULTILINK_REMOTE_THRESHOLD") {
            self.routing.remote_threshold = parse_remote_threshold(&value);
        }

        self.sync_runtime_from_sections();
    }

    fn sync_runtime_from_sections(&mut self) {
        self.runtime.context_embeddings_enabled = self.context.embeddings_enabled;
        self.runtime.context_debug = self.context.debug;
        self.runtime.context_embed_model = self.context.embed_model.clone();
        self.runtime.embed_connect_timeout_ms = self
            .context
            .embed_connect_timeout_ms
            .unwrap_or(2_000)
            .clamp(200, 60_000);
        self.runtime.embed_request_timeout_ms = self
            .context
            .embed_request_timeout_ms
            .unwrap_or(12_000)
            .clamp(500, 120_000);
        self.runtime.embed_max_retries = self.context.embed_max_retries.unwrap_or(1).clamp(0, 5);
        self.runtime.embed_batch_size = self.context.embed_batch_size.unwrap_or(24).clamp(1, 128);
        self.runtime.context_project_top_k = self.context.project_top_k.clamp(2, 24);
        self.runtime.max_project_context_tokens = self.context.max_project_tokens.max(512);
        self.runtime.max_parallel_streams = self.performance.max_parallel_streams.max(1);
        self.runtime.observability_json_logs = self.ui.json_logs;
        self.runtime.remote_threshold = self.routing.remote_threshold;
        self.runtime.context_engine = self.context.engine.clone();
        self.runtime.context_index_refresh_on_query = self.context.index_refresh_on_query;
        self.runtime.context_retrieval_enable_filters = self.context.retrieval_enable_filters;
        self.runtime.context_v2plus_metrics = self.context.v2plus_metrics;
        self.runtime.network_allow_remote = self.network.allow_remote;
        self.runtime.network_shared_secret = self.network.shared_secret.clone();
        self.runtime.network_allowed_ips = self.network.allowed_ips.clone();

        // Chat URL - viene del primary server
        if let Some(server) = self.primary_server() {
            self.runtime.context_ollama_base_url = server.base_url.clone();
        }

        // Embed URL - si está configurado explícitamente, usarlo;
        // si no, fallback al primary server (para backward compatibility)
        self.runtime.embed_base_url = self
            .context
            .embed_base_url
            .clone()
            .or_else(|| self.primary_server().map(|s| s.base_url.clone()))
            .unwrap_or_else(detect_ollama_base_url);

        let mut execution_servers: Vec<ExecutionServerRuntime> = self
            .servers
            .iter()
            .filter(|s| s.enabled && s.provider == ProviderKind::Ollama)
            .map(|s| ExecutionServerRuntime {
                name: s.name.clone(),
                base_url: s.base_url.clone(),
                default_model: s.default_model.clone(),
                priority: s.priority,
                enabled: s.enabled,
                max_concurrency: 1,
            })
            .collect();
        execution_servers.sort_by_key(|s| s.priority);
        self.runtime.execution_servers = execution_servers;
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.version != CURRENT_CONFIG_VERSION {
            return Err(ConfigError::Invalid(format!(
                "unsupported config version; expected {}",
                CURRENT_CONFIG_VERSION
            )));
        }

        if self.servers.is_empty() {
            return Err(ConfigError::Invalid(
                "at least one server is required".to_string(),
            ));
        }

        if !self.servers.iter().any(|s| s.enabled) {
            return Err(ConfigError::Invalid(
                "at least one server must be enabled".to_string(),
            ));
        }

        for server in &self.servers {
            if server.name.trim().is_empty() {
                return Err(ConfigError::Invalid(
                    "server.name cannot be empty".to_string(),
                ));
            }
            if !server.base_url.starts_with("http://") && !server.base_url.starts_with("https://") {
                return Err(ConfigError::Invalid(format!(
                    "server '{}' base_url must start with http:// or https://",
                    server.name
                )));
            }
            if server.default_model.trim().is_empty() {
                return Err(ConfigError::Invalid(format!(
                    "server '{}' default_model cannot be empty",
                    server.name
                )));
            }
        }

        if let Some(url) = self.context.embed_base_url.as_ref() {
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(ConfigError::Invalid(
                    "context.embed_base_url must start with http:// or https://".to_string(),
                ));
            }
        }

        if !(2..=24).contains(&self.context.project_top_k) {
            return Err(ConfigError::Invalid(
                "context.project_top_k must be in range 2..=24".to_string(),
            ));
        }

        let engine = self.context.engine.trim().to_ascii_lowercase();
        if !matches!(engine.as_str(), "v1" | "v2" | "v2plus") {
            return Err(ConfigError::Invalid(
                "context.engine must be one of: v1, v2, v2plus".to_string(),
            ));
        }

        if self.performance.max_parallel_streams == 0 {
            return Err(ConfigError::Invalid(
                "performance.max_parallel_streams must be greater than zero".to_string(),
            ));
        }

        Ok(())
    }

    fn migrate_to_current(&mut self) -> Result<bool, ConfigError> {
        if self.version > CURRENT_CONFIG_VERSION {
            return Err(ConfigError::Invalid(format!(
                "config version {} is newer than supported {}",
                self.version, CURRENT_CONFIG_VERSION
            )));
        }

        let mut changed = false;
        if self.version < 2 {
            let legacy_ollama = self.ollama.clone().unwrap_or(LegacyOllamaConfig {
                base_url: "http://127.0.0.1:11434".to_string(),
                default_model: "qwen2.5-coder:3b".to_string(),
            });

            if self.servers.is_empty() {
                self.servers = vec![ServerConfig {
                    name: "Local Ollama".to_string(),
                    provider: ProviderKind::Ollama,
                    base_url: legacy_ollama.base_url,
                    default_model: legacy_ollama.default_model,
                    priority: 1,
                    enabled: true,
                }];
            }

            self.version = 2;
            self.preferred_provider = None;
            self.ollama = None;
            changed = true;
        }

        Ok(changed)
    }
}

impl RemoteThreshold {
    pub fn allows_remote(self, weight: TaskWeight) -> bool {
        let threshold = match self {
            RemoteThreshold::Auto => TaskWeight::Heavy,
            RemoteThreshold::Light => TaskWeight::Light,
            RemoteThreshold::Medium => TaskWeight::Medium,
            RemoteThreshold::Heavy => TaskWeight::Heavy,
            RemoteThreshold::Critical => TaskWeight::Critical,
        };
        task_weight_rank(weight) >= task_weight_rank(threshold)
    }
}

pub fn task_weight_rank(weight: TaskWeight) -> u8 {
    match weight {
        TaskWeight::Trivial => 0,
        TaskWeight::Light => 1,
        TaskWeight::Medium => 2,
        TaskWeight::Heavy => 3,
        TaskWeight::Critical => 4,
    }
}

fn parse_remote_threshold(value: &str) -> RemoteThreshold {
    match value.trim().to_ascii_lowercase().as_str() {
        "light" => RemoteThreshold::Light,
        "medium" => RemoteThreshold::Medium,
        "heavy" => RemoteThreshold::Heavy,
        "critical" => RemoteThreshold::Critical,
        "auto" => RemoteThreshold::Auto,
        _ => RemoteThreshold::Auto,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_remote_threshold, RemoteThreshold, TaskWeight};

    #[test]
    fn remote_threshold_parser_accepts_known_values() {
        assert_eq!(parse_remote_threshold("light"), RemoteThreshold::Light);
        assert_eq!(parse_remote_threshold("Medium"), RemoteThreshold::Medium);
        assert_eq!(parse_remote_threshold("HEAVY"), RemoteThreshold::Heavy);
        assert_eq!(
            parse_remote_threshold("critical"),
            RemoteThreshold::Critical
        );
        assert_eq!(parse_remote_threshold(" auto "), RemoteThreshold::Auto);
    }

    #[test]
    fn remote_threshold_parser_defaults_to_auto_for_unknown_values() {
        assert_eq!(parse_remote_threshold("invalid"), RemoteThreshold::Auto);
        assert_eq!(parse_remote_threshold(""), RemoteThreshold::Auto);
    }

    #[test]
    fn remote_threshold_auto_only_allows_heavy_or_more() {
        assert!(!RemoteThreshold::Auto.allows_remote(TaskWeight::Light));
        assert!(!RemoteThreshold::Auto.allows_remote(TaskWeight::Medium));
        assert!(RemoteThreshold::Auto.allows_remote(TaskWeight::Heavy));
        assert!(RemoteThreshold::Auto.allows_remote(TaskWeight::Critical));
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

pub fn generate_lan_secret() -> String {
    use sha2::{Digest, Sha256};
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut bytes = [0u8; 32];

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let heap_addr = Box::into_raw(Box::new(0u8)) as u64;
    let pid = std::process::id() as u64;

    let mut hasher = Sha256::new();
    hasher.update(ts.to_le_bytes());
    hasher.update(heap_addr.to_le_bytes());
    hasher.update(pid.to_le_bytes());
    let intermediate = hasher.finalize();

    let mut hasher2 = Sha256::new();
    hasher2.update(intermediate);
    hasher2.update(heap_addr.wrapping_add(pid).to_le_bytes());
    let result = hasher2.finalize();
    bytes.copy_from_slice(&result[..32]);

    hex::encode(bytes)
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml deserialize error: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("toml serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("invalid config: {0}")]
    Invalid(String),
}
