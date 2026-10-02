import QtQuick
import QtQuick.Controls
import QtTest
import "../qml" as Lipa

TestCase {
    id: test
    name: "SettingsLayout"
    when: windowShown
    width: 900
    height: 800

    QtObject {
        id: controller
        property string tesseractJson: JSON.stringify({installed: true, distro: {name: "Arch Linux"},
            version: "5.5", path: "/usr/bin/tesseract", languages: [{code:"eng", name:"English"}, {code:"jpn",name:"Japanese"}], installable: []})
        property bool tesseractBusy: false
        property bool hasRegion: false
        property string windowTitle: "Game"
        property string settingsState: ""
        property string historyJson: JSON.stringify([{timestamp: 1700000000000, region: "Субтитры", original: "Hello", translation: "Привет"}])
        property string diagnosticsJson: JSON.stringify([{name: "Tesseract", state: "ready", detail: "5.5", instruction: "x"},
            {name: "GStreamer", state: "error", detail: "not found", instruction: "sudo pacman -S gstreamer"}])
        property bool diagnosticsBusy: false
        property string copied: ""
        function defaultSettingsJson() { return JSON.stringify({font_size: 20, border_color: "#ff00ff", hotkeys: {toggle: "Ctrl+Alt+P"},
            regions: [{id: "subtitles", name: "Субтитры", enabled: true, rect: null, source_lang: "", target_lang: "", ocr_engine: "", interval_ms: 500, debounce_ms: 400}]}) }
        function refreshDiagnostics() {}
        function clearHistory() { historyJson = "[]" }
        function copyText(t) { copied = t }
        property string saved: JSON.stringify({source_lang:"eng", target_lang:"ru", ocr_engine:"tesseract", capture_backend:"auto", frame_color:"#ff0000", hotkeys:{},
            regions: [{id: "subtitles", name: "Субтитры", enabled: true, rect: {x: 0, y: 0.7, w: 1, h: 0.3}, source_lang: "", target_lang: "", ocr_engine: "", interval_ms: 500, debounce_ms: 400},
                      {id: "dialogue", name: "Диалоги", enabled: false, rect: null, source_lang: "jpn", target_lang: "", ocr_engine: "paddleocr", interval_ms: 800, debounce_ms: 400}],
            active_region: "subtitles", font_size: 22, border_color: "#00ff00"})
        function settingsJson() { return saved }
        function missingLanguages(spec) { return "[]" }
        function applySettings(json) { saved = json }
        function refreshTesseract() {}
    }
    Lipa.SettingsWindow { id: settings; controller: controller }

    function test_regionModel() {
        settings.reload()
        const enabled = () => settings.current.regions.filter(r => r.enabled).map(r => r.id)
        compare(settings.current.regions.length, 2)
        verify(settings.addRegion(), "third region is created")
        compare(settings.current.regions.length, 3)
        verify(!settings.current.regions[2].enabled, "a new region starts inactive")
        verify(!settings.addRegion(), "fourth region is refused")
        compare(settings.current.regions.length, 3)
        verify(settings.regionNotice.length > 0, "the user is told why")

        settings.activateRegion(1, true)
        compare(enabled(), ["dialogue"], "activating one deactivates the previous")
        compare(settings.current.active_region, "dialogue")

        settings.setAllowMultipleRegions(true)
        settings.activateRegion(0, true)
        settings.activateRegion(2, true)
        compare(enabled().length, 3, "up to three active when allowed")

        settings.setAllowMultipleRegions(false)
        compare(enabled(), [settings.current.active_region], "turning the option off keeps only the active one")
        settings.removeRegion(2)
        verify(settings.addRegion(), "after removing one, a region can be added again")
        settings.reload()
    }

    function test_layoutAndSave() {
        settings.show()
        settings.width = 740
        settings.height = 540
        const tabs = findChild(settings, "settingsTabs")
        verify(tabs !== null)
        compare(tabs.itemAt(4).text, "Область")
        compare(tabs.itemAt(5).text, "Клавиши")
        compare(tabs.itemAt(6).text, "Статус")
        compare(tabs.itemAt(7).text, "О программе")
        for (let i = 0; i < 8; ++i) {
            tabs.currentIndex = i
            wait(100)
            const page = findChild(settings, "settingsPage" + i)
            verify(page.contentWidth <= page.availableWidth + 1, "page " + i + " overflows horizontally")
            const grid = page.contentChildren[0]
            verify(grid.width <= page.availableWidth, "form fits viewport")
            for (const child of grid.children) {
                if (child.visible && child.width > 0)
                    verify(child.x >= 0 && child.x + child.width <= grid.width + 1, "control fits page " + i)
            }
        }
        // Reset restores defaults for listed keys only.
        settings.resetKeys(["font_size", "border_color"])
        compare(settings.current.font_size, 20)
        compare(settings.current.border_color, "#ff00ff")
        compare(settings.current.regions.length, 2)
        settings.setRegionField(1, "enabled", true)
        verify(settings.current.regions[1].enabled)
        compare(settings.history.length, 1)
        compare(settings.diagnostics.length, 2)
        settings.set("ocr_engine", "paddleocr")
        compare(settings.current.ocr_engine, "paddleocr")
        compare(settings.langModel.length, 12)
        settings.apply()
        compare(JSON.parse(controller.saved).ocr_engine, "paddleocr")
        tabs.currentIndex = 3
        settings.width = 880
        settings.height = 740
        wait(100)
        findChild(settings, "settingsPage3").grabToImage(function(result) { result.saveToFile("/tmp/lipa-settings-qa.png") })
        wait(150)
        settings.close()
    }
}
