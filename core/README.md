# Core (`core/`)

Rust library crate with all business logic.

## What this folder contains

- `Cargo.toml`: crate dependencies and build config.
- `src/`: runtime, providers, auth, model management, config, system utilities.
- `tests/`: core integration tests.

## How to work here

- Run checks: `cargo check`
- Run tests: `cargo test`
- Keep UI logic out of this crate.

## PR guidance

- Add or update tests when changing runtime/provider behavior.
- Keep async and persistence in core, not GUI.
