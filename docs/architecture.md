# Architecture

## Layered design

```text
Qt/QML GUI
  -> CXX-Qt Bridge
    -> Rust Core
      -> providers/ model_manager/ auth/ system/ config/
```

## Core boundaries

- Core never depends on Qt types.
- UI state is exposed through stable DTO-like values and enums.
- Providers implement a shared trait for runtime swap and fallback.

## Main flows

### Chat flow

1. QML calls `ChatController.sendPrompt(...)`.
2. Bridge forwards to `ChatRuntime.send_message(...)`.
3. Runtime routes provider calls, owns session state, and persists conversation data.
4. Runtime emits `StreamEvent` over `tokio::sync::mpsc` (`Started`, `Chunk`, `Finished`, `Error`).
5. QML only renders incoming events and sends user input.

QML does not perform network requests, streaming parsing, or persistence.

### OAuth flow

1. Core builds authorization URL for provider.
2. App opens system browser.
3. Provider redirects to `http://127.0.0.1:<port>/callback`.
4. Core exchanges code, stores encrypted token, and refreshes when needed.

### Model management flow

1. Detect local model folder and installed models.
2. Query external registries (HTTP / npm-like) for available models.
3. Show size, status, and path.
4. Apply migration and symlink strategy when storage path changes.

## Non-functional guarantees

- Async core (`tokio`) keeps GUI responsive.
- Stream chunk emission is throttled in Rust (time-based) to avoid excessive GUI/FFI churn.
- Security guardrails: encrypted tokens, restrictive file permissions, explicit consent for privileged install.
- Linux-first UX with portable core interfaces for Windows/macOS.

## Session storage layout

`ChatRuntime` resolves platform data paths with `directories::ProjectDirs` and stores:

- `sessions/<session-id>.json`
- `sessions/index.json`
- `sessions/state.json`

Streaming writes partial state into temporary files and uses atomic rename on commit.
