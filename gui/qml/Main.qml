import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: root
    visible: true
    width: 1040
    height: 760
    minimumWidth: 480
    minimumHeight: 400
    title: "MultiLink"

    // Estado del tema (false=light, true=dark)
    property bool isDark: false

    // Colores del tema según modo
    readonly property color colorBackground:      isDark ? "#1A1A2E" : "#F5F6F7"
    readonly property color colorSurface:         isDark ? "#16213E" : "#FFFFFF"
    readonly property color colorSurfaceAlt:      isDark ? "#0F3460" : "#F0F0F0"
    readonly property color colorBorder:          isDark ? "#2A2A4A" : "#E0E0E0"
    readonly property color colorTextPrimary:     isDark ? "#EAEAEA" : "#1E1E1E"
    readonly property color colorTextSecondary:   isDark ? "#9E9E9E" : "#6B6B6B"
    readonly property color colorTextMuted:       isDark ? "#666688" : "#999999"
    readonly property color colorAccentLocal:     isDark ? "#4CAF50" : "#2E7D32"
    readonly property color colorAccentRemote:    isDark ? "#64B5F6" : "#1565C0"
    readonly property color colorAccentWarning:   isDark ? "#FFB74D" : "#ED6C02"
    readonly property color colorAccentError:     isDark ? "#EF5350" : "#C62828"
    readonly property color colorBubbleUser:      isDark ? "#1B5E20" : "#E8F5E9"
    readonly property color colorBubbleAssistant: isDark ? "#16213E" : "#FFFFFF"
    readonly property color colorBubbleThinking:  isDark ? "#37474F" : "#ECEFF1"
    readonly property color colorBubbleThinkingText: isDark ? "#90A4AE" : "#455A64"
    readonly property color colorCodeBackground:  "#1E2A32"
    readonly property color colorCodeBorder:      "#3D4F5F"
    readonly property color colorCodeText:        "#D8DEE9"
    readonly property color colorCodeLanguage:    "#61AFEF"
    readonly property color colorInputBackground: isDark ? "#0F3460" : "#FAFAFA"
    readonly property color colorHoverOverlay:    isDark ? "#4A5A6A" : "#E0E0E0"

    // Fondo de la ventana según tema
    color: colorBackground

    header: ToolBar {
        implicitHeight: Math.max(40, tabLayout.implicitHeight + 12)
        RowLayout {
            id: tabLayout
            anchors.fill: parent
            anchors.margins: 6

            Label {
                text: "MultiLink"
                font.bold: true
                color: colorTextPrimary
                font.pixelSize: Math.max(12, Math.min(18, root.width * 0.018))
            }

            Item { Layout.fillWidth: true }

            // Toggle de tema
            Button {
                text: root.isDark ? "☀️ Claro" : "🌙 Oscuro"
                flat: true
                onClicked: {
                    root.isDark = !root.isDark
                    // Persistir en config via bridge
                    if (chatController) chatController.setThemeDark(root.isDark)
                }
            }

            TabBar {
                id: topTabs
                currentIndex: pages.currentIndex
                Layout.minimumWidth: implicitWidth
                TabButton {
                    text: "Chat"
                    implicitWidth: 64
                }
                TabButton {
                    text: "Opciones"
                    implicitWidth: 110
                    leftPadding: 16
                    rightPadding: 16
                }
                TabButton {
                    text: "Stats"
                    implicitWidth: 64
                }
            }
        }
    }

    StackLayout {
        id: pages
        anchors.fill: parent
        currentIndex: topTabs.currentIndex

        ChatView {
            controller: chatController
            colorBackground: root.colorBackground
            colorSurface: root.colorSurface
            colorBorder: root.colorBorder
            colorTextPrimary: root.colorTextPrimary
            colorTextSecondary: root.colorTextSecondary
            colorLocal: root.colorAccentLocal
            colorRemote: root.colorAccentRemote
            colorWarning: root.colorAccentWarning
            colorError: root.colorAccentError
            colorBubbleUser: root.colorBubbleUser
            colorBubbleAssistant: root.colorBubbleAssistant
            colorBubbleThinking: root.colorBubbleThinking
            colorBubbleThinkingText: root.colorBubbleThinkingText
            colorCodeBackground: root.colorCodeBackground
            colorCodeBorder: root.colorCodeBorder
            colorCodeText: root.colorCodeText
            colorCodeLanguage: root.colorCodeLanguage
            colorInputBackground: root.colorInputBackground
            colorHoverOverlay: root.colorHoverOverlay
        }

        Loader {
            active: topTabs.currentIndex === 1
            sourceComponent: Settings {
                controller: chatController
            }
        }

        Loader {
            active: topTabs.currentIndex === 2
            sourceComponent: StatsView {
                controller: chatController
                colorBackground: root.colorBackground
                colorSurface: root.colorSurface
                colorSurfaceAlt: root.colorSurfaceAlt
                colorBorder: root.colorBorder
                colorTextPrimary: root.colorTextPrimary
                colorTextSecondary: root.colorTextSecondary
                colorTextMuted: root.colorTextMuted
                colorAccentLocal: root.colorAccentLocal
                colorAccentRemote: root.colorAccentRemote
                colorAccentWarning: root.colorAccentWarning
                colorAccentError: root.colorAccentError
            }
        }
    }
}
