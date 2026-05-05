# `lsp-server/src/` - LSP Implementation

This directory contains the experimental semantic LSP implementation.

## Files

- `main.rs`: server entry point.
- `backend.rs`: `tower-lsp` handlers and document lifecycle.
- `ast_cache.rs`: tree-sitter parse cache and language detection.
- `document_cache.rs`: open document text cache.
- `semantic_analysis.rs`: symbol extraction and diagnostics support.
- `bridge.rs`: Context Engine bridge contract; currently no runtime integration.

## Boundary

The LSP may provide semantic signals to MultiLink, but it should not own provider routing, file editing, command execution, or GUI state.
