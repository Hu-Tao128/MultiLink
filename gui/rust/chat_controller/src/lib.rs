use std::ffi::{c_char, c_void, CStr, CString};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use multilink_core::config::ServerConfig;
use multilink_core::providers::ollama::OllamaProvider;
use multilink_core::{
    AppConfig, ChatRuntime, ProviderId, ProviderKind, ProviderRouter, StreamEvent,
};
use serde_json::json;
use tokio::runtime::Runtime;

const PERSIST_INTERVAL: Duration = Duration::from_secs(2);

type SessionCallback = extern "C" fn(*mut c_void, *const c_char);
type SessionStringCallback = extern "C" fn(*mut c_void, *const c_char, *const c_char);
type SessionUsageCallback = extern "C" fn(*mut c_void, *const c_char, usize, usize, usize, bool);
type StringCallback = extern "C" fn(*mut c_void, *const c_char);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BackendCallbacks {
    pub on_stream_started: Option<SessionCallback>,
    pub on_stream_chunk: Option<SessionStringCallback>,
    pub on_stream_finished: Option<SessionCallback>,
    pub on_stream_error: Option<SessionStringCallback>,
    pub on_token_usage: Option<SessionUsageCallback>,
    pub on_sessions_updated: Option<StringCallback>,
    pub on_models_updated: Option<StringCallback>,
    pub on_messages_updated: Option<StringCallback>,
}

#[derive(Clone)]
struct UiState {
    active_provider: String,
    active_model: String,
    provider_scope: String,
    provider_health: String,
    is_loading: bool,
    startup_notice: String,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            active_provider: "Ollama".to_string(),
            active_model: "".to_string(),
            provider_scope: "LOCAL".to_string(),
            provider_health: "unavailable".to_string(),
            is_loading: false,
            startup_notice: String::new(),
        }
    }
}

#[derive(serde::Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelInfo>,
}

#[derive(serde::Deserialize)]
struct OllamaModelInfo {
    name: String,
    size: Option<u64>,
}

#[derive(serde::Serialize)]
struct ServerTestResult {
    ok: bool,
    model_count: usize,
    models: Vec<String>,
    error: String,
    hint: String,
}

pub struct BackendHandle {
    runtime: Runtime,
    chat_runtime: Arc<ChatRuntime>,
    callbacks: BackendCallbacks,
    callback_ctx: usize,
    active_session_id: Arc<Mutex<String>>,
    ui_state: Arc<Mutex<UiState>>,
    ollama_base_url: String,
    config_path: PathBuf,
}

