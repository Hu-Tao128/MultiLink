# Capabilities System — v1 (Stabilized)

## Overview
Este sistema representa la transición de un esquema de capacidades basado puramente en nombres de archivos a uno basado en metadatos técnicos (Ollama `api/show`).

## Qué hace actualmente
- **Detección Técnica**: Identifica `vision`, `audio` y `thinking` (razonamiento) analizando `model_info` y `family`.
- **Presupuesto Adaptativo**: Ajusta el `safe_budget` de contexto utilizando un `quantization_factor`.
- **Seguridad de Contexto**: Implementa un "Safety Clamp" (50% - 85%) para evitar que modelos extremadamente cuantizados o de alta precisión desborden el presupuesto operativo.

## Decisiones Arquitectónicas
1. **Estabilidad sobre Maximización**: Se prefiere reducir el contexto en modelos Q2/Q3 para mantener la coherencia, en lugar de maximizarlo solo por ahorro de RAM.
2. **Heurística Híbrida**: Se combina la información estructurada de Ollama con fallbacks por familia (ej. detección de visión forzada para la familia `gemma`).
3. **Retrocompatibilidad**: No se han alterado las interfaces de `LLMProvider` para permitir una transición suave hacia el diseño estructurado de la V2.

## Limitaciones Conocidas
- La detección de `audio` se realiza pero el sistema de routing aún no despacha tareas específicas de audio (marcado como experimental/noop).
- El factor de cuantización es conservador: prioriza evitar la "alucinación por contexto largo" en modelos pequeños.

## Futuro (V2)
- Sistema formal de `Features` (Trait-based).
- Ajuste dinámico de temperatura basado en la profundidad del contexto utilizado.
- Integración de capacidades de audio en el despachador de tareas.
