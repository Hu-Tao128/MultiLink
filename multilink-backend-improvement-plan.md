# MultiLink Backend Improvement Plan

## Overview

This plan addresses a confirmed routing bug that causes **unnecessary fallback/retry when Ollama is running on the same machine (127.0.0.1)**, plus a set of structural, security, and reliability issues found during a full codebase audit. The goal is to stabilize the local-first execution path, tighten security posture, fix bad practices, and improve maintainability without breaking existing behavior.

---

## Root Cause: Loopback Fallback Bug

The `ExecutionDispatcher` (`core/src/execution/dispatcher.rs`) has **two separate circuit-breaker systems** that don't share state:

1. **Router-level** (`ProviderRouter` in `router.rs`) — has `CircuitBreakerConfig` with 3-failure threshold and 30s recovery.
2. **Dispatcher-level** (`ExecutionDispatcher` in `dispatcher.rs`) — has `CircuitState` with 2-failure threshold and 20s recovery.

When the router's `stream_send()` is called (line 85–92 of `dispatcher.rs`) and it returns `Err`, the dispatcher immediately enters the fallback loop (lines 119–187) and tries the `execution_servers` list — **even when the request was against `127.0.0.1`** (local Ollama). This happens because:

- The `allow_remote_fallback` flag is set to `true` by default in `chat_runtime.rs`.
- The dispatcher does **not check whether the primary failure was on a loopback address** before entering the remote fallback loop.
- The `resolved_server_url` filter (lines 122–126) is only active when a model-specific URL is explicitly set, otherwise it's `None` and **all servers are tried**.

Additionally, `is_available_async()` in `ollama.rs` (lines 619–636) uses a raw `TcpStream::connect()` with **no timeout**, meaning on a slow or busy loopback it can hang until the OS TCP stack times out (typically 75s–2min), triggering the circuit breaker prematurely.

---

## Sub-Tasks

---

### Sub-Task 1 — Fix: Local Loopback Must Not Trigger Remote Fallback

**Status:** `[ ] pending`

**Intent:**
When the primary Ollama target is a loopback address (`127.0.0.1` or `::1`), a transient error should never cause the dispatcher to try a remote execution server. Remote fallback only makes sense when the primary is itself a remote or unreachable host.

**Expected Outcomes:**
- A local Ollama failure on `127.0.0.1` returns the error directly to the caller without attempting remote servers.
- Remote fallback continues to work normally when the primary server is a non-loopback host.
- Existing fallback tests in `execution_dispatcher_tests.rs` are updated/extended to cover this case.

**Todo List:**
1. Add a helper function `is_loopback_url(url: &str) -> bool` in `dispatcher.rs` that checks if the host portion of a URL resolves to `127.0.0.1`, `::1`, or the string `localhost`.
2. In `dispatcher.dispatch()`, after `resolved_server_url` is determined, also compute `primary_is_local: bool` by calling `is_loopback_url` on the primary Ollama base URL (obtainable from `router` or passed in via `ExecutionDispatchRequest`).
3. In the `Err(primary_err)` branch (line 106), add `|| primary_is_local` to the early-return condition so that local failures short-circuit the fallback loop.
4. Add a test: "dispatcher does not fall back to remote server when primary is loopback and fails".

**Relevant Context:**
- `core/src/execution/dispatcher.rs:106–115` — the early-return condition to extend.
- `core/src/execution/dispatcher.rs:69–76` — `resolved_server_url` resolution; `primary_is_local` should be derived here.
- `core/src/chat_runtime.rs:1105–1113` — heuristic that already uses `contains("127.0.0.1")`, same logic needed upstream in dispatcher.

---

### Sub-Task 2 — Fix: Add Timeout to `is_available_async()`

**Status:** `[ ] pending`

**Intent:**
The async availability check in `ollama.rs` uses `tokio::net::TcpStream::connect()` with no timeout. On a busy loopback or when the port is filtered (not refused), this can block indefinitely, causing the router health monitor and the dispatch path to stall, which then erroneously trips the circuit breaker.

**Expected Outcomes:**
- `is_available_async()` always returns within a bounded time (≤ 700ms, matching the sync variant).
- The health monitor and pre-dispatch check never stall.

**Todo List:**
1. Wrap the `tokio::net::TcpStream::connect(socket_addr)` call inside `tokio::time::timeout(Duration::from_millis(700), ...)` in `is_available_async()` (lines 619–636 of `ollama.rs`).
2. Return `false` on timeout, matching the existing `Err(_) => false` branch.
3. Add a unit test or note in `ollama_provider_tests.rs` validating the timeout behavior.

