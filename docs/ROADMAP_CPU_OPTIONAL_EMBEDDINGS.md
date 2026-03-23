# Roadmap - CPU First + Embeddings Opcionales

Fecha: 2026-03-18
Estado general: En progreso

## Objetivo

Mejorar la experiencia de MultiLink en hardware modesto (CPU sin GPU) y tratar embeddings como una capacidad opcional, sin degradar UX cuando no hay modelo instalado.

## Fases

### Paso 1 - Coherencia de roadmap y estado real (docs)

- [x] Definir fuente canónica de estado: `docs/ROADMAP_V3_STATE.md`.
- [x] Alinear estado global con roadmaps específicos (`LSP`, `Context Engine v2plus`).
- [x] Corregir claims no verificables como "LSP consulta Context Engine real" hasta implementar integración real.
- [x] Resolver contradicciones de estado en `SCALABILITY_PLAN.md`.
- [ ] Agregar matriz completa de trazabilidad (roadmap item -> archivo -> test).

Entregable del paso:
- Estado de docs consistente para evitar priorización errónea.

### Paso 2 - Embeddings opcionales sin ruido operacional

- [x] Tratar ausencia de embedding model como estado esperado, no error.
- [x] Normalizar razones de diagnóstico (`optional_not_configured`, `disabled_by_config`, `unsupported_endpoint`).
- [x] Mantener retrieval lexical funcional sin penalización de UX.

Entregable del paso:
- Pipeline de contexto estable con y sin embeddings.

### Paso 3 - Compatibilidad de endpoint Ollama para embeddings

- [x] Intentar `POST /api/embed` y fallback a `POST /api/embeddings` cuando aplique.
- [x] Evitar loops de retry innecesarios en errores 404 de endpoint no soportado.
- [x] Emitir diagnóstico claro de versión/capacidad no soportada.

Entregable del paso:
- Compatibilidad robusta entre versiones de Ollama.

### Paso 4 - Perfil CPU low-end por defecto

- [x] Ajustar presupuesto de contexto/top_k para hardware sin GPU.
- [x] Reducir latencia p95 en preguntas de navegación de proyecto.
- [x] Mantener calidad suficiente en prompts de "overview" y debugging básico.

Entregable del paso:
- Preset CPU-first con mejor tiempo de respuesta percibido.

### Paso 5 - Validación A/B y criterios de promoción

- [x] Definir suite mínima (navegación, debug, refactor) en `docs/CONTEXT_AB_VALIDATION.md`.
- [x] Crear tooling para métricas (`scripts/context_metrics_report.py`).
- [x] Crear plantilla de resultados (`docs/CONTEXT_AB_RESULTS_TEMPLATE.md`).
- [ ] Ejecutar corrida completa v2 vs v2plus en hardware objetivo.
- [ ] Capturar p50/p95, truncation, hit-rate proxy, error rate con evidencia final.
- [ ] Definir decisión explícita: promoción o rollback.

Entregable del paso:
- Evidencia objetiva para decisiones de release.

## Criterios de éxito

- Sin contradicciones de estado en roadmaps.
- Sin errores falsos por embeddings no configurados.
- Menor latencia percibida en CPU en comparación con baseline actual.
- Métricas comparables y reproducibles para v2 vs v2plus.

## Notas

- Este roadmap complementa `docs/context_engine_v2plus_plan.md` y `docs/LSP_ROADMAP.md`.
- Se prioriza estabilidad y claridad documental antes de cambios de comportamiento en runtime.
