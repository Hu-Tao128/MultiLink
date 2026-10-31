import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import MultiLink 1.0

Page {
    id: statsPage
    required property ChatController controller

    // Colores del tema (inyectados desde Main.qml)
    property color colorBackground: "#F5F6F7"
    property color colorSurface: "#FFFFFF"
    property color colorSurfaceAlt: "#F0F0F0"
    property color colorBorder: "#E0E0E0"
    property color colorTextPrimary: "#1E1E1E"
    property color colorTextSecondary: "#6B6B6B"
    property color colorTextMuted: "#999999"
    property color colorAccentLocal: "#2E7D32"
    property color colorAccentRemote: "#1565C0"
    property color colorAccentWarning: "#ED6C02"
    property color colorAccentError: "#C62828"

    property var globalUsage: ({})
    property var sessionUsage: ({})
    property var budgetInfo: ({})
    property bool dataLoaded: false

    function loadUsageData() {
        if (!controller) return

        // Cargar resumen global
        const globalRaw = controller.usageSummaryJson()
        try {
            globalUsage = JSON.parse(globalRaw)
        } catch (e) {
            globalUsage = {}
        }

        // Cargar resumen de sesión activa
        const sessionRaw = controller.sessionUsageJson()
        try {
            sessionUsage = JSON.parse(sessionRaw)
        } catch (e) {
            sessionUsage = {}
        }

        // Cargar información de presupuesto
        const budgetRaw = controller.tokenBudgetJson()
        try {
            budgetInfo = JSON.parse(budgetRaw)
        } catch (e) {
            budgetInfo = {}
        }

        dataLoaded = true
    }

    Component.onCompleted: loadUsageData()

    Connections {
        target: controller
        function onSessionsChanged() { loadUsageData() }
    }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            x: Math.min(16, statsPage.width * 0.03)
            width: Math.max(0, statsPage.width - (2 * Math.min(16, statsPage.width * 0.03)))
            spacing: 16
            Layout.fillWidth: true
            Layout.topMargin: 12
            Layout.bottomMargin: 12

            // ─── Título ──────────────────────────────────────────────
            Label {
                text: "Estadísticas de Uso"
                font.bold: true
                font.pixelSize: 22
                color: Theme.textPrimary
                Layout.fillWidth: true
            }

            Label {
                text: "Resumen de consumo de tokens y modelos utilizados."
                color: Theme.textSecondary
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            // ─── Presupuesto / Créditos ──────────────────────────────
            Frame {
                Layout.fillWidth: true
                padding: 14
                visible: budgetInfo.total_pool > 0

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 10

                    RowLayout {
                        Layout.fillWidth: true
                        Label {
                            text: "💳 Presupuesto de Tokens"
                            font.bold: true
                            font.pixelSize: 16
                            color: Theme.textPrimary
                            Layout.fillWidth: true
                        }
                        Label {
                            text: budgetInfo.is_exceeded ? "⚠️ Excedido" : ""
                            color: Theme.accentError
                            font.bold: true
                            visible: budgetInfo.is_exceeded
                        }
                    }

                    // Barra de progreso principal
                    Rectangle {
                        Layout.fillWidth: true
                        height: 24
                        radius: 12
                        color: Theme.surfaceAlt

                        Rectangle {
                            height: parent.height
                            radius: parent.radius
                            width: {
                                const pct = Math.min(budgetInfo.usage_percent || 0, 100) / 100
                                return parent.width * pct
                            }
                            color: {
                                const pct = budgetInfo.usage_percent || 0
                                if (pct >= 90) return Theme.accentError
                                if (pct >= 75) return Theme.accentWarning
                                return Theme.accentLocal
                            }

                            Label {
                                anchors.centerIn: parent
                                text: Math.round(budgetInfo.usage_percent || 0) + "%"
                                color: "#FFFFFF"
                                font.bold: true
                                font.pixelSize: 12
                            }
                        }
                    }

                    // Estadísticas de presupuesto
                    GridLayout {
                        columns: 2
                        columnSpacing: 24
                        rowSpacing: 8
                        Layout.fillWidth: true

                        Label { text: "Pool total"; color: Theme.textSecondary }
                        Label {
                            text: formatNumber(budgetInfo.total_pool || 0)
                            font.bold: true
                            color: Theme.textPrimary
                        }

                        Label { text: "Consumido"; color: Theme.textSecondary }
                        Label {
                            text: formatNumber(budgetInfo.consumed || 0)
                            font.bold: true
                            color: budgetInfo.is_exceeded ? Theme.accentError : Theme.accentRemote
                        }

                        Label { text: "Restante"; color: Theme.textSecondary }
                        Label {
                            text: budgetInfo.remaining < 0 ? "∞ Sin límite" : formatNumber(budgetInfo.remaining)
                            font.bold: true
                            color: budgetInfo.remaining < 0 ? Theme.textMuted : Theme.accentLocal
                        }
                    }

                    // Control para ajustar presupuesto
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8

                        Label {
                            text: "Ajustar límite:"
                            color: Theme.textSecondary
                            font.pixelSize: 12
                        }

                        SpinBox {
                            id: budgetSpinBox
                            Layout.preferredWidth: 120
                            from: 0
                            to: 10000000
                            value: budgetInfo.total_pool || 0
                            stepSize: 10000
                            editable: true

                            textFromValue: function(value) {
                                return formatNumber(value)
                            }
                        }

                        Button {
                            text: "Aplicar"
                            enabled: budgetSpinBox.value !== (budgetInfo.total_pool || 0)
                            onClicked: {
                                if (controller) {
                                    controller.setTokenBudget(budgetSpinBox.value)
                                    loadUsageData()
                                }
                            }
                        }

                        Button {
                            text: "Sin límite"
                            onClicked: {
                                if (controller) {
                                    controller.setTokenBudget(0)
                                    loadUsageData()
                                }
                            }
                        }
                    }
                }
            }

            // ─── Resumen Global ───────────────────────────────────────
            Frame {
                Layout.fillWidth: true
                padding: 14

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 10

                    Label {
                        text: "Resumen Global"
                        font.bold: true
                        font.pixelSize: 16
                        color: Theme.textPrimary
                    }

                    GridLayout {
                        columns: 2
                        columnSpacing: 24
                        rowSpacing: 8
                        Layout.fillWidth: true

                        Label { text: "Total tokens prompt"; color: Theme.textSecondary }
                        Label {
                            text: formatNumber(globalUsage.total_prompt_tokens || 0)
                            font.bold: true
                            color: Theme.textPrimary
                        }

                        Label { text: "Total tokens respuesta"; color: Theme.textSecondary }
                        Label {
                            text: formatNumber(globalUsage.total_completion_tokens || 0)
                            font.bold: true
                            color: Theme.accentRemote
                        }

                        Label { text: "Total tokens consumidos"; color: Theme.textSecondary }
                        Label {
                            text: formatNumber(globalUsage.total_tokens || 0)
                            font.bold: true
                            color: Theme.accentLocal
                        }

                        Label { text: "Sesiones con actividad"; color: Theme.textSecondary }
                        Label {
                            text: (globalUsage.session_count || 0) + " sesiones"
                            font.bold: true
                            color: Theme.textPrimary
                        }
                    }
                }
            }

            // ─── Resumen Sesión Activa ────────────────────────────────
            Frame {
                Layout.fillWidth: true
                padding: 14
                visible: sessionUsage.total_tokens > 0

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 10

                    Label {
                        text: "Sesión Activa"
                        font.bold: true
                        font.pixelSize: 16
                        color: Theme.textPrimary
                    }

                    Label {
                        text: sessionUsage.session_id || "—"
                        color: Theme.textMuted
                        font.pixelSize: 11
                        font.family: "monospace"
                    }

                    GridLayout {
                        columns: 2
                        columnSpacing: 24
                        rowSpacing: 8
                        Layout.fillWidth: true

                        Label { text: "Tokens prompt"; color: Theme.textSecondary }
                        Label {
                            text: formatNumber(sessionUsage.total_prompt_tokens || 0)
                            font.bold: true
                            color: Theme.textPrimary
                        }

                        Label { text: "Tokens respuesta"; color: Theme.textSecondary }
                        Label {
                            text: formatNumber(sessionUsage.total_completion_tokens || 0)
                            font.bold: true
                            color: Theme.accentRemote
                        }

                        Label { text: "Total sesión"; color: Theme.textSecondary }
                        Label {
                            text: formatNumber(sessionUsage.total_tokens || 0)
                            font.bold: true
                            color: Theme.accentLocal
                        }
                    }
                }
            }

            // ─── Desglose por Modelo ─────────────────────────────────
            Frame {
                Layout.fillWidth: true
                padding: 14
                visible: (globalUsage.model_breakdown && globalUsage.model_breakdown.length > 0)

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 10

                    Label {
                        text: "Uso por Modelo"
                        font.bold: true
                        font.pixelSize: 16
                        color: Theme.textPrimary
                    }

                    Repeater {
                        model: globalUsage.model_breakdown || []
                        delegate: Frame {
                            Layout.fillWidth: true
                            padding: 10

                            ColumnLayout {
                                anchors.fill: parent
                                spacing: 6

                                RowLayout {
                                    Layout.fillWidth: true
                                    Label {
                                        text: modelData.model || "desconocido"
                                        font.bold: true
                                        color: Theme.textPrimary
                                        Layout.fillWidth: true
                                    }
                                    Label {
                                        text: formatNumber(modelData.total_tokens || 0) + " tokens"
                                        color: Theme.textSecondary
                                        font.pixelSize: 12
                                    }
                                }

                                // Barra de progreso visual
                                Rectangle {
                                    Layout.fillWidth: true
                                    height: 8
                                    radius: 4
                                    color: Theme.surfaceAlt

                                    Rectangle {
                                        height: parent.height
                                        radius: parent.radius
                                        color: Theme.accentLocal
                                        width: {
                                            if (!globalUsage.total_tokens || globalUsage.total_tokens === 0) return 0
                                            return parent.width * ((modelData.total_tokens || 0) / globalUsage.total_tokens)
                                        }
                                    }
                                }

                                Label {
                                    text: {
                                        const pct = globalUsage.total_tokens > 0
                                            ? Math.round(((modelData.total_tokens || 0) / globalUsage.total_tokens) * 100)
                                            : 0
                                        return pct + "% del total · " + (modelData.request_count || 0) + " requests"
                                    }
                                    color: Theme.textMuted
                                    font.pixelSize: 11
                                }
                            }
                        }
                    }
                }
            }

            // ─── Estado vacío ────────────────────────────────────────
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 8
                visible: !dataLoaded || (globalUsage.total_tokens || 0) === 0

                Item { Layout.preferredHeight: 40 }

                Label {
                    text: "☕"
                    font.pixelSize: 48
                    Layout.alignment: Qt.AlignHCenter
                }

                Label {
                    text: "Aún no hay datos de uso"
                    font.bold: true
                    font.pixelSize: 16
                    color: Theme.textPrimary
                    Layout.alignment: Qt.AlignHCenter
                }

                Label {
                    text: "Envía un prompt para empezar a ver estadísticas."
                    color: Theme.textSecondary
                    wrapMode: Text.Wrap
                    Layout.alignment: Qt.AlignHCenter
                }

                Item { Layout.preferredHeight: 40 }
            }
        }
    }

    // ─── Helpers ─────────────────────────────────────────────────────
    function formatNumber(n) {
        if (n >= 1000000) return (n / 1000000).toFixed(1) + "M"
        if (n >= 1000) return (n / 1000).toFixed(1) + "K"
        return String(n)
    }
}
