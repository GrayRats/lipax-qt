import QtQuick
import "InplaceGeometry.js" as Geometry
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
            return Geometry.desktopRect(JSON.parse(gameGeometry), entry)
        } catch (e) { return null }
    }
    // The screen Rust kept the field to (it sends a point inside it), not the one under the middle of the plate.
    readonly property var targetScreen: {
        if (!desktopRect) return null
        const p = entry && entry.screen_point
        const cx = p ? p[0] : desktopRect[0] + desktopRect[2] / 2, cy = p ? p[1] : desktopRect[1] + desktopRect[3] / 2
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
    // Edges are rounded, not the size: rounding x and the width separately could grow the window by a
    // pixel past the edge Rust kept it to. Rust fits the text with a pixel to spare for this rounding.
    readonly property var pixelRect: desktopRect ? [Math.round(desktopRect[0]), Math.round(desktopRect[1]),
        Math.round(desktopRect[0] + desktopRect[2]), Math.round(desktopRect[1] + desktopRect[3])] : null
    width: pixelRect ? Math.max(1, pixelRect[2] - pixelRect[0]) : 8
    height: pixelRect ? Math.max(1, pixelRect[3] - pixelRect[1]) : 8
    x: pixelRect ? pixelRect[0] : 0
    y: pixelRect ? pixelRect[1] : 0

    LayerShell.Window.scope: "lipa-inplace"
    LayerShell.Window.layer: LayerShell.Window.LayerOverlay
    LayerShell.Window.anchors: LayerShell.Window.AnchorTop | LayerShell.Window.AnchorLeft
    LayerShell.Window.margins: ({ left: pixelRect && targetScreen ? pixelRect[0] - targetScreen.virtualX : 0,
                                  top: pixelRect && targetScreen ? pixelRect[1] - targetScreen.virtualY : 0,
                                  right: 0, bottom: 0 })
    LayerShell.Window.exclusionZone: -1
    LayerShell.Window.keyboardInteractivity: LayerShell.Window.KeyboardInteractivityNone

    InplaceTranslationContent { anchors.fill: parent; entry: win.entry; visible: win.desktopRect !== null }

    MouseArea {
        objectName: "inplaceMouse"
        anchors.fill: parent
        acceptedButtons: Qt.MiddleButton
        onPressed: (mouse) => { if (mouse.button === Qt.MiddleButton) win.hideRequested() }
    }
}
