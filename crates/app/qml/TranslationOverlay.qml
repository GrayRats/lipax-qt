import QtQuick
import org.kde.layershell 1.0 as LayerShell

// Overlay с переводом. Поверх всех окон на Wayland его держит wlr-layer-shell (слой Overlay,
// библиотека layer-shell-qt). Положение задаётся отступами от левого верхнего угла экрана:
// у layer-поверхности свойства x/y не действуют. Захвата клавиатуры нет.
Window {
    id: win
    property string translation: ""
    property string original: ""
    property var settings: ({})
    property string gameGeometry: ""
    property var targetScreen: null
    property bool relocating: false

    function updateScreen() {
        const screens = Qt.application.screens
        let chosen = screens.find(s => s.name === settings.overlay_screen)
        if (!chosen && gameGeometry.length) {
            try {
                const g = JSON.parse(gameGeometry)
                // Largest intersection also works for games spanning monitors.
                let best = 0
                for (const s of screens) {
                    const area = Math.max(0, Math.min(g[0] + g[2], s.virtualX + s.width) - Math.max(g[0], s.virtualX))
                               * Math.max(0, Math.min(g[1] + g[3], s.virtualY + s.height) - Math.max(g[1], s.virtualY))
                    if (area > best) { best = area; chosen = s }
                }
            } catch (e) {}
        }
        if (!chosen) chosen = screens.length ? screens[0] : null
        if (chosen && targetScreen !== chosen) {
            // A layer surface is tied to an output at creation. Remap it on monitor changes.
            relocating = true
            targetScreen = chosen
            win.screen = chosen
            Qt.callLater(function() { win.relocating = false })
        }
    }
    onGameGeometryChanged: updateScreen()
    onSettingsChanged: {
        updateScreen()
        if (!dragArea.moving) {
            posX = settings.overlay_pos ? settings.overlay_pos[0] : 100
            posY = settings.overlay_pos ? settings.overlay_pos[1] : 100
        }
        Qt.callLater(updateInput)
    }
    Component.onCompleted: updateScreen()
    Connections { target: Qt.application; function onScreensChanged() { win.updateScreen() } }

    property var controller
    readonly property bool pinned: settings.overlay_pinned === true
    readonly property bool clickThrough: settings.click_through !== false
    // An unpinned overlay always shows its frame so it is clear that it can be dragged.
    readonly property bool frameVisible: !pinned || settings.border_always !== false || frameTimer.running
    function flashFrame() { frameTimer.interval = Math.max(1, settings.border_seconds || 5) * 1000; frameTimer.restart() }
    Timer { id: frameTimer }
    readonly property int frameWidth: Math.max(1, settings.border_width || 2) + (pinned ? 0 : 2)
    readonly property int padding: (settings.overlay_padding !== undefined ? settings.overlay_padding : 16) + frameWidth
    property int posX: settings.overlay_pos ? settings.overlay_pos[0] : 100
    property int posY: settings.overlay_pos ? settings.overlay_pos[1] : 100
    // Size of the screen the overlay is on; before it is chosen, the screen Qt placed the window on.
    readonly property int screenWidth: targetScreen ? targetScreen.width : Screen.width
    readonly property int screenHeight: targetScreen ? targetScreen.height : Screen.height
    readonly property int actualX: Math.max(0, Math.min(posX, screenWidth - width))
    readonly property int actualY: Math.max(0, Math.min(posY, screenHeight - height))
    signal moved(int x, int y)
    signal pinToggled(bool pinned)
    objectName: "translationOverlay"

    // The pin handle is always shown and always accepts input, whatever the frame mode: MMB on it
    // unpins even when the frame is hidden. The frame band accepts input only while it is visible.
    readonly property int handleSize: 26
    readonly property var handleRect: [width - handleSize - 2, 2, handleSize, handleSize]
    readonly property bool passthrough: pinned && clickThrough
    function inputRects() {
        const rects = [handleRect]
        if (frameVisible) {
            const band = Math.max(frameWidth, 10)
            rects.push([0, 0, width, band], [0, height - band, width, band],
                       [0, band, band, height - 2 * band], [width - band, band, band, height - 2 * band])
        }
        return rects
    }
    function updateInput() {
        if (controller) controller.configureOverlay(passthrough, JSON.stringify(inputRects()))
    }
    onFrameVisibleChanged: Qt.callLater(updateInput)
    // Show the frame briefly on every pin change so the new state is visible in any frame mode.
    onPinnedChanged: { flashFrame(); Qt.callLater(updateInput) }
    onClickThroughChanged: Qt.callLater(updateInput)
    onWidthChanged: Qt.callLater(updateInput)
    onHeightChanged: Qt.callLater(updateInput)
    onVisibleChanged: if (visible) Qt.callLater(updateInput)

    flags: Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool | Qt.WindowDoesNotAcceptFocus
    color: "transparent"
    // On an X11 Qt backend these coordinates accompany the WM keep-above hint.
    x: (targetScreen ? targetScreen.virtualX : 0) + actualX
    y: (targetScreen ? targetScreen.virtualY : 0) + actualY

    LayerShell.Window.scope: "lipa-overlay"
    LayerShell.Window.layer: LayerShell.Window.LayerOverlay
    LayerShell.Window.anchors: LayerShell.Window.AnchorTop | LayerShell.Window.AnchorLeft
    LayerShell.Window.margins: ({ left: win.actualX, top: win.actualY, right: 0, bottom: 0 })
    LayerShell.Window.exclusionZone: -1
    LayerShell.Window.keyboardInteractivity: LayerShell.Window.KeyboardInteractivityNone

    width: Math.min(settings.overlay_size ? settings.overlay_size[0] : 700,
                    settings.max_width_enabled !== false ? (settings.overlay_max_width || 900) : 100000,
                    screenWidth)
    height: Math.min(settings.overlay_size ? settings.overlay_size[1] : 120, screenHeight)

    Rectangle {
        anchors.fill: parent
        color: win.settings.background_color || "#181818"
        opacity: win.settings.opacity !== undefined ? win.settings.opacity : 0.85
    }
    Rectangle {
        anchors.fill: parent
        color: "transparent"
        visible: win.frameVisible && win.settings.border_pattern !== true
        border.width: win.frameWidth
        border.color: win.settings.border_color || "#ff00ff"
        opacity: win.settings.border_opacity !== undefined ? win.settings.border_opacity : 0.65
    }
    ErrorPattern {
        anchors.fill: parent
        visible: win.frameVisible && win.settings.border_pattern === true
        band: win.frameWidth
        color: win.settings.border_color || "#ff00ff"
        opacity: win.settings.border_opacity !== undefined ? win.settings.border_opacity : 0.65
    }
    Flickable {
        id: textScroll
        anchors.fill: parent
        anchors.margins: win.padding
        clip: true
        contentWidth: width
        contentHeight: Math.max(height, textColumn.implicitHeight)
        interactive: false
        Column {
            id: textColumn
            width: parent.width
            y: Math.max(0, (textScroll.height - implicitHeight) / 2)
            spacing: 4
            Text {
                id: originalText
                width: parent.width
                visible: win.settings.show_original === true && text.length > 0
                text: win.original
                textFormat: Text.PlainText
                color: win.settings.original_color || "#b0b0b0"
                wrapMode: translatedText.wrapMode
                elide: translatedText.elide
                maximumLineCount: win.settings.text_wrap === false ? 1 : 1000
                horizontalAlignment: translatedText.horizontalAlignment
                font.family: win.settings.original_font_family || translatedText.font.family
                font.pixelSize: win.settings.original_font_size || 14
                style: translatedText.style
                styleColor: translatedText.styleColor
            }
            Text {
                id: translatedText
                width: parent.width
                text: win.translation
                textFormat: Text.PlainText
                color: win.settings.text_color || "#ffffff"
                wrapMode: win.settings.text_wrap !== false ? Text.Wrap : Text.NoWrap
                elide: win.settings.text_wrap === false ? Text.ElideRight : Text.ElideNone
                lineHeight: win.settings.line_spacing || 1.0
                horizontalAlignment: win.settings.text_alignment === "left" ? Text.AlignLeft : win.settings.text_alignment === "right" ? Text.AlignRight : Text.AlignHCenter
                font.family: win.settings.font_family || Qt.application.font.family
                font.pixelSize: win.settings.font_size || 20
                font.bold: win.settings.font_bold === true
                font.italic: win.settings.font_italic === true
                style: win.settings.text_outline !== false ? Text.Outline : Text.Normal
                styleColor: win.settings.outline_color || "#000000"
            }
        }
    }
    // Pin handle: stays visible in every frame mode, so there is always a target for MMB.
    Rectangle {
        id: pinHandle
        objectName: "pinHandle"
        x: win.handleRect[0]; y: win.handleRect[1]
        width: win.handleSize; height: win.handleSize; radius: width / 2
        color: "#80000000"
        border.width: 1
        border.color: win.settings.border_color || "#ff00ff"
        opacity: win.pinned && !win.frameVisible ? 0.55 : 1
        Text {
            anchors.centerIn: parent
            text: win.pinned ? "●" : "✥"
            color: win.settings.border_color || "#ff00ff"
            font.pixelSize: 14
        }
    }
    Text {
        anchors.bottom: parent.bottom; anchors.horizontalCenter: parent.horizontalCenter
        text: "⌄"; color: win.settings.text_color || "white"
        visible: textScroll.contentHeight > textScroll.height + 1
    }
    MouseArea {
        id: dragArea
        objectName: "overlayDragArea"
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton
        cursorShape: win.pinned ? Qt.ArrowCursor : Qt.SizeAllCursor
        property real originX: 0
        property real originY: 0
        property real startX: 0
        property real startY: 0
        property bool moving: false
        onPressed: (m) => {
            if (m.button === Qt.MiddleButton) { moving = false; win.pinToggled(!win.pinned); return }
            if (win.pinned) return
            const global = mapToGlobal(m.x, m.y)
            originX = global.x; originY = global.y
            startX = win.actualX; startY = win.actualY
            moving = true
        }
        onPositionChanged: (m) => {
            if (!moving || win.pinned) return
            const global = mapToGlobal(m.x, m.y)
            win.posX = Math.max(0, Math.min(startX + global.x - originX, win.screenWidth - win.width))
            win.posY = Math.max(0, Math.min(startY + global.y - originY, win.screenHeight - win.height))
        }
        onReleased: (m) => {
            if (moving) { moving = false; win.moved(win.actualX, win.actualY) }
        }
        onCanceled: { if (moving) { moving = false; win.moved(win.actualX, win.actualY) } }
        onWheel: (wheel) => {
            if (!win.pinned) textScroll.contentY = Math.max(0, Math.min(textScroll.contentY - wheel.angleDelta.y, textScroll.contentHeight - textScroll.height))
        }
    }
}
