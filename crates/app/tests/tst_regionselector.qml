import QtQuick
import QtTest
import "../qml" as Lipa

TestCase {
    name: "RegionSelector"
    when: windowShown

    QtObject {
        id: ctl
        property string previewSource: ""
        property int previewTitleBar: 0
        property var saved: null
        signal previewReady()
        function requestPreview() { previewReady() }
        function setRegion(x, y, w, h) { saved = [x, y, w, h] }
    }
    Lipa.RegionSelector { id: selector; controller: ctl }

    function visualChild(item, name) {
        if (item.objectName === name) return item
        for (const c of item.children || []) { const f = visualChild(c, name); if (f) return f }
        return null
    }

    function test_noHintWhenTheWindowHasNoTitleBarOfItsOwn() {
        ctl.previewTitleBar = 0
        selector.begin()
        compare(selector.barPx, 0)
        verify(!selector.overlapsBar)
        verify(!visualChild(selector.contentItem, "titleBarNote").visible)
        selector.close()
    }

    function test_theBarIsHighlightedAndASelectionOverItIsFlagged() {
        ctl.previewTitleBar = 40
        selector.begin()
        compare(selector.barPx, 40)
        const note = visualChild(selector.contentItem, "titleBarNote")
        verify(note.visible)
        verify(note.text.indexOf("выделяйте область ниже") >= 0, note.text)
        verify(!selector.overlapsBar, "nothing selected yet")
        // Without a loaded picture one frame pixel is one pixel of the picture.
        selector.selX = 10; selector.selY = 5; selector.selW = 200; selector.selH = 100
        verify(selector.overlapsBar)
        verify(note.text.indexOf("захватывает собственный заголовок") >= 0, note.text)
        const button = visualChild(selector.contentItem, "belowTitleBar")
        verify(button.visible)
        button.clicked()
        compare(selector.selY, selector.barHeight)
        compare(selector.selY + selector.selH, 105, "the bottom edge stays where it was")
        verify(!selector.overlapsBar)
        selector.close()
        ctl.previewTitleBar = 0
    }
}
