pragma Singleton
import QtQuick

QtObject {
    id: theme

    // false = light, true = dark — se setea desde el bridge al iniciar
    property bool isDark: false

    // ─── Colores semánticos ─────────────────────────────────────────
    readonly property color background:      isDark ? "#1A1A2E" : "#F5F6F7"
    readonly property color surface:         isDark ? "#16213E" : "#FFFFFF"
    readonly property color surfaceAlt:      isDark ? "#0F3460" : "#F0F0F0"
    readonly property color border:          isDark ? "#2A2A4A" : "#E0E0E0"
    readonly property color textPrimary:     isDark ? "#EAEAEA" : "#1E1E1E"
    readonly property color textSecondary:   isDark ? "#9E9E9E" : "#6B6B6B"
    readonly property color textMuted:       isDark ? "#666688" : "#999999"

    // Acentos por contexto
    readonly property color accentLocal:     isDark ? "#4CAF50" : "#2E7D32"
    readonly property color accentRemote:    isDark ? "#64B5F6" : "#1565C0"
    readonly property color accentWarning:   isDark ? "#FFB74D" : "#ED6C02"
    readonly property color accentError:     isDark ? "#EF5350" : "#C62828"

    // Burbujas de chat
    readonly property color bubbleUser:      isDark ? "#1B5E20" : "#E8F5E9"
    readonly property color bubbleAssistant: isDark ? "#16213E" : "#FFFFFF"
    readonly property color bubbleThinking:  isDark ? "#37474F" : "#ECEFF1"
    readonly property color bubbleThinkingText: isDark ? "#90A4AE" : "#455A64"

    // Bloques de código (igual en ambos temas)
    readonly property color codeBackground:  "#1E2A32"
    readonly property color codeBorder:      "#3D4F5F"
    readonly property color codeText:        "#D8DEE9"
    readonly property color codeLanguage:    "#61AFEF"

    // Inputs y controles
    readonly property color inputBackground: isDark ? "#0F3460" : "#FAFAFA"
    readonly property color hoverOverlay:    isDark ? "#4A5A6A" : "#E0E0E0"

    // Status indicator helper
    function statusColor(health) {
        if (health === "available") return accentLocal
        if (health === "starting") return accentWarning
        return accentError
    }
}
