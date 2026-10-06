import QtQuick
import QtTest
import "../qml" as Lipa

TestCase {
    name: "OcrPreviewWindow"
    when: windowShown

    QtObject {
        id: ctl
        property string ocrPreviewJson: "[]"
        property string patch: ""
        property string autotuneJson: ""
        property int tuneCalls: 0
        property bool enabled: false
        property int enableCalls: 0
        property int onceCalls: 0
        property string copied: ""
        function setOcrPreviewEnabled(on) { enabled = on; enableCalls++ }
        function translateOnce() { onceCalls++ }
        function copyText(text) { copied = text }
        function applySettingsPatch(json) { patch = json }
        function autoTuneFilters() { tuneCalls++ }
    }
    Lipa.OcrPreviewWindow { id: preview; controller: ctl }

    readonly property var subtitles: ({id: "subtitles", name: "Субтитры", width: 400, height: 100, image: "", phase: "ожидание кадра", timings: [],
        original: "Hello\nWorld", translation: "Привет\nМир",
        boxes: [{x: 10, y: 10, w: 100, h: 20, original: "Hello", translation: "Привет"},
                {x: 10, y: 50, w: 120, h: 20, original: "World", translation: "Мир"}]})

    function visualChild(item, name) {
        if (item.objectName === name) return item
        for (const c of item.children || []) {
            const found = visualChild(c, name)
            if (found) return found
        }
        return null
    }

    function test_frameIsRequestedOnlyWhileTheWindowIsOpen() {
        compare(ctl.enabled, false)
        preview.openWindow()
        tryCompare(ctl, "enabled", true)
        preview.close()
        tryCompare(ctl, "enabled", false)
    }

    function test_emptyStateExplainsWhatToDo() {
        preview.openWindow()
        verify(findChild(preview, "ocrPreviewEmpty").visible)
        verify(!preview.region)
        preview.close()
    }

    function test_blocksAreListedAndOutlined() {
        preview.openWindow()
        ctl.ocrPreviewJson = JSON.stringify([subtitles])
        compare(preview.boxes.length, 2)
        verify(!findChild(preview, "ocrPreviewEmpty").visible)
        compare(findChild(preview, "ocrBlockList").count, 2)
        tryVerify(() => visualChild(preview.contentItem, "ocrBox0") !== null)
        tryVerify(() => visualChild(preview.contentItem, "ocrBox1") !== null)
        preview.close()
        ctl.ocrPreviewJson = "[]"
    }

    function test_inspectorShowsTimingsPhaseAndDecisions() {
        preview.openWindow()
        const inspected = Object.assign({}, subtitles, {phase: "перевод показан",
            timings: [{stage: "capture", last_ms: 12, p50_ms: 10, p95_ms: 20, samples: 5}, {stage: "ocr", last_ms: 310, p50_ms: 290, p95_ms: 400, samples: 5}],
            boxes: [{x: 10, y: 10, w: 100, h: 20, original: "Hello", translation: "Привет", details: ["шрифт: Inter (Sans), уверенность 0.82", "фон: InpaintBlur"]}]})
        ctl.ocrPreviewJson = JSON.stringify([inspected])
        const timings = findChild(preview, "ocrPreviewTimings")
        verify(timings.text.indexOf("перевод показан") >= 0)
        verify(timings.text.indexOf("захват 12 / 10 / 20") >= 0, timings.text)
        verify(timings.text.indexOf("OCR 310 / 290 / 400") >= 0, timings.text)
        compare(findChild(preview, "ocrBlockList").count, 1)
        preview.close()
        ctl.ocrPreviewJson = "[]"
    }

    function test_wholeRegionTranslationIsShownWhenLinesCarryNone() {
        preview.openWindow()
        const lines = Object.assign({}, subtitles, {translation: "Привет\nМир", boxes: [
            {x: 10, y: 10, w: 100, h: 20, original: "Hello", translation: "", details: ["уверенность OCR (tesseract): 91%"]},
            {x: 10, y: 50, w: 120, h: 20, original: "World", translation: "", details: []}]})
        ctl.ocrPreviewJson = JSON.stringify([lines])
        const whole = findChild(preview, "ocrWholeTranslation")
        tryVerify(() => whole.visible)
        verify(whole.text.indexOf("Привет") >= 0)
        // Per-block translations (translation over the original) are already in the list.
        ctl.ocrPreviewJson = JSON.stringify([subtitles])
        tryVerify(() => !whole.visible)
        preview.close()
        ctl.ocrPreviewJson = "[]"
    }

    function test_refreshAndCopy() {
        preview.openWindow()
        ctl.ocrPreviewJson = JSON.stringify([subtitles])
        findChild(preview, "ocrPreviewRefresh").clicked()
        compare(ctl.onceCalls, 1)
        preview.close()
        ctl.ocrPreviewJson = "[]"
    }

    function test_severalRegionsGetTabs() {
        preview.openWindow()
        const second = Object.assign({}, subtitles, {id: "dialogue", name: "Диалоги", boxes: []})
        ctl.ocrPreviewJson = JSON.stringify([subtitles, second])
        compare(preview.regions.length, 2)
        preview.regionIndex = 1
        compare(preview.region.name, "Диалоги")
        compare(preview.boxes.length, 0)
        preview.regionIndex = 0
        preview.close()
        ctl.ocrPreviewJson = "[]"
    }

    function test_confidenceColoursAndFilteredFrame() {
        compare(preview.confidenceColor(90, 30), "#3fb950")
        compare(preview.confidenceColor(45, 30), "#f5a623")
        compare(preview.confidenceColor(10, 30), "#e5484d")
        compare(preview.confidenceColor(undefined, 30), "#00c8b4")
        preview.openWindow()
        const entry = JSON.parse(JSON.stringify(subtitles))
        entry.minimum_confidence = 30
        entry.filters = {binarize: true, auto_invert: false, sharpen: false, contrast: 0, filter_noise: true}
        entry.boxes[0].confidence = 12
        entry.image = "file:///raw.png"
        ctl.ocrPreviewJson = JSON.stringify([entry])
        verify(!findChild(preview, "ocrShowFiltered").enabled, "no filtered frame yet")
        verify(findChild(preview, "ocrFilterBinarize").checked)
        entry.filtered_image = "file:///filtered.png"
        ctl.ocrPreviewJson = JSON.stringify([entry])
        verify(findChild(preview, "ocrShowFiltered").enabled)
        compare(findChild(preview, "ocrPreviewImage").source.toString(), "file:///raw.png")
        preview.showFiltered = true
        compare(findChild(preview, "ocrPreviewImage").source.toString(), "file:///filtered.png")
        preview.showFiltered = false
        mouseClick(findChild(preview, "ocrFilterInvert"))
        compare(JSON.parse(ctl.patch), {"recognition.auto_invert": true})
        preview.close()
        ctl.ocrPreviewJson = "[]"
    }

    function test_autoTuneAsksForTheBestFilterAndShowsTheScores() {
        preview.openWindow()
        ctl.ocrPreviewJson = JSON.stringify([subtitles])
        const button = findChild(preview, "ocrAutoTune")
        verify(button.enabled)
        button.clicked()
        compare(ctl.tuneCalls, 1)
        verify(!button.enabled, "busy until the answer arrives")
        ctl.autotuneJson = JSON.stringify({region: "subtitles", best: 1, applied: true,
            results: [{name: "Без фильтров", score: 40}, {name: "Бинаризация", score: 170.4}]})
        verify(button.enabled)
        const label = findChild(preview, "ocrAutoTuneResult")
        verify(label.visible)
        verify(label.text.indexOf("Применено: Бинаризация") === 0, label.text)
        verify(label.text.indexOf("Без фильтров — 40") >= 0, label.text)
        preview.close()
        ctl.ocrPreviewJson = "[]"
        ctl.autotuneJson = ""
    }
}
