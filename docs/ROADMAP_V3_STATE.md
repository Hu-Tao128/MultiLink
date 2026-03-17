# Roadmap V3 State

Last updated: 2026-03-07

CURRENT_PHASE = 8.5 (LAN Agent + MCP adapter)
NEXT_PHASE = 9 (Orchestrator + Skills)

## Status Summary

- Phase 1 Observability: complete.
- Phase 2 Config V2 + migration: complete.
- Phase 3 Model-aware budgeting: complete.
- Phase 4 Intent-aware budgeting: complete.
- Phase 5 Hardware-aware caps: complete.
- Phase 6 Context Engine v2: complete (integrated with evidence enforcement).
- Phase 7 Network security enforcement: partial (config available, enforcement depends on Phase 8.5).
- Phase 8 Multi-server router hardening: complete (health monitor, circuit breaker, backoff).
- Phase 8.5 LAN Agent + MCP adapter: in progress.
- Phase 9 Orchestrator + Skills: not started.
- Phase 10 Benchmark/release gate: not started.

## Immediate Focus

1. Phase 8.5: LAN Agent + MCP adapter implementation.
   - [x] Step 1: LAN Agent protocol structure (MessagePack envelope/payload)
   - [x] Step 2: MCP adapter basic tool conversion
   - [x] Step 3: Implement LAN Agent TCP server for MessagePack streaming
   - [ ] Step 4: Implement MCP adapter response handling
   - [ ] Step 5: Integrate LAN Agent with ChatRuntime
2. Phase 9: Orchestrator + Skills
3. Phase 10: Benchmark/release gate

## Recovery Plan (Pre-test execution)

- [ ] CI stabilization for GUI build matrix (Linux/macOS/Windows) with smoke-run fallback when CTest suites are not present.
- [ ] Phase 8 hardening closure: health monitor, circuit breaker, backoff/retry policy.
- [ ] Phase 8.5 bootstrap: LAN Agent protocol (MessagePack streaming) + MCP thin adapter (no core logic move).
- [ ] Exit criteria definition before Phase 9 start: runtime fallback validated, network security hooks connected, adapter E2E request/response path verified.

## What is behind

- Phase 7 is still partial because enforcement completion depends on 8.5 integration.
- Phase 8 is partial and blocks the guardrail to start Phase 9.
- Phase 8.5 is not started and is currently the main critical path item.
- Phase 9 and 10 are blocked by design (not sequencing errors, but dependency debt).

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
