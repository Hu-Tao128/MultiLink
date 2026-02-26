use std::ffi::{c_char, c_void, CStr, CString};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use multilink_core::providers::ollama::OllamaProvider;
use multilink_core::{ChatRuntime, ProviderId, ProviderRouter, StreamEvent};
use serde_json::json;
use tokio::runtime::Runtime;

const PERSIST_INTERVAL: Duration = Duration::from_secs(2);

type SessionCallback = extern "C" fn(*mut c_void, *const c_char);
type SessionStringCallback = extern "C" fn(*mut c_void, *const c_char, *const c_char);
type StringCallback = extern "C" fn(*mut c_void, *const c_char);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BackendCallbacks {
    pub on_stream_started: Option<SessionCallback>,
    pub on_stream_chunk: Option<SessionStringCallback>,
    pub on_stream_finished: Option<SessionCallback>,
    pub on_stream_error: Option<SessionStringCallback>,
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
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            active_provider: "Ollama".to_string(),
            active_model: "".to_string(),
            provider_scope: "LOCAL".to_string(),
            provider_health: "unavailable".to_string(),
            is_loading: false,
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

pub struct BackendHandle {
    runtime: Runtime,
    chat_runtime: Arc<ChatRuntime>,
    callbacks: BackendCallbacks,
    callback_ctx: usize,
    active_session_id: Arc<Mutex<String>>,
    ui_state: Arc<Mutex<UiState>>,
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

    let mut router = ProviderRouter::new();
    router.register(Arc::new(OllamaProvider::new(
        "http://127.0.0.1:11434".to_string(),
        "llama3.2".to_string(),
    )));

    let chat_runtime = Arc::new(
        ChatRuntime::new_portable(Arc::new(router), PERSIST_INTERVAL).unwrap_or_else(|_| {
            ChatRuntime::new(
                Arc::new(ProviderRouter::new()),
                PathBuf::from("./.multilink/sessions"),
                PERSIST_INTERVAL,
            )
        }),
    );

    let _ = runtime.block_on(chat_runtime.load_sessions_from_disk());
    let active = runtime.block_on(async {
        if let Some(existing_active) = chat_runtime.active_session().await {
            return existing_active;
        }

        let sessions = chat_runtime.list_sessions().await;
        if let Some(first) = sessions.first() {
            return first.id.clone();
        }

        chat_runtime
            .create_session(ProviderId::Ollama, Some("llama3.2".to_string()))
            .await
    });

    let ui = UiState::default();

    Box::into_raw(Box::new(BackendHandle {
        runtime,
        chat_runtime,
        callbacks,
        callback_ctx: callback_ctx as usize,
        active_session_id: Arc::new(Mutex::new(active)),
        ui_state: Arc::new(Mutex::new(ui)),
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
    let runtime = backend.runtime.handle().clone();
    let chat_runtime = backend.chat_runtime.clone();
    let active_session_id = backend.active_session_id.clone();

    runtime.spawn(async move {
        if chat_runtime.select_session(&id).await.is_ok() {
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
        .block_on(async { build_models_json().await });
    into_c_string(payload)
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

    runtime.spawn(async move {
        let payload = build_models_json().await;
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

async fn build_models_json() -> String {
    let client = reqwest::Client::new();
    let response = match client.get("http://127.0.0.1:11434/api/tags").send().await {
        Ok(res) => res,
        Err(_) => return "[]".to_string(),
    };

    if !response.status().is_success() {
        return "[]".to_string();
    }

    let tags = match response.json::<OllamaTagsResponse>().await {
        Ok(t) => t,
        Err(_) => return "[]".to_string(),
    };

    let rows: Vec<_> = tags
        .models
        .into_iter()
        .map(|m| {
            let size_bytes = m.size.unwrap_or(0);
            let size_label = if size_bytes > 0 {
                format_bytes(size_bytes)
            } else {
                "size unknown".to_string()
            };
            json!({
                "provider": "ollama",
                "name": m.name,
                "label": format!("{} ({})", m.name, size_label),
                "sizeBytes": size_bytes,
            })
        })
        .collect();

    serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string())
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
