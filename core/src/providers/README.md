# `core/src/providers/` - LLM Provider Integrations

This module within the `core` crate provides a unified interface for interacting with various Large Language Model (LLM) providers. Its primary goal is to abstract the complexities of different LLM APIs behind a consistent trait, allowing the rest of the application to interact with any supported model seamlessly.

## 🗄️ Files and Their Responsibilities

*   **`mod.rs`**: Defines the central `LLMProvider` trait, which all concrete provider implementations must adhere to. It also includes common types used across providers, such as `PromptOptions`, `LLMResponse`, error types, and definitions for stream events.
*   **`ollama.rs`**: Implements the `LLMProvider` trait for local Ollama instances. This file includes the logic for making HTTP requests to Ollama, handling streaming responses with robust chunk-safe parsing, explicit forwarding of system messages, and fine-tuning for network timeouts.
*   **`gemini.rs`**: Serves as a scaffold for integrating with the remote Gemini LLM API. This file will contain the specific API client logic, request/response mapping, and error handling for Gemini.
*   **`codex.rs`**: Serves as a scaffold for integrating with the remote Codex LLM API. Similar to `gemini.rs`, it will house the API client logic for Codex.

## 🤝 Provider Contracts and Principles

All `LLMProvider` implementations are expected to adhere to the following principles:

*   **Detailed Error Reporting**: Providers should return detailed HTTP errors (including status codes and response bodies) whenever possible. This helps the `chat_runtime` accurately classify failures (e.g., connection issues vs. prompt/content errors).
*   **Context Budget Delegation**: The `PromptOptions.num_ctx` field defaults to `None`. The `chat_runtime` is responsible for deciding and enforcing context budget constraints, giving it flexibility to manage token usage across different providers.
*   **Robust Stream Handling**: Stream implementations must be resilient to arbitrary transport chunk boundaries. They should not assume that each network frame corresponds to a complete JSON object or message, requiring robust parsing logic (e.g., line-buffered decoding).

## 🚀 How to Extend MultiLink with a New LLM Provider

Adding support for a new LLM provider involves these steps:

1.  **Create a New File**: Add a new Rust file (e.g., `new_provider.rs`) within this `providers/` directory.
2.  **Implement `LLMProvider` Trait**: Implement the `LLMProvider` trait in your new file, providing concrete logic for `send_prompt` and `send_prompt_stream` methods specific to your chosen LLM's API.
3.  **Register with Router**: Modify the `router.rs` module in the `core/src/` directory to include and select your new provider based on configuration.
4.  **Add Tests**: Develop comprehensive unit and integration tests to verify the new provider's availability detection, prompt sending behavior, and streaming capabilities.
5.  **Update Configuration**: Ensure that your new provider can be configured via `config/default.toml` if necessary.