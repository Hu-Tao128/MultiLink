# GUI C++ Source (`gui/src/`)

Thin adapter layer between QML and Rust backend.

## Files

- `main.cpp`: Qt app bootstrap, injects `chatController` context property.
- `chatcontroller.h/.cpp`: shim only; converts Rust FFI payloads to Qt properties/signals and forwards user actions to Rust.
- `bridge.rs`: legacy/prototype bridge experiments (not primary runtime path).

## Important rule

No networking, persistence, or provider logic belongs here.