**Relevant Context:**
- `core/src/providers/ollama.rs:619–636` — `is_available_async()` implementation.
- `core/src/providers/ollama.rs:602–617` — `is_available()` sync variant already uses `Duration::from_millis(700)`.

---

### Sub-Task 3 — Fix: Dual Circuit-Breaker State Desync

**Status:** `[ ] pending`

**Intent:**
Two independent circuit-breaker implementations (`ProviderRouter` and `ExecutionDispatcher`) track failures separately. A single Ollama error can trip both independently, leading to over-eager blocking and confusing recovery behavior. The router's circuit breaker should be the single source of truth for the Ollama provider health state.

**Expected Outcomes:**
- The `ExecutionDispatcher`'s per-server circuit state is only used for named `execution_servers` (remote servers), not for the primary router path.
- The primary router path failure does not interact with the `ExecutionDispatcher` circuits.
- No duplicate failure-tracking for the same server.

**Todo List:**
1. Remove the `record_status(primary_key, false, ...)` call on dispatcher line 107 that records a failure against `"primary-router"` — this is redundant with the router's own `record_failure()` call inside `router.rs`.
2. Keep the dispatcher circuit state only for the `execution_servers` entries (lines 119–187), which is correct.
3. Document in code comments that `ExecutionDispatcher.circuits` tracks only remote execution servers, not the primary router.

**Relevant Context:**
- `core/src/execution/dispatcher.rs:107` — redundant `record_status` on primary failure.
- `core/src/router.rs:242–255` — `record_failure()` already tracks this.
- `core/src/execution/dispatcher.rs:43–44` — `CIRCUIT_FAIL_THRESHOLD` and `CIRCUIT_OPEN_SECS` for dispatcher circuits.

---

### Sub-Task 4 — Fix: `is_port_open()` Panics on Malformed Address

**Status:** `[ ] pending`

**Intent:**
In `config.rs:63`, a nested `.unwrap()` in the fallback of `addr.parse()` can theoretically panic if the hardcoded fallback string `"127.0.0.1:0"` ever fails to parse (which should not happen, but is a bad pattern that hides errors and will fail a future unwrap-audit). Additionally, IPv6 addresses like `"::1:11434"` are not valid socket address format — they need brackets: `"[::1]:11434"`. The current code constructs IPv6 addresses correctly for the URL but may fail at the parse step.

**Expected Outcomes:**
- `is_port_open()` returns `false` instead of panicking on any parse failure.
- IPv6 address construction for the socket parse is confirmed correct with brackets.

**Todo List:**
1. In `is_port_open()` (`config.rs:58–67`), change the `.unwrap_or_else(|_| "127.0.0.1:0".parse().unwrap())` to return `false` early if `addr.parse::<std::net::SocketAddr>()` fails.
2. Verify that when `host = "::1"`, the format string `"{}:{}"` produces `"::1:11434"` which is NOT a valid SocketAddr — it needs to be `"[::1]:11434"`. Fix the format to bracket IPv6 hosts: detect IPv6 and use `"[{}]:{}"`.
3. Add a test or inline comment verifying the parse for both IPv4 and IPv6 forms.

**Relevant Context:**
- `core/src/config.rs:58–67` — `is_port_open()`.
- `core/src/config.rs:25–33` — IPv6 detection loop that constructs `"::1"` as host.
- `std::net::SocketAddr` parsing rules: IPv6 literals must be bracketed.

---

### Sub-Task 5 — Security: Restrict Config File Permissions for LAN Secret

**Status:** `[ ] pending`

**Intent:**
The LAN shared secret is stored in plain text in `~/.config/multilink/config.toml`. On Linux, depending on the system umask, this file may be readable by all users (`0644`). Since the secret provides full access to the LAN agent (including executing tools), it must only be readable by the owning user.

**Expected Outcomes:**
- Config file is written with mode `0600` (owner read/write only) on Unix systems.
- Existing config files are not retroactively changed (only new writes enforce the permission).
- On non-Unix platforms (Windows), the behavior is unchanged.

**Todo List:**
1. In `config.rs`, in the `save()` or `write` function that serializes the config to disk, use `std::fs::set_permissions()` with `0o600` mode immediately after writing on `cfg(unix)` targets.
2. Alternatively, open the file with `OpenOptions` setting mode `0o600` before writing, using the `std::os::unix::fs::OpenOptionsExt` trait.
3. Document in a comment why the restrictive permission is necessary.

