# Context Engine A/B Validation (v2 vs v2plus)

Fecha: 2026-03-18
Objetivo: ejecutar una validacion reproducible para decidir promocion o rollback de `v2plus`.

## 1) Setup comun

- Misma maquina, mismo repo, mismo modelo, mismo servidor Ollama.
- Ejecutar dos corridas separadas:
  - Baseline: `context.engine = "v2"`
  - Candidate: `context.engine = "v2plus"`
- Activar logs de contexto:

```bash
MULTILINK_DEBUG_CONTEXT=1 ./build/gui/multilink
```

## 2) Suite minima (60 prompts)

### Navegacion (20)

1. De que trata este proyecto
2. Cual es la arquitectura general
3. Donde se define la configuracion runtime
4. Donde se construye el contexto del proyecto
5. Donde se selecciona el engine de contexto
6. Donde se aplica fallback entre proveedores
7. Donde se gestionan sesiones
8. Donde se serializa y persiste el estado
9. Donde se integra Ollama
10. Donde se define top_k
11. Donde se calcula max_project_context_tokens
12. Donde se parsea el config TOML
13. Donde estan las rutas de comandos slash
14. Donde esta la logica del bridge LSP
15. Donde se publican diagnostics del LSP
16. Donde se manejan embeddings opcionales
17. Donde se resuelve el modelo de embeddings
18. Donde se aplican filtros de retrieval
19. Donde se emiten metricas de contexto
20. Donde se detecta hardware caps

### Debugging (20)

1. Por que tengo `unsupported_endpoint` en embeddings
2. Por que context truncation_rate sale 1.0
3. Por que tarda mucho en CPU sin GPU
4. Que provoca `query_embedding_failed`
5. Por que top_k efectivo baja respecto a config
6. Que pasa si `context.engine` no es valido
7. Por que no encuentra archivos relevantes
8. Que pasa si falla index build en v2plus
9. Como depurar latencia de contexto
10. Como validar que esta usando v2plus
11. Donde se define timeout de embeddings
12. Que pasa si no hay modelo embed instalado
13. Como confirmar fallback lexical-only
14. Como detectar cold start de Ollama
15. Que logs indican error real vs estado esperado
16. Como confirmar que usa CPU caps
17. Que pasa si el presupuesto de tokens se supera
18. Como interpretar context.metrics
19. Que rutas de API usa embeddings en Ollama
20. Como comparar v2 vs v2plus sin sesgo

### Refactor/Feature (20)

1. Propon refactor para reducir latencia en CPU
2. Agrega un preset de rendimiento CPU
3. Mejorar mensajes de diagnostico de embeddings
4. Separar logs de warning vs info en contexto
5. Ajustar estrategia top_k por intent
6. Diseñar caché para queries repetidas
7. Añadir pruebas para endpoint fallback embeddings
8. Mejorar parser de project context
9. Agregar metricas de first token latency
10. Crear reporte automatizado de context.metrics
11. Proponer umbrales de promocion/rollback
12. Reducir costo de scanning inicial de proyecto
13. Evitar retries innecesarios en 404
14. Documentar modo sin embeddings obligatorios
15. Diseñar prueba de carga para retrieval
16. Mejorar priorizacion de archivos raiz
17. Agregar cobertura para hardware mid-range CPU
18. Proponer plan de rollout gradual de v2plus
19. Diseñar estrategia de benchmark reproducible
20. Revisar consistencia roadmap vs codigo

## 3) Metricas requeridas

- `context_latency_ms` p50/p95
- `embedding_latency_ms` p50/p95
- `retrieval_hit_rate` (proxy operativo)
- `truncation_rate`
- `error_rate`
- distribucion de `reason=` en logs de `[context]`

## 4) Comando de analisis (nuevo)

```bash
python3 scripts/context_metrics_report.py --input /ruta/a/log.txt
python3 scripts/context_metrics_report.py --input /ruta/a/log.txt --engine v2plus
```

## 5) Criterios de decision

- Promocion:
  - `hit_rate(v2plus) >= hit_rate(v2)`
  - `p95_latency(v2plus) <= p95_latency(v2) * 1.15`
  - `error_rate(v2plus) <= error_rate(v2)`
- Rollback inmediato:
  - degradacion de `hit_rate` > 10% en 2 corridas
  - incremento p95 > 25% sostenido
  - errores repetibles de contexto en flujo normal

## 6) Evidencia requerida

- Tabla comparativa final `v2` vs `v2plus`
- Lista de prompts ejecutados
- Decision final: `promote` o `rollback`
