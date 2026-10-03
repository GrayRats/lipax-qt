import QtQuick

// Translation window controller: one state (pinned or floating, visibility, content), two
// surface implementations. A Wayland surface cannot change its shell role after creation, so
// switching between pinned (layer-shell) and floating (xdg_toplevel) destroys one platform
// window and creates the other. Position state lives in the settings (Rust): the pinned
// placement (overlay_screen + overlay_pos) and the last floating geometry are kept separately,
// so pinning never destroys where the floating window was and vice versa.
Item {
    id: ov
    property var controller
    property var settings: ({})
    property string translation: ""
    property string original: ""
    property string gameGeometry: ""
    // Requested by the owner (window mode active, visibility on, text present).
    property bool shown: false
    signal pinToggled(bool pinned)
    signal closeRequested()

    readonly property bool pinned: settings.overlay_pinned === true
    readonly property bool floating: !pinned
    // The live surface, or null while hidden.
    readonly property var window: pinned ? pinnedSurface.object : floatingSurface.object
    function flashFrame() { if (window) window.content.flashFrame() }

    Instantiator {
        id: pinnedSurface
        active: ov.shown && ov.pinned
        delegate: PinnedOverlayWindow {
            controller: ov.controller
            settings: ov.settings
            translation: ov.translation
            original: ov.original
            gameGeometry: ov.gameGeometry
            onPinToggleRequested: ov.pinToggled(false)
        }
    }
    Instantiator {
        id: floatingSurface
        active: ov.shown && ov.floating
        delegate: FloatingOverlayWindow {
            controller: ov.controller
            settings: ov.settings
            translation: ov.translation
            original: ov.original
            onPinToggleRequested: ov.pinToggled(true)
            onCloseRequested: ov.closeRequested()
        }
    }
}
