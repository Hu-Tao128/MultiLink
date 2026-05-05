# `core/src/execution/` - Provider Dispatch

This module owns dispatch-time routing, fallback, concurrency limits, health status, and circuit-breaker behavior for provider calls.

## Agent Relevance

The coding agent should use this layer for model execution decisions, not for filesystem or command execution. Tool execution belongs in `core/src/tools/`; provider dispatch belongs here.

## Safety Notes

- Keep fallback behavior explicit and observable.
- Preserve bounded concurrency per server.
- Do not put business planning or editing logic in this module.
