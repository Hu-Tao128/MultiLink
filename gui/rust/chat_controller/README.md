# `gui/rust/chat_controller/` - Chat Controller Backend

This Rust crate functions as the direct Foreign Function Interface (FFI) layer between the C++ Qt shim (`gui/src/chatcontroller.*`) and the main `multilink-core` crate. It is compiled as a static library and linked directly into the Qt GUI executable. Its primary role is to translate GUI-initiated actions into calls to the core Rust logic and to forward core-generated events and data back to the GUI.

## 🗄️ Files and Their Responsibilities

*   **`Cargo.toml`**: Defines this crate's metadata, dependencies, and build configuration. It ensures that `multilink-core` is correctly linked as a dependency.
*   **`src/lib.rs`**: This file contains the FFI API exposed to the C++ shim. It acts as the intermediary, delegating chat operations (such as sending prompts, stopping streams, or managing sessions) to the `multilink-core::ChatRuntime` and handling the data flow back to the GUI.

## 🎯 Key Responsibilities

The `chat_controller` crate is responsible for:

*   **`ChatRuntime` Management**: Creating and managing the lifecycle of a single `multilink-core::ChatRuntime` instance.
*   **Action Forwarding**: Reliably forwarding user-initiated actions from the GUI to the `multilink-core`, including:
    *   Sending new chat prompts.
    *   Stopping ongoing streaming responses.
    *   Creating new chat sessions.
    *   Selecting active sessions or models.
*   **Event Emission to UI**: Emitting structured stream callbacks for the UI to render real-time updates:
    *   `started`: Signifying the beginning of a stream.
    *   `chunk`: Delivering incremental parts of the response.
    *   `finished`: Indicating the completion of a stream.
    *   `error`: Reporting any errors encountered during the stream.
*   **Data Provision**: Providing JSON payloads containing session data and available model lists to the GUI.
*   **Provider Health Tracking**: Maintaining and exposing UI-facing provider health status, carefully distinguishing between transport connectivity failures (e.g., network issues) and prompt/content-specific failures.

## 📝 Runtime Behavior Notes

*   **`provider_health` State**: The `provider_health` status transitions to `starting` while a prompt send operation is in flight.
*   **Stream Start Health**: Upon the successful start of a streaming response, the provider's health state moves to `available`.
*   **Error Handling and Health Status**: If an error occurs, the provider's health is set to `unavailable` only for issues likely related to connection problems (e.g., timeouts, connection refused, socket errors, DNS resolution failures). For other types of errors (e.g., content-related, model refusals), the health status remains `available`.

## 🛠️ Local Development Commands

To build and test this specific crate during local development:

```bash
# Build the chat_controller in release mode
cargo build --release

# Run tests for the chat_controller
cargo test
```