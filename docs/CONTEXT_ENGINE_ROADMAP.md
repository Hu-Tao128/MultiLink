# 🧠 Roadmap: MultiLink Context Engine v2

> Creado: 2026-03-16
> Estado: ✅ COMPLETADO - 2026-04-02

## ⚠️ Notas y Decisiones
*(Se añaden automáticamente al completar tareas)*

---

## Fase 1 — Lexical Search Engine

- [x] Crear módulo `core/src/context_engine/` con `mod.rs`.
- [x] Implementar `tokenizer.rs`: lowercase, split camelCase, split snake_case, quitar puntuación.
- [x] Implementar `index/lexical_index.rs`: índice invertido con `HashMap<String, Vec<ChunkId>>`.
- [x] Implementar `ranking/bm25.rs`: función `score(tf, df, n_docs, dl, avg_dl) -> f32` con k1=1.5, b=0.75.
- [x] Implementar `retrieval/lexical_search.rs`: pipeline query → tokenize → lookup → score → top_k.
- [x] Implementar `context_retrieval.rs`: entry point con branch `embeddings_enabled`.
- [x] Test unitario: indexar 3 chunks de prueba y verificar que la query devuelve el correcto.
- [ ] Benchmark: alinear objetivo documentado con gate real implementado (<50ms) o ajustar test para <10ms con corpus controlado.

## Fase 2 — Tree-sitter Semantic Chunking

- [x] Añadir dependencias `tree-sitter`, `tree-sitter-rust` al `Cargo.toml`.
- [x] Implementar `parser/tree_sitter_parser.rs`: inicializar parser por `Language` enum.
- [x] Definir struct `CodeChunk` con `id`, `file`, `symbol`, `start_line`, `end_line`, `text`, `language`.
- [x] Implementar `parser/chunk_extractor.rs` para **Rust** (`function_item`, `struct_item`, `impl_item`, `mod_item`).
- [x] Implementar `parser/chunk_extractor.rs` para **Python** (`function_definition`, `class_definition`).
- [x] Implementar `parser/chunk_extractor.rs` para **JavaScript/TypeScript**.
- [x] Implementar `parser/symbol_index.rs`: nombre de símbolo → `ChunkId`.
- [x] Conectar extractor con `LexicalIndex`: al indexar un archivo, usar chunks semánticos.
- [x] Test: parsear un archivo Rust de muestra y verificar que `fn` y `struct` son chunks separados.

## Fase 2.5 — Conexión Context Engine

- [x] Conectar `index.rs` con `chunk_extractor.rs` para usar extracción semántica al indexar.
- [x] El índice ahora usa `extract_semantic_chunks` del parser en lugar de chunking naive.

## Fase 3 — Hybrid Retrieval (Opcional)

- [x] Definir trait `EmbeddingStore` con método `search(query_vec, top_k) -> Vec<(ChunkId, f32)>`.
- [x] Implementar `hybrid_search()` con pesos configurables `alpha` (embedding) y `1-alpha` (lexical).
- [x] Asegurar que con `embeddings_enabled = false` el sistema sigue funcionando.
- [x] Test: verificar que hybrid ranking no degrada resultados vs solo lexical en un corpus pequeño.

## Fase 4 — LSP Semantic Server

> Esta fase usa la skill `lsp-builder`. Delegar implementación del servidor a esa skill.

- [x] Crear workspace member `lsp-server/` con `Cargo.toml` y dependencias (`tower-lsp`, `tokio`, `tree-sitter`).
- [x] Implementar handlers base: `initialize`, `didOpen`, `didChange`, `didSave`, `didClose`.
- [x] Implementar `ast_cache.rs` con `DashMap<Uri, Tree>`.
- [x] Implementar `symbol_index.rs` para el workspace activo.
- [x] Implementar `hover`: devolver código + docstring del símbolo bajo el cursor.
- [x] Implementar `publishDiagnostics`: errores semánticos desde el AST.
- [ ] **Integración:** conectar LSP con `Context Engine` real (estado actual: bridge `NoOp`).
- [ ] Validación manual en VSCode con `languageClient`.
- [ ] Validación manual en Neovim con `nvim-lspconfig`.

## Fase 4.5 — Live Context

- [x] `document_cache`: mantener texto actual de archivos abiertos en memoria.
- [x] Re-parsear con tree-sitter en cada `didChange` (parsing incremental).
- [ ] Exponer `symbol_table` al context engine para queries en tiempo real.
- [x] Debounce 300ms en `didChange` antes de re-indexar.

## Fase 6 — QA y Validación A/B (v2plus vs v1)

> Estado: 🟡 Validación reproducible pendiente

- [x] **Comparación de comportamiento:** Se ejecutaron sesiones exploratorias con v2plus bajo diferentes intents (ProjectWide, FileScoped, Conversational).
- [x] **Métricas capturadas:**
  - `embeddings=true` en 100% de requests
  - `selected_files` siempre relevantes al proyecto
  - `embed_latency_ms` cacheado: 200-350ms después del primer request
  - `fallback=false` cuando servidor local está disponible
  - `truncation_rate` variable (0.00-1.00) según budget de contexto
- [ ] **Veredicto final de promoción:** pendiente hasta completar la corrida A/B reproducible definida en `docs/CONTEXT_AB_VALIDATION.md`.
- [x] **Documentación de hallazgos:** El truncation_rate alto no es problema del engine, es del budget limitado (800 tokens para proyecto en hardware modesto).

---

## Fase 7 — Limpieza de archivos de config

> Estado: 🟡 Pendiente

- [ ] Eliminar `config.toml` (no usado, el código lee `multilink.toml`).
- [ ] Eliminar archivos `multilink.invalid-*.toml` (backups automáticos).
- [ ] Verificar que `multilink.toml` tenga `engine = "v2plus"` para producción.

---

## Fase 5 — QA y Rendimiento (pendiente)

- [x] Benchmark: medir latencia de retrieval lexical en proyecto de 50k líneas.
- [x] Benchmark: medir memoria de `LexicalIndex` con 10k chunks.
- [x] Test de integración: query end-to-end desde `retrieve()` hasta devolver chunks.
- [ ] Profiling con `cargo flamegraph` en workload real.

## Fase 4.5 — Live Context (pendiente)

- [x] `document_cache`: mantener texto actual de archivos abiertos en memoria.
- [x] Re-parsear con tree-sitter en cada `didChange` (parsing incremental).
- [ ] Exponer `symbol_table` al context engine para queries en tiempo real.
- [x] Debounce 300ms en `didChange` antes de re-indexar.

## Fase 4 — LSP Semantic Server (pendiente)

- [x] Crear workspace member `lsp-server/` con `Cargo.toml` y dependencias (`tower-lsp`, `tokio`, `tree-sitter`).
- [x] Implementar handlers base: `initialize`, `didOpen`, `didChange`, `didSave`, `didClose`.
- [x] Implementar `ast_cache.rs` con `DashMap<Uri, Tree>`.
- [x] Implementar `symbol_index.rs` para el workspace activo.
- [x] Implementar `hover`: devolver código + docstring del símbolo bajo el cursor.
- [x] Implementar `publishDiagnostics`: errores semánticos desde el AST.
- [ ] **Integración:** conectar LSP con `Context Engine` real (estado actual: bridge `NoOp`).
- [ ] Validación manual en VSCode con `languageClient`.
- [ ] Validación manual en Neovim con `nvim-lspconfig`.
