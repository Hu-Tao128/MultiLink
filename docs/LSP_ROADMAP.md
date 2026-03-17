# 🛠️ Roadmap: MultiLink Semantic LSP

> Última actualización: 2026-03-16
> Estado general: 🟡 En progreso

## ⚠️ Notas y Deuda Técnica
*(Se añaden aquí automáticamente al completar tareas)*

---

## Fase 1: Base del servidor LSP
- [ ] Crear el workspace member `lsp-server` con `Cargo.toml` y dependencias base.
- [ ] Implementar el struct `Backend` con `#[derive(Clone)]` que implemente `LanguageServer`.
- [ ] Handler `initialize`: devolver `ServerCapabilities` con hover y sync full.
- [ ] Handler `initialized`: log de confirmación.
- [ ] Handler `textDocument/didOpen`: almacenar contenido del documento.
- [ ] Handler `textDocument/didChange`: actualizar contenido en memoria.
- [ ] Handler `textDocument/didSave`: disparar re-análisis.
- [ ] Handler `textDocument/didClose`: limpiar recursos.
- [ ] Handler `textDocument/publishDiagnostics`: enviar lista de errores al editor.
- [ ] Handler `textDocument/hover`: devolver documentación básica del símbolo.

## Fase 2: Parser y AST (`tree-sitter`)
- [ ] Integrar `tree-sitter` en el `Cargo.toml` de `lsp-server`.
- [ ] Implementar `AstCache` en `cache.rs` con soporte de parsing incremental.
- [ ] Configurar parser para **Prisma** (usando grammar de tree-sitter-prisma).
- [ ] Configurar parser para **Dart** (usando grammar de tree-sitter-dart).
- [ ] Configurar parser para **Bash** (usando grammar de tree-sitter-bash).
- [ ] Prueba: parsear un archivo `.prisma` y loggear el AST por stderr.

## Fase 3: Análisis Semántico
- [ ] Walker del AST: extraer funciones, variables y tipos de un documento.
- [ ] Resolver referencias cruzadas (símbolo usado → símbolo definido).
- [ ] Extraer docstrings/comentarios asociados a nodos del AST.
- [ ] Alimentar `publishDiagnostics` con errores semánticos encontrados.
- [ ] Alimentar `hover` con el docstring del símbolo bajo el cursor.

## Fase 4: Integración con MultiLink (Router Semántico)
- [ ] Definir el contrato (trait o enum) de comunicación LSP ↔ Router.
- [ ] Conectar el LSP al dispatcher de MultiLink para sugerencias de código.
- [ ] Lógica de decisión: modelo local (sugerencias rápidas) vs remoto (análisis profundo).
- [ ] Timeout y fallback si el modelo remoto no responde en < 2s.

## Fase 5: Optimización de Rendimiento
- [ ] Parsing incremental: pasar el árbol anterior a `parser.parse()` en ediciones.
- [ ] Cache de AST con `DashMap` (concurrent hashmap) por URI de documento.
- [ ] Debounce de 300ms en `didChange` antes de re-analizar.
- [ ] Benchmark: medir latencia de hover en proyecto de 10k líneas.

## Fase 6: Extensibilidad y Lenguajes Adicionales
- [ ] Sistema de registro de lenguajes: trait `LanguageSupport` con `grammar()` y `analyze()`.
- [ ] Soporte completo: Prisma (tipos, modelos, relaciones).
- [ ] Soporte completo: Dart (clases, mixins, null safety).
- [ ] Soporte completo: Bash (funciones, variables, subshells).
- [ ] Endpoint para que nodos LAN anuncien qué lenguajes soportan.

## Fase 7: Pruebas y QA
- [ ] Test de integración: iniciar el servidor LSP y enviar una request `initialize`.
- [ ] Test de hover: verificar que devuelve el docstring correcto para un símbolo.
- [ ] Validación manual en VSCode (extensión con `languageClient`).
- [ ] Validación manual en Neovim (`nvim-lspconfig`).
- [ ] Profiling: medir uso de RAM/CPU con proyectos grandes (+50k líneas).
- [ ] Test de latencia: diagnósticos en < 100ms desde `didSave`.
