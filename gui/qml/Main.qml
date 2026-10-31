import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Theme 1.0

ApplicationWindow {
    id: root
    visible: true
    width: 1040
    height: 760
    minimumWidth: 480
    minimumHeight: 400
    title: "MultiLink"

    // Fondo de la ventana según tema
    color: Theme.background

    header: ToolBar {
        implicitHeight: Math.max(40, tabLayout.implicitHeight + 12)
        RowLayout {
            id: tabLayout
            anchors.fill: parent
            anchors.margins: 6

            Label {
                text: "MultiLink"
                font.bold: true
                color: Theme.textPrimary
                font.pixelSize: Math.max(12, Math.min(18, root.width * 0.018))
            }

            Item { Layout.fillWidth: true }

            // Toggle de tema
            Button {
                text: Theme.isDark ? "☀️ Claro" : "🌙 Oscuro"
                flat: true
                onClicked: {
                    Theme.isDark = !Theme.isDark
                    // Persistir en config via bridge
                    if (chatController) chatController.setThemeDark(Theme.isDark)
                }
            }

            TabBar {
                id: topTabs
                currentIndex: pages.currentIndex
                TabButton { text: "Chat" }
                TabButton { text: "Opciones" }
                TabButton { text: "Stats" }
            }
        }
    }

    StackLayout {
        id: pages
        anchors.fill: parent
        currentIndex: topTabs.currentIndex

        ChatView {
            controller: chatController
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
            }
        }
    }
}
