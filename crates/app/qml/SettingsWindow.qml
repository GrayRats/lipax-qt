import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

ApplicationWindow {
    id: win
    objectName: "settingsWindow"
    property var controller
    property var current: ({
            capture: {},
            recognition: {},
            translation: {},
            translation_window: {},
            appearance: {
                window: {},
                inplace: {}
            }
        })
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

    signal selectRegionRequested

    Component.onCompleted: reload()
    // Every edit is previewed at once (translation window binds to `current`) and saved after a short pause.
    property string lastApplied: ""
    function reload() {
        const loaded = JSON.parse(controller.settingsJson());
        lastApplied = JSON.stringify(loaded);
        current = loaded;
    }
    function openWindow() {
        reload();
        show();
        raise();
        requestActivate();
    }
    function get(object, path) {
        return path.split(".").reduce((o, k) => o === undefined || o === null ? undefined : o[k], object);
    }
    function assign(object, path, value) {
        const parts = path.split(".");
        let node = object;
        for (let i = 0; i < parts.length - 1; ++i) {
            if (!node[parts[i]])
                node[parts[i]] = ({});
            node = node[parts[i]];
        }
        node[parts[parts.length - 1]] = value;
    }
    function set(key, v) {
        const c = JSON.parse(JSON.stringify(current));
        assign(c, key, v);
        current = c;
    }
    onCurrentChanged: if (JSON.stringify(current) !== lastApplied)
        autosave.restart()
    Timer {
        id: autosave
        interval: 500
        onTriggered: win.apply()
    }
    onClosing: if (autosave.running) {
        autosave.stop();
        apply();
    }
    function resetKeys(keys) {
        const d = JSON.parse(controller.defaultSettingsJson());
        const c = JSON.parse(JSON.stringify(current));
        for (const k of keys)
            assign(c, k, get(d, k));
        current = c;
    }
    readonly property var windowBackgroundStyles: [
        {
            value: "blur",
            label: "Фон с размытием"
        },
        {
            value: "transparent",
            label: "Без фона (прозрачный)"
        },
        {
            value: "dim",
            label: "Лёгкий тёмный фон"
        },
        {
            value: "solid",
            label: "Сплошной фон"
        }
    ]
    readonly property string windowDisplayDescription: "Показывает перевод в отдельном окне. Его можно закрепить поверх игры или сделать свободным и перемещать мышью. Положение и размер задаются на вкладке «Окно перевода»."
    readonly property string inplaceDisplayDescription: "Размещает перевод на месте исходного текста в области захвата. Требуются точные координаты окна и шрифт для языка перевода. Если способ недоступен, причина указана в подсказке."
    readonly property bool solidStyle: (current.appearance.window.background_style || "solid") === "solid"
    // Largest connected screen in logical pixels: bounds for window position and size.
    readonly property int screenMaxWidth: Math.max(200, ...Qt.application.screens.map(s => s.width))
    readonly property int screenMaxHeight: Math.max(60, ...Qt.application.screens.map(s => s.height))
    readonly property var fontFamilies: bundledFonts
    function fontIndex(name) {
        return name ? Math.max(0, fontFamilies.indexOf(name)) : 0;
    }

    // ── «Поверх исходного текста»: каждое свойство отдельно — Авто или Вручную ──
    function inplace() {
        return current.appearance.inplace || ({});
    }
    function setInplace(key, value) {
        set("appearance.inplace." + key, value);
    }
    function propManual(key) {
        const v = inplace()[key];
        return !!v && v.mode === "manual";
    }
    function propValue(key, fallback) {
        const v = inplace()[key];
        return v && v.mode === "manual" ? v.value : fallback;
    }
    function setProp(key, manual, value) {
        setInplace(key, manual ? {
            mode: "manual",
            value: value
        } : {
            mode: "auto"
        });
    }
    readonly property var inplaceBackgrounds: [
        {
            value: "auto",
            label: "Авто (по фону вокруг текста)"
        },
        {
            value: "text_replacement",
            label: "Подмена текста"
        },
        {
            value: "transparent_outline",
            label: "Прозрачный фон + обводка"
        },
        {
            value: "padded_fill",
            label: "Заливка с дополнительными полями"
        }
    ]
    readonly property var weightNames: [
        {
            value: "thin",
            label: "Тонкий"
        },
        {
            value: "extra_light",
            label: "Сверхсветлый"
        },
        {
            value: "light",
            label: "Светлый"
        },
        {
            value: "normal",
            label: "Обычный"
        },
        {
            value: "medium",
            label: "Средний"
        },
        {
            value: "demi_bold",
            label: "Полужирный"
        },
        {
            value: "bold",
            label: "Жирный"
        },
        {
            value: "extra_bold",
            label: "Сверхжирный"
        },
        {
            value: "black",
            label: "Чёрный"
        }
    ]
    readonly property var wrapNames: [
        {
            value: "word_wrap",
            label: "По словам"
        },
        {
            value: "wrap_anywhere",
            label: "По символам"
        },
        {
            value: "no_wrap",
            label: "Без переноса"
        },
        {
            value: "elide",
            label: "Обрезать с многоточием"
        }
    ]
    readonly property var fontCategories: [
        {
            value: "serif",
            label: "С засечками"
        },
        {
            value: "sans_serif",
            label: "Без засечек"
        },
        {
            value: "slab_serif",
            label: "Брусковые засечки"
        },
        {
            value: "monospace",
            label: "Моноширинный"
        },
        {
            value: "cjk_sans",
            label: "CJK без засечек"
        },
        {
            value: "cjk_serif",
            label: "CJK с засечками"
        }
    ]
    // The families LipaX ships (and Qt loaded); the registry lives in Rust.
    readonly property var bundledFonts: {
        try {
            return JSON.parse(controller.bundledFonts());
        } catch (e) {
            return [];
        }
    }
    // Строка свойства: «Авто / Вручную» и редактор значения (дочерние элементы экземпляра).
    component PropRow: RowLayout {
        id: propRow
        property string key
        property var defaultValue
        readonly property bool manual: win.propManual(key)
        Layout.fillWidth: true
        Layout.minimumWidth: 0
        ComboBox {
            Layout.preferredWidth: 120
            model: ["Авто", "Вручную"]
            currentIndex: propRow.manual ? 1 : 0
            onActivated: win.setProp(propRow.key, currentIndex === 1, win.propValue(propRow.key, propRow.defaultValue))
        }
    }

    component FieldLabel: Label {
        id: fieldLabel
        property string helpText: ""
        property Item helpControl: null
        Accessible.description: helpText
        HoverHint {
            control: fieldLabel
            feature: fieldLabel.text
            explanation: feature + "\n\n" + fieldLabel.helpText
            active: fieldLabel.helpText.length > 0
        }
        HoverHint {
            control: fieldLabel.helpControl || fieldLabel
            feature: fieldLabel.text
            explanation: feature + "\n\n" + fieldLabel.helpText
            active: fieldLabel.helpText.length > 0 && !!fieldLabel.helpControl && fieldLabel.helpControl.enabled
        }
        wrapMode: Text.Wrap
        Layout.preferredWidth: 230
        Layout.maximumWidth: 230
        Layout.minimumWidth: 0
    }
    component SectionTitle: Label {
        font.bold: true
        Layout.columnSpan: 2
        Layout.topMargin: 6
    }
    component ResetButton: Button {
        property var keys: []
        Layout.columnSpan: 2
        Layout.topMargin: 10
        text: keys.some(k => k.startsWith("appearance.")) ? "Сбросить настройки оформления" : "Сбросить настройки раздела"
        onClicked: win.resetKeys(keys)
    }
    // Swatch opens the system color dialog; text accepts only complete #rrggbb values.
    component ColorRow: RowLayout {
        id: colorRow
        property string key
        property var presets: []
        readonly property string value: win.get(win.current, key) || "#000000"
        Layout.fillWidth: true
        Layout.minimumWidth: 0
        Rectangle {
            implicitWidth: 28
            implicitHeight: 28
            radius: 4
            color: colorRow.value
            border.width: 1
            border.color: palette.mid
            MouseArea {
                anchors.fill: parent
                onClicked: {
                    colorDialog.key = colorRow.key;
                    colorDialog.selectedColor = colorRow.value;
                    colorDialog.open();
                }
            }
        }
        TextField {
            Layout.preferredWidth: 100
            maximumLength: 7
            text: colorRow.value
            validator: RegularExpressionValidator {
                regularExpression: /#[0-9a-fA-F]{0,6}/
            }
            onTextEdited: if (/^#[0-9a-fA-F]{6}$/.test(text))
                win.set(colorRow.key, text.toLowerCase())
        }
        Repeater {
            model: colorRow.presets
            Rectangle {
                implicitWidth: 22
                implicitHeight: 22
                radius: 4
                color: modelData
                border.width: 1
                border.color: palette.mid
                MouseArea {
                    anchors.fill: parent
                    onClicked: win.set(colorRow.key, modelData)
                }
            }
        }
        Item {
            Layout.fillWidth: true
        }
    }
    ColorDialog {
        id: colorDialog
        property string key
        onAccepted: win.set(key, selectedColor.toString().slice(0, 7))
    }
    // ── Tesseract ───────────────────────────────────────────────────────
    readonly property var tess: {
        try {
            return JSON.parse(controller.tesseractJson || "{}");
        } catch (e) {
            return ({});
        }
    }
    readonly property var specParts: (current.recognition.language || "eng").split("+").filter(x => x.length > 0)
    readonly property string primaryLang: specParts.length ? specParts[0] : "eng"
    readonly property var extraLangs: specParts.slice(1)
    // Установленные языки плюс выбранные, но отсутствующие (помечены).
    readonly property var langModel: {
        if (current.recognition.engine === "paddleocr")
            return langs.map(c => ({
                        code: c,
                        label: c,
                        installed: true
                    }));
        const have = (tess.languages || []).map(l => ({
                    code: l.code,
                    label: l.code + " — " + l.name,
                    installed: true
                }));
        for (const c of specParts)
            if (!have.some(l => l.code === c) && tess.installed)
                have.push({
                    code: c,
                    label: c + " — не установлен",
                    installed: false
                });
        if (have.length === 0)
            have.push({
                code: primaryLang,
                label: primaryLang,
                installed: false
            });
        return have;
    }
    readonly property var installable: (tess.installable || []).filter(l => l.available).map(l => ({
                code: l.code,
                package: l.package,
                name: l.name,
                command: l.command,
                label: l.code + " — " + l.name + " (" + l.package + ")"
            }))
    readonly property var missing: {
        // Зависимость от tesseractJson обновляет сообщения после установки пакета.
        if (current.recognition.engine === "paddleocr")
            return [];
        const _ = controller.tesseractJson;
        try {
            return JSON.parse(controller.missingLanguages(current.recognition.language || "eng"));
        } catch (e) {
            return [];
        }
    }

    readonly property var hotkeyRows: [
        {
            key: "toggle",
            title: "Запустить / остановить слежение"
        },
        {
            key: "select_region",
            title: "Выбрать область захвата"
        },
        {
            key: "translate_once",
            title: "Перевести сейчас"
        },
        {
            key: "toggle_translation",
            title: "Показать / скрыть перевод"
        },
        {
            key: "toggle_pin",
            title: "Закрепить окно / сделать окно свободным"
        }
    ]
    function setHotkey(key, v) {
        const h = Object.assign({}, current.hotkeys || ({}));
        h[key] = v;
        set("hotkeys", h);
    }
    function hotkeyDuplicate(key) {
        const h = current.hotkeys || ({});
        return !!h[key] && hotkeyRows.some(r => r.key !== key && h[r.key] === h[key]);
    }
    readonly property bool hasDuplicateHotkeys: hotkeyRows.some(r => hotkeyDuplicate(r.key))

    readonly property var frameModes: [
        {
            value: "pattern",
            label: "Узор (error purple/black)"
        },
        {
            value: "solid",
            label: "Простая красная обводка"
        },
        {
            value: "off",
            label: "Выкл."
        },
        {
            value: "selection",
            label: "При выделении"
        }
    ]

    function setLangs(primary, extras) {
        set("recognition.language", [primary].concat(extras.filter(c => c !== primary)).join("+"));
    }

    property string pendingPackage: ""
    property string pendingCommand: ""
    function askInstall(pkg, name) {
        const info = (tess.installable || []).find(l => l.package === pkg);
        pendingPackage = pkg;
        pendingCommand = info ? info.command : pkg;
        installDialog.text = "Установить языковой пакет «" + name + "»?\n\nБудет выполнена команда:\n" + pendingCommand + "\n\nСистема запросит пароль администратора.";
        installDialog.open();
    }
    Dialog {
        id: installDialog
        property string text: ""
        title: "Установка языкового пакета"
        modal: true
        anchors.centerIn: parent
        width: Math.min(parent.width - 40, 460)
        standardButtons: Dialog.Yes | Dialog.No
        Label {
            width: parent.width
            wrapMode: Text.Wrap
            text: installDialog.text
        }
        onAccepted: win.controller.installPackage(win.pendingPackage)
    }

    readonly property var settingPaths: JSON.parse(controller.editableSettingsPaths())
    function pendingPatch() {
        const previous = JSON.parse(lastApplied || "{}");
        const patch = ({});
        // Only registered leaves are patched; managed capture state never enters autosave.
        for (const key of settingPaths)
            if (JSON.stringify(get(current, key)) !== JSON.stringify(get(previous, key)))
                patch[key] = get(current, key);
        return patch;
    }
    function apply() {
        autosave.stop();
        const patch = pendingPatch();
        if (Object.keys(patch).length)
            controller.applySettingsPatch(JSON.stringify(patch));
        reload();
    }

    // Changes made elsewhere (window drag, pin, region selection) arrive here; pending edits are saved first.
    Connections {
        target: win.controller
        function onSettingsStateChanged() {
            if (autosave.running)
                win.apply();
            else
                win.reload();
        }
    }
    readonly property var gameProfileKeys: Object.keys(current.game_profiles || ({})).sort()
    readonly property string currentGameKey: current.capture.window && current.capture.window.resource_class ? current.capture.window.resource_class.trim().toLowerCase() : ""
    // What the capture backend of the chosen window can do (see CaptureCapabilities in core).
    readonly property bool usesPaddle: current.recognition.engine === "paddleocr" || current.recognition.engine === "auto"
    readonly property var capabilities: {
        try {
            return JSON.parse(controller.captureCapabilities || "{}");
        } catch (e) {
            return ({});
        }
    }
    readonly property string frameBlocker: capabilities.frameBlocker || ""
    readonly property var inplaceAvailability: capabilities.inplaceTranslation || ({
            available: true,
            reason: "",
            remedy: ""
        })
    readonly property string inplaceBlocker: inplaceAvailability.reason || ""
    readonly property var history: {
        try {
            return JSON.parse(controller.historyJson || "[]");
        } catch (e) {
            return [];
        }
    }
    readonly property var diagnostics: {
        try {
            return JSON.parse(controller.diagnosticsJson || "[]");
        } catch (e) {
            return [];
        }
    }
    // ── Regions: at most `maxRegions`; one active unless several are allowed ──
    readonly property int maxRegions: 3
    property string regionNotice: ""
    Timer {
        id: regionNoticeTimer
        interval: 4000
        onTriggered: win.regionNotice = ""
    }
    function notifyRegions(text) {
        regionNotice = text;
        regionNoticeTimer.restart();
    }
    function withRegions(regions, extra) {
        const c = JSON.parse(JSON.stringify(current));
        Object.assign(c.capture, extra || ({}));
        c.capture.regions = regions;
        current = c;
    }
    function activateRegion(index, on) {
        const r = (current.capture.regions || []).map(x => Object.assign({}, x));
        if (!r[index])
            return;
        if (on && current.capture.allow_multiple_regions !== true)
            r.forEach((x, i) => x.enabled = false);
        r[index].enabled = on;
        withRegions(r, on ? {
            active_region: r[index].id
        } : ({}));
    }
    function setAllowMultipleRegions(on) {
        const r = (current.capture.regions || []).map(x => Object.assign({}, x));
        if (!on) {
            let keep = r.findIndex(x => x.enabled && x.id === current.capture.active_region);
            if (keep < 0)
                keep = r.findIndex(x => x.enabled);
            r.forEach((x, i) => x.enabled = i === keep);
        }
        withRegions(r, {
            allow_multiple_regions: on
        });
    }
    function addRegion() {
        const r = (current.capture.regions || []).slice();
        if (r.length >= maxRegions) {
            notifyRegions("Можно создать не больше " + maxRegions + " областей захвата. Удалите одну, чтобы добавить новую.");
            return false;
        }
        let n = r.length + 1;
        while (r.some(x => x.id === "region-" + n))
            ++n;
        // A new region starts inactive: it becomes active once its area is selected.
        r.push({
            id: "region-" + n,
            name: "Область захвата " + n,
            enabled: false,
            rect: null,
            recognition_language: "",
            target_language: "",
            engine: null,
            interval_ms: 500,
            debounce_ms: 400
        });
        withRegions(r);
        return true;
    }
    function removeRegion(index) {
        withRegions((current.capture.regions || []).filter((_, i) => i !== index));
    }
    function setRegionField(index, key, v) {
        const r = (current.capture.regions || []).map(x => Object.assign({}, x));
        r[index][key] = v;
        set("capture.regions", r);
    }

    header: TabBar {
        id: tabs
        objectName: "settingsTabs"
        Repeater {
            model: ["Источник изображения", "Распознавание текста", "Перевод", "Отображение перевода", "Окно перевода", "Оформление перевода", "Клавиши", "Статус", "О программе"]
            TabButton {
                text: modelData
                width: implicitWidth
            }
        }
        onCurrentIndexChanged: if (currentIndex === 7 && win.diagnostics.length === 0)
            win.controller.refreshDiagnostics()
    }
    footer: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.margins: 10
            Label {
                text: autosave.running ? "Сохранение…" : "Изменения применяются сразу и сохраняются автоматически"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
                opacity: 0.7
            }
            Button {
                text: "Закрыть"
                onClicked: win.close()
            }
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
                SectionTitle {
                    text: "Источник захвата"
                }
                Button {
                    objectName: "selectCaptureWindow"
                    Layout.columnSpan: 2
                    text: "Выбрать окно для захвата"
                    onClicked: {
                        win.apply();
                        win.controller.pickWindow();
                    }
                }
                FieldLabel {
                    helpText: "KWin получает изображение и положение окна. Portal открывает системный выбор источника; положение окна на рабочем столе ему недоступно."
                    text: "Способ захвата"
                    helpControl: captureSourceHelpTarget1
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    ComboBox {
                        id: captureSourceHelpTarget1
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        model: ["Авто: KWin, а если недоступен — portal", "KWin ScreenShot2 (только KDE)", "xdg-desktop-portal (PipeWire)"]
                        currentIndex: Math.max(0, win.backends.indexOf(win.current.capture.source))
                        onActivated: win.set("capture.source", win.backends[currentIndex])
                    }
                    Label {
                        objectName: "captureBackendNote"
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        color: "#ffc23d"
                        text: "Способ захвата сменится при следующем выборе окна игры (кнопка «Выбрать окно для захвата»)."
                    }
                    Label {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        opacity: 0.7
                        visible: win.current.capture.source !== "kwin"
                        text: "Portal показывает системный диалог выбора окна и требует gstreamer с gst-plugin-pipewire. " + "Выбор запоминается, диалог при следующем запуске не нужен. Выбранное окно переключается заново кнопкой «Выбрать окно для захвата»."
                    }
                    Switch {
                        id: unavailableControl1
                        objectName: "portalFillsMonitor"
                        Layout.fillWidth: true
                        visible: win.current.capture.source !== "kwin"
                        enabled: !!win.current.translation_window.screen
                        checked: win.current.capture.portal_fills_monitor === true
                        text: "Выбранное через portal окно занимает весь указанный экран"
                        onToggled: win.set("capture.portal_fills_monitor", checked)

                        Accessible.description: unavailableHint1.accessibleExplanation
                        UnavailableHint {
                            id: unavailableHint1
                            control: unavailableControl1
                            feature: "Полноэкранный захват portal"
                            reason: "Экран не указан. Portal не сообщает положение окна."
                            remedy: "Выберите экран на вкладке «Окно перевода»."
                        }
                    }
                    Label {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        color: "#ffc23d"
                        visible: win.current.capture.source !== "kwin" && !win.current.translation_window.screen
                        text: "Для полноэкранного portal сначала выберите экран на вкладке «Окно перевода»."
                    }
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: "KWin исключает серверную рамку. Если приложение рисует собственный заголовок (CSD), выделите область захвата ниже него. Portal не сообщает положение окна на рабочем столе."
                }
                FieldLabel {
                    text: "Запоминать настройки каждой игры"
                }
                Switch {
                    objectName: "gameProfilesSwitch"
                    checked: win.current.game_profiles_enabled !== false
                    onToggled: win.set("game_profiles_enabled", checked)
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.75
                    text: "Области захвата, языки, движок распознавания и положение перевода сохраняются отдельно для каждого окна (по его классу) " + "и возвращаются, когда вы снова выбираете окно этой игры. Оформление перевода и клавиши общие."
                }
                Repeater {
                    objectName: "gameProfileList"
                    model: win.gameProfileKeys
                    delegate: RowLayout {
                        required property string modelData
                        readonly property var profile: (win.current.game_profiles || ({}))[modelData] || ({})
                        readonly property bool selected: modelData === win.currentGameKey
                        Layout.columnSpan: 2
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        Label {
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            elide: Text.ElideRight
                            text: modelData + (profile.caption ? " — " + profile.caption : "") + " · областей захвата: " + (profile.capture.regions || []).filter(r => r.rect).length + (selected ? " (выбрана)" : "")
                        }
                        Button {
                            id: unavailableControl2
                            objectName: "forgetGame_" + modelData
                            text: "Забыть профиль игры"
                            enabled: !selected
                            onClicked: win.controller.forgetGameProfile(modelData)

                            Accessible.description: unavailableHint2.accessibleExplanation
                            UnavailableHint {
                                id: unavailableHint2
                                control: unavailableControl2
                                feature: "Удалить сохранённый профиль игры"
                                reason: "Этот профиль используется выбранным окном и автоматически сохраняется."
                                remedy: "Сначала выберите другое окно."
                            }
                        }
                    }
                }
                SectionTitle {
                    text: "Рамка областей захвата в игре"
                }
                FieldLabel {
                    text: "Показывать рамку"
                }
                ComboBox {
                    objectName: "regionFrameMode"
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    model: win.frameModes.map(m => m.label)
                    currentIndex: Math.max(0, win.frameModes.findIndex(m => m.value === (win.current.capture.region_frame_mode || "selection")))
                    onActivated: win.set("capture.region_frame_mode", win.frameModes[currentIndex].value)
                }
                FieldLabel {
                    text: "Исчезает через, с"
                    visible: win.current.capture.region_frame_mode === "selection"
                }
                SpinBox {
                    visible: win.current.capture.region_frame_mode === "selection"
                    from: 1
                    to: 60
                    editable: true
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    value: win.current.capture.frame_seconds || 3
                    onValueModified: win.set("capture.frame_seconds", value)
                }
                FieldLabel {
                    helpText: "Цвет рамки, обозначающей участок изображения, который передаётся на распознавание."
                    text: "Цвет рамки области захвата"
                    visible: win.current.capture.region_frame_mode !== "pattern" && win.current.capture.region_frame_mode !== "off"
                    helpControl: translationSettingHelpTarget1
                }
                ColorRow {
                    id: translationSettingHelpTarget1
                    visible: win.current.capture.region_frame_mode !== "pattern" && win.current.capture.region_frame_mode !== "off"
                    key: "capture.frame_color"
                    presets: ["#ff0000", "#00c800", "#1e90ff", "#ffd400", "#ffffff"]
                }
                FieldLabel {
                    text: "Толщина рамки, px"
                    visible: win.current.capture.region_frame_mode !== "off"
                }
                SpinBox {
                    visible: win.current.capture.region_frame_mode !== "off"
                    from: 1
                    to: 12
                    editable: true
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    value: win.current.capture.frame_width || 2
                    onValueModified: win.set("capture.frame_width", value)
                }
                FieldLabel {
                    text: "Когда окно перевода закреплено"
                }
                ComboBox {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    model: ["Полупрозрачная обводка", "Скрыть обводку"]
                    currentIndex: win.current.capture.region_frame_pinned === "hide" ? 1 : 0
                    onActivated: win.set("capture.region_frame_pinned", currentIndex === 1 ? "hide" : "dim")
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.7
                    text: "Рамка рисуется вокруг каждой активной области захвата и не мешает кликам. У каждой области свой таймер; " + "режим можно переопределить для отдельной области в разделе областей захвата."
                }
                Label {
                    objectName: "frameBlockerNote"
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    color: "#ffc23d"
                    visible: win.frameBlocker.length > 0
                    text: "Для этого окна недоступно: " + win.frameBlocker + "."
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.75
                    text: "До " + win.maxRegions + " областей захвата. Каждая распознаётся отдельно; пустые поля берутся из общих настроек. " + "Выключенная область захвата сохраняет выделение."
                }
                FieldLabel {
                    text: "Разрешить несколько активных областей захвата"
                }
                Switch {
                    objectName: "allowMultipleRegions"
                    checked: win.current.capture.allow_multiple_regions === true
                    onToggled: win.setAllowMultipleRegions(checked)
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.7
                    text: win.current.capture.allow_multiple_regions === true ? "Активными могут быть до " + win.maxRegions + " областей захвата одновременно." : "Активна одна область захвата: включение другой выключает предыдущую."
                }
                Repeater {
                    model: win.current.capture.regions || []
                    delegate: Frame {
                        id: regionFrame
                        required property var modelData
                        required property int index
                        readonly property bool hasRect: !!modelData.rect
                        Layout.columnSpan: 2
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
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
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                text: regionFrame.modelData.name
                                onEditingFinished: if (text.trim().length && text !== regionFrame.modelData.name)
                                    win.setRegionField(regionFrame.index, "name", text.trim())
                            }
                            FieldLabel {
                                Layout.columnSpan: 2
                                text: regionFrame.hasRect ? "Область захвата задана" : "Область захвата не задана"
                                color: regionFrame.hasRect ? palette.text : "#ffb347"
                            }
                            Flow {
                                objectName: "captureRegionActions"
                                Layout.columnSpan: 2
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                spacing: 8
                                Button {
                                    id: unavailableControl3
                                    text: "Выбрать область захвата"
                                    enabled: win.controller.windowTitle !== undefined && win.controller.windowTitle.length > 0
                                    onClicked: {
                                        win.activateRegion(regionFrame.index, true);
                                        win.apply();
                                        win.selectRegionRequested();
                                    }

                                    Accessible.description: unavailableHint3.accessibleExplanation
                                    UnavailableHint {
                                        id: unavailableHint3
                                        control: unavailableControl3
                                        feature: "Выбрать область захвата"
                                        reason: "Окно захвата ещё не выбрано."
                                        remedy: "Нажмите «Выбрать окно для захвата»."
                                    }
                                }
                                Button {
                                    id: unavailableControl4
                                    text: "Сбросить область захвата"
                                    enabled: regionFrame.hasRect
                                    onClicked: win.setRegionField(regionFrame.index, "rect", null)

                                    Accessible.description: unavailableHint4.accessibleExplanation
                                    UnavailableHint {
                                        id: unavailableHint4
                                        control: unavailableControl4
                                        feature: "Сбросить область захвата"
                                        reason: "У этой области ещё нет выделения."
                                        remedy: "Сначала выделите область в окне игры."
                                    }
                                }
                                Item {
                                    Layout.fillWidth: true
                                }
                                Button {
                                    id: unavailableControl5
                                    text: "Удалить область захвата"
                                    enabled: (win.current.capture.regions || []).length > 1
                                    onClicked: win.removeRegion(regionFrame.index)

                                    Accessible.description: unavailableHint5.accessibleExplanation
                                    UnavailableHint {
                                        id: unavailableHint5
                                        control: unavailableControl5
                                        feature: "Удалить область захвата"
                                        reason: "Нужно оставить хотя бы одну область захвата."
                                        remedy: "Очистите выделение или добавьте другую область."
                                    }
                                }
                            }
                            FieldLabel {
                                helpText: "Выберите язык исходного текста. Для выбранного движка должна быть установлена соответствующая языковая модель."
                                text: "Язык распознавания"
                                helpControl: recognitionLanguageHelpTarget1
                            }
                            ComboBox {
                                id: recognitionLanguageHelpTarget1
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                model: ["Как в общих настройках"].concat(win.langs)
                                currentIndex: Math.max(0, win.langs.indexOf(regionFrame.modelData.recognition_language) + 1)
                                onActivated: win.setRegionField(regionFrame.index, "recognition_language", currentIndex === 0 ? "" : currentText)
                            }
                            FieldLabel {
                                helpText: "Язык, на котором будет показан результат перевода."
                                text: "Язык перевода"
                                helpControl: targetLanguageHelpTarget1
                            }
                            ComboBox {
                                id: targetLanguageHelpTarget1
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                model: ["Как в общих настройках"].concat(win.targets)
                                currentIndex: Math.max(0, win.targets.indexOf(regionFrame.modelData.target_language) + 1)
                                onActivated: win.setRegionField(regionFrame.index, "target_language", currentIndex === 0 ? "" : currentText)
                            }
                            FieldLabel {
                                helpText: "Tesseract и PaddleOCR распознают текст в области захвата. Автоматический режим повторяет распознавание через PaddleOCR при низкой уверенности Tesseract."
                                text: "Движок распознавания"
                                helpControl: recognitionEngineHelpTarget1
                            }
                            ComboBox {
                                id: recognitionEngineHelpTarget1
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                readonly property var values: ["", "tesseract", "paddleocr", "auto"]
                                model: ["Как в общих настройках", "Tesseract", "PaddleOCR", "Авто"]
                                currentIndex: Math.max(0, values.indexOf(regionFrame.modelData.engine || ""))
                                onActivated: win.setRegionField(regionFrame.index, "engine", values[currentIndex] || null)
                            }
                            FieldLabel {
                                text: "Рамка области захвата"
                            }
                            ComboBox {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                model: ["Как в общих настройках"].concat(win.frameModes.map(m => m.label))
                                currentIndex: Math.max(0, win.frameModes.findIndex(m => m.value === regionFrame.modelData.frame_mode) + 1)
                                onActivated: win.setRegionField(regionFrame.index, "frame_mode", currentIndex === 0 ? "" : win.frameModes[currentIndex - 1].value)
                            }
                            FieldLabel {
                                text: "Интервал / задержка перед распознаванием, мс"
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                SpinBox {
                                    Layout.fillWidth: true
                                    Layout.minimumWidth: 0
                                    from: 100
                                    to: 10000
                                    stepSize: 50
                                    editable: true
                                    value: regionFrame.modelData.interval_ms || 500
                                    onValueModified: win.setRegionField(regionFrame.index, "interval_ms", value)
                                }
                                SpinBox {
                                    Layout.fillWidth: true
                                    Layout.minimumWidth: 0
                                    from: 0
                                    to: 5000
                                    stepSize: 50
                                    editable: true
                                    value: regionFrame.modelData.debounce_ms || 0
                                    onValueModified: win.setRegionField(regionFrame.index, "debounce_ms", value)
                                }
                            }
                        }
                    }
                }
                RowLayout {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    // Stays enabled at the limit: pressing it explains why nothing was added.
                    Button {
                        objectName: "addRegion"
                        text: "Добавить область захвата"
                        onClicked: win.addRegion()
                    }
                    Label {
                        text: (win.current.capture.regions || []).length + " из " + win.maxRegions
                        opacity: 0.7
                    }
                }
                Label {
                    objectName: "regionNotice"
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    visible: win.regionNotice.length > 0
                    text: win.regionNotice
                    color: "#ffc23d"
                }
                ResetButton {
                    keys: ["capture.regions", "capture.active_region", "capture.allow_multiple_regions"]
                    text: "Сбросить области захвата (выделения будут очищены)"
                }
                ResetButton {
                    text: "Сбросить настройки захвата"
                    keys: win.settingPaths.filter(k => k.startsWith("capture.") && !["capture.regions", "capture.active_region", "capture.allow_multiple_regions"].includes(k))
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
                FieldLabel {
                    helpText: "Tesseract и PaddleOCR распознают текст в области захвата. Автоматический режим повторяет распознавание через PaddleOCR при низкой уверенности Tesseract."
                    text: "Движок распознавания"
                    helpControl: recognitionEngineHelpTarget2
                }
                ComboBox {
                    id: recognitionEngineHelpTarget2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    objectName: "ocrEngineBox"
                    readonly property var values: ["tesseract", "paddleocr", "auto"]
                    model: ["Tesseract", "PaddleOCR 3.x", "Авто: Tesseract, при сомнении PaddleOCR"]
                    currentIndex: Math.max(0, values.indexOf(win.current.recognition.engine))
                    onActivated: win.set("recognition.engine", values[currentIndex])
                }
                FieldLabel {
                    helpText: "Путь к интерпретатору Python, в окружении которого установлены PaddleOCR и PaddlePaddle."
                    text: "Python для PaddleOCR"
                    visible: win.usesPaddle
                    helpControl: paddlePythonHelpTarget1
                }
                TextField {
                    id: paddlePythonHelpTarget1
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    visible: win.usesPaddle
                    text: win.current.recognition.paddle_python || "python3"
                    placeholderText: "/путь/к/venv/bin/python"
                    onTextEdited: win.set("recognition.paddle_python", text)
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    visible: win.usesPaddle
                    text: win.current.recognition.engine === "auto" ? "Сначала читает Tesseract; если он сам не уверен в результате (ниже 60 %), текст перечитывает PaddleOCR. Если PaddleOCR не установлен, остаётся результат Tesseract, и повторная попытка будет через несколько минут. Установка: docs/PaddleOCR.md." : "PaddleOCR использует основной язык; дополнительные языки относятся к Tesseract. При первом запуске загружаются модели. Установка: docs/PaddleOCR.md."
                }
                FieldLabel {
                    helpText: "Результаты ниже этого порога не переводятся. Значение 0 отключает проверку уверенности."
                    text: "Минимальная уверенность распознавания, %"
                    helpControl: recognitionConfidenceHelpTarget1
                }
                SpinBox {
                    id: recognitionConfidenceHelpTarget1
                    objectName: "ocrMinConfidence"
                    from: 0
                    to: 95
                    stepSize: 5
                    editable: true
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    value: win.current.recognition.minimum_confidence !== undefined ? win.current.recognition.minimum_confidence : 30
                    onValueModified: win.set("recognition.minimum_confidence", value)
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.75
                    text: "Текст, в котором сам движок не уверен (мусор из-за фона, анимации, мелкого шрифта), не переводится и не показывается. " + "0 — не проверять. Работает с Tesseract и PaddleOCR; причина отброшенного текста видна в «Просмотре OCR»."
                }
                Label {
                    text: "Tesseract: состояние и языковые пакеты"
                    visible: win.current.recognition.engine !== "paddleocr"
                    font.bold: true
                    Layout.columnSpan: 2
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    visible: win.current.recognition.engine !== "paddleocr" && (!win.tess.distro || win.tess.installed)
                    text: !win.tess.distro ? "Проверка…" : "Установлен · " + (win.tess.version ? "версия " + win.tess.version : "версия неизвестна") + "\nПуть: " + win.tess.path + "\ntessdata: " + (win.tess.tessdata || "не определён") + "\nСистема: " + win.tess.distro.name + " · менеджер пакетов: " + win.tess.package_manager + (win.tess.engine_package ? "\nПакет: " + win.tess.engine_package : "") + (win.tess.tessdata_package ? " · данные: " + win.tess.tessdata_package + " и др." : "")
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    visible: win.current.recognition.engine !== "paddleocr" && !!win.tess.distro && !win.tess.installed
                    color: "#ff6b6b"
                    text: "Tesseract не установлен" + (win.tess.engine_install_command ? ". Команда установки: " + win.tess.engine_install_command : "")
                }
                FieldLabel {
                    helpText: "Язык исходного текста. Для выбранного движка должна быть установлена соответствующая языковая модель."
                    text: "Язык распознавания"
                    helpControl: primaryBox
                }
                ComboBox {
                    id: primaryBox
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    model: win.langModel
                    textRole: "label"
                    currentIndex: Math.max(0, win.langModel.findIndex(l => l.code === win.primaryLang))
                    onActivated: win.setLangs(win.langModel[currentIndex].code, win.extraLangs)
                }
                Label {
                    Layout.preferredWidth: 230
                    Layout.maximumWidth: 230
                    Layout.minimumWidth: 0
                    text: "Дополнительные языки (по желанию)"
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
                Repeater {
                    model: win.langModel.filter(l => l.code !== win.primaryLang && l.installed)
                    CheckBox {
                        text: modelData.label
                        checked: win.extraLangs.indexOf(modelData.code) >= 0
                        onToggled: {
                            const set = win.extraLangs.filter(c => c !== modelData.code);
                            if (checked)
                                set.push(modelData.code);
                            win.setLangs(win.primaryLang, set);
                        }
                    }
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    text: "Будет использовано: " + (win.current.recognition.language || "eng")
                    opacity: 0.7
                }
                Repeater {
                    model: win.missing
                    delegate: RowLayout {
                        Layout.columnSpan: 2
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        Label {
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            wrapMode: Text.Wrap
                            color: "#ff6b6b"
                            text: modelData.message
                        }
                        Button {
                            id: unavailableControl7
                            visible: modelData.package !== null && win.tess.package_manager !== "unknown"
                            text: "Установить языковой пакет"
                            enabled: !win.controller.tesseractBusy
                            onClicked: win.askInstall(modelData.package, modelData.name)

                            Accessible.description: unavailableHint7.accessibleExplanation
                            UnavailableHint {
                                id: unavailableHint7
                                control: unavailableControl7
                                feature: "Проверка и установка Tesseract"
                                reason: "Операция с Tesseract ещё выполняется."
                                remedy: "Дождитесь завершения текущей операции."
                            }
                        }
                    }
                }
                FieldLabel {
                    text: "Установить другой язык"
                    visible: win.current.recognition.engine !== "paddleocr"
                }
                RowLayout {
                    visible: win.current.recognition.engine !== "paddleocr"
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    ComboBox {
                        id: installBox
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        model: win.installable
                        textRole: "label"
                        enabled: win.installable.length > 0

                        Accessible.description: unavailableHint8.accessibleExplanation
                        UnavailableHint {
                            id: unavailableHint8
                            control: installBox
                            feature: "Выбор языкового пакета"
                            reason: "Для этой системы нет доступных пакетов в списке установки."
                            remedy: "Установите языковой пакет средствами системы и обновите список."
                        }
                    }
                    Button {
                        id: unavailableControl9
                        text: "Установить"
                        enabled: installBox.currentIndex >= 0 && win.installable.length > 0 && !win.controller.tesseractBusy
                        onClicked: win.askInstall(win.installable[installBox.currentIndex].package, win.installable[installBox.currentIndex].name)

                        Accessible.description: unavailableHint9.accessibleExplanation
                        UnavailableHint {
                            id: unavailableHint9
                            control: unavailableControl9
                            feature: "Установка языкового пакета"
                            reason: (win.controller.tesseractBusy ? "Установка уже выполняется." : "Доступный языковой пакет не выбран.")
                            remedy: "Выберите доступный пакет или дождитесь завершения установки."
                        }
                    }
                }
                Button {
                    id: unavailableControl6
                    Layout.columnSpan: 2
                    visible: win.current.recognition.engine !== "paddleocr"
                    text: win.controller.tesseractBusy ? "Проверка…" : "Обновить список языков"
                    enabled: !win.controller.tesseractBusy
                    onClicked: win.controller.refreshTesseract()

                    Accessible.description: unavailableHint6.accessibleExplanation
                    UnavailableHint {
                        id: unavailableHint6
                        control: unavailableControl6
                        feature: "Проверка и установка Tesseract"
                        reason: "Операция с Tesseract ещё выполняется."
                        remedy: "Дождитесь завершения текущей операции."
                    }
                }
                FieldLabel {
                    helpText: "Пауза между проверками изображения на изменение текста."
                    text: "Интервал распознавания, мс"
                    helpControl: recognitionIntervalHelpTarget1
                }
                SpinBox {
                    id: recognitionIntervalHelpTarget1
                    from: 100
                    to: 5000
                    stepSize: 50
                    editable: true
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    value: win.current.recognition.interval_ms || 500
                    onValueModified: win.set("recognition.interval_ms", value)
                }
                Label {
                    Layout.preferredWidth: 230
                    Layout.maximumWidth: 230
                    Layout.minimumWidth: 0
                    text: "Чувствительность обнаружения текста (ниже — чувствительнее)"
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
                Slider {
                    from: 0.2
                    to: 20
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    value: win.current.recognition.sensitivity || 2
                    onMoved: win.set("recognition.sensitivity", value)
                }
                FieldLabel {
                    helpText: "После изменения изображения приложение ждёт стабилизации текста перед распознаванием."
                    text: "Задержка перед распознаванием, мс"
                    helpControl: recognitionDelayHelpTarget1
                }
                SpinBox {
                    id: recognitionDelayHelpTarget1
                    from: 0
                    to: 3000
                    stepSize: 50
                    editable: true
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    value: win.current.recognition.debounce_ms || 0
                    onValueModified: win.set("recognition.debounce_ms", value)
                }
                ResetButton {
                    text: "Сбросить настройки распознавания"
                    keys: ["recognition"]
                }
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
                FieldLabel {
                    helpText: "По умолчанию берётся из языка распознавания активной области захвата. Можно выбрать отдельный язык для сервиса перевода."
                    text: "Исходный язык"
                    helpControl: sourceLanguageHelpTarget1
                }
                ComboBox {
                    id: sourceLanguageHelpTarget1
                    objectName: "translationSourceLanguage"
                    Layout.fillWidth: true
                    model: ["Из языка распознавания"].concat(win.targets)
                    currentIndex: win.current.translation.source_language && win.current.translation.source_language.mode === "explicit" ? Math.max(0, win.targets.indexOf(win.current.translation.source_language.language) + 1) : 0
                    onActivated: win.set("translation.source_language", currentIndex === 0 ? {
                        mode: "recognition_language"
                    } : {
                        mode: "explicit",
                        language: win.targets[currentIndex - 1]
                    })
                }
                FieldLabel {
                    helpText: "Язык, на котором будет показан результат перевода."
                    text: "Язык перевода"
                    helpControl: targetLanguageHelpTarget2
                }
                ComboBox {
                    id: targetLanguageHelpTarget2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    model: win.targets
                    currentIndex: Math.max(0, win.targets.indexOf(win.current.translation.target_language))
                    onActivated: win.set("translation.target_language", currentText)
                }
                FieldLabel {
                    helpText: "Сервис, которому отправляется распознанный текст. Для некоторых сервисов требуется ключ API."
                    text: "Сервис перевода"
                    helpControl: tr
                }
                ComboBox {
                    id: tr
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    model: ["Google Translate", "Yandex Translate", "Свой API"]
                    currentIndex: Math.max(0, win.translators.indexOf(win.current.translation.service))
                    onActivated: win.set("translation.service", win.translators[currentIndex])
                }
                FieldLabel {
                    text: "Yandex API-ключ"
                    visible: tr.currentIndex === 1
                }
                TextField {
                    visible: tr.currentIndex === 1
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    echoMode: TextInput.Password
                    text: win.current.translation.yandex_api_key || ""
                    onEditingFinished: win.set("translation.yandex_api_key", text)
                }
                FieldLabel {
                    text: "Yandex folder ID"
                    visible: tr.currentIndex === 1
                }
                TextField {
                    visible: tr.currentIndex === 1
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    text: win.current.translation.yandex_folder_id || ""
                    onEditingFinished: win.set("translation.yandex_folder_id", text)
                }
                FieldLabel {
                    text: "URL API (LibreTranslate-формат)"
                    visible: tr.currentIndex === 2
                }
                TextField {
                    visible: tr.currentIndex === 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    placeholderText: "https://host/translate"
                    text: win.current.translation.custom_url || ""
                    onEditingFinished: win.set("translation.custom_url", text)
                }
                FieldLabel {
                    text: "Ключ API (необязательно)"
                    visible: tr.currentIndex === 2
                }
                TextField {
                    visible: tr.currentIndex === 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    echoMode: TextInput.Password
                    text: win.current.translation.custom_api_key || ""
                    onEditingFinished: win.set("translation.custom_api_key", text)
                }
                FieldLabel {
                    text: "Автоматический перевод"
                }
                Switch {
                    checked: win.current.translation.auto_translate !== false
                    onToggled: win.set("translation.auto_translate", checked)
                }
                ResetButton {
                    text: "Сбросить настройки перевода"
                    keys: ["translation"]
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
                SectionTitle {
                    text: "Способ отображения"
                }
                FieldLabel {
                    helpText: "Перевод поверх исходного текста требует точных координат окна. Отдельное окно перевода можно разместить независимо от текста игры."
                    text: "Где показывать перевод"
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    RadioButton {
                        id: windowChoice
                        objectName: "displayWindow"
                        HoverHint {
                            control: windowChoice
                            feature: "Отдельное окно перевода"
                            objectName: "windowDisplayHint"
                            explanation: feature + "\n\n" + win.windowDisplayDescription
                        }
                        text: "В отдельном окне"
                        Accessible.description: win.windowDisplayDescription
                        checked: win.current.display_mode !== "inplace"
                        onClicked: win.set("display_mode", "window")
                    }
                    Label {
                        objectName: "windowDisplayDescription"
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        text: win.windowDisplayDescription
                    }
                    RadioButton {
                        id: inplaceChoice
                        objectName: "displayInplace"
                        text: "Поверх исходного текста"
                        checked: win.current.display_mode === "inplace"
                        enabled: win.inplaceAvailability.available
                        HoverHint {
                            control: inplaceChoice
                            feature: "Поверх исходного текста"
                            objectName: "inplaceDisplayHint"
                            explanation: feature + "\n\n" + win.inplaceDisplayDescription
                            active: inplaceChoice.enabled
                        }
                        Accessible.description: enabled ? win.inplaceDisplayDescription : inplaceHint.explanation
                        onClicked: win.set("display_mode", "inplace")
                        UnavailableHint {
                            id: inplaceHint
                            objectName: "inplaceUnavailableHint"
                            control: inplaceChoice
                            feature: "In-place Translation — перевод поверх исходного текста"
                            reason: win.inplaceAvailability.reason
                            remedy: win.inplaceAvailability.remedy
                        }
                    }
                    Label {
                        objectName: "inplaceDisplayDescription"
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        text: win.inplaceDisplayDescription
                    }
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.7
                    visible: win.current.display_mode === "inplace"
                    text: "Перевод закрывает исходный текст в каждой активной области: размытая заливка цвета фона, " + "цвет, размер и начертание оцениваются по кадру; длинный перевод уменьшается. " + "Свой шрифт можно выбрать на вкладке «Оформление перевода». Средняя кнопка мыши скрывает перевод; вернуть его можно переключателем «Поверх исходного текста»."
                }
                Label {
                    objectName: "inplaceBlockerNote"
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    color: "#ffc23d"
                    visible: win.current.display_mode === "inplace" && win.inplaceBlocker.length > 0
                    text: "Для этого окна недоступно: " + win.inplaceBlocker + "."
                }
                ResetButton {
                    text: "Сбросить способ отображения"
                    keys: ["display_mode"]
                }
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
                rowSpacing: 14
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: "Прозрачность: 100 % — непрозрачно, 0 % — полностью прозрачно."
                }
                FieldLabel {
                    helpText: "Экран для размещения окна перевода. Для portal его нужно указать вручную."
                    text: "Экран перевода"
                    helpControl: translationScreenHelpTarget1
                }
                ComboBox {
                    id: translationScreenHelpTarget1
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    readonly property var screens: Qt.application.screens
                    model: ["Автоматически: экран игры"].concat(screens.map(s => s.name))
                    currentIndex: Math.max(0, screens.findIndex(s => s.name === win.current.translation_window.screen) + 1)
                    onActivated: win.set("translation_window.screen", currentIndex === 0 ? "" : screens[currentIndex - 1].name)
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.75
                    text: "В KWin перевод следует за окном игры. Для portal выберите экран вручную: положение окна скрыто порталом. Координаты ниже считаются от угла этого экрана."
                }
                FieldLabel {
                    helpText: "Закреплённое окно сохраняет положение и может пропускать клики в игру. Свободное окно можно перетаскивать мышью."
                    text: "Закрепить окно"
                    helpControl: windowPinnedHelpTarget1
                }
                Switch {
                    id: windowPinnedHelpTarget1
                    checked: win.current.translation_window.mode === "pinned"
                    // Through the Controller: pinning places the pinned window where the floating one was.
                    onToggled: if (win.controller.setTranslationWindowPinned)
                        win.controller.setTranslationWindowPinned(checked, "settings")
                    else
                        win.set("translation_window.mode", checked ? "pinned" : "floating")
                }
                FieldLabel {
                    helpText: "Радиус углов фона закреплённого окна перевода."
                    text: "Скругление углов закреплённого окна, px"
                    helpControl: pinnedCornerRadiusHelpTarget1
                }
                SpinBox {
                    id: pinnedCornerRadiusHelpTarget1
                    objectName: "pinnedCornerRadius"
                    from: 0
                    to: 32
                    editable: true
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    value: win.current.translation_window.pinned_corner_radius !== undefined ? win.current.translation_window.pinned_corner_radius : 0
                    onValueModified: win.set("translation_window.pinned_corner_radius", value)
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.75
                    text: "Средняя кнопка мыши по рамке перевода переключает закрепление. Свободное окно перевода перетаскивается левой кнопкой, его рамка толще. " + "Закреплённый пропускает клики в игру (если включено ниже); рамка и значок в углу остаются доступны для средней кнопки."
                }
                FieldLabel {
                    helpText: "Клики по содержимому закреплённого окна перевода передаются в игру."
                    text: "Пропускать клики мыши"
                    helpControl: windowClickThroughHelpTarget1
                }
                Switch {
                    id: windowClickThroughHelpTarget1
                    checked: win.current.translation_window.click_through !== false
                    onToggled: win.set("translation_window.click_through", checked)
                }
                FieldLabel {
                    helpText: "Координаты окна перевода относительно выбранного экрана."
                    text: "Положение окна перевода (x, y)"
                    helpControl: windowPositionHelpTarget1
                }
                RowLayout {
                    id: windowPositionHelpTarget1
                    SpinBox {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        from: 0
                        to: win.screenMaxWidth
                        editable: true
                        value: win.current.translation_window.position ? win.current.translation_window.position[0] : 0
                        onValueModified: win.set("translation_window.position", [value, win.current.translation_window.position[1]])
                    }
                    SpinBox {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        from: 0
                        to: win.screenMaxHeight
                        editable: true
                        value: win.current.translation_window.position ? win.current.translation_window.position[1] : 0
                        onValueModified: win.set("translation_window.position", [win.current.translation_window.position[0], value])
                    }
                }
                FieldLabel {
                    helpText: "Ширина и высота отдельного окна перевода."
                    text: "Размер окна перевода (ш, в)"
                    helpControl: windowSizeHelpTarget1
                }
                RowLayout {
                    id: windowSizeHelpTarget1
                    SpinBox {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        from: 200
                        to: win.screenMaxWidth
                        editable: true
                        value: win.current.translation_window.size ? win.current.translation_window.size[0] : 700
                        onValueModified: win.set("translation_window.size", [value, win.current.translation_window.size[1]])
                    }
                    SpinBox {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        from: 60
                        to: win.screenMaxHeight
                        editable: true
                        value: win.current.translation_window.size ? win.current.translation_window.size[1] : 120
                        onValueModified: win.set("translation_window.size", [win.current.translation_window.size[0], value])
                    }
                }
                FieldLabel {
                    text: "Сворачивать приложение в системный трей"
                }
                Switch {
                    objectName: "closeToTraySwitch"
                    checked: win.current.close_to_tray === true
                    onToggled: win.set("close_to_tray", checked)
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.75
                    text: "Включено: закрытие главного окна скрывает его в трей, захват и перевод продолжают работать. " + "Выключено: закрытие главного окна завершает LipaX. Полностью выйти можно из меню значка в трее."
                }
                FieldLabel {
                    text: "Автоматический размер текста (уменьшать, если не помещается)"
                }
                Switch {
                    objectName: "translationWindowAutoShrink"
                    checked: win.current.translation_window.auto_shrink !== false
                    onToggled: win.set("translation_window.auto_shrink", checked)
                }
                FieldLabel {
                    helpText: "100 % — непрозрачно, 0 % — полностью прозрачно. Изменяет только фон."
                    text: "Прозрачность фона"
                    visible: win.solidStyle
                    helpControl: backgroundOpacityHelpTarget1
                }
                RowLayout {
                    id: backgroundOpacityHelpTarget1
                    visible: win.solidStyle
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    Slider {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        from: 0
                        to: 1
                        value: win.current.translation_window.opacity !== undefined ? win.current.translation_window.opacity : 0.85
                        onMoved: win.set("translation_window.opacity", Math.round(value * 100) / 100)
                    }
                    Label {
                        text: Math.round((win.current.translation_window.opacity !== undefined ? win.current.translation_window.opacity : 0.85) * 100) + "%"
                    }
                }
                FieldLabel {
                    text: "Цвет рамки"
                }
                ColorRow {
                    key: "translation_window.border_color"
                    presets: ["#ff00ff", "#ff2a6d", "#00e5ff", "#ffd400"]
                }
                FieldLabel {
                    text: "Показывать рамку"
                }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    ComboBox {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        model: ["Всегда", "После выделения области"]
                        currentIndex: win.current.translation_window.border_always === false ? 1 : 0
                        onActivated: win.set("translation_window.border_always", currentIndex === 0)
                    }
                    SpinBox {
                        visible: win.current.translation_window.border_always === false
                        from: 1
                        to: 120
                        editable: true
                        value: win.current.translation_window.border_seconds || 5
                        onValueModified: win.set("translation_window.border_seconds", value)
                    }
                    Label {
                        visible: win.current.translation_window.border_always === false
                        text: "с"
                    }
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.7
                    visible: win.current.translation_window.border_always === false
                    text: "Рамка появляется на заданное время после выделения или изменения области. Свободное окно перевода показывает рамку всегда."
                }
                FieldLabel {
                    text: "Узор «текстура ошибки» (цвет/чёрный)"
                }
                Switch {
                    checked: win.current.translation_window.border_pattern === true
                    onToggled: win.set("translation_window.border_pattern", checked)
                }
                FieldLabel {
                    helpText: "100 % — непрозрачно, 0 % — полностью прозрачно. Изменяет только рамку окна перевода."
                    text: "Прозрачность рамки"
                    helpControl: borderOpacityHelpTarget1
                }
                RowLayout {
                    id: borderOpacityHelpTarget1
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    Slider {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        from: 0
                        to: 1
                        value: win.current.translation_window.border_opacity !== undefined ? win.current.translation_window.border_opacity : 0.65
                        onMoved: win.set("translation_window.border_opacity", Math.round(value * 100) / 100)
                    }
                    Label {
                        text: Math.round((win.current.translation_window.border_opacity !== undefined ? win.current.translation_window.border_opacity : 0.65) * 100) + "%"
                    }
                }
                FieldLabel {
                    text: "Толщина рамки закреплённого окна, px"
                }
                SpinBox {
                    from: 1
                    to: 16
                    editable: true
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    value: win.current.translation_window.border_width || 2
                    onValueModified: win.set("translation_window.border_width", value)
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.7
                    text: "Свободное окно перевода показывается с рамкой на 2 px толще: так видно, что его можно перетаскивать."
                }
                FieldLabel {
                    helpText: "Радиус углов фона свободного окна перевода."
                    text: "Скругление углов свободного окна, px"
                    helpControl: floatingCornerRadiusHelpTarget1
                }
                SpinBox {
                    id: floatingCornerRadiusHelpTarget1
                    from: 0
                    to: 32
                    editable: true
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    value: win.current.translation_window.corner_radius !== undefined ? win.current.translation_window.corner_radius : 12
                    onValueModified: win.set("translation_window.corner_radius", value)
                }
                CheckBox {
                    Layout.preferredWidth: 230
                    Layout.maximumWidth: 230
                    Layout.minimumWidth: 0
                    text: "Максимальная ширина, px"
                    checked: win.current.translation_window.max_width_enabled !== false
                    onToggled: win.set("translation_window.max_width_enabled", checked)
                }
                SpinBox {
                    id: unavailableControl10
                    enabled: win.current.translation_window.max_width_enabled !== false
                    from: 200
                    to: win.screenMaxWidth
                    stepSize: 50
                    editable: true
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    value: win.current.translation_window.maximum_width || 900
                    onValueModified: win.set("translation_window.maximum_width", value)

                    Accessible.description: unavailableHint10.accessibleExplanation
                    UnavailableHint {
                        id: unavailableHint10
                        control: unavailableControl10
                        feature: "Максимальная ширина окна перевода"
                        reason: "Ограничение ширины выключено."
                        remedy: "Включите «Максимальная ширина»."
                    }
                }
                ResetButton {
                    text: "Сбросить настройки окна перевода"
                    keys: win.settingPaths.filter(k => k.startsWith("translation_window."))
                }
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
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: "Прозрачность: 100 % — непрозрачно, 0 % — полностью прозрачно."
                }
                SectionTitle {
                    objectName: "appearanceModeTitle"
                    text: win.current.display_mode === "inplace" ? "Поверх исходного текста" : "Отдельное окно перевода"
                }
                GridLayout {
                    objectName: "inplaceSettings"
                    visible: win.current.display_mode === "inplace"
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    columns: 2
                    columnSpacing: 24
                    rowSpacing: 10
                    Label {
                        Layout.columnSpan: 2
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        opacity: 0.7
                        text: "Каждое поле текста находится и отслеживается отдельно. Шрифт для поля выбирается один раз — " + "по признакам начертания оригинала и среди шрифтов приложения с глифами языка перевода — и дальше не меняется. " + "Каждое свойство ниже можно оставить автоматическим или задать вручную независимо от остальных."
                    }
                    FieldLabel {
                        text: "Фон под переводом"
                    }
                    ComboBox {
                        objectName: "inplaceBackground"
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        model: win.inplaceBackgrounds.map(m => m.label)
                        currentIndex: Math.max(0, win.inplaceBackgrounds.findIndex(m => m.value === (win.inplace().background_mode || "auto")))
                        onActivated: win.setInplace("background_mode", win.inplaceBackgrounds[currentIndex].value)
                    }
                    FieldLabel {
                        helpText: "Цвет обводки букв перевода."
                        text: "Цвет контура текста"
                        visible: win.inplace().background_mode === "transparent_outline"
                        helpControl: outlineColorRow
                    }
                    PropRow {
                        id: outlineColorRow
                        visible: win.inplace().background_mode === "transparent_outline"
                        key: "outline_color"
                        defaultValue: "#000000"
                        TextField {
                            visible: outlineColorRow.manual
                            Layout.fillWidth: true
                            text: win.propValue("outline_color", "#000000")
                            onEditingFinished: win.setProp("outline_color", true, text)
                        }
                    }
                    FieldLabel {
                        helpText: "Толщина обводки букв перевода в пикселях."
                        text: "Толщина контура текста, px"
                        visible: win.inplace().background_mode === "transparent_outline"
                        helpControl: translationSettingHelpTarget5
                    }
                    SpinBox {
                        id: translationSettingHelpTarget5
                        visible: win.inplace().background_mode === "transparent_outline"
                        from: 0
                        to: 8
                        value: win.inplace().outline_width === undefined ? 1 : win.inplace().outline_width
                        onValueModified: win.setInplace("outline_width", value)
                    }
                    FieldLabel {
                        helpText: "Тень букв перевода помогает отделить текст от фона."
                        text: "Тень текста"
                        visible: win.inplace().background_mode === "transparent_outline"
                    }
                    CheckBox {
                        visible: win.inplace().background_mode === "transparent_outline"
                        checked: win.inplace().shadow === true
                        onToggled: win.setInplace("shadow", checked)
                    }
                    FieldLabel {
                        helpText: "100 % — непрозрачно, 0 % — полностью прозрачно. Изменяет только текст перевода."
                        text: "Прозрачность текста, %"
                        visible: win.inplace().background_mode === "transparent_outline"
                        helpControl: translationSettingHelpTarget6
                    }
                    SpinBox {
                        id: translationSettingHelpTarget6
                        visible: win.inplace().background_mode === "transparent_outline"
                        from: 0
                        to: 100
                        value: Math.round((win.inplace().text_opacity === undefined ? 1 : win.inplace().text_opacity) * 100)
                        onValueModified: win.setInplace("text_opacity", value / 100)
                    }
                    FieldLabel {
                        text: "Цвет заливки"
                        visible: win.inplace().background_mode === "padded_fill"
                    }
                    PropRow {
                        id: fillColorRow
                        visible: win.inplace().background_mode === "padded_fill"
                        key: "fill_color"
                        defaultValue: "#202020"
                        TextField {
                            visible: fillColorRow.manual
                            Layout.fillWidth: true
                            text: win.propValue("fill_color", "#202020")
                            onEditingFinished: win.setProp("fill_color", true, text)
                        }
                    }
                    FieldLabel {
                        helpText: "100 % — непрозрачно, 0 % — полностью прозрачно. Изменяет только фон под переводом."
                        text: "Прозрачность фона, %"
                        visible: win.inplace().background_mode === "padded_fill"
                        helpControl: translationSettingHelpTarget7
                    }
                    SpinBox {
                        id: translationSettingHelpTarget7
                        visible: win.inplace().background_mode === "padded_fill"
                        from: 0
                        to: 100
                        value: Math.round((win.inplace().fill_opacity === undefined ? 1 : win.inplace().fill_opacity) * 100)
                        onValueModified: win.setInplace("fill_opacity", value / 100)
                    }
                    FieldLabel {
                        text: "Внутренние отступы по горизонтали, px"
                        visible: win.inplace().background_mode === "padded_fill"
                    }
                    SpinBox {
                        visible: win.inplace().background_mode === "padded_fill"
                        from: 0
                        to: 64
                        value: win.inplace().padding_x || 0
                        onValueModified: win.setInplace("padding_x", value)
                    }
                    FieldLabel {
                        text: "Внутренние отступы по вертикали, px"
                        visible: win.inplace().background_mode === "padded_fill"
                    }
                    SpinBox {
                        visible: win.inplace().background_mode === "padded_fill"
                        from: 0
                        to: 64
                        value: win.inplace().padding_y || 0
                        onValueModified: win.setInplace("padding_y", value)
                    }
                    FieldLabel {
                        helpText: "Отступы снаружи блока перевода."
                        text: "Внешние отступы, px"
                        visible: win.inplace().background_mode === "padded_fill"
                        helpControl: translationSettingHelpTarget8
                    }
                    SpinBox {
                        id: translationSettingHelpTarget8
                        visible: win.inplace().background_mode === "padded_fill"
                        from: 0
                        to: 64
                        value: win.inplace().extra_margin || 0
                        onValueModified: win.setInplace("extra_margin", value)
                    }
                    FieldLabel {
                        text: "Скругление углов, px"
                        visible: win.inplace().background_mode === "padded_fill"
                    }
                    SpinBox {
                        visible: win.inplace().background_mode === "padded_fill"
                        from: 0
                        to: 64
                        value: win.inplace().corner_radius === undefined ? 4 : win.inplace().corner_radius
                        onValueModified: win.setInplace("corner_radius", value)
                    }
                    FieldLabel {
                        helpText: "Шрифт результата перевода. Список содержит шрифты из комплекта приложения."
                        text: "Шрифт перевода"
                        helpControl: fontRow
                    }
                    PropRow {
                        id: fontRow
                        key: "font_family"
                        defaultValue: "Inter"
                        ComboBox {
                            visible: fontRow.manual
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            model: win.bundledFonts
                            currentIndex: Math.max(0, model.indexOf(win.propValue("font_family", "")))
                            onActivated: win.setProp("font_family", true, currentText)
                        }
                    }
                    FieldLabel {
                        helpText: "Размер текста перевода в пикселях. Автоматический режим подбирает его по исходному тексту."
                        text: "Размер текста, px"
                        helpControl: sizeRow
                    }
                    PropRow {
                        id: sizeRow
                        key: "font_size"
                        defaultValue: 20
                        SpinBox {
                            visible: sizeRow.manual
                            from: 4
                            to: 400
                            editable: true
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            value: win.propValue("font_size", 20)
                            onValueModified: win.setProp("font_size", true, value)
                        }
                    }
                    FieldLabel {
                        text: "Насыщенность"
                    }
                    PropRow {
                        id: weightRow
                        key: "font_weight"
                        defaultValue: "normal"
                        ComboBox {
                            visible: weightRow.manual
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            model: win.weightNames.map(w => w.label)
                            currentIndex: Math.max(0, win.weightNames.findIndex(w => w.value === win.propValue("font_weight", "normal")))
                            onActivated: win.setProp("font_weight", true, win.weightNames[currentIndex].value)
                        }
                    }
                    FieldLabel {
                        text: "Курсив"
                    }
                    PropRow {
                        id: italicRow
                        key: "italic"
                        defaultValue: false
                        CheckBox {
                            visible: italicRow.manual
                            text: "Курсив"
                            checked: win.propValue("italic", false) === true
                            onToggled: win.setProp("italic", true, checked)
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                    }
                    FieldLabel {
                        helpText: "Расстояние между строками относительно размера текста."
                        text: "Межстрочный интервал, %"
                        helpControl: lineRow
                    }
                    PropRow {
                        id: lineRow
                        key: "line_height"
                        defaultValue: 1.0
                        SpinBox {
                            visible: lineRow.manual
                            from: 70
                            to: 300
                            stepSize: 5
                            editable: true
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            value: Math.round(win.propValue("line_height", 1.0) * 100)
                            onValueModified: win.setProp("line_height", true, value / 100)
                        }
                    }
                    FieldLabel {
                        helpText: "Дополнительное расстояние между буквами перевода."
                        text: "Межбуквенный интервал, px"
                        helpControl: spacingRow
                    }
                    PropRow {
                        id: spacingRow
                        key: "letter_spacing"
                        defaultValue: 0
                        SpinBox {
                            visible: spacingRow.manual
                            from: -3
                            to: 20
                            editable: true
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            value: win.propValue("letter_spacing", 0)
                            onValueModified: win.setProp("letter_spacing", true, value)
                        }
                    }
                    FieldLabel {
                        helpText: "Расположение строк внутри блока перевода: слева, по центру или справа."
                        text: "Выравнивание текста"
                        helpControl: alignRow
                    }
                    PropRow {
                        id: alignRow
                        key: "alignment"
                        defaultValue: "center"
                        ComboBox {
                            visible: alignRow.manual
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            readonly property var values: ["left", "center", "right"]
                            model: ["По левому краю", "По центру", "По правому краю"]
                            currentIndex: Math.max(0, values.indexOf(win.propValue("alignment", "center")))
                            onActivated: win.setProp("alignment", true, values[currentIndex])
                        }
                    }
                    FieldLabel {
                        helpText: "Как размещать текст, который не помещается в одну строку."
                        text: "Перенос строк"
                        helpControl: wrapRow
                    }
                    PropRow {
                        id: wrapRow
                        key: "wrap_mode"
                        defaultValue: "word_wrap"
                        ComboBox {
                            visible: wrapRow.manual
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            model: win.wrapNames.map(w => w.label)
                            currentIndex: Math.max(0, win.wrapNames.findIndex(w => w.value === win.propValue("wrap_mode", "word_wrap")))
                            onActivated: win.setProp("wrap_mode", true, win.wrapNames[currentIndex].value)
                        }
                    }
                    FieldLabel {
                        text: "Цвет текста"
                    }
                    PropRow {
                        id: colorRow
                        key: "text_color"
                        defaultValue: "#ffffff"
                        Rectangle {
                            visible: colorRow.manual
                            implicitWidth: 24
                            implicitHeight: 24
                            radius: 4
                            color: win.propValue("text_color", "#ffffff")
                            border.width: 1
                            border.color: palette.mid
                        }
                        TextField {
                            visible: colorRow.manual
                            Layout.preferredWidth: 100
                            maximumLength: 7
                            text: win.propValue("text_color", "#ffffff")
                            validator: RegularExpressionValidator {
                                regularExpression: /#[0-9a-fA-F]{0,6}/
                            }
                            onTextEdited: if (/^#[0-9a-fA-F]{6}$/.test(text))
                                win.setProp("text_color", true, text.toLowerCase())
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                    }
                    FieldLabel {
                        helpText: "Расстояние от текста до границы его фона."
                        text: "Внутренние отступы, px"
                        helpControl: paddingRow
                    }
                    PropRow {
                        id: paddingRow
                        key: "padding"
                        defaultValue: ({
                                left: 4,
                                right: 4,
                                top: 4,
                                bottom: 4
                            })
                        SpinBox {
                            visible: paddingRow.manual
                            from: 0
                            to: 64
                            editable: true
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            value: win.propValue("padding", {
                                left: 4
                            }).left
                            onValueModified: win.setProp("padding", true, {
                                left: value,
                                right: value,
                                top: value,
                                bottom: value
                            })
                        }
                    }
                    FieldLabel {
                        text: "Размер текста: от / до, px"
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        SpinBox {
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            from: 4
                            to: 200
                            editable: true
                            value: win.inplace().minimum_font_size || 8
                            onValueModified: win.setInplace("minimum_font_size", value)
                        }
                        SpinBox {
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            from: 4
                            to: 400
                            editable: true
                            value: win.inplace().maximum_font_size || 96
                            onValueModified: win.setInplace("maximum_font_size", value)
                        }
                    }
                    FieldLabel {
                        text: "Узкий шрифт, если перевод не помещается"
                    }
                    Switch {
                        checked: win.inplace().allow_condensed_fallback !== false
                        onToggled: win.setInplace("allow_condensed_fallback", checked)
                    }
                    FieldLabel {
                        text: "Предпочтительные шрифты (через запятую)"
                    }
                    TextField {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        placeholderText: "Noto Serif, Inter"
                        text: (win.inplace().preferred_fonts || []).join(", ")
                        onEditingFinished: win.setInplace("preferred_fonts", text.split(",").map(f => f.trim()).filter(f => f.length > 0))
                    }
                    Label {
                        text: "Замена шрифта по начертанию оригинала"
                        font.bold: true
                        Layout.columnSpan: 2
                        Layout.topMargin: 4
                    }
                    Repeater {
                        model: win.fontCategories
                        delegate: RowLayout {
                            required property var modelData
                            Layout.columnSpan: 2
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            FieldLabel {
                                text: modelData.label
                            }
                            ComboBox {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                model: ["Авто"].concat(win.bundledFonts)
                                currentIndex: Math.max(0, model.indexOf((win.inplace().font_overrides || {})[modelData.value] || ""))
                                onActivated: {
                                    const o = Object.assign({}, win.inplace().font_overrides || {});
                                    if (currentIndex === 0)
                                        delete o[modelData.value];
                                    else
                                        o[modelData.value] = currentText;
                                    win.setInplace("font_overrides", o);
                                }
                            }
                        }
                    }
                    RowLayout {
                        Layout.columnSpan: 2
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        Button {
                            text: "Определить шрифты заново"
                            onClicked: if (win.controller.reanalyzeFonts)
                                win.controller.reanalyzeFonts()
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        Button {
                            text: "Сбросить оформление перевода поверх текста"
                            onClicked: win.resetKeys(["appearance.inplace"])
                        }
                    }
                    Label {
                        Layout.columnSpan: 2
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: "Защита соседних полей от наложения всегда включена. Если перевод не помещается, поле пропускается и причина выводится в журнал."
                        opacity: 0.7
                    }
                }
                GridLayout {
                    visible: win.current.display_mode !== "inplace"
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    columns: 2
                    columnSpacing: 24
                    rowSpacing: 12
                    FieldLabel {
                        text: "Фон перевода"
                    }
                    ComboBox {
                        objectName: "windowBackgroundStyle"
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        model: win.windowBackgroundStyles.map(m => m.label)
                        currentIndex: Math.max(0, win.windowBackgroundStyles.findIndex(m => m.value === (win.current.appearance.window.background_style || "solid")))
                        onActivated: win.set("appearance.window.background_style", win.windowBackgroundStyles[currentIndex].value)
                    }
                    FieldLabel {
                        text: "Размытие (KWin)"
                        visible: win.current.appearance.window.background_style === "blur"
                    }
                    Switch {
                        visible: win.current.appearance.window.background_style === "blur"
                        checked: win.current.appearance.window.blur_enabled !== false
                        onToggled: win.set("appearance.window.blur_enabled", checked)
                    }
                    FieldLabel {
                        helpText: "100 % — непрозрачно, 0 % — полностью прозрачно. Изменяет только фон."
                        text: "Прозрачность фона"
                        visible: win.current.appearance.window.background_style === "blur"
                        helpControl: backgroundOpacityHelpTarget2
                    }
                    RowLayout {
                        id: backgroundOpacityHelpTarget2
                        visible: win.current.appearance.window.background_style === "blur"
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        Slider {
                            objectName: "blurTint"
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            from: 0
                            to: 0.8
                            value: win.current.appearance.window.blur_tint !== undefined ? win.current.appearance.window.blur_tint : 0.3
                            onMoved: win.set("appearance.window.blur_tint", Math.round(value * 100) / 100)
                        }
                        Label {
                            text: Math.round((win.current.appearance.window.blur_tint !== undefined ? win.current.appearance.window.blur_tint : 0.3) * 100) + "%"
                        }
                    }
                    FieldLabel {
                        text: "Светлый фон, тёмный текст"
                        visible: win.current.appearance.window.background_style === "dim"
                    }
                    Switch {
                        visible: win.current.appearance.window.background_style === "dim"
                        checked: win.current.appearance.window.dim_inverse === true
                        onToggled: win.set("appearance.window.dim_inverse", checked)
                    }
                    Label {
                        Layout.columnSpan: 2
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        opacity: 0.7
                        visible: !win.solidStyle
                        text: win.current.appearance.window.background_style === "transparent" ? "Белый текст с лёгкой тенью, без фона. Цвета текста и фона ниже действуют только в режиме «Сплошной фон»." : "Размытие делает KWin; его силу задают «Системные настройки → Эффекты рабочего стола → Размытие». " + "Без KWin рисуется только тонированный фон. Цвета текста и фона ниже действуют только в режиме «Сплошной фон»."
                    }
                    SectionTitle {
                        text: "Текст перевода"
                    }
                    FieldLabel {
                        helpText: "Шрифт результата перевода. Список содержит шрифты из комплекта приложения."
                        text: "Шрифт перевода"
                        helpControl: translationFontHelpTarget2
                    }
                    ComboBox {
                        id: translationFontHelpTarget2
                        objectName: "translationFontFamily"
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        model: win.fontFamilies
                        currentIndex: win.fontIndex(win.current.appearance.window.font_family)
                        onActivated: win.set("appearance.window.font_family", currentText)
                    }
                    FieldLabel {
                        helpText: "Размер текста перевода в пикселях. Автоматический режим подбирает его по исходному тексту."
                        text: "Размер текста, px"
                        helpControl: translationFontSizeHelpTarget2
                    }
                    SpinBox {
                        id: translationFontSizeHelpTarget2
                        from: 8
                        to: 96
                        editable: true
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        value: win.current.appearance.window.font_size || 20
                        onValueModified: win.set("appearance.window.font_size", value)
                    }
                    FieldLabel {
                        text: "Начертание"
                    }
                    RowLayout {
                        CheckBox {
                            text: "Жирный"
                            checked: win.current.appearance.window.font_bold === true
                            onToggled: win.set("appearance.window.font_bold", checked)
                        }
                        CheckBox {
                            text: "Курсив"
                            checked: win.current.appearance.window.font_italic === true
                            onToggled: win.set("appearance.window.font_italic", checked)
                        }
                    }
                    FieldLabel {
                        text: "Цвет текста"
                        visible: win.solidStyle
                    }
                    ColorRow {
                        visible: win.solidStyle
                        key: "appearance.window.text_color"
                        presets: ["#ffffff", "#ffe066", "#7cf29c", "#7cc7ff"]
                    }
                    FieldLabel {
                        helpText: "Обводка букв повышает читаемость перевода на сложном фоне."
                        text: "Контур текста"
                        helpControl: translationSettingHelpTarget13
                    }
                    RowLayout {
                        id: translationSettingHelpTarget13
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        Switch {
                            checked: win.current.appearance.window.text_outline !== false
                            onToggled: win.set("appearance.window.text_outline", checked)
                        }
                        ColorRow {
                            id: unavailableControl11
                            key: "appearance.window.outline_color"
                            presets: ["#000000", "#400040"]
                            enabled: win.current.appearance.window.text_outline !== false
                            Accessible.description: unavailableHint11.accessibleExplanation
                            UnavailableHint {
                                id: unavailableHint11
                                control: unavailableControl11
                                feature: "Цвет контура текста"
                                reason: "Контур текста выключен."
                                remedy: "Включите контур текста."
                            }
                        }
                    }
                    FieldLabel {
                        helpText: "Расстояние между строками текста перевода."
                        text: "Межстрочный интервал"
                        helpControl: translationSettingHelpTarget14
                    }
                    RowLayout {
                        id: translationSettingHelpTarget14
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        Slider {
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            from: 0.8
                            to: 2.5
                            stepSize: 0.05
                            value: win.current.appearance.window.line_spacing || 1.0
                            onMoved: win.set("appearance.window.line_spacing", Math.round(value * 100) / 100)
                        }
                        Label {
                            text: (win.current.appearance.window.line_spacing || 1.0).toFixed(2)
                        }
                    }
                    FieldLabel {
                        helpText: "Расположение строк внутри блока перевода: слева, по центру или справа."
                        text: "Выравнивание текста"
                        helpControl: translationSettingHelpTarget15
                    }
                    ComboBox {
                        id: translationSettingHelpTarget15
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        readonly property var values: ["left", "center", "right"]
                        model: ["По левому краю", "По центру", "По правому краю"]
                        currentIndex: Math.max(0, values.indexOf(win.current.appearance.window.text_alignment || "center"))
                        onActivated: win.set("appearance.window.text_alignment", values[currentIndex])
                    }
                    FieldLabel {
                        helpText: "Как размещать текст, который не помещается в одну строку."
                        text: "Перенос строк"
                        helpControl: translationSettingHelpTarget16
                    }
                    Switch {
                        id: translationSettingHelpTarget16
                        checked: win.current.appearance.window.text_wrap !== false
                        onToggled: win.set("appearance.window.text_wrap", checked)
                    }
                    SectionTitle {
                        text: "Оригинал"
                    }
                    FieldLabel {
                        text: "Показывать оригинал над переводом"
                    }
                    Switch {
                        checked: win.current.appearance.window.show_original === true
                        onToggled: win.set("appearance.window.show_original", checked)
                    }
                    FieldLabel {
                        text: "Шрифт оригинала"
                        visible: win.current.appearance.window.show_original === true
                    }
                    ComboBox {
                        visible: win.current.appearance.window.show_original === true
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        model: ["Как у перевода"].concat(win.fontFamilies)
                        currentIndex: win.current.appearance.window.original_font_family ? win.fontIndex(win.current.appearance.window.original_font_family) + 1 : 0
                        onActivated: win.set("appearance.window.original_font_family", currentIndex === 0 ? "" : currentText)
                    }
                    FieldLabel {
                        text: "Размер оригинала, px"
                        visible: win.current.appearance.window.show_original === true
                    }
                    SpinBox {
                        visible: win.current.appearance.window.show_original === true
                        from: 8
                        to: 96
                        editable: true
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        value: win.current.appearance.window.original_font_size || 14
                        onValueModified: win.set("appearance.window.original_font_size", value)
                    }
                    FieldLabel {
                        text: "Цвет оригинала"
                        visible: win.current.appearance.window.show_original === true
                    }
                    ColorRow {
                        visible: win.current.appearance.window.show_original === true
                        key: "appearance.window.original_color"
                        presets: ["#b0b0b0", "#ffffff", "#ffe066"]
                    }
                    FieldLabel {
                        text: "Цвет фона"
                        visible: win.solidStyle
                    }
                    ColorRow {
                        visible: win.solidStyle
                        key: "appearance.window.background_color"
                        presets: ["#181818", "#000000", "#202040", "#300030"]
                    }
                    FieldLabel {
                        helpText: "Расстояние от текста до границы его фона."
                        text: "Внутренние отступы, px"
                        helpControl: textPaddingHelpTarget2
                    }
                    SpinBox {
                        id: textPaddingHelpTarget2
                        from: 0
                        to: 64
                        editable: true
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        value: win.current.appearance.window.padding !== undefined ? win.current.appearance.window.padding : 16
                        onValueModified: win.set("appearance.window.padding", value)
                    }
                }
                ResetButton {
                    keys: win.current.display_mode === "inplace" ? ["appearance.inplace"] : ["appearance.window"]
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
                rowSpacing: 14
                Label {
                    text: "Горячие клавиши"
                    font.bold: true
                    Layout.columnSpan: 2
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.7
                    text: "Нажмите кнопку и сочетание. Backspace — очистить, Esc — отмена. Применяются автоматически; " + "если сочетание занято другой программой, будет показано в статусе."
                }
                Repeater {
                    model: win.hotkeyRows
                    delegate: RowLayout {
                        Layout.columnSpan: 2
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        Label {
                            wrapMode: Text.Wrap
                            text: modelData.title
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                        }
                        HotkeyButton {
                            Layout.preferredWidth: 190
                            value: (win.current.hotkeys || ({}))[modelData.key] || ""
                            conflict: win.hotkeyDuplicate(modelData.key)
                            onEdited: v => win.setHotkey(modelData.key, v)
                        }
                    }
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    color: "#ff6b6b"
                    visible: win.hasDuplicateHotkeys
                    text: "Одинаковые сочетания у разных действий — работать будет только одно."
                }
                Button {
                    Layout.columnSpan: 2
                    text: "Сбросить горячие клавиши"
                    onClicked: win.resetKeys(["hotkeys"])
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
                columns: 2
                columnSpacing: 24
                rowSpacing: 14
                RowLayout {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    Label {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        opacity: 0.75
                        text: "Проверка только читает состояние системы: ничего не устанавливает и не скачивает."
                    }
                    Button {
                        id: unavailableControl12
                        text: win.controller.diagnosticsBusy ? "Проверка…" : "Проверить снова"
                        enabled: !win.controller.diagnosticsBusy
                        onClicked: win.controller.refreshDiagnostics()

                        Accessible.description: unavailableHint12.accessibleExplanation
                        UnavailableHint {
                            id: unavailableHint12
                            control: unavailableControl12
                            feature: "Обновить диагностику"
                            reason: "Проверка компонентов ещё выполняется."
                            remedy: "Дождитесь завершения проверки."
                        }
                    }
                }
                Repeater {
                    model: win.diagnostics
                    delegate: Frame {
                        id: checkFrame
                        required property var modelData
                        readonly property color stateColor: modelData.state === "ready" ? "#3ecf6e" : modelData.state === "warning" ? "#ffc23d" : "#ff5555"
                        Layout.columnSpan: 2
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        ColumnLayout {
                            width: parent.width
                            spacing: 4
                            RowLayout {
                                Layout.fillWidth: true
                                Rectangle {
                                    implicitWidth: 12
                                    implicitHeight: 12
                                    radius: 6
                                    color: checkFrame.stateColor
                                }
                                Label {
                                    text: modelData.name
                                    font.bold: true
                                    Layout.fillWidth: true
                                    Layout.minimumWidth: 0
                                    elide: Text.ElideRight
                                }
                                Label {
                                    text: modelData.state === "ready" ? "готово" : modelData.state === "warning" ? "внимание" : "нет"
                                    color: checkFrame.stateColor
                                }
                            }
                            Label {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                wrapMode: Text.WrapAnywhere
                                opacity: 0.7
                                font.pixelSize: 12
                                visible: text.length > 0
                                text: modelData.detail
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                visible: modelData.state !== "ready"
                                TextEdit {
                                    Layout.fillWidth: true
                                    Layout.minimumWidth: 0
                                    readOnly: true
                                    selectByMouse: true
                                    wrapMode: TextEdit.Wrap
                                    color: palette.text
                                    selectionColor: palette.highlight
                                    text: modelData.instruction
                                }
                                Button {
                                    text: "Копировать"
                                    flat: true
                                    onClicked: win.controller.copyText(modelData.instruction)
                                }
                            }
                        }
                    }
                }
                Label {
                    Layout.columnSpan: 2
                    visible: win.diagnostics.length === 0
                    opacity: 0.6
                    text: win.controller.diagnosticsBusy ? "Идёт проверка…" : "Нажмите «Проверить снова»"
                }
            }
        }
        ScrollView {
            id: page8
            objectName: "settingsPage8"
            contentWidth: availableWidth
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            GridLayout {
                width: page8.availableWidth - 16
                columns: 2
                columnSpacing: 24
                rowSpacing: 14
                Item {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 155
                    Image {
                        id: aboutIcon
                        objectName: "aboutIcon"
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: 140
                        height: 140
                        source: Qt.resolvedUrl(".").toString().indexOf("qrc:") === 0 ? "qrc:/lipa/icon.svg" : "../assets/lipa.svg"
                        sourceSize.width: 280
                        sourceSize.height: 280
                        SequentialAnimation on y {
                            running: win.visible && tabs.currentIndex === 7
                            loops: Animation.Infinite
                            NumberAnimation {
                                from: 10
                                to: 2
                                duration: 850
                                easing.type: Easing.InOutSine
                            }
                            NumberAnimation {
                                from: 2
                                to: 10
                                duration: 850
                                easing.type: Easing.InOutSine
                            }
                        }
                    }
                }
                Item {
                    id: tickerViewport
                    Layout.fillWidth: true
                    Layout.preferredHeight: 32
                    clip: true
                    Label {
                        id: tickerText
                        objectName: "aboutTicker"
                        text: "LipaX — распознавание и перевод игрового текста · Qt / KDE / Wayland"
                        font.pixelSize: 16
                        NumberAnimation on x {
                            running: win.visible && tabs.currentIndex === 7
                            from: tickerViewport.width
                            to: -tickerText.implicitWidth
                            duration: 18000
                            loops: Animation.Infinite
                        }
                    }
                }
                Label {
                    text: "Автор: GrayRat"
                    font.bold: true
                    font.pixelSize: 18
                }
                Label {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    textFormat: Text.RichText
                    text: 'Репозиторий проекта: <a href="https://github.com/GrayRats/lipax-qt">https://github.com/GrayRats/lipax-qt</a>'
                    onLinkActivated: url => Qt.openUrlExternally(url)
                }
                Label {
                    text: "Другие исходники проекта:"
                    font.bold: true
                }
                Repeater {
                    model: [
                        {
                            url: "https://github.com/satix-one/lipa.git",
                            role: "форк"
                        },
                        {
                            url: "https://github.com/rtr46/meikipop",
                            role: "зависимость"
                        },
                        {
                            url: "https://github.com/tesseract-ocr/tesseract",
                            role: "зависимость"
                        },
                        {
                            url: "https://github.com/tesseract-ocr/tessdata",
                            role: "зависимость"
                        }
                    ]
                    Label {
                        required property var modelData
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        textFormat: Text.RichText
                        text: '<a href="' + modelData.url + '">' + modelData.url + '</a> (' + modelData.role + ')'
                        onLinkActivated: url => Qt.openUrlExternally(url)
                    }
                }
            }
        }
    }
}
