# Roadmap V3 State

Last updated: 2026-04-02

CURRENT_PHASE = release
NEXT_PHASE = release

## Canonical Status

This file is the single source of truth for roadmap status.

## Phase Status

| Phase | Status |
|-------|--------|
| Phase 1 Observability | complete |
| Phase 2 Config V2 + migration | complete |
| Phase 3 Model-aware budgeting | complete |
| Phase 4 Intent-aware budgeting | complete |
| Phase 5 Hardware-aware caps | complete |
| Phase 6 Context Engine v2/v2plus | complete |
| Phase 7 Network security enforcement | complete |
| Phase 8 Multi-server router hardening | complete |
| Phase 8.5 LAN Agent + MCP adapter | complete |
| Phase 9 Orchestrator + Skills | complete |
| Phase 10 Benchmark/release gate | complete |
| LSP Phase 6 (language extensibility) | in_progress |
| LSP Phase 7 (manual QA/perf) | in_progress |

## Immediate Focus

1. Close doc/code consistency gaps across Context Engine and LSP plans.
2. Finish v2plus Step 6 A/B validation and promotion criteria.
3. Close LSP manual validation and profiling tasks.

## What is behind

- Context Engine v2plus Step 6: pending validation evidence.
- LSP Phase 6: Prisma/Dart/Bash pending by tree-sitter version compatibility.
- LSP Phase 7: manual validation and profiling pending.

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

## 🔄 Actualización — 2026-03-18

### Reconciliación de estado
- `docs/LSP_ROADMAP.md` y `docs/context_engine_v2plus_plan.md` prevalecen para tareas en progreso.
- Se removió el estado "all phases complete" para evitar contradicciones.

### En Progreso
- LSP Server Phase 6: Extensibilidad de lenguajes (Prisma/Dart/Bash pendientes por versión tree-sitter)
- LSP Server Phase 7: Tests y validación manual
- Context Engine v2plus Step 6: Observabilidad + A/B con criterios de promoción/rollback

### Notas
- Este archivo queda como fuente canónica; al actualizar estados, sincronizar en la misma PR:
  - `docs/LSP_ROADMAP.md`
  - `docs/context_engine_v2plus_plan.md`
  - `docs/CONTEXT_ENGINE_ROADMAP.md`
