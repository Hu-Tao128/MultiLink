# `scripts/` - Developer Utilities

This directory contains helper scripts for documentation and validation workflows.

## Current Role

Scripts are support tooling, not runtime agent tools. If a script becomes part of agent execution, wrap it behind a guarded Rust tool with explicit input validation and tests.

## Agent Guidance

- Prefer Rust tools in `core/src/tools/` for runtime behavior.
- Keep scripts reproducible and documented with command-line examples.
- Do not rely on scripts for hidden side effects during chat/runtime execution.
