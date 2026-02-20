# QML (`gui/qml/`)

Declarative UI only.

## Files

- `Main.qml`: app window and root navigation container.
- `ChatView.qml`: main chat UI (messages, input, selectors, status rendering).
- `Settings.qml`: settings view shell.

## QML contract

QML must not:

- call HTTP APIs
- parse stream protocols
- persist sessions to disk

QML should only render data and send user intent to `chatController`.
