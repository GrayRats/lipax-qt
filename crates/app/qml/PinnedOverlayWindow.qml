import QtQuick
import QtQml
import org.kde.layershell 1.0 as LayerShell

// Pinned translation window: a wlr-layer-shell surface (layer Overlay) that stays above the
// game. Its position is overlay-controlled: margins from the top-left corner of one screen.
// The screen is chosen when the surface is shown and changes only when the user picks
// another screen in the settings or that screen disappears — never on game geometry updates
// or settings autosave. The body is click-through; the pin handle keeps receiving input.
Window {
    id: win
    objectName: "translationOverlay"
    property var controller
    property var settings: ({})
    property string translation: ""
    property string original: ""
    property string gameGeometry: ""
    signal pinToggleRequested()
    readonly property alias content: content

    LoggingCategory { id: logLayer; name: "overlay.layershell"; defaultLogLevel: LoggingCategory.Debug }
    LoggingCategory { id: logScreen; name: "overlay.screen"; defaultLogLevel: LoggingCategory.Debug }

    property var targetScreen: null
    // The screen for a new pinned surface: the one chosen in the settings, else the game's.
    function chooseScreen() {
        const screens = Qt.application.screens
        let chosen = screens.find(s => s.name === settings.overlay_screen)
        if (!chosen && gameGeometry.length) {
            try {
                const g = JSON.parse(gameGeometry)
                let best = 0
                for (const s of screens) {
                    const area = Math.max(0, Math.min(g[0] + g[2], s.virtualX + s.width) - Math.max(g[0], s.virtualX))
                               * Math.max(0, Math.min(g[1] + g[3], s.virtualY + s.height) - Math.max(g[1], s.virtualY))
                    if (area > best) { best = area; chosen = s }
                }
            } catch (e) {}
        }
        return chosen || (screens.length ? screens[0] : null)
    }
    // A layer surface is bound to its output at creation, so the screen is set before showing.
    function place(reason) {
        targetScreen = chooseScreen()
        if (targetScreen) screen = targetScreen
        console.debug(logLayer, "active=true anchors=top|left layer=overlay reason=" + reason
                      + " screen=" + (targetScreen ? targetScreen.name : "?") + " margins=" + actualX + "," + actualY)
    }
    Component.onCompleted: {
        place("pinned_surface_created")
    }
    onVisibleChanged: if (visible) {
        // Show the frame briefly after pinning so the new state is visible in any frame mode.
        content.flashFrame()
        Qt.callLater(updateInput)
        Qt.callLater(updateBlur)
    }
    // Only an explicit screen choice in the settings moves the pinned surface to another screen.
    readonly property string screenSetting: settings.overlay_screen || ""
    onScreenSettingChanged: if (visible) { console.debug(logScreen, "position changed by user_screen_setting"); place("user_screen_setting") }
    Connections {
        target: Qt.application
        function onScreensChanged() {
            if (!Qt.application.screens.includes(win.targetScreen)) {
                console.debug(logScreen, "position changed by monitor_reassignment")
                win.place("monitor_reassignment")
            }
        }
    }

    readonly property int screenWidth: targetScreen ? targetScreen.width : Screen.width
    readonly property int screenHeight: targetScreen ? targetScreen.height : Screen.height
    // Kept on its screen: this is the placement of an overlay, not a drag constraint.
    readonly property int actualX: Math.max(0, Math.min(settings.overlay_pos ? settings.overlay_pos[0] : 100, screenWidth - width))
    readonly property int actualY: Math.max(0, Math.min(settings.overlay_pos ? settings.overlay_pos[1] : 100, screenHeight - height))

    visible: false
    color: "transparent"
    flags: Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool | Qt.WindowDoesNotAcceptFocus
    // On an X11 Qt backend these coordinates accompany the WM keep-above hint.
    x: (targetScreen ? targetScreen.virtualX : 0) + actualX
    y: (targetScreen ? targetScreen.virtualY : 0) + actualY
    width: Math.min(settings.overlay_size ? settings.overlay_size[0] : 700,
                    settings.max_width_enabled !== false ? (settings.overlay_max_width || 900) : 100000,
                    screenWidth)
    height: Math.min(settings.overlay_size ? settings.overlay_size[1] : 120, screenHeight)

    LayerShell.Window.scope: "lipa-overlay"
    LayerShell.Window.layer: LayerShell.Window.LayerOverlay
    LayerShell.Window.anchors: LayerShell.Window.AnchorTop | LayerShell.Window.AnchorLeft
    LayerShell.Window.margins: ({ left: win.actualX, top: win.actualY, right: 0, bottom: 0 })
    LayerShell.Window.exclusionZone: -1
    LayerShell.Window.keyboardInteractivity: LayerShell.Window.KeyboardInteractivityNone

    // Click-through except the pin handle (and the frame band while it is visible).
    readonly property bool passthrough: settings.click_through !== false
    function updateInput() {
        if (!visible) return
        if (controller) controller.configureOverlay(passthrough, JSON.stringify(content.inputRects()))
    }
    function updateBlur() {
        if (!visible) return
        if (controller && controller.configureOverlayBlur) controller.configureOverlayBlur(content.blurBehind, content.cornerRadius)
    }
    onPassthroughChanged: Qt.callLater(updateInput)
    onWidthChanged: { Qt.callLater(updateInput); Qt.callLater(updateBlur) }
    onHeightChanged: { Qt.callLater(updateInput); Qt.callLater(updateBlur) }

    OverlayContent {
        id: content
        anchors.fill: parent
        settings: win.settings
        translation: win.translation
        original: win.original
        pinned: true
        onFrameVisibleChanged: Qt.callLater(win.updateInput)
        // The mask is in content coordinates: recompute when the content (not only the window)
        // gets its final size, otherwise the handle zone is computed for a zero-width item.
        onWidthChanged: Qt.callLater(win.updateInput)
        onHeightChanged: Qt.callLater(win.updateInput)
        onBlurBehindChanged: Qt.callLater(win.updateBlur)
        onCornerRadiusChanged: { Qt.callLater(win.updateInput); Qt.callLater(win.updateBlur) }
        onPinToggleRequested: win.pinToggleRequested()
    }
}
