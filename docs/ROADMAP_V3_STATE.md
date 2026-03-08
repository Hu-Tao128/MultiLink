# Roadmap V3 State

Last updated: 2026-03-07

CURRENT_PHASE = 8 (partial)
NEXT_PHASE = 6 (Context Engine v2)

## Status Summary

- Phase 1 Observability: complete.
- Phase 2 Config V2 + migration: complete.
- Phase 3 Model-aware budgeting: complete.
- Phase 4 Intent-aware budgeting: complete.
- Phase 5 Hardware-aware caps: complete.
- Phase 6 Context Engine v2: pending.
- Phase 7 Network security enforcement: partial.
- Phase 8 Multi-server router hardening: partial.
- Phase 9 Orchestrator: not started.
- Phase 10 Benchmark/release gate: not started.

## Immediate Focus

1. Close focused commit for `TaskWeight` + `remote_threshold` + `allow_remote_fallback` gating.
2. Implement Phase 6 in `core/src/context_retrieval.rs` (persistent index, semantic chunking, embedding cache, hybrid retrieval, evidence enforcement, compression) without changing `ChatRuntime` public API.
3. Return to remaining Phase 8 items (health monitor, circuit breaker, backoff, active model@server UI).
4. Complete remaining Phase 7 enforcement (`shared_secret`, `allowed_ips`) in runtime paths.

## Guardrails

- Keep core behavior stable; prefer small verifiable changes.
- Do not start Phase 9 before Phase 6 and Phase 8 are complete.
- Preserve credential privacy: no sharing Gemini/Codex/Claude credentials over LAN nodes.
