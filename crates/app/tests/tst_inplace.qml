import QtQuick
import QtTest
import "../qml" as Lipa

TestCase {
    name: "Inplace"
    when: windowShown

    // Window client area 1000×500 at (100, 50); region = bottom 30%; the field box is in the
    // middle of the region frame. All decisions come from Rust; QML only renders them.
    property var baseEntry: ({
        key: "subtitles:7", block_id: 7, rect: { x: 0, y: 0.7, w: 1, h: 0.3 }, box: [0.25, 0.3, 0.5, 0.4],
        inner: [6, 4, 6, 4], text: "Привет", font_family: "DejaVu Serif", font_px: 20, font_weight: 700, italic: true,
        line_height: 1.25, letter_spacing: 1.5, alignment: "left", wrap: "word", max_lines: 2,
        text_color: "#ffe066", outline: false, outline_color: "#000000",
        background: { mode: "solid_fill", color: "#203040", image: "" } })
    property int hides: 0
    Lipa.InplaceText {
        id: inplace
        visible: true
        gameGeometry: "[100,50,1000,500]"
        entry: baseEntry
        onHideRequested: hides++
    }
    function child(name) { return findChild(inplace.contentItem, name) }
    function init() { inplace.entry = baseEntry; inplace.gameGeometry = "[100,50,1000,500]"; wait(20) }

    function test_placedOverTheOriginalField() {
        // Region: y = 50 + 0.7·500 = 400, h = 150; box: x = 100 + 0.25·1000, y = 400 + 0.3·150.
        compare(inplace.desktopRect, [350, 445, 500, 60])
        compare(inplace.width, 500)
        compare(inplace.height, 60)
    }

    function test_rendersEveryPropertyFromRust() {
        const t = child("inplaceText")
        compare(t.font.family, "DejaVu Serif")
        compare(t.font.pixelSize, 20)
        compare(t.font.weight, 700)
        verify(t.font.italic)
        fuzzyCompare(t.font.letterSpacing, 1.5, 0.01)
        fuzzyCompare(t.lineHeight, 1.25, 0.001)
        compare(t.horizontalAlignment, Text.AlignLeft)
        compare(t.wrapMode, Text.Wrap)
        verify(Qt.colorEqual(t.color, "#ffe066"))
        compare([t.x, t.y, t.width, t.height], [6, 4, 488, 52], "inner padding from Rust")
    }

    function test_backgroundModes_data() {
        return [
            { tag: "solid", mode: "solid_fill", fill: true, image: false, shown: true },
            { tag: "adaptive", mode: "adaptive_padding_fill", fill: true, image: false, shown: true },
            { tag: "inpaint", mode: "inpaint_blur", fill: false, image: true, shown: true },
            { tag: "transparent", mode: "transparent", fill: false, image: false, shown: false },
        ]
    }
    function test_backgroundModes(data) {
        inplace.entry = Object.assign({}, baseEntry, {
            outline: data.mode === "transparent",
            background: { mode: data.mode, color: "#203040", image: data.image ? "file:///nonexistent.png" : "" } })
        compare(child("inplaceBackground").visible, data.shown)
        compare(child("inplaceFill").visible, data.fill)
        compare(child("inplaceBackdrop").visible, data.image)
        compare(child("inplaceText").style, data.mode === "transparent" ? Text.Outline : Text.Normal, "readable without a fill")
    }

    function test_overflowFallbackElides() {
        inplace.entry = Object.assign({}, baseEntry, { wrap: "elide", max_lines: 1, text: "очень ".repeat(50) })
        const t = child("inplaceText")
        compare(t.elide, Text.ElideRight)
        compare(t.maximumLineCount, 1)
        verify(t.truncated, "text never runs outside the field")
    }

    function test_propertiesUpdateWithoutRecreatingTheItem() {
        const t = child("inplaceText")
        inplace.entry = Object.assign({}, baseEntry, { text: "Другой перевод", font_px: 16 })
        compare(child("inplaceText"), t, "same item")
        compare(t.font.pixelSize, 16)
    }

    function test_middleButtonRequestsHide() {
        hides = 0
        mouseClick(child("inplaceMouse"), 20, 20, Qt.MiddleButton)
        compare(hides, 1)
    }

    function test_transparentOutlineAndShadowSettings() {
        inplace.entry = Object.assign({}, baseEntry, { outline: true, outline_width: 3,
            shadow: true, text_opacity: 0.7, background: { mode: "transparent", color: "#203040", image: "" } })
        compare(child("inplaceBackground").visible, false)
        compare(child("inplaceText").style, Text.Normal, "wide outline uses explicit surrounding glyphs")
        fuzzyCompare(child("inplaceText").opacity, 0.7, 0.001)
    }

    function test_noGeometryNoPlacement() {
        inplace.gameGeometry = ""
        compare(inplace.desktopRect, null)
    }
}
