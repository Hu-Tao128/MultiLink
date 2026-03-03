# `core/src/auth/` - Authentication Module

This module within the `core` crate is dedicated to handling authentication and token management for remote Large Language Model (LLM) providers. Its primary responsibility is to securely manage the OAuth flow, including authorization, token storage, and lifecycle management, ensuring that sensitive credentials are never exposed or mishandled.

## 🗄️ Files and Their Responsibilities

*   **`mod.rs`**: The module's entry point, defining the public interface and re-exporting key components.
*   **`oauth.rs`**: Implements the core OAuth 2.0 protocol logic. This includes generating authorization URLs, handling the localhost callback from the browser, performing code exchange for access tokens, refreshing expired tokens, and revoking tokens.
*   **`token_store.rs`**: Manages the secure storage of authentication tokens at rest. It utilizes encryption (currently AES-256-GCM) to protect sensitive token data on the file system.
*   **`service.rs`**: Provides a high-level, simplified interface for the authentication workflow, intended to be used by callers (e.g., the `chat_runtime` or GUI shim). It abstracts away the complexities of OAuth and token storage.

## 🔒 Security Best Practices & Notes

*   **Credential Protection**: A strict policy is enforced to keep all sensitive credentials and tokens out of logs and transient memory.
*   **Stable Storage API**: The token storage API is designed to be stable, ensuring that the GUI interacts only with the high-level service methods, never directly with raw token data.
*   **Encrypted Storage**: Current token storage employs encrypted files (AES-256-GCM) on the local filesystem.
*   **Future Enhancements**: Integration with native system keyrings is planned as the next major security enhancement for even more robust credential protection.
*   **Rust-Owned Logic**: The entire OAuth login URL generation, callback handling, and token refresh logic are implemented and owned by the Rust core, ensuring consistent and secure behavior independent of the GUI.