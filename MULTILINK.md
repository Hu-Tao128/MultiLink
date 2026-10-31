# MultiLink — Project Definition

> Este archivo fue generado automáticamente por MultiLink.
> Lee este archivo primero antes de cualquier tarea.

---

## 📌 Project Overview

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Build Status](https://github.com/multilink-dev/multilink/actions/workflows/rust.yml/badge.svg)](https://github.com/multilink-dev/multilink/actions)
[![Rust Version](https://img.shields.io/badge/Rust-1.75%2B-blue.svg)](https://rustup.rs/)
[![Qt Version](https://img.shields.io/badge/Qt-6.5%2B-purple.svg)](https://www.qt.io/)


**Tipo de proyecto:** desktop app  
**Complejidad:** medium

---

## 🏗️ Architecture

**Stack detected:** Qt/QML

**Estructura detectada:**
  - `docs`
  - `gui`
  - `config`
  - `scripts`
  - `tests`
  - `core`

---

## 📊 Current State

- **Tests:** ✅
- **Documentación:** ✅
- **Docker:** ✅
- **Linting:** ✅

### ⚠️ Problemas Detectados

✓ No se detectaron problemas

### 💡 Sugerencias

✓ Proyecto bien configurado

---

## 🗺️ Active Roadmaps

- `docs/CONTEXT_ENGINE_ROADMAP.md`
- `docs/LSP_ROADMAP.md`
- `docs/ROADMAP_CPU_OPTIONAL_EMBEDDINGS.md`
- `docs/ROADMAP_V3_STATE.md`
- `docs/roadmap.md`

### Module Docs

- `config/README.md`
- `core/README.md`
- `core/src/README.md`
- `core/src/auth/README.md`
- `core/src/commands/README.md`
- `core/src/context_engine/README.md`
- `core/src/context_engine/parser/README.md`
- `core/src/execution/README.md`
- `core/src/model_manager/README.md`
- `core/src/orchestrator/README.md`
- ... y 15 archivos más

### Markdown Highlights

- `.github/pull_request_template.md`: Briefly describe the change in 2-4 bullets.
- `AGENTS.md`: **NOTE**: This file provides repo-specific guidance for coding agents.
- `CHANGELOG.md`: All notable changes to this project will be documented in this file.
- `CONTRIBUTING.md`: 2. Clone your fork: git clone https://github.com/YOUR_USERNAME/MultiLink.git

---

## ⚙️ Execution Rules

1. Leer este archivo primero
2. Usar roadmap si existe en docs/
3. Ejecutar UNA tarea a la vez
4. Validar antes de completar (ver comandos abajo)
5. No asumir contexto faltante

### Comandos de Validación

- (No standard validation commands detected)

### Agent Guidance Files

- `AGENTS.md`
- `GEMINI.md`

---

## 🔒 Constraints

- No romper código existente
- Mantener compatibilidad
- Preferir cambios pequeños
- No compartir credenciales

---

## 🏃 Execution Context

### Runtime Commands

- (No runtime commands detected)

### Quick Reference

```bash
# No quick reference commands detected
```

### Architecture Boundaries

- `core/` o `lib/` → lógica de dominio y componentes reutilizables
- `gui/`, `frontend/` o `app/` → capa de UI e interacción

---

## 🧠 Agent Behavior

### Cómo pensar sobre este proyecto

- **Tipo:** desktop app
- **Stack:** Qt/QML
- **Complejidad:** medium

### Workflow recomendado

```
1. Entender la tarea → leer docs/ si existe
2. Identificar módulo afectado
3. Implementar cambio mínimo necesario
4. Validar con tests
5. Commit con mensaje descriptivo
```

### Cuando detenerse

- Tarea demasiado grande → dividir
- Falta de contexto → pedir clarificación
- Ambigüedad → confirmar antes de proceder

---

## 📂 Important Paths

  - `docs`
  - `gui`
  - `config`
  - `scripts`
  - `tests`
  - `core`

---

## 🤖 Machine Readable

```json
{"complexity":"medium","guidance_files":["AGENTS.md","GEMINI.md"],"has_docker":true,"has_docs":true,"has_linting":true,"has_tests":true,"markdown_files":[".github/pull_request_template.md","AGENTS.md","CHANGELOG.md","CONTRIBUTING.md","GEMINI.md","README.md","SCALABILITY_PLAN.md","SECURITY.md","config/README.md","core/README.md","core/src/README.md","core/src/auth/README.md","core/src/commands/README.md","core/src/context_engine/README.md","core/src/context_engine/parser/README.md","core/src/execution/README.md","core/src/model_manager/README.md","core/src/orchestrator/README.md","core/src/orchestrator/tools/README.md","core/src/providers/README.md","core/src/system/README.md","core/src/tools/README.md","core/tests/README.md","docs/CAPABILITIES_V1.md","docs/CODING_AGENT_MVP.md","docs/CONTEXT_AB_RESULTS_TEMPLATE.md","docs/CONTEXT_AB_VALIDATION.md","docs/CONTEXT_ENGINE_ROADMAP.md","docs/LSP_ROADMAP.md","docs/MCP.md","docs/README.md","docs/RETRY_POLICY.md","docs/ROADMAP_CPU_OPTIONAL_EMBEDDINGS.md","docs/ROADMAP_V3_STATE.md","docs/architecture.md","docs/auth.md","docs/context_engine_v2plus_plan.md","docs/network-troubleshooting.md","docs/providers.md","docs/roadmap.md","gui/README.md","gui/qml/README.md","gui/rust/README.md","gui/rust/chat_controller/README.md","gui/src/README.md","lsp-server/README.md","lsp-server/src/README.md","multilink-backend-improvement-plan.md","scripts/README.md","tests/README.md"],"module_readmes":["config/README.md","core/README.md","core/src/README.md","core/src/auth/README.md","core/src/commands/README.md","core/src/context_engine/README.md","core/src/context_engine/parser/README.md","core/src/execution/README.md","core/src/model_manager/README.md","core/src/orchestrator/README.md","core/src/orchestrator/tools/README.md","core/src/providers/README.md","core/src/system/README.md","core/src/tools/README.md","core/tests/README.md","docs/README.md","gui/README.md","gui/qml/README.md","gui/rust/README.md","gui/rust/chat_controller/README.md","gui/src/README.md","lsp-server/README.md","lsp-server/src/README.md","scripts/README.md","tests/README.md"],"paths":["docs","gui","config","scripts","tests","core"],"roadmap_files":["docs/CONTEXT_ENGINE_ROADMAP.md","docs/LSP_ROADMAP.md","docs/ROADMAP_CPU_OPTIONAL_EMBEDDINGS.md","docs/ROADMAP_V3_STATE.md","docs/roadmap.md"],"stack":["Qt/QML"],"type":"desktop app","validation_commands":[]}
```

<!-- multilink:user:custom_notes -->
<!-- multilink:end -->

---



<!-- multilink:model_analysis:start -->
## 🤖 Model Analysis

### Quality Notes
- Proyecto "MultiLink" es aplicación de escritorio Qt/QML con núcleo Rust, health_score 100.
- Flags has_docs, has_tests, has_docker, has_linting confirman soporte completo de calidad.
- Rutas detectadas (core, gui, docs, config, scripts, tests) reflejan estructura modular complejidad media.
- Amplia cobertura Markdown/READMEs indica buenas prácticas de onboarding.

### Risks
- No se detectaron riesgos críticos con la evidencia actual.

### Next Actions
- Revisar docs/CODING_AGENT_MVP.md para definir el contrato del agente local-first.
- Implementar integración LSP y bucles de validación pendientes para el MVP.
- Sincronizar READMEs de módulos para mantener consistencia con has_docs: true.
- Agregar comandos de validación iniciales, dado validation_commands vacío.
<!-- multilink:model_analysis:end -->

## 🤖 Generated by MultiLink

Este archivo fue inicializado automáticamente el 2026-10-30.

Puedes editar la sección `custom_notes` para agregar notas específicas del proyecto.

---

## 🏷️ Tags

`qt/qml`
