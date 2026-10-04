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
        property int patchCalls: 0
        function defaultSettingsJson() { return JSON.stringify({font_size: 20, border_color: "#ff00ff", overlay_pinned_corner_radius: 0, hotkeys: {toggle: "Ctrl+Alt+P"},
            regions: [{id: "subtitles", name: "Субтитры", enabled: true, rect: null, source_lang: "", target_lang: "", ocr_engine: "", interval_ms: 500, debounce_ms: 400}]}) }
        function refreshDiagnostics() {}
        function clearHistory() { historyJson = "[]" }
        function copyText(t) { copied = t }
        property string saved: JSON.stringify({source_lang:"eng", target_lang:"ru", ocr_engine:"tesseract", capture_backend:"auto", frame_color:"#ff0000", hotkeys:{},
            regions: [{id: "subtitles", name: "Субтитры", enabled: true, rect: {x: 0, y: 0.7, w: 1, h: 0.3}, source_lang: "", target_lang: "", ocr_engine: "", interval_ms: 500, debounce_ms: 400},
                      {id: "dialogue", name: "Диалоги", enabled: false, rect: null, source_lang: "jpn", target_lang: "", ocr_engine: "paddleocr", interval_ms: 800, debounce_ms: 400}],
            active_region: "subtitles", font_size: 22, border_color: "#00ff00", overlay_pinned_corner_radius: 0})
        function settingsJson() { return saved }
        function missingLanguages(spec) { return "[]" }
        function applySettings(json) { saved = json }
        function applySettingsPatch(json) {
            patchCalls++
            saved = JSON.stringify(Object.assign({}, JSON.parse(saved), JSON.parse(json)))
            settingsState = saved
        }
        property string captureCapabilities: "{}"
        property var forgotten: []
        function forgetGameProfile(key) { forgotten = forgotten.concat([key]) }
        function refreshTesseract() {}
        function bundledFonts() { return JSON.stringify(["Inter", "PT Serif", "Roboto Slab", "JetBrains Mono", "Noto Sans CJK SC"]) }
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

    function test_inplaceSettingsDoNotShowWindowControls() {
        settings.reload()
        settings.show()
        findChild(settings, "settingsTabs").currentIndex = 3
        compare(settings.inplaceBackgrounds.length, 4)
        settings.set("translation_display", "inplace")
        wait(50)
        verify(findChild(settings, "inplaceSettings").visible)
        verify(!findChild(settings, "overlayStyle").visible)
        settings.set("translation_display", "window")
        wait(50)
        verify(!findChild(settings, "inplaceSettings").visible)
        verify(findChild(settings, "overlayStyle").visible)
    }

    function test_appearanceEditKeepsNewCaptureState() {
        settings.reload()
        settings.set("font_size", 31)
        settings.set("overlay_pinned_corner_radius", 18)
        const newer = JSON.parse(controller.saved)
        newer.capture_backend = "portal"
        newer.regions[0].rect = {x: 0.2, y: 0.5, w: 0.6, h: 0.3}
        controller.saved = JSON.stringify(newer)
        controller.settingsState = controller.saved
        tryCompare(settings.current, "font_size", 31)
        compare(settings.current.overlay_pinned_corner_radius, 18)
        compare(settings.current.capture_backend, "portal")
        compare(settings.current.regions[0].rect.x, 0.2)
        settings.resetKeys(["overlay_pinned_corner_radius"])
        compare(settings.current.overlay_pinned_corner_radius, 0)
        settings.apply()
        settings.reload()
    }

    function test_pinnedRadiusIsVisibleAndEditsTheSavedSetting() {
        settings.show()
        settings.reload()
        const tabs = findChild(settings, "settingsTabs")
        tabs.currentIndex = 2
        const page = findChild(settings, "settingsPage2")
        const control = findChild(settings, "pinnedCornerRadius")
        verify(control !== null)
        const pos = control.mapToItem(page, 0, 0)
        verify(pos.y >= 0 && pos.y + control.height <= page.height,
               "the pinned radius is visible without scrolling the Window tab")
        settings.set("overlay_pinned_corner_radius", 0)
        mouseClick(control, control.width - 12, control.height / 4)
        compare(settings.current.overlay_pinned_corner_radius, 1)
        settings.apply()
        compare(JSON.parse(controller.saved).overlay_pinned_corner_radius, 1)

        tabs.currentIndex = 3
        const appearanceControl = findChild(settings, "pinnedCornerRadiusAppearance")
        verify(appearanceControl !== null)
        compare(appearanceControl.value, 1, "both tabs edit the same setting")
        settings.set("overlay_pinned_corner_radius", 0)
        settings.apply()
    }

    function test_closeToTraySwitchSavesTheSetting() {
        settings.show()
        settings.reload()
        findChild(settings, "settingsTabs").currentIndex = 1
        const toggle = findChild(settings, "closeToTraySwitch")
        verify(toggle !== null)
        compare(toggle.checked, false, "off by default: closing the window quits")
        toggle.toggle()
        toggle.toggled()
        compare(settings.current.close_to_tray, true)
        settings.apply()
        compare(JSON.parse(controller.saved).close_to_tray, true)
        settings.set("close_to_tray", false)
        settings.apply()
    }

    function test_gameProfilesAreListedAndTheSelectedOneCannotBeForgotten() {
        settings.show()
        settings.reload()
        findChild(settings, "settingsTabs").currentIndex = 1
        settings.current = Object.assign({}, settings.current, {
            window: {uuid: "u", resource_class: "Witcher3.exe", caption: "The Witcher 3"},
            game_profiles: {
                "witcher3.exe": {caption: "The Witcher 3", regions: [{id: "subtitles", rect: {x: 0, y: 0.7, w: 1, h: 0.3}}]},
                "other": {caption: "Other", regions: []}}})
        compare(settings.gameProfileKeys.length, 2)
        compare(settings.currentGameKey, "witcher3.exe")
        const page = findChild(settings, "settingsPage1")
        function find(item, name) {
            if (item.objectName === name) return item
            for (const c of item.children || []) { const f = find(c, name); if (f) return f }
            return null
        }
        tryVerify(() => find(page.contentItem, "forgetGame_other") !== null)
        const other = find(page.contentItem, "forgetGame_other")
        const selected = find(page.contentItem, "forgetGame_witcher3.exe")
        verify(other.enabled)
        verify(!selected.enabled, "the game being played is saved again at once")
        other.clicked()
        compare(controller.forgotten.length, 1)
        compare(controller.forgotten[0], "other")
        settings.reload()
    }

    function test_unsupportedCaptureFeaturesAreExplained() {
        settings.show()
        settings.reload()
        const tabs = findChild(settings, "settingsTabs")
        function find(item, name) {
            if (item.objectName === name) return item
            for (const c of item.children || []) { const f = find(c, name); if (f) return f }
            return null
        }
        function note(page, name) { return find(findChild(settings, page).contentItem, name) }
        settings.set("translation_display", "inplace")
        controller.captureCapabilities = "{}"

        tabs.currentIndex = 2
        tryVerify(() => note("settingsPage2", "frameBlockerNote") !== null)
        verify(!note("settingsPage2", "frameBlockerNote").visible, "nothing is blocked while the backend can do everything")
        tabs.currentIndex = 3
        tryVerify(() => note("settingsPage3", "inplaceBlockerNote") !== null)
        verify(!note("settingsPage3", "inplaceBlockerNote").visible)

        // The backend of the chosen window (the portal one) can do neither.
        controller.captureCapabilities = JSON.stringify({window_geometry: false, inplace_overlay: false,
            frameBlocker: "положение окна на экране неизвестно, рамка не показывается",
            inplaceBlocker: "положение окна на экране неизвестно, перевод показывается в окне перевода"})
        const inplaceNote = note("settingsPage3", "inplaceBlockerNote")
        tryVerify(() => inplaceNote.visible)
        verify(inplaceNote.text.indexOf("в окне перевода") >= 0)
        tabs.currentIndex = 2
        const frameNote = note("settingsPage2", "frameBlockerNote")
        tryVerify(() => frameNote.visible)
        verify(frameNote.text.indexOf("рамка не показывается") >= 0)

        // Nothing to warn about while the translation window is chosen.
        settings.set("translation_display", "window")
        tabs.currentIndex = 3
        tryVerify(() => !inplaceNote.visible)
        controller.captureCapabilities = "{}"
        settings.apply()
    }

    function test_ocrEngineAutoAndConfidenceThreshold() {
        settings.show()
        settings.reload()
        findChild(settings, "settingsTabs").currentIndex = 0
        const box = findChild(settings, "ocrEngineBox")
        verify(box !== null)
        compare(box.count, 3)
        settings.set("ocr_engine", "tesseract")
        compare(box.currentIndex, 0)
        settings.set("ocr_engine", "auto")
        compare(box.currentIndex, 2)
        verify(settings.usesPaddle, "auto may call PaddleOCR: its Python is configurable")
        box.currentIndex = 1
        box.activated(1)
        compare(settings.current.ocr_engine, "paddleocr")
        settings.set("ocr_engine", "tesseract")
        verify(!settings.usesPaddle)

        const spin = findChild(settings, "ocrMinConfidence")
        verify(spin !== null)
        compare(spin.value, 30, "default threshold")
        spin.value = 55
        spin.valueModified()
        compare(settings.current.ocr_min_confidence, 55)
        settings.apply()
        compare(JSON.parse(controller.saved).ocr_min_confidence, 55)
        settings.set("ocr_min_confidence", 30)
        settings.apply()
    }

    function test_gameProfilesSwitchSavesTheSetting() {
        settings.show()
        settings.reload()
        findChild(settings, "settingsTabs").currentIndex = 1
        const toggle = findChild(settings, "gameProfilesSwitch")
        verify(toggle !== null)
        compare(toggle.checked, true, "on by default")
        settings.set("game_profiles_enabled", false)
        settings.apply()
        compare(JSON.parse(controller.saved).game_profiles_enabled, false)
        settings.set("game_profiles_enabled", true)
        settings.apply()
    }

    function test_appearanceOffersOnlyBundledFontsAndSavesAutoShrink() {
        settings.show()
        settings.reload()
        const tabs = findChild(settings, "settingsTabs")
        tabs.currentIndex = 3
        const fontPicker = findChild(settings, "translationFontFamily")
        compare(fontPicker.count, 5)
        compare(fontPicker.textAt(0), "Inter")
        verify(settings.fontFamilies.every(name => JSON.parse(controller.bundledFonts()).includes(name)))
        verify(!settings.fontFamilies.includes("Системный"))
        const shrink = findChild(settings, "overlayAutoShrink")
        verify(shrink !== null)
        settings.set("overlay_auto_shrink", false)
        settings.apply()
        compare(JSON.parse(controller.saved).overlay_auto_shrink, false)
        settings.set("overlay_auto_shrink", true)
        settings.apply()
    }

    function test_reloadDoesNotScheduleAnUnchangedSave() {
        settings.apply()
        const before = controller.patchCalls
        settings.reload()
        wait(650)
        compare(controller.patchCalls, before)
    }
}
