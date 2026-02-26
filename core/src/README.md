# Core Source (`core/src/`)

Main Rust modules and their responsibilities.

## Files

- `lib.rs`: public module/export surface.
- `chat_runtime.rs`: streaming lifecycle, sessions, throttled persistence, cancellation, context assembly/summarization, and fallback retry on likely context overflow.
- `router.rs`: provider selection and fallback behavior.
- `session.rs`: session/message domain types.
- `config.rs`: app config model, defaults, env overrides.

## Runtime design notes

- Context building is token-budgeted and can include cached project context per session.
- On provider errors that look like context-window overflow, runtime retries once without project context.
- Persistence and stream emission are decoupled to keep UI updates responsive while minimizing disk churn.

## Folders

- `providers/`: provider trait + implementations.
- `auth/`: OAuth flow and token storage.
- `model_manager/`: model discovery/registry/migration helpers.
- `system/`: OS/system checks and install helpers.

## Rule of thumb

If state changes over time (chat, stream, retries, persistence), it belongs here.
