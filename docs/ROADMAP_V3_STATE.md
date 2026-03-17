# Roadmap V3 State

Last updated: 2026-03-16

CURRENT_PHASE = release
NEXT_PHASE = maintenance

## Status Summary

- Phase 1 Observability: complete.
- Phase 2 Config V2 + migration: complete.
- Phase 3 Model-aware budgeting: complete.
- Phase 4 Intent-aware budgeting: complete.
- Phase 5 Hardware-aware caps: complete.
- Phase 6 Context Engine v2: complete (integrated with evidence enforcement).
- Phase 7 Network security enforcement: complete (IP filtering, shared secret, remote access control).
- Phase 8 Multi-server router hardening: complete (health monitor, circuit breaker, backoff).
- Phase 8.5 LAN Agent + MCP adapter: complete.
- Phase 9 Orchestrator + Skills: complete.
- Phase 10 Benchmark/release gate: complete.
- Phase 11 Maintenance: in progress.

## Immediate Focus

1. Phase 8.5: LAN Agent + MCP adapter implementation.
   - [x] Step 1: LAN Agent protocol structure (MessagePack envelope/payload)
   - [x] Step 2: MCP adapter basic tool conversion
   - [x] Step 3: Implement LAN Agent TCP server for MessagePack streaming
   - [x] Step 4: Implement MCP adapter response handling
   - [x] Step 5: Integrate LAN Agent with ChatRuntime
2. Phase 9: Orchestrator + Skills
   - [x] Step 1: Create Skill struct and SkillManifest in TOML format
   - [x] Step 2: Implement skill loader (global + project paths)
   - [x] Step 3: Create SkillOrchestrator for skill selection/execution
   - [x] Step 4: Integrate with ChatRuntime for skill-based prompts
   - [x] Step 5: Add skill sharing mechanism (opt-in)
3. Phase 10: Benchmark/release gate
   - [x] Step 1: Create benchmark suite for latency/throughput
   - [x] Step 2: Add release criteria thresholds
   - [x] Step 3: Create release checklist
   - [x] Step 4: Add CI smoke tests

## Recovery Plan (Pre-test execution)

- [x] CI stabilization for GUI build matrix (Linux/macOS/Windows) with smoke-run fallback when CTest suites are not present.
- [x] Phase 8 hardening closure: health monitor, circuit breaker, backoff/retry policy.
- [x] Phase 8.5 bootstrap: LAN Agent protocol (MessagePack streaming) + MCP thin adapter (no core logic move).
- [x] Exit criteria definition before Phase 9 start: runtime fallback validated, network security hooks connected, adapter E2E request/response path verified.

## What is behind

- Phase 7 is still partial because enforcement completion depends on 8.5 integration.
- Phase 8: complete.
- Phase 8.5: complete.
- Phase 9: complete.
- Phase 10: in progress.

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
