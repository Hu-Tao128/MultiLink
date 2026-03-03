# `core/src/system/` - System-Level Utilities

This module within the `core` crate provides essential operating system and system-level utility functions. These utilities are designed to interact with the underlying OS environment without any direct user interface components, focusing on tasks such as detecting external software, managing system-specific paths, and ensuring secure operations.

## 🗄️ Files and Their Responsibilities

*   **`mod.rs`**: This file acts as the primary hub for system-level helpers. Its responsibilities include:
    *   **Ollama Installation Detection**: Logic to detect if Ollama is installed on the user's system.
    *   **Installation Command Planning**: Describes and potentially stages commands necessary for installing external dependencies like Ollama.
    *   **Installation Verification**: Verifies the success of installation steps.
    *   **Platform-Specific Paths**: Resolves standard system directories for data, configuration, and caches in a cross-platform manner.

## 🔒 Safety Contract & Principles

The `system` module operates under strict safety principles to protect user systems:

*   **Explicit User Consent for Privileged Commands**: Critical commands that modify the system or require elevated privileges are *never* executed without explicit, informed user consent.
*   **Deterministic and Side-Effect Aware**: All operations within this module are designed to be as deterministic as possible. Any potential side effects on the user's system are carefully considered, documented, and minimized.
*   **Minimal Interaction**: This module avoids any UI interaction, focusing purely on backend system queries and operations.