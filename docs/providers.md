# Providers

## Unified contract

All providers implement the `LLMProvider` trait:

```rust
trait LLMProvider {
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
    async fn send(&self, prompt: String, options: PromptOptions) -> Result<LLMResponse, LLMError>;
}
```

The core also supports `stream_send(...)` for token-by-token updates.

## Current providers

- `OllamaProvider`: local HTTP API, streaming enabled.
- `GeminiProvider`: OAuth bearer token + HTTP endpoint.
- `CodexProvider`: OAuth bearer token + HTTP endpoint.

## Fallback policy

- Preferred provider is attempted first.
- If unavailable, router tries registered alternatives.
- Local provider is the safety fallback when remote fails.

## Error model

- `NotConfigured`, `Unavailable`, `Timeout`, `RateLimited`, `Http`, `Serialization`, `Unexpected`.
- Errors are typed for clean UI mapping and user-friendly messages.
