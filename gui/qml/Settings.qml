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
                test_result: ""
            })
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
                enabled: !!item.enabled
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

    Component.onCompleted: loadServers()

    ListModel { id: serversModel }

    ScrollView {
        anchors.fill: parent

        ColumnLayout {
            anchors.margins: Math.min(16, settingsPage.width * 0.03)
            spacing: 12

            Label {
                text: "Servidores"
                font.bold: true
                font.pixelSize: 18
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
                                text: "Eliminar"
                                onClicked: {
                                    serversModel.remove(index)
                                }
                            }
                        }

                        GridLayout {
                            id: serverFormGrid
                            columns: width > settingsPage.narrowThreshold ? 2 : 1
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
                                model: ["ollama", "ollama_cloud", "gemini", "codex"]
                                currentIndex: Math.max(0, ["ollama", "ollama_cloud", "gemini", "codex"].indexOf(model.provider))
                                onActivated: serversModel.setProperty(index, "provider", currentText)
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
        }
    }
}
