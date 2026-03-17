use std::sync::Arc;
use std::time::Duration;

use multilink_core::providers::ollama::OllamaProvider;
use multilink_core::{AppConfig, ChatRuntime, ProviderId, ProviderRouter, StreamEvent};

const PERSIST_INTERVAL: Duration = Duration::from_secs(2);

pub trait ChatCallbacks: Send + Sync {
    fn on_stream_started(&self);
    fn on_token_received(&self, token: &str);
    fn on_stream_finished(&self);
    fn on_error(&self, message: &str);
    fn on_loading_changed(&self, loading: bool);
}

pub struct ChatBridge {
    pub active_provider: String,
    pub active_model: String,
    pub provider_scope: String,
    chat_runtime: Arc<ChatRuntime>,
    active_session_id: String,
    runtime: Option<tokio::runtime::Runtime>,
}

impl Default for ChatBridge {
    fn default() -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .ok();

        let (selected_server, default_model) = if let Some(rt) = runtime.as_ref() {
            let config_path = AppConfig::default_user_config_path();
            let config = rt.block_on(async { AppConfig::load_or_create(&config_path).await.unwrap_or_default() });
            let server = config.primary_server().cloned().unwrap_or_default();
            let model = server.default_model.clone();
            (server, model)
        } else {
            let server = multilink_core::config::ServerConfig {
                name: "Local Ollama".to_string(),
                provider: multilink_core::config::ProviderKind::Ollama,
                base_url: "http://127.0.0.1:11434".to_string(),
                default_model: "qwen2.5-coder:3b".to_string(),
                priority: 1,
                enabled: true,
            };
            let model = server.default_model.clone();
            (server, model)
        };

        let mut router = ProviderRouter::new();
        router.register(Arc::new(OllamaProvider::new(
            selected_server.base_url.clone(),
            default_model.clone(),
        )));

        let chat_runtime = if let Some(rt) = runtime.as_ref() {
            rt.block_on(async {
                let chat_runtime = Arc::new(
                    ChatRuntime::new_portable(Arc::new(router), PERSIST_INTERVAL)
                        .unwrap_or_else(|_| {
                            ChatRuntime::new(
                                Arc::new(ProviderRouter::new()),
                                std::path::PathBuf::from("./.multilink/sessions"),
                                PERSIST_INTERVAL,
                            )
                        }),
                );
                let _ = chat_runtime.load_sessions_from_disk().await;
                chat_runtime
            })
        } else {
            Arc::new(ChatRuntime::new(
                Arc::new(ProviderRouter::new()),
                std::path::PathBuf::from("./.multilink/sessions"),
                PERSIST_INTERVAL,
            ))
        };

        let active_session_id = if let Some(rt) = runtime.as_ref() {
            rt.block_on(chat_runtime.create_session(ProviderId::Ollama, Some(default_model.clone())))
        } else {
            "session-unavailable".to_string()
        };

        Self {
            active_provider: "Ollama".to_string(),
            active_model: default_model,
            provider_scope: "LOCAL".to_string(),
            chat_runtime,
            active_session_id,
            runtime,
        }
    }
}

impl ChatBridge {
    pub fn set_provider(&mut self, provider: &str) {
        self.active_provider = provider.to_string();
        self.provider_scope = if provider.eq_ignore_ascii_case("ollama") {
            "LOCAL".to_string()
        } else {
            "REMOTE".to_string()
        };
    }

    pub fn set_model(&mut self, model: &str) {
        self.active_model = model.to_string();
    }

    pub fn new_session(&mut self) {
        let Some(runtime) = self.runtime.as_ref() else {
            return;
        };

        let provider = parse_provider(&self.active_provider);
        let model = Some(self.active_model.clone());
        self.active_session_id = runtime.block_on(self.chat_runtime.create_session(provider, model));
    }

    pub fn stop_generation(&self) {
        let Some(runtime) = self.runtime.as_ref() else {
            return;
        };
        let _ = runtime.block_on(self.chat_runtime.cancel_stream(&self.active_session_id));
    }

    pub fn select_session(&mut self, session_id: &str) {
        let Some(runtime) = self.runtime.as_ref() else {
            return;
        };

        if runtime
            .block_on(self.chat_runtime.select_session(session_id))
            .is_ok()
        {
            self.active_session_id = session_id.to_string();
        }
    }

    pub fn send_prompt(&self, prompt: String, callbacks: Arc<dyn ChatCallbacks>) {
        if prompt.trim().is_empty() {
            return;
        }

        let Some(runtime) = self.runtime.as_ref() else {
            callbacks.on_error("Runtime not available");
            return;
        };

        callbacks.on_loading_changed(true);
        let chat_runtime = self.chat_runtime.clone();
        let session_id = self.active_session_id.clone();

        runtime.spawn(async move {
            match chat_runtime.send_message(&session_id, prompt).await {
                Ok(mut receiver) => {
                    while let Some(event) = receiver.recv().await {
                        match event {
                            StreamEvent::Started => callbacks.on_stream_started(),
                            StreamEvent::Chunk(chunk) => callbacks.on_token_received(&chunk),
                            StreamEvent::Finished => {
                                callbacks.on_stream_finished();
                                callbacks.on_loading_changed(false);
                            }
                            StreamEvent::Error(message) => {
                                callbacks.on_error(&message);
                                callbacks.on_loading_changed(false);
                            }
                        }
                    }
                }
                Err(err) => {
                    callbacks.on_error(&err.to_string());
                    callbacks.on_loading_changed(false);
                }
            }
        });
    }
}

fn parse_provider(provider: &str) -> ProviderId {
    match provider.to_ascii_lowercase().as_str() {
        "gemini" => ProviderId::Gemini,
        "codex" => ProviderId::Codex,
        _ => ProviderId::Ollama,
    }
}
