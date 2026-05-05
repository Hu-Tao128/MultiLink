# MultiLink Coding Agent MVP

Estado: ✅ COMPLETADO — 2026-05-04

> Las 6 tareas del MVP están implementadas. Ver `docs/roadmap.md` para el estado consolidado.

## Objetivo

MultiLink debe evolucionar de una app de chat con contexto de proyecto a un agente de código local-first que pueda inspeccionar, razonar, modificar y validar un workspace con trazabilidad. El criterio no es "parecer agente", sino completar cambios pequeños de software con evidencia reproducible.

## Definición de "agente real"

Un agente de código real en este proyecto debe cumplir estos mínimos:

- Entiende el workspace: lee `README.md`, `GEMINI.md`, `AGENTS.md`, `MULTILINK.md` y README por carpeta antes de proponer cambios grandes.
- Usa herramientas deterministas antes de responder: búsqueda, lectura de archivos, contexto semántico y comandos de diagnóstico cuando correspondan.
- Planifica tareas de varios pasos: separa exploración, edición, validación y reporte final.
- Edita archivos dentro del proyecto con guardrails de path, diff y rollback explícito.
- Ejecuta validaciones detectadas por `/init` o definidas en documentación del repo.
- Reporta qué cambió, qué se validó y qué queda sin verificar.

## Estado actual del código

MultiLink ya tiene piezas útiles, pero todavía no llega al estándar anterior.

### Disponible

- `core/src/context_engine/`: retrieval lexical/híbrido, chunking semántico y presupuestos de contexto.
- `core/src/tools/`: registry de herramientas internas sin shell general.
- `search_code`: búsqueda por Context Engine.
- `open_file`: lectura segura con truncado/chunking.
- `search_and_open`: búsqueda y apertura combinada.
- `fs_ls`: listado de directorios dentro del project root.
- `fs_cat`: lectura de archivo dentro del project root.
- `fs_grep`: búsqueda textual pura en Rust.
- `git_status`: estado git read-only del workspace.
- `git_diff`: diff git read-only del workspace o de una ruta relativa.
- `system_version`: detección de versiones de Node, Java, Python, Rust, Cargo y Git.
- `/init`: escaneo del proyecto y generación/merge de `MULTILINK.md`.
- `/doctor`: diagnóstico del proyecto; `--security` añade revisión de seguridad y LAN secret.
- `/write-file`: escritura explícita de archivo relativo dentro del proyecto.
- `lsp-server/`: servidor LSP experimental con AST cache, hover, diagnostics y symbol index local.
- `core/src/lan_agent.rs` y `core/src/mcp_adapter.rs`: transporte LAN y adaptador MCP delgado para `chat.dispatch`/`chat.ping`.
- `core/src/skills.rs`: carga de skills globales/proyecto desde TOML.

### Limitaciones (MVP resueltas, persisten otras)

- [x] ~~No existe herramienta de edición estructurada tipo patch/diff.~~ → `write_file` + `apply_patch` implementadas.
- [x] ~~Falta herramienta de comandos allowlisted.~~ → `run_command` implementado con 23 comandos built-in + detección vía `/init`.
- [x] ~~No hay bucle formal de "editar -> validar -> corregir".~~ → Executor auto-inserta `run_command` tras write/edit.
- [x] ~~LSP no conectado al Context Engine.~~ → `RealContextBridge` conecta LSP con `ContextEngineV2` + `ChunkExtractor`.
- [x] ~~MCP solo chat básico.~~ → `tools.list` y `tools.execute` expuestos via MCP/LAN.
- El planner sigue siendo heurístico; no construye planes robustos para bugs/refactors complejos.
- Las skills solo seleccionan manifiestos; no ejecutan workflows con pasos verificables.
- Falta rollback explícito para write_file/apply_patch.

## Contrato de herramientas v1

Las herramientas deben ser pequeñas, auditables y sin efectos colaterales ocultos.

| Tool | Tipo | Estado | Contrato |
|------|------|--------|----------|
| `fs_ls` | lectura | implementada | Lista una ruta relativa dentro del project root. |
| `fs_cat` | lectura | implementada | Lee un archivo relativo completo. |
| `fs_grep` | lectura | implementada | Busca texto en extensiones permitidas sin usar shell. |
| `search_code` | lectura | implementada | Usa Context Engine para encontrar archivos/snippets relevantes. |
| `open_file` | lectura | implementada | Lee archivo con límites, chunking y búsqueda interna opcional. |
| `search_and_open` | lectura | implementada | Combina retrieval y apertura. |
| `system_version` | lectura sistema | implementada | Detecta versiones de herramientas comunes. |
| `write_file` | escritura | implementada | Tool interna con límite de 1MB, path guard y diff summary (líneas antes/después). |
| `apply_patch` | escritura | implementada | Aplica cambios por diff unificado, con validación de hunk, path guard y resumen. |
| `run_command` | ejecución | implementada | Ejecuta comandos allowlist (built-in + detectados por `/init` en MULTILINK.md); sin shell arbitrario. |
| `git_status`/`git_diff` | lectura | implementada | Reporta cambios sin modificar el repo. |

## Guardrails obligatorios

- Todas las rutas deben ser relativas al project root y rechazar `..`, rutas absolutas, symlinks escapando del root y URLs.
- Las herramientas de escritura deben devolver diff o hash antes/después.
- Los comandos deben ser allowlisted por tipo de proyecto o documentación local; no se permite shell arbitrario como primera versión.
- Operaciones destructivas (`rm`, reset, checkout, migrations destructivas) requieren consentimiento explícito.
- Secrets, tokens y archivos `.env` deben tratarse como sensibles: se puede reportar existencia, no contenido.
- El agente no debe inventar validaciones: si no pudo ejecutarlas, debe decirlo.

## Flujo mínimo de una tarea de código

1. Orientación: leer documentación raíz y README de carpetas afectadas.
2. Exploración: usar `search_code`, `fs_grep`, `open_file` y Context Engine.
3. Plan breve: declarar archivos candidatos, riesgo y validación esperada.
4. Edición: aplicar cambios pequeños con herramienta de patch o escritura explícita.
5. Validación: ejecutar comandos allowlisted relevantes.
6. Reporte: listar archivos modificados, pruebas ejecutadas y riesgos residuales.

## Prioridad de implementación

1. Convertir `/write-file` en tool interna `write_file` con límite de tamaño y resumen.
2. Implementar `apply_patch` con path guard, diff y tests.
3. Implementar `run_command` allowlisted usando comandos detectados por `/init`.
4. Hacer que el executor ejecute el ciclo `plan -> tools -> edit -> validate -> final`.
5. Conectar LSP/live symbols al Context Engine real.
6. Ampliar MCP para exponer tools read-only antes de exponer escritura.

## Gates de aceptación

- El agente puede resolver una corrección pequeña en `core/` con modificación, test específico y reporte.
- Si una validación falla, el agente muestra error relevante y hace una iteración de corrección o se detiene con diagnóstico claro.
- Las herramientas de escritura tienen tests de path traversal, ruta absoluta y archivo fuera del root.
- Los tests relevantes pasan localmente, como mínimo:
  - `cargo test --manifest-path core/Cargo.toml tools`
  - `cargo test --manifest-path core/Cargo.toml orchestrator`
  - `cargo test --manifest-path core/Cargo.toml commands`
- La documentación de tool contracts está sincronizada con código y tests.
