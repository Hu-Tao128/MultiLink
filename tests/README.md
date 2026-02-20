# Tests (`tests/`)

Top-level testing notes and strategy.

## Where tests live

- `core/tests/`: required integration tests for runtime, router, config, token store.
- `gui/rust/chat_controller` crate: FFI smoke/lifecycle tests.

## Current minimum

- Core tests are mandatory and run in CI/local checks.
- GUI visual tests are optional; smoke-run with offscreen platform is used for quick verification.

## Suggested PR checklist

1. Run `cargo test` in `core/`.
2. Run `cargo test` in `gui/rust/chat_controller/`.
3. Run `cmake --build build/gui` from repo root.
