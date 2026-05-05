# Architecture

## Layered design

```text
Qt/QML GUI
  -> CXX-Qt Bridge
    -> Rust Core
      -> chat_runtime/
      -> orchestrator/ tools/ commands/ context_engine/
      -> providers/ model_manager/ auth/ system/ config/
      -> optional LAN Agent / thin MCP adapter / LSP server
```

## Core boundaries

- Core never depends on Qt types.
- UI state is exposed through stable DTO-like values and enums.
- Providers implement a shared trait for runtime swap and fallback.
- Coding-agent logic belongs in Rust core. QML may expose buttons or status, but it must not implement planning, filesystem access, command execution, or patch logic.
- Tools must be deterministic, path-guarded, and auditable. Model-generated text is not a substitute for tool results.

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

### Coding-agent flow

1. Runtime resolves the active project root from session state or command context.
2. Planner classifies the user request as read, search, system-info, or LLM-only.
3. Executor runs deterministic tools first when applicable (`search_code`, `open_file`, `fs_grep`, etc.).
4. Context Engine supplies project snippets within a model-aware token budget.
5. Provider response synthesizes the final answer from real tool output.
6. For writes today, `/write-file` is explicit and path-guarded. General patch editing and validation loops are planned in `docs/CODING_AGENT_MVP.md`.

### Agent surfaces

- Slash commands: `/init`, `/doctor`, `/write-file`.
- Internal tools: read/search/system tools in `core/src/tools/`.
- Skills: TOML manifests loaded from global and project locations.
- LSP: experimental semantic server in `lsp-server/`; not yet a live source for runtime Context Engine.
- MCP/LAN: thin adapter currently maps chat dispatch and ping, not a complete tool API.

## Non-functional guarantees

- Async core (`tokio`) keeps GUI responsive.
- Stream chunk emission is throttled in Rust (time-based) to avoid excessive GUI/FFI churn.
- Security guardrails: encrypted tokens, restrictive file permissions, explicit consent for privileged install.
- Linux-first UX with portable core interfaces for Windows/macOS.
- Coding-agent operations prefer read-only tools and explicit writes until patch and command tools have complete guardrail tests.

## Session storage layout

`ChatRuntime` resolves platform data paths with `directories::ProjectDirs` and stores:

- `sessions/<session-id>.json`
- `sessions/index.json`
- `sessions/state.json`

Streaming writes partial state into temporary files and uses atomic rename on commit.
