import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import MultiLink 1.0

Page {
    id: settingsPage
    title: "Configuracion"
    required property ChatController controller

    property string statusText: ""
    property bool statusOk: false

    readonly property real narrowThreshold: 600
    readonly property real outerMargin: Math.min(16, settingsPage.width * 0.03)

    function loadServers() {
        serversModel.clear()
        const raw = controller ? controller.serversConfigJson() : "[]"
        let rows = []
        try {
            rows = JSON.parse(raw)
        } catch (e) {
            statusOk = false
            statusText = "No se pudo leer configuracion de servidores"
            return
        }

        for (let i = 0; i < rows.length; i += 1) {
            const row = rows[i]
            serversModel.append({
                name: row.name || "",
                provider: row.provider || "ollama",
                base_url: row.base_url || "http://127.0.0.1:11434",
                default_model: row.default_model || "auto",
                priority: Number(row.priority || (i + 1)),
                enabled: row.enabled === undefined ? true : !!row.enabled,
                selected_models: JSON.stringify(Array.isArray(row.selected_models) ? row.selected_models : []),
                test_result: "",
                testing: false
            })
        }
    }

    function parseSelectedModels(raw) {
        try {
            const parsed = JSON.parse(raw || "[]")
            return Array.isArray(parsed) ? parsed : []
        } catch (e) {
            return []
        }
    }

    function serializeServers() {
        const arr = []
        for (let i = 0; i < serversModel.count; i += 1) {
            const item = serversModel.get(i)
            arr.push({
                name: item.name,
                provider: item.provider,
                base_url: item.base_url,
                default_model: item.default_model,
                priority: Math.max(1, Number(item.priority || (i + 1))),
                enabled: !!item.enabled,
                selected_models: parseSelectedModels(item.selected_models)
            })
        }
        return JSON.stringify(arr)
    }

    function saveServers() {
        if (!controller) return
        const ok = controller.saveServersConfigJson(serializeServers())
        statusOk = ok
        statusText = ok
            ? "Configuracion guardada. Reinicia la app para aplicar cambios de servidor activos."
            : "No se pudo guardar la configuracion"
    }

    function openModelPicker(index) {
        if (!controller || index < 0 || index >= serversModel.count) {
            return
        }
        const item = serversModel.get(index)
        modelPickerDialog.serverIndex = index
        modelPickerDialog.serverName = item.name
        modelPickerDialog.provider = item.provider
        modelPickerDialog.baseUrl = item.base_url
        modelPickerDialog.previousSelection = parseSelectedModels(item.selected_models)
        const preselected = ({})
        for (let i = 0; i < modelPickerDialog.previousSelection.length; i += 1) {
            preselected[modelPickerDialog.previousSelection[i]] = true
        }
        modelPickerDialog.searchText = ""
        modelPickerDialog.statusText = ""
        modelPickerDialog.statusOk = true
        modelPickerDialog.loaded = false
        modelPickerDialog.allModels = []
        modelPickerDialog.selectedSet = preselected
        pickerModel.clear()
        modelPickerDialog.open()
        modelPickerDialog.startLoad()
    }

    function defaultBaseUrlFor(provider) {
        if (provider === "deepseek") return "https://api.deepseek.com"
        if (provider === "gemini") return "https://generativelanguage.googleapis.com"
        if (provider === "codex") return "https://api.openai.com/v1"
        if (provider === "openrouter") return "https://openrouter.ai/api/v1"
        return "http://127.0.0.1:11434"
    }

    function loadProviderTokens() {
        providerKeysModel.clear()
        const providers = [
            { key: "deepseek", label: "DeepSeek" },
            { key: "gemini", label: "Gemini" },
            { key: "codex", label: "Codex / OpenAI" },
            { key: "openrouter", label: "OpenRouter" }
        ]
        for (let i = 0; i < providers.length; i += 1) {
            const entry = providers[i]
            providerKeysModel.append({
                providerKey: entry.key,
                label: entry.label,
                hasToken: controller ? controller.hasProviderToken(entry.key) : false,
                inputText: "",
                testing: false,
                statusText: ""
            })
        }
    }

    function saveProviderTokenAt(index) {
        const item = providerKeysModel.get(index)
        if (!item.inputText || item.inputText.length === 0) {
            return
        }
        const ok = controller
            ? controller.saveProviderToken(item.providerKey, item.inputText)
            : false
        providerKeysModel.setProperty(index, "hasToken", ok)
        providerKeysModel.setProperty(index, "statusText",
            ok ? "Clave guardada. Ya esta activa." : "No se pudo guardar la clave.")
        providerKeysModel.setProperty(index, "inputText", "")
    }

    function testProviderKeyAt(index) {
        const item = providerKeysModel.get(index)
        const candidate = item.inputText && item.inputText.length > 0 ? item.inputText : ""
        providerKeysModel.setProperty(index, "testing", true)
        providerKeysModel.setProperty(index, "statusText", "Probando clave...")
        const raw = controller
            ? controller.testProviderKey(item.providerKey, candidate)
            : "{\"ok\":false,\"error\":\"controller no disponible\",\"hint\":\"\"}"
        let parsed = { ok: false, model_count: 0, error: "respuesta invalida", hint: "" }
        try {
            parsed = JSON.parse(raw)
        } catch (e) {
        }
        const ok = !!parsed.ok
        providerKeysModel.setProperty(index, "testing", false)
        if (ok) {
            providerKeysModel.setProperty(index, "hasToken", true)
            providerKeysModel.setProperty(index, "inputText", "")
            providerKeysModel.setProperty(index, "statusText",
                "Clave valida. Modelos detectados: " + parsed.model_count
                + ". Pulsa \"Recargar\" en Servidores y luego \"Modelos\" para elegir cuales ver en el chat.")
        } else {
            providerKeysModel.setProperty(index, "statusText",
                "Error: " + parsed.error
                + (parsed.hint && parsed.hint.length > 0 ? "\n" + parsed.hint : ""))
        }
    }

    function clearProviderTokenAt(index) {
        const item = providerKeysModel.get(index)
        const ok = controller
            ? controller.clearProviderToken(item.providerKey)
            : false
        if (ok) {
            providerKeysModel.setProperty(index, "hasToken", false)
        }
        providerKeysModel.setProperty(index, "statusText",
            ok ? "Clave eliminada." : "No se pudo eliminar la clave.")
        providerKeysModel.setProperty(index, "inputText", "")
    }

    Component.onCompleted: {
        loadServers()
        loadProviderTokens()
    }

    ListModel { id: serversModel }
    ListModel { id: providerKeysModel }
    ListModel { id: pickerModel }
    Timer {
        id: pickerLoadTimer
        interval: 60
        repeat: false
        onTriggered: modelPickerDialog.loadModels()
    }

    ScrollView {
        id: settingsScroll
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            x: settingsPage.outerMargin
            width: Math.max(0, settingsScroll.availableWidth - (2 * settingsPage.outerMargin))
            spacing: 12
            Layout.fillWidth: true

            Label {
                text: "Servidores"
                font.bold: true
                font.pixelSize: 18
                Layout.topMargin: settingsPage.outerMargin
            }

            Label {
                text: "Define servidores Ollama locales o remotos y su prioridad de uso."
                color: "#666"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            Repeater {
                model: serversModel
                delegate: Frame {
                    Layout.fillWidth: true
                    padding: 10

                    ColumnLayout {
                        anchors.fill: parent
                        spacing: 8

                        RowLayout {
                            Layout.fillWidth: true
                            Label {
                                text: "Servidor #" + (index + 1)
                                font.bold: true
                            }
                            Item { Layout.fillWidth: true }
                            CheckBox {
                                text: "Activo"
                                checked: model.enabled
                                onToggled: serversModel.setProperty(index, "enabled", checked)
                            }
                            Button {
                                text: {
                                    const count = settingsPage.parseSelectedModels(model.selected_models).length
                                    return count > 0 ? ("Modelos (" + count + ")") : "Modelos"
                                }
                                onClicked: settingsPage.openModelPicker(index)
                            }
                            Button {
                                text: "Eliminar"
                                onClicked: {
                                    serversModel.remove(index)
                                }
                            }
                        }

                        GridLayout {
                            id: serverFormGrid
                            columns: width > 1100 ? 3 : (width > settingsPage.narrowThreshold ? 2 : 1)
                            columnSpacing: 8
                            rowSpacing: 8
                            Layout.fillWidth: true

                            TextField {
                                placeholderText: "Nombre"
                                text: model.name
                                Layout.fillWidth: true
                                onTextChanged: serversModel.setProperty(index, "name", text)
                            }
                            ComboBox {
                                model: ["ollama", "ollama_cloud", "gemini", "codex", "deepseek", "openrouter"]
                                currentIndex: Math.max(0, ["ollama", "ollama_cloud", "gemini", "codex", "deepseek", "openrouter"].indexOf(model.provider))
                                onActivated: {
                                    const nextProvider = currentText
                                    const previousBase = model.base_url || ""
                                    const looksLocal = previousBase.length === 0
                                        || previousBase.indexOf("127.0.0.1") >= 0
                                        || previousBase.indexOf("localhost") >= 0
                                    serversModel.setProperty(index, "provider", nextProvider)
                                    if (looksLocal) {
                                        serversModel.setProperty(index, "base_url",
                                            defaultBaseUrlFor(nextProvider))
                                    }
                                    if (nextProvider === "deepseek"
                                            && (!model.default_model || model.default_model === "auto")) {
                                        serversModel.setProperty(index, "default_model", "deepseek-chat")
                                    }
                                }
                            }

                            TextField {
                                placeholderText: "http://127.0.0.1:11434"
                                text: model.base_url
                                Layout.fillWidth: true
                                onTextChanged: serversModel.setProperty(index, "base_url", text)
                            }

                            TextField {
                                placeholderText: "Modelo por defecto (auto o nombre)"
                                text: model.default_model
                                Layout.fillWidth: true
                                onTextChanged: serversModel.setProperty(index, "default_model", text)
                            }

                            SpinBox {
                                from: 1
                                to: 255
                                value: model.priority
                                editable: true
                                Layout.fillWidth: true
                                onValueChanged: serversModel.setProperty(index, "priority", value)
                            }

                            Button {
                                text: model.testing ? "Cargando..." : "Probar conexion"
                                enabled: !model.testing
                                Layout.fillWidth: true
                                onClicked: {
                                    serversModel.setProperty(index, "test_result", "Cargando modelos...")
                                    serversModel.setProperty(index, "testing", true)
                                    const raw = controller.testServerConnection(model.base_url)
                                    let parsed = { ok: false, model_count: 0, error: "respuesta invalida", hint: "" }
                                    try {
                                        parsed = JSON.parse(raw)
                                    } catch (e) {
                                    }
                                    const text = parsed.ok
                                        ? ("OK - modelos detectados: " + parsed.model_count)
                                        : ("Error: " + parsed.error + (parsed.hint && parsed.hint.length > 0 ? "\n\nSugerencia:\n" + parsed.hint : ""))
                                    serversModel.setProperty(index, "test_result", text)
                                    serversModel.setProperty(index, "testing", false)
                                }
                            }
                        }

                        Label {
                            text: "Usa \"Modelos\" para elegir que modelos de este servidor aparecen en el chat. Sin seleccion se muestran todos."
                            color: "#666"
                            font.pixelSize: 11
                            wrapMode: Text.Wrap
                            Layout.fillWidth: true
                        }

                        Label {
                            text: model.test_result
                            color: model.test_result.startsWith("OK") ? "#2E7D32" : "#B3261E"
                            visible: model.test_result.length > 0
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                        }
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 6
                Button {
                    text: settingsPage.width > settingsPage.narrowThreshold ? "Agregar servidor" : "Agregar"
                    Layout.fillWidth: true
                    onClicked: {
                        serversModel.append({
                            name: "Nuevo servidor",
                            provider: "ollama",
                            base_url: "http://127.0.0.1:11434",
                            default_model: "auto",
                            priority: serversModel.count + 1,
                            enabled: true,
                            test_result: "",
                            testing: false
                        })
                    }
                }
                Button {
                    text: "Recargar"
                    Layout.fillWidth: true
                    onClicked: loadServers()
                }
                Button {
                    text: "Guardar"
                    Layout.fillWidth: true
                    highlighted: true
                    onClicked: saveServers()
                }
            }

            Label {
                text: statusText
                visible: statusText.length > 0
                color: statusOk ? "#2E7D32" : "#B3261E"
                Layout.fillWidth: true
                wrapMode: Text.Wrap
            }

            Label {
                text: "Claves de API"
                font.bold: true
                font.pixelSize: 18
                Layout.topMargin: 12
            }

            Label {
                text: "Pega la clave y pulsa \"Probar clave\": se valida, se guarda y los modelos del proveedor aparecen como un servidor mas (con prioridad editable en Servidores)."
                color: "#666"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            Repeater {
                model: providerKeysModel
                delegate: Frame {
                    Layout.fillWidth: true
                    padding: 10

                    ColumnLayout {
                        anchors.fill: parent
                        spacing: 8

                        RowLayout {
                            Layout.fillWidth: true
                            Label {
                                text: model.label
                                font.bold: true
                            }
                            Item { Layout.fillWidth: true }
                            Label {
                                text: model.hasToken ? "Configurada" : "Sin configurar"
                                color: model.hasToken ? "#2E7D32" : "#B3261E"
                            }
                        }

                        GridLayout {
                            Layout.fillWidth: true
                            columns: settingsPage.width > settingsPage.narrowThreshold ? 4 : 1
                            columnSpacing: 6
                            rowSpacing: 6

                            TextField {
                                Layout.fillWidth: true
                                placeholderText: model.hasToken
                                    ? "Reemplazar clave..."
                                    : "Pega la API key aqui"
                                echoMode: TextInput.Password
                                text: model.inputText
                                onTextChanged: providerKeysModel.setProperty(index, "inputText", text)
                            }
                            Button {
                                text: model.testing ? "Probando..." : "Probar clave"
                                enabled: !model.testing
                                    && (model.hasToken || model.inputText.length > 0)
                                Layout.fillWidth: settingsPage.width <= settingsPage.narrowThreshold
                                onClicked: testProviderKeyAt(index)
                            }
                            Button {
                                text: "Guardar"
                                enabled: model.inputText.length > 0
                                Layout.fillWidth: settingsPage.width <= settingsPage.narrowThreshold
                                onClicked: saveProviderTokenAt(index)
                            }
                            Button {
                                text: "Borrar"
                                enabled: model.hasToken && !model.testing
                                Layout.fillWidth: settingsPage.width <= settingsPage.narrowThreshold
                                onClicked: clearProviderTokenAt(index)
                            }
                        }

                        Label {
                            text: model.statusText
                            visible: model.statusText.length > 0
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            color: "#666"
                        }
                    }
                }
            }

            // ─── MCP Servers ─────────────────────────────────────────────
            Label {
                text: "Servidores MCP"
                font.bold: true
                font.pixelSize: 18
                Layout.topMargin: 12
            }

            Label {
                text: "Configura servidores MCP (Model Context Protocol) para dar herramientas externas al modelo. Usa formato JSON compatible con OpenCode/Claude."
                color: "#666"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            Button {
                text: "Abrir editor MCP"
                Layout.fillWidth: true
                onClicked: mcpEditorDialog.open()
            }
        }
    }

    // ─── MCP Config Modal ──────────────────────────────────────────────────
    Dialog {
        id: mcpEditorDialog
        anchors.centerIn: parent
        modal: true
        focus: true
        standardButtons: Dialog.NoButton
        width: Math.min(settingsPage.width * 0.9, 900)
        height: Math.min(settingsPage.height * 0.85, 700)
        padding: 16

        property string editorText: ""
        property string statusText: ""
        property bool statusOk: false

        function loadMcpConfig() {
            const raw = controller ? controller.mcpConfigJson() : "[]"
            editorText = ""
            try {
                const obj = JSON.parse(raw)
                const pretty = JSON.stringify(obj, null, 2)
                editorText = pretty
            } catch (e) {
                editorText = raw
            }
            statusText = ""
            statusOk = false
        }

        function saveMcpConfig() {
            if (!controller) return
            try {
                JSON.parse(editorText)
            } catch (e) {
                statusText = "JSON invalid: " + e.message
                statusOk = false
                return
            }
            const ok = controller.saveMcpConfigJson(editorText)
            statusText = ok
                ? "Configuracion MCP guardada."
                : "No se pudo guardar la configuracion MCP."
            statusOk = ok
        }

        onOpened: loadMcpConfig()

        ColumnLayout {
            anchors.fill: parent
            spacing: 12

            Label {
                text: "Configuracion MCP (JSON)"
                font.bold: true
                font.pixelSize: 16
                Layout.fillWidth: true
            }

            Label {
                text: "Cada servidor en el arreglo \"servers\" usa: { \"name\": \"...\", \"command\": \"npx\", \"args\": [\"-y\", \"@modelcontextprotocol/server-filesystem\", \"/path\"], \"env\": {}, \"enabled\": true }"
                color: "#666"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
                font.pixelSize: 11
            }

            Label {
                text: "Guia completa con ejemplos y seguridad: docs/MCP.md"
                color: "#1565C0"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
                font.pixelSize: 11
            }

            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true

                TextArea {
                    id: mcpTextArea
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    text: mcpEditorDialog.editorText
                    font.family: "monospace"
                    font.pixelSize: 12
                    wrapMode: Text.NoWrap
                    selectByMouse: true
                    onTextChanged: mcpEditorDialog.editorText = text
                }
            }

            Label {
                text: mcpEditorDialog.statusText
                visible: mcpEditorDialog.statusText.length > 0
                color: mcpEditorDialog.statusOk ? "#2E7D32" : "#B3261E"
                Layout.fillWidth: true
                wrapMode: Text.Wrap
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 6
                Button {
                    text: "Recargar"
                    Layout.fillWidth: true
                    onClicked: mcpEditorDialog.loadMcpConfig()
                }
                Button {
                    text: "Formatear JSON"
                    Layout.fillWidth: true
                    onClicked: {
                        try {
                            const obj = JSON.parse(mcpEditorDialog.editorText)
                            mcpEditorDialog.editorText = JSON.stringify(obj, null, 2)
                            mcpEditorDialog.statusText = ""
                        } catch (e) {
                            mcpEditorDialog.statusText = "JSON invalido: " + e.message
                            mcpEditorDialog.statusOk = false
                        }
                    }
                }
                Button {
                    text: "Guardar"
                    Layout.fillWidth: true
                    highlighted: true
                    onClicked: mcpEditorDialog.saveMcpConfig()
                }
                Button {
                    text: "Cerrar"
                    Layout.fillWidth: true
                    onClicked: mcpEditorDialog.close()
                }
            }
        }
    }

    // ─── Model selection modal ─────────────────────────────────────────────
    Dialog {
        id: modelPickerDialog
        anchors.centerIn: parent
        modal: true
        focus: true
        standardButtons: Dialog.NoButton
        width: Math.min(settingsPage.width * 0.92, 720)
        height: Math.min(settingsPage.height * 0.85, 640)
        padding: 16

        property int serverIndex: -1
        property string serverName: ""
        property string provider: ""
        property string baseUrl: ""
        property string searchText: ""
        property string statusText: ""
        property bool statusOk: true
        property bool loaded: false
        property var allModels: []
        property var selectedSet: ({})
        property var previousSelection: []

        onClosed: pickerLoadTimer.stop()

        function startLoad() {
            loaded = false
            statusOk = true
            statusText = "Cargando modelos del servidor..."
            pickerModel.clear()
            pickerLoadTimer.restart()
        }

        function loadModels() {
            const raw = controller
                ? controller.listServerModels(provider, baseUrl)
                : "{\"ok\":false,\"models\":[],\"error\":\"controller no disponible\",\"hint\":\"\"}"
            let parsed = { ok: false, models: [], error: "respuesta invalida", hint: "" }
            try {
                parsed = JSON.parse(raw)
            } catch (e) {
            }
            if (!parsed.ok) {
                loaded = false
                statusOk = false
                statusText = "Error: " + (parsed.error || "respuesta invalida")
                    + (parsed.hint && parsed.hint.length > 0 ? "\n" + parsed.hint : "")
                return
            }
            allModels = Array.isArray(parsed.models) ? parsed.models : []
            loaded = true
            rebuildRows()
            statusOk = true
            statusText = allModels.length + " modelos detectados. Marca los que quieras ver en el chat."
        }

        function rebuildRows() {
            pickerModel.clear()
            const needle = String(searchText || "").trim().toLowerCase()
            for (let i = 0; i < allModels.length; i += 1) {
                const modelId = String(allModels[i] || "")
                if (modelId.length === 0) {
                    continue
                }
                if (needle.length > 0 && modelId.toLowerCase().indexOf(needle) < 0) {
                    continue
                }
                pickerModel.append({
                    modelId: modelId,
                    checked: selectedSet[modelId] === true
                })
            }
        }

        function setChecked(modelId, checked) {
            const next = ({})
            for (const key in selectedSet) {
                if (selectedSet[key] === true) {
                    next[key] = true
                }
            }
            if (checked) {
                next[modelId] = true
            } else {
                delete next[modelId]
            }
            selectedSet = next
            for (let i = 0; i < pickerModel.count; i += 1) {
                if (pickerModel.get(i).modelId === modelId) {
                    pickerModel.setProperty(i, "checked", checked)
                    break
                }
            }
        }

        function setVisibleChecked(checked) {
            const next = ({})
            for (const key in selectedSet) {
                if (selectedSet[key] === true) {
                    next[key] = true
                }
            }
            for (let i = 0; i < pickerModel.count; i += 1) {
                const modelId = pickerModel.get(i).modelId
                if (checked) {
                    next[modelId] = true
                } else {
                    delete next[modelId]
                }
            }
            selectedSet = next
            rebuildRows()
        }

        function saveSelection() {
            const ids = []
            for (const key in selectedSet) {
                if (selectedSet[key] === true) {
                    ids.push(key)
                }
            }
            ids.sort()
            serversModel.setProperty(serverIndex, "selected_models", JSON.stringify(ids))
            settingsPage.saveServers()
            statusOk = true
            statusText = ids.length === 0
                ? "Guardado: se mostraran todos los modelos del servidor."
                : ("Guardado: " + ids.length + " modelos seleccionados.")
        }

        ColumnLayout {
            anchors.fill: parent
            spacing: 10

            Label {
                text: "Modelos de " + modelPickerDialog.serverName
                font.bold: true
                font.pixelSize: 16
                Layout.fillWidth: true
                elide: Label.ElideRight
            }

            Label {
                text: modelPickerDialog.provider + " - " + modelPickerDialog.baseUrl
                color: "#666"
                font.pixelSize: 11
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            TextField {
                Layout.fillWidth: true
                placeholderText: "Buscar modelo (ej. claude, gpt, free)..."
                text: modelPickerDialog.searchText
                onTextChanged: {
                    modelPickerDialog.searchText = text
                    modelPickerDialog.rebuildRows()
                }
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 6
                Label {
                    Layout.fillWidth: true
                    text: pickerModel.count + " visibles de " + modelPickerDialog.allModels.length
                    color: "#666"
                    font.pixelSize: 11
                }
                Button {
                    text: "Seleccionar visibles"
                    enabled: modelPickerDialog.loaded && pickerModel.count > 0
                    onClicked: modelPickerDialog.setVisibleChecked(true)
                }
                Button {
                    text: "Quitar visibles"
                    enabled: modelPickerDialog.loaded && pickerModel.count > 0
                    onClicked: modelPickerDialog.setVisibleChecked(false)
                }
            }

            ListView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: pickerModel
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                delegate: CheckBox {
                    width: ListView.view.width
                    text: model.modelId
                    checked: model.checked
                    onToggled: modelPickerDialog.setChecked(model.modelId, checked)
                }
            }

            Label {
                text: modelPickerDialog.statusText
                visible: modelPickerDialog.statusText.length > 0
                color: modelPickerDialog.statusOk ? "#2E7D32" : "#B3261E"
                Layout.fillWidth: true
                wrapMode: Text.Wrap
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 6
                Button {
                    text: "Recargar"
                    Layout.fillWidth: true
                    onClicked: modelPickerDialog.startLoad()
                }
                Button {
                    text: "Guardar seleccion"
                    Layout.fillWidth: true
                    highlighted: true
                    enabled: modelPickerDialog.loaded
                    onClicked: modelPickerDialog.saveSelection()
                }
                Button {
                    text: "Cerrar"
                    Layout.fillWidth: true
                    onClicked: modelPickerDialog.close()
                }
            }
        }
    }
}
