import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import MultiLink 1.0

Page {
    id: chatPage
    required property ChatController controller
    property string pendingAssistantText: ""
    property bool pendingSelectNewestSession: false
    property string pendingDeleteSessionId: ""

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
        if (!controller) return colorLocal
        return controller.providerScope === "LOCAL" ? colorLocal : colorRemote
    }

    function sessionIdAt(index) {
        if (!controller) return ""
        if (index < 0 || index >= controller.sessions.length) {
            return ""
        }
        const row = controller.sessions[index]
        return row.sessionId || row.id || ""
    }

    function currentViewSessionId() {
        if (!controller || !sessionBox) return ""
        return sessionIdAt(sessionBox.currentIndex)
    }

    function indexForSessionId(sessionId) {
        if (!controller || !sessionId || sessionId.length === 0) {
            return -1
        }
        for (let i = 0; i < controller.sessions.length; i += 1) {
            if (sessionIdAt(i) === sessionId) {
                return i
            }
        }
        return -1
    }

    function isStreamingActiveScope() {
        if (!controller) return false
        return controller.isLoading && controller.streamingSessionId === currentViewSessionId()
    }

    function urlToLocalPath(urlValue) {
        const raw = String(urlValue || "")
        if (raw.startsWith("file://")) {
            return decodeURIComponent(raw.replace("file://", ""))
        }
        return raw
    }

    function parseMessageSegments(text) {
        const source = String(text || "")
        const pattern = /```[\t ]*([^\n`]*)\n([\s\S]*?)```/g
        const segments = []
        let last = 0
        let match

        while ((match = pattern.exec(source)) !== null) {
            if (match.index > last) {
                segments.push({ kind: "text", value: source.slice(last, match.index) })
            }
            segments.push({
                kind: "code",
                value: String(match[2] || ""),
                language: String(match[1] || "").trim()
            })
            last = pattern.lastIndex
        }

        if (last < source.length) {
            segments.push({ kind: "text", value: source.slice(last) })
        }

        if (segments.length === 0) {
            segments.push({ kind: "text", value: source })
        }

        return segments
    }

    function scrollToBottom() {
        chatList.positionViewAtEnd()
    }

    function hydrateCurrentSession() {
        if (!controller) return
        if (controller.sessions.length === 0) {
            controller.requestSessions()
            return
        }

        const targetId = controller.selectedSessionId
        let targetIndex = -1
        
        if (targetId && targetId.length > 0) {
            for (let i = 0; i < controller.sessions.length; i += 1) {
                if (sessionIdAt(i) === targetId) {
                    targetIndex = i
                    break
                }
            }
        }

        if (targetIndex < 0) {
            targetIndex = 0
            sessionBox.currentIndex = 0
            controller.selectSessionAtIndex(0)
            return
        }

        if (sessionBox.currentIndex !== targetIndex) {
            sessionBox.currentIndex = targetIndex
        }
    }

    function selectSessionIndex(index) {
        if (index < 0 || index >= controller.sessions.length) {
            return
        }
        const sessionId = sessionIdAt(index)
        if (sessionId.length === 0) {
            return
        }
        if (controller.selectedSessionId === sessionId) {
            return
        }
        pendingAssistantText = ""
        messageModel.clear()
        controller.selectSessionAtIndex(index)
    }

    Component.onCompleted: {
        hydrateCurrentSession()
    }

    onVisibleChanged: {
        if (visible) {
            hydrateCurrentSession()
        }
    }

    FolderDialog {
        id: projectFolderDialog
        title: "Seleccionar carpeta del proyecto"
        currentFolder: "file:///"
        onAccepted: {
            controller.setSelectedSessionProjectRoot(urlToLocalPath(selectedFolder))
        }
    }

    MessageDialog {
        id: deleteSessionDialog
        title: "Eliminar sesion"
        text: "Esta accion no se puede deshacer.\n\nDeseas eliminar esta sesion?"
        buttons: MessageDialog.Yes | MessageDialog.No
        onAccepted: {
            if (controller && pendingDeleteSessionId.length > 0) {
                controller.deleteSession(pendingDeleteSessionId)
            }
            pendingDeleteSessionId = ""
        }
        onRejected: {
            pendingDeleteSessionId = ""
        }
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
                text: controller ? controller.providerScope : ""
                color: currentAccent()
            }
            Rectangle {
                width: 8
                height: 8
                radius: 4
                color: (controller && controller.providerHealth === "available")
                       ? colorLocal
                       : ((controller && controller.providerHealth === "starting") ? colorWarning : colorError)
            }
            Label {
                text: (controller && controller.providerHealth === "available")
                      ? ("Proveedor activo")
                      : ((controller && controller.providerHealth === "starting") ? "Iniciando" : "No disponible")
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
                model: controller ? controller.sessions : []
                Layout.preferredWidth: 280
                onActivated: function(index) {
                    selectSessionIndex(index)
                }
            }
            Button {
                text: "Nueva sesion"
                onClicked: {
                    pendingSelectNewestSession = true
                    controller.newSession()
                }
            }
            Button {
                text: "Eliminar sesion"
                enabled: currentViewSessionId().length > 0
                onClicked: {
                    pendingDeleteSessionId = currentViewSessionId()
                    if (pendingDeleteSessionId.length > 0) {
                        deleteSessionDialog.open()
                    }
                }
            }
            Button {
                text: "Limpiar vacías"
                onClicked: controller.deleteEmptySessions()
            }
            Button {
                text: "Proyecto"
                enabled: currentViewSessionId().length > 0
                onClicked: projectFolderDialog.open()
            }
            Label {
                Layout.preferredWidth: 320
                elide: Label.ElideMiddle
                color: colorTextSecondary
                text: (controller && controller.selectedProjectRoot.length > 0)
                      ? controller.selectedProjectRoot
                      : "Sin carpeta de proyecto"
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
                boundsBehavior: Flickable.DragOverBounds
                ScrollBar.vertical: ScrollBar { 
                    policy: ScrollBar.AlwaysOn
                    width: 12
                }
                model: ListModel { id: messageModel }
                delegate: Item {
                    width: ListView.view.width
                    height: bubble.implicitHeight + 6
                    property var segments: parseMessageSegments(model.text)

                    Rectangle {
                        id: bubble
                        width: parent.width * 0.86
                        implicitHeight: bubbleContent.implicitHeight + 14
                        anchors.right: model.role === "user" ? parent.right : undefined
                        anchors.left: model.role === "assistant" ? parent.left : undefined
                        color: model.role === "user" ? "#E8F5E9" : "#FFFFFF"
                        border.color: colorBorder
                        radius: 8

                        Column {
                            id: bubbleContent
                            anchors.fill: parent
                            anchors.margins: 7
                            spacing: 6

                            Column {
                                id: segmentColumn
                                width: parent.width
                                spacing: 6

                                Repeater {
                                    model: segments
                                    delegate: Item {
                                        required property var modelData
                                        property var segment: modelData ? modelData : ({ kind: "text", value: "", language: "" })
                                        width: segmentColumn.width
                                        implicitHeight: segment.kind === "code" ? codeBlock.implicitHeight : textBlock.implicitHeight

                                        TextArea {
                                            id: textBlock
                                            visible: segment.kind !== "code"
                                            width: parent.width
                                            color: colorTextPrimary
                                            text: segment.value
                                            wrapMode: TextArea.Wrap
                                            readOnly: true
                                            selectByMouse: true
                                            selectionColor: "#90CAF9"
                                            selectedTextColor: colorTextPrimary
                                            padding: 0
                                            background: null
                                        }

                                        Rectangle {
                                            id: codeBlock
                                            visible: segment.kind === "code"
                                            width: parent.width
                                            color: "#1F2933"
                                            radius: 6
                                            border.color: "#2F3E4D"
                                            implicitHeight: codeHeader.implicitHeight + codeFlick.implicitHeight + 10

                                            Column {
                                                anchors.fill: parent
                                                anchors.margins: 6
                                                spacing: 4

                                                RowLayout {
                                                    id: codeHeader
                                                    width: parent.width
                                                    Label {
                                                        text: segment.language && segment.language.length > 0 ? segment.language : "code"
                                                        color: "#9FB3C8"
                                                        font.pixelSize: 11
                                                        Layout.fillWidth: true
                                                    }
                                                    Rectangle {
                                                        width: copyLabel.implicitWidth + 16
                                                        height: copyLabel.implicitHeight + 6
                                                        radius: 4
                                                        color: copyMa.containsMouse ? "#3D4F5F" : "#2F3E4D"
                                                        Label {
                                                            id: copyLabel
                                                            anchors.centerIn: parent
                                                            text: "⎘ Copiar Código"
                                                            color: "#9FB3C8"
                                                            font.pixelSize: 11
                                                        }
                                                        MouseArea {
                                                            id: copyMa
                                                            anchors.fill: parent
                                                            hoverEnabled: true
                                                            cursorShape: Qt.PointingHandCursor
                                                            onClicked: {
                                                                if (controller) {
                                                                    controller.copyText(segment.value)
                                                                }
                                                                copyLabel.text = "✓ Copiado"
                                                                copyTimer.start()
                                                            }
                                                        }
                                                        Timer {
                                                            id: copyTimer
                                                            interval: 1500
                                                            onTriggered: copyLabel.text = "⎘ Copiar Código"
                                                        }
                                                    }
                                                }

                                                Flickable {
                                                    id: codeFlick
                                                    width: parent.width
                                                    implicitHeight: Math.min(260, codeText.implicitHeight + 4)
                                                    contentWidth: Math.max(width, codeText.contentWidth + 8)
                                                    contentHeight: codeText.implicitHeight + 4
                                                    clip: true
                                                    boundsBehavior: Flickable.StopAtBounds
                                                    ScrollBar.horizontal: ScrollBar { }

                                                    TextArea {
                                                        id: codeText
                                                        x: 4
                                                        width: Math.max(codeFlick.width, contentWidth + 8)
                                                        text: segment.value
                                                        color: "#D8DEE9"
                                                        font.family: "Monospace"
                                                        font.pixelSize: 13
                                                        wrapMode: TextArea.NoWrap
                                                        readOnly: true
                                                        selectByMouse: true
                                                        padding: 0
                                                        background: null
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            Row {
                                width: parent.width
                                visible: model.role === "assistant"
                                layoutDirection: Qt.RightToLeft

                                Rectangle {
                                    width: 26
                                    height: 22
                                    radius: 4
                                    color: modelCopyMouse.containsMouse ? "#E0E0E0" : "#F5F5F5"
                                    border.color: "#D0D0D0"
                                    border.width: 1

                                    Label {
                                        id: modelCopyLabel
                                        anchors.centerIn: parent
                                        text: "⎘"
                                        color: colorTextSecondary
                                        font.pixelSize: 13
                                    }

                                    MouseArea {
                                        id: modelCopyMouse
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: {
                                            if (controller) {
                                                controller.copyText(model.text)
                                            }
                                            modelCopyLabel.text = "✓"
                                            modelCopyTimer.start()
                                        }
                                    }

                                    Timer {
                                        id: modelCopyTimer
                                        interval: 1200
                                        onTriggered: modelCopyLabel.text = "⎘"
                                    }
                                }
                            }

                        }
                    }
                }
            }

            Button {
                id: scrollBottomButton
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.rightMargin: 18
                anchors.bottomMargin: 18
                text: "↓ Ir abajo"
                visible: messageModel.count > 0 && !chatList.atYEnd
                onClicked: scrollToBottom()
            }
        }

        RowLayout {
            Layout.fillWidth: true
            ComboBox {
                id: providerModelBox
                model: controller ? controller.availableModelsDetailed : []
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
                enabled: !isStreamingActiveScope()
                onAccepted: sendButton.clicked()
            }

            Button {
                id: sendButton
                text: "Enviar"
                enabled: !isStreamingActiveScope() && !pendingSelectNewestSession
                onClicked: {
                    if (pendingSelectNewestSession) {
                        return
                    }
                    const prompt = promptInput.text.trim()
                    if (prompt.length === 0) {
                        return
                    }
                    const viewSessionId = currentViewSessionId()
                    if (viewSessionId.length === 0) {
                        return
                    }
                    if (controller.selectedSessionId !== viewSessionId) {
                        controller.selectSession(viewSessionId)
                    }
                    messageModel.append({ role: "user", text: prompt })
                    scrollToBottom()
                    promptInput.text = ""
                    pendingAssistantText = ""
                    controller.sendPromptForSession(viewSessionId, prompt)
                }
            }

            Button {
                text: "Detener"
                visible: isStreamingActiveScope()
                enabled: isStreamingActiveScope()
                onClicked: controller.stopGeneration()
            }

            BusyIndicator {
                running: isStreamingActiveScope()
                visible: isStreamingActiveScope()
            }
        }
    }

    Connections {
        target: controller
        function onStreamStarted(sessionId) {
            if (sessionId !== currentViewSessionId()) {
                return
            }
            pendingAssistantText = ""
            messageModel.append({ role: "assistant", text: "" })
            scrollToBottom()
        }
        function onStreamChunk(sessionId, text) {
            if (sessionId !== currentViewSessionId()) {
                return
            }
            pendingAssistantText += text
            const lastIndex = messageModel.count - 1
            if (lastIndex >= 0) {
                messageModel.setProperty(lastIndex, "text", pendingAssistantText)
                chatList.positionViewAtEnd()
            }
        }
        function onStreamFinished(sessionId) {
            if (sessionId !== currentViewSessionId()) {
                return
            }
            pendingAssistantText = ""
        }
        function onStreamError(sessionId, message) {
            if (sessionId !== currentViewSessionId()) {
                return
            }
            messageModel.append({ role: "assistant", text: "Error: " + message })
            scrollToBottom()
        }
        function onMessagesHydrated(messages) {
            messageModel.clear()
            for (let i = 0; i < messages.length; i += 1) {
                messageModel.append({
                    role: messages[i].role,
                    text: messages[i].text
                })
            }
            scrollToBottom()
        }
        function onSessionsChanged() {
            if (!controller || controller.sessions.length === 0) {
                return
            }

            if (pendingSelectNewestSession) {
                pendingSelectNewestSession = false
                const selectedId = controller.selectedSessionId
                const selectedIndex = indexForSessionId(selectedId)
                if (selectedIndex >= 0) {
                    sessionBox.currentIndex = selectedIndex
                    controller.selectSession(selectedId)
                    return
                }
            }

            hydrateCurrentSession()
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
