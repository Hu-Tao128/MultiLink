import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import MultiLink 1.0

Page {
    id: chatPage
    required property ChatController controller
    property string pendingAssistantText: ""

    readonly property color colorBackground: "#F5F6F7"
    readonly property color colorSurface: "#FFFFFF"
    readonly property color colorBorder: "#E0E0E0"
    readonly property color colorTextPrimary: "#1E1E1E"
    readonly property color colorTextSecondary: "#6B6B6B"
    readonly property color colorLocal: "#2E7D32"
    readonly property color colorRemote: "#1565C0"
    readonly property color colorWarning: "#ED6C02"
    readonly property color colorError: "#C62828"

    function currentAccent() {
        return controller.providerScope === "LOCAL" ? colorLocal : colorRemote
    }

    Rectangle {
        anchors.fill: parent
        color: colorBackground
        z: -1
    }

    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.margins: 8
            Label {
                text: "MultiLink"
                font.pixelSize: 18
                font.bold: true
            }
            Item { Layout.fillWidth: true }
            Label {
                text: controller.providerScope
                color: currentAccent()
            }
            Rectangle {
                width: 8
                height: 8
                radius: 4
                color: controller.providerHealth === "available"
                       ? colorLocal
                       : (controller.providerHealth === "starting" ? colorWarning : colorError)
            }
            Label {
                text: controller.providerHealth === "available"
                      ? (controller.activeProvider + " activo")
                      : (controller.providerHealth === "starting" ? "Iniciando" : "No disponible")
                color: colorTextSecondary
            }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 10

        RowLayout {
            Layout.fillWidth: true
            ComboBox {
                id: sessionBox
                textRole: "title"
                model: controller.sessions
                Layout.preferredWidth: 280
                onCurrentIndexChanged: {
                    if (currentIndex < 0 || currentIndex >= controller.sessions.length) {
                        return
                    }
                    controller.selectSession(controller.sessions[currentIndex].id)
                    messageModel.clear()
                }
            }
            Button {
                text: "Nueva sesion"
                onClicked: {
                    controller.newSession()
                    messageModel.clear()
                }
            }
            Item { Layout.fillWidth: true }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.alignment: Qt.AlignHCenter
            Layout.maximumWidth: 980
            color: colorSurface
            border.color: colorBorder
            radius: 8

            ListView {
                id: chatList
                anchors.fill: parent
                anchors.margins: 10
                spacing: 6
                clip: true
                cacheBuffer: 800
                reuseItems: true
                model: ListModel { id: messageModel }
                delegate: Item {
                    width: ListView.view.width
                    height: bubble.implicitHeight + 6

                    Rectangle {
                        id: bubble
                        width: Math.min(parent.width * 0.8, textItem.implicitWidth + 20)
                        implicitHeight: textItem.implicitHeight + 14
                        anchors.right: model.role === "user" ? parent.right : undefined
                        anchors.left: model.role === "assistant" ? parent.left : undefined
                        color: model.role === "user" ? "#E8F5E9" : "#FFFFFF"
                        border.color: colorBorder
                        radius: 8

                        Text {
                            id: textItem
                            anchors.fill: parent
                            anchors.margins: 7
                            color: colorTextPrimary
                            text: model.text
                            wrapMode: Text.Wrap
                        }
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            ComboBox {
                id: providerModelBox
                model: controller.availableModelsDetailed
                textRole: "label"
                Layout.preferredWidth: 340
                delegate: ItemDelegate {
                    width: providerModelBox.width
                    text: modelData.provider + " - " + modelData.label
                }
                onCurrentIndexChanged: {
                    if (currentIndex < 0 || currentIndex >= controller.availableModelsDetailed.length) {
                        return
                    }
                    const selected = controller.availableModelsDetailed[currentIndex]
                    controller.selectModel(selected.name)
                }
            }

            TextField {
                id: promptInput
                Layout.fillWidth: true
                Layout.preferredHeight: 40
                placeholderText: "Escribe tu mensaje..."
                enabled: !controller.isLoading
                onAccepted: sendButton.clicked()
            }

            Button {
                id: sendButton
                text: "Enviar"
                enabled: !controller.isLoading
                onClicked: {
                    const prompt = promptInput.text.trim()
                    if (prompt.length === 0) {
                        return
                    }
                    messageModel.append({ role: "user", text: prompt })
                    promptInput.text = ""
                    pendingAssistantText = ""
                    controller.sendPrompt(prompt)
                }
            }

            Button {
                text: "Detener"
                visible: controller.isLoading
                enabled: controller.isLoading
                onClicked: controller.stopGeneration()
            }

            BusyIndicator {
                running: controller.isLoading
                visible: controller.isLoading
            }
        }
    }

    Connections {
        target: controller
        function onStreamStarted() {
            pendingAssistantText = ""
            messageModel.append({ role: "assistant", text: "" })
        }
        function onStreamChunk(text) {
            pendingAssistantText += text
            const lastIndex = messageModel.count - 1
            if (lastIndex >= 0) {
                messageModel.setProperty(lastIndex, "text", pendingAssistantText)
                chatList.positionViewAtEnd()
            }
        }
        function onStreamFinished() {
            pendingAssistantText = ""
        }
        function onStreamError(message) {
            messageModel.append({ role: "assistant", text: "Error: " + message })
        }
        function onSessionsChanged() {
            if (controller.sessions.length > 0 && sessionBox.currentIndex < 0) {
                sessionBox.currentIndex = 0
            }
        }
        function onModelsChanged() {
            for (let i = 0; i < controller.availableModelsDetailed.length; i += 1) {
                if (controller.availableModelsDetailed[i].name === controller.activeModel) {
                    providerModelBox.currentIndex = i
                    break
                }
            }
        }
    }
}
