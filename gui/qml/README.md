# `gui/qml/` - Declarative User Interface

This directory contains all the QML (Qt Modeling Language) files that define the declarative user interface for MultiLink. As the purely visual layer of the application, these files are responsible for rendering the application's state and capturing user interactions, which are then passed to the C++ shim and the Rust core.

## 🗄️ Files and Their Responsibilities

*   **`Main.qml`**: Serves as the application's main window and the root container for navigation. It orchestrates the display of different views and overall application layout.
*   **`ChatView.qml`**: Implements the primary chat user interface. This includes displaying messages, providing input fields, selectors, rendering status information, and incorporating features like code/text copy affordances and scroll helpers.
*   **`Settings.qml`**: Defines the user interface for application settings, allowing users to configure various aspects of MultiLink.

## 📐 Design Principles for QML

The QML layer adheres to strict design principles to maintain a clear separation of concerns and ensure maintainability:

*   **Purely Declarative UI**: QML is used solely for defining the appearance and behavior of the user interface. It should not contain any complex business logic.
*   **No Direct External Communication**: QML must **not** perform direct HTTP API calls, parse streaming protocols, or handle persistence operations to disk. These responsibilities belong to the Rust core.
*   **Data Rendering and Intent Dispatch**: The primary functions of QML are to render data received from the `chatController` (the C++ shim) and to dispatch user intents (actions) back to it.

### Current UI Affordances in Chat

*   **Message Segmentation**: Chat messages are intelligently segmented to differentiate between plain text and fenced code blocks, enhancing readability.
*   **Read-Only Selectable Text**: Assistant output is rendered as read-only selectable text, facilitating easy copy-paste workflows for users.
*   **Contextual Copy Actions**: Provides specific copy buttons for individual code blocks within messages, as well as a general copy button for the entire assistant message. Clipboard write operations are delegated to the `chatController` in the C++ layer for platform-agnostic handling.