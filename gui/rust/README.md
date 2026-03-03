# `gui/rust/` - GUI-Facing Rust Code

This directory contains Rust code specifically designed for integration with the MultiLink graphical user interface. Unlike the primary `core` crate, which is a reusable and platform-agnostic backend, the code within this directory directly supports the GUI's needs, often serving as a Foreign Function Interface (FFI) layer.

## 🗄️ Folders

*   **`chat_controller/`**: This subdirectory contains a Rust library that is compiled into a static backend, which is then linked by CMake into the Qt executable. It facilitates communication between the C++ Qt shim (`gui/src/chatcontroller.cpp/.h`) and the main Rust `core` crate. Its responsibilities include:
    *   **FFI Definitions**: Defining the interfaces that allow the C++ layer to call Rust functions and vice-versa.
    *   **GUI-Specific Adapters**: Implementing any necessary adapters or logic to transform data between the GUI's expected format and the `core`'s data structures.
    *   **Event Handling**: Managing events and callbacks that originate from the Rust core and need to be forwarded to the GUI.

## 💡 Purpose

The primary purpose of this `gui/rust/` directory is to provide a clean and efficient bridge between the Qt/QML frontend and the powerful `core` Rust backend. It ensures that GUI-specific interactions and data flows can be handled in a type-safe and performant manner, while keeping the core business logic entirely separated and reusable.