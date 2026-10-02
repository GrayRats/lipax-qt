import QtQuick
import QtTest
import "../qml" as Lipa

TestCase {
    name: "Inplace"
    when: windowShown

    // Window client area 1000×500 at (100, 50); region = bottom 30%; block in the middle of it.
    // Captured frame of the region is 2000×300 (scale 2 capture), so 1 frame px = 0.5 screen px.
    property var baseEntry: ({ id: "subtitles", rect: { x: 0, y: 0.7, w: 1, h: 0.3 },
        bbox: [0.25, 0.3, 0.5, 0.4], frame: [2000, 300], font_px: 40, bold: true,
        text_color: "#ffe066", background_color: "#203040", backdrop: "", text: "Привет" })
    Lipa.InplaceText {
        id: inplace
        visible: true
        gameGeometry: "[100,50,1000,500]"
        entry: baseEntry
        settings: ({})
    }
    Text { id: unfitted; visible: false; font.pixelSize: 20; font.bold: true; wrapMode: Text.Wrap }
    SignalSpy { id: hiddenSpy; target: inplace; signalName: "hideRequested" }
    function init() { inplace.entry = baseEntry; inplace.gameGeometry = "[100,50,1000,500]"; hiddenSpy.clear() }
    function test_middleButtonRequestsHideWithoutFocus() {
        mouseClick(child("inplaceMouse"), 10, 10, Qt.MiddleButton)
        compare(hiddenSpy.count, 1)
        verify((inplace.flags & Qt.WindowDoesNotAcceptFocus) !== 0)
    }
    function child(name) { return findChild(inplace.contentItem, name) }

    function test_placedOverTheOriginalBlock() {
        // Region: y = 50 + 0.7·500 = 400, h = 150; block: y = 400 + 0.3·150 = 445.
        compare(inplace.desktopRect, [350, 445, 500, 60])
        fuzzyCompare(inplace.scale, 0.5, 0.0001)
        compare(inplace.width, 500)
        compare(inplace.height, 60)
    }

    function test_takesOriginalLook() {
        const t = child("inplaceText")
        verify(Qt.colorEqual(t.color, "#ffe066"))
        verify(t.font.bold)
        compare(t.font.pixelSize, 20, "original 40 frame px at scale 0.5")
        verify(Qt.colorEqual(child("inplaceFill").color, "#203040"), "fill takes the background colour")
    }

    function test_longTranslationShrinksToFit() {
        inplace.entry = Object.assign({}, baseEntry, { text: "Это очень длинный перевод, который не помещается в исходный блок текста целиком. ".repeat(4) })
        const t = child("inplaceText")
        tryVerify(() => t.contentHeight <= t.height + 1, 1000, "fits vertically")
        verify(t.contentWidth <= t.width + 1, "fits horizontally")
        // The same text at the original size would not fit the block.
        unfitted.text = t.text
        unfitted.width = t.width
        verify(unfitted.contentHeight > t.height, "shrinking was needed")
        inplace.entry = baseEntry
    }

    function test_noGeometryNoPlacement() {
        inplace.gameGeometry = ""
        compare(inplace.desktopRect, null)
        inplace.gameGeometry = "[100,50,1000,500]"
    }
}
