# Core (`core/`)

Rust library crate with all business logic.

## What this folder contains

- `Cargo.toml`: crate dependencies and build config.
- `src/`: runtime, providers, auth, model management, config, system utilities.
- `tests/`: core integration tests.

## How to work here

- Run checks: `cargo check`
- Run tests: `cargo test`
- Cross-platform builds (Linux, macOS, Windows) are automatically verified via GitHub Actions CI/CD.
- Keep UI logic out of this crate.
- Prefer performance-safe defaults (bounded context, throttled persistence, non-blocking stream handling).

## Current core priorities

- Prevent provider-facing failures caused by oversized context payloads.
- Keep provider health signaling accurate (connection failures vs prompt/content failures).
- Keep streaming robust under partial network frames and long-running responses.

## PR guidance

- Add or update tests when changing runtime/provider behavior.
- Keep async and persistence in core, not GUI.
