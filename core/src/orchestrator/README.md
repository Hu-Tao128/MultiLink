# `core/src/orchestrator/` - Coding-Agent Planning and Execution

This module coordinates tool use, context retrieval, provider selection, and final response synthesis. It is the right place for coding-agent workflow logic.

## Files

- `planner.rs`: classifies intent and builds simple tool/LLM plans.
- `executor.rs`: runs plan steps, stores tool outputs, falls back to Context Engine when a tool fails, and calls providers for synthesis.
- `provider_selector.rs`: chooses a provider based on required capabilities.
- `old_tools.rs`: compatibility helpers for Context Engine and skills.
- `tool.rs`, `tool_registry.rs`, `tools/`: earlier/parallel tool abstractions kept for compatibility; prefer `core/src/tools/` for the active registry.

## Current Behavior

- Read/search/system-info prompts can route to tools first.
- General analysis still often falls back to LLM-only or Context Engine retrieval.
- Multi-step execution is bounded by `DEFAULT_MAX_STEPS`.
- Tool output is serialized and passed to the LLM for synthesis when needed.

## Required Agent Upgrades

- Add explicit edit steps instead of relying on model prose.
- Add validation steps after edits.
- Track changed files and command outcomes in step metadata.
- Prefer structured final reports: files changed, tests run, failures, residual risks.
- Remove duplicate/legacy tool abstractions once the active registry is stable.
