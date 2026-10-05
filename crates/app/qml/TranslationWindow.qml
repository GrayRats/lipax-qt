import QtQuick

// Translation window controller: one state (pinned or floating, visibility, content), two
// surface implementations. A Wayland surface cannot change its shell role, so both windows keep
// their own role and are reused across pin transitions. Position state lives in settings (Rust): the pinned
// placement (translation_window.screen + translation_window.position) and the last floating geometry are kept separately,
// so pinning never destroys where the floating window was and vice versa.
Item {
    id: ov
    property var controller
    property var settings: ({capture: {}, recognition: {}, translation: {}, translation_window: {}, appearance: {window: {}, inplace: {}}})
    property string translation: ""
    property string original: ""
    property string gameGeometry: ""
    // Requested by the owner (window mode active, visibility on, text present).
    property bool shown: false
    signal pinToggled(bool pinned)
    signal closeRequested()

    readonly property bool pinned: settings.translation_window.mode === "pinned"
    readonly property bool floating: !pinned
    property bool ready: false
    // The currently shown surface, or null while the translation is hidden.
    readonly property var window: shown ? (pinned ? pinnedSurface : floatingSurface) : null
    function flashFrame() { if (window) window.content.flashFrame() }

    // Arm KWin before mapping the normal window. The flag belongs to this automatic show,
    // not to the lifetime of the QML object: later task-manager clicks may take focus.
    function syncSurfaces() {
        if (!ready) return
        if (!shown) {
            pinnedSurface.visible = false
            floatingSurface.visible = false
            if (controller && controller.configureTranslationWindowBlur) controller.configureTranslationWindowBlur(false, 0)
        } else if (pinned) {
            if (!pinnedSurface.visible) {
                pinnedSurface.place("pin_transition")
                pinnedSurface.visible = true
            }
            floatingSurface.visible = false
        } else {
            if (!floatingSurface.visible) {
                if (controller && controller.armFloatingFocusRestore) controller.armFloatingFocusRestore()
                floatingSurface.visible = true
            }
            pinnedSurface.visible = false
        }
    }
    onShownChanged: syncSurfaces()
    onPinnedChanged: syncSurfaces()
    Component.onCompleted: { ready = true; syncSurfaces() }

    PinnedTranslationSurface {
        id: pinnedSurface
        controller: ov.controller
        settings: ov.settings
        translation: ov.translation
        original: ov.original
        gameGeometry: ov.gameGeometry
        onPinToggleRequested: ov.pinToggled(false)
    }
    FloatingTranslationSurface {
        id: floatingSurface
        controller: ov.controller
        settings: ov.settings
        translation: ov.translation
        original: ov.original
        onPinToggleRequested: ov.pinToggled(true)
        onCloseRequested: ov.closeRequested()
    }
}
