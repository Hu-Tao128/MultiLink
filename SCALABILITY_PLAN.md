# Plan de Escalabilidad y Despliegue

> Generado el: 2026-03-16 | Auditor: Skill `auditor-escalabilidad`
> Actualizado el: 2026-03-17

## Resumen Ejecutivo

El proyecto MultiLink presenta una arquitectura bien diseñada con separación clara entre el núcleo Rust (`core/`) y la interfaz gráfica Qt/QML (`gui/`). El sistema demuestra robustez en el manejo de errores con circuit breaker y fallback entre servidores Ollama. **Todos los hallazgos críticos han sido resueltos.**

**Nivel de Riesgo Global:** 🟢 Bajo

---

## 🔴 Hallazgos Críticos

### HC-01: URLs de Ollama hardcodeadas como valores por defecto ✅ RESUELTO
- **Archivo(s):** `core/src/config.rs`, `core/src/context_engine/mod.rs`
- **Descripción:** Los valores por defecto de `base_url` y URLs de embedding apuntaban directamente a `http://127.0.0.1:11434`.
- **Solución implementada:** 
  - Nueva función `detect_ollama_base_url()` en `config.rs` que:
    1. Lee `OLLAMA_HOST` o `MULTILINK_OLLAMA_BASE_URL`
    2. Prueba puertos comunes (11434, 10101) con verificación de conectividad
    3. Intenta `ollama list` como fallback
    4. Advertencia clara si no detecta nada
  - Reemplazados todos los hardcoded defaults por llamadas a `detect_ollama_base_url()`

---

### HC-02: Ruta de modelos Ollama no detectada para Linux ✅ RESUELTO
- **Archivo(s):** `core/src/model_manager/ollama.rs`
- **Descripción:** La función `detect_models_dir()` solo buscaba en `/var/lib/ollama/models` para Linux.
- **Solución implementada:**
  - Agregada detección de `~/.ollama/models` para Linux y macOS
  - Verifica tanto el directorio como el padre `.ollama` para mayor fiabilidad
  - Fallback a `/var/lib/ollama/models` si no existe ninguno

---

## 🟡 Deuda Técnica

### DT-01: Timeouts no configurables para proveedores remotos ✅ RESUELTO
- **Área:** Backend
- **Descripción:** Los providers ya soportan configuración de timeouts via variables de entorno:
  - `MULTILINK_OLLAMA_CONNECT_TIMEOUT_SECS` (default: 10s)
  - `MULTILINK_OLLAMA_HTTP_TIMEOUT_SECS` (default: 1800s)
  - `MULTILINK_OLLAMA_STREAM_IDLE_TIMEOUT_SECS` (default: 180s)
  - `MULTILINK_OLLAMA_STREAM_RETRIES` (default: 4)
- **Estado:** Implementado en `providers/ollama.rs:44-60`

### DT-02: Sin archivo Dockerfile para despliegue contenedorizado ✅ RESUELTO
- **Área:** DevOps
- **Descripción:** Creados archivos para despliegue:
  - `Dockerfile` - imagen multi-stage optimizada
  - `docker-compose.yml` - configuración para desarrollo y producción
- **Estado:** Resuelto

### DT-03: Modelo de embeddings por defecto no disponible ⚪ Pendiente
- **Área:** Backend
- **Descripción:** El config especifica `embed_model = "embeddinggemma"` que no existe por defecto en Ollama.
- **Esfuerzo estimado:** 1 hora
- **Estado:** Pendiente (baja prioridad - solo afecta si embeddings están habilitados)

---

## 📋 Plan de Acción

### Corto Plazo (0–2 semanas) — Desbloquear el Despliegue
- [x] HC-01: Implementar detección automática de puerto Ollama
- [x] HC-02: Agregar detección de `~/.ollama/models` para Linux
- [x] DT-01: Timeouts configurables via variables de entorno
- [x] DT-02: Crear Dockerfile y docker-compose.yml
- ✅ DT-03: Cambiar `embed_model` por uno disponible (e.g., `nomic-embed-text`) — *implementado auto-detección en resolve_embed_model()*

### Mediano Plazo (1–3 meses) — Reducir Deuda Técnica
- [ ] Agregar logging claro cuando fallan proveedores

### Largo Plazo (3–6 meses) — Escalabilidad y Distribución
- [ ] Documentar despliegue en Kubernetes
- [ ] Implementar health check endpoint

---

## ✅ Aspectos Positivos Identificados

| Área | Hallazgo |
|------|----------|
| **Arquitectura** | Separación limpia entre core (Rust) y GUI (Qt/QML) |
| **Resiliencia** | Circuit breaker con backoff exponencial |
| **Fallback** | Sistema robusto de fallback entre servidores Ollama |
| **Rutas** | Uso correcto de PathBuf y crate dirs |
| **Config** | Soporte de variables de entorno + detección automática |
| **CI/CD** | Workflow multiplataforma en GitHub Actions (Ubuntu, Windows, macOS) |
| **CMake** | Gestión de dependencias por plataforma (APPLE, WIN32) |
| **Docker** | Dockerfile multi-stage + docker-compose.yml |
| **Seguridad** | No hay API keys hardcodeadas |
| **UI** | Sin bloqueo del hilo principal detectado |

---

## 🔄 Actualización — 2026-03-17

### Hallazgos Resueltos desde la Última Auditoría
- ✅ HC-01 — URLs de Ollama hardcodeadas — *resuelto con `detect_ollama_base_url()` en config.rs*
- ✅ HC-02 — Ruta de modelos no detectada para Linux — *resuelto en model_manager/ollama.rs con ~/.ollama/models*
- ✅ DT-01 — Timeouts no configurables — *implementado via MULTILINK_OLLAMA_*_TIMEOUT*
- ✅ DT-02 — Sin Dockerfile — *creado Dockerfile y docker-compose.yml*

### Verificación de Código
- `detect_ollama_base_url()` presente en config.rs (líneas 12, 297, 337, 474, 697)
- `detect_models_dir()` verifica `~/.ollama/models` en ollama.rs (líneas 28-29)
- Timeouts configurables via env vars en providers/ollama.rs (líneas 45-55)
- Dockerfile existe en raíz del proyecto

### Pendientes sin Cambios
- ⏳ Ninguno — Todos los hallazgos resueltos

### Notas
- Los defaults hardcodeados (127.0.0.1:11434) en config.rs son aceptables como **fallback final** después de que la detección falla
- No se detectaron nuevas rutas absolutas de usuario en el código
- **DT-03 resuelto**: `embed_model` por defecto es vacío y `resolve_embed_model()` auto-detecta el mejor modelo disponible
