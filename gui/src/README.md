# `gui/src/` - GUI C++ Source

This directory contains the C++ source code for the MultiLink graphical user interface. Its primary function is to act as a critical, thin adapter layer between the declarative QML frontend and the powerful Rust backend. This layer is intentionally kept minimal to ensure a clear separation of concerns, high performance, and ease of maintenance.

## 🗄️ Files and Their Responsibilities

*   **`main.cpp`**: This is the application's entry point for the Qt GUI. It handles the bootstrap process for the Qt application, sets up the `QQmlApplicationEngine`, and crucialy injects the `chatController` object into the QML context, making the Rust-backed functionalities accessible from QML.
*   **`chatcontroller.h` / `chatcontroller.cpp`**: These files define and implement the `ChatController` class. This class serves as the shim between QML and the Rust backend. Its responsibilities include:
    *   Converting Rust FFI payloads into Qt properties and signals that QML can easily consume.
    *   Exposing C++ invokable methods that QML can call (e.g., for clipboard copy operations).
    *   Forwarding user actions and data from QML to the Rust backend for processing.
    *   Managing the communication channels with the `gui/rust/chat_controller` static library.
*   **`bridge.rs`**: (Note: This file appears to be for legacy or prototype bridge experiments and is not part of the primary runtime path.)

## 🎯 Key Design Principle

To maintain a clean architecture and prevent unintended side effects, a strict design principle is enforced in this layer:

**No networking, persistence, or LLM provider-specific logic belongs here.**

All such complex business logic must reside within the `core` Rust crate. This C++ layer is purely for type adaptation, signal/slot mediation, and action forwarding.