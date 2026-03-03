# `tests/` - Project-Wide Testing Strategy

This directory outlines the comprehensive testing strategy for the MultiLink project, ensuring the stability, correctness, and reliability of both its core functionalities and graphical user interface. A robust testing suite is crucial for maintaining high code quality and facilitating confident development.

## 🗄️ Where Tests Reside

Tests are strategically placed throughout the project to ensure targeted coverage:

*   **`core/tests/`**: This directory contains essential integration tests specifically for the `multilink-core` crate. These tests cover critical backend behaviors such as:
    *   **Runtime Logic**: Verifying the chat runtime's lifecycle, streaming, and persistence.
    *   **Router Behavior**: Confirming correct provider selection and fallback mechanisms.
    *   **Configuration Management**: Ensuring proper loading, parsing, and application of settings.
    *   **Token Store Security**: Validating the encryption and management of authentication tokens.
*   **`gui/rust/chat_controller/` crate**: This crate includes dedicated tests for the Foreign Function Interface (FFI) layer that bridges the Rust core with the Qt GUI. These are typically smoke tests and lifecycle tests to ensure the C++ shim can correctly interact with the Rust backend.
*   **GUI Visual Tests**: While not explicitly shown as files here, the GUI is expected to have visual tests (e.g., using Qt's testing frameworks or manual checks). Automated visual tests, potentially using offscreen rendering for quick verification, are encouraged to ensure UI consistency and responsiveness.

## ✅ Minimum Test Requirements

*   **Core Tests are Mandatory**: All tests within `core/tests/` are considered mandatory and are automatically executed as part of the continuous integration (CI) pipeline and local development checks.
*   **GUI FFI Tests**: Tests within the `gui/rust/chat_controller/` crate are also mandatory to ensure the integrity of the communication layer.

## 🚀 Suggested Pull Request (PR) Checklist for Testing

Before submitting a Pull Request, contributors should perform the following steps to ensure their changes meet the project's quality standards:

1.  **Run Core Tests**:
    ```bash
    cd core
    cargo test
    ```
    Verify that all core unit and integration tests pass.
2.  **Run GUI FFI Tests**:
    ```bash
    cd gui/rust/chat_controller
    cargo test
    ```
    Ensure that the tests for the GUI-Rust bridge pass without errors.
3.  **Perform a Full GUI Build**:
    ```bash
    # From the repository root
    cmake -S gui -B build -DCMAKE_BUILD_TYPE=Release
    cmake --build build --config Release
    ```
    Confirm that the GUI application compiles successfully after your changes.
4.  **(Optional but Recommended) Manual GUI Verification**: Launch the compiled GUI application and manually test the affected features to ensure proper functionality and visual correctness.