# `core/src/orchestrator/tools/` - Legacy Tool Adapters

This folder contains an older tool abstraction used by the orchestrator module. The active internal tool registry is `core/src/tools/`.

## Guidance

- Prefer adding new production tools under `core/src/tools/`.
- Keep this folder only for compatibility until duplicate abstractions are removed.
- Do not add new write or command execution behavior here.
