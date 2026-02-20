# Providers (`core/src/providers/`)

Unified provider interface and concrete implementations.

## Files

- `mod.rs`: `LLMProvider` trait, common types (`PromptOptions`, `LLMResponse`, errors, stream events).
- `ollama.rs`: local Ollama HTTP implementation with stream support.
- `gemini.rs`: remote provider integration scaffold.
- `codex.rs`: remote provider integration scaffold.

## How to extend

1. Add a new provider file implementing `LLMProvider`.
2. Register provider in router/bootstrap path.
3. Add tests for availability, send, and stream behavior.
