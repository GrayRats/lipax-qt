import QtQuick
import QtTest
import "../qml" as Lipa

TestCase {
    name: "Overlay"
    when: windowShown
    property var toggles: []
    property var moves: []
    QtObject { id: ctl; property int calls: 0; function configureOverlay(p, c, e) { calls++ } }
    Lipa.TranslationOverlay {
        id: ov
        controller: ctl
        translation: "Привет, мир"
        original: "Hello, world"
        settings: ({ overlay_pinned: false, show_original: true, overlay_pos: [50, 60], overlay_size: [600, 140], border_width: 2, font_size: 20 })
        visible: true
        onPinToggled: (p) => toggles.push(p)
        onMoved: (x, y) => moves.push([x, y])
    }
    function test_overlay() {
        wait(100)
        compare(ov.frameWidth, 4, "unpinned frame is thicker")
        const area = findChild(ov.contentItem, "overlayDragArea")
        verify(area)
        mouseClick(area, 100, 50, Qt.MiddleButton)
        compare(toggles, [true])
        mouseDrag(area, 100, 50, 40, 30, Qt.LeftButton)
        verify(moves.length === 1)
        verify(moves[0][0] > 50)
        ov.settings = Object.assign({}, ov.settings, { overlay_pinned: true })
        compare(ov.frameWidth, 2)
        verify(ov.frameVisible, "frame shown by default")
        ov.settings = Object.assign({}, ov.settings, { border_always: false, border_seconds: 1 })
        verify(!ov.frameVisible, "pinned frame hidden in temporary mode")
        ov.flashFrame()
        verify(ov.frameVisible, "frame flashes after selection")
        tryVerify(() => !ov.frameVisible, 2500)
        ov.settings = Object.assign({}, ov.settings, { overlay_pinned: false })
        verify(ov.frameVisible, "unpinned overlay always shows frame")
        verify(ctl.calls > 0)
        ov.contentItem.grabToImage(r => r.saveToFile("/tmp/lipa-overlay-qa.png")); wait(150)
    }
}
