import QtQuick
import QtQml

// Floating (unpinned) translation window: an ordinary top-level window (xdg_toplevel on Wayland),
// deliberately without any LayerShell properties — layer-shell anchoring must not control a free
// window. LMB starts a native compositor move (startSystemMove); the compositor owns the
// movement, including its own edge snapping/tiling if the user enabled it in KWin.
//
// Position: on Wayland an application can neither read nor set where its top-level window is.
// The KWin script of LipaX restores the saved place, keeps this window above others and reports
// the result after a move (it finds the window by PID and this title). On X11 Qt knows the
// position, so it is restored and saved here. Nothing in this file clamps, re-anchors or moves
// the window after the user releases it.
Window {
    id: win
    objectName: "translationWindow"
    // Matched exactly by the KWin script (capture/geometry.js); keep both in sync.
    title: "LipaX · окно перевода"
    property var controller
    property var settings: ({capture: {}, recognition: {}, translation: {}, translation_window: {}, appearance: {window: {}, inplace: {}}})
    property string translation: ""
    property string original: ""
    signal pinToggleRequested()
    signal closeRequested()
    readonly property alias content: content
    readonly property bool x11: Qt.platform.pluginName === "xcb"

    LoggingCategory { id: logState; name: "translation_window.state"; defaultLogLevel: LoggingCategory.Debug }
    LoggingCategory { id: logDrag; name: "translation_window.drag"; defaultLogLevel: LoggingCategory.Debug }
    LoggingCategory { id: logGeometry; name: "translation_window.geometry"; defaultLogLevel: LoggingCategory.Debug }

    // Independent top-level: not transient for the main window.
    transientParent: null
    // A normal xdg_toplevel is listed by Plasma's task manager. The application's
    // desktopFileName (io.lipa.Translator) supplies its Wayland app_id.
    flags: Qt.Window | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint
    color: "transparent"
    visible: false
    onClosing: closeRequested()
    width: Math.min(settings.translation_window.size ? settings.translation_window.size[0] : 700,
                    settings.translation_window.max_width_enabled !== false ? (settings.translation_window.maximum_width || 900) : 100000)
    height: settings.translation_window.size ? settings.translation_window.size[1] : 120

    Component.onCompleted: {
        const g = settings.translation_window.floating_geometry
        if (x11 && g) {
            // X11: Qt may position top-level windows; restore once, before showing.
            x = Math.round(g.x); y = Math.round(g.y)
            console.debug(logGeometry, "position changed by restore_geometry x=" + x + " y=" + y)
        }
        console.debug(logState, "pinned=false floating=true window_role=" + (x11 ? "x11_toplevel" : "xdg_toplevel") + " layershell=false")
    }
    onVisibleChanged: if (visible) {
        Qt.callLater(updateInput)
        Qt.callLater(updateBlur)
    }

    // X11 only: save where the window ended up, after it stops moving (not during the drag).
    Timer {
        id: saveTimer
        interval: 400
        onTriggered: {
            const s = win.screen
            const g = { x: win.x, y: win.y, w: win.width, h: win.height, output: s ? s.name : "",
                        output_x: s ? s.virtualX : 0, output_y: s ? s.virtualY : 0 }
            console.debug(logGeometry, "move_finished x=" + g.x + " y=" + g.y + " width=" + g.w + " height=" + g.h + " screen=" + g.output)
            if (win.controller && win.controller.reportFloatingGeometry) win.controller.reportFloatingGeometry(JSON.stringify(g), "move_finished")
        }
    }
    onXChanged: if (x11 && visible) saveTimer.restart()
    onYChanged: if (x11 && visible) saveTimer.restart()

    // Floating windows capture input: no click-through mask.
    function updateInput() {
        if (!visible) return
        if (controller) controller.configureTranslationWindow(false, "[]")
    }
    function updateBlur() {
        if (!visible) return
        if (controller && controller.configureTranslationWindowBlur) controller.configureTranslationWindowBlur(content.blurBehind, content.cornerRadius)
    }
    onWidthChanged: Qt.callLater(updateBlur)
    onHeightChanged: Qt.callLater(updateBlur)

    TranslationWindowContent {
        id: content
        anchors.fill: parent
        settings: win.settings
        translation: win.translation
        original: win.original
        pinned: false
        onBlurBehindChanged: Qt.callLater(win.updateBlur)
        onCornerRadiusChanged: Qt.callLater(win.updateBlur)
        onPinToggleRequested: win.pinToggleRequested()
        onCloseRequested: win.closeRequested()
        onMoveRequested: {
            console.debug(logDrag, "system_move_started")
            if (!win.startSystemMove()) console.warn(logDrag, "startSystemMove is not supported by this platform")
        }
    }
}
