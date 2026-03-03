# `core/`

This directory houses the core Rust library crate, which encapsulates all the essential business logic for MultiLink. It is designed to be platform-agnostic and reusable, serving as the robust backend for the application.

## 📁 What This Folder Contains

*   **`Cargo.toml`**: Defines crate dependencies, metadata, and build configurations.
*   **`src/`**: Contains the main source code for the core logic, including:
    *   **`chat_runtime.rs`**: Manages the lifecycle of chat sessions, message processing, and interaction with providers.
    *   **`config.rs`**: Handles application configuration loading, saving, and validation.
    *   **`context_retrieval.rs`**: Implements logic for building and managing LLM context, including project-specific context injection.
    *   **`lib.rs`**: The main library entry point, orchestrating various core components.
    *   **`router.rs`**: Directs requests to the appropriate LLM provider based on configuration and availability.
    *   **`model_profile.rs`**: Builds model-aware budgets from provider metadata (parameter count, context window, etc.).
    *   **`observability.rs`**: Structured execution metrics emitted per generation.
    *   **`session.rs`**: Manages chat session state, history, and persistence.
    *   **`auth/`**: Modules for authentication mechanisms, including OAuth flows and secure token storage.
    *   **`model_manager/`**: Handles discovery, registration, and management of LLM models (e.g., Ollama models).
    *   **`providers/`**: Integrations with various Large Language Model APIs (e.g., Gemini, Ollama, Codex).
    *   **`system/`**: Platform-specific utilities and interactions, such as resolving data directories.
*   **`tests/`**: Contains unit and integration tests specifically for the core Rust logic.

## 🛠️ How to Work Here

This crate focuses solely on backend logic. UI concerns should remain entirely outside this directory.

*   **Run Checks**: To perform a quick check for common errors and warnings:
    ```bash
    cargo check
    ```
*   **Run Tests**: To execute all unit and integration tests for the core crate:
    ```bash
    cargo test
    ```
    For specific tests, use `cargo test <test_name>`.
*   **Cross-Platform Verification**: Changes pushed to the repository are automatically validated across Linux, macOS, and Windows via GitHub Actions CI/CD workflows.

### Development Principles

*   **UI-Agnosticism**: Strictly keep UI logic out of this crate. The core should operate independently of any specific frontend.
*   **Performance & Safety**: Prefer performance-safe defaults, such as bounded context windows, throttled persistence mechanisms, and non-blocking stream handling to ensure a responsive and stable application.

## 🎯 Current Core Priorities

*   **Error Resilience**: Prevent failures that are exposed to LLM providers due to oversized context payloads.
*   **Accurate Health Signaling**: Ensure that provider health reporting accurately distinguishes between connection failures and prompt/content-specific errors.
*   **Robust Streaming**: Maintain robust and reliable streaming under various network conditions, including partial frames and long-running responses from providers.
*   **Model-Aware Context**: Use `/api/show` metadata to tune context budgets for small/medium/large models.

## 📝 Pull Request Guidance

When contributing to the `core` crate:

*   **Test Coverage**: Always add new tests or update existing ones when modifying runtime behavior or provider interactions.
*   **Separation of Concerns**: Ensure that asynchronous operations and persistence logic remain within the `core` crate, and are not introduced into the GUI layer.
