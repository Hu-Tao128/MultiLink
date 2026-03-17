# 🧠 Roadmap: MultiLink Context Engine v2

> Creado: 2026-03-16
> Estado: 🟡 En progreso

## ⚠️ Notas y Decisiones
*(Se añaden automáticamente al completar tareas)*

---

## Fase 1 — Lexical Search Engine

- [ ] Crear módulo `core/src/context_engine/` con `mod.rs`.
- [ ] Implementar `tokenizer.rs`: lowercase, split camelCase, split snake_case, quitar puntuación.
- [ ] Implementar `index/lexical_index.rs`: índice invertido con `HashMap<String, Vec<ChunkId>>`.
- [ ] Implementar `ranking/bm25.rs`: función `score(tf, df, n_docs, dl, avg_dl) -> f32` con k1=1.5, b=0.75.
- [ ] Implementar `retrieval/lexical_search.rs`: pipeline query → tokenize → lookup → score → top_k.
- [ ] Implementar `context_retrieval.rs`: entry point con branch `embeddings_enabled`.
- [ ] Test unitario: indexar 3 chunks de prueba y verificar que la query devuelve el correcto.
- [ ] Benchmark: verificar latencia < 10ms con 1 000 chunks sintéticos.

## Fase 2 — Tree-sitter Semantic Chunking

- [x] Añadir dependencias `tree-sitter`, `tree-sitter-rust` al `Cargo.toml`.
- [x] Implementar `parser/tree_sitter_parser.rs`: inicializar parser por `Language` enum.
- [x] Definir struct `CodeChunk` con `id`, `file`, `symbol`, `start_line`, `end_line`, `text`, `language`.
- [x] Implementar `parser/chunk_extractor.rs` para **Rust** (`function_item`, `struct_item`, `impl_item`, `mod_item`).
- [x] Implementar `parser/chunk_extractor.rs` para **Python** (`function_definition`, `class_definition`).
- [x] Implementar `parser/chunk_extractor.rs` para **JavaScript/TypeScript**.
- [x] Implementar `parser/symbol_index.rs`: nombre de símbolo → `ChunkId`.
- [ ] Conectar extractor con `LexicalIndex`: al indexar un archivo, usar chunks semánticos.
- [x] Test: parsear un archivo Rust de muestra y verificar que `fn` y `struct` son chunks separados.

## Fase 3 — Hybrid Retrieval (Opcional)

- [ ] Definir trait `EmbeddingStore` con método `search(query_vec, top_k) -> Vec<(ChunkId, f32)>`.
- [ ] Implementar `hybrid_search()` con pesos configurables `alpha` (embedding) y `1-alpha` (lexical).
- [ ] Asegurar que con `embeddings_enabled = false` el sistema sigue funcionando.
- [ ] Test: verificar que hybrid ranking no degrada resultados vs solo lexical en un corpus pequeño.

## Fase 4 — LSP Semantic Server

> Esta fase usa la skill `lsp-builder`. Delegar implementación del servidor a esa skill.

- [ ] Crear workspace member `lsp-server/` con `Cargo.toml` y dependencias (`tower-lsp`, `tokio`, `tree-sitter`).
- [ ] Implementar handlers base: `initialize`, `didOpen`, `didChange`, `didSave`, `didClose`.
- [ ] Implementar `ast_cache.rs` con `DashMap<Uri, Tree>`.
- [ ] Implementar `symbol_index.rs` para el workspace activo.
- [ ] Implementar `hover`: devolver código + docstring del símbolo bajo el cursor.
- [ ] Implementar `publishDiagnostics`: errores semánticos desde el AST.
- [ ] **Integración:** el LSP puede consultar el `Context Engine` para enriquecer respuestas.
- [ ] Validación manual en VSCode con `languageClient`.
- [ ] Validación manual en Neovim con `nvim-lspconfig`.

## Fase 4.5 — Live Context

- [ ] `document_cache`: mantener texto actual de archivos abiertos en memoria.
- [ ] Re-parsear con tree-sitter en cada `didChange` (parsing incremental).
- [ ] Exponer `symbol_table` al context engine para queries en tiempo real.
- [ ] Debounce 300ms en `didChange` antes de re-indexar.

## Fase 5 — QA y Rendimiento

- [ ] Benchmark: medir latencia de retrieval lexical en proyecto de 50k líneas.
- [ ] Benchmark: medir memoria de `LexicalIndex` con 10k chunks.
- [ ] Test de integración: query end-to-end desde `retrieve()` hasta devolver chunks.
- [ ] Profiling con `cargo flamegraph` en workload real.