#[unsafe(no_mangle)]
pub extern "C" fn chat_backend_create(
    callbacks: BackendCallbacks,
    callback_ctx: *mut c_void,
) -> *mut BackendHandle {
    let runtime = match Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return std::ptr::null_mut(),
    };

    let config_path = AppConfig::default_user_config_path();
    let (config, startup_notice) = load_config_with_recovery(&runtime, &config_path);

    let mut selected_server = config
        .primary_server()
        .cloned()
        .unwrap_or_else(|| multilink_core::config::ServerConfig {
            name: "Local Ollama".to_string(),
            provider: ProviderKind::Ollama,
            base_url: "http://127.0.0.1:11434".to_string(),
            default_model: "qwen2.5-coder:3b".to_string(),
            priority: 1,
            enabled: true,
        });
    selected_server.base_url = normalize_base_url(&selected_server.base_url);

    let mut router = ProviderRouter::new();
    router.register(Arc::new(OllamaProvider::new(
        selected_server.base_url.clone(),
        selected_server.default_model.clone(),
    )));

    let chat_runtime = Arc::new(
        ChatRuntime::new_portable_with_settings(
            Arc::new(router),
            PERSIST_INTERVAL,
            config.runtime.clone(),
            config.system_context_dir.clone(),
        )
        .unwrap_or_else(|_| {
            ChatRuntime::new(
                Arc::new(ProviderRouter::new()),
                PathBuf::from("./.multilink/sessions"),
                PERSIST_INTERVAL,
            )
        }),
    );

    let _ = runtime.block_on(chat_runtime.load_sessions_from_disk());
    let active_session_id = runtime.block_on(async {
        if let Some(existing_active) = chat_runtime.active_session().await {
            return existing_active;
        }

        let sessions = chat_runtime.list_sessions().await;
        if let Some(first) = sessions.first() {
            return first.id.clone();
        }

        chat_runtime
            .create_session(ProviderId::Ollama, Some(selected_server.default_model.clone()))
            .await
    });

    let active_model = runtime.block_on(async {
        let sessions = chat_runtime.list_sessions().await;
        if let Some(session) = sessions.iter().find(|s| s.id == active_session_id) {
            let model = session
                .model
                .clone()
                .unwrap_or_else(|| selected_server.default_model.clone());
            if is_embedding_like_model(&model) {
                let fallback = selected_server.default_model.clone();
                let _ = chat_runtime
                    .update_session_model(&active_session_id, Some(fallback.clone()))
                    .await;
                return fallback;
            }
            return model;
        }
        selected_server.default_model.clone()
    });

    let ui = UiState {
        active_model,
        startup_notice,
        ..UiState::default()
    };

    Box::into_raw(Box::new(BackendHandle {
        runtime,
        chat_runtime,
        callbacks,
        callback_ctx: callback_ctx as usize,
        active_session_id: Arc::new(Mutex::new(active_session_id)),
        ui_state: Arc::new(Mutex::new(ui)),
        ollama_base_url: selected_server.base_url,
        config_path,
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_destroy(handle: *mut BackendHandle) {
    if handle.is_null() {
        return;
    }
    // SAFETY: pointer originates from Box::into_raw in chat_backend_create
    let _ = unsafe { Box::from_raw(handle) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_send_prompt(handle: *mut BackendHandle, text: *const c_char) {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return;
    };
    let Some(text_cstr) = (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) }) else {
        return;
    };
    let prompt = text_cstr.to_string_lossy().trim().to_string();
    if prompt.is_empty() {
        return;
    }

    if let Ok(mut ui) = backend.ui_state.lock() {
        ui.is_loading = true;
        ui.provider_health = "starting".to_string();
    }

    let active_session = backend
        .active_session_id
        .lock()
        .map(|v| v.clone())
        .unwrap_or_default();
    if active_session.is_empty() {
        return;
    }
    spawn_send_prompt(
        backend.runtime.handle().clone(),
        backend.chat_runtime.clone(),
        backend.callbacks,
        backend.callback_ctx,
        backend.ui_state.clone(),
        active_session,
        prompt,
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_send_prompt_for_session(
    handle: *mut BackendHandle,
    session_id: *const c_char,
    text: *const c_char,
) {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return;
    };
    let Some(session_cstr) = (!session_id.is_null()).then(|| unsafe { CStr::from_ptr(session_id) })
    else {
        return;
    };
    let Some(text_cstr) = (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) }) else {
        return;
    };

    let target_session = session_cstr.to_string_lossy().to_string();
    let prompt = text_cstr.to_string_lossy().trim().to_string();
    if target_session.is_empty() || prompt.is_empty() {
        return;
    }

    if let Ok(mut ui) = backend.ui_state.lock() {
        ui.is_loading = true;
        ui.provider_health = "starting".to_string();
    }

    if let Ok(mut active) = backend.active_session_id.lock() {
        *active = target_session.clone();
    }

    spawn_send_prompt(
        backend.runtime.handle().clone(),
        backend.chat_runtime.clone(),
        backend.callbacks,
        backend.callback_ctx,
        backend.ui_state.clone(),
        target_session,
        prompt,
    );
}

fn spawn_send_prompt(
    runtime: tokio::runtime::Handle,
    chat_runtime: Arc<ChatRuntime>,
    callbacks: BackendCallbacks,
    ctx: usize,
    ui_state: Arc<Mutex<UiState>>,
    stream_session: String,
    prompt: String,
) {
    runtime.spawn(async move {
        match chat_runtime.send_message(&stream_session, prompt).await {
            Ok(mut rx) => {
                while let Some(event) = rx.recv().await {
                    match event {
                        StreamEvent::Started => {
                            if let Ok(mut ui) = ui_state.lock() {
                                ui.provider_health = "available".to_string();
                            }
                            emit_session(
                                callbacks.on_stream_started,
                                ctx as *mut c_void,
                                &stream_session,
                            );
                        }
                        StreamEvent::Chunk(chunk) => {
                            emit_session_string(
                                callbacks.on_stream_chunk,
                                ctx as *mut c_void,
                                &stream_session,
                                &chunk,
                            );
                        }
                        StreamEvent::Usage { prompt_tokens, completion_tokens, total_tokens, is_estimated } => {
                            if let Some(callback) = callbacks.on_token_usage {
                                let session_cstr = CString::new(stream_session.clone()).unwrap();
                                callback(
                                    ctx as *mut c_void,
                                    session_cstr.as_ptr(),
                                    prompt_tokens,
                                    completion_tokens,
                                    total_tokens,
                                    is_estimated,
                                );
                            }
                        }
                        StreamEvent::Finished => {
                            if let Ok(mut ui) = ui_state.lock() {
                                ui.is_loading = false;
                            }
                            emit_session(
                                callbacks.on_stream_finished,
                                ctx as *mut c_void,
                                &stream_session,
                            );
                        }
                        StreamEvent::Error(message) => {
                            if let Ok(mut ui) = ui_state.lock() {
                                ui.is_loading = false;
                                ui.provider_health = if is_provider_connection_error(&message) {
                                    "unavailable".to_string()
                                } else {
                                    "available".to_string()
                                };
                            }
                            emit_session_string(
                                callbacks.on_stream_error,
                                ctx as *mut c_void,
                                &stream_session,
                                &message,
                            );
                        }
                    }
                }
            }
            Err(err) => {
                let message = err.to_string();
                if let Ok(mut ui) = ui_state.lock() {
                    ui.is_loading = false;
                    ui.provider_health = if is_provider_connection_error(&message) {
                        "unavailable".to_string()
                    } else {
                        "available".to_string()
                    };
                }
                emit_session_string(
                    callbacks.on_stream_error,
                    ctx as *mut c_void,
                    &stream_session,
                    &message,
                );
            }
        }
    });
}

fn is_provider_connection_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("connection refused")
        || lower.contains("failed to connect")
        || lower.contains("dns")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("socket")
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_stop_generation(handle: *mut BackendHandle) {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return;
    };
    let active_session = backend
        .active_session_id
        .lock()
        .map(|v| v.clone())
        .unwrap_or_default();
    let runtime = backend.runtime.handle().clone();
    let chat_runtime = backend.chat_runtime.clone();
    runtime.spawn(async move {
        let _ = chat_runtime.cancel_stream(&active_session).await;
    });
    if let Ok(mut ui) = backend.ui_state.lock() {
        ui.is_loading = false;
        ui.provider_health = "available".to_string();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_new_session(handle: *mut BackendHandle) -> *mut c_char {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let model = backend
        .ui_state
        .lock()
        .map(|s| s.active_model.clone())
        .unwrap_or_else(|_| "".to_string());
    let selected_model = if model.trim().is_empty() {
        Some("llama3.2".to_string())
    } else {
        Some(model)
    };

    let id = backend
        .runtime
        .block_on(backend.chat_runtime.create_session(ProviderId::Ollama, selected_model));

    if let Ok(mut active) = backend.active_session_id.lock() {
        *active = id.clone();
    }

    let payload = backend
        .runtime
        .block_on(async { build_sessions_json(&backend.chat_runtime).await });
    emit_string(
        backend.callbacks.on_sessions_updated,
        backend.callback_ctx as *mut c_void,
        &payload,
    );

    let messages_payload = backend
        .runtime
        .block_on(async { build_messages_json(&backend.chat_runtime, &id).await });
    emit_string(
        backend.callbacks.on_messages_updated,
        backend.callback_ctx as *mut c_void,
        &messages_payload,
    );

    into_c_string(id)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_select_session(
    handle: *mut BackendHandle,
    session_id: *const c_char,
) {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return;
    };
    let Some(id_cstr) = (!session_id.is_null()).then(|| unsafe { CStr::from_ptr(session_id) })
    else {
        return;
    };
    let id = id_cstr.to_string_lossy().to_string();
    let fallback_model = backend
        .ui_state
        .lock()
        .ok()
        .map(|ui| ui.active_model.clone())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "llama3.2".to_string());
    let runtime = backend.runtime.handle().clone();
    let chat_runtime = backend.chat_runtime.clone();
    let active_session_id = backend.active_session_id.clone();

    runtime.spawn(async move {
        if chat_runtime.select_session(&id).await.is_ok() {
            let sessions = chat_runtime.list_sessions().await;
            if let Some(session) = sessions.iter().find(|s| s.id == id) {
                if let Some(model) = session.model.as_ref() {
                    if is_embedding_like_model(model) {
                        let _ = chat_runtime
                            .update_session_model(&id, Some(fallback_model.clone()))
                            .await;
                    }
                }
            }
            if let Ok(mut active) = active_session_id.lock() {
                *active = id;
            }
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_select_model(
    handle: *mut BackendHandle,
    model: *const c_char,
) {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return;
    };
    let Some(model_cstr) = (!model.is_null()).then(|| unsafe { CStr::from_ptr(model) }) else {
        return;
    };
    let value = model_cstr.to_string_lossy().to_string();
    if is_embedding_like_model(&value) {
        return;
    }
    let active_session = backend
        .active_session_id
        .lock()
        .map(|v| v.clone())
        .unwrap_or_default();

    let runtime = backend.runtime.handle().clone();
    let chat_runtime = backend.chat_runtime.clone();
    let value_for_task = value.clone();
    runtime.spawn(async move {
        let _ = chat_runtime
            .update_session_model(&active_session, Some(value_for_task))
            .await;
    });

    if let Ok(mut ui) = backend.ui_state.lock() {
        ui.active_model = value;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_set_session_project_root(
    handle: *mut BackendHandle,
    session_id: *const c_char,
    project_root: *const c_char,
) {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return;
    };
    let Some(id_cstr) = (!session_id.is_null()).then(|| unsafe { CStr::from_ptr(session_id) })
    else {
        return;
    };
    let session_id_value = id_cstr.to_string_lossy().to_string();
    if session_id_value.is_empty() {
        return;
    }

    let project_root_value = if project_root.is_null() {
        None
    } else {
        let root_cstr = unsafe { CStr::from_ptr(project_root) };
        let root = root_cstr.to_string_lossy().trim().to_string();
        if root.is_empty() {
            None
        } else {
            Some(root)
        }
    };

    let runtime = backend.runtime.handle().clone();
    let chat_runtime = backend.chat_runtime.clone();
    let callbacks = backend.callbacks;
    let ctx = backend.callback_ctx;
    runtime.spawn(async move {
        let _ = chat_runtime
            .set_session_project_root(&session_id_value, project_root_value)
            .await;
        let payload = build_sessions_json(&chat_runtime).await;
        emit_string(callbacks.on_sessions_updated, ctx as *mut c_void, &payload);
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_sessions_json(handle: *mut BackendHandle) -> *mut c_char {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return std::ptr::null_mut();
    };

    let payload = backend
        .runtime
        .block_on(async { build_sessions_json(&backend.chat_runtime).await });
    into_c_string(payload)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_models_json(handle: *mut BackendHandle) -> *mut c_char {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return into_c_string("[]".to_string());
    };

    let payload = backend
        .runtime
        .block_on(async {
            let base_url = resolve_active_base_url_from_path(
                &backend.config_path,
                &backend.ollama_base_url,
            );
            build_models_json(&backend.config_path, &base_url).await
        });
    into_c_string(payload)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_servers_config_json(
    handle: *mut BackendHandle,
) -> *mut c_char {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return into_c_string("[]".to_string());
    };

    let payload = backend.runtime.block_on(async {
        match AppConfig::load_or_create(&backend.config_path).await {
            Ok(cfg) => serde_json::to_string(&cfg.servers).unwrap_or_else(|_| "[]".to_string()),
            Err(_) => "[]".to_string(),
        }
    });

    into_c_string(payload)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_save_servers_config_json(
    handle: *mut BackendHandle,
    servers_json: *const c_char,
) -> bool {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return false;
    };
    let Some(raw) = (!servers_json.is_null()).then(|| unsafe { CStr::from_ptr(servers_json) })
    else {
        return false;
    };
    let payload = raw.to_string_lossy().to_string();
    let parsed: Vec<ServerConfig> = match serde_json::from_str(&payload) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let mut parsed = parsed;
    for server in &mut parsed {
        server.base_url = normalize_base_url(&server.base_url);
    }

    backend.runtime.block_on(async {
        let mut cfg = match AppConfig::load_or_create(&backend.config_path).await {
            Ok(v) => v,
            Err(_) => return false,
        };
        cfg.servers = parsed;
        let content = match toml::to_string_pretty(&cfg) {
            Ok(v) => v,
            Err(_) => return false,
        };
        std::fs::write(&backend.config_path, content).is_ok()
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_test_server_connection(
    handle: *mut BackendHandle,
    base_url: *const c_char,
) -> *mut c_char {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return into_c_string("{\"ok\":false,\"error\":\"backend unavailable\"}".to_string());
    };
    let Some(raw) = (!base_url.is_null()).then(|| unsafe { CStr::from_ptr(base_url) }) else {
        return into_c_string("{\"ok\":false,\"error\":\"missing base_url\"}".to_string());
    };
    let url = normalize_base_url(raw.to_string_lossy().as_ref());
    if url.is_empty() {
        return into_c_string("{\"ok\":false,\"error\":\"empty base_url\"}".to_string());
    }
    if base_url_is_wildcard(&url) {
        let result = ServerTestResult {
            ok: false,
            model_count: 0,
            models: Vec::new(),
            error: "0.0.0.0 no es una direccion de destino valida para cliente".to_string(),
            hint: "Usa la IP real del servidor (ej. 192.168.x.x o 100.x.x.x) o http://127.0.0.1:11434 si es esta misma maquina.".to_string(),
        };
        return into_c_string(
            serde_json::to_string(&result)
                .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"serialization failed\"}".to_string()),
        );
    }

    let result = backend.runtime.block_on(async move {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10))
            .build();
        let Ok(client) = client else {
            return ServerTestResult {
                ok: false,
                model_count: 0,
                models: Vec::new(),
                error: "failed to create HTTP client".to_string(),
                hint: String::new(),
            };
        };

        let endpoint = format!("{}/api/tags", url);
        let response = match client.get(endpoint).send().await {
            Ok(v) => v,
            Err(err) => {
                let err_text = err.to_string();
                return ServerTestResult {
                    ok: false,
                    model_count: 0,
                    models: Vec::new(),
                    error: err_text.clone(),
                    hint: connection_hint_for_error(&url, &err_text),
                }
            }
        };

        if !response.status().is_success() {
            return ServerTestResult {
                ok: false,
                model_count: 0,
                models: Vec::new(),
                error: format!("http status {}", response.status()),
                hint: String::new(),
            };
        }

        let tags = match response.json::<OllamaTagsResponse>().await {
            Ok(v) => v,
            Err(err) => {
                return ServerTestResult {
                    ok: false,
                    model_count: 0,
                    models: Vec::new(),
                    error: format!("invalid response: {}", err),
                    hint: String::new(),
                }
            }
        };

        let models: Vec<String> = tags.models.into_iter().map(|m| m.name).collect();
        ServerTestResult {
            ok: true,
            model_count: models.len(),
            models,
            error: String::new(),
            hint: String::new(),
        }
    });

    into_c_string(serde_json::to_string(&result).unwrap_or_else(|_| "{\"ok\":false,\"error\":\"serialization failed\"}".to_string()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_request_sessions(handle: *mut BackendHandle) {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return;
    };

    let runtime = backend.runtime.handle().clone();
    let chat_runtime = backend.chat_runtime.clone();
    let callbacks = backend.callbacks;
    let ctx = backend.callback_ctx;

    runtime.spawn(async move {
        let payload = build_sessions_json(&chat_runtime).await;
        emit_string(callbacks.on_sessions_updated, ctx as *mut c_void, &payload);
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_request_models(handle: *mut BackendHandle) {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return;
    };

    let runtime = backend.runtime.handle().clone();
    let callbacks = backend.callbacks;
    let ctx = backend.callback_ctx;
    let config_path = backend.config_path.clone();
    let fallback_base_url = backend.ollama_base_url.clone();

    runtime.spawn(async move {
        let base_url = resolve_active_base_url_from_path(&config_path, &fallback_base_url);
        let payload = build_models_json(&config_path, &base_url).await;
        emit_string(callbacks.on_models_updated, ctx as *mut c_void, &payload);
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_request_messages(
    handle: *mut BackendHandle,
    session_id: *const c_char,
) {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return;
    };
    let Some(id_cstr) = (!session_id.is_null()).then(|| unsafe { CStr::from_ptr(session_id) })
    else {
        return;
    };
    let id = id_cstr.to_string_lossy().to_string();

    let runtime = backend.runtime.handle().clone();
    let chat_runtime = backend.chat_runtime.clone();
    let callbacks = backend.callbacks;
    let ctx = backend.callback_ctx;

    runtime.spawn(async move {
        let payload = build_messages_json(&chat_runtime, &id).await;
        emit_string(callbacks.on_messages_updated, ctx as *mut c_void, &payload);
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_get_active_provider(
    handle: *mut BackendHandle,
) -> *mut c_char {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let value = backend
        .ui_state
        .lock()
        .map(|s| s.active_provider.clone())
        .unwrap_or_else(|_| "Ollama".to_string());
    into_c_string(value)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_get_active_model(handle: *mut BackendHandle) -> *mut c_char {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let value = backend
        .ui_state
        .lock()
        .map(|s| s.active_model.clone())
        .unwrap_or_default();
    into_c_string(value)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_get_provider_scope(
    handle: *mut BackendHandle,
) -> *mut c_char {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let value = backend
        .ui_state
        .lock()
        .map(|s| s.provider_scope.clone())
        .unwrap_or_else(|_| "LOCAL".to_string());
    into_c_string(value)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_get_provider_health(
    handle: *mut BackendHandle,
) -> *mut c_char {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let value = backend
        .ui_state
        .lock()
        .map(|s| s.provider_health.clone())
        .unwrap_or_else(|_| "unavailable".to_string());
    into_c_string(value)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_get_is_loading(handle: *mut BackendHandle) -> bool {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return false;
    };
    backend
        .ui_state
        .lock()
        .map(|s| s.is_loading)
        .unwrap_or(false)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_get_startup_notice(
    handle: *mut BackendHandle,
) -> *mut c_char {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let value = backend
        .ui_state
        .lock()
        .map(|s| s.startup_notice.clone())
        .unwrap_or_default();
    into_c_string(value)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_clear_startup_notice(handle: *mut BackendHandle) {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return;
    };
    if let Ok(mut state) = backend.ui_state.lock() {
        state.startup_notice.clear();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_delete_empty_sessions(handle: *mut BackendHandle) {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return;
    };
    let _ = backend
        .runtime
        .block_on(backend.chat_runtime.delete_empty_sessions());
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_delete_session(
    handle: *mut BackendHandle,
    session_id: *const c_char,
) -> bool {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return false;
    };
    let Some(id_cstr) = (!session_id.is_null()).then(|| unsafe { CStr::from_ptr(session_id) })
    else {
        return false;
    };
    let id = id_cstr.to_string_lossy().trim().to_string();
    if id.is_empty() {
        return false;
    }

    let deleted = backend
        .runtime
        .block_on(backend.chat_runtime.delete_session(&id))
        .is_ok();
    if !deleted {
        return false;
    }

    let next_active = backend
        .runtime
        .block_on(async { backend.chat_runtime.active_session().await })
        .unwrap_or_default();

    if let Ok(mut active) = backend.active_session_id.lock() {
        *active = next_active.clone();
    }

    let payload = backend
        .runtime
        .block_on(async { build_sessions_json(&backend.chat_runtime).await });
    emit_string(
        backend.callbacks.on_sessions_updated,
        backend.callback_ctx as *mut c_void,
        &payload,
    );

    let messages_payload = if next_active.is_empty() {
        "{\"sessionId\":\"\",\"messages\":[]}".to_string()
    } else {
        backend
            .runtime
            .block_on(async { build_messages_json(&backend.chat_runtime, &next_active).await })
    };
    emit_string(
        backend.callbacks.on_messages_updated,
        backend.callback_ctx as *mut c_void,
        &messages_payload,
    );

    true
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_string_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: pointer allocated by CString::into_raw
    let _ = unsafe { CString::from_raw(ptr) };
}

fn emit_string(callback: Option<StringCallback>, ctx: *mut c_void, text: &str) {
    let Some(cb) = callback else {
        return;
    };
    if let Ok(value) = CString::new(text) {
        cb(ctx, value.as_ptr());
    }
}

fn emit_session(callback: Option<SessionCallback>, ctx: *mut c_void, session_id: &str) {
    let Some(cb) = callback else {
        return;
    };
    if let Ok(session) = CString::new(session_id) {
        cb(ctx, session.as_ptr());
    }
}

fn emit_session_string(
    callback: Option<SessionStringCallback>,
    ctx: *mut c_void,
    session_id: &str,
    text: &str,
) {
    let Some(cb) = callback else {
        return;
    };
    if let (Ok(session), Ok(value)) = (CString::new(session_id), CString::new(text)) {
        cb(ctx, session.as_ptr(), value.as_ptr());
    }
}

async fn build_sessions_json(chat_runtime: &ChatRuntime) -> String {
    let sessions = chat_runtime.list_sessions().await;
    let arr: Vec<_> = sessions
        .into_iter()
        .map(|s| {
            let last_updated = s.messages.last().map(|m| m.timestamp).unwrap_or(0);
            let first_user = s
                .messages
                .iter()
                .find(|m| m.role == "user")
                .map(|m| m.content.clone())
                .unwrap_or_default();
            let session_title = if first_user.is_empty() {
                format!(
                    "{} · {}",
                    s.model.clone().unwrap_or_else(|| "model".to_string()),
                    provider_label(s.provider)
                )
            } else {
                let preview: String = first_user.chars().take(24).collect();
                format!(
                    "{} · {}",
                    s.model.clone().unwrap_or_else(|| "model".to_string()),
                    preview
                )
            };

            json!({
                "id": s.id,
                "sessionId": s.id,
                "title": session_title,
                "provider": provider_label(s.provider),
                "model": s.model.unwrap_or_default(),
                "projectRoot": s.project_root.unwrap_or_default(),
                "lastUpdated": last_updated,
            })
        })
        .collect();

    serde_json::to_string(&arr).unwrap_or_else(|_| "[]".to_string())
}

async fn build_models_json(config_path: &PathBuf, fallback_base_url: &str) -> String {
    let mut servers = load_enabled_ollama_servers(config_path);
    if servers.is_empty() {
        servers.push(("Primary".to_string(), normalize_base_url(fallback_base_url)));
    }

    let mut rows = Vec::new();
    for (server_name, base_url) in servers {
        let models = fetch_models_for_server(&server_name, &base_url).await;
        rows.extend(models);
    }

    serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string())
}

fn load_enabled_ollama_servers(config_path: &PathBuf) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Ok(raw) = std::fs::read_to_string(config_path) {
        if let Ok(cfg) = toml::from_str::<AppConfig>(&raw) {
            let mut servers = cfg
                .servers
                .into_iter()
                .filter(|s| s.enabled && s.provider == ProviderKind::Ollama)
                .collect::<Vec<_>>();
            servers.sort_by_key(|s| s.priority);
            for server in servers {
                out.push((server.name, normalize_base_url(&server.base_url)));
            }
        }
    }
    out
}

async fn fetch_models_for_server(server_name: &str, base_url: &str) -> Vec<serde_json::Value> {
    let base_url = normalize_base_url(base_url);
    let client = reqwest::Client::new();
    let response = match client
        .get(format!("{}/api/tags", base_url.trim_end_matches('/')))
        .send()
        .await
    {
        Ok(res) => res,
        Err(_) => return Vec::new(),
    };

    if !response.status().is_success() {
        return Vec::new();
    }

    let tags = match response.json::<OllamaTagsResponse>().await {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    tags
        .models
        .into_iter()
        .filter(|m| !is_embedding_like_model(&m.name))
        .map(|m| {
            let size_bytes = m.size.unwrap_or(0);
            let size_label = if size_bytes > 0 {
                format_bytes(size_bytes)
            } else {
                "size unknown".to_string()
            };
            json!({
                "provider": format!("ollama@{}", server_name),
                "name": m.name,
                "label": format!("{} ({}) [{}]", m.name, size_label, server_name),
                "serverName": server_name,
                "serverUrl": base_url,
                "sizeBytes": size_bytes,
            })
        })
        .collect()
}

async fn build_messages_json(chat_runtime: &ChatRuntime, session_id: &str) -> String {
    let messages = chat_runtime
        .list_messages(session_id)
        .await
        .unwrap_or_default();
    let arr: Vec<_> = messages
        .into_iter()
        .map(|m| {
            json!({
                "role": m.role,
                "text": m.content,
                "timestamp": m.timestamp,
            })
        })
        .collect();

    let payload = json!({
        "sessionId": session_id,
        "messages": arr,
    });
    serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())
}

fn into_c_string(value: String) -> *mut c_char {
    CString::new(value)
        .unwrap_or_else(|_| CString::new("{}").expect("valid fallback"))
        .into_raw()
}

fn provider_label(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::Ollama => "Ollama",
        ProviderId::Gemini => "Gemini",
        ProviderId::Codex => "Codex",
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let value = bytes as f64;
    if value >= TB {
        format!("{:.1} TB", value / TB)
    } else if value >= GB {
        format!("{:.1} GB", value / GB)
    } else if value >= MB {
        format!("{:.1} MB", value / MB)
    } else if value >= KB {
        format!("{:.1} KB", value / KB)
    } else {
        format!("{} B", bytes)
    }
}

fn is_embedding_like_model(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("embed") || lower.contains("embedding")
}

fn normalize_base_url(input: &str) -> String {
    let trimmed = input.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return trimmed.to_string();
    }
    format!("http://{}", trimmed)
}

fn base_url_is_wildcard(url: &str) -> bool {
    if let Ok(parsed) = reqwest::Url::parse(url) {
        if let Some(host) = parsed.host_str() {
            return host == "0.0.0.0" || host == "::";
        }
    }
    false
}

fn connection_hint_for_error(url: &str, err: &str) -> String {
    let lower = err.to_ascii_lowercase();
    let host = reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|v| v.to_string()))
        .unwrap_or_else(|| "<server-ip>".to_string());

    if lower.contains("connection refused") {
        return format!(
            "El host responde, pero Ollama no escucha en 11434. En el servidor ejecuta:\n1) OLLAMA_HOST=0.0.0.0:11434 ollama serve\n2) sudo systemctl edit ollama (agrega Environment=\"OLLAMA_HOST=0.0.0.0:11434\")\n3) sudo systemctl daemon-reload && sudo systemctl restart ollama\n4) sudo ss -tlnp | grep 11434\n5) sudo ufw allow from 192.168.0.0/24 to any port 11434 proto tcp\n6) sudo ufw allow from 100.64.0.0/10 to any port 11434 proto tcp\n7) desde cliente: curl http://{}:11434/api/tags",
            host
        );
    }

    if lower.contains("timed out") || lower.contains("operation timed out") {
        return "Timeout de red: probable firewall/ruta. Revisa UFW/iptables y que el puerto 11434 este abierto para tu LAN/Tailscale.".to_string();
    }

    if lower.contains("dns") || lower.contains("name or service not known") {
        return "No se pudo resolver el host. Usa IP directa (ej. 192.168.x.x:11434 o 100.x.x.x:11434).".to_string();
    }

    String::new()
}

fn resolve_active_base_url_from_path(config_path: &PathBuf, fallback: &str) -> String {
    if let Ok(cfg) = std::fs::read_to_string(config_path) {
        if let Ok(app) = toml::from_str::<AppConfig>(&cfg) {
            if let Some(server) = app.primary_server() {
                let normalized = normalize_base_url(&server.base_url);
                if !normalized.is_empty() {
                    return normalized;
                }
            }
        }
    }
    normalize_base_url(fallback)
}

fn load_config_with_recovery(runtime: &Runtime, config_path: &PathBuf) -> (AppConfig, String) {
    match runtime.block_on(AppConfig::load_or_create(config_path)) {
        Ok(cfg) => (cfg, String::new()),
        Err(err) => {
            let backup_path = backup_invalid_config(config_path).ok();
            let default_cfg = AppConfig::default();
            if let Some(parent) = config_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(body) = toml::to_string_pretty(&default_cfg) {
                let _ = std::fs::write(config_path, body);
            }

            let mut notice = format!(
                "Tu archivo de configuracion tiene un error. Se restauro la configuracion por defecto.\n\nArchivo: {}\nError: {}",
                config_path.display(),
                err
            );
            if let Some(backup) = backup_path {
                notice.push_str(&format!("\nRespaldo: {}", backup.display()));
            }

            match runtime.block_on(AppConfig::load_or_create(config_path)) {
                Ok(cfg) => (cfg, notice),
                Err(_) => (default_cfg, notice),
            }
        }
    }
}

fn backup_invalid_config(config_path: &PathBuf) -> Result<PathBuf, std::io::Error> {
    if !config_path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "config does not exist",
        ));
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup_name = format!("multilink.invalid-{}.toml", stamp);
    let backup = config_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(backup_name);
    std::fs::rename(config_path, &backup)?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffi_lifecycle_smoke() {
        let callbacks = BackendCallbacks {
            on_stream_started: None,
            on_stream_chunk: None,
            on_stream_finished: None,
            on_stream_error: None,
            on_token_usage: None,
            on_sessions_updated: None,
            on_models_updated: None,
            on_messages_updated: None,
        };

        let handle = chat_backend_create(callbacks, std::ptr::null_mut());
        assert!(!handle.is_null());

        let empty = CString::new("").expect("valid cstring");
        // SAFETY: handle is valid and created above
        unsafe {
            chat_backend_send_prompt(handle, empty.as_ptr());
            chat_backend_stop_generation(handle);
            chat_backend_destroy(handle);
        }
    }
}
