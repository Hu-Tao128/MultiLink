# `gui/` - Graphical User Interface (GUI)

This directory contains the source code for the MultiLink graphical user interface, built using Qt and QML. It serves as the user-facing component of the application, providing an intuitive and responsive desktop experience. The GUI interacts with the robust Rust core through a thin C++ bridge.

## 🗄️ What This Folder Contains

*   **`CMakeLists.txt`**: The CMake build configuration script that orchestrates the compilation of the Qt/QML application and automatically triggers the build of the Rust backend (`rust/chat_controller`).
*   **`src/`**: Contains the C++ source files, including the application entry point (`main.cpp`) and the thin Qt shim (`chatcontroller.h`, `chatcontroller.cpp`). This shim acts as the bridge, adapting Rust FFI data into Qt-friendly properties and signals for the QML frontend.
*   **`qml/`**: Houses all the declarative QML files that define the user interface screens, layouts, and interactive components (e.g., `Main.qml`, `ChatView.qml`, `Settings.qml`).
*   **`assets/`**: Stores static assets such as images, icons, and screenshots used within the application or for documentation purposes.
*   **`rust/chat_controller/`**: This subdirectory contains a Rust library that is compiled into a static backend, specifically designed to be linked into the Qt application. It encapsulates GUI-specific Rust logic or components that directly interface with the GUI through FFI.

## 🚀 Building the GUI

The GUI build process is integrated with the Rust backend compilation. When you build the GUI, CMake automatically handles the compilation of the necessary Rust components.

From the repository root, execute the following commands:

```bash
# Configure the build system (creates 'build' directory)
cmake -S gui -B build -DCMAKE_BUILD_TYPE=Release

# Compile the project
cmake --build build --config Release
```

For platform-specific prerequisites (Rust, CMake, Qt 6), please refer to the main [README.md](../README.md#prerequisites) in the project root.

## ✨ Current Scope & Features

*   **Comprehensive Chat/Session UX**: The user interface is fully integrated with the Rust core's runtime, providing a seamless chat and session management experience.
*   **Session Restoration**: Existing chat sessions are automatically restored upon application startup, ensuring continuity for the user.
*   **Enhanced Chat View**: The chat view supports selectable assistant text, allowing users to easily copy code blocks and full assistant messages to the clipboard.
*   **Server Settings Panel**: `qml/Settings.qml` allows creating/editing/removing servers, assigning priorities, and testing connectivity (`/api/tags`).
*   **Multi-Server Model List**: Chat model selector aggregates models from all enabled Ollama servers and tags each model with its source server.

## 👩‍💻 Development Emphasis

*   **Preserve Runtime-First Architecture**: GUI development should always adhere to the principle of keeping business logic in the `core` Rust crate. The GUI's role is to present data and capture user intent, not to manage complex application state or logic.
*   **Low-Overhead UX**: Favor user experience enhancements that are performant and visually clean, such as improved rendering, selection mechanisms, copy actions, and navigation aids, rather than introducing heavy visual complexity or animations that could impact performance.

## 🧪 Running GUI-Related Tests

To test the Rust components that directly interface with the GUI, navigate to the `gui/rust/chat_controller/` directory and execute:

```bash
cd gui/rust/chat_controller
cargo test
```
This will run any unit or integration tests specific to the `chat_controller` Rust library.
