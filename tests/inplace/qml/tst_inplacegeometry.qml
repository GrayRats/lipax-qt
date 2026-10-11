import QtQuick
import QtTest
import "../../../crates/app/qml/InplaceGeometry.js" as Geometry
import "../../../crates/app/qml" as Lipa

TestCase {
    name: "InplaceGeometry"
    when: windowShown
    Lipa.InplaceTranslationContent { id: content; width: 240; height: 120 }
    function entry(box) { return { rect: { x: 0, y: 0, w: 1, h: 1 }, box: box } }
    function test_negativeMonitorAndFractionalScale() {
        const r = Geometry.desktopRect([-1920, -200, 1536, 864], entry([0.25, 0.5, 0.5, 0.25]))
        compare(r, [-1536, 232, 768, 216])
    }
    function test_captureBoundary() {
        compare(Geometry.desktopRect([0, 0, 100, 100], entry([-0.5, -0.5, 1, 1])), [0, 0, 50, 50])
        compare(Geometry.desktopRect([0, 0, 100, 100], entry([2, 2, 1, 1])), null)
        const e = { rect: { x: 0.2, y: 0.3, w: 0.4, h: 0.5 }, box: [-1, -1, 3, 3] }
        const r = Geometry.desktopRect([0, 0, 100, 100], e)
        fuzzyCompare(r[0], 20, 0.001); fuzzyCompare(r[1], 30, 0.001)
        fuzzyCompare(r[2], 40, 0.001); fuzzyCompare(r[3], 50, 0.001)
    }
    function test_invalidInput_data() {
        return [ {tag:"missing", g:null, b:[0,0,1,1]}, {tag:"zero",g:[0,0,0,100],b:[0,0,1,1]},
            {tag:"height",g:[0,0,100,-1],b:[0,0,1,1]}, {tag:"nan",g:[NaN,0,100,100],b:[0,0,1,1]},
            {tag:"infinite",g:[0,0,100,100],b:[0,0,Infinity,1]}, {tag:"string",g:[0,0,100,100],b:["0",0,1,1]} ]
    }
    function test_invalidInput(data) { compare(Geometry.desktopRect(data.g, entry(data.b)), null) }
    function test_allDecorationClipped() { verify(content.clip) }
    function test_unusedOutlineDoesNotAllocateEightTextItems() {
        content.entry = null
        const layers = findChild(content, "inplaceOutlineLayers")
        compare(layers.count, 0)
        content.entry = { outline: true, outline_width: 3, text: "test", font_family: "Noto Sans",
            font_px: 16, font_weight: 400, line_height: 1, letter_spacing: 0, inner: [0,0,0,0],
            alignment: "left", wrap: "word", text_color: "white", outline_color: "black", background: {mode:"transparent"} }
        compare(layers.count, 8)
        const text = findChild(content, "inplaceText")
        content.entry = Object.assign({}, content.entry, {text: "updated"})
        compare(findChild(content, "inplaceText"), text)
        compare(layers.count, 8)
    }
    function test_mixedScriptAndHardBreaks() {
        content.entry = { text: "Hello Привет 日本語\nSecond line", font_family: "Noto Sans", font_px: 16,
            font_weight: 400, line_height: 1, inner: [0,0,0,0], alignment: "left", wrap: "word",
            letter_spacing: 0, text_color: "white", outline_color: "black", background: {mode:"transparent"} }
        const text = findChild(content, "inplaceText")
        compare(text.text, content.entry.text)
        verify(text.font.contextFontMerging)
        verify(text.font.kerning)
        tryVerify(() => text.lineCount >= 2)
        verify(text.clip)
    }
}
