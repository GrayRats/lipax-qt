import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

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

    Component.onCompleted: reload()
    function reload() { current = JSON.parse(controller.settingsJson()) }
    function openWindow() { reload(); show(); raise(); requestActivate() }
    function set(key, v) { const c = Object.assign({}, current); c[key] = v; current = c }
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
        { key: "toggle_overlay", title: "Показать / скрыть перевод" }
    ]
    function setHotkey(key, v) { const h = Object.assign({}, current.hotkeys || ({})); h[key] = v; set("hotkeys", h) }
    function hotkeyDuplicate(key) {
        const h = current.hotkeys || ({})
        return !!h[key] && hotkeyRows.some(r => r.key !== key && h[r.key] === h[key])
    }
    readonly property bool hasDuplicateHotkeys: hotkeyRows.some(r => hotkeyDuplicate(r.key))

    readonly property bool frameColorValid: /^#[0-9a-fA-F]{6}$/.test(current.frame_color || "")

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

    function apply() { controller.applySettings(JSON.stringify(current)); reload() }

    Connections { target: win.controller; function onHasRegionChanged() { win.reload() } }

    header: TabBar {
        id: tabs
        objectName: "settingsTabs"
        TabButton { text: "Распознавание" }
        TabButton { text: "Перевод и захват" }
        TabButton { text: "Внешний вид" }
        TabButton { text: "Клавиши" }
    }
    footer: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.margins: 10
            Label { text: "Изменения сохраняются кнопкой «Применить»"; wrapMode: Text.Wrap; Layout.fillWidth: true; opacity: 0.7 }
            Button { text: "Закрыть"; onClicked: win.close() }
            Button { text: "Применить"; highlighted: true; onClicked: win.apply() }
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
                text: "PaddleOCR использует основной язык; дополнительные языки относятся к Tesseract. При первом запуске загружаются модели. Установка: docs/PADDLEOCR.md."
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
            Label { Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Чувствительность к изменениям (ниже — чувствительнее)" ; wrapMode: Text.Wrap; Layout.fillWidth: true }
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
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Размер шрифта" }
            SpinBox {
                from: 8; to: 96; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                value: win.current.font_size || 20
                onValueModified: win.set("font_size", value)
            }
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Непрозрачность фона" }
            Slider {
                from: 0.1; to: 1; Layout.fillWidth: true; Layout.minimumWidth: 0
                value: win.current.opacity !== undefined ? win.current.opacity : 0.85
                onMoved: win.set("opacity", value)
            }
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Пропускать клики мыши" }
            Switch { checked: win.current.click_through !== false; onToggled: win.set("click_through", checked) }
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Положение перевода (x, y)" }
            RowLayout {
                SpinBox {
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    from: 0; to: 10000; editable: true
                    value: win.current.overlay_pos ? win.current.overlay_pos[0] : 0
                    onValueModified: win.set("overlay_pos", [value, win.current.overlay_pos[1]])
                }
                SpinBox {
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    from: 0; to: 10000; editable: true
                    value: win.current.overlay_pos ? win.current.overlay_pos[1] : 0
                    onValueModified: win.set("overlay_pos", [win.current.overlay_pos[0], value])
                }
            }
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Размер перевода (ш, в)" }
            RowLayout {
                SpinBox {
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    from: 100; to: 5000; editable: true
                    value: win.current.overlay_size ? win.current.overlay_size[0] : 700
                    onValueModified: win.set("overlay_size", [value, win.current.overlay_size[1]])
                }
                SpinBox {
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    from: 40; to: 3000; editable: true
                    value: win.current.overlay_size ? win.current.overlay_size[1] : 120
                    onValueModified: win.set("overlay_size", [win.current.overlay_size[0], value])
                }
            }

            // ── Рамка выбора ────────────────────────────────────────────────────
            Label { text: "Рамка выбора окна и области"; font.bold: true; Layout.columnSpan: 2 }
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Цвет рамки" }
            RowLayout {
                Layout.fillWidth: true; Layout.minimumWidth: 0
                Rectangle {
                    implicitWidth: 28; implicitHeight: 28; radius: 4
                    color: win.frameColorValid ? win.current.frame_color : "transparent"
                    border.width: 1; border.color: palette.mid
                }
                TextField {
                    Layout.fillWidth: true; Layout.minimumWidth: 0
                    placeholderText: "#ff0000"
                    maximumLength: 7
                    text: win.current.frame_color || "#ff0000"
                    onTextEdited: win.set("frame_color", text)
                }
                Repeater {
                    model: ["#ff0000", "#00c800", "#1e90ff", "#ffd400", "#ffffff"]
                    Rectangle {
                        implicitWidth: 22; implicitHeight: 22; radius: 4; color: modelData
                        border.width: 1; border.color: palette.mid
                        MouseArea { anchors.fill: parent; onClicked: win.set("frame_color", modelData) }
                    }
                }
            }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; wrapMode: Text.Wrap
                visible: !win.frameColorValid
                color: "#ff6b6b"
                text: "Введите цвет в формате #rrggbb, например #ff0000."
            }
            Label { wrapMode: Text.Wrap; Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Толщина рамки, px" }
            SpinBox {
                from: 1; to: 12; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                value: win.current.frame_width || 2
                onValueModified: win.set("frame_width", value)
            }
            Label { Layout.preferredWidth: 230; Layout.maximumWidth: 230; Layout.minimumWidth: 0; text: "Показывать после выбора, с (0 — не показывать)"; wrapMode: Text.Wrap; Layout.fillWidth: true }
            SpinBox {
                from: 0; to: 30; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                value: win.current.frame_seconds !== undefined ? win.current.frame_seconds : 3
                onValueModified: win.set("frame_seconds", value)
            }

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
                rowSpacing: 14
            // ── Горячие клавиши ─────────────────────────────────────────────────
            Label { text: "Горячие клавиши"; font.bold: true; Layout.columnSpan: 2 }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; opacity: 0.7
                text: "Нажмите кнопку и сочетание. Backspace — очистить, Esc — отмена. Применяются сразу после «Применить»; "
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
                onClicked: win.set("hotkeys", ({ toggle: "Ctrl+Alt+P", select_region: "Ctrl+Alt+R", translate_once: "Ctrl+Alt+Y", toggle_overlay: "Ctrl+Alt+H" }))
            }

            }
        }
    }
}
