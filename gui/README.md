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
- Chat view includes selectable assistant text and clipboard copy actions for code blocks and full assistant messages.
- OAuth controls in `qml/Settings.qml` are placeholders until auth wiring is connected to core `AuthService`.

## Development emphasis

- GUI work should preserve runtime-first architecture: smooth interaction, but no business logic migration from core.
- Favor low-overhead UX changes (rendering, selection, copy, navigation aids) over heavy visual complexity.
