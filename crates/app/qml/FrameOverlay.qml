import QtQuick
import org.kde.layershell 1.0 as LayerShell

// Тонкая прозрачная рамка вокруг выбранного окна или области. Это layer-shell поверхность (слой Overlay),
// без заливки и без перехвата мыши: внутри рамки всё видно и кликается как раньше.
// Цвет, толщина и время показа берутся из настроек.
Window {
    id: win
    property var settings: ({capture: {}, recognition: {}, translation: {}, translation_window: {}, appearance: {window: {}, inplace: {}}})
    readonly property int borderWidth: Math.max(1, settings.capture.frame_width || 2)
    readonly property color borderColor: /^#[0-9a-fA-F]{6}$/.test(settings.capture.frame_color || "") ? settings.capture.frame_color : "#ff0000"
    property real gx: 0   // положение на рабочем столе
    property real gy: 0

    // Экран, на котором находится центр рамки; отступы layer-shell считаются от его угла.
    function screenFor(cx, cy) {
        for (const s of Qt.application.screens)
            if (cx >= s.virtualX && cx < s.virtualX + s.width && cy >= s.virtualY && cy < s.virtualY + s.height) return s
        return Qt.application.screens.length ? Qt.application.screens[0] : null
    }

    function flash(x, y, w, h) {
        const seconds = settings.capture.frame_seconds || 3
        if (settings.capture.region_frame_mode === "off" || w < 1 || h < 1) return
        const scr = screenFor(x + w / 2, y + h / 2)
        if (scr && win.screen !== scr) {
            win.visible = false // Recreate the layer surface on the new output.
            win.screen = scr
        }
        gx = x - (scr ? scr.virtualX : 0)
        gy = y - (scr ? scr.virtualY : 0)
        width = Math.max(Math.round(w), 2 * borderWidth)
        height = Math.max(Math.round(h), 2 * borderWidth)
        visible = true
        hideTimer.interval = seconds * 1000
        hideTimer.restart()
    }

    visible: false
    color: "transparent"
    flags: Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool | Qt.WindowTransparentForInput

    LayerShell.Window.scope: "lipa-frame"
    LayerShell.Window.layer: LayerShell.Window.LayerOverlay
    LayerShell.Window.anchors: LayerShell.Window.AnchorTop | LayerShell.Window.AnchorLeft
    LayerShell.Window.margins: ({ left: Math.round(win.gx), top: Math.round(win.gy), right: 0, bottom: 0 })
    LayerShell.Window.exclusionZone: -1
    LayerShell.Window.keyboardInteractivity: LayerShell.Window.KeyboardInteractivityNone

    Rectangle {
        anchors.fill: parent
        color: "transparent"
        border.width: win.borderWidth
        border.color: win.borderColor
    }

    Timer { id: hideTimer; onTriggered: win.visible = false }
}