**Relevant Context:**
- `core/src/config.rs` — the `save()` / write method for `AppConfig`.
- The generated secret is at `core/src/config.rs:557–562`.

---

### Sub-Task 6 — Security: Validate LAN Envelope Timestamp Against Replay Attacks

**Status:** `[ ] pending`

**Intent:**
`LanEnvelope::validate()` in `lan_agent.rs` checks that the timestamp is not too far in the **future** (line 39: `MAX_CLOCK_SKEW_MS = 300_000`, 5 minutes). However, it does **not** reject messages with timestamps in the **past** — a captured signed message can be replayed indefinitely. A replay window check (reject if timestamp is older than 5 minutes in the past) would close this attack vector.

**Expected Outcomes:**
- Messages older than `MAX_CLOCK_SKEW_MS` milliseconds from the current time are rejected with an appropriate error.
- Existing ping tests are not broken (they should use fresh timestamps).
- A test is added for the replay rejection case.

**Todo List:**
1. In `LanEnvelope::validate()` in `lan_agent.rs`, after the future-timestamp check, add a check: if `now_ms - timestamp_ms > MAX_CLOCK_SKEW_MS`, return `Err("message expired: replay attack window exceeded")`.
2. Handle the case where `now_ms < timestamp_ms` gracefully (already handled by future-check).
3. Add a test: "envelope with stale timestamp is rejected".

**Relevant Context:**
- `core/src/lan_agent.rs:32–50` — `LanEnvelope::validate()`.
- `core/src/lan_agent.rs:30` — `MAX_CLOCK_SKEW_MS = 300_000`.
- `core/src/lan_agent.rs:478–562` — existing security tests to extend.

---

### Sub-Task 7 — Security: Rate-Limit LAN Agent Connections

**Status:** `[ ] pending`

**Intent:**
The LAN agent's `run()` loop (lines 198–230 of `lan_agent.rs`) accepts connections without any rate limiting. An attacker on the local network (or a misconfigured device) can flood the server with connection attempts, consuming resources or causing legitimate requests to time out. A simple connection-rate limiter (per source IP) prevents this without requiring complex infrastructure.

**Expected Outcomes:**
- No single source IP can open more than N connections per second (configurable, default: 20/s).
- Excess connections are dropped at the accept stage with no processing.
- The rate limit is logged at debug level when triggered.

**Todo List:**
1. Add a `RateLimiter` struct (or use a simple `HashMap<IpAddr, (u32, Instant)>`) inside `LanAgentServer` to track per-IP connection counts within a sliding window.
2. In `run()`, after extracting the peer IP, check the rate limiter before spawning a handler task.
3. If the IP exceeds the threshold, close the socket without reading and log the event.
4. Make the limit configurable via `NetworkConfig` with a sensible default.

**Relevant Context:**
- `core/src/lan_agent.rs:198–234` — `run()` connection accept loop.
- `core/src/config.rs` — `NetworkConfig` struct to extend with `lan_rate_limit_per_sec`.

---

### Sub-Task 8 — Reliability: Unify and Document the Retry/Fallback Strategy

**Status:** `[ ] pending`

**Intent:**
There are currently **four different retry mechanisms** operating at different layers with no shared policy:

| Layer | Retries | Backoff | Location |
|---|---|---|---|
| `OllamaProvider.post_chat_with_retry()` | 3 | 200ms exponential | `ollama.rs:378` |
| `OllamaProvider.stream_send()` | 4+1 | 500ms exponential | `ollama.rs:741` |
| `ProviderRouter.send_with_retry()` | 3 | 500ms exponential | `router.rs:443` |
| `ExecutionDispatcher` fallback loop | per-server | 150ms exponential | `dispatcher.rs:182` |

In the worst case, a single user request can trigger up to **3 × 5 × 3 = 45 attempts** (provider retries × stream retries × router retries) before the dispatcher even enters the fallback loop. This produces very long hangs on error paths. The intent of each layer needs to be documented and deduplicated.

**Expected Outcomes:**
- Each retry layer has a documented responsibility comment.
- The `ProviderRouter.send_with_retry()` is not called in the `stream_send` path (streams handle their own retries in `ollama.rs`), confirmed with a code comment to prevent future duplication.
- A `RETRY_POLICY.md` file in `docs/` summarizes the intended retry strategy for contributors.

