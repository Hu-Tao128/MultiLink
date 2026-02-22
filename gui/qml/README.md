# QML (`gui/qml/`)

Declarative UI only.

## Files

- `Main.qml`: app window and root navigation container.
- `ChatView.qml`: main chat UI (messages, input, selectors, status rendering, code/text copy affordances, scroll helpers).
- `Settings.qml`: settings view shell.

## QML contract

QML must not:

- call HTTP APIs
- parse stream protocols
- persist sessions to disk

QML should only render data and send user intent to `chatController`.

Current UI affordances in chat:

- Message segmentation between plain text and fenced code blocks.
- Read-only selectable text for assistant output to support copy/paste workflows.
- Per-code-block copy button and assistant-message copy button (clipboard write is delegated to controller).
