# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2024-XX-XX

### Added
- Cross-platform Qt/QML desktop application
- Ollama provider integration with HTTP API
- Gemini and Codex provider scaffolding
- Multi-server support with priority routing
- Session persistence (JSON-based)
- Context management with project-aware retrieval
- Token usage tracking
- Server connectivity testing
- Model-aware context budgets
- Encrypted token storage (AES-256-GCM)
- Hardware and intent-based budget allocation
- Execution dispatcher with server status monitoring

### Changed
- Migrated from V1 to V2 configuration schema
- Improved markdown rendering in chat view

### Fixed
- Model selector state restoration
- Session restoration on app startup
- Qt version compatibility handling

### Documentation
- Architecture documentation
- OAuth design specification
- Provider interface contract
- Network troubleshooting guide

## [0.0.1] - 2024-02-23

### Added
- Initial release with basic chat functionality
- Core provider trait and router
