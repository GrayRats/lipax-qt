import QtQuick
import org.kde.layershell 1.0 as LayerShell

// One tracked text field drawn over the original. Everything is decided in Rust (font chosen once
// per field, typography, Qt-metric fitting, background); this item only renders `entry`:
//   InplaceTranslation
//   ├── BackgroundItem — the replacement background (never chooses the font)
//   └── TextItem       — the translation (never reconstructs the background)
// Instances are reused by field key, so a stable field keeps its window between scans.
Window {
    id: win
    property var settings: ({capture: {}, recognition: {}, translation: {}, translation_window: {}, appearance: {window: {}, inplace: {}}})
    // Element of Controller.inplaceJson.
    property var entry: null
    property string gameGeometry: ""
    property bool relocating: false
    signal hideRequested()

    // Desktop rectangle: game window → region → field box (box is relative to the region frame).
    readonly property var desktopRect: {
        try {
            const g = JSON.parse(gameGeometry), r = entry && entry.rect, b = entry && entry.box
            if (!g || !r || !b || g[2] <= 0) return null
            const rx = g[0] + r.x * g[2], ry = g[1] + r.y * g[3], rw = r.w * g[2], rh = r.h * g[3]
            return [rx + b[0] * rw, ry + b[1] * rh, b[2] * rw, b[3] * rh]
        } catch (e) { return null }
    }
    readonly property var targetScreen: {
        if (!desktopRect) return null
        const cx = desktopRect[0] + desktopRect[2] / 2, cy = desktopRect[1] + desktopRect[3] / 2
        for (const s of Qt.application.screens)
            if (cx >= s.virtualX && cx < s.virtualX + s.width && cy >= s.virtualY && cy < s.virtualY + s.height) return s
        return Qt.application.screens.length ? Qt.application.screens[0] : null
    }
    onTargetScreenChanged: if (targetScreen && screen !== targetScreen) {
        relocating = true
        screen = targetScreen
        Qt.callLater(function() { win.relocating = false })
    }

    color: "transparent"
    flags: Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool | Qt.WindowDoesNotAcceptFocus
    width: desktopRect ? Math.max(8, Math.round(desktopRect[2])) : 8
    height: desktopRect ? Math.max(8, Math.round(desktopRect[3])) : 8
    x: desktopRect ? Math.round(desktopRect[0]) : 0
    y: desktopRect ? Math.round(desktopRect[1]) : 0

    LayerShell.Window.scope: "lipa-inplace"
    LayerShell.Window.layer: LayerShell.Window.LayerOverlay
    LayerShell.Window.anchors: LayerShell.Window.AnchorTop | LayerShell.Window.AnchorLeft
    LayerShell.Window.margins: ({ left: desktopRect && targetScreen ? Math.round(desktopRect[0] - targetScreen.virtualX) : 0,
                                  top: desktopRect && targetScreen ? Math.round(desktopRect[1] - targetScreen.virtualY) : 0,
                                  right: 0, bottom: 0 })
    LayerShell.Window.exclusionZone: -1
    LayerShell.Window.keyboardInteractivity: LayerShell.Window.KeyboardInteractivityNone

    InplaceTranslationContent { anchors.fill: parent; entry: win.entry }

    MouseArea {
        objectName: "inplaceMouse"
        anchors.fill: parent
        acceptedButtons: Qt.MiddleButton
        onPressed: (mouse) => { if (mouse.button === Qt.MiddleButton) win.hideRequested() }
    }
}
