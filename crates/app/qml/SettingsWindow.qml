import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

ApplicationWindow {
    id: win
    property var controller
    property var current: ({})
    readonly property var translators: ["google", "yandex", "custom"]
    readonly property var backends: ["auto", "kwin", "portal"]
    readonly property var langs: ["eng", "rus", "jpn", "deu", "fra", "spa", "ita", "por", "kor", "chi_sim", "ukr", "pol"]
    readonly property var targets: ["ru", "en", "uk", "de", "fr", "es", "it", "pt", "ja", "ko", "zh", "pl"]

    title: "Настройки LipaX"
    width: 880
    height: 740
    minimumWidth: 740
    minimumHeight: 540
    visible: false

    signal selectRegionRequested()

    Component.onCompleted: reload()
    // Every edit is previewed at once (overlay binds to `current`) and saved after a short pause.
    property string lastApplied: ""
    function reload() { current = JSON.parse(controller.settingsJson()); lastApplied = JSON.stringify(current) }
    function openWindow() { reload(); show(); raise(); requestActivate() }
    function set(key, v) { const c = Object.assign({}, current); c[key] = v; current = c }
    onCurrentChanged: if (JSON.stringify(current) !== lastApplied) autosave.restart()
    Timer { id: autosave; interval: 500; onTriggered: win.apply() }
    onClosing: if (autosave.running) { autosave.stop(); apply() }
    function resetKeys(keys) {
        const d = JSON.parse(controller.defaultSettingsJson())
        const c = Object.assign({}, current)
        for (const k of keys) c[k] = d[k]
        current = c
    }
    readonly property var overlayStyles: [
        { value: "blur", label: "Фон с размытием" },
        { value: "transparent", label: "Без фона (прозрачный)" },
        { value: "dim", label: "Лёгкий тёмный фон" },
        { value: "solid", label: "Сплошной фон" }
    ]
    readonly property bool solidStyle: (current.overlay_style || "solid") === "solid"
    readonly property var appearanceKeys: ["translation_display", "overlay_style", "blur_enabled", "blur_tint", "dim_inverse", "overlay_corner_radius",
        "font_family", "font_size", "font_bold", "font_italic", "text_color", "background_color",
        "opacity", "border_color", "border_opacity", "border_width", "border_pattern", "border_always", "border_seconds", "overlay_padding", "text_alignment",
        "text_wrap", "text_outline", "outline_color", "line_spacing", "show_original", "original_font_family",
        "original_font_size", "original_color", "max_width_enabled", "overlay_max_width"]
    // Largest connected screen in logical pixels: bounds for overlay position and size.
    readonly property int screenMaxWidth: Math.max(200, ...Qt.application.screens.map(s => s.width))
    readonly property int screenMaxHeight: Math.max(60, ...Qt.application.screens.map(s => s.height))
    readonly property var fontFamilies: ["Системный"].concat(Qt.fontFamilies())
    function fontIndex(name) { return name ? Math.max(0, fontFamilies.indexOf(name)) : 0 }

    // ── «Поверх оригинала»: каждое свойство отдельно — Авто или Вручную ──
    function inplace() { return current.inplace || ({}) }
    function setInplace(key, value) { const i = Object.assign({}, inplace()); i[key] = value; set("inplace", i) }
    function propManual(key) { const v = inplace()[key]; return !!v && v.mode === "manual" }
    function propValue(key, fallback) { const v = inplace()[key]; return v && v.mode === "manual" ? v.value : fallback }
    function setProp(key, manual, value) { setInplace(key, manual ? { mode: "manual", value: value } : { mode: "auto" }) }
    readonly property var inplaceBackgrounds: [
        { value: "auto", label: "Авто (по фону вокруг текста)" },
        { value: "inpaint_blur", label: "Восстановить и размыть" },
        { value: "solid_fill", label: "Заливка цветом фона" },
        { value: "adaptive_padding_fill", label: "Заливка с расширенными полями" },
        { value: "transparent", label: "Прозрачный (обводка текста)" }
    ]
    readonly property var weightNames: [
        { value: "thin", label: "Тонкий" }, { value: "extra_light", label: "Сверхсветлый" }, { value: "light", label: "Светлый" },
        { value: "normal", label: "Обычный" }, { value: "medium", label: "Средний" }, { value: "demi_bold", label: "Полужирный" },
        { value: "bold", label: "Жирный" }, { value: "extra_bold", label: "Сверхжирный" }, { value: "black", label: "Чёрный" }
    ]
    readonly property var wrapNames: [
        { value: "word_wrap", label: "По словам" }, { value: "wrap_anywhere", label: "По символам" },
        { value: "no_wrap", label: "Без переноса" }, { value: "elide", label: "Обрезать с многоточием" }
    ]
    readonly property var fontCategories: [
        { value: "serif", label: "С засечками" }, { value: "sans_serif", label: "Без засечек" },
        { value: "slab_serif", label: "Брусковые засечки" }, { value: "monospace", label: "Моноширинный" },
        { value: "cjk_sans", label: "CJK без засечек" }, { value: "cjk_serif", label: "CJK с засечками" }
    ]
    // Строка свойства: «Авто / Вручную» и редактор значения (дочерние элементы экземпляра).
    component PropRow: RowLayout {
        id: propRow
        property string key
        property var defaultValue
        readonly property bool manual: win.propManual(key)
        Layout.fillWidth: true; Layout.minimumWidth: 0
        ComboBox {
            Layout.preferredWidth: 120
            model: ["Авто", "Вручную"]
            currentIndex: propRow.manual ? 1 : 0
            onActivated: win.setProp(propRow.key, currentIndex === 1, win.propValue(propRow.key, propRow.defaultValue))
        }
    }

    component FieldLabel: Label {
        wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0
    }
    component SectionTitle: Label { font.bold: true; Layout.columnSpan: 2; Layout.topMargin: 6 }
    component ResetButton: Button {
        property var keys: []
        Layout.columnSpan: 2; Layout.topMargin: 10
        text: "Сбросить раздел по умолчанию"
        onClicked: win.resetKeys(keys)
    }
    // Swatch opens the system color dialog; text accepts only complete #rrggbb values.
    component ColorRow: RowLayout {
        id: colorRow
        property string key
        property var presets: []
        readonly property string value: win.current[key] || "#000000"
        Layout.fillWidth: true; Layout.minimumWidth: 0
        Rectangle {
            implicitWidth: 28; implicitHeight: 28; radius: 4; color: colorRow.value
            border.width: 1; border.color: palette.mid
            MouseArea { anchors.fill: parent; onClicked: { colorDialog.key = colorRow.key; colorDialog.selectedColor = colorRow.value; colorDialog.open() } }
        }
        TextField {
            Layout.preferredWidth: 100
            maximumLength: 7
            text: colorRow.value
            validator: RegularExpressionValidator { regularExpression: /#[0-9a-fA-F]{0,6}/ }
            onTextEdited: if (/^#[0-9a-fA-F]{6}$/.test(text)) win.set(colorRow.key, text.toLowerCase())
        }
        Repeater {
            model: colorRow.presets
            Rectangle {
                implicitWidth: 22; implicitHeight: 22; radius: 4; color: modelData
                border.width: 1; border.color: palette.mid
                MouseArea { anchors.fill: parent; onClicked: win.set(colorRow.key, modelData) }
            }
        }
        Item { Layout.fillWidth: true }
    }
    ColorDialog {
        id: colorDialog
        property string key
        onAccepted: win.set(key, selectedColor.toString().slice(0, 7))
    }
    // ── Tesseract ───────────────────────────────────────────────────────
    readonly property var tess: {
        try { return JSON.parse(controller.tesseractJson || "{}") } catch (e) { return ({}) }
    }
    readonly property var specParts: (current.source_lang || "eng").split("+").filter(x => x.length > 0)
    readonly property string primaryLang: specParts.length ? specParts[0] : "eng"
    readonly property var extraLangs: specParts.slice(1)
    // Установленные языки плюс выбранные, но отсутствующие (помечены).
    readonly property var langModel: {
        if (current.ocr_engine === "paddleocr") return langs.map(c => ({code: c, label: c, installed: true}))
        const have = (tess.languages || []).map(l => ({ code: l.code, label: l.code + " — " + l.name, installed: true }))
        for (const c of specParts)
            if (!have.some(l => l.code === c) && tess.installed) have.push({ code: c, label: c + " — не установлен", installed: false })
        if (have.length === 0) have.push({ code: primaryLang, label: primaryLang, installed: false })
        return have
    }
    readonly property var installable: (tess.installable || []).filter(l => l.available)
        .map(l => ({ code: l.code, package: l.package, name: l.name, command: l.command, label: l.code + " — " + l.name + " (" + l.package + ")" }))
    readonly property var missing: {
        // Зависимость от tesseractJson обновляет сообщения после установки пакета.
        if (current.ocr_engine === "paddleocr") return []
        const _ = controller.tesseractJson
        try { return JSON.parse(controller.missingLanguages(current.source_lang || "eng")) } catch (e) { return [] }
    }

    readonly property var hotkeyRows: [
        { key: "toggle", title: "Запустить / остановить слежение" },
        { key: "select_region", title: "Выбрать область перевода" },
        { key: "translate_once", title: "Перевести сейчас" },
        { key: "toggle_overlay", title: "Показать / скрыть перевод" },
        { key: "toggle_pin", title: "Закрепить / открепить перевод" }
    ]
    function setHotkey(key, v) { const h = Object.assign({}, current.hotkeys || ({})); h[key] = v; set("hotkeys", h) }
    function hotkeyDuplicate(key) {
        const h = current.hotkeys || ({})
        return !!h[key] && hotkeyRows.some(r => r.key !== key && h[r.key] === h[key])
    }
    readonly property bool hasDuplicateHotkeys: hotkeyRows.some(r => hotkeyDuplicate(r.key))

    readonly property var frameModes: [
        { value: "pattern", label: "Узор (error purple/black)" },
        { value: "solid", label: "Простая красная обводка" },
        { value: "off", label: "Выкл." },
        { value: "selection", label: "При выделении" }
    ]

    function setLangs(primary, extras) { set("source_lang", [primary].concat(extras.filter(c => c !== primary)).join("+")) }

    property string pendingPackage: ""
    property string pendingCommand: ""
    function askInstall(pkg, name) {
        const info = (tess.installable || []).find(l => l.package === pkg)
        pendingPackage = pkg
        pendingCommand = info ? info.command : pkg
        installDialog.text = "Установить языковой пакет «" + name + "»?\n\nБудет выполнена команда:\n" + pendingCommand
            + "\n\nСистема запросит пароль администратора."
        installDialog.open()
    }
    Dialog {
        id: installDialog
        property string text: ""
        title: "Установка языкового пакета"
        modal: true
        anchors.centerIn: parent
        width: Math.min(parent.width - 40, 460)
        standardButtons: Dialog.Yes | Dialog.No
        Label { width: parent.width; wrapMode: Text.Wrap; text: installDialog.text }
        onAccepted: win.controller.installPackage(win.pendingPackage)
    }

    function apply() { autosave.stop(); controller.applySettings(JSON.stringify(current)); reload() }

    // Changes made elsewhere (overlay drag, pin, region selection) arrive here; pending edits are saved first.
    Connections {
        target: win.controller
        function onSettingsStateChanged() { if (autosave.running) win.apply(); else win.reload() }
    }
    readonly property var history: { try { return JSON.parse(controller.historyJson || "[]") } catch (e) { return [] } }
    readonly property var diagnostics: { try { return JSON.parse(controller.diagnosticsJson || "[]") } catch (e) { return [] } }
    // ── Regions: at most `maxRegions`; one active unless several are allowed ──
    readonly property int maxRegions: 3
    property string regionNotice: ""
    Timer { id: regionNoticeTimer; interval: 4000; onTriggered: win.regionNotice = "" }
    function notifyRegions(text) { regionNotice = text; regionNoticeTimer.restart() }
    function withRegions(regions, extra) {
        const c = Object.assign({}, current, extra || ({}))
        c.regions = regions
        current = c
    }
    function activateRegion(index, on) {
        const r = (current.regions || []).map(x => Object.assign({}, x))
        if (!r[index]) return
        if (on && current.allow_multiple_regions !== true) r.forEach((x, i) => x.enabled = false)
        r[index].enabled = on
        withRegions(r, on ? { active_region: r[index].id } : ({}))
    }
    function setAllowMultipleRegions(on) {
        const r = (current.regions || []).map(x => Object.assign({}, x))
        if (!on) {
            let keep = r.findIndex(x => x.enabled && x.id === current.active_region)
            if (keep < 0) keep = r.findIndex(x => x.enabled)
            r.forEach((x, i) => x.enabled = i === keep)
        }
        withRegions(r, { allow_multiple_regions: on })
    }
    function addRegion() {
        const r = (current.regions || []).slice()
        if (r.length >= maxRegions) {
            notifyRegions("Можно создать не больше " + maxRegions + " областей. Удалите одну, чтобы добавить новую.")
            return false
        }
        let n = r.length + 1
        while (r.some(x => x.id === "region-" + n)) ++n
        // A new region starts inactive: it becomes active once its area is selected.
        r.push({ id: "region-" + n, name: "Область " + n, enabled: false, rect: null, source_lang: "", target_lang: "",
                 ocr_engine: "", interval_ms: 500, debounce_ms: 400 })
        withRegions(r)
        return true
    }
    function removeRegion(index) {
        withRegions((current.regions || []).filter((_, i) => i !== index))
    }
    function setRegionField(index, key, v) {
        const r = (current.regions || []).map(x => Object.assign({}, x))
        r[index][key] = v
        set("regions", r)
    }

    header: TabBar {
        id: tabs
        objectName: "settingsTabs"
        Repeater {
            model: ["Распознавание", "Перевод и захват", "Окно перевода", "Внешний вид перевода", "Область", "Клавиши", "Статус", "О программе"]
            TabButton { text: modelData; width: implicitWidth }
        }
        onCurrentIndexChanged: if (currentIndex === 6 && win.diagnostics.length === 0) win.controller.refreshDiagnostics()
    }
    footer: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.margins: 10
            Label {
                text: autosave.running ? "Сохранение…" : "Изменения применяются сразу и сохраняются автоматически"
                wrapMode: Text.Wrap; Layout.fillWidth: true; opacity: 0.7
            }
            Button { text: "Закрыть"; onClicked: win.close() }
        }
        implicitHeight: 66
    }
    StackLayout {
        anchors.fill: parent
        anchors.margins: 18
        currentIndex: tabs.currentIndex
        ScrollView {
            id: page0
            objectName: "settingsPage0"
            contentWidth: availableWidth
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            GridLayout {
                width: page0.availableWidth - 16
                columns: 2
                columnSpacing: 24
                rowSpacing: 14
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "OCR-движок" }
            ComboBox {
                Layout.fillWidth: true; Layout.minimumWidth: 0
                model: ["Tesseract", "PaddleOCR 3.x"]
                currentIndex: win.current.ocr_engine === "paddleocr" ? 1 : 0
                onActivated: win.set("ocr_engine", currentIndex === 1 ? "paddleocr" : "tesseract")
            }
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Python для PaddleOCR"; visible: win.current.ocr_engine === "paddleocr" }
            TextField {
                Layout.fillWidth: true; Layout.minimumWidth: 0; visible: win.current.ocr_engine === "paddleocr"
                text: win.current.paddle_python || "python3"
                placeholderText: "/путь/к/venv/bin/python"
                onTextEdited: win.set("paddle_python", text)
            }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap
                visible: win.current.ocr_engine === "paddleocr"
                text: "PaddleOCR использует основной язык; дополнительные языки относятся к Tesseract. При первом запуске загружаются модели. Установка: docs/PaddleOCR.md."
            }

            // ── Tesseract OCR: состояние и языки ────────────────────────────────
            Label { text: "Tesseract: состояние и языковые пакеты"; visible: win.current.ocr_engine !== "paddleocr"; font.bold: true; Layout.columnSpan: 2 }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap
                visible: win.current.ocr_engine !== "paddleocr" && (!win.tess.distro || win.tess.installed)
                text: !win.tess.distro ? "Проверка…"
                    : "Установлен · " + (win.tess.version ? "версия " + win.tess.version : "версия неизвестна")
                      + "\nПуть: " + win.tess.path
                      + "\ntessdata: " + (win.tess.tessdata || "не определён")
                      + "\nСистема: " + win.tess.distro.name + " · менеджер пакетов: " + win.tess.package_manager
                      + (win.tess.engine_package ? "\nПакет: " + win.tess.engine_package : "")
                      + (win.tess.tessdata_package ? " · данные: " + win.tess.tessdata_package + " и др." : "")
            }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap
                visible: win.current.ocr_engine !== "paddleocr" && !!win.tess.distro && !win.tess.installed
                color: "#ff6b6b"
                text: "Tesseract не установлен" + (win.tess.engine_install_command ? ". Команда установки: " + win.tess.engine_install_command : "")
            }

            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Основной язык текста (OCR)" }
            ComboBox {
                id: primaryBox
                Layout.fillWidth: true; Layout.minimumWidth: 0
                model: win.langModel
                textRole: "label"
                currentIndex: Math.max(0, win.langModel.findIndex(l => l.code === win.primaryLang))
                onActivated: win.setLangs(win.langModel[currentIndex].code, win.extraLangs)
            }
            Label { Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Дополнительные языки (по желанию)"; wrapMode: Text.Wrap; Layout.fillWidth: true }
            Flow {
                Layout.fillWidth: true; Layout.minimumWidth: 0
                spacing: 8
                Repeater {
                    model: win.langModel.filter(l => l.code !== win.primaryLang && l.installed)
                    CheckBox {
                        text: modelData.label
                        checked: win.extraLangs.indexOf(modelData.code) >= 0
                        onToggled: {
                            const set = win.extraLangs.filter(c => c !== modelData.code)
                            if (checked) set.push(modelData.code)
                            win.setLangs(win.primaryLang, set)
                        }
                    }
                }
            }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap
                text: "Будет использовано: " + (win.current.source_lang || "eng")
                opacity: 0.7
            }

            // Выбраны языки, которых нет в Tesseract: понятное сообщение и пакет для этого дистрибутива.
            Repeater {
                model: win.missing
                delegate: RowLayout {
                    Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0
                    Label { Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; color: "#ff6b6b"; text: modelData.message }
                    Button {
                        visible: modelData.package !== null && win.tess.package_manager !== "unknown"
                        text: "Установить языковой пакет"
                        enabled: !win.controller.tesseractBusy
                        onClicked: win.askInstall(modelData.package, modelData.name)
                    }
                }
            }

            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Установить другой язык"; visible: win.current.ocr_engine !== "paddleocr" }
            RowLayout {
                visible: win.current.ocr_engine !== "paddleocr"
                Layout.fillWidth: true; Layout.minimumWidth: 0
                ComboBox {
                    id: installBox
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    model: win.installable
                    textRole: "label"
                    enabled: win.installable.length > 0
                }
                Button {
                    text: "Установить"
                    enabled: installBox.currentIndex >= 0 && win.installable.length > 0 && !win.controller.tesseractBusy
                    onClicked: win.askInstall(win.installable[installBox.currentIndex].package, win.installable[installBox.currentIndex].name)
                }
            }
            Button {
                Layout.columnSpan: 2
                visible: win.current.ocr_engine !== "paddleocr"
                text: win.controller.tesseractBusy ? "Проверка…" : "Обновить список языков"
                enabled: !win.controller.tesseractBusy
                onClicked: win.controller.refreshTesseract()
            }
            ResetButton { keys: ["ocr_engine", "paddle_python", "source_lang"] }

            }
        }
        ScrollView {
            id: page1
            objectName: "settingsPage1"
            contentWidth: availableWidth
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            GridLayout {
                width: page1.availableWidth - 16
                columns: 2
                columnSpacing: 24
                rowSpacing: 14
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Язык перевода" }
            ComboBox {
                Layout.fillWidth: true; Layout.minimumWidth: 0
                model: win.targets
                currentIndex: Math.max(0, win.targets.indexOf(win.current.target_lang))
                onActivated: win.set("target_lang", currentText)
            }
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Захват окна" }
            ColumnLayout {
                Layout.fillWidth: true; Layout.minimumWidth: 0
                ComboBox {
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    model: ["Авто: KWin, а если недоступен — portal", "KWin ScreenShot2 (только KDE)", "xdg-desktop-portal (PipeWire)"]
                    currentIndex: Math.max(0, win.backends.indexOf(win.current.capture_backend))
                    onActivated: win.set("capture_backend", win.backends[currentIndex])
                }
                Label {
                    Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; opacity: 0.7
                    visible: win.current.capture_backend !== "kwin"
                    text: "Portal показывает системный диалог выбора окна и требует gstreamer с gst-plugin-pipewire. "
                          + "Выбор запоминается, диалог при следующем запуске не нужен. Выбранное окно переключается заново кнопкой «Выбрать окно»."
                }
            }

            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Сервис перевода" }
            ComboBox {
                id: tr
                Layout.fillWidth: true; Layout.minimumWidth: 0
                model: ["Google Translate", "Yandex Translate", "Свой API"]
                currentIndex: Math.max(0, win.translators.indexOf(win.current.translator))
                onActivated: win.set("translator", win.translators[currentIndex])
            }

            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Yandex API-ключ"; visible: tr.currentIndex === 1 }
            TextField {
                visible: tr.currentIndex === 1; Layout.fillWidth: true; Layout.minimumWidth: 0
                echoMode: TextInput.Password
                text: win.current.yandex_api_key || ""
                onEditingFinished: win.set("yandex_api_key", text)
            }
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Yandex folder ID"; visible: tr.currentIndex === 1 }
            TextField {
                visible: tr.currentIndex === 1; Layout.fillWidth: true; Layout.minimumWidth: 0
                text: win.current.yandex_folder_id || ""
                onEditingFinished: win.set("yandex_folder_id", text)
            }
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "URL API (LibreTranslate-формат)"; visible: tr.currentIndex === 2 }
            TextField {
                visible: tr.currentIndex === 2; Layout.fillWidth: true; Layout.minimumWidth: 0
                placeholderText: "https://host/translate"
                text: win.current.custom_url || ""
                onEditingFinished: win.set("custom_url", text)
            }
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Ключ API (необязательно)"; visible: tr.currentIndex === 2 }
            TextField {
                visible: tr.currentIndex === 2; Layout.fillWidth: true; Layout.minimumWidth: 0
                echoMode: TextInput.Password
                text: win.current.custom_api_key || ""
                onEditingFinished: win.set("custom_api_key", text)
            }

            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Интервал проверки, мс" }
            SpinBox {
                from: 100; to: 5000; stepSize: 50; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                value: win.current.interval_ms || 500
                onValueModified: win.set("interval_ms", value)
            }
            Label { Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Порог контраста букв для детектора смены текста (ниже — чувствительнее)" ; wrapMode: Text.Wrap; Layout.fillWidth: true }
            Slider {
                from: 0.2; to: 20; Layout.fillWidth: true; Layout.minimumWidth: 0
                value: win.current.sensitivity || 2
                onMoved: win.set("sensitivity", value)
            }
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Ждать стабилизации текста, мс" }
            SpinBox {
                from: 0; to: 3000; stepSize: 50; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                value: win.current.debounce_ms || 0
                onValueModified: win.set("debounce_ms", value)
            }
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Автоматический перевод" }
            Switch { checked: win.current.auto_translate !== false; onToggled: win.set("auto_translate", checked) }
            ResetButton { keys: ["target_lang", "capture_backend", "translator", "interval_ms", "sensitivity", "debounce_ms", "auto_translate"] }

            }
        }
        ScrollView {
            id: page2
            objectName: "settingsPage2"
            contentWidth: availableWidth
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            GridLayout {
                width: page2.availableWidth - 16
                columns: 2
                columnSpacing: 24
                rowSpacing: 14
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Экран перевода" }
            ComboBox {
                Layout.fillWidth: true; Layout.minimumWidth: 0
                readonly property var screens: Qt.application.screens
                model: ["Автоматически: экран игры"].concat(screens.map(s => s.name))
                currentIndex: Math.max(0, screens.findIndex(s => s.name === win.current.overlay_screen) + 1)
                onActivated: win.set("overlay_screen", currentIndex === 0 ? "" : screens[currentIndex - 1].name)
            }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; opacity: 0.75
                text: "В KWin перевод следует за окном игры. Для portal выберите монитор вручную: положение окна скрыто порталом. Координаты ниже считаются от угла этого экрана."
            }
            FieldLabel { text: "Закрепить перевод" }
            Switch { checked: win.current.overlay_pinned === true; onToggled: win.set("overlay_pinned", checked) }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; opacity: 0.75
                text: "Средняя кнопка мыши по рамке перевода переключает закрепление. Незакреплённый перевод перетаскивается левой кнопкой, его рамка толще. "
                      + "Закреплённый пропускает клики в игру (если включено ниже); рамка и значок в углу остаются доступны для средней кнопки."
            }
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Пропускать клики мыши" }
            Switch { checked: win.current.click_through !== false; onToggled: win.set("click_through", checked) }
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Положение перевода (x, y)" }
            RowLayout {
                SpinBox {
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    from: 0; to: win.screenMaxWidth; editable: true
                    value: win.current.overlay_pos ? win.current.overlay_pos[0] : 0
                    onValueModified: win.set("overlay_pos", [value, win.current.overlay_pos[1]])
                }
                SpinBox {
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    from: 0; to: win.screenMaxHeight; editable: true
                    value: win.current.overlay_pos ? win.current.overlay_pos[1] : 0
                    onValueModified: win.set("overlay_pos", [win.current.overlay_pos[0], value])
                }
            }
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Размер перевода (ш, в)" }
            RowLayout {
                SpinBox {
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    from: 200; to: win.screenMaxWidth; editable: true
                    value: win.current.overlay_size ? win.current.overlay_size[0] : 700
                    onValueModified: win.set("overlay_size", [value, win.current.overlay_size[1]])
                }
                SpinBox {
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    from: 60; to: win.screenMaxHeight; editable: true
                    value: win.current.overlay_size ? win.current.overlay_size[1] : 120
                    onValueModified: win.set("overlay_size", [win.current.overlay_size[0], value])
                }
            }

            // ── Рамка выбора ────────────────────────────────────────────────────
            SectionTitle { text: "Рамка областей в игре" }
            FieldLabel { text: "Показывать рамку" }
            ComboBox {
                objectName: "regionFrameMode"
                Layout.fillWidth: true; Layout.minimumWidth: 0
                model: win.frameModes.map(m => m.label)
                currentIndex: Math.max(0, win.frameModes.findIndex(m => m.value === (win.current.region_frame_mode || "selection")))
                onActivated: win.set("region_frame_mode", win.frameModes[currentIndex].value)
            }
            FieldLabel { text: "Исчезает через, с"; visible: win.current.region_frame_mode === "selection" }
            SpinBox {
                visible: win.current.region_frame_mode === "selection"
                from: 1; to: 60; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                value: win.current.frame_seconds || 3
                onValueModified: win.set("frame_seconds", value)
            }
            FieldLabel { text: "Цвет обводки"; visible: win.current.region_frame_mode !== "pattern" && win.current.region_frame_mode !== "off" }
            ColorRow {
                visible: win.current.region_frame_mode !== "pattern" && win.current.region_frame_mode !== "off"
                key: "frame_color"; presets: ["#ff0000", "#00c800", "#1e90ff", "#ffd400", "#ffffff"]
            }
            FieldLabel { text: "Толщина рамки, px"; visible: win.current.region_frame_mode !== "off" }
            SpinBox {
                visible: win.current.region_frame_mode !== "off"
                from: 1; to: 12; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                value: win.current.frame_width || 2
                onValueModified: win.set("frame_width", value)
            }
            FieldLabel { text: "Когда перевод закреплён" }
            ComboBox {
                Layout.fillWidth: true; Layout.minimumWidth: 0
                model: ["Полупрозрачная обводка", "Скрыть обводку"]
                currentIndex: win.current.region_frame_pinned === "hide" ? 1 : 0
                onActivated: win.set("region_frame_pinned", currentIndex === 1 ? "hide" : "dim")
            }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; opacity: 0.7
                text: "Рамка рисуется вокруг каждой активной области и не мешает кликам. У каждой области свой таймер; "
                      + "режим можно переопределить для отдельной области во вкладке «Области». "
                      + "Для окна, выбранного через portal, положение на экране неизвестно, и рамка не показывается."
            }
            ResetButton { keys: ["overlay_screen", "overlay_pinned", "click_through", "overlay_pos", "overlay_size", "frame_color", "frame_width", "frame_seconds", "region_frame_mode", "region_frame_pinned"] }

            }
        }
        ScrollView {
            id: page3
            objectName: "settingsPage3"
            contentWidth: availableWidth
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            GridLayout {
                width: page3.availableWidth - 16
                columns: 2
                columnSpacing: 24
                rowSpacing: 12
            SectionTitle { text: "Режим отображения" }
            FieldLabel { text: "Где показывать перевод" }
            ComboBox {
                objectName: "translationDisplay"
                Layout.fillWidth: true; Layout.minimumWidth: 0
                model: ["В окне перевода", "Поверх оригинала"]
                currentIndex: win.current.translation_display === "inplace" ? 1 : 0
                onActivated: win.set("translation_display", currentIndex === 1 ? "inplace" : "overlay")
            }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; opacity: 0.7
                visible: win.current.translation_display === "inplace"
                text: "Перевод закрывает исходный текст в каждой активной области: размытая заливка цвета фона, "
                      + "цвет, размер и начертание оцениваются по кадру; длинный перевод уменьшается. "
                      + "Можно выбрать свой шрифт ниже. Средняя кнопка мыши скрывает перевод; вернуть его можно переключателем «Поверх игры». "
                      + "Нужен захват через KWin: при захвате через portal используется окно перевода."
            }
            GridLayout {
                objectName: "inplaceSettings"
                visible: win.current.translation_display === "inplace"
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0
                columns: 2
                columnSpacing: 24
                rowSpacing: 10
                Label {
                    Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; opacity: 0.7
                    text: "Каждое поле текста находится и отслеживается отдельно. Шрифт для поля выбирается один раз — "
                          + "по признакам начертания оригинала и среди установленных шрифтов с глифами языка перевода — и дальше не меняется. "
                          + "Каждое свойство ниже можно оставить автоматическим или задать вручную независимо от остальных."
                }
                FieldLabel { text: "Фон под переводом" }
                ComboBox {
                    objectName: "inplaceBackground"
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    model: win.inplaceBackgrounds.map(m => m.label)
                    currentIndex: Math.max(0, win.inplaceBackgrounds.findIndex(m => m.value === (win.inplace().background_mode || "auto")))
                    onActivated: win.setInplace("background_mode", win.inplaceBackgrounds[currentIndex].value)
                }
                FieldLabel { text: "Шрифт" }
                PropRow {
                    id: fontRow
                    key: "font_family"; defaultValue: Qt.application.font.family
                    ComboBox {
                        visible: fontRow.manual
                        Layout.fillWidth: true; Layout.minimumWidth: 0
                        model: Qt.fontFamilies()
                        currentIndex: Math.max(0, model.indexOf(win.propValue("font_family", "")))
                        onActivated: win.setProp("font_family", true, currentText)
                    }
                }
                FieldLabel { text: "Размер, px" }
                PropRow {
                    id: sizeRow
                    key: "font_size"; defaultValue: 20
                    SpinBox {
                        visible: sizeRow.manual
                        from: 4; to: 400; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                        value: win.propValue("font_size", 20)
                        onValueModified: win.setProp("font_size", true, value)
                    }
                }
                FieldLabel { text: "Насыщенность" }
                PropRow {
                    id: weightRow
                    key: "font_weight"; defaultValue: "normal"
                    ComboBox {
                        visible: weightRow.manual
                        Layout.fillWidth: true; Layout.minimumWidth: 0
                        model: win.weightNames.map(w => w.label)
                        currentIndex: Math.max(0, win.weightNames.findIndex(w => w.value === win.propValue("font_weight", "normal")))
                        onActivated: win.setProp("font_weight", true, win.weightNames[currentIndex].value)
                    }
                }
                FieldLabel { text: "Курсив" }
                PropRow {
                    id: italicRow
                    key: "italic"; defaultValue: false
                    CheckBox { visible: italicRow.manual; text: "Курсив"; checked: win.propValue("italic", false) === true; onToggled: win.setProp("italic", true, checked) }
                    Item { Layout.fillWidth: true }
                }
                FieldLabel { text: "Межстрочный интервал, %" }
                PropRow {
                    id: lineRow
                    key: "line_height"; defaultValue: 1.0
                    SpinBox {
                        visible: lineRow.manual
                        from: 70; to: 300; stepSize: 5; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                        value: Math.round(win.propValue("line_height", 1.0) * 100)
                        onValueModified: win.setProp("line_height", true, value / 100)
                    }
                }
                FieldLabel { text: "Трекинг, px" }
                PropRow {
                    id: spacingRow
                    key: "letter_spacing"; defaultValue: 0
                    SpinBox {
                        visible: spacingRow.manual
                        from: -3; to: 20; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                        value: win.propValue("letter_spacing", 0)
                        onValueModified: win.setProp("letter_spacing", true, value)
                    }
                }
                FieldLabel { text: "Выравнивание" }
                PropRow {
                    id: alignRow
                    key: "alignment"; defaultValue: "center"
                    ComboBox {
                        visible: alignRow.manual
                        Layout.fillWidth: true; Layout.minimumWidth: 0
                        readonly property var values: ["left", "center", "right"]
                        model: ["По левому краю", "По центру", "По правому краю"]
                        currentIndex: Math.max(0, values.indexOf(win.propValue("alignment", "center")))
                        onActivated: win.setProp("alignment", true, values[currentIndex])
                    }
                }
                FieldLabel { text: "Перенос строк" }
                PropRow {
                    id: wrapRow
                    key: "wrap_mode"; defaultValue: "word_wrap"
                    ComboBox {
                        visible: wrapRow.manual
                        Layout.fillWidth: true; Layout.minimumWidth: 0
                        model: win.wrapNames.map(w => w.label)
                        currentIndex: Math.max(0, win.wrapNames.findIndex(w => w.value === win.propValue("wrap_mode", "word_wrap")))
                        onActivated: win.setProp("wrap_mode", true, win.wrapNames[currentIndex].value)
                    }
                }
                FieldLabel { text: "Цвет текста" }
                PropRow {
                    id: colorRow
                    key: "text_color"; defaultValue: "#ffffff"
                    Rectangle {
                        visible: colorRow.manual
                        implicitWidth: 24; implicitHeight: 24; radius: 4
                        color: win.propValue("text_color", "#ffffff"); border.width: 1; border.color: palette.mid
                    }
                    TextField {
                        visible: colorRow.manual
                        Layout.preferredWidth: 100
                        maximumLength: 7
                        text: win.propValue("text_color", "#ffffff")
                        validator: RegularExpressionValidator { regularExpression: /#[0-9a-fA-F]{0,6}/ }
                        onTextEdited: if (/^#[0-9a-fA-F]{6}$/.test(text)) win.setProp("text_color", true, text.toLowerCase())
                    }
                    Item { Layout.fillWidth: true }
                }
                FieldLabel { text: "Поля, px" }
                PropRow {
                    id: paddingRow
                    key: "padding"; defaultValue: ({ left: 4, right: 4, top: 4, bottom: 4 })
                    SpinBox {
                        visible: paddingRow.manual
                        from: 0; to: 64; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                        value: win.propValue("padding", { left: 4 }).left
                        onValueModified: win.setProp("padding", true, { left: value, right: value, top: value, bottom: value })
                    }
                }
                FieldLabel { text: "Кегль: от / до, px" }
                RowLayout {
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    SpinBox {
                        Layout.fillWidth: true; Layout.minimumWidth: 0
                        from: 4; to: 200; editable: true
                        value: win.inplace().minimum_font_size || 8
                        onValueModified: win.setInplace("minimum_font_size", value)
                    }
                    SpinBox {
                        Layout.fillWidth: true; Layout.minimumWidth: 0
                        from: 4; to: 400; editable: true
                        value: win.inplace().maximum_font_size || 96
                        onValueModified: win.setInplace("maximum_font_size", value)
                    }
                }
                FieldLabel { text: "Узкий шрифт, если перевод не помещается" }
                Switch { checked: win.inplace().allow_condensed_fallback !== false; onToggled: win.setInplace("allow_condensed_fallback", checked) }
                FieldLabel { text: "Предпочтительные шрифты (через запятую)" }
                TextField {
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    placeholderText: "PT Serif, Inter"
                    text: (win.inplace().preferred_fonts || []).join(", ")
                    onEditingFinished: win.setInplace("preferred_fonts", text.split(",").map(f => f.trim()).filter(f => f.length > 0))
                }
                Label { text: "Замена шрифта по начертанию оригинала"; font.bold: true; Layout.columnSpan: 2; Layout.topMargin: 4 }
                Repeater {
                    model: win.fontCategories
                    delegate: RowLayout {
                        required property var modelData
                        Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0
                        FieldLabel { text: modelData.label }
                        ComboBox {
                            Layout.fillWidth: true; Layout.minimumWidth: 0
                            model: ["Авто"].concat(Qt.fontFamilies())
                            currentIndex: Math.max(0, model.indexOf((win.inplace().font_overrides || {})[modelData.value] || ""))
                            onActivated: {
                                const o = Object.assign({}, win.inplace().font_overrides || {})
                                if (currentIndex === 0) delete o[modelData.value]; else o[modelData.value] = currentText
                                win.setInplace("font_overrides", o)
                            }
                        }
                    }
                }
                RowLayout {
                    Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0
                    Button {
                        text: "Определить шрифты заново"
                        onClicked: if (win.controller.reanalyzeFonts) win.controller.reanalyzeFonts()
                    }
                    Item { Layout.fillWidth: true }
                    Button { text: "Сбросить «Поверх оригинала»"; onClicked: win.resetKeys(["inplace"]) }
                }
            }
            FieldLabel { text: "Фон перевода" }
            ComboBox {
                objectName: "overlayStyle"
                Layout.fillWidth: true; Layout.minimumWidth: 0
                model: win.overlayStyles.map(m => m.label)
                currentIndex: Math.max(0, win.overlayStyles.findIndex(m => m.value === (win.current.overlay_style || "solid")))
                onActivated: win.set("overlay_style", win.overlayStyles[currentIndex].value)
            }
            FieldLabel { text: "Размытие (KWin)"; visible: win.current.overlay_style === "blur" }
            Switch {
                visible: win.current.overlay_style === "blur"
                checked: win.current.blur_enabled !== false
                onToggled: win.set("blur_enabled", checked)
            }
            FieldLabel { text: "Прозрачность фона"; visible: win.current.overlay_style === "blur" }
            RowLayout {
                visible: win.current.overlay_style === "blur"
                Layout.fillWidth: true; Layout.minimumWidth: 0
                Slider {
                    objectName: "blurTint"
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    from: 0; to: 0.8
                    value: win.current.blur_tint !== undefined ? win.current.blur_tint : 0.3
                    onMoved: win.set("blur_tint", Math.round(value * 100) / 100)
                }
                Label { text: Math.round((win.current.blur_tint !== undefined ? win.current.blur_tint : 0.3) * 100) + "%" }
            }
            FieldLabel { text: "Светлый фон, тёмный текст"; visible: win.current.overlay_style === "dim" }
            Switch {
                visible: win.current.overlay_style === "dim"
                checked: win.current.dim_inverse === true
                onToggled: win.set("dim_inverse", checked)
            }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; opacity: 0.7
                visible: !win.solidStyle
                text: win.current.overlay_style === "transparent"
                      ? "Белый текст с лёгкой тенью, без фона. Цвета текста и фона ниже действуют только в режиме «Сплошной фон»."
                      : "Размытие делает KWin; его силу задают «Системные настройки → Эффекты рабочего стола → Размытие». "
                        + "Без KWin рисуется только тонированный фон. Цвета текста и фона ниже действуют только в режиме «Сплошной фон»."
            }

            SectionTitle { text: "Текст перевода" }
            FieldLabel { text: "Шрифт" }
            ComboBox {
                Layout.fillWidth: true; Layout.minimumWidth: 0
                model: win.fontFamilies
                currentIndex: win.fontIndex(win.current.font_family)
                onActivated: win.set("font_family", currentIndex === 0 ? "" : currentText)
            }
            FieldLabel { text: "Размер шрифта, px" }
            SpinBox {
                from: 8; to: 96; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                value: win.current.font_size || 20
                onValueModified: win.set("font_size", value)
            }
            FieldLabel { text: "Начертание" }
            RowLayout {
                CheckBox { text: "Жирный"; checked: win.current.font_bold === true; onToggled: win.set("font_bold", checked) }
                CheckBox { text: "Курсив"; checked: win.current.font_italic === true; onToggled: win.set("font_italic", checked) }
            }
            FieldLabel { text: "Цвет текста"; visible: win.solidStyle }
            ColorRow { visible: win.solidStyle; key: "text_color"; presets: ["#ffffff", "#ffe066", "#7cf29c", "#7cc7ff"] }
            FieldLabel { text: "Обводка текста" }
            RowLayout {
                Layout.fillWidth: true; Layout.minimumWidth: 0
                Switch { checked: win.current.text_outline !== false; onToggled: win.set("text_outline", checked) }
                ColorRow { key: "outline_color"; presets: ["#000000", "#400040"]; enabled: win.current.text_outline !== false }
            }
            FieldLabel { text: "Межстрочный интервал" }
            RowLayout {
                Layout.fillWidth: true; Layout.minimumWidth: 0
                Slider {
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    from: 0.8; to: 2.5; stepSize: 0.05
                    value: win.current.line_spacing || 1.0
                    onMoved: win.set("line_spacing", Math.round(value * 100) / 100)
                }
                Label { text: (win.current.line_spacing || 1.0).toFixed(2) }
            }
            FieldLabel { text: "Выравнивание" }
            ComboBox {
                Layout.fillWidth: true; Layout.minimumWidth: 0
                readonly property var values: ["left", "center", "right"]
                model: ["По левому краю", "По центру", "По правому краю"]
                currentIndex: Math.max(0, values.indexOf(win.current.text_alignment || "center"))
                onActivated: win.set("text_alignment", values[currentIndex])
            }
            FieldLabel { text: "Переносить строки" }
            Switch { checked: win.current.text_wrap !== false; onToggled: win.set("text_wrap", checked) }

            SectionTitle { text: "Оригинал" }
            FieldLabel { text: "Показывать оригинал над переводом" }
            Switch { checked: win.current.show_original === true; onToggled: win.set("show_original", checked) }
            FieldLabel { text: "Шрифт оригинала"; visible: win.current.show_original === true }
            ComboBox {
                visible: win.current.show_original === true
                Layout.fillWidth: true; Layout.minimumWidth: 0
                model: ["Как у перевода"].concat(win.fontFamilies.slice(1))
                currentIndex: win.fontIndex(win.current.original_font_family)
                onActivated: win.set("original_font_family", currentIndex === 0 ? "" : currentText)
            }
            FieldLabel { text: "Размер оригинала, px"; visible: win.current.show_original === true }
            SpinBox {
                visible: win.current.show_original === true
                from: 8; to: 96; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                value: win.current.original_font_size || 14
                onValueModified: win.set("original_font_size", value)
            }
            FieldLabel { text: "Цвет оригинала"; visible: win.current.show_original === true }
            ColorRow { visible: win.current.show_original === true; key: "original_color"; presets: ["#b0b0b0", "#ffffff", "#ffe066"] }

            SectionTitle { text: "Фон и рамка" }
            FieldLabel { text: "Цвет фона"; visible: win.solidStyle }
            ColorRow { visible: win.solidStyle; key: "background_color"; presets: ["#181818", "#000000", "#202040", "#300030"] }
            FieldLabel { text: "Непрозрачность фона"; visible: win.solidStyle }
            RowLayout {
                visible: win.solidStyle
                Layout.fillWidth: true; Layout.minimumWidth: 0
                Slider {
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    from: 0; to: 1
                    value: win.current.opacity !== undefined ? win.current.opacity : 0.85
                    onMoved: win.set("opacity", Math.round(value * 100) / 100)
                }
                Label { text: Math.round((win.current.opacity !== undefined ? win.current.opacity : 0.85) * 100) + "%" }
            }
            FieldLabel { text: "Цвет рамки" }
            ColorRow { key: "border_color"; presets: ["#ff00ff", "#ff2a6d", "#00e5ff", "#ffd400"] }
            FieldLabel { text: "Показывать рамку" }
            RowLayout {
                Layout.fillWidth: true; Layout.minimumWidth: 0
                ComboBox {
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    model: ["Всегда", "После выделения области"]
                    currentIndex: win.current.border_always === false ? 1 : 0
                    onActivated: win.set("border_always", currentIndex === 0)
                }
                SpinBox {
                    visible: win.current.border_always === false
                    from: 1; to: 120; editable: true
                    value: win.current.border_seconds || 5
                    onValueModified: win.set("border_seconds", value)
                }
                Label { visible: win.current.border_always === false; text: "с" }
            }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; opacity: 0.7
                visible: win.current.border_always === false
                text: "Рамка появляется на заданное время после выделения или изменения области. Незакреплённый перевод показывает рамку всегда."
            }
            FieldLabel { text: "Узор «текстура ошибки» (цвет/чёрный)" }
            Switch { checked: win.current.border_pattern === true; onToggled: win.set("border_pattern", checked) }
            FieldLabel { text: "Непрозрачность рамки" }
            RowLayout {
                Layout.fillWidth: true; Layout.minimumWidth: 0
                Slider {
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    from: 0; to: 1
                    value: win.current.border_opacity !== undefined ? win.current.border_opacity : 0.65
                    onMoved: win.set("border_opacity", Math.round(value * 100) / 100)
                }
                Label { text: Math.round((win.current.border_opacity !== undefined ? win.current.border_opacity : 0.65) * 100) + "%" }
            }
            FieldLabel { text: "Толщина рамки (закреплён), px" }
            SpinBox {
                from: 1; to: 16; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                value: win.current.border_width || 2
                onValueModified: win.set("border_width", value)
            }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; opacity: 0.7
                text: "Незакреплённый перевод показывается с рамкой на 2 px толще: так видно, что его можно перетаскивать."
            }

            SectionTitle { text: "Размеры" }
            FieldLabel { text: "Скругление углов открепленного окна, px" }
            SpinBox {
                from: 0; to: 32; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                value: win.current.overlay_corner_radius !== undefined ? win.current.overlay_corner_radius : 12
                onValueModified: win.set("overlay_corner_radius", value)
            }
            FieldLabel { text: "Внутренние отступы, px" }
            SpinBox {
                from: 0; to: 64; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                value: win.current.overlay_padding !== undefined ? win.current.overlay_padding : 16
                onValueModified: win.set("overlay_padding", value)
            }
            CheckBox {
                Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0
                text: "Максимальная ширина, px"
                checked: win.current.max_width_enabled !== false
                onToggled: win.set("max_width_enabled", checked)
            }
            SpinBox {
                enabled: win.current.max_width_enabled !== false
                from: 200; to: win.screenMaxWidth; stepSize: 50; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                value: win.current.overlay_max_width || 900
                onValueModified: win.set("overlay_max_width", value)
            }
            ResetButton { keys: win.appearanceKeys }
            }
        }
        ScrollView {
            id: page4
            objectName: "settingsPage4"
            contentWidth: availableWidth
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            GridLayout {
                width: page4.availableWidth - 16
                columns: 2
                columnSpacing: 24
                rowSpacing: 12
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; opacity: 0.75
                text: "До " + win.maxRegions + " областей. Каждая распознаётся отдельно; пустые поля берутся из общих настроек. "
                      + "Выключенная область сохраняет выделение."
            }
            FieldLabel { text: "Разрешить несколько активных областей" }
            Switch {
                objectName: "allowMultipleRegions"
                checked: win.current.allow_multiple_regions === true
                onToggled: win.setAllowMultipleRegions(checked)
            }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; opacity: 0.7
                text: win.current.allow_multiple_regions === true
                      ? "Активными могут быть до " + win.maxRegions + " областей одновременно."
                      : "Активна одна область: включение другой выключает предыдущую."
            }
            Repeater {
                model: win.current.regions || []
                delegate: Frame {
                    id: regionFrame
                    required property var modelData
                    required property int index
                    readonly property bool hasRect: !!modelData.rect
                    Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0
                    GridLayout {
                        width: parent.width
                        columns: 2
                        columnSpacing: 16
                        rowSpacing: 8
                        Switch {
                            text: "Активна"
                            checked: regionFrame.modelData.enabled === true
                            onToggled: win.activateRegion(regionFrame.index, checked)
                        }
                        TextField {
                            Layout.fillWidth: true; Layout.minimumWidth: 0
                            text: regionFrame.modelData.name
                            onEditingFinished: if (text.trim().length && text !== regionFrame.modelData.name) win.setRegionField(regionFrame.index, "name", text.trim())
                        }
                        FieldLabel {
                            text: regionFrame.hasRect ? "Область задана" : "Область не задана"
                            color: regionFrame.hasRect ? palette.text : "#ffb347"
                        }
                        RowLayout {
                            Layout.fillWidth: true; Layout.minimumWidth: 0
                            Button {
                                text: "Выделить на экране"
                                enabled: win.controller.windowTitle !== undefined && win.controller.windowTitle.length > 0
                                onClicked: {
                                    win.activateRegion(regionFrame.index, true)
                                    win.apply()
                                    win.selectRegionRequested()
                                }
                            }
                            Button {
                                text: "Очистить"
                                enabled: regionFrame.hasRect
                                onClicked: win.setRegionField(regionFrame.index, "rect", null)
                            }
                            Item { Layout.fillWidth: true }
                            Button {
                                text: "Удалить"
                                enabled: (win.current.regions || []).length > 1
                                onClicked: win.removeRegion(regionFrame.index)
                            }
                        }
                        FieldLabel { text: "Язык текста (OCR)" }
                        ComboBox {
                            Layout.fillWidth: true; Layout.minimumWidth: 0
                            model: ["Как в общих настройках"].concat(win.langs)
                            currentIndex: Math.max(0, win.langs.indexOf(regionFrame.modelData.source_lang) + 1)
                            onActivated: win.setRegionField(regionFrame.index, "source_lang", currentIndex === 0 ? "" : currentText)
                        }
                        FieldLabel { text: "Язык перевода" }
                        ComboBox {
                            Layout.fillWidth: true; Layout.minimumWidth: 0
                            model: ["Как в общих настройках"].concat(win.targets)
                            currentIndex: Math.max(0, win.targets.indexOf(regionFrame.modelData.target_lang) + 1)
                            onActivated: win.setRegionField(regionFrame.index, "target_lang", currentIndex === 0 ? "" : currentText)
                        }
                        FieldLabel { text: "OCR-движок" }
                        ComboBox {
                            Layout.fillWidth: true; Layout.minimumWidth: 0
                            readonly property var values: ["", "tesseract", "paddleocr"]
                            model: ["Как в общих настройках", "Tesseract", "PaddleOCR"]
                            currentIndex: Math.max(0, values.indexOf(regionFrame.modelData.ocr_engine || ""))
                            onActivated: win.setRegionField(regionFrame.index, "ocr_engine", values[currentIndex])
                        }
                        FieldLabel { text: "Рамка области" }
                        ComboBox {
                            Layout.fillWidth: true; Layout.minimumWidth: 0
                            model: ["Как в общих настройках"].concat(win.frameModes.map(m => m.label))
                            currentIndex: Math.max(0, win.frameModes.findIndex(m => m.value === regionFrame.modelData.frame_mode) + 1)
                            onActivated: win.setRegionField(regionFrame.index, "frame_mode", currentIndex === 0 ? "" : win.frameModes[currentIndex - 1].value)
                        }
                        FieldLabel { text: "Интервал / ожидание стабилизации, мс" }
                        RowLayout {
                            Layout.fillWidth: true; Layout.minimumWidth: 0
                            SpinBox {
                                Layout.fillWidth: true; Layout.minimumWidth: 0
                                from: 100; to: 10000; stepSize: 50; editable: true
                                value: regionFrame.modelData.interval_ms || 500
                                onValueModified: win.setRegionField(regionFrame.index, "interval_ms", value)
                            }
                            SpinBox {
                                Layout.fillWidth: true; Layout.minimumWidth: 0
                                from: 0; to: 5000; stepSize: 50; editable: true
                                value: regionFrame.modelData.debounce_ms || 0
                                onValueModified: win.setRegionField(regionFrame.index, "debounce_ms", value)
                            }
                        }
                    }
                }
            }
            RowLayout {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0
                // Stays enabled at the limit: pressing it explains why nothing was added.
                Button { objectName: "addRegion"; text: "Добавить область"; onClicked: win.addRegion() }
                Label { text: (win.current.regions || []).length + " из " + win.maxRegions; opacity: 0.7 }
            }
            Label {
                objectName: "regionNotice"
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap
                visible: win.regionNotice.length > 0
                text: win.regionNotice
                color: "#ffc23d"
            }
            ResetButton { keys: ["regions", "active_region", "allow_multiple_regions"]; text: "Сбросить области по умолчанию (выделения будут очищены)" }
            }
        }
        ScrollView {
            id: page5
            objectName: "settingsPage5"
            contentWidth: availableWidth
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            GridLayout {
                width: page5.availableWidth - 16
                columns: 2
                columnSpacing: 24
                rowSpacing: 14
            // ── Горячие клавиши ─────────────────────────────────────────────────
            Label { text: "Горячие клавиши"; font.bold: true; Layout.columnSpan: 2 }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; opacity: 0.7
                text: "Нажмите кнопку и сочетание. Backspace — очистить, Esc — отмена. Применяются автоматически; "
                      + "если сочетание занято другой программой, будет показано в статусе."
            }
            Repeater {
                model: win.hotkeyRows
                delegate: RowLayout {
                    Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0
                    Label { wrapMode: Text.Wrap; text: modelData.title; Layout.fillWidth: true; Layout.minimumWidth: 0 }
                    HotkeyButton {
                        Layout.preferredWidth: 190
                        value: (win.current.hotkeys || ({}))[modelData.key] || ""
                        conflict: win.hotkeyDuplicate(modelData.key)
                        onEdited: (v) => win.setHotkey(modelData.key, v)
                    }
                }
            }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; color: "#ff6b6b"
                visible: win.hasDuplicateHotkeys
                text: "Одинаковые сочетания у разных действий — работать будет только одно."
            }
            Button {
                Layout.columnSpan: 2
                text: "Сбросить клавиши по умолчанию"
                onClicked: win.resetKeys(["hotkeys"])
            }

            }
        }
        ScrollView {
            id: page6
            objectName: "settingsPage6"
            contentWidth: availableWidth
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            GridLayout {
                width: page6.availableWidth - 16
                columns: 2
                columnSpacing: 24
                rowSpacing: 10
            RowLayout {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0
                Label {
                    Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; opacity: 0.75
                    text: "Проверка только читает состояние системы: ничего не устанавливает и не скачивает."
                }
                Button {
                    text: win.controller.diagnosticsBusy ? "Проверка…" : "Проверить снова"
                    enabled: !win.controller.diagnosticsBusy
                    onClicked: win.controller.refreshDiagnostics()
                }
            }
            Repeater {
                model: win.diagnostics
                delegate: Frame {
                    id: checkFrame
                    required property var modelData
                    readonly property color stateColor: modelData.state === "ready" ? "#3ecf6e" : modelData.state === "warning" ? "#ffc23d" : "#ff5555"
                    Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0
                    ColumnLayout {
                        width: parent.width
                        spacing: 4
                        RowLayout {
                            Layout.fillWidth: true
                            Rectangle { implicitWidth: 12; implicitHeight: 12; radius: 6; color: checkFrame.stateColor }
                            Label { text: modelData.name; font.bold: true; Layout.fillWidth: true; Layout.minimumWidth: 0; elide: Text.ElideRight }
                            Label {
                                text: modelData.state === "ready" ? "готово" : modelData.state === "warning" ? "внимание" : "нет"
                                color: checkFrame.stateColor
                            }
                        }
                        Label {
                            Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.WrapAnywhere; opacity: 0.7; font.pixelSize: 12
                            visible: text.length > 0; text: modelData.detail
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            visible: modelData.state !== "ready"
                            TextEdit {
                                Layout.fillWidth: true; Layout.minimumWidth: 0
                                readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap
                                color: palette.text; selectionColor: palette.highlight
                                text: modelData.instruction
                            }
                            Button { text: "Копировать"; flat: true; onClicked: win.controller.copyText(modelData.instruction) }
                        }
                    }
                }
            }
            Label {
                Layout.columnSpan: 2; visible: win.diagnostics.length === 0; opacity: 0.6
                text: win.controller.diagnosticsBusy ? "Идёт проверка…" : "Нажмите «Проверить снова»"
            }
            }
        }
        ScrollView {
            id: page7
            objectName: "settingsPage7"
            contentWidth: availableWidth
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            GridLayout {
                width: page7.availableWidth - 16
                columns: 1
                rowSpacing: 20
                Item {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 155
                    Image {
                        id: aboutIcon
                        objectName: "aboutIcon"
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: 140; height: 140
                        source: Qt.resolvedUrl(".").toString().indexOf("qrc:") === 0 ? "qrc:/lipa/icon.svg" : "../assets/lipa.svg"
                        sourceSize.width: 280; sourceSize.height: 280
                        SequentialAnimation on y {
                            running: win.visible && tabs.currentIndex === 7
                            loops: Animation.Infinite
                            NumberAnimation { from: 10; to: 2; duration: 850; easing.type: Easing.InOutSine }
                            NumberAnimation { from: 2; to: 10; duration: 850; easing.type: Easing.InOutSine }
                        }
                    }
                }
                Item {
                    id: tickerViewport
                    Layout.fillWidth: true; Layout.preferredHeight: 32
                    clip: true
                    Label {
                        id: tickerText
                        objectName: "aboutTicker"
                        text: "LipaX — распознавание и перевод игрового текста · Qt / KDE / Wayland"
                        font.pixelSize: 16
                        NumberAnimation on x {
                            running: win.visible && tabs.currentIndex === 7
                            from: tickerViewport.width; to: -tickerText.implicitWidth
                            duration: 18000; loops: Animation.Infinite
                        }
                    }
                }
                Label { text: "Автор: GrayRat"; font.bold: true; font.pixelSize: 18 }
                Label {
                    Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap
                    textFormat: Text.RichText
                    text: 'Репозиторий проекта: <a href="https://github.com/GrayRats/lipax-qt">https://github.com/GrayRats/lipax-qt</a>'
                    onLinkActivated: (url) => Qt.openUrlExternally(url)
                }
                Label { text: "Другие исходники проекта:"; font.bold: true }
                Repeater {
                    model: [
                        {url: "https://github.com/satix-one/lipa.git", role: "форк"},
                        {url: "https://github.com/rtr46/meikipop", role: "зависимость"},
                        {url: "https://github.com/tesseract-ocr/tesseract", role: "зависимость"},
                        {url: "https://github.com/tesseract-ocr/tessdata", role: "зависимость"}
                    ]
                    Label {
                        required property var modelData
                        Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap
                        textFormat: Text.RichText
                        text: '<a href="' + modelData.url + '">' + modelData.url + '</a> (' + modelData.role + ')'
                        onLinkActivated: (url) => Qt.openUrlExternally(url)
                    }
                }
            }
        }
    }
}
