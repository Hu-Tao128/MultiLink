import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import MultiLink 1.0

Page {
    id: chatPage
    required property ChatController controller
    
    // Colores del tema (inyectados desde Main.qml)
    property color colorBackground: "#F5F6F7"
    property color colorSurface: "#FFFFFF"
    property color colorBorder: "#E0E0E0"
    property color colorTextPrimary: "#1E1E1E"
    property color colorTextSecondary: "#6B6B6B"
    property color colorLocal: "#2E7D32"
    property color colorRemote: "#1565C0"
    property color colorWarning: "#ED6C02"
    property color colorError: "#C62828"
    property color colorBubbleUser: "#E8F5E9"
    property color colorBubbleAssistant: "#FFFFFF"
    property color colorBubbleThinking: "#ECEFF1"
    property color colorBubbleThinkingText: "#455A64"
    property color colorCodeBackground: "#1E2A32"
    property color colorCodeBorder: "#3D4F5F"
    property color colorCodeText: "#D8DEE9"
    property color colorCodeLanguage: "#61AFEF"
    property color colorInputBackground: "#FAFAFA"
    property color colorHoverOverlay: "#E0E0E0"

    property string pendingAssistantText: ""
    property bool pendingSelectNewestSession: false
    property string pendingDeleteSessionId: ""
    property bool tokenSidebarExpanded: true
    property bool modelSelectorInitializing: true
    property bool modelSelectorUpdating: false
    property string modelFilterText: ""
    property int currentPromptTokens: 0
    property int currentCompletionTokens: 0
    property int currentTotalTokens: 0
    property bool currentUsageIsEstimated: false
    property int contextMaxTokens: 4096
    property bool suppressStartupWarning: false

    // Composer: attachments + @file completion + live telemetry
    property var pendingImages: []
    property var pendingContextFiles: []
    property var pendingWriteOps: []
    property var projectFiles: []
    property var mentionResults: []
    property bool mentionOpen: false
    property int mentionIndex: 0
    property string mentionQuery: ""
    property var liveExecutorStatus: ({ tool: "", file: "", validation: "" })
    property int contextEstimatedTokens: 0
    property int contextNumCtx: 0

    onMentionQueryChanged: refreshMentionResults()
    onProjectFilesChanged: refreshMentionResults()

    function formatTokenCount(value) {
        const n = Math.max(0, Math.round(Number(value) || 0))
        return String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ",")
    }

    function imageSource(pathValue) {
        const raw = String(pathValue || "")
        if (raw.length === 0) {
            return ""
        }
        if (raw.startsWith("file:")) {
            return raw
        }
        const normalized = raw.replace(/\\/g, "/")
        return normalized.startsWith("/") ? "file://" + normalized : "file:///" + normalized
    }

    function addPendingImage(pathValue) {
        const path = String(pathValue || "")
        if (path.length === 0) {
            return
        }
        const extension = path.split(".").pop().toLowerCase()
        if (["png", "jpg", "jpeg", "webp"].indexOf(extension) < 0) {
            return
        }
        if (pendingImages.some(entry => entry.path === path)) {
            return
        }
        const next = pendingImages.slice()
        const parts = path.split(/[\\/]/)
        next.push({
            path: path,
            sourceUrl: imageSource(path),
            name: parts.length > 0 ? parts[parts.length - 1] : path
        })
        pendingImages = next
    }

    function removePendingImage(targetIndex) {
        if (targetIndex < 0 || targetIndex >= pendingImages.length) {
            return
        }
        const next = pendingImages.slice()
        next.splice(targetIndex, 1)
        pendingImages = next
    }

    function refreshProjectFiles() {
        if (!controller) {
            projectFiles = []
            return
        }
        const sessionId = currentViewSessionId()
        const files = sessionId.length > 0
            ? controller.listProjectFiles(sessionId)
            : controller.listProjectFilesForActiveSession()
        projectFiles = files ? files : []
    }

    function refreshMentionResults() {
        const query = String(mentionQuery || "").toLowerCase()
        const source = projectFiles || []
        const results = []
        for (let i = 0; i < source.length && results.length < 8; i += 1) {
            const file = String(source[i] || "")
            if (query.length === 0 || file.toLowerCase().indexOf(query) >= 0) {
                results.push(file)
            }
        }
        mentionResults = results
        if (mentionIndex >= results.length) {
            mentionIndex = Math.max(0, results.length - 1)
        }
    }

    function updateMentionFromInput() {
        if (!promptInput) {
            return
        }
        const text = promptInput.text
        const cursor = promptInput.cursorPosition
        const before = text.substring(0, cursor)
        const at = before.lastIndexOf("@")
        if (at < 0) {
            mentionOpen = false
            mentionQuery = ""
            return
        }
        const token = before.substring(at + 1)
        if (token.indexOf(" ") >= 0 || token.indexOf("\n") >= 0 || token.indexOf("\t") >= 0) {
            mentionOpen = false
            mentionQuery = ""
            return
        }
        const opening = !mentionOpen
        if (opening) {
            refreshProjectFiles()
            mentionIndex = 0
        }
        mentionQuery = token
        mentionOpen = true
        refreshMentionResults()
    }

    function acceptMention(targetIndex) {
        if (targetIndex < 0 || targetIndex >= mentionResults.length) {
            return
        }
        const path = String(mentionResults[targetIndex])
        const text = promptInput.text
        const cursor = promptInput.cursorPosition
        const before = text.substring(0, cursor)
        const at = before.lastIndexOf("@")
        if (at < 0) {
            mentionOpen = false
            return
        }
        const after = text.substring(cursor)
        const insertion = "@" + path + " "
        promptInput.text = text.substring(0, at) + insertion + after
        promptInput.cursorPosition = at + insertion.length
        mentionOpen = false
        mentionQuery = ""
    }

    function recordExecutorStatus(tool, file, validation) {
        const toolName = String(tool || "")
        const fileName = String(file || "")
        const validationText = String(validation || "")
        liveExecutorStatus = { tool: toolName, file: fileName, validation: validationText }

        const isWrite = toolName === "write_file" || toolName === "apply_patch"
        if (!isWrite && toolName !== "auto_validate" && toolName !== "run_command") {
            return
        }

        let ops = (pendingWriteOps || []).slice()
        if (isWrite) {
            const existing = ops.findIndex(op => op.tool === toolName && op.file === fileName)
            const previousValidation = existing >= 0 ? String(ops[existing].validation || "") : ""
            const entry = {
                tool: toolName,
                file: fileName,
                toolStatus: validationText,
                validation: previousValidation
            }
            if (existing >= 0) {
                ops[existing] = entry
            } else {
                ops.push(entry)
            }
        } else if (toolName === "auto_validate") {
            for (let i = ops.length - 1; i >= 0; i -= 1) {
                if (ops[i].tool === "write_file" || ops[i].tool === "apply_patch") {
                    ops[i] = {
                        tool: ops[i].tool,
                        file: ops[i].file,
                        toolStatus: ops[i].toolStatus || "",
                        validation: validationText.length > 0 ? validationText : (ops[i].validation || "")
                    }
                    break
                }
            }
        } else if (toolName === "run_command") {
            for (let i = ops.length - 1; i >= 0; i -= 1) {
                const isWriteOp = ops[i].tool === "write_file" || ops[i].tool === "apply_patch"
                if (isWriteOp && String(ops[i].validation || "").startsWith("running:")) {
                    ops[i] = {
                        tool: ops[i].tool,
                        file: ops[i].file,
                        toolStatus: ops[i].toolStatus || "",
                        validation: validationText
                    }
                    break
                }
            }
        }
        pendingWriteOps = ops
    }

    function validationColor(value) {
        const text = String(value || "").toLowerCase()
        if (text.length === 0) {
            return colorTextSecondary
        }
        if (text.indexOf("running:") === 0) {
            return colorWarning
        }
        if (text === "success" || text.indexOf("ok") === 0) {
            return colorLocal
        }
        return colorError
    }

    function validationLabel(value) {
        const text = String(value || "")
        if (text.length === 0) {
            return "Sin validación automática"
        }
        if (text.indexOf("running:") === 0) {
            return "Validando " + text.substring("running:".length).trim()
        }
        if (text === "success") {
            return "Validación OK"
        }
        return "Validación: " + text
    }

    function resetComposerTransients() {
        pendingImages = []
        pendingContextFiles = []
        pendingWriteOps = []
        liveExecutorStatus = { tool: "", file: "", validation: "" }
        mentionOpen = false
        mentionQuery = ""
        mentionResults = []
    }

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
        const source = normalizeStreamingMarkup(String(text || ""))
        const segments = []

        const thinkPattern = /<think>([\s\S]*?)<\/think>/g
        let thinkLastIndex = 0
        let thinkMatch

        while ((thinkMatch = thinkPattern.exec(source)) !== null) {
            if (thinkMatch.index > thinkLastIndex) {
                const textBeforeThink = source.slice(thinkLastIndex, thinkMatch.index)
                segments.push(...parseCodeAndInlineSegments(textBeforeThink))
            }
            segments.push({
                kind: "thinking",
                value: String(thinkMatch[1] || "").trim()
            })
            thinkLastIndex = thinkPattern.lastIndex
        }

        if (thinkLastIndex < source.length) {
            segments.push(...parseCodeAndInlineSegments(source.slice(thinkLastIndex)))
        }

        if (segments.length === 0) {
            segments.push({ kind: "text", value: source })
        }

        return segments
    }

    function parseCodeAndInlineSegments(source) {
        const segments = []

        const codePattern = /```[\t ]*([^\n`]*)\n([\s\S]*?)```/g
        let lastIndex = 0
        let match

        while ((match = codePattern.exec(source)) !== null) {
            if (match.index > lastIndex) {
                const textBefore = source.slice(lastIndex, match.index)
                segments.push(...parseInlineMarkdown(textBefore))
            }
            segments.push({
                kind: "code",
                value: String(match[2] || ""),
                language: String(match[1] || "").trim()
            })
            lastIndex = codePattern.lastIndex
        }

        if (lastIndex < source.length) {
            segments.push(...parseInlineMarkdown(source.slice(lastIndex)))
        }

        return segments
    }

    function normalizeStreamingMarkup(source) {
        let normalized = String(source || "")

        const thinkOpen = (normalized.match(/<think>/g) || []).length
        const thinkClose = (normalized.match(/<\/think>/g) || []).length
        if (thinkOpen > thinkClose) {
            normalized += "</think>"
        }

        const codeFenceCount = (normalized.match(/```/g) || []).length
        if (codeFenceCount % 2 === 1) {
            normalized += "\n```"
        }

        const doubleStarCount = (normalized.match(/\*\*/g) || []).length
        if (doubleStarCount % 2 === 1) {
            normalized += "**"
        }

        let withoutTripleStars = normalized.replace(/\*\*\*/g, "")
        withoutTripleStars = withoutTripleStars.replace(/\*\*/g, "")
        const singleStarCount = (withoutTripleStars.match(/\*/g) || []).length
        if (singleStarCount % 2 === 1) {
            normalized += "*"
        }

        const doubleUnderscoreCount = (normalized.match(/__/g) || []).length
        if (doubleUnderscoreCount % 2 === 1) {
            normalized += "__"
        }

        const withoutDoubleUnderscore = normalized.replace(/__/g, "")
        const singleUnderscoreCount = (withoutDoubleUnderscore.match(/_/g) || []).length
        if (singleUnderscoreCount % 2 === 1) {
            normalized += "_"
        }

        const withoutCodeFence = normalized.replace(/```[\s\S]*?```/g, "")
        const inlineCodeCount = (withoutCodeFence.match(/`/g) || []).length
        if (inlineCodeCount % 2 === 1) {
            normalized += "`"
        }

        return normalized
    }

    function parseInlineMarkdown(text) {
        const lines = String(text || "").split('\n')
        const parsed = []

        for (let i = 0; i < lines.length; i++) {
            const line = lines[i]
            const headingMatch = line.match(/^(#{1,6})\s+(.+)$/)
            if (headingMatch) {
                parsed.push({
                    kind: "heading",
                    level: headingMatch[1].length,
                    value: headingMatch[2]
                })
                continue
            }

            const bulletMatch = line.match(/^(\s*)([-*+]|\d+\.)\s+(.+)$/)
            if (bulletMatch) {
                const indent = Math.floor((bulletMatch[1] || "").length / 2)
                const marker = bulletMatch[2]
                const content = bulletMatch[3]
                parsed.push({
                    kind: "listitem",
                    indent: indent,
                    bullet: marker,
                    value: content
                })
                continue
            }

            const withLineEnding = line + (i < lines.length - 1 ? '\n' : '')
            parsed.push(...parseInlineStyleSegments(withLineEnding))
        }

        if (parsed.length === 0) {
            parsed.push({ kind: "text", value: text })
        }

        return parsed
    }

    function parseInlineStyleSegments(text) {
        return [{ kind: "richtext", value: inlineMarkdownToHtml(text) }]
    }

    function escapeHtml(text) {
        return String(text || "")
            .replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;")
            .replace(/\"/g, "&quot;")
            .replace(/'/g, "&#39;")
    }

    function inlineMarkdownToHtml(text) {
        let html = escapeHtml(text)
        html = html.replace(/`([^`]+)`/g, "<code>" + "$1" + "</code>")
        html = html.replace(/\*\*\*([^*]+)\*\*\*/g, "<b><i>" + "$1" + "</i></b>")
        html = html.replace(/\*\*([^*]+)\*\*/g, "<b>" + "$1" + "</b>")
        html = html.replace(/__([^_]+)__/g, "<b>" + "$1" + "</b>")
        html = html.replace(/\*([^*]+)\*/g, "<i>" + "$1" + "</i>")
        html = html.replace(/_([^_]+)_/g, "<i>" + "$1" + "</i>")
        html = html.replace(/\n/g, "<br>")
        return html
    }

    function scrollToBottom() {
        chatList.positionViewAtEnd()
    }

    function rebuildModelChoices() {
        if (!controller || !providerModelBox || !filteredModelsModel) {
            return
        }
        const rows = controller.availableModelsDetailed || []
        const needle = String(modelFilterText || "").trim().toLowerCase()
        modelSelectorUpdating = true
        filteredModelsModel.clear()
        let activeIndex = -1
        for (let i = 0; i < rows.length; i += 1) {
            const row = rows[i]
            if (!row) {
                continue
            }
            const name = String(row.name || "")
            const label = String(row.label || name)
            const provider = String(row.provider || "")
            if (needle.length > 0) {
                const haystack = (provider + " " + name + " " + label).toLowerCase()
                if (haystack.indexOf(needle) < 0) {
                    continue
                }
            }
            filteredModelsModel.append({
                name: name,
                label: label,
                provider: provider,
                serverUrl: String(row.serverUrl || "")
            })
            if (name === controller.activeModel) {
                activeIndex = filteredModelsModel.count - 1
            }
        }
        providerModelBox.currentIndex = activeIndex
        modelSelectorUpdating = false
    }

    function currentAssistantDraftIndex() {
        const lastIndex = messageModel.count - 1
        if (lastIndex < 0) {
            return -1
        }
        const lastMessage = messageModel.get(lastIndex)
        if (!lastMessage || lastMessage.role !== "assistant") {
            return -1
        }
        return lastIndex
    }

    function hydrateCurrentSession() {
        if (!controller) return
        if (controller.sessions.length === 0) {
            controller.requestSessions()
            return
        }
        modelSelectorInitializing = false

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
        resetComposerTransients()
        contextEstimatedTokens = 0
        contextNumCtx = 0
        messageModel.clear()
        controller.selectSessionAtIndex(index)
    }

    Component.onCompleted: {
        hydrateCurrentSession()
        rebuildModelChoices()
        if (controller && controller.startupNotice.length > 0) {
            const notice = controller.startupNotice.toLowerCase()
            const isOllamaMissingNotice = notice.indexOf("ollama") >= 0
                && (notice.indexOf("no tienes ollama") >= 0
                    || notice.indexOf("no se detecto respuesta de ollama") >= 0)
            if (isOllamaMissingNotice) {
                startupWarningDialog.open()
            } else {
                configErrorDialog.open()
            }
        }
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

    FileDialog {
        id: imagePickerDialog
        title: "Adjuntar imágenes"
        fileMode: FileDialog.OpenFiles
        nameFilters: ["Imágenes (*.png *.jpg *.jpeg *.webp)"]
        onAccepted: {
            const files = selectedFiles || []
            for (let i = 0; i < files.length; i += 1) {
                chatPage.addPendingImage(chatPage.urlToLocalPath(files[i]))
            }
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

    MessageDialog {
        id: configErrorDialog
        title: "Error de configuracion"
        text: controller ? controller.startupNotice : ""
        buttons: MessageDialog.Ok
        onAccepted: {
            if (controller) controller.clearStartupNotice()
        }
    }

    Dialog {
        id: startupWarningDialog
        title: "Configura tu proveedor"
        modal: true
        standardButtons: Dialog.Ok
        closePolicy: Popup.NoAutoClose
        width: Math.min(chatPage.width - 40, 620)
        onAccepted: {
            if (controller && suppressStartupWarning) {
                controller.setMissingOllamaNoticeSuppressed(true)
            }
            if (controller) {
                controller.clearStartupNotice()
            }
        }

        contentItem: ColumnLayout {
            spacing: 10
            Label {
                Layout.fillWidth: true
                text: controller ? controller.startupNotice : ""
                wrapMode: Text.Wrap
            }
            CheckBox {
                text: "No mostrar de nuevo"
                checked: suppressStartupWarning
                onToggled: suppressStartupWarning = checked
            }
        }
    }

    Connections {
        target: controller
        function onStartupNoticeChanged() {
            if (controller && controller.startupNotice.length > 0) {
                const notice = controller.startupNotice.toLowerCase()
                const isOllamaMissingNotice = notice.indexOf("ollama") >= 0
                    && (notice.indexOf("no tienes ollama") >= 0
                        || notice.indexOf("no se detecto respuesta de ollama") >= 0)
                if (isOllamaMissingNotice) {
                    startupWarningDialog.open()
                } else {
                    configErrorDialog.open()
                }
            }
        }
    }

    ListModel { id: filteredModelsModel }

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
            Label {
                text: "🗂 Sesión"
                color: colorTextSecondary
                font.pixelSize: 12
                font.bold: true
                ToolTip.visible: sessionHover.hovered
                ToolTip.text: "Sesiones del proyecto (distinto del modelo activo)"
                HoverHandler {
                    id: sessionHover
                }
            }
            ComboBox {
                id: sessionBox
                textRole: "title"
                model: controller ? controller.sessions : []
                Layout.fillWidth: true
                Layout.maximumWidth: 340
                Layout.minimumWidth: 120
                onActivated: function(index) {
                    selectSessionIndex(index)
                }
            }
            Button {
                text: "Nueva"
                onClicked: {
                    pendingSelectNewestSession = true
                    controller.newSession()
                }
            }
            Button {
                text: "Eliminar"
                enabled: currentViewSessionId().length > 0
                onClicked: {
                    pendingDeleteSessionId = currentViewSessionId()
                    if (pendingDeleteSessionId.length > 0) {
                        deleteSessionDialog.open()
                    }
                }
            }
            Button {
                text: "Limpiar"
                onClicked: controller.deleteEmptySessions()
            }
            Button {
                text: "Proyecto"
                enabled: currentViewSessionId().length > 0
                onClicked: projectFolderDialog.open()
            }
            Label {
                Layout.fillWidth: true
                Layout.maximumWidth: 320
                elide: Label.ElideMiddle
                color: colorTextSecondary
                text: (controller && controller.selectedProjectRoot.length > 0)
                      ? controller.selectedProjectRoot
                      : "Sin carpeta de proyecto"
            }
        }

        Rectangle {
            id: chatArea
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

                    property bool isStreaming: model.isStreaming === true
                    property var segments: isStreaming ? [{ kind: "text", value: model.text }] : parseMessageSegments(model.text)

                    Rectangle {
                        id: bubble
                        width: Math.min(chatArea.width * 0.86, chatArea.width - 20)
                        implicitHeight: bubbleContent.implicitHeight + 14
                        anchors.right: model.role === "user" ? parent.right : undefined
                        anchors.left: model.role === "assistant" ? parent.left : undefined
                        color: model.role === "user" ? Theme.bubbleUser : Theme.bubbleAssistant
                        border.color: Theme.border
                        radius: 8

                        Column {
                            id: bubbleContent
                            anchors.fill: parent
                            anchors.margins: 7
                            spacing: 6

                            Rectangle {
                                width: parent.width
                                height: visible ? contextColumn.implicitHeight + 10 : 0
                                visible: model.role === "assistant" && (model.contextFiles || []).length > 0
                                radius: 5
                                color: Theme.surfaceAlt
                                border.color: Theme.border

                                Column {
                                    id: contextColumn
                                    anchors.fill: parent
                                    anchors.margins: 5
                                    spacing: 2

                                    RowLayout {
                                        width: parent.width
                                        Label {
                                            Layout.fillWidth: true
                                            text: "📚 Contexto usado (" + ((model.contextFiles || []).length) + ")"
                                            color: Theme.textSecondary
                                            font.pixelSize: 11
                                            font.bold: true
                                        }
                                        Label {
                                            text: model.contextExpanded === true ? "▾" : "▸"
                                            color: Theme.textSecondary
                                            font.pixelSize: 11
                                        }
                                        MouseArea {
                                            anchors.fill: parent
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: messageModel.setProperty(index, "contextExpanded", !(model.contextExpanded === true))
                                        }
                                    }

                                    Repeater {
                                        model: model.contextExpanded === true ? (model.contextFiles || []) : []
                                        delegate: Label {
                                            required property var modelData
                                            width: contextColumn.width
                                            text: "• " + modelData
                                            color: Theme.textSecondary
                                            font.pixelSize: 11
                                            elide: Text.ElideMiddle
                                        }
                                    }
                                }
                            }

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
                                            textFormat: isRichText ? TextArea.RichText : TextArea.AutoText
                                            property string baseColor: colorTextPrimary
                                            property bool isBold: segment.kind === "bold" || segment.kind === "bolditalic"
                                            property bool isItalic: segment.kind === "italic" || segment.kind === "bolditalic"
                                            property bool isInlineCode: segment.kind === "inlinecode"
                                            property bool isInlineRichSegment: segment.kind === "richtext"
                                            property bool isHeading: segment.kind === "heading"
                                            property bool isListItem: segment.kind === "listitem"
                                            property bool isThinking: segment.kind === "thinking"
                                            property bool isRichText: isInlineRichSegment || isListItem || isHeading || isThinking
                                            property int headingLevel: Number(segment.level || 2)
                                            property int listIndent: Number(segment.indent || 0)
                                            property string listBullet: String(segment.bullet || "-")
                                            color: isThinking ? Theme.bubbleThinkingText : (isInlineCode ? "#E53935" : (model.role === "user" ? Theme.textPrimary : baseColor))
                                            text: isListItem
                                                  ? inlineMarkdownToHtml(Array(listIndent + 1).join("  ") + listBullet + " " + String(segment.value || ""))
                                                  : (isThinking
                                                     ? inlineMarkdownToHtml("Pensamiento del modelo:\n" + String(segment.value || ""))
                                                  : (isInlineRichSegment
                                                     ? String(segment.value || "")
                                                     : (isHeading
                                                         ? inlineMarkdownToHtml(String(segment.value || ""))
                                                         : String(segment.value || ""))))
                                            wrapMode: TextArea.Wrap
                                            readOnly: true
                                            selectByMouse: true
                                            selectionColor: "#90CAF9"
                                            selectedTextColor: colorTextPrimary
                                            padding: isInlineCode ? 4 : (isThinking ? 8 : 0)
                                            leftPadding: isThinking ? 12 : padding
                                            font.family: isInlineCode ? "Monospace" : "sans-serif"
                                            font.pixelSize: isInlineCode
                                                            ? 12
                                                            : (isHeading
                                                               ? Math.max(16, 24 - (headingLevel * 2))
                                                               : 14)
                                            font.bold: isBold || isHeading
                                            font.italic: isItalic || isThinking
                                            background: Rectangle {
                                                visible: textBlock.isInlineCode || textBlock.isThinking
                                                color: textBlock.isThinking ? Theme.bubbleThinking : "#F5F5F5"
                                                radius: 3
                                                border.color: textBlock.isThinking ? Theme.border : "#E0E0E0"
                                            }
                                        }

                                            Rectangle {
                                                id: codeBlock
                                                visible: segment.kind === "code"
                                                width: parent.width
                                                gradient: Gradient {
                                                    GradientStop { position: 0; color: Theme.codeBackground }
                                                    GradientStop { position: 1; color: Theme.codeBackground }
                                                }
                                                radius: 8
                                                border.color: Theme.codeBorder
                                                implicitHeight: codeHeader.implicitHeight + codeFlick.implicitHeight + 10

                                                Column {
                                                    anchors.fill: parent
                                                    anchors.margins: 6
                                                    spacing: 4

                                                    RowLayout {
                                                        id: codeHeader
                                                        width: parent.width
                                                        Label {
                                                            text: segment.language && segment.language.length > 0
                                                                  ? segment.language.toLowerCase()
                                                                  : "text"
                                                            color: Theme.codeLanguage
                                                            font.pixelSize: 11
                                                            font.bold: true
                                                            font.family: "Monospace"
                                                            Layout.fillWidth: true
                                                        }
                                                        Rectangle {
                                                            width: copyLabel.implicitWidth + 16
                                                            height: copyLabel.implicitHeight + 6
                                                            radius: 4
                                                            color: copyMa.containsMouse ? Theme.hoverOverlay : Theme.codeBackground
                                                            Label {
                                                            id: copyLabel
                                                            anchors.centerIn: parent
                                                            text: "⎘ Copiar Código"
                                                            color: Theme.codeText
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
                                                    implicitHeight: codeText.implicitHeight + 4
                                                    contentWidth: codeText.width
                                                    contentHeight: codeText.implicitHeight
                                                    clip: true
                                                    boundsBehavior: Flickable.StopAtBounds
                                                    interactive: contentHeight > height || contentWidth > width
                                                    ScrollBar.vertical: ScrollBar {
                                                        policy: ScrollBar.AsNeeded
                                                    }
                                                    ScrollBar.horizontal: ScrollBar {
                                                        policy: ScrollBar.AsNeeded
                                                    }

                                                    TextArea {
                                                        id: codeText
                                                        text: segment.value
                                                        color: Theme.codeText
                                                        font.family: "Monospace"
                                                        font.pixelSize: 13
                                                        wrapMode: TextArea.NoWrap
                                                        readOnly: true
                                                        selectByMouse: true
                                                        padding: 4
                                                        background: null
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }


                            Rectangle {
                                width: parent.width
                                height: visible ? operationsColumn.implicitHeight + 10 : 0
                                visible: model.role === "assistant" && (model.writeOps || []).length > 0
                                radius: 5
                                color: Theme.surfaceAlt
                                border.color: Theme.border

                                Column {
                                    id: operationsColumn
                                    anchors.fill: parent
                                    anchors.margins: 5
                                    spacing: 4

                                    RowLayout {
                                        width: parent.width
                                        Label {
                                            Layout.fillWidth: true
                                            text: "🔧 Operaciones (" + ((model.writeOps || []).length) + ")"
                                                  + (((model.writeOps || []).length > 0)
                                                     ? " · " + model.writeOps[0].tool + " " + (model.writeOps[0].file || "")
                                                     : "")
                                            color: Theme.textSecondary
                                            font.pixelSize: 11
                                            font.bold: true
                                        }
                                        Label {
                                            text: model.writeExpanded === true ? "▾" : "▸"
                                            color: Theme.textSecondary
                                            font.pixelSize: 11
                                        }
                                        MouseArea {
                                            anchors.fill: parent
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: messageModel.setProperty(index, "writeExpanded", !(model.writeExpanded === true))
                                        }
                                    }

                                    Repeater {
                                        model: model.writeExpanded === true ? (model.writeOps || []) : []
                                        delegate: Column {
                                            required property var modelData
                                            width: operationsColumn.width
                                            spacing: 2

                                            Label {
                                                width: parent.width
                                                text: "• " + modelData.tool + " · " + (modelData.file && modelData.file.length > 0 ? modelData.file : "(sin archivo)")
                                                color: Theme.textPrimary
                                                font.pixelSize: 11
                                                font.bold: true
                                                elide: Text.ElideMiddle
                                            }

                                            Rectangle {
                                                width: parent.width
                                                height: previewLabel.implicitHeight + 6
                                                radius: 3
                                                color: Theme.codeBackground
                                                Label {
                                                    id: previewLabel
                                                    anchors.fill: parent
                                                    anchors.margins: 4
                                                    text: "path: " + (modelData.file && modelData.file.length > 0 ? modelData.file : "-")
                                                        + "\ntool: " + (modelData.toolStatus && modelData.toolStatus.length > 0 ? modelData.toolStatus : "running")
                                                        + "\nvalidation: " + (modelData.validation && modelData.validation.length > 0 ? modelData.validation : "not run")
                                                    color: Theme.codeText
                                                    font.family: "Monospace"
                                                    font.pixelSize: 10
                                                    wrapMode: Text.WrapAnywhere
                                                }
                                            }

                                            Label {
                                                width: parent.width
                                                text: "Herramienta: " + (modelData.toolStatus && modelData.toolStatus.length > 0 ? modelData.toolStatus : "en ejecución")
                                                color: chatPage.validationColor(modelData.toolStatus)
                                                font.pixelSize: 11
                                            }

                                            Label {
                                                width: parent.width
                                                text: chatPage.validationLabel(modelData.validation)
                                                color: chatPage.validationColor(modelData.validation)
                                                font.pixelSize: 11
                                                font.bold: modelData.validation === "success"
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
                                    color: modelCopyMouse.containsMouse ? Theme.hoverOverlay : Theme.surfaceAlt
                                    border.color: Theme.border
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
            visible: controller && controller.availableModelsDetailed.length > 20
            spacing: 6
            TextField {
                Layout.fillWidth: true
                placeholderText: "Filtrar modelos (ej. claude, deepseek, gemini, free)..."
                text: chatPage.modelFilterText
                onTextChanged: {
                    chatPage.modelFilterText = text
                    chatPage.rebuildModelChoices()
                }
            }
            Label {
                text: filteredModelsModel.count + " de "
                      + (controller ? controller.availableModelsDetailed.length : 0)
                color: colorTextSecondary
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: chatPage.pendingImages.length > 0
            spacing: 8

            Label {
                text: "Adjuntos:"
                color: colorTextSecondary
                font.pixelSize: 11
            }

            Row {
                spacing: 6
                Repeater {
                    model: chatPage.pendingImages
                    delegate: Rectangle {
                        required property var modelData
                        required property int index
                        width: 54
                        height: 54
                        radius: 5
                        color: Theme.surfaceAlt
                        border.color: Theme.border

                        Image {
                            anchors.fill: parent
                            anchors.margins: 2
                            source: modelData.sourceUrl || ""
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                            cache: false
                        }

                        Rectangle {
                            width: 16
                            height: 16
                            radius: 8
                            anchors.right: parent.right
                            anchors.top: parent.top
                            anchors.rightMargin: -4
                            anchors.topMargin: -4
                            color: colorError
                            Label {
                                anchors.centerIn: parent
                                text: "×"
                                color: "white"
                                font.pixelSize: 12
                                font.bold: true
                            }
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: chatPage.removePendingImage(index)
                            }
                        }
                    }
                }
            }

            Item { Layout.fillWidth: true }
        }

        Rectangle {
            Layout.fillWidth: true
            height: visible ? 28 : 0
            visible: chatPage.liveExecutorStatus.tool.length > 0 && controller && controller.isLoading
            radius: 5
            color: Theme.surfaceAlt
            border.color: Theme.border

            Label {
                anchors.fill: parent
                anchors.margins: 6
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
                color: colorTextSecondary
                font.pixelSize: 11
                text: "🔧 " + chatPage.liveExecutorStatus.tool
                      + (chatPage.liveExecutorStatus.file.length > 0 ? " → " + chatPage.liveExecutorStatus.file : "")
                      + (chatPage.liveExecutorStatus.validation.length > 0 ? "  ·  " + chatPage.liveExecutorStatus.validation : "")
            }
        }

        RowLayout {
            Layout.fillWidth: true
            ComboBox {
                id: providerModelBox
                model: filteredModelsModel
                textRole: "label"
                Layout.fillWidth: true
                Layout.maximumWidth: 380
                Layout.minimumWidth: 120
                delegate: ItemDelegate {
                    width: providerModelBox.width
                    text: model.provider + " - " + model.label
                }
                onCurrentIndexChanged: {
                    if (modelSelectorInitializing || modelSelectorUpdating) {
                        return
                    }
                    if (currentIndex < 0 || currentIndex >= filteredModelsModel.count) {
                        return
                    }
                    const selected = filteredModelsModel.get(currentIndex)
                    controller.selectModel(selected.name, selected.serverUrl || "")
                }
            }

            Button {
                id: attachImageButton
                text: "📎"
                enabled: !isStreamingActiveScope()
                ToolTip.visible: attachImageHover.hovered
                ToolTip.text: "Adjuntar imagen (PNG/JPG/WEBP)"
                onClicked: imagePickerDialog.open()
                HoverHandler {
                    id: attachImageHover
                }
            }

            TextField {
                id: promptInput
                Layout.fillWidth: true
                Layout.minimumWidth: 80
                placeholderText: "Escribe tu mensaje...  (@ para archivos)"
                enabled: !isStreamingActiveScope()
                onAccepted: sendButton.clicked()
                onTextChanged: chatPage.updateMentionFromInput()
                onCursorPositionChanged: chatPage.updateMentionFromInput()
                onActiveFocusChanged: {
                    if (!activeFocus) {
                        chatPage.mentionOpen = false
                    }
                }
                Keys.onPressed: (event) => {
                    const isPaste = event.key === Qt.Key_V
                        && (event.modifiers & (Qt.ControlModifier | Qt.MetaModifier))
                    if (isPaste) {
                        const path = controller ? controller.saveClipboardImageToTemp() : ""
                        if (path && path.length > 0) {
                            chatPage.addPendingImage(path)
                            event.accepted = true
                            return
                        }
                    }

                    if (!chatPage.mentionOpen) {
                        return
                    }

                    if (event.key === Qt.Key_Down) {
                        chatPage.mentionIndex = Math.min(chatPage.mentionIndex + 1, chatPage.mentionResults.length - 1)
                        event.accepted = true
                        return
                    }
                    if (event.key === Qt.Key_Up) {
                        chatPage.mentionIndex = Math.max(chatPage.mentionIndex - 1, 0)
                        event.accepted = true
                        return
                    }
                    if (event.key === Qt.Key_Escape) {
                        chatPage.mentionOpen = false
                        event.accepted = true
                        return
                    }
                    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Tab) {
                        chatPage.acceptMention(chatPage.mentionIndex)
                        event.accepted = true
                    }
                }
            }

            Label {
                id: tokenCounter
                text: chatPage.formatTokenCount(chatPage.contextEstimatedTokens)
                      + " / "
                      + chatPage.formatTokenCount(chatPage.contextNumCtx > 0 ? chatPage.contextNumCtx : chatPage.contextMaxTokens)
                color: (chatPage.contextNumCtx > 0 && chatPage.contextEstimatedTokens > chatPage.contextNumCtx * 0.9)
                       ? colorWarning
                       : colorTextSecondary
                font.pixelSize: 11
                font.family: "Monospace"
                ToolTip.visible: tokenCounterHover.hovered
                ToolTip.text: "Tokens estimados frente al num_ctx configurado"
                HoverHandler {
                    id: tokenCounterHover
                }
            }

            Button {
                id: sendButton
                text: "Enviar"
                enabled: !isStreamingActiveScope()
                         && !pendingSelectNewestSession
                         && (promptInput.text.trim().length > 0 || chatPage.pendingImages.length > 0)
                onClicked: {
                    if (pendingSelectNewestSession) {
                        return
                    }
                    const prompt = promptInput.text.trim()
                    const imagePaths = chatPage.pendingImages.map(entry => entry.path)
                    if (prompt.length === 0 && imagePaths.length === 0) {
                        return
                    }
                    const viewSessionId = currentViewSessionId()
                    if (viewSessionId.length === 0) {
                        return
                    }
                    if (controller.selectedSessionId !== viewSessionId) {
                        controller.selectSession(viewSessionId)
                    }

                    const outboundPrompt = prompt.length > 0 ? prompt : "Describe la imagen adjunta."
                    messageModel.append({
                        role: "user",
                        text: prompt.length > 0 ? prompt : "(imagen adjunta)",
                        contextFiles: [],
                        contextExpanded: false,
                        writeOps: [],
                        writeExpanded: false
                    })
                    scrollToBottom()
                    promptInput.text = ""
                    pendingAssistantText = ""

                    if (imagePaths.length > 0) {
                        controller.sendPromptForSessionWithImages(viewSessionId, outboundPrompt, imagePaths)
                    } else {
                        controller.sendPromptForSession(viewSessionId, outboundPrompt)
                    }
                    resetComposerTransients()
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

    Rectangle {
        id: mentionPopup
        visible: chatPage.mentionOpen && chatPage.mentionResults.length > 0
        z: 20
        width: Math.min(420, chatPage.width - 32)
        height: Math.min(260, Math.max(36, mentionList.contentHeight + 8))
        radius: 6
        color: Theme.surface
        border.color: Theme.border
        border.width: 1
        x: {
            if (!promptInput) {
                return 16
            }
            const reference = mentionPopup.parent ? mentionPopup.parent : chatPage
            const point = promptInput.mapToItem(reference, 0, 0)
            return Math.max(12, Math.min(chatPage.width - width - 12, point.x))
        }
        y: {
            if (!promptInput) {
                return 120
            }
            const reference = mentionPopup.parent ? mentionPopup.parent : chatPage
            const point = promptInput.mapToItem(reference, 0, 0)
            return Math.max(12, point.y - height - 6)
        }

        ListView {
            id: mentionList
            anchors.fill: parent
            anchors.margins: 4
            clip: true
            model: chatPage.mentionResults
            currentIndex: chatPage.mentionIndex
            delegate: Rectangle {
                required property var modelData
                required property int index
                width: mentionList.width
                height: 28
                radius: 4
                color: index === chatPage.mentionIndex ? Theme.hoverOverlay : "transparent"

                Label {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    verticalAlignment: Text.AlignVCenter
                    text: "@" + modelData
                    color: Theme.textPrimary
                    font.pixelSize: 12
                    elide: Text.ElideMiddle
                }

                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onEntered: chatPage.mentionIndex = index
                    onClicked: chatPage.acceptMention(index)
                }
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
            const draftIndex = currentAssistantDraftIndex()
            if (draftIndex < 0) {
                messageModel.append({
                    role: "assistant",
                    text: "",
                    isStreaming: true,
                    contextFiles: chatPage.pendingContextFiles,
                    contextExpanded: false,
                    writeOps: chatPage.pendingWriteOps,
                    writeExpanded: false
                })
            } else {
                messageModel.setProperty(draftIndex, "isStreaming", true)
                messageModel.setProperty(draftIndex, "contextFiles", chatPage.pendingContextFiles)
                messageModel.setProperty(draftIndex, "writeOps", chatPage.pendingWriteOps)
            }
            scrollToBottom()
        }
        function onStreamChunk(sessionId, text) {
            if (sessionId !== currentViewSessionId()) {
                return
            }
            let draftIndex = currentAssistantDraftIndex()
            if (draftIndex < 0) {
                messageModel.append({
                    role: "assistant",
                    text: "",
                    isStreaming: true,
                    contextFiles: chatPage.pendingContextFiles,
                    contextExpanded: false,
                    writeOps: chatPage.pendingWriteOps,
                    writeExpanded: false
                })
                draftIndex = currentAssistantDraftIndex()
                pendingAssistantText = ""
            }
            if (draftIndex < 0) {
                return
            }
            if (pendingAssistantText.length === 0) {
                const currentDraft = messageModel.get(draftIndex)
                pendingAssistantText = String(currentDraft && currentDraft.text ? currentDraft.text : "")
            }
            pendingAssistantText += text
            messageModel.setProperty(draftIndex, "text", pendingAssistantText)
        }
        function onStreamFinished(sessionId) {
            if (sessionId !== currentViewSessionId()) {
                return
            }
            let draftIndex = currentAssistantDraftIndex()
            if (draftIndex >= 0) {
                messageModel.setProperty(draftIndex, "isStreaming", false)
            }
            pendingAssistantText = ""
            scrollToBottom()
        }
        function onStreamError(sessionId, message) {
            if (sessionId !== currentViewSessionId()) {
                return
            }
            messageModel.append({
                role: "assistant",
                text: "Error: " + message,
                isStreaming: false,
                contextFiles: chatPage.pendingContextFiles,
                contextExpanded: false,
                writeOps: chatPage.pendingWriteOps,
                writeExpanded: false
            })
            scrollToBottom()
        }
        function onMessagesHydrated(messages) {
            messageModel.clear()
            for (let i = 0; i < messages.length; i += 1) {
                messageModel.append({
                    role: messages[i].role,
                    text: messages[i].text,
                    isStreaming: false,
                    contextFiles: [],
                    contextExpanded: false,
                    writeOps: [],
                    writeExpanded: false
                })
            }
            scrollToBottom()
        }
        function onSelectedSessionIdChanged() {
            chatPage.resetComposerTransients()
            chatPage.contextEstimatedTokens = 0
            chatPage.contextNumCtx = 0
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
            rebuildModelChoices()
            modelSelectorInitializing = false
        }
        function onActiveModelChanged() {
            if (!modelSelectorInitializing) {
                rebuildModelChoices()
            }
        }
        function onContextFilesUpdated(sessionId, files) {
            if (sessionId !== currentViewSessionId()) {
                return
            }
            chatPage.pendingContextFiles = files ? files : []
            const draftIndex = currentAssistantDraftIndex()
            if (draftIndex >= 0 && messageModel.get(draftIndex).isStreaming === true) {
                messageModel.setProperty(draftIndex, "contextFiles", chatPage.pendingContextFiles)
            }
        }
        function onExecutorStatusUpdated(sessionId, tool, file, validationResult) {
            if (sessionId !== currentViewSessionId()) {
                return
            }
            chatPage.recordExecutorStatus(tool, file, validationResult)
            const draftIndex = currentAssistantDraftIndex()
            if (draftIndex >= 0 && messageModel.get(draftIndex).isStreaming === true) {
                messageModel.setProperty(draftIndex, "writeOps", chatPage.pendingWriteOps)
            }
        }
        function onTokenBudgetUpdated(sessionId, estimatedTokens, numCtx) {
            if (sessionId !== currentViewSessionId()) {
                return
            }
            chatPage.contextEstimatedTokens = estimatedTokens
            chatPage.contextNumCtx = numCtx
        }
        function onTokenUsageUpdated(sessionId, promptTokens, completionTokens, totalTokens, isEstimated) {
            if (sessionId !== currentViewSessionId()) {
                return
            }
            currentPromptTokens = promptTokens
            currentCompletionTokens = completionTokens
            currentTotalTokens = totalTokens
            currentUsageIsEstimated = isEstimated
        }
    }
}
