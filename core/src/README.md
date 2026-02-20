# Core Source (`core/src/`)

Main Rust modules and their responsibilities.

## Files

- `lib.rs`: public module/export surface.
- `chat_runtime.rs`: streaming lifecycle, sessions, throttled persistence, cancellation.
- `router.rs`: provider selection and fallback behavior.
- `session.rs`: session/message domain types.
- `config.rs`: app config model, defaults, env overrides.

## Folders

- `providers/`: provider trait + implementations.
- `auth/`: OAuth flow and token storage.
- `model_manager/`: model discovery/registry/migration helpers.
- `system/`: OS/system checks and install helpers.

## Rule of thumb

If state changes over time (chat, stream, retries, persistence), it belongs here.
