# MultiLink

MultiLink is a Linux-first, truly cross-platform LLM desktop client with a native Qt/QML GUI and a reusable Rust core.

## Current engineering priority

- Performance and runtime correctness are prioritized over interface polish.
- The Rust core is treated as the product backbone; GUI changes should not regress stream stability, memory behavior, or context handling.
- UI improvements are welcome when they preserve low overhead and keep logic in core.

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
- Context builder with bounded token budgets and project-context injection (per session) with defensive fallback when provider context windows are exceeded.
- OAuth module for Gemini/Codex in core (browser + localhost callback + encrypted token store), pending end-to-end GUI wiring.
- Model manager registry with local detection, size tracking, migration, and delete support.
- Guided Ollama installation plan for Linux with explicit consent gating.

## Current behavior notes (important for contributors)

- Runtime classifies connection failures separately from prompt/content failures so provider health is not marked down for non-connectivity errors.
- Ollama requests now include system prompts explicitly in `messages` for both sync and streaming paths.
- Streaming parser in Ollama provider handles chunk boundaries safely (line-buffered JSON decode) to avoid partial-frame parse errors.
- If a stream fails due to likely context overflow, runtime retries without project context before surfacing an error.
- GUI supports selectable assistant text and copy actions (code block copy + full message copy) without moving clipboard logic into QML.
- Project context support for models is in progress and intentionally conservative to reduce 500-class startup/send failures.

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

### Prerequisites

- **Rust**: Stable toolchain (install via [rustup](https://rustup.rs/)).
- **CMake**: Version 3.21 or higher.
- **Qt 6**: (6.5+ recommended) with QML and Quick modules.

### Platform-Specific Dependencies

#### Linux (Ubuntu 24.04+)
```bash
sudo apt update
sudo apt install -y qt6-base-dev qt6-declarative-dev cmake build-essential libgl1-mesa-dev libxkbcommon-dev
```

#### macOS
```bash
brew install qt cmake ninja
```

#### Windows
Install via [Chocolatey](https://chocolatey.org/):
```bash
choco install cmake ninja -y
# Qt6 is best installed via the official online installer or 'install-qt-action' in CI.
```

### Compilation

The project uses a unified CMake build that orchestrates the Rust backend automatically.

```bash
# From the repository root
cmake -S gui -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --config Release
```

The executable will be located in `build/` (or `build/Release` on Windows).

## Security notes

- OAuth tokens are encrypted at rest with AES-256-GCM in the core token store.
- Config/token files are saved with restrictive permissions on Unix.
- No privileged install command is run without explicit consent.
- OAuth logic stays in Rust modules; QML does not handle secrets.
- System keyring integration and PKCE hardening are planned next.

## Performance notes

- Streaming UI updates are real-time.
- During streaming responses, persistence is throttled by time (400 ms) inside the Rust runtime and always flushed on stream completion/error to reduce unnecessary CPU and write pressure.
- Streaming cancellation is explicit and non-blocking through runtime-owned cancellation handles.
- Session files are stored in platform data directories resolved via `directories::ProjectDirs`.
- Context limits are intentionally capped below provider maximums to keep requests resilient under real project payloads.

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
