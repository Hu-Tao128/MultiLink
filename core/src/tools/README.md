# `core/src/tools/` - Deterministic Agent Tools

This module contains the internal tool registry used by the coding-agent runtime. Tools here must be deterministic, scoped to the active project root, and safe to call from planner/executor flows.

## Current Tools

- `search_code`: uses the Context Engine to retrieve relevant files/snippets.
- `open_file`: reads a relative file with line limits, chunking, and optional in-file search.
- `search_and_open`: combines retrieval with file opening.
- `fs_ls`: lists files/directories under the project root.
- `fs_cat`: reads a file under the project root.
- `fs_grep`: performs pure-Rust textual search without invoking shell.
- `git_status`: reads branch and worktree status without modifying git state.
- `git_diff`: reads unstaged worktree diffs for the project or a relative path.
- `system_version`: detects versions of common development tools.
- `write_file`: writes content to a file with size limit (1MB), path guard, and diff summary (lines added/removed).
- `apply_patch`: applies a unified diff to a file with hunk-level validation and path guard.
- `run_command`: executes allowlisted validation/build commands (built-in + detected by /init in MULTILINK.md).

## Safety Contract

- Paths must remain inside the active project root.
- Read tools must not mutate files, git state, config, or environment.
- Shell execution does not belong here until an allowlisted `run_command` tool exists.
- Write tools must return traceable evidence such as diff, hash, or before/after metadata.
- Secrets should be redacted or reported by presence only.

## Next Tools (post-MVP)

- Expose tools via MCP for remote agent use.
- Rollback capability for write_file/apply_patch.
- Semantic search for patches (find what changed across sessions).
