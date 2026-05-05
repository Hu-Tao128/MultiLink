# `lsp-server/` - Experimental Semantic LSP

This crate contains the experimental MultiLink language server. It provides editor-facing semantic signals that should eventually feed the main Context Engine.

## Current Capabilities

- LSP `initialize`, open/change/save/close handlers.
- Incremental document cache.
- Tree-sitter AST cache.
- Symbol analysis and workspace symbol index.
- Hover and diagnostics for supported languages.
- Debounced re-analysis on document changes.

## Supported Languages

- Rust
- Python
- JavaScript
- TypeScript

## Current Limitation

The bridge contract exists, but the server is not yet wired as a live source for the main `core/src/context_engine/` runtime. Treat LSP output as experimental until the bridge is connected and manually validated in editors.

## Validation Targets

- VSCode client smoke test.
- Neovim `nvim-lspconfig` smoke test.
- Hover latency on a 10k+ line project.
- RAM/CPU profiling on 50k+ line projects.
