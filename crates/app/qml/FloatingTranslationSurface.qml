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
    // A native resize changes QWindow's size. Rebinding it to the whole settings object
    // resets that size whenever a move is saved or a translation appearance is edited.
    // Initialise once; only an explicit size/maximum-width edit changes it later.
    width: 700
    height: 120
    property bool geometryReady: false
    readonly property string requestedSize: JSON.stringify([
        settings.translation_window.size || [700, 120],
        settings.translation_window.max_width_enabled !== false,
        settings.translation_window.maximum_width || 900])
    function applySize(saved) {
        const size = settings.translation_window.size || [700, 120]
        width = Math.min(saved ? saved.w : size[0],
                         settings.translation_window.max_width_enabled !== false ? (settings.translation_window.maximum_width || 900) : 100000)
        height = saved ? saved.h : size[1]
    }
    onRequestedSizeChanged: if (geometryReady) applySize(null)

    Component.onCompleted: {
        const g = settings.general && settings.general.remember_window_geometry === false
            ? null : settings.translation_window.floating_geometry
        applySize(g)
        geometryReady = true
        if (x11 && g) {
            // X11: Qt may position top-level windows; restore once, before showing.
            x = Math.round(g.x); y = Math.round(g.y)
            console.debug(logGeometry, "position changed by restore_geometry x=" + x + " y=" + y)
        }
        console.debug(logState, "pinned=false floating=true window_role=" + (x11 ? "x11_toplevel" : "xdg_toplevel") + " layershell=false")
    }
    onVisibleChanged: {
        console.debug(logState, "visible=" + visible + " width=" + width + " height=" + height
                      + " screen=" + (screen ? screen.name : "?"))
        if (visible) {
            Qt.callLater(updateInput)
            Qt.callLater(updateBlur)
        } else if (geometryReady && x11) reportGeometry("hidden")
    }

    // KWin reports interactiveMoveResizeFinished on both Wayland and X11. Without
    // that service, X11 can at least save the final geometry when the window is hidden.
    function reportGeometry(reason) {
        const s = screen
        const g = { x: x, y: y, w: width, h: height, output: s ? s.name : "",
                    output_x: s ? s.virtualX : 0, output_y: s ? s.virtualY : 0 }
        if (controller && controller.reportFloatingGeometry)
            controller.reportFloatingGeometry(JSON.stringify(g), reason)
    }

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

    // Frameless top-levels have no decoration resize grip. Hand the corner gesture
    // to the compositor, just like movement; never change global coordinates here.
    MouseArea {
        objectName: "floatingResizeGrip"
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        width: 14
        height: 14
        cursorShape: Qt.SizeFDiagCursor
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton
        onPressed: mouse => {
            if (mouse.button === Qt.MiddleButton) win.pinToggleRequested()
            else {
                const accepted = win.startSystemResize(Qt.RightEdge | Qt.BottomEdge)
                console.debug(logDrag, "system_resize_requested accepted=" + accepted)
                if (!accepted) console.warn(logDrag, "startSystemResize is not supported by this platform")
            }
        }
    }
}
