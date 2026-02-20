use std::ffi::{c_char, c_void, CStr, CString};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use multilink_core::providers::ollama::OllamaProvider;
use multilink_core::{ChatRuntime, ProviderId, ProviderRouter, StreamEvent};
use serde_json::json;
use tokio::runtime::Runtime;

type VoidCallback = extern "C" fn(*mut c_void);
type StringCallback = extern "C" fn(*mut c_void, *const c_char);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BackendCallbacks {
    pub on_stream_started: Option<VoidCallback>,
    pub on_stream_chunk: Option<StringCallback>,
    pub on_stream_finished: Option<VoidCallback>,
    pub on_stream_error: Option<StringCallback>,
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
pub extern "C" fn chat_backend_create(callbacks: BackendCallbacks, callback_ctx: *mut c_void) -> *mut BackendHandle {
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
        ChatRuntime::new_portable(Arc::new(router), Duration::from_millis(400)).unwrap_or_else(|_| {
            ChatRuntime::new(
                Arc::new(ProviderRouter::new()),
                PathBuf::from("./.multilink/sessions"),
                Duration::from_millis(400),
            )
        }),
    );

    let _ = runtime.block_on(chat_runtime.load_sessions_from_disk());
    let active = runtime
        .block_on(chat_runtime.active_session())
        .unwrap_or_else(|| runtime.block_on(chat_runtime.create_session(ProviderId::Ollama, Some("llama3.2".to_string()))));

    let mut ui = UiState::default();
    ui.provider_health = "available".to_string();

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
    let runtime = backend.runtime.handle().clone();
    let chat_runtime = backend.chat_runtime.clone();
    let callbacks = backend.callbacks;
    let ctx = backend.callback_ctx;
    let ui_state = backend.ui_state.clone();

    runtime.spawn(async move {
        match chat_runtime.send_message(&active_session, prompt).await {
            Ok(mut rx) => {
                while let Some(event) = rx.recv().await {
                    match event {
                        StreamEvent::Started => {
                            if let Some(cb) = callbacks.on_stream_started {
                                cb(ctx as *mut c_void);
                            }
                        }
                        StreamEvent::Chunk(chunk) => {
                            if let Ok(mut ui) = ui_state.lock() {
                                ui.provider_health = "available".to_string();
                            }
                            emit_string(callbacks.on_stream_chunk, ctx as *mut c_void, &chunk);
                        }
                        StreamEvent::Finished => {
                            if let Ok(mut ui) = ui_state.lock() {
                                ui.is_loading = false;
                            }
                            if let Some(cb) = callbacks.on_stream_finished {
                                cb(ctx as *mut c_void);
                            }
                        }
                        StreamEvent::Error(message) => {
                            if let Ok(mut ui) = ui_state.lock() {
                                ui.is_loading = false;
                                ui.provider_health = "unavailable".to_string();
                            }
                            emit_string(callbacks.on_stream_error, ctx as *mut c_void, &message);
                        }
                    }
                }
            }
            Err(err) => {
                if let Ok(mut ui) = ui_state.lock() {
                    ui.is_loading = false;
                    ui.provider_health = "unavailable".to_string();
                }
                emit_string(callbacks.on_stream_error, ctx as *mut c_void, &err.to_string());
            }
        }
    });
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
    let _ = backend.runtime.block_on(backend.chat_runtime.cancel_stream(&active_session));
    if let Ok(mut ui) = backend.ui_state.lock() {
        ui.is_loading = false;
        ui.provider_health = "available".to_string();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_new_session(handle: *mut BackendHandle) {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return;
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
        *active = id;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_select_session(handle: *mut BackendHandle, session_id: *const c_char) {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return;
    };
    let Some(id_cstr) = (!session_id.is_null()).then(|| unsafe { CStr::from_ptr(session_id) }) else {
        return;
    };
    let id = id_cstr.to_string_lossy().to_string();
    if backend.runtime.block_on(backend.chat_runtime.select_session(&id)).is_ok() {
        if let Ok(mut active) = backend.active_session_id.lock() {
            *active = id;
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_select_model(handle: *mut BackendHandle, model: *const c_char) {
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

    let _ = backend
        .runtime
        .block_on(backend.chat_runtime.update_session_model(&active_session, Some(value.clone())));

    if let Ok(mut ui) = backend.ui_state.lock() {
        ui.active_model = value;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_sessions_json(handle: *mut BackendHandle) -> *mut c_char {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return std::ptr::null_mut();
    };

    let sessions = backend.runtime.block_on(backend.chat_runtime.list_sessions());
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
                "title": session_title,
                "provider": provider_label(s.provider),
                "model": s.model.unwrap_or_default(),
                "lastUpdated": last_updated,
            })
        })
        .collect();

    into_c_string(serde_json::to_string(&arr).unwrap_or_else(|_| "[]".to_string()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_models_json(handle: *mut BackendHandle) -> *mut c_char {
    let Some(backend) = (unsafe { handle.as_ref() }) else {
        return into_c_string("[]".to_string());
    };

    let payload = backend.runtime.block_on(async {
        let client = reqwest::Client::new();
        let response = client
            .get("http://127.0.0.1:11434/api/tags")
            .send()
            .await
            .map_err(|_| ())?;
        if !response.status().is_success() {
            return Err(());
        }

        let tags = response
            .json::<OllamaTagsResponse>()
            .await
            .map_err(|_| ())?;

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

        serde_json::to_string(&rows).map_err(|_| ())
    });

    into_c_string(payload.unwrap_or_else(|_| "[]".to_string()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chat_backend_get_active_provider(handle: *mut BackendHandle) -> *mut c_char {
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
pub unsafe extern "C" fn chat_backend_get_provider_scope(handle: *mut BackendHandle) -> *mut c_char {
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
pub unsafe extern "C" fn chat_backend_get_provider_health(handle: *mut BackendHandle) -> *mut c_char {
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
    backend.ui_state.lock().map(|s| s.is_loading).unwrap_or(false)
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
