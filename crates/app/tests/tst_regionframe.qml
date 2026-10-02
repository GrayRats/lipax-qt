import QtQuick
import QtTest
import "../qml" as Lipa

TestCase {
    name: "RegionFrame"
    when: windowShown

    property var base: ({ frame_seconds: 1, frame_width: 2, frame_color: "#ff0000", region_frame_pinned: "dim" })
    Lipa.RegionFrame {
        id: subtitles
        flashOnCreate: false
        gameGeometry: "[100,50,1000,500]"
        region: ({ id: "subtitles", enabled: true, rect: { x: 0, y: 0.7, w: 1, h: 0.3 }, frame_mode: "" })
        settings: Object.assign({}, base, { region_frame_mode: "selection" })
    }
    Lipa.RegionFrame {
        id: dialogue
        flashOnCreate: false
        gameGeometry: "[100,50,1000,500]"
        region: ({ id: "dialogue", enabled: true, rect: { x: 0.2, y: 0.1, w: 0.6, h: 0.2 }, frame_mode: "" })
        settings: Object.assign({}, base, { region_frame_mode: "selection" })
    }

    function content(f) { return findChild(f.contentItem, "regionFrameContent") }
    function setMode(f, mode, extra) { f.settings = Object.assign({}, base, { region_frame_mode: mode }, extra || ({})) }

    function init() {
        for (const f of [subtitles, dialogue]) { setMode(f, "selection"); f.pinned = false }
        // Let flashes started by the previous test run out.
        tryVerify(() => !subtitles.flashing && !dialogue.flashing, 3000)
        tryVerify(() => content(subtitles).opacity < 0.01 && content(dialogue).opacity < 0.01, 3000)
    }

    function test_geometryFollowsRegion() {
        compare(subtitles.desktopRect, [100, 400, 1000, 150])
        compare(dialogue.desktopRect, [300, 100, 600, 100])
    }

    function test_selectionFadesOutWithOwnTimer() {
        subtitles.flash()
        compare(subtitles.targetOpacity, 1)
        compare(dialogue.targetOpacity, 0, "the other region keeps its own state")
        tryVerify(() => subtitles.visible && content(subtitles).opacity > 0.9, 1000)
        // Fades out after frame_seconds, not abruptly.
        tryVerify(() => content(subtitles).opacity > 0.05 && content(subtitles).opacity < 0.95, 2500)
        tryVerify(() => !subtitles.visible, 2000)
    }

    function test_newSelectionFlashes() {
        dialogue.region = Object.assign({}, dialogue.region, { rect: { x: 0.1, y: 0.1, w: 0.5, h: 0.2 } })
        verify(dialogue.flashing, "changing the area flashes the outline")
        verify(!subtitles.flashing)
    }

    function test_persistentModes_data() {
        return [
            { tag: "pattern", mode: "pattern", shown: 1, dim: 0.35, hidden: 0 },
            { tag: "solid", mode: "solid", shown: 1, dim: 0.35, hidden: 0 },
            { tag: "off", mode: "off", shown: 0, dim: 0, hidden: 0 },
        ]
    }
    function test_persistentModes(data) {
        setMode(subtitles, data.mode)
        compare(subtitles.targetOpacity, data.shown)
        subtitles.pinned = true
        compare(subtitles.targetOpacity, data.dim, "pinned translation dims the outline")
        setMode(subtitles, data.mode, { region_frame_pinned: "hide" })
        compare(subtitles.targetOpacity, data.hidden, "or hides it")
        subtitles.flash()
        compare(subtitles.flashing, data.mode !== "off", "nothing flashes when the frame is off")
    }

    function test_perRegionOverride() {
        dialogue.region = Object.assign({}, dialogue.region, { frame_mode: "solid" })
        compare(dialogue.mode, "solid")
        compare(subtitles.mode, "selection")
        dialogue.region = Object.assign({}, dialogue.region, { frame_mode: "" })
    }
}
