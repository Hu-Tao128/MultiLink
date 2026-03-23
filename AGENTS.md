# AGENTS.md

> **IMPORTANT**: Read `./MULTILINK.md` first for project context and agent behavior guidelines.

This file provides guidance for coding agents working in this repository.

## Scope and Priority

- Keep core behavior stable before adding new features.
- Prefer small, verifiable changes over large rewrites.
- Preserve architecture boundaries:
  - `core/` owns runtime, routing, providers, persistence, auth.
  - `gui/` owns presentation and user interaction.
  - C++/Qt shim stays thin; business logic belongs in Rust core.

## Rules Discovery

- `.cursor/rules/` -> not present
- `.cursorrules` -> not present  
- `.github/copilot-instructions.md` -> not present

No additional repo-specific AI rule files are currently enforced.

## Repository Layout

```
MultiLink/
├── core/                    # Rust core library (multilink-core)
│   ├── src/
│   │   ├── chat_runtime.rs  # Central runtime orchestration
│   │   ├── config.rs        # Config schema, migration, validation
│   │   ├── execution/       # Execution dispatcher and fallback routing
│   │   ├── providers/       # Provider integrations (Ollama, Gemini, Codex)
│   │   ├── context_engine/  # Context retrieval, embeddings, chunking
│   │   ├── model_manager/   # Model registry and npm integration
│   │   ├── auth/            # OAuth and token storage
│   │   ├── session.rs       # Session management
│   │   ├── router.rs        # Provider routing with health monitor, circuit breaker
│   │   ├── lan_agent.rs    # LAN Agent TCP server for MessagePack
│   │   ├── mcp_adapter.rs  # MCP protocol adapter
│   │   ├── skills.rs        # Skill system for orchestrator
│   │   └── benchmark.rs     # Benchmark suite and release criteria
│   └── Cargo.toml
├── gui/                     # Qt/QML GUI application
│   ├── qml/                 # QML screens (Main.qml, ChatView.qml, Settings.qml)
│   ├── src/                 # C++ bridge layer (chatcontroller.*, bridge.rs)
│   ├── rust/chat_controller/ # Rust backend for GUI bridge
│   └── CMakeLists.txt
├── config/                  # Default configuration files
├── .github/workflows/       # CI/CD pipelines
└── build/                   # Build output (gitignored)
```

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

Smoke test (headless):
```bash
QT_QPA_PLATFORM=offscreen timeout 8s ./build/gui/multilink_gui
```

## Lint/Format Commands

### Rust formatting

```bash
cargo fmt --all --manifest-path core/Cargo.toml
```

### Rust linting

```bash
cargo clippy --manifest-path core/Cargo.toml -- -D warnings
```

### GUI Rust linting

```bash
cargo clippy --manifest-path gui/rust/chat_controller/Cargo.toml -- -D warnings
```

If linting is noisy due to toolchain/platform differences, document it in PR notes.

## Code Style Guidelines

### Rust (core + GUI Rust bridge)

- **Edition**: Rust 2021 (no Rust 2024-only syntax)
- **Modules**: Keep focused; avoid giant mixed-responsibility files
- **Types**: Prefer explicit structs/enums over untyped maps for domain data
- **Stability**: Keep function signatures stable for runtime contracts once introduced
- **Errors**: Use `Result<_, _>` and `thiserror`-based errors for recoverable failures
- **No unwrap()**: Never `unwrap()` in production-path logic; use fallbacks or propagate errors
- **unwrap_or_default()**: Acceptable for non-critical telemetry/UI serialization
- **Serde**: Add `#[serde(default)]` for backward-compatible config evolution
- **Imports**: Keep grouped: std, third-party, crate-local
- **Concurrency**: Prefer `Arc` + async-safe synchronization (`tokio::sync::*`)
- **Channels**: Use bounded channels/semaphores for concurrency control

### Naming Conventions

| Element | Convention | Example |
|---------|------------|---------|
| Types | PascalCase | `ExecutionDispatcher`, `ServerStatus` |
| Functions/vars | snake_case | `get_config()`, `model_list` |
| Constants | SCREAMING_SNAKE_CASE | `MAX_RETRIES`, `DEFAULT_TIMEOUT` |
| Test names | describe behavior | `runtime_recovers_partial_wal_on_load` |

### Error Handling and Resilience

- Distinguish transient network errors from permanent config/logic errors
- For provider calls, prefer retry/fallback in dispatcher layer
- Include actionable user-facing hints for connectivity issues
- Preserve session state consistency on both success and failure paths

### Config and Migration

- Treat config compatibility as a contract
- New config fields should be backward compatible
- If schema changes, update migration logic and tests together
- Keep defaults safe for low-resource hardware

### QML/C++ Bridge

- QML is declarative UI only; no networking or persistence in QML
- Expose minimal, stable Q_INVOKABLE methods from `ChatController`
- C++ bridge should convert and relay data/events, not own business decisions
- Keep string encoding UTF-8 clean when crossing FFI boundaries

## Testing Expectations

For core behavior changes, run:
1. `cargo test --manifest-path core/Cargo.toml`
2. `cargo clippy --manifest-path core/Cargo.toml -- -D warnings`
3. `cmake --build build/gui --config Release`

For GUI bridge changes, also smoke-run app:
```bash
QT_QPA_PLATFORM=offscreen timeout 8s ./build/gui/multilink_gui
```

For config/routing changes, manually verify:
- Config load/migration path
- Server test in Settings
- Model list refresh from configured servers
- At least one prompt round-trip

For LAN Agent changes:
- Run `cargo test --manifest-path core/Cargo.toml --test lan_agent_tests`
- Manually test TCP connectivity from another device on the LAN
- Verify HMAC signature rejection with wrong secret
- Verify `allow_remote=false` denies non-loopback IPs with empty allowlist

## Manual GUI Testing Checklist

After building, test these features manually:

### First Run
- [ ] Config auto-created with LAN secret printed to stderr
- [ ] No crash on startup
- [ ] Ollama auto-detection works

### Settings → Servers
- [ ] Add server button works
- [ ] "Probar conexion" shows correct OK/error
- [ ] Save persists after restart
- [ ] Provider dropdown shows: ollama, gemini, codex

### Chat
- [ ] Send prompt → get response from Ollama
- [ ] New session button works
- [ ] Delete session works
- [ ] Model selector changes active model

### LAN Agent (multi-device)
- [ ] `/doctor --security` shows LAN secret
- [ ] Remote device can connect with correct HMAC secret
- [ ] Remote device rejected with wrong secret

### Known GUI Gaps (not yet wired)
- Provider OAuth token UI not implemented (Gemini/Codex tokens via file only)

## Commit Guidance

- Keep commits scoped by concern: `feat(core): ...`, `fix(gui): ...`, `docs: ...`
- Do not mix major refractors with unrelated docs churn
- Include tests when changing runtime, config, routing, or retrieval behavior

## Agent Working Notes

- Read current diffs before editing; do not revert unrelated user changes
- Prefer non-destructive fixes and incremental refactors
- If a CI/platform issue appears (Qt version differences), add compatibility fallback
- When uncertain, choose the option that preserves runtime correctness first

## Agent Working Notes

- Read current diffs before editing; do not revert unrelated user changes
- Prefer non-destructive fixes and incremental refactors
- If a CI/platform issue appears (Qt version differences), add compatibility fallback
- When uncertain, choose the option that preserves runtime correctness first
