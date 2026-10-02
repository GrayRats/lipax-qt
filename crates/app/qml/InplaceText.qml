import QtQuick
import org.kde.layershell 1.0 as LayerShell

// Translation drawn over the original text of one region: a blurred fill made from the
// captured frame hides the original, and the translation takes its colour, size and weight,
// shrinking until it fits the block. MMB hides the translation without focusing the game away.
Window {
    id: win
    property var settings: ({})
    // Entry of Controller.inplaceJson: rect (region in the window), bbox (text block in the
    // region frame), frame size, font_px, bold, text_color, background_color, backdrop, text.
    property var entry: null
    property string gameGeometry: ""
    property bool relocating: false
    signal hideRequested()

    // Desktop rectangle of the text block: window → region → block.
    readonly property var desktopRect: {
        try {
            const g = JSON.parse(gameGeometry), r = entry && entry.rect, b = entry && entry.bbox
            if (!g || !r || !b || g[2] <= 0) return null
            const rx = g[0] + r.x * g[2], ry = g[1] + r.y * g[3], rw = r.w * g[2], rh = r.h * g[3]
            return [rx + b[0] * rw, ry + b[1] * rh, b[2] * rw, b[3] * rh]
        } catch (e) { return null }
    }
    // Screen pixels per pixel of the captured frame (capture is in native resolution).
    readonly property real scale: desktopRect && entry.frame && entry.frame[0] > 0
        ? desktopRect[2] / (entry.bbox[2] * entry.frame[0]) : 1
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

    // A few pixels of the frame stretched with smoothing: a blur of the scene around the text.
    Image {
        objectName: "inplaceBackdrop"
        anchors.fill: parent
        source: win.entry ? win.entry.backdrop : ""
        fillMode: Image.Stretch
        smooth: true
        cache: false
    }
    // Background colour around the block hides what the blur leaves of the original.
    Rectangle {
        objectName: "inplaceFill"
        anchors.fill: parent
        radius: 0
        color: win.entry ? win.entry.background_color : "#000000"
        opacity: 0.72
    }
    Text {
        objectName: "inplaceText"
        anchors.fill: parent
        anchors.margins: 2
        text: win.entry ? win.entry.text : ""
        textFormat: Text.PlainText
        color: win.entry ? win.entry.text_color : "#ffffff"
        font.family: win.settings.font_family || (win.entry && win.entry.font_family) || Qt.application.font.family
        font.bold: !!(win.entry && win.entry.bold)
        // Start from the original size; shrink until the translation fits the block.
        font.pixelSize: Math.max(8, Math.round((win.entry ? win.entry.font_px : 20) * win.scale))
        fontSizeMode: Text.Fit
        minimumPixelSize: 7
        wrapMode: Text.Wrap
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }
    MouseArea {
        objectName: "inplaceMouse"
        anchors.fill: parent
        acceptedButtons: Qt.MiddleButton
        onPressed: (mouse) => { if (mouse.button === Qt.MiddleButton) win.hideRequested() }
    }
}
