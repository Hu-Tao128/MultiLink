# Providers (`core/src/providers/`)

Unified provider interface and concrete implementations.

## Files

- `mod.rs`: `LLMProvider` trait, common types (`PromptOptions`, `LLMResponse`, errors, stream events).
- `ollama.rs`: local Ollama HTTP implementation with stream support, explicit system-message forwarding, timeout tuning, and chunk-safe stream parsing.
- `gemini.rs`: remote provider integration scaffold.
- `codex.rs`: remote provider integration scaffold.

## Current contracts

- Providers should return detailed HTTP errors when possible (status + body) to help runtime classify failures.
- `PromptOptions.num_ctx` defaults to `None`; runtime decides when to constrain context budgets.
- Stream implementations must tolerate transport chunk boundaries and avoid assuming one JSON object per TCP frame.

## How to extend

1. Add a new provider file implementing `LLMProvider`.
2. Register provider in router/bootstrap path.
3. Add tests for availability, send, and stream behavior.
