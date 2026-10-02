import QtQuick
import QtTest
import "../qml" as Lipa

TestCase {
    name: "Overlay"
    when: windowShown
    property var moves: []
    QtObject {
        id: ctl
        property bool passthrough: false
        property var rects: []
        function configureOverlay(p, json) { passthrough = p; rects = JSON.parse(json) }
    }
    Lipa.TranslationOverlay {
        id: ov
        controller: ctl
        translation: "Привет, мир"
        original: "Hello, world"
        settings: ({ overlay_pinned: false, show_original: true, overlay_pos: [50, 60], overlay_size: [600, 140], border_width: 2, font_size: 20 })
        visible: true
        // As in main.qml: the toggle is saved into settings, which flips `pinned`.
        onPinToggled: (p) => settings = Object.assign({}, settings, { overlay_pinned: p })
        onMoved: (x, y) => moves.push([x, y])
    }

    function init() {
        ov.settings = { overlay_pinned: false, overlay_pos: [50, 60], overlay_size: [600, 140], border_width: 2 }
        wait(50)
    }

    function inside(r, x, y) { return x >= r[0] && y >= r[1] && x < r[0] + r[2] && y < r[1] + r[3] }

    function test_dragAndFrameWidth() {
        compare(ov.frameWidth, 4, "unpinned frame is thicker")
        const area = findChild(ov.contentItem, "overlayDragArea")
        moves = []
        mouseDrag(area, 100, 50, 40, 30, Qt.LeftButton)
        compare(moves.length, 1)
        verify(moves[0][0] > 50)
    }

    // Regression: in "after selection" mode the frame and the old corner icon were hidden once
    // pinned, leaving only invisible input zones, so MMB could not unpin the overlay.
    function test_unpinInEveryFrameMode_data() {
        return [
            { tag: "always", mode: { border_always: true } },
            { tag: "after-selection", mode: { border_always: false, border_seconds: 1 } },
        ]
    }
    function test_unpinInEveryFrameMode(data) {
        ov.settings = Object.assign({}, ov.settings, data.mode)
        const area = findChild(ov.contentItem, "overlayDragArea")
        const handle = findChild(ov.contentItem, "pinHandle")
        const cx = handle.x + handle.width / 2, cy = handle.y + handle.height / 2

        mouseClick(area, cx, cy, Qt.MiddleButton)
        verify(ov.pinned, "MMB pins")
        // Let the post-toggle flash of the frame run out.
        tryVerify(() => data.mode.border_always !== false || !ov.frameVisible, 3000)
        wait(50)

        verify(handle.visible && handle.opacity > 0.3, "pin handle stays visible")
        verify(ctl.passthrough, "clicks pass through to the game")
        verify(ctl.rects.some(r => inside(r, cx, cy)), "pin handle keeps receiving input")
        verify(!ctl.rects.some(r => inside(r, ov.width / 2, ov.height / 2)), "the text area passes clicks through")

        mouseClick(area, cx, cy, Qt.MiddleButton)
        verify(!ov.pinned, "MMB on the handle unpins")
        tryVerify(() => !ctl.passthrough, 1000, "input region is cleared after unpinning")
    }

    function test_screenshot() {
        ov.settings = Object.assign({}, ov.settings, { show_original: true })
        ov.contentItem.grabToImage(r => r.saveToFile("/tmp/lipa-overlay-qa.png")); wait(150)
    }
}
