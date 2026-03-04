# Contributing to MultiLink

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR_USERNAME/MultiLink.git`
3. Create a branch: `git checkout -b feature/your-feature`

## Development Setup

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Qt6 (Ubuntu)
sudo apt install qt6-base-dev qt6-declarative-dev cmake build-essential libgl1-mesa-dev libxkbcommon-dev

# Build
cmake -S gui -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --config Release
```

## Code Style

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

Types:
- feat:     New feature
- fix:      Bug fix
- docs:     Documentation only
- refactor: Code change that neither fixes nor adds
- chore:    Maintenance, deps, build changes
- test:    Adding or updating tests
```

Examples:
```
feat(core): add intent/hardware budgeting and execution dispatcher
fix(gui): load app config and guard qt policies by version
docs: document multi-server troubleshooting workflow
```

## Pull Request Process

### Before Submitting
1. **Run tests**:
   ```bash
   cargo test --manifest-path core/Cargo.toml
   ```
2. **Run linters**:
   ```bash
   cargo fmt --manifest-path core/Cargo.toml
   cargo clippy --manifest-path core/Cargo.toml -- -D warnings
   ```
3. **Build the GUI**:
   ```bash
   cmake -S gui -B build -DCMAKE_BUILD_TYPE=Release
   cmake --build build --config Release
   ```

### PR Requirements
- Clear title matching commit style
- Description explaining the "why" and "what"
- Link related issues if applicable
- All tests passing
- No clippy warnings

### Review Criteria
- Does it follow the architecture principles?
- Are there adequate tests?
- Is the code readable and documented where needed?
- Does it maintain backward compatibility?

## Architecture Principles

- **Core-first**: Business logic lives in Rust (`core/`), not in Qt
- **Thin bridge**: C++ shim only adapts types, no business logic
- **Declarative UI**: QML only renders state, never makes network calls
- **Async everywhere**: Use `tokio` for async operations in core

## Testing

- Unit tests: `core/tests/` and inline `#[cfg(test)]`
- FFI tests: `gui/rust/chat_controller/`
- Run all: `cargo test --all`

## Questions?

Open an issue for discussion before starting major changes.
