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

    header: ToolBar {
        implicitHeight: Math.max(40, tabLayout.implicitHeight + 12)
        RowLayout {
            id: tabLayout
            anchors.fill: parent
            anchors.margins: 6

            Label {
                text: "MultiLink"
                font.bold: true
                font.pixelSize: Math.max(12, Math.min(18, root.width * 0.018))
            }

            Item { Layout.fillWidth: true }

            TabBar {
                id: topTabs
                currentIndex: pages.currentIndex
                TabButton { text: "Chat" }
                TabButton { text: "Opciones" }
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
    }
}
