import QtQuick
import QtQuick.Controls
import QtTest
import "../qml" as Lipa
import "SettingsFixture.js" as Fixture

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
        property string historyJson: JSON.stringify([{timestamp: 1700000000000, "capture.region": "Субтитры", original: "Hello", translation: "Привет"}])
        property string diagnosticsJson: JSON.stringify([{name: "Tesseract", state: "ready", detail: "5.5", instruction: "x"},
            {name: "GStreamer", state: "error", detail: "not found", instruction: "sudo pacman -S gstreamer"}])
        property bool diagnosticsBusy: false
        property string copied: ""
        property int patchCalls: 0
        property string pickedSource: ""
        function pickWindow() { pickedSource = JSON.parse(saved).capture.source }
        function defaultSettingsJson() { return JSON.stringify(Fixture.make({"appearance.window.font_size": 20, "translation_window.border_color": "#ff00ff", "translation_window.pinned_corner_radius": 0, hotkeys: {toggle: "Ctrl+Alt+P"},
            "capture.regions": [{id: "subtitles", name: "Субтитры", enabled: true, rect: null, recognition_language: "", target_language: "", engine: null, interval_ms: 500, debounce_ms: 400}]})) }
        function editableSettingsPaths() { return JSON.stringify(Fixture.paths()) }
        function refreshDiagnostics() {}
        function clearHistory() { historyJson = "[]" }
        function copyText(t) { copied = t }
        property string saved: JSON.stringify(Fixture.make({"recognition.language":"eng", "translation.target_language":"ru", "recognition.engine":"tesseract", "capture.source":"auto", "capture.frame_color":"#ff0000", hotkeys:{},
            "capture.regions": [{id: "subtitles", name: "Субтитры", enabled: true, rect: {x: 0, y: 0.7, w: 1, h: 0.3}, recognition_language: "", target_language: "", engine: null, interval_ms: 500, debounce_ms: 400},
                      {id: "dialogue", name: "Диалоги", enabled: false, rect: null, recognition_language: "jpn", target_language: "", engine: "paddleocr", interval_ms: 800, debounce_ms: 400}],
            "capture.active_region": "subtitles", "appearance.window.font_size": 22, "translation_window.border_color": "#00ff00", "translation_window.pinned_corner_radius": 0}))
        function settingsJson() { return saved }
        function missingLanguages(spec) { return "[]" }
        function applySettings(json) { saved = json }
        function applySettingsPatch(json) {
            patchCalls++
            saved = JSON.stringify(Fixture.patch(JSON.parse(saved), JSON.parse(json)))
            settingsState = saved
        }
        property string captureCapabilities: "{}"
        property var forgotten: []
        function forgetGameProfile(key) { forgotten = forgotten.concat([key]) }
        function refreshTesseract() {}
        function bundledFonts() { return JSON.stringify(["Inter", "PT Serif", "Roboto Slab", "JetBrains Mono", "Noto Sans CJK SC"]) }
    }
    Lipa.SettingsWindow { id: settings; controller: controller }

    function test_captureSelectionUsesTheJustEditedSource() {
        settings.reload()
        const previous = settings.current.capture.source
        settings.set("capture.source", "portal")
        findChild(settings, "selectCaptureWindow").clicked()
        compare(controller.pickedSource, "portal")
        settings.set("capture.source", previous)
        settings.apply()
    }

    function test_disabledDisplayChoiceExplainsReasonOnHover() {
        settings.show()
        findChild(settings, "settingsTabs").currentIndex = 3
        settings.set("display_mode", "inplace")
        controller.captureCapabilities = JSON.stringify({inplaceTranslation: {
            available: false, reason: "Нет глобальных координат окна", remedy: "Выберите KWin"}})
        const choice = findChild(settings, "displayInplace")
        const hint = findChild(settings.contentItem, "inplaceUnavailableHint")
        verify(!choice.enabled)
        verify(choice.checked, "the desired mode is preserved during fallback")
        verify(hint !== null)
        wait(60)
        const point = choice.mapToItem(settings.contentItem, choice.width / 2, choice.height / 2)
        mouseMove(settings.contentItem, point.x, point.y)
        tryVerify(() => hint.tooltipVisible)
        verify(hint.explanation.includes("Нет глобальных координат окна"))
        verify(hint.explanation.includes("Выберите KWin"))
        controller.captureCapabilities = JSON.stringify({inplaceTranslation: {
            available: false, reason: "Нет шрифта", remedy: "Измените язык"}})
        tryVerify(() => hint.explanation.includes("Нет шрифта"))
        controller.captureCapabilities = JSON.stringify({inplaceTranslation: {available: true, reason: "", remedy: ""}})
        tryVerify(() => choice.enabled && !hint.visible)
        settings.set("display_mode", "window")
        settings.apply()
    }

    function test_appearanceResetAndSourceLanguageAreIndependent() {
        settings.reload()
        settings.set("appearance.window.font_size", 33)
        settings.setInplace("shadow", true)
        settings.set("display_mode", "inplace")
        settings.resetKeys(["appearance.window"])
        compare(settings.current.appearance.window.font_size, 20)
        verify(settings.current.appearance.inplace.shadow)
        compare(settings.current.display_mode, "inplace")
        const before = settings.current.recognition.language
        const source = findChild(settings, "translationSourceLanguage")
        source.currentIndex = 2
        source.activated(2)
        compare(settings.current.translation.source_language.mode, "explicit")
        compare(settings.current.translation.source_language.language, "en")
        compare(settings.current.recognition.language, before)
        source.currentIndex = 0
        source.activated(0)
        compare(settings.current.translation.source_language.mode, "recognition_language")
        settings.set("display_mode", "window")
        settings.apply()
    }

    function test_topTabsScrollAndAcceptKeyboardNavigation() {
        settings.show()
        settings.width = 740
        settings.height = 540
        const tabs = findChild(settings, "settingsTabs")
        tabs.currentIndex = 0
        tabs.itemAt(0).forceActiveFocus()
        keyClick(Qt.Key_Right)
        tryCompare(tabs, "currentIndex", 1)
        tabs.currentIndex = 8
        tryVerify(() => tabs.contentItem.contentX > 0, 2000, "the last tab scrolls into view")
        tabs.currentIndex = 0
    }

    function test_manualInplacePropertyUsesOnlyItsOwnAppearance() {
        settings.show()
        findChild(settings, "settingsTabs").currentIndex = 5
        settings.set("display_mode", "inplace")
        settings.setProp("font_size", false, 20)
        const editor = findChild(settings, "inplaceSettings")
        function findRow(item) {
            if (item.key === "font_size" && item.defaultValue !== undefined) return item
            for (const child of item.children || []) { const found = findRow(child); if (found) return found }
            return null
        }
        const row = findRow(editor)
        verify(row !== null)
        const mode = row.children[0]
        mode.currentIndex = 1
        mode.activated(1)
        verify(settings.propManual("font_size"))
        compare(settings.current.appearance.inplace.font_size.value, 20)
        verify(settings.pendingPatch()["appearance.inplace.font_size"] !== undefined)
        settings.set("display_mode", "window")
        settings.apply()
    }

    function test_regionModel() {
        settings.reload()
        const enabled = () => settings.current.capture.regions.filter(r => r.enabled).map(r => r.id)
        compare(settings.current.capture.regions.length, 2)
        verify(settings.addRegion(), "third region is created")
        compare(settings.current.capture.regions.length, 3)
        verify(!settings.current.capture.regions[2].enabled, "a new region starts inactive")
        verify(!settings.addRegion(), "fourth region is refused")
        compare(settings.current.capture.regions.length, 3)
        verify(settings.regionNotice.length > 0, "the user is told why")

        settings.activateRegion(1, true)
        compare(enabled(), ["dialogue"], "activating one deactivates the previous")
        compare(settings.current.capture.active_region, "dialogue")

        settings.setAllowMultipleRegions(true)
        settings.activateRegion(0, true)
        settings.activateRegion(2, true)
        compare(enabled().length, 3, "up to three active when allowed")

        settings.setAllowMultipleRegions(false)
        compare(enabled(), [settings.current.capture.active_region], "turning the option off keeps only the active one")
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
        compare(tabs.itemAt(0).text, "Источник изображения")
        compare(tabs.itemAt(6).text, "Клавиши")
        compare(tabs.itemAt(7).text, "Статус")
        compare(tabs.itemAt(8).text, "О программе")
        for (let i = 0; i < 9; ++i) {
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
        settings.resetKeys(["appearance.window.font_size", "translation_window.border_color"])
        compare(settings.current.appearance.window.font_size, 20)
        compare(settings.current.translation_window.border_color, "#ff00ff")
        compare(settings.current.capture.regions.length, 2)
        settings.setRegionField(1, "enabled", true)
        verify(settings.current.capture.regions[1].enabled)
        compare(settings.history.length, 1)
        compare(settings.diagnostics.length, 2)
        settings.set("recognition.engine", "paddleocr")
        compare(settings.current.recognition.engine, "paddleocr")
        compare(settings.langModel.length, 12)
        settings.apply()
        compare(JSON.parse(controller.saved).recognition.engine, "paddleocr")
        tabs.currentIndex = 3
        settings.width = 880
        settings.height = 740
        wait(100)
        findChild(settings, "settingsPage3").grabToImage(function(result) { result.saveToFile("/tmp/lipa-settings-qa.png") })
        wait(150)
        settings.close()
    }

    function visualFind(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) { const f = visualFind(child, name); if (f) return f }
        return null
    }

    function test_appearanceFollowsTheChosenDisplayWayWithoutASecondChoice() {
        settings.reload()
        settings.show()
        const tabs = findChild(settings, "settingsTabs")
        tabs.currentIndex = 5
        const page = findChild(settings, "settingsPage5")
        // The duplicate selector is gone: the way is chosen once, in "Отображение перевода".
        verify(visualFind(page.contentItem, "appearanceContext") === null)
        settings.set("display_mode", "window")
        compare(settings.appearanceMode, "window")
        const note = visualFind(page.contentItem, "appearanceModeNote")
        verify(note !== null)
        verify(note.text.indexOf("«В отдельном окне»") >= 0 && note.text.indexOf("«Отображение перевода»") >= 0, note.text)
        settings.set("display_mode", "inplace")
        compare(settings.appearanceMode, "inplace")
        verify(note.text.indexOf("«Поверх исходного текста»") >= 0, note.text)
        wait(50)
        verify(findChild(settings, "inplaceSettings").visible)
        verify(!findChild(settings, "windowBackgroundStyle").visible)
        // The way can be changed from here: the button leads to the tab where it is chosen.
        const change = visualFind(page.contentItem, "appearanceChangeMode")
        verify(change !== null)
        change.clicked()
        compare(tabs.currentIndex, 3)
        settings.set("display_mode", "window")
        settings.apply()
    }

    function test_bothDisplayWaysAreDescribedAndHaveHints() {
        settings.reload()
        settings.show()
        findChild(settings, "settingsTabs").currentIndex = 3
        const page = findChild(settings, "settingsPage3")
        const windowDescription = visualFind(page.contentItem, "displayWindowDescription")
        const inplaceDescription = visualFind(page.contentItem, "displayInplaceDescription")
        verify(windowDescription !== null && inplaceDescription !== null)
        // Both are always on screen, whichever way is chosen: one can compare them before choosing.
        for (const mode of ["window", "inplace"]) {
            settings.set("display_mode", mode)
            wait(30)
            verify(windowDescription.visible && inplaceDescription.visible, mode)
        }
        verify(windowDescription.text.indexOf("отдельном окне") >= 0, windowDescription.text)
        verify(inplaceDescription.text.indexOf("поверх исходного текста") >= 0, inplaceDescription.text)
        // The same words are the hover hints of the choices.
        const windowChoice = visualFind(page.contentItem, "displayWindow")
        const inplaceChoice = visualFind(page.contentItem, "displayInplace")
        compare(windowChoice.ToolTip.text, windowDescription.text)
        compare(inplaceChoice.ToolTip.text, inplaceDescription.text)
        settings.set("display_mode", "window")
        settings.apply()
    }

    function test_inplaceSettingsDoNotShowWindowControls() {
        settings.reload()
        settings.show()
        findChild(settings, "settingsTabs").currentIndex = 5
        compare(settings.inplaceBackgrounds.length, 4)
        settings.set("display_mode", "inplace")
        wait(50)
        verify(findChild(settings, "inplaceSettings").visible)
        verify(!findChild(settings, "windowBackgroundStyle").visible)
        settings.set("display_mode", "window")
        wait(50)
        verify(!findChild(settings, "inplaceSettings").visible)
        verify(findChild(settings, "windowBackgroundStyle").visible)
    }

    function test_appearanceEditKeepsNewCaptureState() {
        settings.reload()
        settings.set("appearance.window.font_size", 31)
        settings.set("translation_window.pinned_corner_radius", 18)
        const newer = JSON.parse(controller.saved)
        newer.capture.source = "portal"
        newer.capture.regions[0].rect = {x: 0.2, y: 0.5, w: 0.6, h: 0.3}
        controller.saved = JSON.stringify(newer)
        controller.settingsState = controller.saved
        tryVerify(() => settings.current.appearance.window.font_size === 31)
        compare(settings.current.translation_window.pinned_corner_radius, 18)
        compare(settings.current.capture.source, "portal")
        compare(settings.current.capture.regions[0].rect.x, 0.2)
        settings.resetKeys(["translation_window.pinned_corner_radius"])
        compare(settings.current.translation_window.pinned_corner_radius, 0)
        settings.apply()
        settings.reload()
    }

    function test_pinnedRadiusIsVisibleAndEditsTheSavedSetting() {
        settings.show()
        settings.reload()
        const tabs = findChild(settings, "settingsTabs")
        tabs.currentIndex = 4
        const page = findChild(settings, "settingsPage4")
        const control = findChild(settings, "pinnedCornerRadius")
        verify(control !== null)
        const pos = control.mapToItem(page, 0, 0)
        verify(pos.y >= 0 && pos.y + control.height <= page.height,
               "the pinned radius is visible without scrolling the Window tab")
        settings.set("translation_window.pinned_corner_radius", 0)
        mouseClick(control, control.width - 12, control.height / 4)
        compare(settings.current.translation_window.pinned_corner_radius, 1)
        settings.apply()
        compare(JSON.parse(controller.saved).translation_window.pinned_corner_radius, 1)

        verify(findChild(settings, "pinnedCornerRadiusAppearance") === null, "window geometry belongs only to the Window tab")
        settings.set("translation_window.pinned_corner_radius", 0)
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
        findChild(settings, "settingsTabs").currentIndex = 0
        settings.current = Fixture.patch(settings.current, {
            "capture.window": {uuid: "u", resource_class: "Witcher3.exe", caption: "The Witcher 3"},
            game_profiles: {
                "witcher3.exe": {caption: "The Witcher 3", capture: {regions: [{id: "subtitles", rect: {x: 0, y: 0.7, w: 1, h: 0.3}}]}},
                "other": {caption: "Other", capture: {regions: []}}}})
        compare(settings.gameProfileKeys.length, 2)
        compare(settings.currentGameKey, "witcher3.exe")
        const page = findChild(settings, "settingsPage0")
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
        settings.set("display_mode", "inplace")
        controller.captureCapabilities = "{}"

        tabs.currentIndex = 0
        tryVerify(() => note("settingsPage0", "frameBlockerNote") !== null)
        verify(!note("settingsPage0", "frameBlockerNote").visible, "nothing is blocked while the backend can do everything")
        tabs.currentIndex = 3
        tryVerify(() => note("settingsPage3", "inplaceBlockerNote") !== null)
        verify(!note("settingsPage3", "inplaceBlockerNote").visible)

        // The backend of the chosen window (the portal one) can do neither.
        controller.captureCapabilities = JSON.stringify({window_geometry: false, inplace_translation: false,
            frameBlocker: "положение окна на экране неизвестно, рамка не показывается",
            inplaceTranslation: {available: false, reason: "положение окна на экране неизвестно, перевод показывается в окне перевода", remedy: "Выберите KWin"}})
        const inplaceNote = note("settingsPage3", "inplaceBlockerNote")
        tryVerify(() => inplaceNote.visible)
        verify(inplaceNote.text.indexOf("в окне перевода") >= 0)
        tabs.currentIndex = 0
        const frameNote = note("settingsPage0", "frameBlockerNote")
        tryVerify(() => frameNote.visible)
        verify(frameNote.text.indexOf("рамка не показывается") >= 0)

        // Nothing to warn about while the translation window is chosen.
        settings.set("display_mode", "window")
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
        settings.set("recognition.engine", "tesseract")
        compare(box.currentIndex, 0)
        settings.set("recognition.engine", "auto")
        compare(box.currentIndex, 2)
        verify(settings.usesPaddle, "auto may call PaddleOCR: its Python is configurable")
        box.currentIndex = 1
        box.activated(1)
        compare(settings.current.recognition.engine, "paddleocr")
        settings.set("recognition.engine", "tesseract")
        verify(!settings.usesPaddle)

        const spin = findChild(settings, "ocrMinConfidence")
        verify(spin !== null)
        compare(spin.value, 30, "default threshold")
        spin.value = 55
        spin.valueModified()
        compare(settings.current.recognition.minimum_confidence, 55)
        settings.apply()
        compare(JSON.parse(controller.saved).recognition.minimum_confidence, 55)
        settings.set("recognition.minimum_confidence", 30)
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
        const shrink = findChild(settings, "translationWindowAutoShrink")
        verify(shrink !== null)
        settings.set("translation_window.auto_shrink", false)
        settings.apply()
        compare(JSON.parse(controller.saved).translation_window.auto_shrink, false)
        settings.set("translation_window.auto_shrink", true)
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
