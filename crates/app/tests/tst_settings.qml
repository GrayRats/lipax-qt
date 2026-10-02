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
        property string saved: JSON.stringify({source_lang:"eng", target_lang:"ru", ocr_engine:"tesseract", capture_backend:"auto", frame_color:"#ff0000", hotkeys:{}})
        function settingsJson() { return saved }
        function missingLanguages(spec) { return "[]" }
        function applySettings(json) { saved = json }
        function refreshTesseract() {}
    }
    Lipa.SettingsWindow { id: settings; controller: controller }

    function test_layoutAndSave() {
        settings.show()
        settings.width = 740
        settings.height = 540
        const tabs = findChild(settings, "settingsTabs")
        verify(tabs !== null)
        for (let i = 0; i < 4; ++i) {
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
        settings.set("ocr_engine", "paddleocr")
        compare(settings.current.ocr_engine, "paddleocr")
        compare(settings.langModel.length, 12)
        settings.apply()
        compare(JSON.parse(controller.saved).ocr_engine, "paddleocr")
        tabs.currentIndex = 2
        settings.width = 880
        settings.height = 740
        wait(100)
        findChild(settings, "settingsPage2").grabToImage(function(result) { result.saveToFile("/tmp/lipa-settings-qa.png") })
        wait(150)
        settings.close()
    }
}
