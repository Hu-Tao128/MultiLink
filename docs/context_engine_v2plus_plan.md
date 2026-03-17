# ContextEngine V2Plus - Plan de Ejecucion y Seguimiento

Este plan es complementario al plan de 10 fases del orquestador LLM.

## Reglas de no interferencia

- No cambia contratos del orquestador (`ContextEngine::retrieve` se mantiene).
- Todo se activa por `context.engine` y flags de contexto.
- Hay fallback a `v2`/`v1` si falla el flujo nuevo.

## Estado rapido

- Paso actual: **6 - Observabilidad + A/B**.
- Siguiente paso: **6** (en progreso)
- Ultimo paso completado: **5**

---

## Checklist por pasos

### 1) Capa de compatibilidad + flags + metricas base

**Objetivo**
- Introducir `v2plus` sin romper `v1`/`v2`.

**Implementado**
- `ContextEngineVersion::V2Plus` y parser `v2plus`.
- Nuevo `ContextEngineV2Plus` con el mismo contrato del trait.
- En runtime, `context.engine = v2plus` usa engine dedicado.
- Flags runtime/config agregados:
  - `context_index_refresh_on_query`
  - `context_retrieval_enable_filters`
  - `context_v2plus_metrics`
- Env vars agregadas:
  - `MULTILINK_CONTEXT_INDEX_REFRESH`
  - `MULTILINK_CONTEXT_ENABLE_FILTERS`
  - `MULTILINK_CONTEXT_V2PLUS_METRICS`
  - `MULTILINK_CONTEXT_ENGINE`
- Validacion de engine: `v1 | v2 | v2plus`.

**Verificacion pendiente**
- [ ] Smoke test GUI con `context.engine = "v2plus"`.
- [ ] Confirmar que logs de `context.v2plus.metrics` se emiten solo cuando corresponde.

**Estado**: ✅ Completado (con verificacion funcional pendiente)

---

### 2) Indice incremental persistente

**Objetivo**
- Reindexar solo archivos cambiados en lugar de recomputar todos los chunks.

**Implementado**
- Extendido `manifest.json` con estado por archivo (`path`, `content_hash`, `chunk_hashes`).
- Reutilizacion de chunks para archivos sin cambios (`build_incremental_chunks`).
- Persistencia backward-compatible (`#[serde(default)]` en campos nuevos).

**Verificacion pendiente**
- [ ] Test de regresion: cambiar un solo archivo y confirmar que se recalcula solo ese subconjunto.
- [ ] Medir mejora de latencia en 2+ consultas consecutivas.

**Estado**: ✅ Completado (con verificacion de rendimiento pendiente)

---

### 3) Chunking AST inicial (Rust) + estructurado (Python/TS)

**Objetivo**
- Mejorar calidad semantica de chunks para retrieval.

**Implementado**
- Rust: extraccion basada en AST con `syn` (funciones, structs, enums, traits, mods, impls y metodos).
- Python/TS: deteccion estructurada mejorada de simbolos (def/class/async def/interface/type/enum/function/arrow).
- Fallback conservador a chunking por bloques cuando no hay marcadores.

**Verificacion pendiente**
- [ ] Casos edge en Rust macros/impl complejos.
- [ ] Revisar ruido de chunks en TS con arrow functions muy densas.

**Estado**: ✅ Completado (iteracion de precision pendiente)

---

### 4) Retrieval con filtros y particiones

**Objetivo**
- Permitir filtros por lenguaje/path y mejorar precision en codebase grande.

**Implementado**
- API interna de filtros en `hybrid_retrieval`.
- Filtros opcionales por lenguaje y patrones de path (con wildcard `*`/`?`).
- Merge por particiones de lenguaje cuando hay multiples lenguajes filtrados.
- Activacion por `context_retrieval_enable_filters`.

**Verificacion pendiente**
- [ ] Caso real: prompt con `src/...` y filtro de lenguaje multiple.
- [ ] Validar calidad cuando filtros no encuentran candidatos (fallback global).

**Estado**: ✅ Completado (con verificacion funcional pendiente)

---

### 5) Refresh asincrono con lock

