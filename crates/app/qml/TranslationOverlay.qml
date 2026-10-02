import QtQuick
import org.kde.layershell 1.0 as LayerShell

// Overlay с переводом. Поверх всех окон на Wayland его держит wlr-layer-shell (слой Overlay,
// библиотека layer-shell-qt). Положение задаётся отступами от левого верхнего угла экрана:
// у layer-поверхности свойства x/y не действуют. Захвата клавиатуры нет.
Window {
    id: win
    property string translation: ""
    property var settings: ({})
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
    LayerShell.Window.margins: ({ left: win.posX, top: win.posY, right: 0, bottom: 0 })
    LayerShell.Window.exclusionZone: -1
    LayerShell.Window.keyboardInteractivity: LayerShell.Window.KeyboardInteractivityNone

    width: settings.overlay_size ? settings.overlay_size[0] : 700
    height: settings.overlay_size ? settings.overlay_size[1] : 120

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
