# `core/src/commands/` - Slash Commands

This module contains user-facing commands parsed from chat input. Commands are explicit operations and should stay predictable.

## Commands

- `/init`: scans the active project and creates, merges, or analyzes `MULTILINK.md`.
- `/doctor`: runs project diagnostics and reports quality issues.
- `/doctor --security`: adds security-oriented checks and LAN secret reporting/generation.
- `/write-file <relative-path>`: writes explicit content to a relative path inside the project.

## Boundaries

- Commands may inspect or write project files only through path-guarded logic.
- Commands should not become a hidden shell.
- Any generated analysis must be constrained by real scan output; provider text must not invent facts.
- New commands need tests in `core/tests/` or module tests.

## Agent Role

Commands provide project bootstrap and explicit user actions. The autonomous coding-agent loop should use internal tools for routine inspect/edit/validate steps, while commands remain stable entry points for users.
