# Roadmap V3 State

Last updated: 2026-03-07

CURRENT_PHASE = 7 (Network security enforcement)
NEXT_PHASE = 8 (Router hardening)

## Status Summary

- Phase 1 Observability: complete.
- Phase 2 Config V2 + migration: complete.
- Phase 3 Model-aware budgeting: complete.
- Phase 4 Intent-aware budgeting: complete.
- Phase 5 Hardware-aware caps: complete.
- Phase 6 Context Engine v2: complete (integrated with evidence enforcement).
- Phase 7 Network security enforcement: partial (config available, enforcement depends on Phase 8.5).
- Phase 8 Multi-server router hardening: partial.
- Phase 8.5 LAN Agent + MCP adapter: not started.
- Phase 9 Orchestrator + Skills: not started.
- Phase 10 Benchmark/release gate: not started.

## Immediate Focus

1. Complete remaining Phase 8 items (health monitor, circuit breaker, backoff).
2. Phase 8.5: LAN Agent protocol implementation.
3. MCP adapter as thin external layer.

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