**Todo List:**
1. Add a `// RETRY POLICY:` block comment above each retry loop explaining its scope and why it exists at that layer.
2. Audit whether `router.stream_send()` calls `send_with_retry()` — if not, add a comment explicitly noting that stream retries are owned by the provider. If it does, remove the duplication.
3. Write `docs/RETRY_POLICY.md` documenting the four layers, their thresholds, and the total worst-case attempt count.
4. Consider reducing `OllamaProvider::RETRY_ATTEMPTS` from 3 to 2 to halve the max latency on hard failures, and update tests accordingly.

**Relevant Context:**
- `core/src/providers/ollama.rs:36,378,741` — provider-level retries.
- `core/src/router.rs:443–471` — router-level retry.
- `core/src/execution/dispatcher.rs:119–187` — dispatcher-level fallback.

---

### Sub-Task 9 — Structure: Folder Reorganization for Scalability

**Status:** `[ ] pending`

**Intent:**
The `core/src/` directory currently mixes transport, domain logic, infrastructure, and utilities at the same level. As the project grows, adding providers or execution backends will make navigation harder. Reorganizing into clear layers makes boundaries explicit and prevents cross-layer imports.

**Expected Outcomes:**
- `core/src/` follows a layered structure: `transport/`, `providers/`, `execution/`, `routing/`, `session/`, `config/`, `security/`, and `observability/`.
- All existing modules compile and tests pass after the move.
- `lib.rs` re-exports public APIs without exposing internal modules.

**Proposed New Structure:**
```
core/src/
├── config/           # config.rs + migration logic
├── providers/        # ollama.rs, gemini.rs, codex.rs (already exists, keep)
├── execution/        # dispatcher.rs (already exists, keep)
├── routing/          # router.rs → routing/mod.rs
├── session/          # session.rs, chat_runtime.rs
├── security/         # lan_agent.rs, auth/ (merge)
├── context_engine/   # (already exists, keep)
├── orchestrator/     # (already exists, keep)
├── tools/            # (already exists, keep)
├── observability/    # observability.rs, metrics
└── lib.rs
```

**Todo List:**
1. Create `core/src/routing/` and move `router.rs` into it as `mod.rs`.
2. Create `core/src/session/` and move `session.rs` and `chat_runtime.rs` into it.
3. Create `core/src/security/` and move `lan_agent.rs` and `auth/` into it.
4. Update `lib.rs` re-exports and fix all `use crate::` paths throughout.
5. Run `cargo build` and `cargo test` after each file move to catch path breakage early.

**Relevant Context:**
- `core/src/lib.rs` — top-level module declarations.
- `AGENTS.md` — architecture boundary rules: "core/ owns runtime, routing, providers, persistence, auth".

---

### Sub-Task 10 — Quality: Enable unwrap() Linting in CI

**Status:** `[ ] pending`

**Intent:**
The CI pipeline in `.github/workflows/rust.yml` has an unwrap-detection command that is commented out (line 35). Meanwhile, `core/src/config.rs:63` contains a nested `.unwrap()` that can panic at runtime. Enabling the lint prevents future regressions.

**Expected Outcomes:**
- The CI step `cargo clippy -- -D warnings` enforces no production-path unwrap().
- The nested unwrap in `config.rs:63` is replaced with a safe alternative (covered by Sub-Task 4).
- The CI configuration comment is replaced with an active `cargo clippy` step.

**Todo List:**
1. Uncomment and update the lint step in `.github/workflows/rust.yml` to run `cargo clippy --manifest-path core/Cargo.toml -- -D warnings`.
2. Fix the remaining unwrap in `config.rs:63` (handled in Sub-Task 4).
3. Verify all tests pass with `cargo test --manifest-path core/Cargo.toml`.

**Relevant Context:**
- `.github/workflows/rust.yml:32–36` — disabled unwrap check.
- `core/src/config.rs:63` — the only remaining production-path unwrap.
- `AGENTS.md` — "No unwrap(): Never unwrap() in production-path logic".

---

## Dependency Map

```
Sub-Task 4  ──► Sub-Task 10   (fix unwrap first, then enable lint)
Sub-Task 1  ──► Sub-Task 3    (local loopback fix clarifies circuit state ownership)
Sub-Task 8  (independent, documentation + minor reduction)
Sub-Task 2  (independent, one-line fix)
Sub-Task 5  (independent, file permission)
Sub-Task 6  (independent, security fix)
Sub-Task 7  (independent, rate limiting)
Sub-Task 9  (independent but high-risk, do last)
```

**Recommended execution order:** 2 → 4 → 1 → 3 → 6 → 5 → 7 → 10 → 8 → 9
