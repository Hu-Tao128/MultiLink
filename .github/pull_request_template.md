## What changed

- Briefly describe the change in 2-4 bullets.
- Focus on behavior and intent, not only file edits.

## Why this change

- What problem does this solve?
- Why is this the right layer for the change (QML / shim / Rust backend / core)?

## Architecture checklist

- [ ] QML remains declarative (no HTTP, no persistence logic).
- [ ] C++ shim remains thin (type adaptation only).
- [ ] Rust backend/core owns runtime state, streaming, and persistence.
- [ ] No duplicated business logic across layers.

## Files touched

- List key files and why they were modified.
- Example: `core/src/chat_runtime.rs` (stream throttling behavior).

## Testing done

- [ ] `cd core && cargo test`
- [ ] `cd gui/rust/chat_controller && cargo test`
- [ ] `cmake -S gui -B build/gui && cmake --build build/gui`
- [ ] Manual smoke run (if UI-affecting): `QT_QPA_PLATFORM=offscreen ./build/gui/multilink`

## Screenshots / demo (if UI changes)

- Add before/after screenshots or a short clip.
- Place assets in `gui/assets/screenshots/` when applicable.

## Risks and rollback

- Note any migration risk, behavior change, or compatibility concern.
- Describe a quick rollback path.

## Follow-ups

- Optional small items to address in subsequent PRs.
