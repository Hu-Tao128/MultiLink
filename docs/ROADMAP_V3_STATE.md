# Roadmap V3 State

Last updated: 2026-03-17

CURRENT_PHASE = release
NEXT_PHASE = maintenance

## All Phases Complete

| Phase | Status |
|-------|--------|
| Phase 1 Observability | complete |
| Phase 2 Config V2 + migration | complete |
| Phase 3 Model-aware budgeting | complete |
| Phase 4 Intent-aware budgeting | complete |
| Phase 5 Hardware-aware caps | complete |
| Phase 6 Context Engine v2 | complete |
| Phase 7 Network security enforcement | complete |
| Phase 8 Multi-server router hardening | complete |
| Phase 8.5 LAN Agent + MCP adapter | complete |
| Phase 9 Orchestrator + Skills | complete |
| Phase 10 Benchmark/release gate | complete |

## Immediate Focus

1. Maintenance mode: ongoing improvements and bug fixes.

## What is behind

- Phase 7: complete.
- Phase 8: complete.
- Phase 8.5: complete.
- Phase 9: complete.
- Phase 10: complete.

## MCP Strategy

- MCP as external adapter layer (not core)
- Server mode: MultiLink serves MCP to Claude Desktop/Cursor
- Protocol own for LAN: MessagePack binary streaming

## Skills Strategy

- Global skills: ~/.local/share/multilink/skills/global/
- Project skills: .multilink/skills/ (never shared)
- Sharing: opt-in between active MultiLink users (not LAN nodes)
- Format: TOML

## Guardrails

- Keep core behavior stable; prefer small verifiable changes.
- Do not start Phase 9 before Phase 6 and Phase 8 are complete.
- Preserve credential privacy: no sharing Gemini/Codex/Claude credentials over LAN nodes.
- MCP as thin adapter layer, not core logic.

---

## 🔄 Actualización — 2026-03-17

### Nuevos Completados
- Context Engine v2 Plus: Observabilidad (context_latency_ms, hit_rate, truncation_rate)
- LSP Server: tower-lsp con tree-sitter para Rust, Python, JS/TS
- Benchmarks: lexical search (1k, 10k chunks), indexing (100, 1k files)

### En Progreso
- LSP Server Phase 6: Extensibilidad de lenguajes (Prisma/Dart/Bash pendientes por versión tree-sitter)
- LSP Server Phase 7: Tests y validación manual

### Notas
- SCALABILITY_PLAN.md actualizado: todos los hallazgos críticos resueltos
