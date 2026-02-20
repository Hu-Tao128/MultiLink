# Model Manager (`core/src/model_manager/`)

Model inventory and model-source helpers.

## Files

- `registry.rs`: in-memory model registry (`ModelInfo`, status, totals, active model).
- `registry_source.rs`: model sources (HTTP / npm-like) and conversion into registry entries.
- `ollama.rs`: local Ollama model directory operations (detect/list/migrate/delete).
- `npm.rs`: npm-like manager placeholder.
- `mod.rs`: exports.

## Intent

Core owns model metadata and storage behavior so UI remains stateless.
