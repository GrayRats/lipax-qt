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
    width: 520
    height: 640
    visible: false

    Component.onCompleted: reload()
    function reload() { current = JSON.parse(controller.settingsJson()) }
    function openWindow() { reload(); show(); raise(); requestActivate() }
    function set(key, v) { const c = current; c[key] = v; current = c }
    // ── Tesseract ───────────────────────────────────────────────────────
    readonly property var tess: {
        try { return JSON.parse(controller.tesseractJson || "{}") } catch (e) { return ({}) }
    }
    readonly property var specParts: (current.source_lang || "eng").split("+").filter(x => x.length > 0)
    readonly property string primaryLang: specParts.length ? specParts[0] : "eng"
    readonly property var extraLangs: specParts.slice(1)
    // Установленные языки плюс выбранные, но отсутствующие (помечены).
    readonly property var langModel: {
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
        const _ = controller.tesseractJson
        try { return JSON.parse(controller.missingLanguages(current.source_lang || "eng")) } catch (e) { return [] }
    }

    readonly property var hotkeyRows: [
        { key: "toggle", title: "Запустить / остановить слежение" },
        { key: "select_region", title: "Выбрать область перевода" },
        { key: "translate_once", title: "Перевести сейчас" },
        { key: "toggle_overlay", title: "Показать / скрыть overlay" }
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

    ScrollView {
        anchors.fill: parent
        anchors.margins: 12
        contentWidth: availableWidth

        GridLayout {
            width: parent.width
            columns: 2
            columnSpacing: 10
            rowSpacing: 8

            // ── Tesseract OCR: состояние и языки ────────────────────────────────
            Label { text: "Tesseract OCR"; font.bold: true; Layout.columnSpan: 2 }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; wrapMode: Text.Wrap
                visible: !win.tess.distro || win.tess.installed
                text: !win.tess.distro ? "Проверка…"
                    : "Установлен · " + (win.tess.version ? "версия " + win.tess.version : "версия неизвестна")
                      + "\nПуть: " + win.tess.path
                      + "\ntessdata: " + (win.tess.tessdata || "не определён")
                      + "\nСистема: " + win.tess.distro.name + " · менеджер пакетов: " + win.tess.package_manager
                      + (win.tess.engine_package ? "\nПакет: " + win.tess.engine_package : "")
                      + (win.tess.tessdata_package ? " · данные: " + win.tess.tessdata_package + " и др." : "")
            }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; wrapMode: Text.Wrap
                visible: !!win.tess.distro && !win.tess.installed
                color: "#ff6b6b"
                text: "Tesseract не установлен" + (win.tess.engine_install_command ? ". Команда установки: " + win.tess.engine_install_command : "")
            }

            Label { text: "Основной язык текста (OCR)" }
            ComboBox {
                id: primaryBox
                Layout.fillWidth: true
                model: win.langModel
                textRole: "label"
                currentIndex: Math.max(0, win.langModel.findIndex(l => l.code === win.primaryLang))
                onActivated: win.setLangs(win.langModel[currentIndex].code, win.extraLangs)
            }
            Label { text: "Дополнительные языки (по желанию)"; wrapMode: Text.Wrap; Layout.fillWidth: true }
            Flow {
                Layout.fillWidth: true
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
                Layout.columnSpan: 2; Layout.fillWidth: true; wrapMode: Text.Wrap
                text: "Будет использовано: " + (win.current.source_lang || "eng")
                opacity: 0.7
            }

            // Выбраны языки, которых нет в Tesseract: понятное сообщение и пакет для этого дистрибутива.
            Repeater {
                model: win.missing
                delegate: RowLayout {
                    Layout.columnSpan: 2; Layout.fillWidth: true
                    Label { Layout.fillWidth: true; wrapMode: Text.Wrap; color: "#ff6b6b"; text: modelData.message }
                    Button {
                        visible: modelData.package !== null && win.tess.package_manager !== "unknown"
                        text: "Установить языковой пакет"
                        enabled: !win.controller.tesseractBusy
                        onClicked: win.askInstall(modelData.package, modelData.name)
                    }
                }
            }

            Label { text: "Установить другой язык" }
            RowLayout {
                Layout.fillWidth: true
                ComboBox {
                    id: installBox
                    Layout.fillWidth: true
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
                text: win.controller.tesseractBusy ? "Проверка…" : "Обновить список языков"
                enabled: !win.controller.tesseractBusy
                onClicked: win.controller.refreshTesseract()
            }

            Label { text: "Язык перевода" }
            ComboBox {
                Layout.fillWidth: true
                model: win.targets
                currentIndex: Math.max(0, win.targets.indexOf(win.current.target_lang))
                onActivated: win.set("target_lang", currentText)
            }
            Label { text: "OCR-движок" }
            ComboBox { Layout.fillWidth: true; model: ["tesseract"]; enabled: false }

            Label { text: "Захват окна" }
            ColumnLayout {
                Layout.fillWidth: true
                ComboBox {
                    Layout.fillWidth: true
                    model: ["Авто: KWin, а если недоступен — portal", "KWin ScreenShot2 (только KDE)", "xdg-desktop-portal (PipeWire)"]
                    currentIndex: Math.max(0, win.backends.indexOf(win.current.capture_backend))
                    onActivated: win.set("capture_backend", win.backends[currentIndex])
                }
                Label {
                    Layout.fillWidth: true; wrapMode: Text.Wrap; opacity: 0.7
                    visible: win.current.capture_backend !== "kwin"
                    text: "Portal показывает системный диалог выбора окна и требует gstreamer с gst-plugin-pipewire. "
                          + "Выбор запоминается, диалог при следующем запуске не нужен. Выбранное окно переключается заново кнопкой «Выбрать окно»."
                }
            }

            Label { text: "Сервис перевода" }
            ComboBox {
                id: tr
                Layout.fillWidth: true
                model: ["Google Translate", "Yandex Translate", "Свой API"]
                currentIndex: Math.max(0, win.translators.indexOf(win.current.translator))
                onActivated: win.set("translator", win.translators[currentIndex])
            }

            Label { text: "Yandex API-ключ"; visible: tr.currentIndex === 1 }
            TextField {
                visible: tr.currentIndex === 1; Layout.fillWidth: true
                echoMode: TextInput.Password
                text: win.current.yandex_api_key || ""
                onEditingFinished: win.set("yandex_api_key", text)
            }
            Label { text: "Yandex folder ID"; visible: tr.currentIndex === 1 }
            TextField {
                visible: tr.currentIndex === 1; Layout.fillWidth: true
                text: win.current.yandex_folder_id || ""
                onEditingFinished: win.set("yandex_folder_id", text)
            }
            Label { text: "URL API (LibreTranslate-формат)"; visible: tr.currentIndex === 2 }
            TextField {
                visible: tr.currentIndex === 2; Layout.fillWidth: true
                placeholderText: "https://host/translate"
                text: win.current.custom_url || ""
                onEditingFinished: win.set("custom_url", text)
            }
            Label { text: "Ключ API (необязательно)"; visible: tr.currentIndex === 2 }
            TextField {
                visible: tr.currentIndex === 2; Layout.fillWidth: true
                echoMode: TextInput.Password
                text: win.current.custom_api_key || ""
                onEditingFinished: win.set("custom_api_key", text)
            }

            Label { text: "Интервал проверки, мс" }
            SpinBox {
                from: 100; to: 5000; stepSize: 50; editable: true; Layout.fillWidth: true
                value: win.current.interval_ms || 500
                onValueModified: win.set("interval_ms", value)
            }
            Label { text: "Чувствительность к изменениям (ниже — чувствительнее)" ; wrapMode: Text.Wrap; Layout.fillWidth: true }
            Slider {
                from: 0.2; to: 20; Layout.fillWidth: true
                value: win.current.sensitivity || 2
                onMoved: win.set("sensitivity", value)
            }
            Label { text: "Задержка debounce, мс" }
            SpinBox {
                from: 0; to: 3000; stepSize: 50; editable: true; Layout.fillWidth: true
                value: win.current.debounce_ms || 0
                onValueModified: win.set("debounce_ms", value)
            }
            Label { text: "Автоматический перевод" }
            Switch { checked: win.current.auto_translate !== false; onToggled: win.set("auto_translate", checked) }

            Label { text: "Размер шрифта" }
            SpinBox {
                from: 8; to: 96; editable: true; Layout.fillWidth: true
                value: win.current.font_size || 20
                onValueModified: win.set("font_size", value)
            }
            Label { text: "Прозрачность overlay" }
            Slider {
                from: 0.1; to: 1; Layout.fillWidth: true
                value: win.current.opacity !== undefined ? win.current.opacity : 0.85
                onMoved: win.set("opacity", value)
            }
            Label { text: "Пропускать клики мыши" }
            Switch { checked: win.current.click_through !== false; onToggled: win.set("click_through", checked) }
            Label { text: "Положение overlay (x, y)" }
            RowLayout {
                SpinBox {
                    from: 0; to: 10000; editable: true
                    value: win.current.overlay_pos ? win.current.overlay_pos[0] : 0
                    onValueModified: win.set("overlay_pos", [value, win.current.overlay_pos[1]])
                }
                SpinBox {
                    from: 0; to: 10000; editable: true
                    value: win.current.overlay_pos ? win.current.overlay_pos[1] : 0
                    onValueModified: win.set("overlay_pos", [win.current.overlay_pos[0], value])
                }
            }
            Label { text: "Размер overlay (ш, в)" }
            RowLayout {
                SpinBox {
                    from: 100; to: 5000; editable: true
                    value: win.current.overlay_size ? win.current.overlay_size[0] : 700
                    onValueModified: win.set("overlay_size", [value, win.current.overlay_size[1]])
                }
                SpinBox {
                    from: 40; to: 3000; editable: true
                    value: win.current.overlay_size ? win.current.overlay_size[1] : 120
                    onValueModified: win.set("overlay_size", [win.current.overlay_size[0], value])
                }
            }

            // ── Рамка выбора ────────────────────────────────────────────────────
            Label { text: "Рамка выбора окна и области"; font.bold: true; Layout.columnSpan: 2 }
            Label { text: "Цвет рамки" }
            RowLayout {
                Layout.fillWidth: true
                Rectangle {
                    width: 28; height: 28; radius: 4
                    color: win.frameColorValid ? win.current.frame_color : "transparent"
                    border.width: 1; border.color: palette.mid
                }
                TextField {
                    Layout.fillWidth: true
                    placeholderText: "#ff0000"
                    maximumLength: 7
                    text: win.current.frame_color || "#ff0000"
                    color: win.frameColorValid ? palette.text : "#d9534f"
                    onTextEdited: win.set("frame_color", text)
                }
                Repeater {
                    model: ["#ff0000", "#00c800", "#1e90ff", "#ffd400", "#ffffff"]
                    Rectangle {
                        width: 22; height: 22; radius: 4; color: modelData
                        border.width: 1; border.color: palette.mid
                        MouseArea { anchors.fill: parent; onClicked: win.set("frame_color", modelData) }
                    }
                }
            }
            Label { text: "Толщина рамки, px" }
            SpinBox {
                from: 1; to: 12; editable: true; Layout.fillWidth: true
                value: win.current.frame_width || 2
                onValueModified: win.set("frame_width", value)
            }
            Label { text: "Показывать после выбора, с (0 — не показывать)"; wrapMode: Text.Wrap; Layout.fillWidth: true }
            SpinBox {
                from: 0; to: 30; editable: true; Layout.fillWidth: true
                value: win.current.frame_seconds !== undefined ? win.current.frame_seconds : 3
                onValueModified: win.set("frame_seconds", value)
            }

            // ── Горячие клавиши ─────────────────────────────────────────────────
            Label { text: "Горячие клавиши"; font.bold: true; Layout.columnSpan: 2 }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; wrapMode: Text.Wrap; opacity: 0.7
                text: "Нажмите кнопку и сочетание. Backspace — очистить, Esc — отмена. Применяются сразу после «Применить»; "
                      + "если сочетание занято другой программой, будет показано в статусе."
            }
            Repeater {
                model: win.hotkeyRows
                delegate: RowLayout {
                    Layout.columnSpan: 2; Layout.fillWidth: true
                    Label { text: modelData.title; Layout.fillWidth: true }
                    HotkeyButton {
                        Layout.preferredWidth: 190
                        value: (win.current.hotkeys || ({}))[modelData.key] || ""
                        conflict: win.hotkeyDuplicate(modelData.key)
                        onEdited: (v) => win.setHotkey(modelData.key, v)
                    }
                }
            }
            Label {
                Layout.columnSpan: 2; Layout.fillWidth: true; wrapMode: Text.Wrap; color: "#ff6b6b"
                visible: win.hasDuplicateHotkeys
                text: "Одинаковые сочетания у разных действий — работать будет только одно."
            }
            Button {
                Layout.columnSpan: 2
                text: "Сбросить клавиши по умолчанию"
                onClicked: win.set("hotkeys", ({ toggle: "Ctrl+Alt+P", select_region: "Ctrl+Alt+R", translate_once: "Ctrl+Alt+Y", toggle_overlay: "Ctrl+Alt+H" }))
            }

            RowLayout {
                Layout.columnSpan: 2
                Layout.alignment: Qt.AlignRight
                Button { text: "Закрыть"; onClicked: win.close() }
                Button { text: "Применить"; highlighted: true; onClicked: win.apply() }
            }
        }
    }
}
