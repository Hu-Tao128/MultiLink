# AGENTS.md

This file is guidance for coding agents working in this repository.
It is focused on practical build/test workflows and code style expectations.

## Scope and Priority

- Keep core behavior stable before adding new features.
- Prefer small, verifiable changes over large rewrites.
- Preserve architecture boundaries:
  - `core/` owns runtime, routing, providers, persistence, auth.
  - `gui/` owns presentation and user interaction.
  - C++/Qt shim stays thin; business logic belongs in Rust core.

## Rules Discovery

- Checked for Cursor rules:
  - `.cursor/rules/` -> not present
  - `.cursorrules` -> not present
- Checked for Copilot instructions:
  - `.github/copilot-instructions.md` -> not present

No additional repo-specific AI rule files are currently enforced.

## Repository Layout (high value paths)

- `core/src/chat_runtime.rs`: central runtime orchestration.
- `core/src/config.rs`: config schema, migration, validation.
- `core/src/execution/`: execution dispatcher and fallback routing.
- `core/src/providers/`: provider integrations (Ollama, Gemini, Codex).
- `core/src/context_retrieval.rs`: retrieval and project-context assembly.
- `gui/rust/chat_controller/src/lib.rs`: Rust backend for GUI bridge.
- `gui/src/chatcontroller.*`: Qt/C++ bridge layer.
- `gui/qml/`: QML screens (`Main.qml`, `ChatView.qml`, `Settings.qml`).

## Build Commands

Run from repo root unless noted.

### Core build

```bash
cargo build --manifest-path core/Cargo.toml
```

### Core tests

```bash
cargo test --manifest-path core/Cargo.toml
```

### Run a single Rust test (core)

By test name substring:

```bash
cargo test --manifest-path core/Cargo.toml runtime_limits_parallel_streams
```

By test target file + name:

```bash
cargo test --manifest-path core/Cargo.toml --test chat_runtime_tests runtime_streams_and_persists_session
```

### GUI Rust backend build

```bash
cargo build --release --manifest-path gui/rust/chat_controller/Cargo.toml
```

### GUI configure/build

```bash
cmake -S gui -B build/gui -DCMAKE_BUILD_TYPE=Release
cmake --build build/gui --config Release
```

### GUI tests (if present)

```bash
ctest --test-dir build/gui -C Release
```

### Run app

```bash
./build/gui/multilink_gui
```

Debug context behavior:

```bash
MULTILINK_DEBUG_CONTEXT=1 ./build/gui/multilink_gui
```

## Lint/Format Commands

Rust formatting:

```bash
cargo fmt --all --manifest-path core/Cargo.toml
```

Rust linting:

```bash
cargo clippy --manifest-path core/Cargo.toml -- -D warnings
```

GUI Rust linting:

```bash
cargo clippy --manifest-path gui/rust/chat_controller/Cargo.toml -- -D warnings
```

If linting is noisy due to toolchain/platform differences, document it in PR notes.

## Code Style Guidelines

### Rust (core + GUI Rust bridge)

- Use Rust 2021 idioms (no Rust 2024-only syntax).
- Keep modules focused; avoid giant mixed-responsibility files.
- Prefer explicit structs/enums over untyped maps for domain data.
- Keep function signatures stable for runtime contracts once introduced.
- Use `Result<_, _>` and `thiserror`-based errors for recoverable failures.
- Never `unwrap()` in production-path logic; use fallbacks or propagate errors.
- `unwrap_or_default()` is acceptable for non-critical telemetry/UI serialization.
- Add `#[serde(default)]` for backward-compatible config evolution.
- Keep imports grouped: std, third-party, crate-local.
- Prefer `Arc` + async-safe synchronization (`tokio::sync::*`) in runtime paths.
- Use bounded channels/semaphores for concurrency control.

### Naming

- Types: `PascalCase` (`ExecutionDispatcher`, `ServerStatus`).
- Functions/vars: `snake_case`.
- Constants: `SCREAMING_SNAKE_CASE`.
- Test names should describe behavior, e.g. `runtime_recovers_partial_wal_on_load`.

### Error handling and resilience

- Distinguish transient network errors from permanent config/logic errors.
- For provider calls, prefer retry/fallback in dispatcher layer.
- Include actionable user-facing hints for connectivity issues.
- Preserve session state consistency on both success and failure paths.

### Config and migration

- Treat config compatibility as a contract.
- New config fields should be backward compatible.
- If schema changes, update migration logic and tests together.
- Keep defaults safe for low-resource hardware.

### QML/C++ bridge

- QML is declarative UI only; no networking or persistence in QML.
- Expose minimal, stable Q_INVOKABLE methods from `ChatController`.
- C++ bridge should convert and relay data/events, not own business decisions.
- Keep string encoding UTF-8 clean when crossing FFI boundaries.

## Testing Expectations for Changes

For core behavior changes, run:

1. `cargo test --manifest-path core/Cargo.toml`
2. `cmake --build build/gui --config Release`

For GUI bridge changes, also smoke-run app:

```bash
QT_QPA_PLATFORM=offscreen timeout 8s ./build/gui/multilink_gui
```

For config/routing changes, manually verify:

- config load/migration path
- server test in Settings
- model list refresh from configured servers
- at least one prompt round-trip

## Commit Guidance

- Keep commits scoped by concern:
  - `feat(core): ...`
  - `fix(gui): ...`
  - `docs: ...`
- Do not mix major refactors with unrelated docs churn.
- Include tests when changing runtime, config, routing, or retrieval behavior.

## Agent Working Notes

- Read current diffs before editing; do not revert unrelated user changes.
- Prefer non-destructive fixes and incremental refactors.
- If a CI/platform issue appears (Qt version differences), add compatibility fallback.
- When uncertain, choose the option that preserves runtime correctness first.
