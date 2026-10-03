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
- `GeminiProvider`: OAuth bearer token + HTTP endpoint scaffold.
- `CodexProvider`: OAuth bearer token + HTTP endpoint scaffold.
- `DeepSeekProvider`: OpenAI-compatible Chat Completions API (`https://api.deepseek.com`),
  streaming SSE with `data: [DONE]`, Bearer auth, reasoning support.

At runtime today, the GUI bootstrap registers Ollama by default. Remote provider registration and auth lifecycle wiring are tracked as next integration work.

## DeepSeek

DeepSeek speaks the same wire format as OpenAI, so it needs no custom SDK:

- Base URL: `https://api.deepseek.com` (`MULTILINK_DEEPSEEK_BASE_URL`)
- Auth: `Authorization: Bearer <DEEPSEEK_API_KEY>` (also storable from the GUI token store)
- Endpoint: `POST /chat/completions`
- Models: `deepseek-chat` (V3, fast) and `deepseek-reasoner` (R1, emits
  `reasoning_content`, which is surfaced as `<think>...</think>` tokens).

The provider omits `temperature` for reasoner models because the API rejects a
custom value there. Enable it by adding a `deepseek` server to the config:

```toml
[[servers]]
name = "DeepSeek"
provider = "deepseek"
base_url = "https://api.deepseek.com"
default_model = "deepseek-chat"
priority = 2
enabled = true
```

`DEEPSEEK_API_KEY`, `DEEPSEEK_BASE_URL` and `DEEPSEEK_MODEL` are read directly
by `DeepSeekProvider::from_env()`.

## Tooling strategy by model size

The orchestrator adapts its tool loop to the selected model:

- **Small** models get a reduced allow-list, 5 steps, no command execution and
  a single-tool JSON contract.
- **Medium** models may write/patch/run commands and validate after writes.
- **Large** models (including `deepseek-chat`, `deepseek-reasoner`, `gpt-4/5`,
  Claude, 70B+) get the full tool set, 20 steps, richer tool descriptions and
  guidance to gather evidence and validate changes.

Provider selection is capability-aware: reasoning prompts prefer providers that
advertise `supports_thinking` (e.g. `deepseek-reasoner`) even over a local model.


## Fallback policy

- Preferred provider is attempted first.
- If unavailable, router tries registered alternatives.
- Local provider is the safety fallback when remote fails.

## Error model

- `NotConfigured`, `Unavailable`, `Timeout`, `RateLimited`, `Http`, `Serialization`, `Unexpected`.
- Errors are typed for clean UI mapping and user-friendly messages.
