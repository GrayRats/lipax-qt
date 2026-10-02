import QtQuick
import org.kde.layershell 1.0 as LayerShell

// Outline of one capture region in the game. Every active region has its own instance,
// so the "on selection" timer and fade are independent per region. Does not take input.
Window {
    id: win
    property var settings: ({})
    property var region: null
    property string gameGeometry: ""
    property bool pinned: false
    // Flash when created (a region was just activated), but not for regions restored at start-up.
    property bool flashOnCreate: true

    readonly property string mode: (region && region.frame_mode) || settings.region_frame_mode || "selection"
    readonly property bool persistent: mode === "pattern" || mode === "solid"
    readonly property int borderWidth: Math.max(1, settings.frame_width || 2)
    readonly property color frameColor: /^#[0-9a-fA-F]{6}$/.test(settings.frame_color || "") ? settings.frame_color : "#ff0000"
    readonly property real seconds: Math.max(1, settings.frame_seconds || 3)
    readonly property bool flashing: flashTimer.running
    // Pinned translation: persistent outlines are dimmed or hidden. A fresh selection is always shown.
    readonly property real targetOpacity: mode === "off" ? 0
        : flashing ? 1
        : persistent ? (pinned ? (settings.region_frame_pinned === "hide" ? 0 : 0.35) : 1)
        : 0

    // Desktop rectangle: the normalized region inside the client area of the game window.
    readonly property var desktopRect: {
        try {
            const g = JSON.parse(gameGeometry), r = region && region.rect
            if (!r || !g || g[2] <= 0) return null
            return [g[0] + r.x * g[2], g[1] + r.y * g[3], r.w * g[2], r.h * g[3]]
        } catch (e) { return null }
    }
    readonly property var targetScreen: {
        if (!desktopRect) return null
        const cx = desktopRect[0] + desktopRect[2] / 2, cy = desktopRect[1] + desktopRect[3] / 2
        for (const s of Qt.application.screens)
            if (cx >= s.virtualX && cx < s.virtualX + s.width && cy >= s.virtualY && cy < s.virtualY + s.height) return s
        return Qt.application.screens.length ? Qt.application.screens[0] : null
    }
    onTargetScreenChanged: if (targetScreen && screen !== targetScreen) screen = targetScreen

    function flash() {
        if (mode === "off") return
        flashTimer.interval = seconds * 1000
        flashTimer.restart()
    }
    readonly property string rectKey: JSON.stringify(region ? region.rect : null)
    onRectKeyChanged: flash()
    Component.onCompleted: if (flashOnCreate) flash()
    Timer { id: flashTimer }
    onModeChanged: if (mode === "off") flashTimer.stop()

    visible: !!desktopRect && content.opacity > 0.01
    color: "transparent"
    flags: Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool | Qt.WindowTransparentForInput
    width: desktopRect ? Math.max(Math.round(desktopRect[2]), 2 * borderWidth) : 1
    height: desktopRect ? Math.max(Math.round(desktopRect[3]), 2 * borderWidth) : 1
    x: desktopRect ? Math.round(desktopRect[0]) : 0
    y: desktopRect ? Math.round(desktopRect[1]) : 0

    LayerShell.Window.scope: "lipa-region"
    LayerShell.Window.layer: LayerShell.Window.LayerOverlay
    LayerShell.Window.anchors: LayerShell.Window.AnchorTop | LayerShell.Window.AnchorLeft
    LayerShell.Window.margins: ({ left: desktopRect && targetScreen ? Math.round(desktopRect[0] - targetScreen.virtualX) : 0,
                                  top: desktopRect && targetScreen ? Math.round(desktopRect[1] - targetScreen.virtualY) : 0,
                                  right: 0, bottom: 0 })
    LayerShell.Window.exclusionZone: -1
    LayerShell.Window.keyboardInteractivity: LayerShell.Window.KeyboardInteractivityNone

    Item {
        id: content
        objectName: "regionFrameContent"
        anchors.fill: parent
        opacity: win.targetOpacity
        // "On selection" fades out smoothly instead of disappearing at once.
        Behavior on opacity { NumberAnimation { duration: 600; easing.type: Easing.OutQuad } }
        Rectangle {
            anchors.fill: parent
            visible: win.mode !== "pattern"
            color: "transparent"
            border.width: win.borderWidth
            border.color: win.frameColor
        }
        ErrorPattern {
            anchors.fill: parent
            visible: win.mode === "pattern"
            band: win.borderWidth
            color: "#ff00ff"
        }
    }
}
