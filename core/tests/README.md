# `core/tests/` - Core Integration Tests

This directory contains integration tests for the `core` Rust crate. These tests are crucial for verifying the correct behavior, reliability, and interaction between different components of the MultiLink backend logic. They ensure that the core functionalities, such as configuration management, provider routing, secure token storage, and chat runtime processes, operate as expected.

## 🗄️ Test Files and Their Focus

*   **`config_tests.rs`**: Verifies the correct creation, parsing, and management of application configuration, including default values and environment variable overrides.
*   **`router_tests.rs`**: Tests the logic for selecting LLM providers, ensuring correct availability detection and fallback routing mechanisms.
*   **`token_store_tests.rs`**: Validates the end-to-end encryption, storage, retrieval, and decryption processes for authentication tokens.
*   **`chat_runtime_tests.rs`**: Focuses on the core chat runtime's behavior, including stream event flow, persistence mechanisms, context handling, and retry logic.
*   **`ollama_provider_tests.rs`**: Specifically tests the integration and behavior of the Ollama LLM provider.

## 🚀 How to Run Core Tests

To execute all integration tests for the `core` crate, navigate to the `core/` directory from the project root and run the following command:

```bash
cd core
cargo test
```

This command will discover and run all tests defined within this directory and its submodules, providing detailed output on their success or failure.