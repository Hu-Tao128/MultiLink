import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Page {
    title: "Configuracion"

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        Label { text: "Proveedor por defecto" }
        ComboBox { model: ["Ollama", "Gemini", "Codex"] }

        Label { text: "Ruta de modelos" }
        TextField { placeholderText: "/var/lib/ollama/models" }

        Label { text: "OAuth" }
        RowLayout {
            Button { text: "Login Gemini" }
            Button { text: "Login Codex" }
            Button { text: "Logout" }
        }
    }
}
