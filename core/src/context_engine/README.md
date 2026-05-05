# `core/src/context_engine/` - Project Context Retrieval

This module builds relevant project context for chat and coding-agent workflows. It owns indexing, chunking, lexical search, optional embeddings, retrieval scoring, and compression.

## Main Responsibilities

- Tokenize and index source files.
- Extract semantic chunks for supported languages.
- Retrieve relevant files/snippets within a token budget.
- Support lexical-only operation when embeddings are disabled or unavailable.
- Emit metrics for latency, hit rate proxies, truncation, and embedding behavior.

## Supported Code Parsing

- Rust
- Python
- JavaScript
- TypeScript

Unsupported languages should degrade to conservative lexical/file-based retrieval instead of failing the whole request.

## Agent Contract

- Context is advisory, not proof. When exact content matters, the agent should open the file with a deterministic file tool.
- Retrieval must stay bounded for low-resource hardware.
- Missing embeddings are expected on CPU-first setups and should not be treated as fatal.
- Live LSP symbols are not yet wired into runtime retrieval; track that work in `docs/CODING_AGENT_MVP.md` and `docs/LSP_ROADMAP.md`.
