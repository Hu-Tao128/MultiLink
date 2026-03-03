# `core/src/model_manager/` - Model Management Module

This module within the `core` crate is responsible for the discovery, inventory, and management of Large Language Models (LLMs) available to MultiLink. It provides an abstraction layer over various model sources, ensuring the UI remains stateless and decoupled from the underlying complexities of model interaction and storage.

## 🗄️ Files and Their Responsibilities

*   **`mod.rs`**: The module's public interface, re-exporting key components and types for external use.
*   **`registry.rs`**: Implements the in-memory model registry. This component maintains a comprehensive inventory of available models, including their `ModelInfo`, current status, aggregate statistics, and tracks the currently active model.
*   **`registry_source.rs`**: Defines interfaces and logic for different model sources. It handles the conversion of raw model data from various origins (e.g., HTTP APIs, local directories, npm-like registries) into a standardized format compatible with the model registry.
*   **`ollama.rs`**: Provides specific functionalities for interacting with local Ollama model installations. This includes operations such as detecting available Ollama models, listing them, handling migrations, and managing their deletion.
*   **`npm.rs`**: A placeholder module designed for future integration with npm-like package managers, allowing for discovery and management of models distributed via such systems.

## 🎯 Intent and Principles

The core principle behind the model manager is to centralize model metadata and storage behavior within the Rust core. This design decision ensures that:

*   **UI Remains Stateless**: The graphical user interface does not need to concern itself with the intricacies of model discovery, storage paths, or availability. It simply queries the core for model information and dispatches commands.
*   **Decoupled Logic**: Model management logic is robustly implemented and tested independently, reducing potential bugs and improving maintainability.
*   **Extensibility**: New model sources or management strategies can be integrated by implementing the `registry_source` interface without affecting other parts of the application.