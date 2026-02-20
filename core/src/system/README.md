# System (`core/src/system/`)

OS/system-level helpers (non-UI).

## Files

- `mod.rs`: detects Ollama installation, describes install command plan, verifies install result.

## Safety contract

- Never run privileged commands without explicit user consent.
- Keep this module deterministic and side-effect-aware.
