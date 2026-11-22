import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import QtQuick.Controls.Universal

ApplicationWindow {
    id: win
    UiTheme { id: ui; surface: win.palette.window; textScale: win.general.ui_text_scale || "normal" }
    font: ui.body
    objectName: "settingsWindow"
    readonly property var bergamotModel: { try { return JSON.parse(controller.modelState || "{}") } catch (e) { return ({}) } }
    property var controller
    property var current: ({
            general: {},
            capture: {},
            recognition: {},
            translation: {},
            translation_window: {},
            appearance: {
                window: {},
                inplace: {}
            }
        })
    readonly property var themeChoices: [
        { value: "system", label: qsTr("Как в системе") },
        { value: "breeze", label: "Breeze" },
        { value: "fusion", label: "Fusion" },
        { value: "dark", label: qsTr("Тёмная") },
        { value: "light", label: qsTr("Светлая") }
    ]
    // Each language is named in itself, so it can be found when the interface is in a language one cannot read.
    readonly property var languageChoices: [
        { value: "auto", label: qsTr("Автоматически (как в системе)") },
        { value: "ru", label: "Русский" },
        { value: "en", label: "English" },
        { value: "fr", label: "Français" },
        { value: "zh_CN", label: "中文（简体）" },
        { value: "ja", label: "日本語" },
        { value: "es", label: "Español" }
    ]
    readonly property var mainBackgrounds: [
        { value: "gray", label: qsTr("Серый (стандартный)") },
        { value: "system", label: qsTr("Как в системе (палитра темы)") }
    ]
    readonly property var mainWindow: (current.appearance && current.appearance.main_window) || ({})
    readonly property var logLevels: [
        { value: "error", label: qsTr("Ошибки") },
        { value: "warn", label: qsTr("Предупреждения") },
        { value: "info", label: qsTr("Обычный (info)") },
        { value: "debug", label: qsTr("Подробный (debug)") }
    ]
    // Dark and light switch at once (Universal style); the other styles are chosen when the program starts.
    readonly property var general: current.general || ({})
    readonly property string theme: general.theme || "dark"
    readonly property var themePolicy: controller && typeof controller.themePolicy === "function"
        ? JSON.parse(controller.themePolicy(theme)) : ({restartRequired: false, activeTheme: theme, styleOverridden: false})
    readonly property int universalTheme: themePolicy.activeTheme === "light" ? Universal.Light : Universal.Dark
    Universal.theme: universalTheme
    function choiceIndex(list, value, fallback) {
        const i = list.findIndex(c => c.value === value)
        return i < 0 ? fallback : i
    }
    readonly property var translators: ["google", "yandex", "custom", "deepl", "microsoft", "bergamot"]
    readonly property string providerDescription: ({
        google: qsTr("Онлайн · Google Translate. Распознанный текст отправляется Google через публичный веб-интерфейс без API-ключа. Требуется интернет; доступность зависит от сервиса и языков."),
        yandex: qsTr("Онлайн · Yandex Cloud Translate. Требуются интернет, API-ключ и folder ID облачного каталога. Языковые пары зависят от сервиса."),
        custom: qsTr("Свой сервер · API в формате LibreTranslate. Укажите URL /translate и, если сервер требует, API-ключ. Работа без интернета возможна только с доступным локальным сервером."),
        deepl: qsTr("Онлайн · DeepL API. Требуется ключ в настройках или DEEPL_API_KEY. Ключ с окончанием :fx использует API Free. При ошибке запроса перевод повторяется через Google; при отсутствующем ключе перехода нет."),
        microsoft: qsTr("Онлайн · Microsoft Azure Translator. Требуется API-ключ и регион, если его требует ресурс Azure. Поддерживаются переменные окружения. При ошибке запроса используется Google; при отсутствующем ключе перехода нет."),
        bergamot: qsTr("Локально · Bergamot. Переводит установленными моделями выбранной языковой пары без облачных запросов. Интернет нужен для загрузки моделей. При ошибке сетевой fallback отключён.")
    })[current.translation.service || "google"] || qsTr("Выберите сервис перевода.")
    readonly property var backends: ["auto", "kwin", "portal"]
    readonly property var langs: ["eng", "rus", "jpn", "deu", "fra", "spa", "ita", "por", "kor", "chi_sim", "ukr", "pol"]
    readonly property var targets: ["ru", "en", "uk", "de", "fr", "es", "it", "pt", "ja", "ko", "zh", "pl"]

    title: qsTr("Настройки LipaX")
    width: 880
    height: 740
    minimumWidth: 600
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
        // A new recognition language for an engine with downloadable models: once its status says the model is missing,
        // the user is asked whether to download it (never downloaded unasked).
        if (key === "recognition.language" && (get(current, "recognition.engine") === "rapidocr" || get(current, "recognition.engine") === "meikiocr")
                && String(v).split("+")[0].trim() !== String(get(current, key) || "").split("+")[0].trim())
            rapidOfferWanted = true;
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
            label: qsTr("Фон с размытием")
        },
        {
            value: "transparent",
            label: qsTr("Без фона (прозрачный)")
        },
        {
            value: "dim",
            label: qsTr("Лёгкий тёмный фон")
        },
        {
            value: "solid",
            label: qsTr("Сплошной фон")
        }
    ]
    readonly property string windowDisplayDescription: qsTr("Показывает перевод в отдельном окне. Его можно закрепить поверх игры или сделать свободным и перемещать мышью. Положение и размер задаются на вкладке «Окно перевода».")
    readonly property string inplaceDisplayDescription: qsTr("Размещает перевод на месте исходного текста в области захвата. Требуются точные координаты окна и шрифт для языка перевода. Если способ недоступен, причина указана в подсказке.")
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
            label: qsTr("Авто (по фону вокруг текста)")
        },
        {
            value: "text_replacement",
            label: qsTr("Подмена текста")
        },
        {
            value: "transparent_outline",
            label: qsTr("Прозрачный фон + обводка")
        },
        {
            value: "padded_fill",
            label: qsTr("Заливка с дополнительными полями")
        }
    ]
    readonly property var weightNames: [
        {
            value: "thin",
            label: qsTr("Тонкий")
        },
        {
            value: "extra_light",
            label: qsTr("Сверхсветлый")
        },
        {
            value: "light",
            label: qsTr("Светлый")
        },
        {
            value: "normal",
            label: qsTr("Обычный")
        },
        {
            value: "medium",
            label: qsTr("Средний")
        },
        {
            value: "demi_bold",
            label: qsTr("Полужирный")
        },
        {
            value: "bold",
            label: qsTr("Жирный")
        },
        {
            value: "extra_bold",
            label: qsTr("Сверхжирный")
        },
        {
            value: "black",
            label: qsTr("Чёрный")
        }
    ]
    readonly property var wrapNames: [
        {
            value: "word_wrap",
            label: qsTr("По словам")
        },
        {
            value: "wrap_anywhere",
            label: qsTr("По символам")
        },
        {
            value: "no_wrap",
            label: qsTr("Без переноса")
        },
        {
            value: "elide",
            label: qsTr("Обрезать с многоточием")
        }
    ]
    readonly property var fontCategories: [
        {
            value: "serif",
            label: qsTr("С засечками")
        },
        {
            value: "sans_serif",
            label: qsTr("Без засечек")
        },
        {
            value: "slab_serif",
            label: qsTr("Брусковые засечки")
        },
        {
            value: "monospace",
            label: qsTr("Моноширинный")
        },
        {
            value: "cjk_sans",
            label: qsTr("CJK без засечек")
        },
        {
            value: "cjk_serif",
            label: qsTr("CJK с засечками")
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
            id: propertyMode
            HoverHint { control: propertyMode; feature: qsTr("Авто / Вручную"); explanation: qsTr("Авто использует свойства исходного текста. Вручную открывает редактор этого параметра. Изменения оформления видны сразу; повторное распознавание не требуется.") }
            Layout.preferredWidth: Math.max(120, implicitWidth)
            model: [qsTr("Авто"), qsTr("Вручную")]
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
        Layout.preferredWidth: Math.min(230, win.width * 0.3)
        Layout.maximumWidth: Math.min(280, win.width * 0.34)
        Layout.minimumWidth: 0
    }
    component SectionTitle: Label {
        font: ui.heading
        wrapMode: Text.Wrap
        Layout.fillWidth: true
        Layout.columnSpan: 2
        Layout.topMargin: 6
    }
    component ResetButton: Button {
        id: resetSectionButton
        property var keys: []
        Layout.columnSpan: 2
        Layout.topMargin: 10
        text: keys.some(k => k.startsWith("appearance.")) ? qsTr("Сбросить настройки оформления") : qsTr("Сбросить настройки раздела")
        onClicked: win.resetKeys(keys)
        HoverHint { control: resetSectionButton; feature: qsTr("Сброс раздела"); explanation: qsTr("Восстанавливает исходные значения только этого раздела. Другие настройки и профили игры сохраняются.") }
    }
    // Swatch opens the system color dialog; text accepts only complete #rrggbb values.
    component ColorRow: RowLayout {
        id: colorRow
        property string key
        property var presets: []
        readonly property string value: win.get(win.current, key) || "#000000"
        function openPicker() { colorDialog.key = key; colorDialog.selectedColor = value; colorDialog.open() }
        Layout.fillWidth: true
        Layout.minimumWidth: 0
        Rectangle {
            id: colorSwatch
            activeFocusOnTab: true
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Выбрать цвет ") + colorRow.value
            Keys.onSpacePressed: colorRow.openPicker()
            Keys.onReturnPressed: colorRow.openPicker()
            HoverHint { control: colorSwatch; feature: qsTr("Выбрать цвет"); explanation: qsTr("Открывает системный диалог выбора цвета. Также можно ввести код #RRGGBB в соседнем поле. Изменение видно сразу.") }
            implicitWidth: 28
            implicitHeight: 28
            radius: 4
            color: colorRow.value
            border.width: 1
            border.color: activeFocus ? palette.highlight : palette.mid
            MouseArea {
                anchors.fill: parent
                onClicked: {
                    colorRow.openPicker();
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
                id: presetSwatch
                activeFocusOnTab: true
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Применить цвет ") + modelData
                Keys.onSpacePressed: win.set(colorRow.key, modelData)
                Keys.onReturnPressed: win.set(colorRow.key, modelData)
                HoverHint { control: presetSwatch; feature: qsTr("Готовый цвет"); explanation: qsTr("Применяет цвет ") + modelData + qsTr(" к этому параметру. Для произвольного цвета откройте системный диалог.") }
                implicitWidth: 22
                implicitHeight: 22
                radius: 4
                color: modelData
                border.width: 1
                border.color: activeFocus ? palette.highlight : palette.mid
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
    // The PaddleOCR environment of the chosen Python and language (see Controller.refreshPaddle): checked by itself
    // when PaddleOCR is chosen and whenever the Python or the language changes.
    readonly property var paddle: { try { return JSON.parse(controller.paddleJson || "{}") } catch (e) { return ({}) } }
    readonly property string paddleKey: usesPaddle ? (current.recognition.paddle_python || "") + "|" + (current.recognition.language || "") : ""
    onPaddleKeyChanged: if (paddleKey.length > 0) paddleTimer.restart()
    Timer {
        id: paddleTimer
        interval: 700
        onTriggered: if (win.usesPaddle) win.controller.refreshPaddle()
    }
    onVisibleChanged: {
        if (visible && usesPaddle) paddleTimer.restart()
        if (visible && usesRapid) rapidTimer.restart()
    }
    // RapidOCR (see Controller.refreshRapid): the model of the recognition language and ONNX Runtime; checked when the
    // engine is chosen, the language or the model variant changes, and after a download or deletion. Nothing is fetched.
    readonly property var rapid: { try { return JSON.parse(controller.rapidJson || "{}") } catch (e) { return ({}) } }
    readonly property string rapidKey: usesRapid ? (current.recognition.engine || "") + "|" + (current.recognition.language || "") + "|" + (current.recognition.rapid_variant || "mobile") + "|" + (current.recognition.rapid_threads || 0) : ""
    onRapidKeyChanged: if (rapidKey.length > 0) rapidTimer.restart()
    Timer {
        id: rapidTimer
        interval: 500
        onTriggered: if (win.usesRapid) win.controller.refreshRapid()
    }
    // Bergamot versions (see Controller.refreshModelVersions): the installed set against the bundled catalog, no network.
    // Re-read when the pair changes and when an installation starts or ends.
    readonly property var modelVersions: { try { return JSON.parse(controller.modelVersions || "{}") } catch (e) { return ({}) } }
    readonly property string modelVersionsKey: current.translation.service === "bergamot"
        ? (bergamotModel.pair || "") + "|" + (bergamotModel.status || "") + "|" + (controller.modelBusy === true) : ""
    onModelVersionsKeyChanged: if (modelVersionsKey.length > 0) modelVersionsTimer.restart()
    Timer {
        id: modelVersionsTimer
        interval: 300
        onTriggered: win.controller.refreshModelVersions()
    }
    function modelSize(bytes) { return ((bytes || 0) / 1048576).toFixed(1).replace(".", ",") + qsTr(" МБ") }
    function modelVersionLabel(v) {
        return v.version + " — " + modelSize(v.size) + (v.installed ? qsTr(" (установлена)") : "") + (v.newest ? qsTr(" · новейшая") : "")
    }
    // Offer to download the model of a language the user has just switched to (RapidOCR and MeikiOCR chosen explicitly;
    // `auto` has the model and its «Скачать» button in the block of the engine and does not nag).
    property bool rapidOfferWanted: false
    property var pendingRapidOffer: null
    onRapidChanged: offerRapidModel()
    function offerRapidModel() {
        if (!rapidOfferWanted)
            return;
        const r = rapid, engine = current.recognition.engine;
        const primary = (current.recognition.language || "").split("+")[0].trim();
        // A status of another engine or of the previous language is not the answer yet.
        if (!r.language || r.language !== primary || r.engine !== engine)
            return;
        rapidOfferWanted = false;
        if (!visible || r.supported !== true || !r.selected || r.selected.installed || controller.rapidBusy === true)
            return;
        pendingRapidOffer = r.selected;
        rapidOfferDialog.open();
    }
    Dialog {
        id: rapidOfferDialog
        objectName: "rapidOfferDialog"
        title: qsTr("Нужна модель распознавания")
        modal: true
        anchors.centerIn: parent
        width: Math.min(parent.width - 40, 480)
        standardButtons: Dialog.Yes | Dialog.No
        Label {
            width: parent.width
            wrapMode: Text.Wrap
            text: win.pendingRapidOffer ? qsTr("Для языка «") + (win.rapid.language || "") + qsTr("» нужна модель «") + win.pendingRapidOffer.label + "» ("
                + win.megabytes(win.pendingRapidOffer.size) + qsTr("). Скачать её сейчас?\n\nФайлы берутся ")
                + (win.pendingRapidOffer.engine === "meikiocr" ? qsTr("с Hugging Face (rtr46, лицензия LGPL-3.0)") : qsTr("из репозитория RapidAI (ModelScope)"))
                + qsTr(" и проверяются по SHA-256. Без модели ") + (win.pendingRapidOffer.engine === "meikiocr" ? "MeikiOCR" : "RapidOCR") + qsTr(" не прочитает этот язык.") : ""
        }
        onAccepted: if (win.pendingRapidOffer) win.controller.downloadRapidModel(win.pendingRapidOffer.id)
    }
    function megabytes(bytes) { return Math.round((bytes || 0) / 1e6) + qsTr(" МБ") }

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
        if (!usesTesseract)
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
                    label: c + qsTr(" — не установлен"),
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
    // Languages that can be added: from the distribution's repository (administrator password) or, where the
    // repository has no package (Arch: only AUR), downloaded as a model into the user's directory (no password).
    readonly property var installable: (tess.installable || []).filter(l => l.available).map(l => ({
                code: l.code,
                package: l.package,
                name: l.name,
                command: l.command,
                method: l.method || "package",
                label: l.code + " — " + l.name + ((l.method || "package") === "download" ? qsTr(" (скачать модель)") : " (" + l.package + ")")
            }))
    readonly property var missing: {
        // Зависимость от tesseractJson обновляет сообщения после установки пакета.
        if (!usesTesseract)
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
            title: qsTr("Запустить / остановить слежение")
        },
        {
            key: "select_region",
            title: qsTr("Выбрать область захвата")
        },
        {
            key: "translate_once",
            title: qsTr("Перевести сейчас")
        },
        {
            key: "toggle_translation",
            title: qsTr("Показать / скрыть перевод")
        },
        {
            key: "toggle_pin",
            title: qsTr("Закрепить окно / сделать окно свободным")
        }
    ]
    function setHotkey(key, v) {
        const h = Object.assign({}, current.hotkeys || ({}));
        h[key] = v;
        set("hotkeys", h);
    }
    // Кнопки геймпада (R-12): включение, список найденных геймпадов и запись сочетания.
    property string learningPadKey: ""
    readonly property var gamepadProbe: { try { return JSON.parse(controller.gamepadJson || "{}") } catch (e) { return ({}) } }
    function gamepadChord(key) { return get(current, "hotkeys.gamepad." + key) || "" }
    function gamepadDuplicate(key) {
        const c = gamepadChord(key);
        return c.length > 0 && hotkeyRows.some(r => r.key !== key && gamepadChord(r.key) === c);
    }
    function startLearning(key) { learningPadKey = key; controller.learnGamepadChord(key) }
    Connections {
        target: win.controller
        function onGamepadChordLearned(action, chord) {
            if (win.learningPadKey !== action) return
            win.learningPadKey = ""
            if (chord.length > 0) win.set("hotkeys.gamepad." + action, chord)
        }
    }
    function hotkeyDuplicate(key) {
        const h = current.hotkeys || ({});
        return !!h[key] && hotkeyRows.some(r => r.key !== key && h[r.key] === h[key]);
    }
    readonly property bool hasDuplicateHotkeys: hotkeyRows.some(r => hotkeyDuplicate(r.key))

    readonly property var frameModes: [
        {
            value: "pattern",
            label: qsTr("Узор (error purple/black)")
        },
        {
            value: "solid",
            label: qsTr("Простая красная обводка")
        },
        {
            value: "off",
            label: qsTr("Выкл.")
        },
        {
            value: "selection",
            label: qsTr("При выделении")
        }
    ]

    function setLangs(primary, extras) {
        set("recognition.language", [primary].concat(extras.filter(c => c !== primary)).join("+"));
    }

    property string pendingPackage: ""
    property string pendingCommand: ""
    property string pendingCode: ""
    property string pendingMethod: "package"
    function askInstall(code, name) {
        const info = installable.find(l => l.code === code);
        if (!info) return;
        pendingCode = code;
        pendingPackage = info.package;
        pendingMethod = info.method;
        pendingCommand = info.command;
        installDialog.text = info.method === "download"
            ? qsTr("Скачать языковую модель «") + name + "»?\n\n" + info.command + qsTr("\n\nФайл берётся из официального репозитория Tesseract. Пароль администратора не нужен; модель сразу появится в списке языков.")
            : qsTr("Установить языковой пакет «") + name + qsTr("»?\n\nБудет выполнена команда:\n") + info.command + qsTr("\n\nСистема запросит пароль администратора.");
        installDialog.open();
    }
    Dialog {
        id: installDialog
        objectName: "installDialog"
        property string text: ""
        title: win.pendingMethod === "download" ? qsTr("Загрузка языковой модели") : qsTr("Установка языкового пакета")
        modal: true
        anchors.centerIn: parent
        width: Math.min(parent.width - 40, 460)
        standardButtons: Dialog.Yes | Dialog.No
        Label {
            width: parent.width
            wrapMode: Text.Wrap
            text: installDialog.text
        }
        onAccepted: win.pendingMethod === "download" ? win.controller.installLanguage(win.pendingCode) : win.controller.installPackage(win.pendingPackage)
    }

    property var pendingRapidModel: null
    function askDeleteRapid(model) {
        if (!model) return;
        pendingRapidModel = model;
        rapidDeleteDialog.open();
    }
    Dialog {
        id: rapidDeleteDialog
        objectName: "rapidDeleteDialog"
        title: qsTr("Удаление модели OCR")
        modal: true
        anchors.centerIn: parent
        width: Math.min(parent.width - 40, 460)
        standardButtons: Dialog.Yes | Dialog.No
        Label {
            width: parent.width
            wrapMode: Text.Wrap
            text: win.pendingRapidModel ? qsTr("Удалить модель «") + win.pendingRapidModel.label + "» (" + win.pendingRapidModel.variant + ", " + win.megabytes(win.pendingRapidModel.size) + ")?\n\n" + (win.pendingRapidModel.engine === "meikiocr" ? "MeikiOCR" : "RapidOCR") + qsTr(" не сможет читать этот язык, пока модель не будет скачана снова.") : ""
        }
        onAccepted: if (win.pendingRapidModel) win.controller.deleteRapidModel(win.pendingRapidModel.id)
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
    readonly property bool usesRapid: current.recognition.engine === "rapidocr" || current.recognition.engine === "meikiocr" || current.recognition.engine === "auto" || current.recognition.engine === "consensus"
    readonly property string rapidName: current.recognition.engine === "meikiocr" ? "MeikiOCR" : "RapidOCR"
    // Tesseract's languages and packages matter unless the engine is one that reads with its own models.
    readonly property bool usesTesseract: current.recognition.engine !== "paddleocr" && current.recognition.engine !== "rapidocr" && current.recognition.engine !== "meikiocr"
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
            notifyRegions(qsTr("Можно создать не больше ") + maxRegions + qsTr(" областей захвата. Удалите одну, чтобы добавить новую."));
            return false;
        }
        let n = r.length + 1;
        while (r.some(x => x.id === "region-" + n))
            ++n;
        // A new region starts inactive: it becomes active once its area is selected.
        r.push({
            id: "region-" + n,
            name: qsTr("Область захвата ") + n,
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
            model: [qsTr("Источник изображения"), qsTr("Распознавание текста"), qsTr("Перевод"), qsTr("Отображение перевода"), qsTr("Окно перевода"), qsTr("Оформление перевода"), qsTr("Клавиши"), qsTr("Статус"), qsTr("Общие"), qsTr("Приложение"), qsTr("О программе")]
            TabButton {
                id: settingsTab
                text: modelData
                width: implicitWidth
                contentItem: Label {
                    text: settingsTab.text
                    font: settingsTab.font
                    color: settingsTab.checked ? win.palette.highlightedText : win.palette.windowText
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
                background: Rectangle {
                    color: settingsTab.checked ? win.palette.highlight : settingsTab.hovered ? win.palette.alternateBase : "transparent"
                    radius: ui.radius
                    border.width: settingsTab.visualFocus ? 2 : 0
                    border.color: win.palette.highlight
                }
            }
        }
        onCurrentIndexChanged: if (currentIndex === 7 && win.diagnostics.length === 0)
            win.controller.refreshDiagnostics()
    }
    footer: ToolBar {
        RowLayout {
            id: settingsFooter
            anchors.fill: parent
            anchors.margins: ui.gap
            Label {
                text: autosave.running ? qsTr("Сохранение…") : win.themePolicy.restartRequired
                    ? qsTr("Некоторые изменения вступят в силу после перезапуска LipaX. Можно продолжать работу.")
                    : qsTr("Изменения сохраняются автоматически")
                wrapMode: Text.Wrap
                Layout.fillWidth: true
                opacity: 0.85
            }
            Button {
                    id: settingsActionUnique1
                    HoverHint { control: settingsActionUnique1; feature: settingsActionUnique1.text; explanation: qsTr("Закрывает настройки. Изменённые значения сохраняются автоматически перед закрытием."); active: settingsActionUnique1.enabled }
                text: qsTr("Закрыть")
                onClicked: win.close()
            }
        }
        implicitHeight: settingsFooter.implicitHeight + 2 * ui.gap
    }
    StackLayout {
        anchors.fill: parent
        anchors.margins: ui.margin
        anchors.rightMargin: ui.margin
        anchors.topMargin: ui.margin
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
                columnSpacing: ui.margin
                rowSpacing: ui.gap
                SectionTitle {
                    text: qsTr("Источник захвата")
                }
                Button {
                    id: settingsActionUnique2
                    HoverHint { control: settingsActionUnique2; feature: settingsActionUnique2.text; explanation: qsTr("Открывает выбор окна игры. Текущий способ захвата сохраняется перед выбором. Для знакомой игры восстанавливается её профиль."); active: settingsActionUnique2.enabled }
                    objectName: "selectCaptureWindow"
                    Layout.columnSpan: 2
                    text: qsTr("Выбрать окно для захвата")
                    onClicked: {
                        win.apply();
                        win.controller.pickWindow();
                    }
                }
                FieldLabel {
                    helpText: qsTr("Изменение применяется при следующем выборе окна игры. KWin получает изображение и положение окна. Portal открывает системный выбор источника; положение окна на рабочем столе ему недоступно.")
                    text: qsTr("Способ захвата")
                    helpControl: captureSourceHelpTarget1
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    ComboBox {
                        id: captureSourceHelpTarget1
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        model: [qsTr("Авто: KWin, а если недоступен — portal"), qsTr("KWin ScreenShot2 (только KDE)"), "xdg-desktop-portal (PipeWire)"]
                        currentIndex: Math.max(0, win.backends.indexOf(win.current.capture.source))
                        onActivated: win.set("capture.source", win.backends[currentIndex])
                    }
                    Label {
                        objectName: "captureBackendNote"
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        color: ui.warning
                        text: qsTr("Способ захвата сменится при следующем выборе окна игры (кнопка «Выбрать окно для захвата»).")
                    }
                    Label {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        opacity: 0.85
                        visible: win.current.capture.source !== "kwin"
                        text: qsTr("Portal показывает системный диалог выбора окна и требует gstreamer с gst-plugin-pipewire. ") + qsTr("Выбор запоминается, диалог при следующем запуске не нужен. Выбранное окно переключается заново кнопкой «Выбрать окно для захвата».")
                    }
                    Switch {
                        id: unavailableControl1
                        objectName: "portalFillsMonitor"
                        Layout.fillWidth: true
                        visible: win.current.capture.source !== "kwin"
                        enabled: !!win.current.translation_window.screen
                        checked: win.current.capture.portal_fills_monitor === true
                        text: qsTr("Выбранное через portal окно занимает весь указанный экран")
                        onToggled: win.set("capture.portal_fills_monitor", checked)

                        Accessible.description: unavailableHint1.accessibleExplanation
                        UnavailableHint {
                            id: unavailableHint1
                            control: unavailableControl1
                            feature: qsTr("Полноэкранный захват portal")
                            reason: qsTr("Экран не указан. Portal не сообщает положение окна.")
                            remedy: qsTr("Выберите экран на вкладке «Окно перевода».")
                        }
                    }
                    Label {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        color: ui.warning
                        visible: win.current.capture.source !== "kwin" && !win.current.translation_window.screen
                        text: qsTr("Для полноэкранного portal сначала выберите экран на вкладке «Окно перевода».")
                    }
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: qsTr("KWin исключает серверную рамку. Если приложение рисует собственный заголовок (CSD), выделите область захвата ниже него. Portal не сообщает положение окна на рабочем столе.")
                }
                FieldLabel {
                    text: qsTr("Запоминать настройки каждой игры")
                    helpText: qsTr("Сохраняет области, OCR, язык и расположение перевода для выбранной игры. При повторном выборе её окна профиль восстанавливается.")
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
                    opacity: 0.855
                    text: qsTr("Области захвата, языки, движок распознавания и положение перевода сохраняются отдельно для каждого окна (по его классу) ") + qsTr("и возвращаются, когда вы снова выбираете окно этой игры. Оформление перевода и клавиши общие.")
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
                            text: modelData + (profile.caption ? " — " + profile.caption : "") + qsTr(" · областей захвата: ") + (profile.capture.regions || []).filter(r => r.rect).length + (selected ? qsTr(" (выбрана)") : "")
                        }
                        Button {
                    HoverHint { control: unavailableControl2; feature: unavailableControl2.text; explanation: qsTr("Удаляет только сохранённый профиль этой игры. Активный профиль нельзя удалить, пока выбрано его окно."); active: unavailableControl2.enabled }
                            id: unavailableControl2
                            objectName: "forgetGame_" + modelData
                            text: qsTr("Забыть профиль игры")
                            enabled: !selected
                            onClicked: win.controller.forgetGameProfile(modelData)

                            Accessible.description: unavailableHint2.accessibleExplanation
                            UnavailableHint {
                                id: unavailableHint2
                                control: unavailableControl2
                                feature: qsTr("Удалить сохранённый профиль игры")
                                reason: qsTr("Этот профиль используется выбранным окном и автоматически сохраняется.")
                                remedy: qsTr("Сначала выберите другое окно.")
                            }
                        }
                    }
                }
                SectionTitle {
                    text: qsTr("Рамка областей захвата в игре")
                }
                FieldLabel {
                    text: qsTr("Показывать рамку")
                    helpText: qsTr("Выбирает, когда видна рамка: постоянно, временно после выбора или никогда. Изменение применяется сразу.")
                    helpControl: fieldHelpEditor101
                }
                ComboBox {
                    id: fieldHelpEditor101
                    objectName: "regionFrameMode"
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    model: win.frameModes.map(m => m.label)
                    currentIndex: Math.max(0, win.frameModes.findIndex(m => m.value === (win.current.capture.region_frame_mode || "selection")))
                    onActivated: win.set("capture.region_frame_mode", win.frameModes[currentIndex].value)
                }
                FieldLabel {
                    text: qsTr("Исчезает через, с")
                    visible: win.current.capture.region_frame_mode === "selection"
                    helpText: qsTr("Время показа временной рамки после выбора области. Постоянный режим не использует этот таймер.")
                    helpControl: fieldHelpEditor102
                }
                SpinBox {
                    id: fieldHelpEditor102
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
                    helpText: qsTr("Цвет рамки, обозначающей участок изображения, который передаётся на распознавание.")
                    text: qsTr("Цвет рамки области захвата")
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
                    text: qsTr("Толщина рамки, px")
                    visible: win.current.capture.region_frame_mode !== "off"
                    helpText: qsTr("Толщина границы области в логических пикселях. Не меняет границы захвата или OCR.")
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
                    text: qsTr("Когда окно перевода закреплено")
                    helpText: qsTr("Определяет поведение рамки области при закреплённом переводе. Не меняет сами координаты захвата.")
                    helpControl: fieldHelpEditor103
                }
                ComboBox {
                    id: fieldHelpEditor103
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    model: [qsTr("Полупрозрачная обводка"), qsTr("Скрыть обводку")]
                    currentIndex: win.current.capture.region_frame_pinned === "hide" ? 1 : 0
                    onActivated: win.set("capture.region_frame_pinned", currentIndex === 1 ? "hide" : "dim")
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.85
                    text: qsTr("Рамка рисуется вокруг каждой активной области захвата и не мешает кликам. У каждой области свой таймер; ") + qsTr("режим можно переопределить для отдельной области в разделе областей захвата.")
                }
                Label {
                    objectName: "frameBlockerNote"
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    color: ui.warning
                    visible: win.frameBlocker.length > 0
                    text: qsTr("Для этого окна недоступно: ") + win.frameBlocker + "."
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.855
                    text: qsTr("До ") + win.maxRegions + qsTr(" областей захвата. Каждая распознаётся отдельно; пустые поля берутся из общих настроек. ") + qsTr("Выключенная область захвата сохраняет выделение.")
                }
                FieldLabel {
                    text: qsTr("Разрешить несколько активных областей захвата")
                    helpText: qsTr("Разрешает распознавать до трёх областей по очереди. Без этой опции выбор области выключает остальные.")
                    helpControl: fieldHelpEditor104
                }
                Switch {
                    id: fieldHelpEditor104
                    objectName: "allowMultipleRegions"
                    checked: win.current.capture.allow_multiple_regions === true
                    onToggled: win.setAllowMultipleRegions(checked)
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.85
                    text: win.current.capture.allow_multiple_regions === true ? qsTr("Активными могут быть до ") + win.maxRegions + qsTr(" областей захвата одновременно.") : qsTr("Активна одна область захвата: включение другой выключает предыдущую.")
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
                                text: qsTr("Активна")
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
                                text: regionFrame.hasRect ? qsTr("Область захвата задана") : qsTr("Область захвата не задана")
                                color: regionFrame.hasRect ? palette.text : "#ffb347"
                            }
                            Flow {
                                objectName: "captureRegionActions"
                                Layout.columnSpan: 2
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                spacing: 8
                                Button {
                    HoverHint { control: unavailableControl3; feature: unavailableControl3.text; explanation: qsTr("Включает эту область и открывает выделение прямоугольника с текстом в окне игры."); active: unavailableControl3.enabled }
                                    id: unavailableControl3
                                    text: qsTr("Выбрать область захвата")
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
                                        feature: qsTr("Выбрать область захвата")
                                        reason: qsTr("Окно захвата ещё не выбрано.")
                                        remedy: qsTr("Нажмите «Выбрать окно для захвата».")
                                    }
                                }
                                Button {
                    HoverHint { control: unavailableControl4; feature: unavailableControl4.text; explanation: qsTr("Очищает выделение этой области. Остальные области и их настройки сохраняются."); active: unavailableControl4.enabled }
                                    id: unavailableControl4
                                    text: qsTr("Сбросить область захвата")
                                    enabled: regionFrame.hasRect
                                    onClicked: win.setRegionField(regionFrame.index, "rect", null)

                                    Accessible.description: unavailableHint4.accessibleExplanation
                                    UnavailableHint {
                                        id: unavailableHint4
                                        control: unavailableControl4
                                        feature: qsTr("Сбросить область захвата")
                                        reason: qsTr("У этой области ещё нет выделения.")
                                        remedy: qsTr("Сначала выделите область в окне игры.")
                                    }
                                }
                                Item {
                                    Layout.fillWidth: true
                                }
                                Button {
                    HoverHint { control: unavailableControl5; feature: unavailableControl5.text; explanation: qsTr("Удаляет только эту область и её параметры. Должна остаться хотя бы одна область."); active: unavailableControl5.enabled }
                                    id: unavailableControl5
                                    text: qsTr("Удалить область захвата")
                                    enabled: (win.current.capture.regions || []).length > 1
                                    onClicked: win.removeRegion(regionFrame.index)

                                    Accessible.description: unavailableHint5.accessibleExplanation
                                    UnavailableHint {
                                        id: unavailableHint5
                                        control: unavailableControl5
                                        feature: qsTr("Удалить область захвата")
                                        reason: qsTr("Нужно оставить хотя бы одну область захвата.")
                                        remedy: qsTr("Очистите выделение или добавьте другую область.")
                                    }
                                }
                            }
                            FieldLabel {
                                helpText: qsTr("Выберите язык исходного текста. Для выбранного движка должна быть установлена соответствующая языковая модель.")
                                text: qsTr("Язык распознавания")
                                helpControl: recognitionLanguageHelpTarget1
                            }
                            ComboBox {
                                id: recognitionLanguageHelpTarget1
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                model: [qsTr("Как в общих настройках")].concat(win.langs)
                                currentIndex: Math.max(0, win.langs.indexOf(regionFrame.modelData.recognition_language) + 1)
                                onActivated: win.setRegionField(regionFrame.index, "recognition_language", currentIndex === 0 ? "" : currentText)
                            }
                            FieldLabel {
                                helpText: qsTr("Язык, на котором будет показан результат перевода.")
                                text: qsTr("Язык перевода")
                                helpControl: targetLanguageHelpTarget1
                            }
                            ComboBox {
                                id: targetLanguageHelpTarget1
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                model: [qsTr("Как в общих настройках")].concat(win.targets)
                                currentIndex: Math.max(0, win.targets.indexOf(regionFrame.modelData.target_language) + 1)
                                onActivated: win.setRegionField(regionFrame.index, "target_language", currentIndex === 0 ? "" : currentText)
                            }
                            FieldLabel {
                                helpText: qsTr("OCR (Optical Character Recognition) распознаёт текст на изображении. Смена движка или модели переинициализирует распознавание без перезапуска LipaX. Tesseract, PaddleOCR, RapidOCR и MeikiOCR распознают текст в области захвата. MeikiOCR читает только японский (игры и визуальные новеллы). Автоматический режим при низкой уверенности Tesseract повторяет распознавание через MeikiOCR (японский, если его модель скачана), затем RapidOCR или PaddleOCR.")
                                text: qsTr("Движок распознавания")
                                helpControl: recognitionEngineHelpTarget1
                            }
                            ComboBox {
                                id: recognitionEngineHelpTarget1
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                readonly property var values: ["", "tesseract", "paddleocr", "rapidocr", "meikiocr", "auto", "consensus"]
                                model: [qsTr("Как в общих настройках"), "Tesseract", "PaddleOCR", "RapidOCR", "MeikiOCR", qsTr("Авто"), qsTr("Консенсус")]
                                currentIndex: Math.max(0, values.indexOf(regionFrame.modelData.engine || ""))
                                onActivated: win.setRegionField(regionFrame.index, "engine", values[currentIndex] || null)
                            }
                            FieldLabel {
                                text: qsTr("Рамка области захвата")
                                helpText: qsTr("Переопределяет режим рамки для этой области. Общий режим продолжает действовать для других областей.")
                            }
                            ComboBox {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                model: [qsTr("Как в общих настройках")].concat(win.frameModes.map(m => m.label))
                                currentIndex: Math.max(0, win.frameModes.findIndex(m => m.value === regionFrame.modelData.frame_mode) + 1)
                                onActivated: win.setRegionField(regionFrame.index, "frame_mode", currentIndex === 0 ? "" : win.frameModes[currentIndex - 1].value)
                            }
                            FieldLabel {
                                text: qsTr("Интервал / задержка перед распознаванием, мс")
                                helpText: qsTr("Интервал задаёт частоту проверки области, задержка ждёт стабилизации изображения. Изменение обновляет обработку области без перезапуска приложения.")
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
                    id: settingsActionUnique3
                    HoverHint { control: settingsActionUnique3; feature: settingsActionUnique3.text; explanation: qsTr("Добавляет выключенную область. Сначала выделите её границы; можно хранить не больше трёх областей."); active: settingsActionUnique3.enabled }
                        objectName: "addRegion"
                        text: qsTr("Добавить область захвата")
                        onClicked: win.addRegion()
                    }
                    Label {
                        text: (win.current.capture.regions || []).length + qsTr(" из ") + win.maxRegions
                        opacity: 0.85
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
                    color: ui.warning
                }
                ResetButton {
                    keys: ["capture.regions", "capture.active_region", "capture.allow_multiple_regions"]
                    text: qsTr("Сбросить области захвата (выделения будут очищены)")
                }
                ResetButton {
                    text: qsTr("Сбросить настройки захвата")
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
                columnSpacing: ui.margin
                rowSpacing: ui.gap
                FieldLabel {
                    helpText: qsTr("Tesseract, PaddleOCR, RapidOCR и MeikiOCR распознают текст в области захвата. MeikiOCR читает только японский (игры и визуальные новеллы). Автоматический режим при низкой уверенности Tesseract повторяет распознавание через MeikiOCR (японский, если его модель скачана), затем RapidOCR или PaddleOCR.")
                    text: qsTr("Движок распознавания")
                    helpControl: recognitionEngineHelpTarget2
                }
                ComboBox {
                    id: recognitionEngineHelpTarget2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    objectName: "ocrEngineBox"
                    readonly property var values: ["tesseract", "paddleocr", "rapidocr", "meikiocr", "auto", "consensus"]
                    model: ["Tesseract", "PaddleOCR 3.x", "RapidOCR (PP-OCRv5)", qsTr("MeikiOCR (японский, игры)"), qsTr("Авто: Tesseract, при сомнении MeikiOCR, RapidOCR или PaddleOCR"), qsTr("Консенсус: все движки читают кадр, побеждает правдоподобнее и совпавшее")]
                    currentIndex: Math.max(0, values.indexOf(win.current.recognition.engine))
                    onActivated: win.set("recognition.engine", values[currentIndex])
                }
                FieldLabel {
                    helpText: qsTr("Путь к интерпретатору Python, в окружении которого установлены PaddleOCR и PaddlePaddle.")
                    text: qsTr("Python для PaddleOCR")
                    visible: win.usesPaddle
                    helpControl: paddlePythonHelpTarget1
                }
                TextField {
                    id: paddlePythonHelpTarget1
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    visible: win.usesPaddle
                    text: win.current.recognition.paddle_python || "python3"
                    placeholderText: qsTr("/путь/к/venv/bin/python")
                    onTextEdited: win.set("recognition.paddle_python", text)
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    visible: win.usesPaddle
                    text: win.current.recognition.engine === "auto" ? qsTr("Сначала читает Tesseract; если он сам не уверен в результате (ниже 60 %), текст перечитывает RapidOCR, а если RapidOCR недоступен (нет модели или ONNX Runtime) — PaddleOCR. Если и он не установлен, остаётся результат Tesseract; недоступный движок пробуется снова через несколько минут. Установка: docs/RapidOCR.md, docs/PaddleOCR.md.") : qsTr("PaddleOCR использует основной язык; дополнительные языки относятся к Tesseract. При первом запуске загружаются модели. Установка: docs/PaddleOCR.md.")
                }
                ColumnLayout {
                    objectName: "paddleCheck"
                    visible: win.usesPaddle
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    spacing: 8
                    RowLayout {
                        Layout.fillWidth: true
                        Label {
                            objectName: "paddleSummary"
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            wrapMode: Text.Wrap
                            font.bold: true
                            color: win.controller.paddleBusy || win.paddle.ready === undefined ? palette.text : win.paddle.ready ? ui.success : ui.error
                            text: win.controller.paddleBusy ? qsTr("Проверка окружения PaddleOCR…") : (win.paddle.ready === undefined ? qsTr("Окружение PaddleOCR ещё не проверено") : (win.paddle.ready ? "✓ " : "✗ ") + win.paddle.summary)
                        }
                        Button {
                    id: settingsActionUnique4
                    HoverHint { control: settingsActionUnique4; feature: settingsActionUnique4.text; explanation: qsTr("Проверяет выбранное окружение PaddleOCR и язык распознавания. Проверка не устанавливает пакеты."); active: settingsActionUnique4.enabled }
                            objectName: "paddleRecheck"
                            text: qsTr("Проверить снова")
                            enabled: !win.controller.paddleBusy
                            ToolTip.visible: hovered
                            ToolTip.delay: 400
                            ToolTip.text: qsTr("Заново проверить Python, пакеты paddleocr и paddlepaddle, язык и модели")
                            onClicked: win.controller.refreshPaddle()
                        }
                    }
                    Repeater {
                        model: win.paddle.problems || []
                        delegate: ColumnLayout {
                            id: problem
                            required property var modelData
                            objectName: "paddleProblem_" + modelData.id
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            spacing: 4
                            Label {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                wrapMode: Text.Wrap
                                font.bold: true
                                color: problem.modelData.severity === "error" ? ui.error : problem.modelData.severity === "warning" ? ui.warning : ui.information
                                text: (problem.modelData.severity === "error" ? qsTr("Ошибка: ") : problem.modelData.severity === "warning" ? qsTr("Внимание: ") : qsTr("Заметка: ")) + problem.modelData.title
                            }
                            Label {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                wrapMode: Text.Wrap
                                opacity: 0.8
                                text: problem.modelData.detail
                            }
                            Repeater {
                                model: problem.modelData.options || []
                                delegate: ColumnLayout {
                                    id: installOption
                                    required property var modelData
                                    Layout.fillWidth: true
                                    Layout.minimumWidth: 0
                                    spacing: 2
                                    Label {
                                        text: "• " + installOption.modelData.title
                                        font.bold: true
                                    }
                                    TextArea {
                                        objectName: "paddleCommands"
                                        Layout.fillWidth: true
                                        Layout.minimumWidth: 0
                                        visible: installOption.modelData.commands.length > 0
                                        readOnly: true
                                        selectByMouse: true
                                        wrapMode: Text.WrapAnywhere
                                        font.family: "monospace"
                                        font.pointSize: ui.description.pointSize
                                        text: installOption.modelData.commands.join("\n")
                                    }
                                    Button {
                    id: settingsActionUnique5
                    HoverHint { control: settingsActionUnique5; feature: settingsActionUnique5.text; explanation: qsTr("Копирует команду или сведения в буфер обмена. Команда автоматически не выполняется."); active: settingsActionUnique5.enabled }
                                        objectName: "paddleCopy"
                                        visible: installOption.modelData.commands.length > 0
                                        text: qsTr("Копировать команды")
                                        onClicked: win.controller.copyText(installOption.modelData.commands.join("\n"))
                                    }
                                }
                            }
                        }
                    }
                    Label {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        opacity: 0.85
                        font.pointSize: ui.description.pointSize
                        text: qsTr("Команды только показываются: LipaX ничего не устанавливает сам. Выполните их в терминале от своего пользователя (не root), затем укажите Python окружения выше; проверка запустится сама.")
                    }
                }
                FieldLabel {
                    helpText: win.rapidName === "MeikiOCR"
                        ? qsTr("Модели MeikiOCR (rtr46: детектор и распознаватель японского текста игр, лицензия LGPL-3.0) скачиваются только по кнопке «Скачать» с Hugging Face; размер и SHA-256 каждого файла проверяются. Во время распознавания сеть не используется.")
                        : qsTr("Модель PP-OCRv5 для основного языка распознавания. Модели скачиваются только по кнопке «Скачать» из репозитория RapidAI (ModelScope); размер и SHA-256 каждого файла проверяются. Во время распознавания сеть не используется.")
                    text: qsTr("Модель ") + win.rapidName
                    visible: win.usesRapid
                }
                ColumnLayout {
                    id: rapidCheck
                    objectName: "rapidCheck"
                    visible: win.usesRapid
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    spacing: 6
                    readonly property var selected: win.rapid.selected || null
                    readonly property bool working: win.controller.rapidBusy === true && !!selected && win.controller.rapidModel === selected.id
                    Label {
                        objectName: "rapidSummary"
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        font.bold: true
                        color: rapidCheck.working || win.rapid.ready === undefined ? palette.text : win.rapid.ready ? ui.success : ui.error
                        text: rapidCheck.working ? qsTr("Загрузка модели… ") + win.controller.rapidProgress + " %"
                            : win.rapid.ready === undefined ? qsTr("Состояние ") + win.rapidName + qsTr(" ещё не проверено")
                            : (win.rapid.ready ? "✓ " : "✗ ") + win.rapid.summary
                    }
                    Label {
                        objectName: "rapidModel"
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        visible: !!rapidCheck.selected
                        opacity: 0.85
                        text: rapidCheck.selected ? rapidCheck.selected.label + " · " + rapidCheck.selected.variant + " · " + win.megabytes(rapidCheck.selected.size) + " · "
                            + (rapidCheck.working ? qsTr("загрузка ") + win.controller.rapidProgress + " %" : rapidCheck.selected.installed ? qsTr("готова") : qsTr("не скачана")) : ""
                    }
                    ProgressBar {
                        objectName: "rapidProgress"
                        Layout.fillWidth: true
                        visible: rapidCheck.working
                        from: 0; to: 100
                        value: win.controller.rapidProgress || 0
                    }
                    Label {
                        objectName: "rapidLanguages"
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        visible: (win.rapid.ignored || []).length > 0
                        color: ui.warning
                        text: win.rapidName + qsTr(" читает только основной язык («") + (win.rapid.language || "") + "»); " + (win.rapid.ignored || []).join(", ") + qsTr(" распознаёт только Tesseract.")
                    }
                    Label {
                        objectName: "rapidError"
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        visible: (win.controller.rapidError || "").length > 0
                        color: ui.error
                        text: win.controller.rapidError || ""
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        Button {
                    id: settingsActionUnique6
                    HoverHint { control: settingsActionUnique6; feature: settingsActionUnique6.text; explanation: qsTr("Загружает выбранную модель OCR из каталога с проверкой целостности. После установки движок перечитывает модель без перезапуска LipaX."); active: settingsActionUnique6.enabled }
                            objectName: "rapidDownload"
                            visible: !!rapidCheck.selected && !rapidCheck.selected.installed
                            enabled: win.controller.rapidBusy !== true
                            text: qsTr("Скачать (") + win.megabytes(rapidCheck.selected ? rapidCheck.selected.size : 0) + ")"
                            ToolTip.visible: hovered
                            ToolTip.delay: 400
                            ToolTip.text: win.rapidName === "MeikiOCR" ? qsTr("Скачать модель MeikiOCR с Hugging Face с проверкой SHA-256") : qsTr("Скачать модель из репозитория RapidAI (ModelScope) с проверкой SHA-256")
                            onClicked: win.controller.downloadRapidModel(rapidCheck.selected.id)
                        }
                        Button {
                    id: settingsActionUnique7
                    HoverHint { control: settingsActionUnique7; feature: settingsActionUnique7.text; explanation: qsTr("Удаляет выбранную локальную модель после подтверждения. Для следующего использования потребуется загрузить её снова."); active: settingsActionUnique7.enabled }
                            objectName: "rapidDelete"
                            visible: !!rapidCheck.selected && rapidCheck.selected.installed
                            enabled: win.controller.rapidBusy !== true
                            text: qsTr("Удалить")
                            onClicked: win.askDeleteRapid(rapidCheck.selected)
                        }
                        Button {
                    id: settingsActionUnique8
                    HoverHint { control: settingsActionUnique8; feature: settingsActionUnique8.text; explanation: qsTr("Проверяет библиотеку ONNX Runtime и локальные модели выбранного OCR. Загрузка выполняется отдельной кнопкой."); active: settingsActionUnique8.enabled }
                            objectName: "rapidRecheck"
                            text: qsTr("Проверить снова")
                            enabled: win.controller.rapidBusy !== true
                            onClicked: win.controller.refreshRapid()
                        }
                    }
                    RowLayout {
                        objectName: "meikiOffer"
                        visible: !!win.rapid.optional
                        Layout.fillWidth: true
                        readonly property var model: win.rapid.optional || null
                        readonly property bool working: win.controller.rapidBusy === true && !!model && win.controller.rapidModel === model.id
                        Label {
                            objectName: "meikiOfferLabel"
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            wrapMode: Text.Wrap
                            opacity: 0.85
                            text: parent.model ? qsTr("MeikiOCR для японского (необязательно, лучше читает игры): ") + win.megabytes(parent.model.size) + " · "
                                + (parent.working ? qsTr("загрузка ") + win.controller.rapidProgress + " %" : parent.model.installed ? qsTr("готова, используется в автоматическом режиме") : qsTr("не скачана; без неё японский читает RapidOCR")) : ""
                        }
                        Button {
                    id: settingsActionUnique9
                    HoverHint { control: settingsActionUnique9; feature: settingsActionUnique9.text; explanation: qsTr("Загружает выбранную модель OCR из каталога с проверкой целостности. После установки движок перечитывает модель без перезапуска LipaX."); active: settingsActionUnique9.enabled }
                            objectName: "meikiDownload"
                            visible: !!parent.model && !parent.model.installed
                            enabled: win.controller.rapidBusy !== true
                            text: qsTr("Скачать")
                            ToolTip.visible: hovered
                            ToolTip.delay: 400
                            ToolTip.text: qsTr("Скачать модель MeikiOCR (LGPL-3.0) с Hugging Face с проверкой SHA-256")
                            onClicked: win.controller.downloadRapidModel(parent.model.id)
                        }
                        Button {
                    id: settingsActionUnique10
                    HoverHint { control: settingsActionUnique10; feature: settingsActionUnique10.text; explanation: qsTr("Удаляет выбранную локальную модель после подтверждения. Для следующего использования потребуется загрузить её снова."); active: settingsActionUnique10.enabled }
                            objectName: "meikiDelete"
                            visible: !!parent.model && parent.model.installed
                            enabled: win.controller.rapidBusy !== true
                            text: qsTr("Удалить")
                            onClicked: win.askDeleteRapid(parent.model)
                        }
                    }
                    Label {
                        objectName: "rapidLibrary"
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        visible: win.rapid.ready !== undefined
                        opacity: win.rapid.library ? 0.6 : 1
                        font.pointSize: ui.description.pointSize
                        color: win.rapid.library ? palette.text : ui.error
                        text: win.rapid.library ? "ONNX Runtime: " + win.rapid.library : qsTr("ONNX Runtime не найден. Установите его командой ниже и нажмите «Проверить снова».")
                    }
                    RowLayout {
                        visible: win.rapid.ready !== undefined && !win.rapid.library
                        Layout.fillWidth: true
                        TextField {
                            objectName: "rapidInstallCommand"
                            Layout.fillWidth: true
                            readOnly: true
                            selectByMouse: true
                            font.family: "monospace"
                            text: "sudo pacman -S onnxruntime-cpu"
                        }
                        Button {
                    id: settingsActionUnique11
                    HoverHint { control: settingsActionUnique11; feature: settingsActionUnique11.text; explanation: qsTr("Копирует команду или сведения в буфер обмена. Команда автоматически не выполняется."); active: settingsActionUnique11.enabled }
                            objectName: "rapidCopy"
                            text: qsTr("Копировать")
                            onClicked: win.controller.copyText("sudo pacman -S onnxruntime-cpu")
                        }
                    }
                    Repeater {
                        model: (win.rapid.models || []).filter(m => m.installed && (!rapidCheck.selected || m.id !== rapidCheck.selected.id) && m.id !== (win.rapid.optional || {}).id)
                        delegate: RowLayout {
                            id: otherModel
                            required property var modelData
                            objectName: "rapidOther_" + modelData.id
                            Layout.fillWidth: true
                            Label {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                elide: Text.ElideRight
                                opacity: 0.855
                                text: qsTr("Также скачана: ") + otherModel.modelData.label + " · " + otherModel.modelData.variant + " · " + win.megabytes(otherModel.modelData.size)
                            }
                            Button {
                    id: settingsActionUnique12
                    HoverHint { control: settingsActionUnique12; feature: settingsActionUnique12.text; explanation: qsTr("Удаляет выбранную локальную модель после подтверждения. Для следующего использования потребуется загрузить её снова."); active: settingsActionUnique12.enabled }
                                text: qsTr("Удалить")
                                enabled: win.controller.rapidBusy !== true
                                onClicked: win.askDeleteRapid(otherModel.modelData)
                            }
                        }
                    }
                }
                FieldLabel {
                    helpText: qsTr("Mobile — быстрые модели PP-OCRv5 (около 23 МБ для китайского и японского). Server — точнее на сложных кадрах, но около 180 МБ и в несколько раз медленнее на CPU. Есть только для китайского и японского.")
                    text: qsTr("Модели RapidOCR")
                    visible: win.usesRapid && win.rapid.server_available === true
                    helpControl: rapidVariantBox
                }
                ComboBox {
                    id: rapidVariantBox
                    objectName: "rapidVariantBox"
                    visible: win.usesRapid && win.rapid.server_available === true
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    readonly property var values: ["mobile", "server"]
                    model: [qsTr("Mobile — быстрее"), qsTr("Server — точнее, медленнее")]
                    currentIndex: Math.max(0, values.indexOf(win.current.recognition.rapid_variant || "mobile"))
                    onActivated: win.set("recognition.rapid_variant", values[currentIndex])
                }
                FieldLabel {
                    helpText: qsTr("Сколько потоков процессора занимает ONNX Runtime (RapidOCR и MeikiOCR). «Авто» — половина ядер, но не больше 4, чтобы игре хватало процессора. Больше потоков — быстрее распознавание и выше нагрузка.")
                    text: qsTr("Потоки ONNX Runtime")
                    visible: win.usesRapid
                    helpControl: rapidThreads
                }
                SpinBox {
                    id: rapidThreads
                    objectName: "rapidThreads"
                    visible: win.usesRapid
                    from: 0
                    to: 16
                    editable: true
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    value: win.current.recognition.rapid_threads || 0
                    textFromValue: (v, locale) => v === 0 ? qsTr("Авто") : String(v)
                    valueFromText: (text, locale) => /^\s*авто/i.test(text) ? 0 : Math.max(0, Math.min(16, parseInt(text) || 0))
                    onValueModified: win.set("recognition.rapid_threads", value)
                }
                Label {
                    objectName: "rapidThreadsAuto"
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.85
                    font.pointSize: ui.description.pointSize
                    visible: win.usesRapid && rapidThreads.value === 0 && !!win.rapid.threads
                    text: qsTr("Авто: ") + win.rapid.threads + " " + (win.rapid.threads === 1 ? qsTr("поток") : win.rapid.threads < 5 ? qsTr("потока") : qsTr("потоков")) + qsTr(" — половина ядер процессора, но не больше 4.")
                }
                FieldLabel {
                    helpText: qsTr("RapidOCR и MeikiOCR используют видеокарту, если установленная библиотека ONNX Runtime собрана с CUDA, MIGraphX или ROCm. Иначе распознавание идёт на процессоре, а в журнал пишется запись об этом.")
                    text: qsTr("Видеокарта для RapidOCR и MeikiOCR")
                    visible: win.usesRapid
                    helpControl: rapidGpu
                }
                CheckBox {
                    id: rapidGpu
                    objectName: "rapidGpu"
                    visible: win.usesRapid
                    text: qsTr("Использовать GPU, если доступен")
                    checked: win.current.recognition.rapid_use_gpu === true
                    onToggled: win.set("recognition.rapid_use_gpu", checked)
                }
                FieldLabel {
                    helpText: qsTr("Результаты ниже этого порога не переводятся. Значение 0 отключает проверку уверенности.")
                    text: qsTr("Минимальная уверенность распознавания, %")
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
                    opacity: 0.855
                    text: qsTr("Текст, в котором сам движок не уверен (мусор из-за фона, анимации, мелкого шрифта), не переводится и не показывается. ") + qsTr("0 — не проверять. Для японского, китайского и корейского порог автоматически не ниже 38 %: их мусор на текстурном фоне набирает 28–30 %. С RapidOCR порог не ниже 50 %: PP-OCR уверен в мусоре сильнее Tesseract. Работает с Tesseract, PaddleOCR и RapidOCR; причина отброшенного текста видна в «Просмотре OCR».")
                }
                Label {
                    text: qsTr("Фильтры изображения перед распознаванием")
                    font.bold: true
                    Layout.columnSpan: 2
                }
                FieldLabel {
                    helpText: qsTr("Если ни один фильтр ниже не включён вручную, LipaX смотрит на кадр: на чистом фоне текст читается как есть, а на шумном (градиент, текстура, анимация) автоматически включаются бинаризация Оцу с инверсией тёмного кадра. Если результат с автофильтром неуверенный или пустой, кадр читается ещё раз без фильтров и берётся лучший. Включённые вручную фильтры всегда главнее.")
                    text: qsTr("Подбирать фильтры по кадру автоматически")
                    helpControl: filterAuto
                }
                Switch {
                    id: filterAuto
                    objectName: "filterAuto"
                    checked: win.current.recognition.auto_filters !== false
                    onToggled: win.set("recognition.auto_filters", checked)
                }
                FieldLabel {
                    helpText: qsTr("Превращает кадр в чёрно-белый по порогу Оцу: фон становится белым, буквы чёрными. Помогает на пёстром фоне и мелком тексте, но может «съесть» тонкие или полупрозрачные буквы.")
                    text: qsTr("Бинаризация (Оцу)")
                    helpControl: filterBinarize
                }
                Switch {
                    id: filterBinarize
                    objectName: "filterBinarize"
                    checked: !!win.current.recognition.binarize
                    onToggled: win.set("recognition.binarize", checked)
                }
                FieldLabel {
                    helpText: qsTr("Если кадр в среднем тёмный, цвета переворачиваются: светлый текст на тёмном фоне становится тёмным на светлом, как любят движки. Для светлых интерфейсов ничего не меняет.")
                    text: qsTr("Авто-инверсия цветов")
                    helpControl: filterInvert
                }
                Switch {
                    id: filterInvert
                    objectName: "filterInvert"
                    checked: !!win.current.recognition.auto_invert
                    onToggled: win.set("recognition.auto_invert", checked)
                }
                FieldLabel {
                    helpText: qsTr("Усиливает разницу между текстом и фоном (0 — без изменений, положительные значения — сильнее, отрицательные — слабее). Полезно для бледного текста.")
                    text: qsTr("Контраст")
                    helpControl: filterContrast
                }
                SpinBox {
                    id: filterContrast
                    objectName: "filterContrast"
                    from: -100
                    to: 100
                    stepSize: 10
                    editable: true
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    value: win.current.recognition.contrast !== undefined ? win.current.recognition.contrast : 0
                    onValueModified: win.set("recognition.contrast", value)
                }
                FieldLabel {
                    helpText: qsTr("Подчёркивает края букв (нерезкая маска). Помогает на мелких и размытых шрифтах; на шумном изображении усиливает и шум.")
                    text: qsTr("Резкость")
                    helpControl: filterSharpen
                }
                Switch {
                    id: filterSharpen
                    objectName: "filterSharpen"
                    checked: !!win.current.recognition.sharpen
                    onToggled: win.set("recognition.sharpen", checked)
                }
                FieldLabel {
                    helpText: qsTr("Убирает из результата одиночные «~ | ° _ ^» и им подобные знаки, которые движок видит в текстурах фона, и строки, состоящие только из них.")
                    text: qsTr("Убирать мусорные символы")
                    helpControl: filterNoise
                }
                Switch {
                    id: filterNoise
                    objectName: "filterNoise"
                    checked: win.current.recognition.filter_noise !== false
                    onToggled: win.set("recognition.filter_noise", checked)
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.855
                    text: qsTr("Фильтры запоминаются для каждой игры отдельно (профиль игры): тёмному интерфейсу нужна инверсия, светлому — нет. ") + qsTr("Не знаете, что включить, — откройте «Просмотр OCR» и нажмите «Автоподбор»: LipaX сам попробует несколько наборов на текущем кадре.")
                }
                Label {
                    text: qsTr("Tesseract: состояние и языковые пакеты")
                    visible: win.usesTesseract
                    font.bold: true
                    Layout.columnSpan: 2
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    visible: win.usesTesseract && (!win.tess.distro || win.tess.installed)
                    text: !win.tess.distro ? qsTr("Проверка…") : qsTr("Установлен · ") + (win.tess.version ? qsTr("версия ") + win.tess.version : qsTr("версия неизвестна")) + qsTr("\nПуть: ") + win.tess.path + "\ntessdata: " + (win.tess.tessdata || qsTr("не определён")) + qsTr("\nСистема: ") + win.tess.distro.name + qsTr(" · менеджер пакетов: ") + win.tess.package_manager + (win.tess.engine_package ? qsTr("\nПакет: ") + win.tess.engine_package : "") + (win.tess.tessdata_package ? qsTr(" · данные: ") + win.tess.tessdata_package + qsTr(" и др.") : "")
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    visible: win.usesTesseract && !!win.tess.distro && !win.tess.installed
                    color: ui.error
                    text: qsTr("Tesseract не установлен") + (win.tess.engine_install_command ? qsTr(". Команда установки: ") + win.tess.engine_install_command : "")
                }
                FieldLabel {
                    helpText: qsTr("Язык исходного текста. Для выбранного движка должна быть установлена соответствующая языковая модель.")
                    text: qsTr("Язык распознавания")
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
                    text: qsTr("Дополнительные языки (по желанию)")
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
                    text: qsTr("Будет использовано: ") + (win.current.recognition.language || "eng")
                    opacity: 0.85
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
                            color: ui.error
                            text: modelData.message
                        }
                        Button {
                    HoverHint { control: unavailableControl7; feature: unavailableControl7.text; explanation: qsTr("Устанавливает выбранный язык Tesseract через системный менеджер пакетов с подтверждением. Может потребоваться пароль администратора."); active: unavailableControl7.enabled }
                            id: unavailableControl7
                            visible: win.installable.some(l => l.code === modelData.code)
                            text: qsTr("Установить язык")
                            enabled: !win.controller.tesseractBusy
                            onClicked: win.askInstall(modelData.code, modelData.name)

                            Accessible.description: unavailableHint7.accessibleExplanation
                            UnavailableHint {
                                id: unavailableHint7
                                control: unavailableControl7
                                feature: qsTr("Проверка и установка Tesseract")
                                reason: qsTr("Операция с Tesseract ещё выполняется.")
                                remedy: qsTr("Дождитесь завершения текущей операции.")
                            }
                        }
                    }
                }
                FieldLabel {
                    text: qsTr("Установить другой язык")
                    visible: win.usesTesseract
                    helpText: qsTr("Выберите отсутствующий языковой пакет Tesseract и нажмите «Установить». Потребуются сеть и разрешение системного менеджера пакетов.")
                }
                RowLayout {
                    visible: win.usesTesseract
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
                            feature: qsTr("Выбор языкового пакета")
                            reason: qsTr("Для этой системы нет доступных пакетов в списке установки.")
                            remedy: qsTr("Установите языковой пакет средствами системы и обновите список.")
                        }
                    }
                    Button {
                    HoverHint { control: unavailableControl9; feature: unavailableControl9.text; explanation: qsTr("Устанавливает выбранный язык Tesseract через системный менеджер пакетов с подтверждением. Может потребоваться пароль администратора."); active: unavailableControl9.enabled }
                        id: unavailableControl9
                        text: qsTr("Установить")
                        enabled: installBox.currentIndex >= 0 && win.installable.length > 0 && !win.controller.tesseractBusy
                        onClicked: win.askInstall(win.installable[installBox.currentIndex].code, win.installable[installBox.currentIndex].name)

                        Accessible.description: unavailableHint9.accessibleExplanation
                        UnavailableHint {
                            id: unavailableHint9
                            control: unavailableControl9
                            feature: qsTr("Установка языкового пакета")
                            reason: (win.controller.tesseractBusy ? qsTr("Установка уже выполняется.") : qsTr("Доступный языковой пакет не выбран."))
                            remedy: qsTr("Выберите доступный пакет или дождитесь завершения установки.")
                        }
                    }
                }
                Button {
                    HoverHint { control: unavailableControl6; feature: unavailableControl6.text; explanation: qsTr("Повторно проверяет наличие Tesseract и установленные языки без загрузки пакетов."); active: unavailableControl6.enabled }
                    id: unavailableControl6
                    Layout.columnSpan: 2
                    visible: win.usesTesseract
                    text: win.controller.tesseractBusy ? qsTr("Проверка…") : qsTr("Обновить список языков")
                    enabled: !win.controller.tesseractBusy
                    onClicked: win.controller.refreshTesseract()

                    Accessible.description: unavailableHint6.accessibleExplanation
                    UnavailableHint {
                        id: unavailableHint6
                        control: unavailableControl6
                        feature: qsTr("Проверка и установка Tesseract")
                        reason: qsTr("Операция с Tesseract ещё выполняется.")
                        remedy: qsTr("Дождитесь завершения текущей операции.")
                    }
                }
                FieldLabel {
                    helpText: qsTr("Пауза между проверками изображения на изменение текста.")
                    text: qsTr("Интервал распознавания, мс")
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
                    text: qsTr("Чувствительность обнаружения текста (ниже — чувствительнее)")
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
                    helpText: qsTr("После изменения изображения приложение ждёт стабилизации текста перед распознаванием.")
                    text: qsTr("Задержка перед распознаванием, мс")
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
                    text: qsTr("Сбросить настройки распознавания")
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
                columnSpacing: ui.margin
                rowSpacing: ui.gap
                FieldLabel {
                    helpText: qsTr("По умолчанию берётся из языка распознавания активной области захвата. Можно выбрать отдельный язык для сервиса перевода.")
                    text: qsTr("Исходный язык")
                    helpControl: sourceLanguageHelpTarget1
                }
                ComboBox {
                    id: sourceLanguageHelpTarget1
                    objectName: "translationSourceLanguage"
                    Layout.fillWidth: true
                    // The last entry is "auto": the language is guessed from the script of the text (Bergamot) or left to the service.
                    readonly property int autoIndex: win.targets.length + 1
                    model: [qsTr("Из языка распознавания")].concat(win.targets).concat([qsTr("Определять по тексту")])
                    currentIndex: {
                        const source = win.current.translation.source_language
                        if (!source || source.mode !== "explicit") return 0
                        if (source.language === "auto") return autoIndex
                        return Math.max(0, win.targets.indexOf(source.language) + 1)
                    }
                    onActivated: win.set("translation.source_language", currentIndex === 0 ? {
                        mode: "recognition_language"
                    } : {
                        mode: "explicit",
                        language: currentIndex === autoIndex ? "auto" : win.targets[currentIndex - 1]
                    })
                }
                FieldLabel {
                    helpText: qsTr("Язык, на котором будет показан результат перевода.")
                    text: qsTr("Язык перевода")
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
                    helpText: win.providerDescription + qsTr(" Изменение переинициализирует переводчик без перезапуска приложения. Ключи остальных сервисов сохраняются.")
                    text: qsTr("Сервис перевода")
                    helpControl: tr
                }
                ComboBox {
                    id: tr
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    model: ["Google Translate", "Yandex Translate", qsTr("Свой API"), "DeepL", "Microsoft Translator", qsTr("Bergamot (локально)")]
                    currentIndex: Math.max(0, win.translators.indexOf(win.current.translation.service))
                    onActivated: win.set("translation.service", win.translators[currentIndex])
                }
                FieldLabel {
                    helpText: qsTr("Ключи сервисов перевода хранятся в кошельке рабочего стола (KWallet, GNOME Keyring), а не в config.toml. Ключ переносится при следующем сохранении настроек. Если кошелёк недоступен или заблокирован, ключи остаются в config.toml.")
                    text: qsTr("Хранение API-ключей")
                    helpControl: keysInWallet
                }
                CheckBox {
                    id: keysInWallet
                    objectName: "keysInWallet"
                    text: qsTr("Хранить API-ключи в кошельке")
                    checked: win.general.keys_in_wallet === true
                    onToggled: win.set("general.keys_in_wallet", checked)
                }
                Label {
                    objectName: "providerDescription"
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: win.providerDescription
                }
                FieldLabel {
                    helpText: qsTr("Ключ доступа к Yandex Cloud Translate. Без ключа запрос не выполняется. Сохранённый ключ скрыт и не удаляется при выборе другого сервиса.")
                    text: qsTr("Yandex API-ключ")
                    visible: tr.currentIndex === 1
                    helpControl: fieldHelpEditor1
                }
                TextField {
                    id: fieldHelpEditor1
                    visible: tr.currentIndex === 1
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    echoMode: TextInput.Password
                    text: win.current.translation.yandex_api_key || ""
                    onEditingFinished: win.set("translation.yandex_api_key", text)
                }
                FieldLabel {
                    helpText: qsTr("Идентификатор облачного каталога Yandex, передаваемый вместе с API-ключом. Изменение используется в следующих запросах.")
                    text: "Yandex folder ID"
                    visible: tr.currentIndex === 1
                    helpControl: fieldHelpEditor2
                }
                TextField {
                    id: fieldHelpEditor2
                    visible: tr.currentIndex === 1
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    text: win.current.translation.yandex_folder_id || ""
                    onEditingFinished: win.set("translation.yandex_folder_id", text)
                }
                FieldLabel {
                    helpText: qsTr("Адрес POST /translate сервера LibreTranslate или совместимого API. Текст отправляется именно на этот сервер. Пустой адрес блокирует запрос.")
                    text: qsTr("URL API (LibreTranslate-формат)")
                    visible: tr.currentIndex === 2
                    helpControl: fieldHelpEditor3
                }
                TextField {
                    id: fieldHelpEditor3
                    visible: tr.currentIndex === 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    placeholderText: "https://host/translate"
                    text: win.current.translation.custom_url || ""
                    onEditingFinished: win.set("translation.custom_url", text)
                }
                FieldLabel {
                    helpText: qsTr("Передаётся в поле api_key совместимого API. Оставьте пустым, если сервер не требует авторизации.")
                    text: qsTr("Ключ API (необязательно)")
                    visible: tr.currentIndex === 2
                    helpControl: fieldHelpEditor4
                }
                TextField {
                    id: fieldHelpEditor4
                    visible: tr.currentIndex === 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    echoMode: TextInput.Password
                    text: win.current.translation.custom_api_key || ""
                    onEditingFinished: win.set("translation.custom_api_key", text)
                }
                FieldLabel {
                    helpText: qsTr("Ключ DeepL API; пустое поле использует DEEPL_API_KEY. При отсутствии обоих ключей перевод не запускается.")
                    text: qsTr("DeepL API-ключ")
                    visible: tr.currentIndex === 3
                    helpControl: fieldHelpEditor5
                }
                TextField {
                    id: fieldHelpEditor5
                    visible: tr.currentIndex === 3
                    Layout.fillWidth: true
                    echoMode: TextInput.Password
                    placeholderText: qsTr("Можно задать DEEPL_API_KEY")
                    text: win.current.translation.deepl_api_key || ""
                    onEditingFinished: win.set("translation.deepl_api_key", text)
                }
                FieldLabel {
                    helpText: qsTr("Ключ Azure Translator; пустое поле использует MICROSOFT_TRANSLATOR_API_KEY. Ключи других провайдеров сохраняются.")
                    text: qsTr("Microsoft Translator API-ключ")
                    visible: tr.currentIndex === 4
                    helpControl: fieldHelpEditor6
                }
                TextField {
                    id: fieldHelpEditor6
                    visible: tr.currentIndex === 4
                    Layout.fillWidth: true
                    echoMode: TextInput.Password
                    placeholderText: qsTr("Можно задать MICROSOFT_TRANSLATOR_API_KEY")
                    text: win.current.translation.microsoft_api_key || ""
                    onEditingFinished: win.set("translation.microsoft_api_key", text)
                }
                FieldLabel {
                    helpText: qsTr("Регион ресурса Azure, например westeurope. Пустое поле использует MICROSOFT_TRANSLATOR_REGION; глобальный ресурс может не требовать региона.")
                    text: qsTr("Регион Azure (для регионального ресурса)")
                    visible: tr.currentIndex === 4
                    helpControl: fieldHelpEditor7
                }
                TextField {
                    id: fieldHelpEditor7
                    visible: tr.currentIndex === 4
                    Layout.fillWidth: true
                    placeholderText: qsTr("Можно задать MICROSOFT_TRANSLATOR_REGION")
                    text: win.current.translation.microsoft_region || ""
                    onEditingFinished: win.set("translation.microsoft_region", text)
                }
                FieldLabel {
                    text: qsTr("Bergamot: путь к CLI")
                    visible: tr.currentIndex === 5
                    helpText: qsTr("Нативный исполняемый файл bergamot. Пустое поле использует движок из пакета LipaX; также поддерживаются BERGAMOT_BINARY и PATH.")
                    helpControl: fieldHelpEditor8
                }
                TextField {
                    id: fieldHelpEditor8
                    visible: tr.currentIndex === 5
                    Layout.fillWidth: true
                    placeholderText: "bergamot"
                    text: win.current.translation.bergamot_binary || ""
                    onEditingFinished: win.set("translation.bergamot_binary", text)
                }
                FieldLabel {
                    text: qsTr("Bergamot: модель")
                    visible: tr.currentIndex === 5
                    helpText: qsTr("Автоматический поиск: настройки → кэш LipaX → Firefox → загрузка Mozilla. Размеры и SHA-256 проверяются перед установкой.")
                }
                ColumnLayout {
                    visible: tr.currentIndex === 5
                    Layout.fillWidth: true
                    Label {
                        objectName: "bergamotStatus"
                        Layout.fillWidth: true
                        text: (win.bergamotModel.pair || "") + " — " + (win.bergamotModel.status || "Not Found")
                        wrapMode: Text.Wrap
                    }
                    ProgressBar {
                        objectName: "bergamotProgress"
                        Layout.fillWidth: true
                        visible: win.controller.modelBusy === true
                        indeterminate: (win.bergamotModel.progress ?? -1) < 0
                        from: 0; to: 100
                        value: win.bergamotModel.progress || 0
                    }
                    Label {
                        Layout.fillWidth: true
                        visible: !!win.bergamotModel.error
                        text: win.bergamotModel.error || ""
                        wrapMode: Text.Wrap
                    }
                    Label {
                        Layout.fillWidth: true
                        visible: !!win.bergamotModel.path
                        text: win.bergamotModel.path || ""
                        elide: Text.ElideMiddle
                    }
                    Label {
                        objectName: "bergamotVersion"
                        Layout.fillWidth: true
                        visible: !!win.modelVersions.newest
                        wrapMode: Text.Wrap
                        text: !win.modelVersions.installed ? qsTr("Версия модели: не установлена")
                            : win.modelVersions.update ? qsTr("Установлена версия ") + win.modelVersions.installed + qsTr(" · доступна новая ") + win.modelVersions.newest
                            : qsTr("Установлена версия ") + win.modelVersions.installed + qsTr(" — актуальная")
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        visible: (win.modelVersions.versions || []).length > 1
                        ComboBox {
                            id: versionBox
                            objectName: "bergamotVersionBox"
                            Layout.fillWidth: true
                            textRole: "text"
                            valueRole: "value"
                            enabled: win.controller.modelBusy !== true
                            model: (win.modelVersions.versions || []).map(function (v) { return ({value: v.version, text: win.modelVersionLabel(v), installed: v.installed}) })
                            onModelChanged: {
                                const at = model.findIndex(function (v) { return v.installed })
                                currentIndex = at >= 0 ? at : 0
                            }
                        }
                        Button {
                    id: settingsActionUnique13
                    HoverHint { control: settingsActionUnique13; feature: settingsActionUnique13.text; explanation: qsTr("Загружает выбранную версию модели Bergamot для этой языковой пары. Размер и контрольная сумма проверяются перед установкой."); active: settingsActionUnique13.enabled }
                            objectName: "bergamotInstallVersion"
                            text: qsTr("Скачать выбранную версию")
                            enabled: win.controller.modelBusy !== true && versionBox.currentIndex >= 0
                                && !(versionBox.model[versionBox.currentIndex] || {installed: true}).installed
                            onClicked: { win.apply(); win.controller.downloadModelVersion(win.modelVersions.pair, versionBox.currentValue) }
                        }
                    }
                    Button {
                    id: settingsActionUnique14
                    HoverHint { control: settingsActionUnique14; feature: settingsActionUnique14.text; explanation: qsTr("Загружает выбранную версию модели Bergamot для этой языковой пары. Размер и контрольная сумма проверяются перед установкой."); active: settingsActionUnique14.enabled }
                        objectName: "bergamotUpdate"
                        visible: win.modelVersions.update === true
                        text: qsTr("Обновить до ") + win.modelVersions.newest
                        enabled: win.controller.modelBusy !== true
                        onClicked: { win.apply(); win.controller.downloadModelVersion(win.modelVersions.pair, win.modelVersions.newest) }
                    }
                    RowLayout {
                        Button {
                    id: settingsActionUnique15
                    HoverHint { control: settingsActionUnique15; feature: settingsActionUnique15.text; explanation: qsTr("Повторно ищет модель Bergamot для текущей языковой пары и обновляет сведения о её наличии."); active: settingsActionUnique15.enabled }
                            objectName: "bergamotDownload"
                            text: qsTr("Найти / скачать модель")
                            enabled: win.controller.modelBusy !== true
                            onClicked: { win.apply(); win.controller.refreshModels() }
                        }
                        Button {
                    id: settingsActionUnique16
                    HoverHint { control: settingsActionUnique16; feature: settingsActionUnique16.text; explanation: qsTr("Открывает поле пути к локальной модели Bergamot. Укажите конфигурацию для выбранной языковой пары."); active: settingsActionUnique16.enabled }
                            objectName: "bergamotManual"
                            text: qsTr("Указать путь к модели")
                            onClicked: modelPathRow.visible = !modelPathRow.visible
                        }
                    }
                    RowLayout {
                        id: modelPathRow
                        visible: false
                        Layout.fillWidth: true
                        TextField {
                            id: modelPath
                            objectName: "bergamotPath"
                            Layout.fillWidth: true
                            placeholderText: qsTr("/путь/к/модели или en-ru.yml")
                        }
                        Button {
                    id: settingsActionUnique17
                    HoverHint { control: settingsActionUnique17; feature: settingsActionUnique17.text; explanation: qsTr("Использует указанный локальный путь к конфигурации модели Bergamot. Другие языковые пары сохраняются."); active: settingsActionUnique17.enabled }
                            text: qsTr("Применить")
                            onClicked: { win.apply(); win.controller.setModelPath(modelPath.text) }
                        }
                    }
                }
                FieldLabel {
                    text: qsTr("Автоматический перевод")
                    helpControl: fieldHelpEditor9
                    helpText: qsTr("Передаёт распознанный текст выбранному сервису автоматически. Отключение сохраняет распознавание, но не запрашивает новые переводы.")
                }
                Switch {
                    id: fieldHelpEditor9
                    checked: win.current.translation.auto_translate !== false
                    onToggled: win.set("translation.auto_translate", checked)
                }
                FieldLabel {
                    helpText: qsTr("Окно перевода делит распознанный текст на абзацы и отправляет в переводчик только те, которых ещё не было: неизменные заголовки и кнопки берутся из кэша. Меньше запросов, но абзацы переводятся порознь, без общего контекста.")
                    text: qsTr("Переводить только изменения")
                    helpControl: changesOnlySwitch
                }
                Switch {
                    id: changesOnlySwitch
                    objectName: "changesOnly"
                    checked: !!win.current.translation.changes_only
                    onToggled: win.set("translation.changes_only", checked)
                }
                ResetButton {
                    text: qsTr("Сбросить настройки перевода")
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
                columnSpacing: ui.margin
                rowSpacing: ui.gap
                SectionTitle {
                    text: qsTr("Способ отображения")
                }
                FieldLabel {
                    helpText: qsTr("Перевод поверх исходного текста требует точных координат окна. Отдельное окно перевода можно разместить независимо от текста игры.")
                    text: qsTr("Где показывать перевод")
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    RadioButton {
                        id: windowChoice
                        objectName: "displayWindow"
                        HoverHint {
                            control: windowChoice
                            feature: qsTr("Отдельное окно перевода")
                            objectName: "windowDisplayHint"
                            explanation: feature + "\n\n" + win.windowDisplayDescription
                        }
                        text: qsTr("В отдельном окне")
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
                        text: qsTr("Поверх исходного текста")
                        checked: win.current.display_mode === "inplace"
                        enabled: win.inplaceAvailability.available
                        HoverHint {
                            control: inplaceChoice
                            feature: qsTr("Поверх исходного текста")
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
                            feature: qsTr("In-place Translation — перевод поверх исходного текста")
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
                    opacity: 0.85
                    visible: win.current.display_mode === "inplace"
                    text: qsTr("Перевод закрывает исходный текст в каждой активной области: на ровном фоне — заливка цвета фона, на сложном — перерисовка фона только под буквами; ") + qsTr("цвет, размер и начертание оцениваются по кадру; длинный перевод уменьшается. ") + qsTr("Свой шрифт можно выбрать на вкладке «Оформление перевода». Средняя кнопка мыши на переводе скрывает его (если не включено «Пропускать клики сквозь перевод»); вернуть его можно переключателем «Показывать поверх исходного текста».")
                }
                Label {
                    objectName: "inplaceBlockerNote"
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    color: ui.warning
                    visible: win.current.display_mode === "inplace" && win.inplaceBlocker.length > 0
                    text: qsTr("Для этого окна недоступно: ") + win.inplaceBlocker + "."
                }
                FieldLabel {
                    helpText: qsTr("Перевод (поверх оригинала и окно перевода) показывается, только пока окно игры видно и в фокусе. Если окно свёрнуто, закрыто, на другом рабочем столе или поверх него другая программа, перевод скрывается, а распознавание приостанавливается; всё возвращается, когда вы вернётесь в игру. Окна самого LipaX игру «активной» не лишают. Работает для окон, выбранных через KWin; для захвата через портал данных об окне нет.")
                    text: qsTr("Показывать перевод только при активном окне игры")
                    helpControl: onlyWhenActive
                }
                Switch {
                    id: onlyWhenActive
                    objectName: "onlyWhenActive"
                    checked: !!win.inplace().only_when_active
                    onToggled: win.setInplace("only_when_active", checked)
                }
                Label {
                    objectName: "onlyWhenActiveLinkDisplay"
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.855
                    textFormat: Text.StyledText
                    linkColor: palette.link
                    text: qsTr("Та же настройка есть на вкладке <a href=\"appearance\">«Оформление перевода»</a>: там она рядом с остальными параметрами перевода поверх оригинала.")
                    onLinkActivated: tabs.currentIndex = 5
                }
                ResetButton {
                    text: qsTr("Сбросить способ отображения")
                    keys: ["display_mode"]
                }
            }
        }
        ScrollView {
            id: page4
            objectName: "settingsPage4"
            contentWidth: availableWidth
            clip: true
            // The window is used only in the «separate window» mode; in the in-place mode its settings have no effect.
            enabled: win.current.display_mode !== "inplace"
            opacity: enabled ? 1 : 0.45
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            GridLayout {
                width: page4.availableWidth - 16
                columns: 2
                columnSpacing: ui.margin
                rowSpacing: ui.gap
                Label {
                    objectName: "windowSettingsDisabledNote"
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    visible: win.current.display_mode === "inplace"
                    color: ui.warning
                    text: qsTr("Эти настройки не действуют, пока перевод показывается поверх исходного текста. Чтобы включить их, выберите «В отдельном окне» на вкладке «Отображение перевода».")
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: qsTr("Прозрачность: 100 % — непрозрачно, 0 % — полностью прозрачно.")
                }
                FieldLabel {
                    helpText: qsTr("Экран для размещения окна перевода. Для portal его нужно указать вручную.")
                    text: qsTr("Экран перевода")
                    helpControl: translationScreenHelpTarget1
                }
                ComboBox {
                    id: translationScreenHelpTarget1
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    readonly property var screens: Qt.application.screens
                    model: [qsTr("Автоматически: экран игры")].concat(screens.map(s => s.name))
                    currentIndex: Math.max(0, screens.findIndex(s => s.name === win.current.translation_window.screen) + 1)
                    onActivated: win.set("translation_window.screen", currentIndex === 0 ? "" : screens[currentIndex - 1].name)
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.855
                    text: qsTr("В KWin перевод следует за окном игры. Для portal выберите экран вручную: положение окна скрыто порталом. Координаты ниже считаются от угла этого экрана.")
                }
                FieldLabel {
                    helpText: qsTr("Закреплённое окно сохраняет положение и может пропускать клики в игру. Свободное окно можно перетаскивать мышью.")
                    text: qsTr("Закрепить окно")
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
                    helpText: qsTr("Радиус углов фона закреплённого окна перевода.")
                    text: qsTr("Скругление углов закреплённого окна, px")
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
                    opacity: 0.855
                    text: qsTr("Средняя кнопка мыши по рамке перевода переключает закрепление. Свободное окно перевода перетаскивается левой кнопкой, его рамка толще. ") + qsTr("Закреплённый пропускает клики в игру (если включено ниже); рамка и значок в углу остаются доступны для средней кнопки.")
                }
                FieldLabel {
                    helpText: qsTr("Клики по содержимому закреплённого окна перевода передаются в игру.")
                    text: qsTr("Пропускать клики мыши")
                    helpControl: windowClickThroughHelpTarget1
                }
                Switch {
                    id: windowClickThroughHelpTarget1
                    checked: win.current.translation_window.click_through !== false
                    onToggled: win.set("translation_window.click_through", checked)
                }
                FieldLabel {
                    helpText: qsTr("Координаты окна перевода относительно выбранного экрана.")
                    text: qsTr("Положение окна перевода (x, y)")
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
                    helpText: qsTr("Ширина и высота отдельного окна перевода.")
                    text: qsTr("Размер окна перевода (ш, в)")
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
                    text: qsTr("Автоматический размер текста (уменьшать, если не помещается)")
                    helpText: qsTr("Уменьшает текст до читаемого минимума, если перевод не помещается в окно. Остаток доступен прокруткой свободного окна. Пример обновляется сразу.")
                }
                Switch {
                    objectName: "translationWindowAutoShrink"
                    checked: win.current.translation_window.auto_shrink !== false
                    onToggled: win.set("translation_window.auto_shrink", checked)
                }
                FieldLabel {
                    helpText: qsTr("100 % — непрозрачно, 0 % — полностью прозрачно. Изменяет только фон.")
                    text: qsTr("Прозрачность фона")
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
                    text: qsTr("Цвет рамки")
                    helpText: qsTr("Цвет видимой границы окна перевода. Используйте системный выбор цвета или шестнадцатеричный код. Пример обновляется сразу.")
                }
                ColorRow {
                    key: "translation_window.border_color"
                    presets: ["#ff00ff", "#ff2a6d", "#00e5ff", "#ffd400"]
                }
                FieldLabel {
                    text: qsTr("Показывать рамку")
                    helpText: qsTr("Выбирает, когда видна рамка: постоянно, временно после выбора или никогда. Изменение применяется сразу.")
                }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    ComboBox {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        model: [qsTr("Всегда"), qsTr("После выделения области")]
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
                        text: qsTr("с")
                    }
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.85
                    visible: win.current.translation_window.border_always === false
                    text: qsTr("Рамка появляется на заданное время после выделения или изменения области. Свободное окно перевода показывает рамку всегда.")
                }
                FieldLabel {
                    text: qsTr("Узор «текстура ошибки» (цвет/чёрный)")
                    helpText: qsTr("Рисует двухцветный узор рамки вместо сплошной границы. Это оформление, а не сообщение об ошибке OCR.")
                }
                Switch {
                    checked: win.current.translation_window.border_pattern === true
                    onToggled: win.set("translation_window.border_pattern", checked)
                }
                FieldLabel {
                    helpText: qsTr("100 % — непрозрачно, 0 % — полностью прозрачно. Изменяет только рамку окна перевода.")
                    text: qsTr("Прозрачность рамки")
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
                    helpText: qsTr("Толщина видимой границы закреплённого окна. Для свободного окна добавляются 2 логических пикселя. Применяется сразу и видна в примере.")
                    text: qsTr("Толщина рамки закреплённого окна, px")
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
                    opacity: 0.85
                    text: qsTr("Свободное окно перевода показывается с рамкой на 2 px толще: так видно, что его можно перетаскивать.")
                }
                FieldLabel {
                    helpText: qsTr("Радиус углов фона свободного окна перевода.")
                    text: qsTr("Скругление углов свободного окна, px")
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
                    text: qsTr("Максимальная ширина, px")
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
                        feature: qsTr("Максимальная ширина окна перевода")
                        reason: qsTr("Ограничение ширины выключено.")
                        remedy: qsTr("Включите «Максимальная ширина».")
                    }
                }
                ResetButton {
                    text: qsTr("Сбросить настройки окна перевода")
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
                columnSpacing: ui.margin
                rowSpacing: ui.gap
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: qsTr("Прозрачность: 100 % — непрозрачно, 0 % — полностью прозрачно.")
                }
                SectionTitle {
                    objectName: "appearanceModeTitle"
                    text: win.current.display_mode === "inplace" ? qsTr("Поверх исходного текста") : qsTr("Отдельное окно перевода")
                }
                GridLayout {
                    objectName: "inplaceSettings"
                    visible: win.current.display_mode === "inplace"
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    columns: 2
                    columnSpacing: ui.margin
                    rowSpacing: 10
                    Label {
                        Layout.columnSpan: 2
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        opacity: 0.85
                        text: qsTr("Каждое поле текста находится и отслеживается отдельно. Шрифт для поля выбирается один раз — ") + qsTr("по признакам начертания оригинала и среди шрифтов приложения с глифами языка перевода — и дальше не меняется. ") + qsTr("Каждое свойство ниже можно оставить автоматическим или задать вручную независимо от остальных.")
                    }
                    FieldLabel {
                        text: qsTr("Делить блок при шаге строк больше")
                        helpText: qsTr("Порог разделения блока по расстоянию между строками относительно высоты букв. Меняет детектор и перечитывает область; перезапуск LipaX не нужен.")
                    }
                    SpinBox {
                        objectName: "lineGapFactor"
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        from: 13
                        to: 40
                        value: Math.round((win.inplace().line_gap_factor || 1.8) * 10)
                        textFromValue: v => (v / 10).toFixed(1) + qsTr(" × высоты строки")
                        valueFromText: t => Math.round(parseFloat(String(t).replace(",", ".")) * 10)
                        editable: true
                        ToolTip.visible: hovered
                        ToolTip.delay: 400
                        ToolTip.text: qsTr("Если расстояние между соседними строками (от центра до центра) больше этого числа высот строки, ")
                            + qsTr("они считаются разными полями: склеенные имя персонажа и реплика, пункты меню. Обычный абзац — 1,2–1,5. ")
                            + qsTr("Меньше — делится чаще, больше — блоки крупнее.")
                        onValueModified: win.setInplace("line_gap_factor", value / 10)
                    }
                    Label {
                        Layout.columnSpan: 2
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        opacity: 0.85
                        text: qsTr("По умолчанию 1,8: абзац остаётся одним полем, а поля, разделённые пустой строкой (имя над репликой), — разными.")
                    }
                    FieldLabel {
                        helpText: qsTr("Перевод (поверх оригинала и окно перевода) показывается, только пока окно игры видно и в фокусе. Если окно свёрнуто, закрыто, на другом рабочем столе или поверх него другая программа, перевод скрывается, а распознавание приостанавливается; всё возвращается, когда вы вернётесь в игру. Окна самого LipaX игру «активной» не лишают. Работает для окон, выбранных через KWin; для захвата через портал данных об окне нет.")
                        text: qsTr("Показывать перевод только при активном окне игры")
                        helpControl: onlyWhenActiveAppearance
                    }
                    Switch {
                        id: onlyWhenActiveAppearance
                        objectName: "onlyWhenActiveAppearance"
                        checked: !!win.inplace().only_when_active
                        onToggled: win.setInplace("only_when_active", checked)
                    }
                    Label {
                        objectName: "onlyWhenActiveLinkAppearance"
                        Layout.columnSpan: 2
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        opacity: 0.855
                        textFormat: Text.StyledText
                        linkColor: palette.link
                        text: qsTr("Та же настройка есть на вкладке <a href=\"display\">«Отображение перевода»</a>: она действует и для окна перевода.")
                        onLinkActivated: tabs.currentIndex = 3
                    }
                    FieldLabel {
                        helpText: qsTr("Обычно поле перевода принимает мышь: средняя кнопка на нём скрывает наложение. Если включить, поле «прозрачно» для мыши: клики проходят в игру (например, по кнопке меню под переводом), а скрыть перевод можно горячей клавишей или переключателем в главном окне.")
                        text: qsTr("Пропускать клики сквозь перевод")
                        helpControl: clickThroughSwitch
                    }
                    Switch {
                        id: clickThroughSwitch
                        objectName: "clickThrough"
                        checked: !!win.inplace().click_through
                        onToggled: win.setInplace("click_through", checked)
                    }
                    FieldLabel {
                        helpText: qsTr("Кнопки и пункты меню самой игры («SAVE», «LOAD», «Options») остаются как есть: их не читают и не переводят, поверх них ничего не рисуется. По умолчанию выключено: варианты ответа в диалоге похожи на кнопки, и их обычно нужно переводить.")
                        text: qsTr("Не переводить кнопки и пункты меню")
                        helpControl: skipControlsSwitch
                    }
                    Switch {
                        id: skipControlsSwitch
                        objectName: "skipControls"
                        checked: !!win.inplace().skip_controls
                        onToggled: win.setInplace("skip_controls", checked)
                    }
                    FieldLabel {
                        helpText: qsTr("Имя говорящего над репликой остаётся как есть: его не читают и не переводят, поверх него ничего не рисуется. По умолчанию выключено: имя часто нужно перевести или записать по-русски. Включите, если крупное имя из игры остаётся под мелким переводом или обрезано краем области и читается как бессмыслица.")
                        text: qsTr("Не переводить имена персонажей")
                        helpControl: skipNamesSwitch
                    }
                    Switch {
                        id: skipNamesSwitch
                        objectName: "skipNames"
                        checked: !!win.inplace().skip_names
                        onToggled: win.setInplace("skip_names", checked)
                    }
                    FieldLabel {
                        helpText: qsTr("Если верхний край области обрезает текст (обычно имя говорящего над репликой), область читается выше на высоту этой строки, и перевод встаёт на место целого имени. Текст, который целиком лежит в добавленной полосе, не читается. Выключите, чтобы захватывалась строго нарисованная область.")
                        text: qsTr("Дочитывать текст, обрезанный верхним краем")
                        helpControl: extendUpSwitch
                    }
                    Switch {
                        id: extendUpSwitch
                        objectName: "extendUp"
                        checked: win.inplace().extend_up !== false
                        onToggled: win.setInplace("extend_up", checked)
                    }
                    FieldLabel {
                        text: qsTr("Фон под переводом")
                        helpText: qsTr("Выбирает способ закрыть исходный текст: восстановление фона, заливка или прозрачный фон с обводкой. Авто учитывает изображение игры; пример использует условную однотонную сцену.")
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
                        helpText: qsTr("Цвет обводки букв перевода.")
                        text: qsTr("Цвет контура текста")
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
                        helpText: qsTr("Толщина обводки букв перевода в пикселях.")
                        text: qsTr("Толщина контура текста, px")
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
                        helpText: qsTr("Тень букв перевода помогает отделить текст от фона.")
                        text: qsTr("Тень текста")
                        visible: win.inplace().background_mode === "transparent_outline"
                    }
                    CheckBox {
                        visible: win.inplace().background_mode === "transparent_outline"
                        checked: win.inplace().shadow === true
                        onToggled: win.setInplace("shadow", checked)
                    }
                    FieldLabel {
                        helpText: qsTr("100 % — непрозрачно, 0 % — полностью прозрачно. Изменяет только текст перевода.")
                        text: qsTr("Прозрачность текста, %")
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
                        text: qsTr("Цвет заливки")
                        visible: win.inplace().background_mode === "padded_fill"
                        helpText: qsTr("В режиме «Авто» цвет берётся из изображения вокруг текста. Ручной цвет задаёт фон поля; прозрачность регулируется отдельно.")
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
                        helpText: qsTr("100 % — непрозрачно, 0 % — полностью прозрачно. Изменяет только фон под переводом.")
                        text: qsTr("Прозрачность фона, %")
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
                        text: qsTr("Внутренние отступы по горизонтали, px")
                        visible: win.inplace().background_mode === "padded_fill"
                        helpText: qsTr("Дополнительные поля слева и справа в режиме заливки с полями. Применяются к оформлению без повторного OCR.")
                    }
                    SpinBox {
                        visible: win.inplace().background_mode === "padded_fill"
                        from: 0
                        to: 64
                        value: win.inplace().padding_x || 0
                        onValueModified: win.setInplace("padding_x", value)
                    }
                    FieldLabel {
                        text: qsTr("Внутренние отступы по вертикали, px")
                        visible: win.inplace().background_mode === "padded_fill"
                        helpText: qsTr("Дополнительные поля сверху и снизу в режиме заливки с полями. Пример показывает изменение сразу.")
                    }
                    SpinBox {
                        visible: win.inplace().background_mode === "padded_fill"
                        from: 0
                        to: 64
                        value: win.inplace().padding_y || 0
                        onValueModified: win.setInplace("padding_y", value)
                    }
                    FieldLabel {
                        helpText: qsTr("Отступы снаружи блока перевода.")
                        text: qsTr("Внешние отступы, px")
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
                        text: qsTr("Скругление углов, px")
                        visible: win.inplace().background_mode === "padded_fill"
                        helpText: qsTr("Радиус углов подложки перевода. Ноль даёт прямые углы. Не меняет область захвата.")
                    }
                    SpinBox {
                        visible: win.inplace().background_mode === "padded_fill"
                        from: 0
                        to: 64
                        value: win.inplace().corner_radius === undefined ? 4 : win.inplace().corner_radius
                        onValueModified: win.setInplace("corner_radius", value)
                    }
                    FieldLabel {
                        helpText: qsTr("Шрифт результата перевода. Список содержит шрифты из комплекта приложения.")
                        text: qsTr("Шрифт перевода")
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
                        helpText: qsTr("Размер текста перевода в пикселях. Автоматический режим подбирает его по исходному тексту.")
                        text: qsTr("Размер текста, px")
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
                        text: qsTr("Насыщенность")
                        helpText: qsTr("Авто оценивает толщину исходных букв. Вручную выбирает доступное начертание шрифта; результат виден в примере.")
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
                        text: qsTr("Курсив")
                        helpText: qsTr("Авто следует наклону исходного текста; ручной режим принудительно включает или выключает курсив.")
                    }
                    PropRow {
                        id: italicRow
                        key: "italic"
                        defaultValue: false
                        CheckBox {
                            visible: italicRow.manual
                            text: qsTr("Курсив")
                            checked: win.propValue("italic", false) === true
                            onToggled: win.setProp("italic", true, checked)
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                    }
                    FieldLabel {
                        helpText: qsTr("Расстояние между строками относительно размера текста.")
                        text: qsTr("Межстрочный интервал, %")
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
                        helpText: qsTr("Дополнительное расстояние между буквами перевода.")
                        text: qsTr("Межбуквенный интервал, px")
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
                        helpText: qsTr("Расположение строк внутри блока перевода: слева, по центру или справа.")
                        text: qsTr("Выравнивание текста")
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
                            model: [qsTr("По левому краю"), qsTr("По центру"), qsTr("По правому краю")]
                            currentIndex: Math.max(0, values.indexOf(win.propValue("alignment", "center")))
                            onActivated: win.setProp("alignment", true, values[currentIndex])
                        }
                    }
                    FieldLabel {
                        helpText: qsTr("Как размещать текст, который не помещается в одну строку.")
                        text: qsTr("Перенос строк")
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
                        text: qsTr("Цвет текста")
                        helpText: qsTr("Цвет текста перевода. Для режима поверх исходного текста «Авто» оценивает цвет букв и контраст фона. Размер и цвета интерфейса не меняются.")
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
                        helpText: qsTr("Расстояние от текста до границы его фона.")
                        text: qsTr("Внутренние отступы, px")
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
                        text: qsTr("Размер текста: от / до, px")
                        helpText: qsTr("Границы автоматического подбора кегля при размещении перевода в поле. Если текст всё равно не помещается, применяется ограничение переполнения.")
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
                        text: qsTr("Узкий шрифт, если перевод не помещается")
                        helpText: qsTr("Разрешает использовать узкий вариант семейства при автоматическом подборе. Действует только при наличии подходящего встроенного шрифта.")
                    }
                    Switch {
                        checked: win.inplace().allow_condensed_fallback !== false
                        onToggled: win.setInplace("allow_condensed_fallback", checked)
                    }
                    FieldLabel {
                        text: qsTr("Предпочтительные шрифты (через запятую)")
                        helpText: qsTr("Семейства, которые подбор проверяет первыми. Используются доступные встроенные шрифты с глифами нужного языка; поле примера демонстрирует ручное оформление.")
                    }
                    TextField {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        placeholderText: "Noto Serif, Inter"
                        text: (win.inplace().preferred_fonts || []).join(", ")
                        onEditingFinished: win.setInplace("preferred_fonts", text.split(",").map(f => f.trim()).filter(f => f.length > 0))
                    }
                    Label {
                        text: qsTr("Замена шрифта по начертанию оригинала")
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
                                model: [qsTr("Авто")].concat(win.bundledFonts)
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
                    id: settingsActionUnique18
                    HoverHint { control: settingsActionUnique18; feature: settingsActionUnique18.text; explanation: qsTr("Заново определяет шрифты найденных полей на последнем кадре. Не меняет ручные параметры оформления."); active: settingsActionUnique18.enabled }
                            text: qsTr("Определить шрифты заново")
                            onClicked: if (win.controller.reanalyzeFonts)
                                win.controller.reanalyzeFonts()
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        Button {
                    id: settingsActionUnique19
                    HoverHint { control: settingsActionUnique19; feature: settingsActionUnique19.text; explanation: qsTr("Восстанавливает исходные значения указанного раздела. Другие настройки и профили игры сохраняются."); active: settingsActionUnique19.enabled }
                            text: qsTr("Сбросить оформление перевода поверх текста")
                            onClicked: win.resetKeys(["appearance.inplace"])
                        }
                    }
                    Label {
                        Layout.columnSpan: 2
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: qsTr("Защита соседних полей от наложения всегда включена. Если перевод не помещается, поле пропускается и причина выводится в журнал.")
                        opacity: 0.85
                    }
                }
                GridLayout {
                    visible: win.current.display_mode !== "inplace"
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    columns: 2
                    columnSpacing: ui.margin
                    rowSpacing: 12
                    FieldLabel {
                        text: qsTr("Фон перевода")
                        helpText: qsTr("Выбирает сплошной фон, прозрачность, затемнение или размытие KWin. Цвета текста для некоторых режимов подбираются самим компонентом.")
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
                        text: qsTr("Размытие (KWin)")
                        visible: win.current.appearance.window.background_style === "blur"
                        helpText: qsTr("Просит композитор размыть изображение за окном перевода. Сила эффекта зависит от настроек KWin; внутри примера показан оттенок фона без композиторного размытия.")
                    }
                    Switch {
                        visible: win.current.appearance.window.background_style === "blur"
                        checked: win.current.appearance.window.blur_enabled !== false
                        onToggled: win.set("appearance.window.blur_enabled", checked)
                    }
                    FieldLabel {
                        helpText: qsTr("100 % — непрозрачно, 0 % — полностью прозрачно. Изменяет только фон.")
                        text: qsTr("Прозрачность фона")
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
                        text: qsTr("Светлый фон, тёмный текст")
                        visible: win.current.appearance.window.background_style === "dim"
                        helpText: qsTr("Инвертирует режим лёгкого фона: светлая подложка и тёмный текст. Не меняет палитру интерфейса.")
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
                        opacity: 0.85
                        visible: !win.solidStyle
                        text: win.current.appearance.window.background_style === "transparent" ? qsTr("Белый текст с лёгкой тенью, без фона. Цвета текста и фона ниже действуют только в режиме «Сплошной фон».") : qsTr("Размытие делает KWin; его силу задают «Системные настройки → Эффекты рабочего стола → Размытие». ") + qsTr("Без KWin рисуется только тонированный фон. Цвета текста и фона ниже действуют только в режиме «Сплошной фон».")
                    }
                    SectionTitle {
                        text: qsTr("Текст перевода")
                    }
                    FieldLabel {
                        helpText: qsTr("Шрифт результата перевода. Список содержит шрифты из комплекта приложения.")
                        text: qsTr("Шрифт перевода")
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
                        helpText: qsTr("Размер текста перевода в пикселях. Автоматический режим подбирает его по исходному тексту.")
                        text: qsTr("Размер текста, px")
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
                        text: qsTr("Начертание")
                        helpText: qsTr("Полужирный и курсив для отдельного окна перевода. Применяются сразу и отображаются в примере.")
                    }
                    RowLayout {
                        CheckBox {
                            text: qsTr("Жирный")
                            checked: win.current.appearance.window.font_bold === true
                            onToggled: win.set("appearance.window.font_bold", checked)
                        }
                        CheckBox {
                            text: qsTr("Курсив")
                            checked: win.current.appearance.window.font_italic === true
                            onToggled: win.set("appearance.window.font_italic", checked)
                        }
                    }
                    FieldLabel {
                        text: qsTr("Цвет текста")
                        visible: win.solidStyle
                        helpText: qsTr("Цвет текста перевода. Для режима поверх исходного текста «Авто» оценивает цвет букв и контраст фона. Размер и цвета интерфейса не меняются.")
                    }
                    ColorRow {
                        visible: win.solidStyle
                        key: "appearance.window.text_color"
                        presets: ["#ffffff", "#ffe066", "#7cf29c", "#7cc7ff"]
                    }
                    FieldLabel {
                        helpText: qsTr("Обводка букв повышает читаемость перевода на сложном фоне.")
                        text: qsTr("Контур текста")
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
                                feature: qsTr("Цвет контура текста")
                                reason: qsTr("Контур текста выключен.")
                                remedy: qsTr("Включите контур текста.")
                            }
                        }
                    }
                    FieldLabel {
                        helpText: qsTr("Расстояние между строками текста перевода.")
                        text: qsTr("Межстрочный интервал")
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
                        helpText: qsTr("Расположение строк внутри блока перевода: слева, по центру или справа.")
                        text: qsTr("Выравнивание текста")
                        helpControl: translationSettingHelpTarget15
                    }
                    ComboBox {
                        id: translationSettingHelpTarget15
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        readonly property var values: ["left", "center", "right"]
                        model: [qsTr("По левому краю"), qsTr("По центру"), qsTr("По правому краю")]
                        currentIndex: Math.max(0, values.indexOf(win.current.appearance.window.text_alignment || "center"))
                        onActivated: win.set("appearance.window.text_alignment", values[currentIndex])
                    }
                    FieldLabel {
                        helpText: qsTr("Как размещать текст, который не помещается в одну строку.")
                        text: qsTr("Перенос строк")
                        helpControl: translationSettingHelpTarget16
                    }
                    Switch {
                        id: translationSettingHelpTarget16
                        checked: win.current.appearance.window.text_wrap !== false
                        onToggled: win.set("appearance.window.text_wrap", checked)
                    }
                    SectionTitle {
                        text: qsTr("Журнал переводов")
                    }
                    FieldLabel {
                        text: qsTr("Показывать журнал переводов")
                        helpText: qsTr("Предыдущие переводы остаются над текущим и постепенно тускнеют. Удобно в быстрых диалогах, когда реплика меняется раньше, чем её успели прочитать. Журнал живёт, пока работает LipaX, и не зависит от истории переводов.")
                    }
                    Switch {
                        objectName: "logMode"
                        checked: win.current.appearance.window.log_mode === true
                        onToggled: win.set("appearance.window.log_mode", checked)
                    }
                    FieldLabel {
                        text: qsTr("Записей в журнале")
                        visible: win.current.appearance.window.log_mode === true
                        helpText: qsTr("Сколько переводов показывать вместе с текущим (от 2 до 10). Лишние записи свободное окно прокручивает колесом мыши.")
                    }
                    SpinBox {
                        objectName: "logEntries"
                        visible: win.current.appearance.window.log_mode === true
                        from: 2
                        to: 10
                        editable: true
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        value: win.current.appearance.window.log_entries || 5
                        onValueModified: win.set("appearance.window.log_entries", value)
                    }
                    SectionTitle {
                        text: qsTr("Оригинал")
                    }
                    FieldLabel {
                        text: qsTr("Показывать оригинал над переводом")
                        helpText: qsTr("Добавляет исходный распознанный текст над переводом. Для него доступны отдельные шрифт, размер и цвет.")
                    }
                    Switch {
                        checked: win.current.appearance.window.show_original === true
                        onToggled: win.set("appearance.window.show_original", checked)
                    }
                    FieldLabel {
                        text: qsTr("Шрифт оригинала")
                        visible: win.current.appearance.window.show_original === true
                        helpText: qsTr("Семейство для исходного текста над переводом. Не влияет на распознавание или шрифт переведённого текста.")
                    }
                    ComboBox {
                        visible: win.current.appearance.window.show_original === true
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        model: [qsTr("Как у перевода")].concat(win.fontFamilies)
                        currentIndex: win.current.appearance.window.original_font_family ? win.fontIndex(win.current.appearance.window.original_font_family) + 1 : 0
                        onActivated: win.set("appearance.window.original_font_family", currentIndex === 0 ? "" : currentText)
                    }
                    FieldLabel {
                        text: qsTr("Размер оригинала, px")
                        visible: win.current.appearance.window.show_original === true
                        helpText: qsTr("Кегль исходного текста в отдельном окне перевода. Размер интерфейса и перевода остаётся прежним.")
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
                        text: qsTr("Цвет оригинала")
                        visible: win.current.appearance.window.show_original === true
                        helpText: qsTr("Цвет исходного текста над переводом в режиме сплошного фона. В остальных режимах цвет определяет компонент отображения.")
                    }
                    ColorRow {
                        visible: win.current.appearance.window.show_original === true
                        key: "appearance.window.original_color"
                        presets: ["#b0b0b0", "#ffffff", "#ffe066"]
                    }
                    FieldLabel {
                        text: qsTr("Цвет фона")
                        visible: win.solidStyle
                        helpText: qsTr("Цвет сплошной подложки отдельного окна. Прозрачность задаётся отдельно: 100 % означает непрозрачный фон.")
                    }
                    ColorRow {
                        visible: win.solidStyle
                        key: "appearance.window.background_color"
                        presets: ["#181818", "#000000", "#202040", "#300030"]
                    }
                    FieldLabel {
                        helpText: qsTr("Расстояние от текста до границы его фона.")
                        text: qsTr("Внутренние отступы, px")
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
                columnSpacing: ui.margin
                rowSpacing: ui.gap
                Label {
                    text: qsTr("Горячие клавиши")
                    font.bold: true
                    Layout.columnSpan: 2
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.85
                    text: qsTr("Нажмите кнопку и сочетание. Backspace — очистить, Esc — отмена. Применяются автоматически; ") + qsTr("если сочетание занято другой программой, будет показано в статусе.")
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
                    color: ui.error
                    visible: win.hasDuplicateHotkeys
                    text: qsTr("Одинаковые сочетания у разных действий — работать будет только одно.")
                }
                Label {
                    text: qsTr("Геймпад")
                    font.bold: true
                    Layout.columnSpan: 2
                }
                Switch {
                    objectName: "gamepadSwitch"
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    checked: win.get(win.current, "hotkeys.gamepad.enabled") === true
                    text: qsTr("Действия по кнопкам геймпада")
                    onToggled: { win.set("hotkeys.gamepad.enabled", checked); if (checked) win.controller.refreshGamepads() }
                    Component.onCompleted: win.controller.refreshGamepads()
                }
                Label {
                    objectName: "gamepadStatus"
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.85
                    color: ((win.gamepadProbe.gamepads || []).length === 0 && (win.gamepadProbe.denied || 0) > 0) ? ui.error : palette.windowText
                    text: (win.gamepadProbe.gamepads || []).length > 0
                        ? qsTr("Найдено: ") + win.gamepadProbe.gamepads.join(", ")
                        : ((win.gamepadProbe.denied || 0) > 0
                            ? qsTr("Геймпад не найден, а к части устройств ввода нет доступа: добавьте пользователя в группу input или проверьте правила udev (uaccess).")
                            : qsTr("Геймпад не найден. Подключённый позже будет найден автоматически."))
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.85
                    text: qsTr("Нажмите «Записать» и удерживайте две или больше кнопок. Игра по-прежнему получает эти кнопки: выберите сочетание, которое она не использует (например, Select+Start). Кнопки читаются, только пока переключатель включён.")
                }
                Repeater {
                    objectName: "gamepadRows"
                    model: win.hotkeyRows
                    delegate: RowLayout {
                        Layout.columnSpan: 2
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        enabled: win.get(win.current, "hotkeys.gamepad.enabled") === true
                        Label {
                            wrapMode: Text.Wrap
                            text: modelData.title
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                        }
                        Button {
                            objectName: "gamepadChord_" + modelData.key
                            Layout.preferredWidth: 190
                            highlighted: win.learningPadKey === modelData.key
                            palette.buttonText: win.gamepadDuplicate(modelData.key) ? ui.error : undefined
                            text: win.learningPadKey === modelData.key ? qsTr("Нажмите кнопки…")
                                : (win.gamepadChord(modelData.key).length ? win.gamepadChord(modelData.key) : qsTr("не назначено"))
                            onClicked: win.startLearning(modelData.key)
                        }
                        Button {
                            objectName: "gamepadClear_" + modelData.key
                            text: qsTr("Очистить")
                            visible: win.gamepadChord(modelData.key).length > 0
                            onClicked: win.set("hotkeys.gamepad." + modelData.key, "")
                        }
                    }
                }
                Button {
                    id: settingsActionUnique20
                    HoverHint { control: settingsActionUnique20; feature: settingsActionUnique20.text; explanation: qsTr("Восстанавливает исходные значения указанного раздела. Другие настройки и профили игры сохраняются."); active: settingsActionUnique20.enabled }
                    Layout.columnSpan: 2
                    text: qsTr("Сбросить горячие клавиши")
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
                columnSpacing: ui.margin
                rowSpacing: ui.gap
                RowLayout {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    Label {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        opacity: 0.855
                        text: qsTr("Проверка только читает состояние системы: ничего не устанавливает и не скачивает.")
                    }
                    Button {
                    HoverHint { control: unavailableControl12; feature: unavailableControl12.text; explanation: qsTr("Повторно проверяет зависимости, движки OCR и доступность захвата. Ничего не устанавливает."); active: unavailableControl12.enabled }
                        id: unavailableControl12
                        text: win.controller.diagnosticsBusy ? qsTr("Проверка…") : qsTr("Проверить снова")
                        enabled: !win.controller.diagnosticsBusy
                        onClicked: win.controller.refreshDiagnostics()

                        Accessible.description: unavailableHint12.accessibleExplanation
                        UnavailableHint {
                            id: unavailableHint12
                            control: unavailableControl12
                            feature: qsTr("Обновить диагностику")
                            reason: qsTr("Проверка компонентов ещё выполняется.")
                            remedy: qsTr("Дождитесь завершения проверки.")
                        }
                    }
                }
                Repeater {
                    model: win.diagnostics
                    delegate: Frame {
                        id: checkFrame
                        required property var modelData
                        readonly property color stateColor: modelData.state === "ready" ? ui.success : modelData.state === "warning" ? ui.warning : ui.error
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
                                    text: modelData.state === "ready" ? qsTr("готово") : modelData.state === "warning" ? qsTr("внимание") : qsTr("нет")
                                    color: checkFrame.stateColor
                                }
                            }
                            Label {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                wrapMode: Text.WrapAnywhere
                                opacity: 0.85
                                font.pointSize: ui.description.pointSize
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
                    id: settingsActionUnique21
                    HoverHint { control: settingsActionUnique21; feature: settingsActionUnique21.text; explanation: qsTr("Копирует команду или сведения в буфер обмена. Команда автоматически не выполняется."); active: settingsActionUnique21.enabled }
                                    text: qsTr("Копировать")
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
                    opacity: 0.85
                    text: win.controller.diagnosticsBusy ? qsTr("Идёт проверка…") : qsTr("Нажмите «Проверить снова»")
                }
            }
        }
        ScrollView {
            id: pageGeneral
            objectName: "settingsPage8"
            contentWidth: availableWidth
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            GridLayout {
                width: pageGeneral.availableWidth - 16
                columns: 2
                columnSpacing: ui.margin
                rowSpacing: ui.gap
                SectionTitle {
                    text: qsTr("Главное окно: «Оригинал» и «Перевод»")
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.855
                    text: qsTr("Эти параметры относятся только к двум текстовым полям главного окна LipaX. Плавающее окно перевода и перевод поверх игры настраиваются на вкладках «Окно перевода» и «Оформление перевода».")
                }
                FieldLabel {
                    helpText: qsTr("Шрифт текста в главном окне (латиница и кириллица). Доступны только шрифты, поставляемые с LipaX.")
                    text: qsTr("Шрифт")
                    helpControl: mainFont
                }
                ComboBox {
                    id: mainFont
                    objectName: "mainWindowFont"
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    model: win.fontFamilies
                    currentIndex: win.fontIndex(win.mainWindow.font_family)
                    onActivated: win.set("appearance.main_window.font_family", currentText)
                }
                FieldLabel {
                    helpText: qsTr("Шрифт для китайских, японских и корейских иероглифов, которых нет в основном шрифте. «Авто» — Noto Sans CJK из комплекта LipaX.")
                    text: qsTr("Шрифт для CJK")
                    helpControl: mainCjkFont
                }
                ComboBox {
                    id: mainCjkFont
                    objectName: "mainWindowCjkFont"
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    model: [qsTr("Авто")].concat(win.fontFamilies)
                    currentIndex: win.mainWindow.cjk_font_family ? Math.max(0, win.fontFamilies.indexOf(win.mainWindow.cjk_font_family) + 1) : 0
                    onActivated: win.set("appearance.main_window.cjk_font_family", currentIndex === 0 ? "" : currentText)
                }
                FieldLabel {
                    helpText: qsTr("Размер текста в полях «Оригинал» и «Перевод».")
                    text: qsTr("Размер шрифта")
                    helpControl: mainFontSize
                }
                SpinBox {
                    id: mainFontSize
                    objectName: "mainWindowFontSize"
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    from: 8
                    to: 72
                    editable: true
                    value: win.mainWindow.font_size || 16
                    onValueModified: win.set("appearance.main_window.font_size", value)
                }
                FieldLabel {
                    helpText: qsTr("Заливка полей: стандартный серый или цвета текущей темы (светлые при светлой теме, тёмные при тёмной).")
                    text: qsTr("Фон")
                    helpControl: mainBackground
                }
                ComboBox {
                    id: mainBackground
                    objectName: "mainWindowBackground"
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    model: win.mainBackgrounds.map(c => c.label)
                    currentIndex: win.choiceIndex(win.mainBackgrounds, win.mainWindow.background, 0)
                    onActivated: win.set("appearance.main_window.background", win.mainBackgrounds[currentIndex].value)
                }
                ResetButton {
                    text: qsTr("Сбросить настройки главного окна")
                    keys: ["appearance.main_window"]
                }
            }
        }
        ScrollView {
            id: pageApp
            objectName: "settingsPage9"
            contentWidth: availableWidth
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            GridLayout {
                width: pageApp.availableWidth - 16
                columns: 2
                columnSpacing: ui.margin
                rowSpacing: ui.gap
                SectionTitle {
                    text: qsTr("Оформление приложения")
                }
                FieldLabel {
                    helpText: qsTr("Светлая и тёмная палитры переключаются сразу только внутри уже загруженного стиля Universal. «Как в системе», Breeze и Fusion — другие стили Qt: они выбираются при запуске, поэтому действуют после перезапуска LipaX (Breeze нужен пакет qqc2-breeze-style).")
                    text: qsTr("Тема")
                    helpControl: generalTheme
                }
                ComboBox {
                    id: generalTheme
                    objectName: "generalTheme"
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    model: win.themeChoices.map(c => c.label)
                    currentIndex: win.choiceIndex(win.themeChoices, win.theme, 3)
                    onActivated: win.set("general.theme", win.themeChoices[currentIndex].value)
                }
                Label {
                    id: generalThemeNote
                    objectName: "generalThemeNote"
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.855
                    text: win.themePolicy.styleOverridden ? qsTr("Стиль задан переменной QT_QUICK_CONTROLS_STYLE. Она имеет приоритет над выбором в приложении.")
                        : win.themePolicy.restartRequired ? qsTr("Требуется перезапуск · выбранный стиль сохранён. Закройте LipaX через «Выход» в трее и запустите снова.")
                        : qsTr("Смена стиля Qt требует перезапуска. Светлая и тёмная палитры переключаются сразу внутри Universal.")
                    HoverHint { control: generalThemeNote; feature: qsTr("Применение темы"); explanation: qsTr("Для применения другого стиля Qt необходимо перезапустить LipaX. До перезапуска сохраняется активный стиль. Возврат к нему отменяет ожидание.") }
                }
                FieldLabel {
                    text: qsTr("Язык интерфейса")
                    helpControl: uiLanguage
                    helpText: qsTr("Язык меню, настроек и подсказок LipaX. «Автоматически» берёт язык из системных переменных (LANGUAGE, LANG). Переменная LIPAX_UI_LANGUAGE имеет приоритет. Язык применяется после перезапуска LipaX.")
                }
                ComboBox {
                    id: uiLanguage
                    objectName: "uiLanguage"
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    model: win.languageChoices.map(c => c.label)
                    currentIndex: win.choiceIndex(win.languageChoices, win.general.language || "auto", 0)
                    onActivated: win.set("general.language", win.languageChoices[currentIndex].value)
                }
                FieldLabel {
                    text: qsTr("Размер текста интерфейса")
                    helpControl: uiScale
                    helpText: qsTr("Меняет подписи, кнопки и подсказки сразу во всех окнах LipaX. Учитывает системный шрифт KDE. Размер перевода в игре задаётся отдельно.")
                }
                ComboBox {
                    id: uiScale
                    objectName: "uiTextScale"
                    Layout.fillWidth: true
                    model: [qsTr("Небольшой"), qsTr("Обычный"), qsTr("Крупный"), qsTr("Очень крупный")]
                    readonly property var values: ["small", "normal", "large", "extra_large"]
                    currentIndex: Math.max(0, values.indexOf(win.general.ui_text_scale || "normal"))
                    onActivated: win.set("general.ui_text_scale", values[currentIndex])
                }
                SectionTitle {
                    text: qsTr("Трей и запуск")
                }
                FieldLabel {
                    helpText: qsTr("Закрытие скрывает главное окно, пока работает захват. Для полного завершения выберите «Выход» в меню трея. Если трей недоступен, окно завершает приложение.")
                    text: qsTr("Сворачивать в системный трей при закрытии")
                    helpControl: fieldHelpEditor10
                }
                Switch {
                    id: fieldHelpEditor10
                    objectName: "closeToTraySwitch"
                    checked: win.current.close_to_tray === true
                    onToggled: win.set("close_to_tray", checked)
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.855
                    text: qsTr("Включено: закрытие главного окна скрывает его в трей, захват и перевод продолжают работать. ") + qsTr("Выключено: закрытие главного окна завершает LipaX. Полностью выйти можно из меню значка в трее.")
                }
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    wrapMode: Text.Wrap
                    opacity: 0.855
                    text: qsTr("LipaX запускается в одном экземпляре: повторный запуск и действия меню значка (показать, настройки, выбрать окно, запустить и остановить автоперевод, выход) передаются работающему. То же из терминала: lipax --show, --settings, --capture, --start-autotranslate, --stop-autotranslate, --quit.")
                }
                FieldLabel {
                    helpText: qsTr("Создаёт запись в ~/.config/autostart/io.lipa.Translator.desktop: LipaX запускается при входе в систему. Выключение удаляет запись.")
                    text: qsTr("Запускать при входе в систему")
                    helpControl: generalAutostart
                }
                Switch {
                    id: generalAutostart
                    objectName: "generalAutostart"
                    checked: !!win.general.autostart
                    onToggled: win.set("general.autostart", checked)
                }
                FieldLabel {
                    helpText: qsTr("Сохраняет положение, размер и монитор главного окна и окна перевода и восстанавливает их при следующем открытии. Если выключить, сохранённые значения не применяются и не перезаписываются. Под Wayland композитор может не разрешить точное положение: тогда восстанавливается размер.")
                    text: qsTr("Запоминать положение и размер окон")
                    helpControl: generalRememberGeometry
                }
                Switch {
                    id: generalRememberGeometry
                    objectName: "generalRememberGeometry"
                    checked: win.general.remember_window_geometry !== false
                    onToggled: win.set("general.remember_window_geometry", checked)
                }
                SectionTitle {
                    text: qsTr("Уведомления рабочего стола")
                }
                FieldLabel {
                    helpText: qsTr("Всплывающее уведомление, когда область остановилась из-за ошибки и автоповтор закончился. Одно и то же сообщение показывается не чаще раза в минуту.")
                    text: qsTr("Об остановке из-за ошибки")
                    helpControl: generalNotifyErrors
                }
                Switch {
                    id: generalNotifyErrors
                    objectName: "generalNotifyErrors"
                    checked: !!win.general.notify_errors
                    onToggled: win.set("general.notify_errors", checked)
                }
                FieldLabel {
                    helpText: qsTr("Уведомление при каждой ошибке, после которой LipaX повторит попытку (сеть, захват, OCR). Может быть шумным.")
                    text: qsTr("О каждой ошибке с повтором")
                    helpControl: generalNotifyRetries
                }
                Switch {
                    id: generalNotifyRetries
                    objectName: "generalNotifyRetries"
                    checked: !!win.general.notify_retries
                    onToggled: win.set("general.notify_retries", checked)
                }
                SectionTitle {
                    text: qsTr("Журнал")
                }
                FieldLabel {
                    helpText: qsTr("Сколько подробностей писать в журнал (терминал, journalctl). Меняется сразу, без перезапуска. Переменная окружения RUST_LOG при запуске имеет приоритет, пока уровень не изменён здесь.")
                    text: qsTr("Уровень журнала")
                    helpControl: generalLogLevel
                }
                ComboBox {
                    id: generalLogLevel
                    objectName: "generalLogLevel"
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    model: win.logLevels.map(c => c.label)
                    currentIndex: win.choiceIndex(win.logLevels, win.general.log_level, 2)
                    onActivated: win.set("general.log_level", win.logLevels[currentIndex].value)
                }
                ResetButton {
                    text: qsTr("Сбросить настройки приложения")
                    keys: ["general", "close_to_tray"]
                }
            }
        }
        ScrollView {
            id: page10
            objectName: "settingsPage10"
            contentWidth: availableWidth
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ColumnLayout {
                id: about
                objectName: "aboutPage"
                width: page10.availableWidth - 16
                spacing: 10
                // About is static: no perpetual decorative motion or reduced-motion exception needed.
                readonly property bool active: win.visible && tabs.currentIndex === 10
                readonly property string version: win.controller.appVersion ? win.controller.appVersion() : ""

                Item {
                    Layout.alignment: Qt.AlignHCenter
                    Layout.preferredWidth: 160
                    Layout.preferredHeight: 170
                    Image {
                        id: aboutIcon
                        objectName: "aboutIcon"
                        x: 10
                        width: 140
                        height: 140
                        smooth: true
                        source: Qt.resolvedUrl(".").toString().indexOf("qrc:") === 0 ? "qrc:/lipa/icon.svg" : "../assets/lipa.svg"
                        sourceSize.width: 280
                        sourceSize.height: 280
                        transformOrigin: Item.Center
                        y: 24
                    }
                    // The shadow breathes with the float.
                    Rectangle {
                        objectName: "aboutShadow"
                        anchors.horizontalCenter: parent.horizontalCenter
                        y: 162
                        width: 90 - (24 - aboutIcon.y) * 1.6
                        height: 6
                        radius: 3
                        color: "#000000"
                        opacity: 0.18 - (24 - aboutIcon.y) * 0.004
                    }
                }
                Label {
                    objectName: "aboutTitle"
                    Layout.alignment: Qt.AlignHCenter
                    text: "LipaX"
                    font.pointSize: ui.title.pointSize
                    font.bold: true
                }
                Label {
                    objectName: "aboutVersion"
                    Layout.alignment: Qt.AlignHCenter
                    visible: about.version.length > 0
                    text: qsTr("Версия ") + about.version
                    opacity: 0.85
                }
                Label {
                    objectName: "qtVersionDiagnostic"
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    text: win.controller && typeof win.controller.qtVersion === "function" ? win.controller.qtVersion() : "Qt 6.12+"
                }
                Label {
                    objectName: "aboutDescription"
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    text: qsTr("Переводчик игрового текста в реальном времени для KDE Plasma 6 (KWin, Wayland). Распознаёт текст в выбранном окне ")
                        + qsTr("(Tesseract, PaddleOCR или RapidOCR), переводит и показывает поверх оригинала или в отдельном окне.")
                }
                Item {
                    id: tickerViewport
                    Layout.fillWidth: true
                    Layout.preferredHeight: tickerText.implicitHeight + ui.gap
                    clip: true
                    Label {
                        id: tickerText
                        objectName: "aboutTicker"
                        anchors.verticalCenter: parent.verticalCenter
                        text: qsTr("LipaX — распознавание и перевод игрового текста  ·  Qt 6 / QML  ·  KDE Plasma  ·  Wayland  ·  Tesseract  ·  PaddleOCR  ·  RapidOCR")
                        font: ui.body
                        opacity: 0.85
                        wrapMode: Text.Wrap
                        width: parent.width
                    }
                }
                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 1
                    color: palette.mid
                    opacity: 0.5
                }
                GridLayout {
                    Layout.fillWidth: true
                    columns: 2
                    columnSpacing: ui.margin
                    rowSpacing: 8
                    Label { text: qsTr("Автор"); font.bold: true }
                    Label { text: "GrayRat" }
                    Label { text: qsTr("Лицензия"); font.bold: true }
                    Label { text: qsTr("MIT; встроенные шрифты — OFL-1.1 и Apache-2.0") }
                    Label { text: qsTr("Репозиторий"); font.bold: true }
                    Label {
                        objectName: "aboutRepository"
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        textFormat: Text.RichText
                        text: '<a href="https://github.com/GrayRats/lipax-qt">https://github.com/GrayRats/lipax-qt</a>'
                        onLinkActivated: url => Qt.openUrlExternally(url)
                        HoverHandler { cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor }
                    }
                }
                Label {
                    text: qsTr("Основа и зависимости")
                    font.bold: true
                    Layout.topMargin: 6
                }
                Repeater {
                    objectName: "aboutLinks"
                    model: [
                        { url: "https://github.com/satix-one/lipa.git", role: qsTr("исходный проект (форк)") },
                        { url: "https://github.com/rtr46/meikipop", role: qsTr("зависимость") },
                        { url: "https://github.com/tesseract-ocr/tesseract", role: qsTr("распознавание: Tesseract OCR") },
                        { url: "https://github.com/tesseract-ocr/tessdata_fast", role: qsTr("языковые модели Tesseract") },
                        { url: "https://github.com/PaddlePaddle/PaddleOCR", role: qsTr("распознавание: PaddleOCR") },
                        { url: "https://github.com/RapidAI/RapidOCR", role: qsTr("распознавание: RapidOCR (модели PP-OCRv5 в ONNX)") },
                        { url: "https://github.com/rtr46/meikiocr", role: qsTr("распознавание: MeikiOCR (японский, модели LGPL-3.0)") }
                    ]
                    delegate: Label {
                        required property var modelData
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        wrapMode: Text.Wrap
                        textFormat: Text.RichText
                        text: '<a href="' + modelData.url + '">' + modelData.url + '</a> — ' + modelData.role
                        onLinkActivated: url => Qt.openUrlExternally(url)
                        HoverHandler { cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor }
                    }
                }
            }
        }
    }
}