**Implementado**
- Refresh en background por consulta (`refresh_in_background`) sin bloquear respuesta.
- Lock de refresh en vuelo por proyecto para evitar carreras.
- Carga "best effort" (`load_best_effort`) que usa indice exacto si existe,
  o el ultimo indice consistente mientras se refresca.
- Bootstrap sincronico solo si no hay indice previo.

**Verificacion pendiente**
- [ ] Validar en corrida larga que no se encolan refresh duplicados.
- [ ] Validar fallback consistente cuando cambia codigo entre prompts.

**Estado**: ✅ Completado (con verificacion funcional pendiente)

---

### 6) Observabilidad + A/B

**Estado**: 🟡 En progreso

**Implementado**
- Nueva estructura `ContextRetrievalMetrics` en `observability.rs`:
  - `context_latency_ms` - latencia total de retrieval
  - `embedding_latency_ms` - latencia de embeddings
  - `index_refresh_ms` - tiempo de refresh del indice (optional)
  - `retrieval_hit_rate` - tasa de aciertos (preparado para tracking)
  - `truncation_rate` - tasa de truncamiento
  - `error_rate` - tasa de errores
  - `selected_files`, `used_tokens`, `budget_used`, `embedding_used`, `top_k`
- Integracion en `chat_runtime.rs`: medicion de `context_latency_ms` con `Instant::now()`
- Emision de metricas unificadas para v2 y v2plus via `metrics.emit(false)`

**Por completar**
- [ ] Recolectar metricas comparables `v2` vs `v2plus` en misma corrida
- [ ] Definir criterios de promocion/rollback (hit_rate +5%, latency +15%)
- [ ] Suite de 60 prompts (20 navegacion, 20 debug, 20 refactor)
- [ ] Captura de metricas: p50/p95 latency, hit_rate, truncation_rate

**Instrucciones obligatorias de pruebas (gate antes del ultimo paso)**

Estas pruebas deben completarse y documentarse antes de pasar al Paso 7.

1. Preparacion de entorno
- [ ] Ejecutar baseline con `context.engine = "v2"`.
- [ ] Ejecutar canary con `context.engine = "v2plus"`.
- [ ] Mantener mismo repo, mismo modelo y mismo set de prompts para comparar.

2. Suite minima de validacion funcional
- [ ] 20 prompts de navegacion de codigo ("donde se implementa X", "que usa Y").
- [ ] 20 prompts de debugging (errores, stack traces, rutas de ejecucion).
- [ ] 20 prompts de refactor/feature (impacto en modulos y dependencias).
- [ ] Verificar que no hay respuestas vacias de contexto cuando `v2` si encontraba resultados.

3. Metricas a capturar por corrida
- [ ] `context_latency_ms` (p50/p95)
- [ ] `retrieval_hit_rate` (proxy: presencia de archivo correcto en top-k)
- [ ] `truncation_rate`
- [ ] `embedding_latency_ms`
- [ ] `index_refresh_ms` (cuando aplique)
- [ ] `error_rate_context_pipeline`

4. Criterios de aprobacion para avanzar al Paso 7
- [ ] `retrieval_hit_rate` de `v2plus` >= `v2` (ideal: +5% o mas).
- [ ] `context_latency_ms p95` no empeora mas de 15% vs `v2`.
- [ ] `error_rate_context_pipeline` <= `v2`.
- [ ] Sin regresiones funcionales criticas en prompts de debugging.

5. Criterios de rollback inmediato
- [ ] degradacion de `hit_rate` > 10% durante 2 corridas consecutivas.
- [ ] aumento de latencia p95 > 25% sostenido.
- [ ] errores de contexto en produccion/canary repetibles.

6. Evidencia requerida en PR
- [ ] Tabla comparativa `v2` vs `v2plus` con metricas.
- [ ] Lista de prompts usados (anonimizados si hace falta).
- [ ] Decision explicita: "aprobado para Paso 7" o "mantener en canary".

---

## Archivos tocados en pasos 1-3

- `core/src/context_engine/mod.rs`
- `core/src/chat_runtime.rs`
- `core/src/config.rs`
- `core/src/context_engine/index.rs`
- `core/src/context_engine/chunker.rs`
- `core/Cargo.toml`

## Comando de verificacion ejecutado

- `cargo test --manifest-path core/Cargo.toml context_engine`
