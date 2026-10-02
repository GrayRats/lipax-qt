import QtQuick
import org.kde.layershell 1.0 as LayerShell

// One tracked text field drawn over the original. Everything is decided in Rust (font chosen once
// per field, typography, Qt-metric fitting, background); this item only renders `entry`:
//   InplaceText
//   ├── BackgroundItem — the replacement background (never chooses the font)
//   └── TextItem       — the translation (never reconstructs the background)
// Instances are reused by field key, so a stable field keeps its window between scans.
Window {
    id: win
    property var settings: ({})
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

    readonly property var bg: entry ? entry.background : ({ mode: "transparent", color: "#000000", image: "" })

    Item {
        id: backgroundItem
        objectName: "inplaceBackground"
        anchors.fill: parent
        visible: win.bg.mode !== "transparent"
        // Inpaint + blur: a small reconstructed image stretched with smoothing.
        Image {
            objectName: "inplaceBackdrop"
            anchors.fill: parent
            visible: win.bg.mode === "inpaint_blur" && source.toString().length > 0
            source: win.bg.image || ""
            fillMode: Image.Stretch
            smooth: true
            cache: false
        }
        // Solid / adaptive padding fill: the colour sampled around the original glyphs.
        Rectangle {
            objectName: "inplaceFill"
            anchors.fill: parent
            visible: win.bg.mode === "solid_fill" || win.bg.mode === "adaptive_padding_fill"
            radius: Math.min(4, height / 6)
            color: win.bg.color
        }
    }

    Text {
        id: textItem
        objectName: "inplaceText"
        readonly property var inner: win.entry ? win.entry.inner : [0, 0, 0, 0]
        x: inner[0]
        y: inner[1]
        width: Math.max(1, parent.width - inner[0] - inner[2])
        height: Math.max(1, parent.height - inner[1] - inner[3])
        text: win.entry ? win.entry.text : ""
        textFormat: Text.PlainText
        color: win.entry ? win.entry.text_color : "#ffffff"
        font.family: win.entry ? win.entry.font_family : Qt.application.font.family
        font.pixelSize: win.entry ? win.entry.font_px : 16
        font.weight: win.entry ? win.entry.font_weight : Font.Normal
        font.italic: !!(win.entry && win.entry.italic)
        font.letterSpacing: win.entry ? win.entry.letter_spacing : 0
        lineHeightMode: Text.ProportionalHeight
        lineHeight: win.entry ? win.entry.line_height : 1.0
        wrapMode: !win.entry ? Text.Wrap
            : win.entry.wrap === "anywhere" ? Text.WrapAnywhere
            : win.entry.wrap === "none" ? Text.NoWrap : Text.Wrap
        // Explicit overflow fallback: never drawn outside the field.
        elide: win.entry && win.entry.wrap === "elide" ? Text.ElideRight : Text.ElideNone
        maximumLineCount: win.entry && win.entry.wrap === "elide" ? win.entry.max_lines : 10000
        clip: true
        horizontalAlignment: !win.entry ? Text.AlignHCenter
            : win.entry.alignment === "left" ? Text.AlignLeft
            : win.entry.alignment === "right" ? Text.AlignRight : Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        style: win.entry && win.entry.outline ? Text.Outline : Text.Normal
        styleColor: win.entry ? win.entry.outline_color : "#000000"
    }

    MouseArea {
        objectName: "inplaceMouse"
        anchors.fill: parent
        acceptedButtons: Qt.MiddleButton
        onPressed: (mouse) => { if (mouse.button === Qt.MiddleButton) win.hideRequested() }
    }
}
