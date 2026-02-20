# Chat Controller Backend (`gui/rust/chat_controller/`)

Rust static library consumed by Qt GUI.

## Files

- `Cargo.toml`: crate metadata and dependencies.
- `src/lib.rs`: FFI API exposed to C++ shim; delegates chat operations to `multilink-core::ChatRuntime`.

## Responsibilities

- Create and own one `ChatRuntime` instance.
- Forward actions (`send`, `stop`, `new session`, `select session/model`).
- Emit stream callbacks for UI (`started`, `chunk`, `finished`, `error`).
- Provide JSON payloads for sessions and model lists.

## Local commands

```bash
cargo build --release
cargo test
```
