# GUI (`gui/`)

Qt/QML desktop app plus Rust-backed bridge artifacts.

## What this folder contains

- `CMakeLists.txt`: Qt build + Rust backend orchestration.
- `src/`: C++ entrypoint and thin Qt shim (`ChatController`).
- `qml/`: declarative UI screens.
- `assets/`: screenshots and static assets.
- `rust/chat_controller/`: Rust static backend linked into Qt app.

## Build

From repo root:

```bash
cmake -S gui -B build/gui
cmake --build build/gui
```

This automatically builds `rust/chat_controller` via Cargo.

## Current scope

- Chat/session UX is fully wired through the Rust core runtime.
- Existing sessions are restored on startup instead of always creating a new one.
- OAuth controls in `qml/Settings.qml` are placeholders until auth wiring is connected to core `AuthService`.
