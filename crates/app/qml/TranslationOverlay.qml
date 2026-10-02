import QtQuick
import org.kde.layershell 1.0 as LayerShell

// Overlay с переводом. Поверх всех окон на Wayland его держит wlr-layer-shell (слой Overlay,
// библиотека layer-shell-qt). Положение задаётся отступами от левого верхнего угла экрана:
// у layer-поверхности свойства x/y не действуют. Захвата клавиатуры нет.
Window {
    id: win
    property string translation: ""
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
    onSettingsChanged: updateScreen()
    Component.onCompleted: updateScreen()
    Connections { target: Qt.application; function onScreensChanged() { win.updateScreen() } }

    readonly property bool clickThrough: settings.click_through !== false
    // Текущее положение; во время перетаскивания меняется локально, на отпускании уходит в `moved`.
    property int posX: settings.overlay_pos ? settings.overlay_pos[0] : 100
    property int posY: settings.overlay_pos ? settings.overlay_pos[1] : 100
    signal moved(int x, int y)

    flags: Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool
           | (clickThrough ? Qt.WindowTransparentForInput : 0)
    color: "transparent"

    LayerShell.Window.scope: "lipa-overlay"
    LayerShell.Window.layer: LayerShell.Window.LayerOverlay
    LayerShell.Window.anchors: LayerShell.Window.AnchorTop | LayerShell.Window.AnchorLeft
    LayerShell.Window.margins: ({ left: Math.max(0, Math.min(win.posX, (win.targetScreen ? win.targetScreen.width : 1920) - win.width)), top: Math.max(0, Math.min(win.posY, (win.targetScreen ? win.targetScreen.height : 1080) - win.height)), right: 0, bottom: 0 })
    LayerShell.Window.exclusionZone: -1
    LayerShell.Window.keyboardInteractivity: LayerShell.Window.KeyboardInteractivityNone

    width: Math.min(settings.overlay_size ? settings.overlay_size[0] : 700, targetScreen ? targetScreen.width : 1920)
    height: Math.min(settings.overlay_size ? settings.overlay_size[1] : 120, targetScreen ? targetScreen.height : 1080)

    Rectangle {
        anchors.fill: parent
        radius: 10
        color: "#181818"
        opacity: settings.opacity !== undefined ? settings.opacity : 0.85
        border.width: 1
        border.color: "#40ffffff"
    }
    Text {
        anchors.fill: parent
        anchors.margins: 12
        text: win.translation
        color: "white"
        wrapMode: Text.Wrap
        verticalAlignment: Text.AlignVCenter
        horizontalAlignment: Text.AlignHCenter
        font.pixelSize: settings.font_size || 20
        fontSizeMode: Text.Fit
        minimumPixelSize: 10
    }

    // Когда click-through выключен, окно можно перетаскивать. startSystemMove для layer-поверхности
    // недоступен, поэтому сдвигаем отступы вручную.
    MouseArea {
        anchors.fill: parent
        enabled: !win.clickThrough
        cursorShape: Qt.SizeAllCursor
        property real px: 0
        property real py: 0
        onPressed: (m) => { px = m.x; py = m.y }
        onPositionChanged: (m) => {
            if (!pressed) return
            win.posX = Math.max(0, win.posX + Math.round(m.x - px))
            win.posY = Math.max(0, win.posY + Math.round(m.y - py))
        }
        onReleased: win.moved(win.posX, win.posY)
    }
}
