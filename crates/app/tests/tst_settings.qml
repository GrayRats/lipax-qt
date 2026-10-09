import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Universal
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
        property string modelState: "{}"
        property bool modelBusy: false
        property int modelChecks: 0
        property string modelPath: ""
        function refreshModels() { modelChecks++ }
        function setModelPath(path) { modelPath = path }
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
        function defaultSettingsJson() { return JSON.stringify(Fixture.make({"general.log_level": "info", "general.autostart": false, "general.theme": "dark", "appearance.main_window.font_size": 16, "appearance.main_window.background": "gray", "appearance.main_window.font_family": "Inter", "appearance.main_window.cjk_font_family": "", "appearance.window.font_size": 20, "translation_window.border_color": "#ff00ff", "translation_window.pinned_corner_radius": 0, hotkeys: {toggle: "Ctrl+Alt+P"},
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
        function appVersion() { return "9.9.9" }
        property string paddleJson: ""
        property bool paddleBusy: false
        property int paddleChecks: 0
        function refreshPaddle() { paddleChecks++ }
        property var installCalls: []
        function installLanguage(code) { installCalls = installCalls.concat([["download", code]]) }
        function installPackage(pkg) { installCalls = installCalls.concat([["package", pkg]]) }
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
        tryVerify(() => hint.popupVisible, 2000, "the tooltip popup actually opens after hovering")
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

    function test_displayChoiceControlsAppearanceAndExplainsBothModes() {
        settings.show()
        settings.width = 880
        settings.height = 740
        const tabs = findChild(settings, "settingsTabs")
        tabs.currentIndex = 3
        controller.captureCapabilities = JSON.stringify({inplaceTranslation: {available: true, reason: "", remedy: ""}})
        verify(findChild(settings, "appearanceContext") === null)
        const windowChoice = findChild(settings, "displayWindow")
        const inplaceChoice = findChild(settings, "displayInplace")
        const windowDescription = findChild(settings, "windowDisplayDescription")
        const inplaceDescription = findChild(settings, "inplaceDisplayDescription")
        verify(windowDescription.visible && windowDescription.text.includes("перемещать мышью"))
        verify(inplaceDescription.visible && inplaceDescription.text.includes("координаты"))
        for (const entry of [
            {choice: windowChoice, hint: "windowDisplayHint", mode: "window"},
            {choice: inplaceChoice, hint: "inplaceDisplayHint", mode: "inplace"}
        ]) {
            tabs.currentIndex = 3
            wait(80)
            mouseMove(settings.contentItem, 1, 1)
            const point = entry.choice.mapToItem(settings.contentItem, entry.choice.width / 2, entry.choice.height / 2)
            mouseMove(settings.contentItem, point.x, point.y)
            const hint = findChild(settings.contentItem, entry.hint)
            tryVerify(() => hint.popupVisible, 2000)
            mouseClick(entry.choice, entry.choice.width / 2, entry.choice.height / 2)
            compare(settings.current.display_mode, entry.mode)
            tabs.currentIndex = 5
            compare(findChild(settings, "inplaceSettings").visible, entry.mode === "inplace")
            compare(findChild(settings, "appearanceModeTitle").text,
                    entry.mode === "inplace" ? "Поверх исходного текста" : "Отдельное окно перевода")
        }
        // A temporary backend limitation must not switch the saved mode or the style being edited.
        controller.captureCapabilities = JSON.stringify({inplaceTranslation: {available: false, reason: "Нет координат", remedy: "Выберите KWin"}})
        verify(findChild(settings, "inplaceSettings").visible)
        compare(settings.current.display_mode, "inplace")
        controller.captureCapabilities = JSON.stringify({inplaceTranslation: {available: true, reason: "", remedy: ""}})
        settings.set("display_mode", "window")
        settings.apply()
    }

    function test_captureActionsFitNarrowSettingsWindow() {
        settings.show()
        settings.width = 740
        settings.height = 540
        findChild(settings, "settingsTabs").currentIndex = 0
        function findActions(item) {
            if (item.objectName === "captureRegionActions") return item
            for (const child of item.children || []) {
                const found = findActions(child)
                if (found) return found
            }
            return null
        }
        const actions = findActions(settings.contentItem)
        verify(actions !== null)
        wait(100)
        verify(actions.width > 0)
        for (const button of actions.children) {
            if (button.visible && button.width > 0)
                verify(button.x >= 0 && button.x + button.width <= actions.width + 1,
                    "capture action fits the available width: " + button.text)
        }
    }

    function test_enabledSettingTooltipAndClicks() {
        settings.show()
        settings.width = 880
        settings.height = 740
        const tabs = findChild(settings, "settingsTabs")
        tabs.currentIndex = 1
        const page = findChild(settings, "settingsPage1")
        page.contentItem.contentY = 0
        const editor = findChild(settings, "ocrEngineBox")
        const hint = settings.contentItem.children.find(item => item.control === editor && item.active)
        verify(hint !== undefined)
        wait(80)
        mouseMove(settings.contentItem, 1, 1)
        const point = editor.mapToItem(settings.contentItem, editor.width / 2, editor.height / 2)
        mouseMove(settings.contentItem, point.x, point.y)
        tryVerify(() => hint.popupVisible, 2000)
        verify(hint.explanation.includes("Tesseract"))
        verify(hint.explanation.includes("PaddleOCR"))
        mouseClick(editor, editor.width / 2, editor.height / 2)
        tryVerify(() => editor.popup.visible, 2000, "hover help does not intercept clicks")
        keyClick(Qt.Key_Escape)
        page.contentItem.contentY = page.contentItem.contentHeight - page.availableHeight
        tryVerify(() => !hint.visible && !hint.popupVisible, 2000, "a scrolled-out control has no hover target")
        page.contentItem.contentY = 0
        tabs.currentIndex = 0
        tryVerify(() => !hint.visible && !hint.popupVisible)
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
        tabs.currentIndex = 10
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
        compare(tabs.itemAt(8).text, "Общие")
        compare(tabs.itemAt(9).text, "Приложение")
        compare(tabs.itemAt(10).text, "О программе")
        for (let i = 0; i < 11; ++i) {
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

    function paddleReport(ready, problems) {
        return JSON.stringify({ready: ready, summary: ready ? "PaddleOCR 3.0.1 · PaddlePaddle 3.0.0 · виртуальное окружение uv · язык «eng»" : "PaddleOCR не установлен в /usr/bin/python3",
            problems: problems})
    }

    function test_aboutTabAnimatesOnlyWhileShownAndTellsWhatLipaXIs() {
        settings.reload()
        settings.show()
        const tabs = findChild(settings, "settingsTabs")
        tabs.currentIndex = 0
        const page = findChild(settings, "settingsPage10")
        const icon = visualFind(page.contentItem, "aboutIcon")
        const ticker = visualFind(page.contentItem, "aboutTicker")
        verify(icon !== null && ticker !== null)
        // On another tab nothing moves: the icon rests in the middle.
        wait(200)
        compare(icon.y, 24)
        compare(icon.rotation, 0)
        // On its own tab the icon floats and sways and the line runs (the tab index is that of "О программе").
        tabs.currentIndex = 10
        tryVerify(() => icon.y < 20, 3000, "the icon floats")
        tryVerify(() => icon.rotation !== 0, 3000, "and sways")
        const x = ticker.x
        wait(300)
        verify(ticker.x < x, "the line runs: " + x + " -> " + ticker.x)
        // Leaving the tab stops it and puts the icon back.
        tabs.currentIndex = 0
        tryCompare(icon, "y", 24, 3000)
        tryCompare(icon, "rotation", 0, 3000)
        // What the program is, its version, author, licence and what it stands on.
        compare(visualFind(page.contentItem, "aboutVersion").text, "Версия 9.9.9")
        verify(visualFind(page.contentItem, "aboutDescription").text.indexOf("Переводчик игрового текста") >= 0)
        verify(visualFind(page.contentItem, "aboutDescription").text.indexOf("PaddleOCR") >= 0)
        verify(visualFind(page.contentItem, "aboutRepository").text.indexOf("github.com/GrayRats/lipax-qt") >= 0)
        const links = visualFind(page.contentItem, "aboutLinks")
        verify(links.count === 5, "five sources")
        let all = ""
        for (let i = 0; i < links.count; i++) all += links.itemAt(i).text
        for (const needed of ["tesseract-ocr/tesseract", "tessdata_fast", "PaddlePaddle/PaddleOCR", "meikipop", "satix-one/lipa"])
            verify(all.indexOf(needed) >= 0, needed)
    }

    function test_paddleIsCheckedByItselfAndTheFixesAreShown() {
        settings.reload()
        settings.show()
        findChild(settings, "settingsTabs").currentIndex = 1
        settings.set("recognition.engine", "tesseract")
        const panel = visualFind(findChild(settings, "settingsPage1").contentItem, "paddleCheck")
        verify(panel !== null)
        verify(!panel.visible, "Tesseract needs no check of PaddleOCR")
        controller.paddleChecks = 0
        settings.set("recognition.engine", "paddleocr")
        verify(panel.visible)
        tryVerify(() => controller.paddleChecks === 1, 3000, "choosing PaddleOCR starts the check")
        // A change of the Python or of the language checks again.
        settings.set("recognition.paddle_python", "/home/u/.local/share/lipa/paddle-venv/bin/python")
        tryVerify(() => controller.paddleChecks === 2, 3000)
        settings.set("recognition.language", "jpn")
        tryVerify(() => controller.paddleChecks === 3, 3000)
        // What is missing, in words, with the ways to fix it as commands that can be copied.
        controller.paddleJson = paddleReport(false, [{id: "paddleocr-missing", severity: "error", title: "PaddleOCR не установлен в /usr/bin/python3",
            detail: "Пакет paddleocr (3.x) не найден в этом Python. Выберите способ установки:", options: [
            {title: "uv (быстро, рекомендуется)", commands: ["uv venv ~/.local/share/lipa/paddle-venv", "uv pip install --python ~/.local/share/lipa/paddle-venv/bin/python 'paddlepaddle>=3,<4' 'paddleocr>=3,<4'"]},
            {title: "системные пакеты (deb, AUR, pkg)", commands: []}]},
            {id: "models-missing", severity: "info", title: "Моделей нет в кэше", detail: "Скачаются при первом запуске.", options: []}])
        const summary = visualFind(panel, "paddleSummary")
        verify(summary.text.indexOf("✗ ") === 0 && summary.text.indexOf("не установлен") >= 0, summary.text)
        tryVerify(() => visualFind(panel, "paddleProblem_paddleocr-missing") !== null)
        verify(visualFind(panel, "paddleProblem_models-missing") !== null)
        const copy = visualFind(panel, "paddleCopy")
        verify(copy.visible, "an option with commands can be copied")
        controller.copied = ""
        copy.clicked()
        verify(controller.copied.indexOf("uv venv ~/.local/share/lipa/paddle-venv\nuv pip install") === 0, controller.copied)
        // A working environment says so.
        controller.paddleJson = paddleReport(true, [])
        verify(summary.text.indexOf("✓ PaddleOCR 3.0.1") === 0, summary.text)
        // The button checks on demand; a check in progress says so.
        const before = controller.paddleChecks
        visualFind(panel, "paddleRecheck").clicked()
        compare(controller.paddleChecks, before + 1)
        controller.paddleBusy = true
        verify(summary.text.indexOf("Проверка окружения") === 0)
        controller.paddleBusy = false
        controller.paddleJson = ""
        settings.set("recognition.engine", "tesseract")
        settings.set("recognition.language", "eng")
        settings.set("recognition.paddle_python", "")
    }

    function test_languagesWithoutAPackageAreDownloadedWithoutAPassword() {
        const before = controller.tesseractJson
        controller.tesseractJson = JSON.stringify({installed: true, distro: {name: "CachyOS", family: "arch"}, package_manager: "pacman", version: "5.5",
            path: "/usr/bin/tesseract", languages: [{code: "eng", name: "English"}], installable: [
            {code: "deu", name: "German", package: "tesseract-data-deu", available: true, command: "pkexec pacman -S --needed --noconfirm tesseract-data-deu", method: "package"},
            {code: "ara", name: "Arabic", package: "tesseract-data-ara", available: true, command: "Скачать https://example.org/ara.traineddata в /home/u/.local/share/lipa/tessdata", method: "download"}]})
        compare(settings.installable.length, 2)
        compare(settings.installable[0].label, "deu — German (tesseract-data-deu)")
        compare(settings.installable[1].label, "ara — Arabic (скачать модель)")
        const dialog = findChild(settings, "installDialog")
        controller.installCalls = []
        // A repository package asks for the administrator password.
        settings.askInstall("deu", "German")
        verify(dialog.text.indexOf("пароль администратора") >= 0, dialog.text)
        dialog.accepted()
        dialog.close()
        compare(JSON.stringify(controller.installCalls), JSON.stringify([["package", "tesseract-data-deu"]]))
        // A model without a package is downloaded: no password, and the dialog says where from.
        settings.askInstall("ara", "Arabic")
        compare(settings.pendingMethod, "download")
        verify(dialog.text.indexOf("Пароль администратора не нужен") >= 0 && dialog.text.indexOf("ara.traineddata") >= 0, dialog.text)
        dialog.accepted()
        dialog.close()
        compare(JSON.stringify(controller.installCalls[1]), JSON.stringify(["download", "ara"]))
        // Something that is not on offer is ignored.
        settings.askInstall("xyz", "Nothing")
        compare(settings.pendingCode, "ara")
        verify(!dialog.visible, "no dialog for a language that is not on offer")
        controller.tesseractJson = before
    }

    function test_lineGapFactorIsEditableAndSaved() {
        settings.reload()
        settings.show()
        settings.set("display_mode", "inplace")
        findChild(settings, "settingsTabs").currentIndex = 5
        wait(50)
        const spin = findChild(settings, "lineGapFactor")
        verify(spin !== null)
        compare(spin.value, 18, "the default is 1.8")
        compare(spin.textFromValue(18), "1.8 × высоты строки")
        compare(spin.valueFromText("2,5 × высоты строки"), 25, "a decimal comma is understood")
        spin.value = 22
        spin.valueModified()
        compare(settings.current.appearance.inplace.line_gap_factor, 2.2)
        verify(settings.pendingPatch()["appearance.inplace.line_gap_factor"] !== undefined)
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

    function test_filtersAndOverlayOptionsAreInTheSettings() {
        settings.show()
        settings.reload()
        for (const [name, key] of [["filterBinarize", "recognition.binarize"], ["filterInvert", "recognition.auto_invert"], ["filterSharpen", "recognition.sharpen"]]) {
            const toggle = findChild(settings, name)
            verify(toggle !== null, name)
            verify(!toggle.checked, name + " is off by default")
            toggle.checked = true
            toggle.toggled()
            const [group, field] = key.split(".")
            compare(settings.current[group][field], true, key)
            settings.set(key, false)
        }
        verify(findChild(settings, "filterNoise").checked, "noise removal is on by default")
        const automatic = findChild(settings, "filterAuto")
        verify(automatic !== null && automatic.checked, "the automatic choice is on by default")
        automatic.checked = false
        automatic.toggled()
        compare(settings.current.recognition.auto_filters, false)
        settings.set("recognition.auto_filters", true)
        const contrast = findChild(settings, "filterContrast")
        contrast.value = 40
        contrast.valueModified()
        compare(settings.current.recognition.contrast, 40)
        settings.set("recognition.contrast", 0)
        const active = findChild(settings, "onlyWhenActive")
        verify(active !== null && !active.checked)
        active.checked = true
        active.toggled()
        compare(settings.current.appearance.inplace.only_when_active, true)
        settings.set("appearance.inplace.only_when_active", false)
        const changes = findChild(settings, "changesOnly")
        verify(changes !== null && !changes.checked)
        changes.checked = true
        changes.toggled()
        compare(settings.current.translation.changes_only, true)
        settings.set("translation.changes_only", false)
        settings.apply()
    }

    function test_windowSettingsAreOffInTheInplaceMode() {
        settings.show()
        settings.reload()
        findChild(settings, "settingsTabs").currentIndex = 4
        const page = findChild(settings, "settingsPage4")
        const note = findChild(settings, "windowSettingsDisabledNote")
        settings.set("display_mode", "window")
        verify(page.enabled && !note.visible)
        compare(page.opacity, 1)
        settings.set("display_mode", "inplace")
        verify(!page.enabled && note.visible, "the window has no effect over the original text")
        verify(page.opacity < 1, "and it looks so")
        settings.set("display_mode", "window")
        verify(page.enabled, "a separate window brings them back")
        // The overlay option lives with the display mode, not with the appearance.
        const active = findChild(settings, "onlyWhenActive")
        verify(findChild(findChild(settings, "settingsPage3"), "onlyWhenActive") === active)
        // The same option is on the appearance tab too; both switches are one setting and each points to the other.
        const twin = findChild(settings, "onlyWhenActiveAppearance")
        verify(twin !== null && !twin.checked)
        settings.set("appearance.inplace.only_when_active", true)
        verify(active.checked && twin.checked)
        settings.set("appearance.inplace.only_when_active", false)
        findChild(settings, "onlyWhenActiveLinkDisplay").linkActivated("appearance")
        compare(findChild(settings, "settingsTabs").currentIndex, 5)
        findChild(settings, "onlyWhenActiveLinkAppearance").linkActivated("display")
        compare(findChild(settings, "settingsTabs").currentIndex, 3)
    }

    function test_generalTabHoldsOnlyTheMainWindowText() {
        settings.show()
        settings.reload()
        const tabs = findChild(settings, "settingsTabs")
        compare(tabs.itemAt(8).text, "Общие")
        tabs.currentIndex = 8
        const page = findChild(settings, "settingsPage8")
        // Nothing of the application, the tray or the floating window is here.
        for (const name of ["generalTheme", "closeToTraySwitch", "generalAutostart", "generalNotifyErrors", "generalLogLevel", "translationFontFamily"])
            verify(findChild(page, name) === null, name + " does not belong to «General»")
        const font = findChild(page, "mainWindowFont")
        compare(font.currentText, "Inter")
        font.currentIndex = font.find("PT Serif")
        font.activated(font.currentIndex)
        compare(settings.current.appearance.main_window.font_family, "PT Serif")
        const cjk = findChild(page, "mainWindowCjkFont")
        compare(cjk.currentIndex, 0, "automatic")
        cjk.currentIndex = 1
        cjk.activated(1)
        verify(settings.current.appearance.main_window.cjk_font_family.length > 0)
        const size = findChild(page, "mainWindowFontSize")
        compare(size.value, 16)
        size.value = 24
        size.valueModified()
        compare(settings.current.appearance.main_window.font_size, 24)
        const background = findChild(page, "mainWindowBackground")
        compare(background.currentIndex, 0, "grey by default")
        background.currentIndex = 1
        background.activated(1)
        compare(settings.current.appearance.main_window.background, "system")
        // The floating translation window keeps its own font: it is not touched.
        compare(settings.current.appearance.window.font_size, 20)
        settings.resetKeys(["appearance.main_window"])
        compare(settings.current.appearance.main_window.font_size, 16)
        compare(settings.current.appearance.main_window.background, "gray")
        settings.apply()
    }

    function test_applicationTabHoldsThemeTrayAutostartNotificationsAndLog() {
        settings.show()
        settings.reload()
        const tabs = findChild(settings, "settingsTabs")
        compare(tabs.itemAt(9).text, "Приложение")
        tabs.currentIndex = 9
        const page = findChild(settings, "settingsPage9")
        verify(findChild(page, "closeToTraySwitch") !== null, "the tray option moved here")
        const theme = findChild(settings, "generalTheme")
        compare(theme.currentIndex, 3, "dark by default")
        theme.currentIndex = 4
        theme.activated(4)
        compare(settings.current.general.theme, "light")
        compare(settings.universalTheme, Universal.Light, "light applies at once")
        verify(findChild(settings, "generalThemeNote").text.indexOf("сразу") >= 0)
        theme.currentIndex = 2
        theme.activated(2)
        compare(settings.current.general.theme, "fusion")
        verify(findChild(settings, "generalThemeNote").text.indexOf("перезапуска") >= 0, "other styles need a restart")
        settings.set("general.theme", "dark")
        for (const [name, key] of [["generalAutostart", "autostart"], ["generalNotifyErrors", "notify_errors"], ["generalNotifyRetries", "notify_retries"]]) {
            const toggle = findChild(settings, name)
            verify(!toggle.checked, name)
            toggle.checked = true
            toggle.toggled()
            compare(settings.current.general[key], true, key)
        }
        const level = findChild(settings, "generalLogLevel")
        compare(level.currentIndex, 2, "info by default")
        level.currentIndex = 3
        level.activated(3)
        compare(settings.current.general.log_level, "debug")
        settings.resetKeys(["general"])
        compare(settings.current.general.log_level, "info")
        compare(settings.current.general.autostart, false)
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

    function test_bergamotProgressAndMissingModelMessage() {
        settings.set("translation.service", "bergamot")
        controller.modelBusy = true
        controller.modelState = JSON.stringify({pair: "en-ru", status: "Downloading...", progress: 45})
        wait(30)
        const progress = findChild(settings, "bergamotProgress")
        verify(progress !== null)
        compare(progress.value, 45)
        compare(progress.indeterminate, false)
        compare(findChild(settings, "bergamotDownload").enabled, false)
        verify(findChild(settings, "bergamotStatus").text.indexOf("en-ru") >= 0)
        controller.modelBusy = false
        controller.modelState = JSON.stringify({pair: "en-ru", status: "Not Found",
            error: "Translation model for en-ru is missing. Download it in Settings."})
        wait(30)
        compare(findChild(settings, "bergamotDownload").enabled, true)
        verify(findChild(settings, "bergamotStatus").text.indexOf("Not Found") >= 0)
        controller.modelState = "{}"
    }

    function test_reloadDoesNotScheduleAnUnchangedSave() {
        settings.apply()
        const before = controller.patchCalls
        settings.reload()
        wait(650)
        compare(controller.patchCalls, before)
    }
}
