# Roadmap

## ✅ Completed

### Week 1-3: Foundation
- [x] Repository bootstrap
- [x] Core provider traits and router skeleton
- [x] Ollama provider (request + response path)
- [x] Streaming pipeline and session states
- [x] Config loading and environment overrides

### Week 4-5: GUI Development
- [x] Qt/QML shell and CXX-Qt bridge
- [x] Token event rendering in chat view
- [x] Provider/model switch UX
- [x] Session restoration
- [x] Server settings panel
- [x] Multi-server model listing

### Week 6-7: Integrations & CI
- [x] OAuth scaffolding for remote providers
- [x] Token encryption (AES-256-GCM)
- [x] Cross-platform build hardening
- [x] CI/CD automation (GitHub Actions)
- [x] Install scripts

## 🚧 In Progress

- [ ] OAuth hardening (refresh token lifecycle)
- [ ] Remote provider auth UI wiring
- [ ] Model registry external sources
- [x] Context Engine v2 (lexical + semantic + hybrid retrieval)
- [x] LSP Server (tower-lsp with tree-sitter)
- [x] Observability metrics (context_latency_ms, hit_rate, truncation_rate)
- [x] Benchmarks (lexical search, indexing)
- [x] Early orchestrator/tool scaffolding
- [x] Project `/init`, `/doctor`, and explicit `/write-file`

## 📋 Backlog

### Coding Agent MVP ✅
- [x] Read-only Git tools: `git_status`, `git_diff`
- [x] Internal `write_file` tool with size limits, diff/hash reporting, and tests
- [x] `apply_patch` tool with path guard and patch validation
- [x] Allowlisted `run_command` for validation commands detected by `/init`
- [x] Executor loop: plan, inspect, edit, validate, correct, final report
- [x] Live LSP symbol bridge into Context Engine
- [x] MCP tool exposure, read-only first (tools.list + tools.execute)

### Product/Packaging
- [ ] Windows installer (.msi)
- [ ] macOS installer (.dmg)
- [ ] Linux packaging (AppImage, .deb)
- [ ] Auto-update mechanism
- [ ] System tray support
- [ ] Keyboard shortcuts
- [ ] Export/import sessions
- [ ] Plugin system for custom providers

## 🔮 Long Term

- [ ] WebAssembly core for browser demo
- [ ] Mobile companion app
- [ ] Team/enterprise features (shared configs)
- [ ] Telemetry dashboard
