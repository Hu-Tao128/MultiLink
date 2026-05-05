# `docs/` - Project Documentation

This directory serves as the central repository for comprehensive documentation related to the MultiLink project. It provides in-depth insights into the application's architecture, design decisions, specific module behaviors, security models, and future development plans. This documentation is invaluable for both new contributors seeking to understand the project and existing developers needing detailed references.

## 🗄️ Documentation Files

*   **`architecture.md`**: Delve into the core architectural design of MultiLink. This document details the layered structure, component interactions, and includes flow diagrams to visually explain how different parts of the application communicate and operate.
*   **`CODING_AGENT_MVP.md`**: Canonical operational plan for turning MultiLink into a real coding agent, including current tool inventory, gaps, guardrails, and acceptance gates.
*   **`providers.md`**: Provides an in-depth look at the LLM provider interface and the expected behavior of provider implementations. It covers the contracts, design principles, and guidelines for integrating new Large Language Model services.
*   **`auth.md`**: Explains MultiLink's authentication and security model, specifically focusing on the OAuth flow, token management, and strategies for securing user credentials and sensitive data.
*   **`roadmap.md`**: Outlines the project's implementation timeline, key milestones, and future development plans, giving an overview of where MultiLink is headed.
*   **`network-troubleshooting.md`**: Practical guide for remote Ollama connectivity (LAN/Tailscale), including bind mode, UFW rules, and verification commands.
*   **`CONTEXT_AB_VALIDATION.md`**: Reproducible A/B protocol for validating context engine quality/performance (`v2` vs `v2plus`) with prompt suite and promotion/rollback criteria.
*   **`CONTEXT_AB_RESULTS_TEMPLATE.md`**: Template for recording A/B metrics, reasons, and final release decision.
*   **`LSP_ROADMAP.md`**: Tracks the experimental semantic LSP server, including remaining editor validation and live Context Engine bridge work.

## 🤝 Contribution Guidelines

When making changes that affect the architectural design, core contracts, or significant features of MultiLink, it is crucial to:

*   **Update Relevant Documentation**: Ensure that all related documentation files within this `docs/` directory are updated in the same Pull Request. This practice helps maintain consistency and ensures that the documentation accurately reflects the current state of the codebase.
*   **Clarity and Detail**: Strive for clarity, accuracy, and sufficient detail in your documentation updates, making it easy for other contributors to understand the changes and their implications.
