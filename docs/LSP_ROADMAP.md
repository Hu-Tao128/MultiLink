# 🛠️ Roadmap: MultiLink Semantic LSP

> Última actualización: 2026-03-17
> Estado general: 🟡 En progreso

## ⚠️ Notas y Deuda Técnica
- Fase 1: Completada - servidor LSP base con handlers initialize, didOpen, didChange, didSave, didClose, hover
- Fase 2: Implementado tree-sitter con AST cache con soporte para Rust, Python, JavaScript, TypeScript
- Fase 3: Implementado análisis semántico con SemanticAnalyzer y WorkspaceSymbolIndex
- Fase 4: Implementado bridge con ContextBridge trait, BridgeState, timeouts y fallback
- Fase 4.5: Completado - document_cache, parsing incremental, debounce 300ms
- Fase 5: Optimización implementada, falta benchmark
- Fase 6: Infraestructura de lenguajes preparada, soportados: Rust, Python, JS/TS
- Prisma/Dart/Bash: pendientes por conflictos de versiones tree-sitter (0.20 vs 0.24)

---

## Fase 1: Base del servidor LSP
- [x] Crear el workspace member `lsp-server` con `Cargo.toml` y dependencias base.
- [x] Implementar el struct `Backend` con `#[derive(Clone)]` que implemente `LanguageServer`.
- [x] Handler `initialize`: devolver `ServerCapabilities` con hover y sync incremental.
- [x] Handler `initialized`: log de confirmación.
- [x] Handler `textDocument/didOpen`: almacenar contenido del documento.
- [x] Handler `textDocument/didChange`: actualizar contenido en memoria.
- [x] Handler `textDocument/didSave`: disparar re-análisis.
- [x] Handler `textDocument/didClose`: limpiar recursos.
- [x] Handler `textDocument/publishDiagnostics`: enviar lista de errores al editor.
- [x] Handler `textDocument/hover`: devolver documentación básica del símbolo.

## Fase 2: Parser y AST (`tree-sitter`)
- [x] Integrar `tree-sitter` en el `Cargo.toml` de `lsp-server`.
- [x] Implementar `AstCache` en `cache.rs` con soporte de parsing incremental.
- [x] Implementar parser para **Rust** (tree-sitter-rust).
- [x] Implementar parser para **Python** (tree-sitter-python).
- [x] Implementar parser para **JavaScript/TypeScript** (tree-sitter-javascript/typescript).
- [ ] Configurar parser para **Prisma** (usando grammar de tree-sitter-prisma).
- [ ] Configurar parser para **Dart** (usando grammar de tree-sitter-dart).
- [ ] Configurar parser para **Bash** (usando grammar de tree-sitter-bash).

## Fase 3: Análisis Semántico
- [x] Walker del AST: extraer funciones, variables y tipos de un documento.
- [x] Resolver referencias cruzadas (símbolo usado → símbolo definido).
- [x] Extraer docstrings/comentarios asociados a nodos del AST.
- [x] Alimentar `publishDiagnostics` con errores semánticos encontrados.
- [x] Alimentar `hover` con el docstring del símbolo bajo el cursor.

## Fase 4: Integración con MultiLink (Router Semántico)
- [x] Definir el contrato (trait o enum) de comunicación LSP ↔ Router.
- [x] Implementar bridge.rs con ContextBridge trait y BridgeState.
- [x] Lógica de decisión: modelo local (sugerencias rápidas) vs remoto (análisis profundo).
- [x] Timeout y fallback si el modelo remoto no responde en < 2s.
- [x] Conectar el LSP al Context Engine real cuando se usa dentro de MultiLink.

## Fase 5: Optimización de Rendimiento
- [x] Parsing incremental: pasar el árbol anterior a `parser.parse()` en ediciones.
- [x] Cache de AST con `DashMap` (concurrent hashmap) por URI de documento.
- [x] Debounce de 300ms en `didChange` antes de re-analizar.
- [ ] Benchmark: medir latencia de hover en proyecto de 10k líneas.

## Fase 6: Extensibilidad y Lenguajes Adicionales
- [x] Sistema de registro de lenguajes: estructura base preparada en SourceLanguage enum.
- [x] Soporte base: Rust, Python, JavaScript, TypeScript.
- [ ] Prisma, Dart, Bash: pendientes por conflictos de versiones de tree-sitter (requiere tree-sitter 0.20 vs 0.24 actual).
- [ ] Endpoint para que nodos LAN anuncien qué lenguajes soportan.

## Fase 7: Pruebas y QA
- [x] Test de integración: iniciar el servidor LSP y enviar una request `initialize`.
- [x] Test de hover: verificar que devuelve el docstring correcto para un símbolo.
- [x] Test de lenguaje: verificar detección de Rust, Python, JS/TS.
- [x] Test de cache: verificar AST cache y parsing incremental.
- [ ] Validación manual en VSCode (extensión con `languageClient`).
- [ ] Validación manual en Neovim (`nvim-lspconfig`).
- [ ] Profiling: medir uso de RAM/CPU con proyectos grandes (+50k líneas).
- [ ] Test de latencia: diagnósticos en < 100ms desde `didSave`.
