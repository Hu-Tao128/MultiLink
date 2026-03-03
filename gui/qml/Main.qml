import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: root
    visible: true
    width: 1040
    height: 760
    title: "MultiLink"

    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.margins: 6

            Label {
                text: "MultiLink"
                font.bold: true
                font.pixelSize: 18
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

        Settings {
            controller: chatController
        }
    }
}
