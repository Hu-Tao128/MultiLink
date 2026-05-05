# MultiLink Project Overview

MultiLink is a cross-platform desktop application designed for interacting with Large Language Models (LLMs). It features a native Qt/QML graphical user interface (GUI) and a robust, reusable Rust core. The architecture prioritizes performance, stability, and clear separation of concerns.

Product direction: MultiLink is being turned into a practical local-first coding agent, not only a chat UI. Treat `docs/CODING_AGENT_MVP.md` as the source of truth for coding-agent behavior, current tool inventory, missing capabilities, and acceptance gates.

**Key Technologies:**
*   **Rust:** For the high-performance, memory-safe backend core.
*   **Qt/QML:** For the cross-platform, responsive desktop user interface.
*   **C++:** Acts as a minimal "shim" layer to adapt Rust's Foreign Function Interface (FFI) to Qt's QObject system.

**Architecture Highlights:**
The application follows a layered architecture: `GUI (Qt/QML)` -> `Qt Shim (C++ QObject adapter)` -> `Rust Backend (Static Library via FFI)` -> `Core (Rust)`. The Rust core handles critical functionalities like LLM integrations, authentication, model management, and configuration, ensuring performance and security.

## Building and Running MultiLink

To get MultiLink up and running, ensure you have the necessary prerequisites and follow the build steps below.

### Prerequisites

*   **Rust:** A stable toolchain (install via [rustup](https://rustup.rs/)).
*   **CMake:** Version 3.21 or higher.
*   **Qt 6:** (6.5+ recommended) with `QML` and `Quick` modules.

**Platform-Specific Setup:**

*   **Linux (e.g., Ubuntu 24.04+):**
    ```bash
    sudo apt update
    sudo apt install -y qt6-base-dev qt6-declarative-dev cmake build-essential libgl1-mesa-dev libxkbcommon-dev
    ```
*   **macOS:**
    ```bash
    brew install qt cmake ninja
    ```
*   **Windows:**
    Install via [Chocolatey](https://chocolatey.org/):
    ```bash
    choco install cmake ninja -y
    # For Qt6, use the official online installer or 'install-qt-action' in CI/CD.
    ```

### Build Steps

From the repository root directory, execute the following commands:

```bash
# Configure the build system (creates 'build' directory)
cmake -S gui -B build -DCMAKE_BUILD_TYPE=Release

# Compile the project
cmake --build build --config Release
```

After compilation, the executable will be located in the `build/` directory (or `build/Release` on Windows). You can run it directly from there.

## Development Conventions

### Engineering Principles

*   **Performance and Correctness First:** These are prioritized over immediate interface polish.
*   **Rust Core as Backbone:** All core logic for streaming, memory, and context handling resides in Rust.
*   **Declarative QML:** UI layers focus solely on rendering state and dispatching user intent, without directly handling networking or disk operations.
*   **Thin Qt Shim:** The C++ adapter logic is minimized, focusing exclusively on data adaptation between Rust FFI and Qt properties/signals.
*   **Agent Work Must Be Verifiable:** Coding-agent features must use deterministic tools, path guards, explicit edits, and validation commands instead of relying on model text alone.

### Coding Agent Boundaries

*   Read-only tools currently include `fs_ls`, `fs_cat`, `fs_grep`, `search_code`, `open_file`, `search_and_open`, and `system_version`.
*   Write support is currently explicit and limited through `/write-file`; structured patch editing is still pending.
*   Do not document a feature as complete unless there is code and a test or manual validation evidence.
*   Safe command execution must be allowlisted; avoid adding a general shell tool as the default path.
*   Keep business logic in `core/`; QML and C++ only expose user intent and render results.

### Running Tests

To maintain code quality, tests are a critical part of the development workflow.

*   **Rust Core Tests:**
    Navigate to the `core/` directory and run:
    ```bash
    cargo test
    ```
*   **Rust GUI Backend Tests:**
    Navigate to `gui/rust/chat_controller/` and run:
    ```bash
    cargo test
    ```
(Ensure `cargo` is installed via `rustup`.)
