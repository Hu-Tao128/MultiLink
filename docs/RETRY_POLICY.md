# MultiLink Retry / Fallback Policy

This document describes the four retry layers in the MultiLink backend and their responsibilities.
Each layer has a distinct scope; duplication between layers is intentional where documented.

---

## Layer 1 — Provider non-streaming retry (`OllamaProvider::post_chat_with_retry`)

| Property | Value |
|---|---|
| File | `core/src/providers/ollama.rs` |
| Triggered by | `provider.send()` (non-streaming chat) |
| Attempts | `RETRY_ATTEMPTS = 3` |
| Backoff | 200ms base, exponential, no cap |
| Scope | Single provider instance, single HTTP request |
| Retryable errors | Connection refused, 5xx, timeout |

Handles transient Ollama HTTP errors at the lowest level before the response
reaches the router. Does **not** apply to streaming requests.

---

## Layer 2 — Provider streaming retry (`OllamaProvider::stream_send`)

| Property | Value |
|---|---|
| File | `core/src/providers/ollama.rs` |
| Triggered by | `provider.stream_send()` |
| Attempts | `stream_retries() + 1` (default: 5) |
| Backoff | 500ms base, exponential, no cap |
| Scope | Single provider instance, single stream request |
| Retryable errors | Idle timeout, mid-stream disconnect |

Retries the full streaming request when a long-running generation is interrupted.
`router.stream_send()` calls the provider's `stream_send()` directly and does
**not** wrap it in `send_with_retry()` — stream retries are exclusively owned
here and must not be duplicated at the router layer.

---

## Layer 3 — Router non-streaming retry (`ProviderRouter::send_with_retry`)

| Property | Value |
|---|---|
| File | `core/src/router.rs` |
| Triggered by | `router.send()` |
| Attempts | `DEFAULT_MAX_RETRIES + 1 = 4` |
| Backoff | 500ms base, exponential, capped at 5000ms |
| Scope | One provider, multiple retries at the router level |
| Retryable errors | Determined by `is_retryable_error()` |

Provides a second chance at the routing layer for non-streaming requests,
independent of the provider's internal retry loop. This layer can try the same
provider multiple times before the router gives up and the dispatcher fallback
loop kicks in.

**Not called from `stream_send()`** — see Layer 2.

---

## Layer 4 — Dispatcher fallback loop (`ExecutionDispatcher::dispatch`)

| Property | Value |
|---|---|
| File | `core/src/execution/dispatcher.rs` |
| Triggered by | Primary router path failure |
| Attempts | One attempt per enabled `execution_server` |
| Backoff | 150ms base, exponential, capped at 1200ms |
| Scope | Cross-server: tries each remote execution server once |
| Guard conditions | `allow_remote_fallback = true`, non-loopback primary, non-empty server list |

After the primary router path (Layers 1–3) fails, the dispatcher iterates the
configured `execution_servers` list as fallback targets. Each server has its own
circuit breaker — circuit-open servers are skipped without a connection attempt.

**Key guard: loopback suppression.**
If the registered Ollama URL is `127.0.0.1`, `::1`, or `localhost`, the fallback
loop is bypassed unconditionally. A local Ollama failure cannot be resolved by
a remote server and the primary error is returned immediately.

---

## Worst-case attempt count

Without loopback suppression, a single user request could trigger:

```
Layer 1 retries  ×  Layer 2 retries  ×  Layer 3 retries  ×  Layer 4 servers
      3          ×        5          ×        4           ×      N servers
```

In practice:
- Non-streaming path: max `3 × 4 = 12` attempts before dispatcher fallback.
- Streaming path: max `5` attempts per provider; Layer 3 is not involved.
- Dispatcher fallback adds one attempt per configured `execution_server`.

To reduce max latency on hard failures, consider lowering `RETRY_ATTEMPTS`
(Layer 1) from 3 to 2 if the total end-to-end timeout is unacceptable.
