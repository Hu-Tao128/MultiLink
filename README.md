# MultiLink

MultiLink is a Linux-first, truly cross-platform LLM desktop client with a native Qt/QML GUI and a reusable Rust core.

## Architecture decisions

- Rust owns runtime concerns: streaming, session state, persistence, cancellation, and provider routing.
- QML stays declarative: render state, send user intent, no networking or disk logic.
- Qt shim (`gui/src/chatcontroller.*`) is intentionally thin and only adapts Rust JSON/FFI data into Qt-friendly properties/signals.
- Build orchestration is done from CMake; GUI build triggers Rust backend build, then links static library.

## Why Rust backend + Qt shim

- Rust gives predictable async behavior (`tokio`) and safe core logic.
- Qt provides native cross-platform GUI performance.
- The shim pattern keeps QML simple while avoiding unstable type-bridge hacks for dynamic Qt collections.
- Ownership remains in Rust; C++ only translates types and forwards calls/events.

## Why this project

- Native UX without Electron overhead.
- Clear architecture boundaries for maintainability and interview-ready code.
- Local-first workflow with Ollama and optional remote OAuth providers.

## Architecture

```text
GUI (Qt/QML)
  -> Qt shim (QObject adapter, no business logic)
    -> Rust backend static lib (FFI)
      -> Core (Rust)
        -> providers/ + auth/ + model_manager/ + system/ + config/
```

Simple runtime flow:

```text
QML action -> ChatController shim -> Rust backend -> ChatRuntime -> Provider
Provider stream -> ChatRuntime (throttle/persist) -> Rust callbacks -> Qt signals -> QML render
```

Detailed design: `docs/architecture.md`

The core now owns streaming state and persistence through `ChatRuntime`.
`StreamEvent` is emitted over an async channel (`tokio::sync::mpsc`) so UI layers only render and dispatch input.
QML has no HTTP calls, no streaming parser, and no persistence logic.

## Functional coverage

- Provider routing with runtime switch and fallback to local provider.
- Provider availability status API (`Available` / `NotAvailable`).
- Ollama local provider with streaming support.
- OAuth desktop flow for Gemini/Codex (browser + localhost callback + encrypted token store).
- Model manager registry with local detection, size tracking, migration, and delete support.
- Guided Ollama installation plan for Linux with explicit consent gating.

## Project layout

```text
.
├── core/
│   ├── src/
│   │   ├── providers/
│   │   ├── auth/
│   │   ├── model_manager/
│   │   ├── system.rs
│   │   └── config.rs
│   └── tests/
├── gui/
│   ├── qml/
│   ├── src/
│   └── assets/
├── docs/
├── tests/
└── config/
```

## Build

Core only:

```bash
cd core
cargo check
cargo test
```

Qt GUI shell:

```bash
cmake -S gui -B build/gui
cmake --build build/gui
```

## Security notes

- OAuth tokens are encrypted at rest (AES-256-GCM).
- Config/token files are saved with restrictive permissions on Unix.
- No privileged install command is run without explicit consent.
- Remote providers are opt-in; local provider fallback remains available.

## Performance notes

- Streaming UI updates are real-time.
- During streaming responses, persistence is throttled by time (400 ms) inside the Rust runtime and always flushed on stream completion/error to reduce unnecessary CPU and write pressure.
- Streaming cancellation is explicit and non-blocking through runtime-owned cancellation handles.
- Session files are stored in platform data directories resolved via `directories::ProjectDirs`.

## Screenshots

Place screenshots in `gui/assets/screenshots/` and reference them here.

- Main chat view: `gui/assets/screenshots/main-chat.png`
- Sessions and model selector: `gui/assets/screenshots/sessions-models.png`
- Settings/auth view: `gui/assets/screenshots/settings-auth.png`

## Roadmap (8 weeks)

1. Core skeleton and provider contract.
2. Ollama provider + CLI verification.
3. Streaming + config + session state machine.
4. Qt/QML UI and Rust bridge.
5. Chat UX and model/provider controls.
6. Gemini/Codex OAuth and remote provider hardening.
7. Packaging and install guides.
8. Polish, docs, screenshots, demo script.

## Portfolio checklist

- Works without terminal for end users.
- Uses Ollama through HTTP API, not ad-hoc shell loops.
- Manages model storage and migration.
- Implements desktop-friendly OAuth.
- Keeps core reusable and UI-agnostic.
