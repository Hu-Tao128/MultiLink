# `core/src/context_engine/parser/` - Semantic Chunk Parsing

This module contains language detection, tree-sitter parsing, chunk extraction, and symbol indexing used by the Context Engine.

## Supported Languages

- Rust
- Python
- JavaScript
- TypeScript

## Agent Relevance

Parser output improves retrieval quality, but it is not a substitute for exact file reads. When the agent needs exact code, it should open the file through `open_file` or another deterministic file tool.
