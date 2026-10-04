import QtQuick
import QtTest
import "../qml" as Lipa

TestCase {
    name: "Overlay"
    when: windowShown

    property int closes: 0
    QtObject {
        id: ctl
        property bool passthrough: false
        property var rects: []
        property bool blur: false
        property int blurRadius: -1
        property var reported: []
        property int focusArms: 0
        function armFloatingFocusRestore() { focusArms++ }
        function configureOverlay(p, json) { passthrough = p; rects = JSON.parse(json) }
        function configureOverlayBlur(enable, radius) { blur = enable; blurRadius = radius }
        function reportFloatingGeometry(json, reason) { reported.push([JSON.parse(json), reason]) }
    }
    Lipa.TranslationOverlay {
        id: ov
        controller: ctl
        shown: true
        translation: "Привет, мир"
        original: "Hello, world"
        gameGeometry: "[0,0,800,600]"
        settings: ({ overlay_pinned: false })
        // As in main.qml the Controller saves the new pin state, which flips `pinned`.
        onPinToggled: (p) => settings = Object.assign({}, settings, { overlay_pinned: p })
        onCloseRequested: closes++
    }
    SignalSpy { id: moveSpy; signalName: "moveRequested" }

    function base(extra) { return Object.assign({ overlay_pinned: false, overlay_pos: [50, 60], overlay_size: [600, 140], border_width: 2 }, extra || ({})) }
    function child(name) { return findChild(ov.window.contentItem, name) }
    function inside(r, x, y) { return x >= r[0] && y >= r[1] && x < r[0] + r[2] && y < r[1] + r[3] }
    function init() {
        ov.settings = base()
        tryVerify(() => ov.window !== null && ov.window.visible, 2000)
        wait(30)
    }

    function test_floatingIsAnOrdinaryTopLevel() {
        const w = ov.window
        compare(w.title, "LipaX · окно перевода", "the KWin script finds the floating window by this exact title")
        verify(w.transientParent === null, "independent top-level")
        compare(w.flags & 0xff, Qt.Window, "normal task-manager window, not Qt.Tool")
        compare(w.content.cornerRadius, 12)
        verify(child("closeButton").visible)
        verify(child("overlayBorder").visible, "border active while floating")
        tryVerify(() => !ctl.passthrough && ctl.rects.length === 0, 1000, "floating window captures input")
    }

    function test_textShrinksTo14ThenScrollsAndResets() {
        ov.settings = base({ overlay_size: [240, 140], font_size: 40, font_family: "Inter",
                             overlay_auto_shrink: true, text_wrap: true })
        ov.translation = "Привет"
        const t = child("translatedText"), scroll = child("overlayTextScroll")
        tryCompare(t.font, "pixelSize", 40, 1000)
        ov.translation = "Очень длинный перевод. ".repeat(30)
        tryVerify(() => t.font.pixelSize === 14 && scroll.contentHeight > scroll.height, 2000)
        const x0 = ov.window.x, y0 = ov.window.y
        mouseWheel(child("overlayDragArea"), 100, 40, 0, -120)
        tryVerify(() => scroll.contentY > 0, 1000, "wheel scrolls overflow")
        compare([ov.window.x, ov.window.y], [x0, y0], "wheel does not move the window")
        ov.translation = "Привет"
        tryCompare(t.font, "pixelSize", 40, 1000)
        compare(scroll.contentY, 0, "new translation starts from the top")
        ov.translation = "Очень длинный перевод. ".repeat(30)
        ov.settings = Object.assign({}, ov.settings, { overlay_auto_shrink: false })
        tryCompare(t.font, "pixelSize", 40, 1000)
        ov.settings = Object.assign({}, ov.settings, { overlay_auto_shrink: true, overlay_size: [600, 200] })
        tryVerify(() => t.font.pixelSize >= 14 && t.font.pixelSize <= 40, 1000)
    }

    function test_textFitRespondsToWindowSizeAndBaseFont() {
        ov.settings = base({ overlay_size: [240, 140], font_size: 40, font_family: "Inter",
                             overlay_auto_shrink: true, text_wrap: true })
        ov.translation = "Перевод длинной реплики для проверки. ".repeat(4)
        const t = child("translatedText")
        tryVerify(() => t.font.pixelSize < 40 && t.font.pixelSize >= 14, 1000)
        const narrow = t.font.pixelSize
        ov.settings = Object.assign({}, ov.settings, { overlay_size: [600, 250] })
        tryVerify(() => t.font.pixelSize > narrow, 1000, "resize remeasures text")
        ov.settings = Object.assign({}, ov.settings, { font_size: 24 })
        tryVerify(() => t.font.pixelSize <= 24 && t.font.pixelSize >= 14, 1000)
    }

    function test_lmbStartsNativeMoveAndNothingRepositions() {
        const w = ov.window
        moveSpy.target = w.content
        moveSpy.clear()
        const x0 = w.x, y0 = w.y
        mousePress(child("overlayDragArea"), 100, 50, Qt.LeftButton)
        mouseRelease(child("overlayDragArea"), 100, 50, Qt.LeftButton)
        compare(moveSpy.count, 1, "LMB hands the move to the compositor")
        // No application-side re-anchoring after release, settings writes or game geometry updates.
        ov.settings = base({ font_size: 30 })
        ov.gameGeometry = "[900,100,800,600]"
        wait(50)
        compare([ov.window.x, ov.window.y], [x0, y0])
    }

    function test_pinTransitionsSwapTheSurface() {
        const floating = ov.window
        const initialArms = ctl.focusArms
        mouseClick(child("overlayDragArea"), 100, 50, Qt.MiddleButton)
        verify(ov.pinned, "MMB pins")
        tryVerify(() => ov.window !== null && ov.window !== floating && ov.window.visible, 2000, "pinned surface replaces the floating one")
        const pinned = ov.window
        verify(!floating.visible, "the same floating object is hidden")
        compare(pinned.flags & 0xff, Qt.Tool, "layer surface stays out of the task manager")
        tryVerify(() => pinned.content.width === pinned.width, 1000, "content laid out")
        compare(pinned.content.cornerRadius, 0)
        verify(!child("closeButton").visible, "no close button when pinned")
        tryVerify(() => ctl.passthrough, 1000, "pinned body is click-through")
        const h = child("pinHandle")
        const cx = h.x + h.width / 2, cy = h.y + h.height / 2
        tryVerify(() => ctl.rects.some(r => inside(r, cx, cy)), 1000, "pin handle keeps receiving input")
        verify(!ctl.rects.some(r => inside(r, pinned.width / 2, pinned.height / 2)), "text area passes clicks through")
        moveSpy.target = pinned.content
        moveSpy.clear()
        mousePress(child("overlayDragArea"), 100, 50, Qt.LeftButton)
        mouseRelease(child("overlayDragArea"), 100, 50, Qt.LeftButton)
        compare(moveSpy.count, 0, "no LMB drag when pinned")
        mouseClick(child("overlayDragArea"), cx, cy, Qt.MiddleButton)
        verify(!ov.pinned, "MMB on the handle unpins")
        tryVerify(() => ov.window === floating && ov.window.content.cornerRadius === 12, 2000)
        compare(ctl.focusArms, initialArms + 1, "each automatic unpin arms focus restoration once")
        verify(!pinned.visible, "the pinned object is hidden and kept for reuse")
        ov.settings = Object.assign({}, ov.settings, { overlay_pinned: true })
        compare(ov.window, pinned, "the same pinned surface is reused")
        ov.settings = Object.assign({}, ov.settings, { overlay_pinned: false })
        compare(ov.window, floating, "the same floating surface is reused")
        compare(ctl.focusArms, initialArms + 2, "a second unpin gets its own focus guard")
    }

    function test_pinnedCornerRadiusChangesWithoutRecreatingTheWindow() {
        ov.settings = base({ overlay_pinned: true, overlay_style: "blur", overlay_pinned_corner_radius: 0 })
        tryVerify(() => ov.window !== null && ov.window.visible, 2000)
        const pinned = ov.window
        compare(pinned.content.cornerRadius, 0)
        ov.settings = Object.assign({}, ov.settings, { overlay_pinned_corner_radius: 18 })
        compare(ov.window, pinned, "the layer surface stays alive")
        tryCompare(pinned.content, "cornerRadius", 18, 1000)
        compare(child("overlayBackground").radius, 18)
        compare(child("overlayBorder").radius, 18)
        tryCompare(ctl, "blurRadius", 18, 1000)
        ov.settings = Object.assign({}, ov.settings, { overlay_pinned_corner_radius: 0 })
        tryCompare(pinned.content, "cornerRadius", 0, 1000)
        compare(child("overlayBackground").radius, 0, "zero removes rounding")
    }

    function test_pinnedSurfaceIsNotMovedByGameOrSettings() {
        ov.settings = base({ overlay_pinned: true, overlay_screen: "" })
        tryVerify(() => ov.window !== null && ov.window.content.pinned && ov.window.visible, 2000)
        const w = ov.window, screen0 = w.targetScreen, x0 = w.x
        ov.gameGeometry = "[5000,5000,100,100]"
        ov.settings = Object.assign({}, ov.settings, { font_size: 25 })
        wait(50)
        verify(w.targetScreen === screen0, "game geometry updates do not re-pick the screen")
        compare(w.x, x0)
    }

    // Regression: in "after selection" mode the frame was hidden once pinned, leaving only
    // invisible input zones, so MMB could not unpin the overlay.
    function test_unpinInEveryFrameMode_data() {
        return [
            { tag: "always", mode: { border_always: true } },
            { tag: "after-selection", mode: { border_always: false, border_seconds: 1 } },
        ]
    }
    function test_unpinInEveryFrameMode(data) {
        ov.settings = base(Object.assign({ overlay_pinned: true }, data.mode))
        tryVerify(() => ov.window !== null && ov.window.content.pinned && ov.window.visible, 2000)
        tryVerify(() => data.mode.border_always !== false || !ov.window.content.frameVisible, 3000)
        tryVerify(() => ov.window.content.width === ov.window.width, 1000, "content laid out")
        const h = child("pinHandle")
        const cx = h.x + h.width / 2, cy = h.y + h.height / 2
        verify(h.visible && h.opacity > 0.3, "pin handle stays visible")
        tryVerify(() => ctl.rects.some(r => inside(r, cx, cy)), 1000)
        // The new surface must be exposed before it can receive synthetic input.
        waitForRendering(ov.window.contentItem)
        mouseClick(child("overlayDragArea"), cx, cy, Qt.MiddleButton)
        verify(!ov.pinned)
    }

    function test_displayStyles_data() {
        return [
            { tag: "solid", set: { overlay_style: "solid", background_color: "#102030", opacity: 0.85, text_color: "#ffe066" },
              bg: "#102030", alpha: 0.85, text: "#ffe066", blur: false },
            { tag: "blur", set: { overlay_style: "blur", blur_tint: 0.4 }, bg: "#000000", alpha: 0.4, text: "#ffffff", blur: true },
            { tag: "blur-off", set: { overlay_style: "blur", blur_enabled: false, blur_tint: 0.2 }, bg: "#000000", alpha: 0.2, text: "#ffffff", blur: false },
            { tag: "transparent", set: { overlay_style: "transparent" }, bg: "#000000", alpha: 0, text: "#ffffff", blur: false },
            { tag: "dim", set: { overlay_style: "dim" }, bg: "#000000", alpha: 0.55, text: "#ffffff", blur: true },
            { tag: "dim-inverse", set: { overlay_style: "dim", dim_inverse: true }, bg: "#f2f2f2", alpha: 0.72, text: "#141414", blur: true },
        ]
    }
    function test_displayStyles(data) {
        ov.settings = base(data.set)
        const bg = child("overlayBackground"), text = child("translatedText")
        verify(Qt.colorEqual(bg.color, data.bg), "background colour")
        fuzzyCompare(bg.opacity, data.alpha, 0.001)
        verify(Qt.colorEqual(text.color, data.text), "text colour")
        tryVerify(() => ctl.blur === data.blur, 1000, "compositor blur")
        if (data.blur) tryCompare(ctl, "blurRadius", 12, 1000, "blur follows the rounded floating shape")
    }

    function test_closeButtonHidesOnlyThroughTheOwner() {
        closes = 0
        const c = child("closeButton")
        mouseClick(c, c.width / 2, c.height / 2, Qt.LeftButton)
        compare(closes, 1)
    }

    function test_hiddenControllerKeepsBothSurfaceObjects() {
        const floating = ov.window
        ov.shown = false
        tryVerify(() => ov.window === null, 1000)
        verify(!floating.visible)
        ov.shown = true
        tryVerify(() => ov.window === floating && floating.visible, 1000)
    }
}
