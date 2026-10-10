import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Controls.Universal
import Qt.labs.platform as Platform
import io.lipa

ApplicationWindow {
    id: root
    objectName: "mainWindow"
    visible: true
    width: 680
    height: 500
    minimumWidth: 480
    minimumHeight: Math.max(400, mainLayout.implicitHeight + 2 * ui.margin)
    UiTheme { id: ui; textScale: settingsWin.general.ui_text_scale || "normal" }
    font: ui.body
    Universal.theme: settingsWin.universalTheme
    title: "LipaX — переводчик для игр"
    // Two separate things: hiding to the tray (`hideToTray`, everything keeps running) and quitting
    // (`quitApp`, stops everything). Capture, OCR, translation and overlays belong to the
    // Controller and the translation windows below, not to this window, so hiding it changes nothing for them.
    property bool closingDown: false
    readonly property bool trayEnabled: settingsWin.current.close_to_tray === true && tray.available

    onClosing: (close) => {
        if (closingDown) return
        if (trayEnabled) {
            close.accepted = false
            hideToTray()
        } else {
            quitApp()
        }
    }

    function showMain(token) {
        if (visibility === Window.Minimized) showNormal(); else show()
        raise()
        requestActivate()
        ctl.activateWindow("mainWindow", token || "")
    }
    function hideToTray() {
        // Secondary windows would be left without a way back; the overlays and frames stay.
        historyWin.close()
        ocrWin.close()
        settingsWin.close()
        regionWin.close()
        hide()
    }
    // Left click on the tray icon: show / raise / (when it is already in front) hide.
    function toggleMain() {
        if (!visible || visibility === Window.Minimized) showMain("")
        else if (!active) showMain("")
        else hideToTray()
    }
    // The only exit path: command line, tray menu and the close button without the tray option.
    function quitApp() {
        if (closingDown) return
        closingDown = true
        tray.visible = false
        settingsWin.apply()
        ctl.stop()
        historyWin.close()
        ocrWin.close()
        settingsWin.close()
        regionWin.close()
        selectionFrame.close()
        Qt.quit()
    }
    // One handler for the tray menu, `lipax --…`, Desktop Actions and the second launch.
    function perform(action, token) {
        switch (action) {
        case "show": showMain(token); break
        case "settings":
            settingsWin.openWindow()
            Qt.callLater(() => ctl.activateWindow("settingsWindow", token || ""))
            break
        case "capture": ctl.pickWindow(); break
        case "start-autotranslate": if (!ctl.startAutoTranslate()) showMain(token); break
        case "stop-autotranslate": ctl.stopAutoTranslate(); break
        case "quit": quitApp(); break
        }
    }

    HistoryWindow { id: historyWin; settingsWindow: settingsWin }
    OcrPreviewWindow { id: ocrWin; controller: ctl; universalTheme: settingsWin.universalTheme; uiTextScale: settingsWin.general.ui_text_scale || "normal" }

    Controller {
        id: ctl
        objectName: "controller"
        onFrameRequested: (x, y, w, h) => selectionFrame.flash(x, y, w, h)
        onSelectRegionRequested: if (ctl.windowTitle.length > 0) regionWin.begin()
        onTogglePinRequested: ctl.setTranslationWindowPinned(!translationWindow.pinned, "hotkey")
        onActionRequested: (action, token) => root.perform(action, token)
    }

    // System tray. The menu mirrors the Desktop Actions of io.lipa.Translator.desktop; the
    // start / stop entries follow the live state of the Controller.
    Platform.SystemTrayIcon {
        id: tray
        objectName: "trayIcon"
        visible: !root.closingDown
        icon.source: "qrc:/lipa/icon.svg"
        tooltip: ctl.modelBusy ? "LipaX: " + ((JSON.parse(ctl.modelState || "{}").progress ?? -1) < 0
            ? "Поиск модели…" : "Загрузка модели… " + JSON.parse(ctl.modelState).progress + "%")
            : "LipaX — " + (ctl.running ? "автоперевод запущен" : "автоперевод остановлен")
        onActivated: (reason) => { if (reason === Platform.SystemTrayIcon.Trigger) root.toggleMain() }
        menu: Platform.Menu {
            Platform.MenuItem { text: "Открыть LipaX"; onTriggered: root.perform("show", "") }
            Platform.MenuItem { text: "Настройки"; onTriggered: root.perform("settings", "") }
            Platform.MenuItem { text: "Выбрать окно для захвата"; onTriggered: root.perform("capture", "") }
            Platform.MenuItem {
                text: "Запустить автоперевод"
                visible: !ctl.running
                onTriggered: root.perform("start-autotranslate", "")
            }
            Platform.MenuItem {
                text: "Остановить автоперевод"
                visible: ctl.running
                onTriggered: root.perform("stop-autotranslate", "")
            }
            Platform.MenuSeparator {}
            Platform.MenuItem { text: "Выход"; onTriggered: root.perform("quit", "") }
        }
    }

    // ── «Original» and «Translation» areas of this window: their own font and background (the «General» tab) ──
    readonly property var mainSettings: (settingsWin.current.appearance && settingsWin.current.appearance.main_window) || ({})
    readonly property string mainFamily: mainSettings.font_family || "Inter"
    readonly property string mainCjkFamily: mainSettings.cjk_font_family || "Noto Sans CJK SC"
    readonly property int mainFontSize: mainSettings.font_size || 16
    readonly property bool mainSystemBackground: mainSettings.background === "system"
    readonly property color mainBackground: mainSystemBackground ? palette.base : "#2b2b2b"
    readonly property color mainTextColor: mainSystemBackground ? palette.text : "#e8e8e8"
    // QML has no per-glyph font fallback list: a text with Chinese, Japanese or Korean glyphs is shown as rich text,
    // each run of them in the CJK family. Any other text stays plain.
    readonly property var cjkRun: /([\u2e80-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uac00-\ud7af\uf900-\ufaff\uff00-\uffef]+)/g
    function hasCjk(text) { return /[\u2e80-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uac00-\ud7af\uf900-\ufaff\uff00-\uffef]/.test(text || "") }
    function markup(text) {
        if (!hasCjk(text)) return text
        const escaped = text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
        return escaped.replace(cjkRun, "<span style=\"font-family:'" + mainCjkFamily + "'\">$1</span>").replace(/\n/g, "<br>")
    }
    function format(text) { return hasCjk(text) ? TextEdit.RichText : TextEdit.PlainText }

    SettingsWindow { id: settingsWin; controller: ctl; onSelectRegionRequested: regionWin.begin() }

    // The two renderers have separate state owned by the Controller: which one runs
    // (effectiveDisplay, with a logged fallback for portal capture) and their own visibility.
    readonly property bool inplaceActive: ctl.effectiveDisplay === "inplace"
    readonly property bool windowActive: ctl.effectiveDisplay === "window"
    readonly property string portalMonitorGeometry: {
        const s = settingsWin.current
        if (!s || !s.capture.portal_fills_monitor || !s.capture.window || s.capture.window.uuid !== "portal:window" || !s.translation_window.screen)
            return ""
        const screen = Qt.application.screens.find(item => item.name === s.translation_window.screen)
        return screen ? JSON.stringify([screen.virtualX, screen.virtualY, screen.width, screen.height]) : ""
    }
    onPortalMonitorGeometryChanged: ctl.reportPortalMonitorGeometry(portalMonitorGeometry)
    Component.onCompleted: ctl.reportPortalMonitorGeometry(portalMonitorGeometry)
    readonly property var inplaceEntries: { try { return JSON.parse(ctl.inplaceJson || "[]") } catch (e) { return [] } }
    // Keyed by "region:field": a stable field keeps its window; only its properties update.
    readonly property string inplaceKeys: JSON.stringify(closingDown ? [] : inplaceEntries.map(e => e.key))
    Instantiator {
        model: JSON.parse(root.inplaceKeys)
        delegate: InplaceTranslation {
            required property string modelData
            settings: settingsWin.current
            entry: root.inplaceEntries.find(e => e.key === modelData) || null
            gameGeometry: ctl.gameGeometry
            visible: root.inplaceActive && ctl.inplaceVisible && ctl.overlayAllowed && !relocating && !!desktopRect && !!entry && entry.text.length > 0
            // MMB hides only the in-place translation; the translation window is untouched.
            onHideRequested: ctl.setInplaceVisibility(false, "mmb")
        }
    }

    // One outline per active region with an area; each has its own timer.
    property bool started: false
    Timer { interval: 1500; running: true; onTriggered: root.started = true }
    readonly property string frameRegionIds:
        JSON.stringify(closingDown ? [] : (settingsWin.current.capture.regions || []).filter(r => r.enabled && r.rect).map(r => r.id))
    Instantiator {
        model: JSON.parse(root.frameRegionIds)
        delegate: RegionFrame {
            required property string modelData
            settings: settingsWin.current
            region: (settingsWin.current.capture.regions || []).find(r => r.id === modelData) || null
            gameGeometry: ctl.gameGeometry
            pinned: settingsWin.current.translation_window.mode === "pinned"
            flashOnCreate: root.started
        }
    }
    RegionSelector { id: regionWin; controller: ctl }
    FrameOverlay { id: selectionFrame; settings: settingsWin.current }
    // What could not be shown over the game as planned (see Controller.inplaceFallbackJson). The "over the original"
    // mode never uses the translation window: such fields are simplified or left out, and the note below says so.
    readonly property var inplaceFallback: { try { return JSON.parse(ctl.inplaceFallbackJson || "{}") } catch (e) { return ({}) } }
    readonly property int droppedFields: (inplaceFallback.dropped || []).length
    readonly property int degradedFields: inplaceFallback.degraded || 0
    TranslationWindow {
        id: translationWindow
        controller: ctl
        translation: ctl.translation
        original: ctl.original
        gameGeometry: ctl.gameGeometry
        shown: !root.closingDown && ((root.windowActive && ctl.translationWindowVisible && ctl.overlayAllowed && ctl.translation.length > 0))
        settings: settingsWin.current
        // Pin state and the pinned placement are decided in Rust (pinned lands where floating was).
        onPinToggled: (p) => ctl.setTranslationWindowPinned(p, "mmb")
        // Closing hides only the translation window; "Отдельное окно перевода" or Ctrl+Alt+H shows it again.
        onCloseRequested: root.windowActive ? ctl.setTranslationWindowVisibility(false, "close_button") : ctl.setInplaceVisibility(false, "fallback_close")
        // Any change of the selected areas (KWin or portal) briefly reveals a hidden frame.
        readonly property string areasKey: JSON.stringify((settingsWin.current.capture.regions || []).map(r => [r.enabled, r.rect]))
        onAreasKeyChanged: flashFrame()
    }

    ColumnLayout {
        id: mainLayout
        anchors.fill: parent
        anchors.margins: ui.margin
        spacing: ui.gap
        RowLayout {
            Layout.fillWidth: true
            Label { text: "LipaX"; font: ui.heading }
            Label {
                id: gameTitle
                Layout.fillWidth: true
                text: ctl.windowTitle.length ? ctl.windowTitle : "Выберите окно игры"
                elide: Text.ElideRight
                HoverHint { control: gameTitle; feature: "Игра"; explanation: ctl.windowTitle.length ? ctl.windowTitle + "\nПри выборе знакомой игры восстанавливаются её области и профиль." : "Выберите окно игры, затем выделите область с текстом." }
            }
            Label { text: ctl.running ? "● Работает" : "○ Остановлено"; font: ui.description }
        }
        Flow {
            Layout.fillWidth: true
            spacing: ui.gap
            ActionButton { objectName: "selectWindowButton"; iconFile: "select-window.svg"; text: "Выбрать окно"; helpText: "Выберите окно игры щелчком. Для знакомой игры восстановится сохранённый профиль; Esc отменяет выбор."; onClicked: ctl.pickWindow() }
            ActionButton {
                id: startTranslation
                objectName: "startTranslationButton"
                iconFile: "start-translation.svg"
                activity: ctl.running ? "running" : "stopped"
                text: ctl.running ? "Остановить перевод" : "Запуск перевода"
                highlighted: false
                enabled: ctl.running || ctl.hasRegion
                helpText: ctl.running ? "Останавливает автоматическое распознавание и перевод. Области и профиль сохраняются." : "Запускает автоматическое распознавание текста в активных областях и отправляет его выбранному переводчику."
                UnavailableHint { control: startTranslation; feature: "Начать перевод"; reason: "Нет выделенной области захвата."; remedy: "Выберите окно игры и область с текстом." }
                onClicked: ctl.running ? ctl.stop() : ctl.start()
            }
        }
        RowLayout {
            Layout.fillWidth: true
            ComboBox {
                id: regionBox
                Layout.fillWidth: true
                Layout.minimumWidth: 80
                readonly property var regions: settingsWin.current.capture.regions || []
                model: regions.map(r => (r.rect ? "" : "○ ") + r.name + (r.enabled ? "" : " (выкл.)"))
                currentIndex: Math.max(0, regions.findIndex(r => r.id === settingsWin.current.capture.active_region))
                onActivated: { settingsWin.activateRegion(currentIndex, true); settingsWin.apply() }
                HoverHint { control: regionBox; feature: "Активная область"; explanation: "Выбирает и включает область для перевода. Кружок означает, что её границы ещё не выделены. Несколько областей можно включить в настройках захвата." }
            }
            ActionButton {
                id: selectCaptureRegion
                objectName: "selectCaptureRegionButton"
                iconFile: "select-capture-area.svg"
                text: "Выбрать область захвата"
                enabled: ctl.windowTitle.length > 0
                helpText: "Выделите прямоугольник с текстом в выбранном окне игры. Изменяется только активная область."
                onClicked: regionWin.begin()
                UnavailableHint { control: selectCaptureRegion; feature: "Выделить область"; reason: "Окно игры ещё не выбрано."; remedy: "Нажмите «Выбрать окно»." }
            }
            ActionButton {
                id: resetCaptureRegion
                text: "Сбросить"
                enabled: ctl.hasRegion
                helpText: "Сбрасывает границы активной области. Остальные области и настройки игры сохраняются."
                onClicked: ctl.resetRegion()
                UnavailableHint { control: resetCaptureRegion; feature: "Сбросить область"; reason: "Область ещё не выделена."; remedy: "Нажмите «Выделить»." }
            }
        }
        CheckBox {
            id: displayToggle
            Layout.fillWidth: true
            contentItem: Text {
                text: displayToggle.text
                font: displayToggle.font
                color: displayToggle.palette.windowText
                leftPadding: displayToggle.indicator.width + displayToggle.spacing
                wrapMode: Text.Wrap
                verticalAlignment: Text.AlignVCenter
            }
            objectName: root.windowActive ? "translationWindowToggle" : "inplaceToggle"
            text: root.windowActive ? "Показывать окно перевода" : "Показывать поверх исходного текста"
            checked: root.windowActive ? ctl.translationWindowVisible : ctl.inplaceVisible
            onToggled: root.windowActive ? ctl.setTranslationWindowVisibility(checked, "checkbox") : ctl.setInplaceVisibility(checked, "checkbox")
            HoverHint { control: displayToggle; feature: "Видимость перевода"; explanation: "Показывает или скрывает текущий способ отображения. Распознавание и перевод продолжают работать в фоне." }
        }
        Label {
            objectName: "displayNote"
            Layout.fillWidth: true
            visible: ctl.displayNote.length > 0
            text: "⚠ Поверх исходного текста недоступен: " + ctl.displayNote
            wrapMode: Text.Wrap
        }
        Label {
            objectName: "fallbackNote"
            Layout.fillWidth: true
            visible: root.inplaceActive && (root.degradedFields > 0 || root.droppedFields > 0)
            wrapMode: Text.Wrap
            text: "⚠ Упрощено полей: " + root.degradedFields + "; не показано: " + root.droppedFields + ". Недостаточно места без перекрытия текста."
        }
        GridLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            columns: root.width >= 640 && ui.bodyPoints < 18 ? 2 : 1
            columnSpacing: ui.gap
            rowSpacing: ui.gap
            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.preferredWidth: 1
                Label { text: "Оригинал"; font: ui.description }
                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.minimumHeight: 40
                    TextArea {
                        objectName: "mainOriginalText"
                        text: root.markup(ctl.original)
                        textFormat: root.format(ctl.original)
                        placeholderText: "Текст появится после распознавания"
                        readOnly: true
                        wrapMode: Text.Wrap
                        color: root.mainTextColor
                        font.family: root.mainFamily
                        font.pixelSize: root.mainFontSize
                        background: Rectangle { color: root.mainBackground; radius: ui.radius }
                    }
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.preferredWidth: 1
                Label { text: "Перевод"; font.bold: true }
                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.minimumHeight: 50
                    TextArea {
                        objectName: "mainTranslationText"
                        text: root.markup(ctl.translation)
                        textFormat: root.format(ctl.translation)
                        placeholderText: "Здесь будет перевод"
                        readOnly: true
                        wrapMode: Text.Wrap
                        color: root.mainTextColor
                        font.family: root.mainFamily
                        font.pixelSize: root.mainFontSize
                        background: Rectangle { color: root.mainBackground; radius: ui.radius }
                    }
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Label {
                id: statusLabel
                Layout.fillWidth: true
                text: (ctl.statusKind === "error" ? "Ошибка: " : ctl.statusKind === "warning" ? "⚠ " : "") + ctl.status
                textFormat: Text.PlainText
                elide: Text.ElideRight
                font.bold: ctl.statusKind === "error"
                HoverHint { control: statusLabel; feature: "Состояние перевода"; explanation: statusLabel.text }
            }
            Label {
                id: providerFallbackStatus
                visible: ["deepl", "microsoft"].includes(settingsWin.current.translation.service)
                text: (settingsWin.current.translation.service === "deepl" ? "DeepL" : "Microsoft") + " · резерв: Google"
                font: ui.description
                HoverHint { control: providerFallbackStatus; feature: "Резервный сервис"; explanation: "При ошибке запроса выбранного сервиса текст отправляется Google Translate. При отсутствующем API-ключе резерв не используется. Это настроенный порядок обработки, а не подтверждение использования резерва для текущего текста." }
            }
            ActionButton {
                visible: (ctl.statusKind === "error" || ctl.statusKind === "warning") && ctl.hasRegion
                text: "Повторить"
                helpText: "Повторяет неудавшееся распознавание и перевод; сбрасывает остановку автоматических повторов."
                onClicked: ctl.translateOnce()
            }
        }
        Flow {
            Layout.fillWidth: true
            spacing: ui.gap
            ActionButton { objectName: "ocrPreviewButton"; iconFile: "ocr-preview.svg"; text: "Предпросмотр распознавания"; helpText: "Открывает последний кадр распознавания, найденные блоки и фильтры. Для появления кадра запустите распознавание."; onClicked: ocrWin.openWindow() }
            ActionButton { objectName: "historyButton"; iconFile: "translation-history.svg"; text: "История переводов"; helpText: "Открывает историю распознанного текста и переводов с поиском и копированием."; onClicked: historyWin.openWindow() }
            ActionButton { objectName: "settingsButton"; iconFile: "settings.svg"; text: "Настройки"; helpText: "Открывает настройки захвата, OCR, сервисов, оформления и сочетаний клавиш. Изменения сохраняются автоматически."; onClicked: settingsWin.openWindow() }
        }
    }
}
