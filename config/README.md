# `config/` - Configuration Templates

This directory contains default configuration templates for the MultiLink application. These templates serve as a baseline for the application's settings, ensuring that MultiLink can run out-of-the-box with sensible defaults, particularly prioritizing local-first operations and user privacy.

## 🗄️ Files and Their Responsibilities

*   **`default.toml`**: Baseline template used when a user config is missing.
    *   Uses schema `version = 2`.
    *   Defines one or more `[[servers]]` entries.
    *   Includes `[context]`, `[performance]`, `[network]`, `[ui]`, and `[storage]` sections.

## 📍 User Config Location

MultiLink resolves user config from:

*   `~/.config/multilink/multilink.toml` (preferred)
*   `~/.config/multilink/config.toml` (legacy fallback)

Legacy v1 configs are migrated to v2 automatically on load.

## 📝 Principles for Configuration

*   **Safe and Local-First Defaults**: All default settings are chosen to be secure, privacy-respecting, and to enable local-first workflows wherever possible (e.g., defaulting to local Ollama instances).
*   **Core Handles Overrides**: The `multilink-core` configuration module (`core/src/config.rs`) is responsible for handling runtime overrides (e.g., via environment variables) and merging user-specific configurations with these baseline defaults. This ensures a consistent and predictable configuration resolution process.
*   **User-Modifiable**: Users can typically override these defaults via application settings in the GUI or by placing a custom configuration file in the appropriate user data directory (as managed by `multilink-core::system`).
