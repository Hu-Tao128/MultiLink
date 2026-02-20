import QtQuick
import QtQuick.Controls

ApplicationWindow {
    visible: true
    width: 980
    height: 680
    title: "MultiLink"

    StackView {
        anchors.fill: parent
        initialItem: ChatView { controller: chatController }
    }
}
