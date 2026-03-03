# `core/src/` - MultiLink Core Source Code

This directory contains the primary Rust source code for the MultiLink core library. It's structured to clearly separate concerns related to chat runtime, configuration, session management, and integrations with external services and models. For a higher-level overview of the `core` crate, refer to the [main `core/README.md`](../README.md).

## 🗄️ Core Files and Their Responsibilities

*   **`lib.rs`**: The crate's public interface and main entry point. It defines the public modules and types exposed by the `core` library.
*   **`chat_runtime.rs`**: Manages the life-cycle of chat interactions. This includes handling streaming events, orchestrating session state, implementing throttled persistence, managing cancellation signals, assembling and summarizing context, and handling fallback retries in cases of potential context overflow.
*   **`router.rs`**: Responsible for dynamically selecting and routing requests to the appropriate Large Language Model provider based on application configuration and provider availability. It also manages fallback strategies.
*   **`session.rs`**: Defines the data structures and logic for managing individual chat sessions, including message history, metadata, and state transitions.
*   **`config.rs`**: Provides the application's configuration model, handling default values, environment variable overrides, and deserialization/serialization of settings.
*   **`context_retrieval.rs`**: Implements the logic for building and managing the context provided to LLMs. This includes token budgeting and intelligent injection of project-specific context per session.

## ⚙️ Runtime Design Principles

*   **Token-Budgeted Context**: Context building is carefully managed with a token budget to prevent exceeding provider limits. This can include caching and injecting project-specific context per session.
*   **Resilient Error Handling**: Upon detecting provider errors that suggest a context-window overflow, the runtime is designed to retry the request once, automatically excluding project-specific context to increase the chance of success.
*   **Decoupled Persistence & Streaming**: Persistence operations and the emission of stream events are separated. This ensures that the UI remains responsive with real-time updates while minimizing disk I/O churn.

## 📂 Subdirectories

*   **`providers/`**: Contains the trait definitions for LLM providers and their concrete implementations (e.g., Ollama, Gemini). This module abstracts interactions with different model APIs.
*   **`auth/`**: Manages authentication flows, including OAuth, and provides secure storage solutions for authentication tokens.
*   **`model_manager/`**: Facilitates the discovery, registration, migration, and management of various LLM models available to the application.
*   **`system/`**: Offers utilities for interacting with the operating system, such as resolving platform-specific data directories and environment checks.

## 🎯 Guiding Principle

If a component's state is designed to change over time (e.g., chat messages, streaming responses, retry logic, persistent data), its implementation logically belongs within this `core/src/` directory.