import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Controls.Universal
import Qt.labs.platform as Platform
import io.lipa
import "StatusText.js" as StatusText
import "WindowGeometry.js" as WindowGeometry

ApplicationWindow {
    id: root
    objectName: "mainWindow"
    // Shown from `Component.onCompleted`, after the remembered size has been applied (no visible jump).
    visible: false
    width: 680
    height: 500
    minimumWidth: 480
    // The two text areas have a fixed preferred height of 0, so a long text cannot raise this: only the controls do.
    // Never set from the text, and the window is never resized by the program after the user's own sizing.
    minimumHeight: Math.max(400, mainLayout.implicitHeight + 2 * ui.margin + (textColumns === 2 ? 80 : 160))
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

    // ── Remembering the window ──
    // Needs the "remember window geometry" setting (default on). The record is written only once the window has
    // settled and only while it is an ordinary window (not minimised, maximised or full screen); with the setting
    // off nothing is read, applied or overwritten. Wayland compositors place windows themselves: there only the
    // size is restored, the position is still recorded for X11/XWayland sessions.
    readonly property bool rememberGeometry: settingsWin.general.remember_window_geometry !== false
    readonly property bool canPositionWindows: Qt.platform.pluginName !== "wayland"
    property bool geometryReady: false
    function screenList() {
        return Qt.application.screens.map(s => ({ name: s.name, x: s.virtualX, y: s.virtualY, width: s.width, height: s.height,
            availableWidth: s.desktopAvailableWidth, availableHeight: s.desktopAvailableHeight, scale: s.devicePixelRatio }))
    }
    function restoreGeometry() {
        if (!rememberGeometry) return
        const placed = WindowGeometry.restore(settingsWin.general.main_window, screenList(), minimumWidth, minimumHeight, canPositionWindows)
        if (!placed) return
        width = placed.w
        height = placed.h
        if (placed.position) { x = placed.x; y = placed.y }
        const screen = Qt.application.screens.find(s => s.name === placed.screen)
        if (screen) root.screen = screen
        console.debug("main_window.geometry restored", JSON.stringify(placed))
    }
    function saveGeometry() {
        if (!rememberGeometry || !geometryReady || !visible || visibility !== Window.Windowed) return
        const s = root.screen
        ctl.reportMainWindowGeometry(JSON.stringify({ x: x, y: y, w: width, h: height, screen: s ? s.name : "",
            screen_x: s ? s.virtualX : 0, screen_y: s ? s.virtualY : 0, screen_w: s ? s.width : 0, screen_h: s ? s.height : 0,
            scale: s ? s.devicePixelRatio : 1 }))
    }
    Timer { id: geometrySaver; interval: 600; onTriggered: root.saveGeometry() }
    // The compositor and the platform plugin settle the geometry shortly after the first show: not recorded before.
    Timer { id: geometrySettle; interval: 1200; onTriggered: { root.geometryReady = true; root.saveGeometry() } }
    onXChanged: if (geometryReady) geometrySaver.restart()
    onYChanged: if (geometryReady) geometrySaver.restart()
    onWidthChanged: if (geometryReady) geometrySaver.restart()
    onHeightChanged: if (geometryReady) geometrySaver.restart()
    onScreenChanged: if (geometryReady) geometrySaver.restart()
    onVisibilityChanged: if (geometryReady) geometrySaver.restart()
    onRememberGeometryChanged: if (rememberGeometry && geometryReady) geometrySaver.restart()

    // ── Status light: motion is off for reduced-motion desktops (re-read while the window is visible) ──
    property bool reducedMotion: false
    Timer { interval: 5000; repeat: true; running: root.visible; onTriggered: root.reducedMotion = ctl.reducedMotion() }
    readonly property string indicatorState: ctl.statusKind === "error" ? "error"
        : (ctl.modelBusy || ctl.diagnosticsBusy || ctl.rapidBusy || ctl.tesseractBusy || ctl.paddleBusy) ? "processing"
        : ctl.running ? "running" : "stopped"
    readonly property int textColumns: root.width >= 640 && ui.bodyPoints < 18 ? 2 : 1

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
    // Every screen in logical desktop pixels: Rust keeps the in-place fields to the part of the game window
    // that is on a screen. Re-reported when a monitor is added, removed, moved or rescaled.
    readonly property string screensGeometry: JSON.stringify(Qt.application.screens.map(s => [s.virtualX, s.virtualY, s.width, s.height]))
    onScreensGeometryChanged: ctl.reportScreens(screensGeometry)
    Component.onCompleted: {
        // The platform Qt really runs on (wayland or xcb/XWayland), for logs and live tests: a requested platform
        // that Qt silently replaced must be visible.
        console.info("qt platform=" + Qt.platform.pluginName)
        ctl.reportPortalMonitorGeometry(portalMonitorGeometry)
        ctl.reportScreens(screensGeometry)
        reducedMotion = ctl.reducedMotion()
        // After every component (the settings window loads the saved settings in its own completion handler)
        // and before the first frame: the window is shown once, already at its remembered size.
        Qt.callLater(() => { restoreGeometry(); visible = true; geometrySettle.start() })
    }
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
            visible: root.inplaceActive && ctl.inplaceVisible && ctl.overlayAllowed && !relocating && !!targetScreen && !!desktopRect && !!entry && entry.text.length > 0
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
        // ── Top bar: title, the selected process (takes the free width, never pushes the buttons), then the
        // preview / history / settings group with Settings in the corner.
        RowLayout {
            id: topBar
            Layout.fillWidth: true
            spacing: ui.gap
            Label { text: "LipaX"; font: ui.heading; Layout.alignment: Qt.AlignVCenter }
            ProcessTitle {
                id: gameTitle
                objectName: "processTitle"
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                Layout.preferredWidth: 0
                Layout.alignment: Qt.AlignVCenter
                text: ctl.windowTitle.length ? ctl.windowTitle : "Выберите окно игры"
                underline: ctl.windowTitle.length > 0
                alwaysHint: true
                baseFont: ui.body
                basePixelSize: Math.round(ui.bodyPoints * 96 / 72)
                tooltip: ctl.windowTitle.length ? ctl.windowTitle + "\nПри выборе знакомой игры восстанавливаются её области и профиль." : "Выберите окно игры, затем выделите область с текстом."
            }
            RowLayout {
                id: topButtons
                spacing: 2
                Layout.alignment: Qt.AlignVCenter
                ActionButton { objectName: "ocrPreviewButton"; iconFile: "ocr-preview.svg"; text: "Предпросмотр распознавания"; helpText: "Открывает последний кадр распознавания, найденные блоки и фильтры. Для появления кадра запустите распознавание."; onClicked: ocrWin.openWindow() }
                ActionButton { objectName: "historyButton"; iconFile: "translation-history.svg"; text: "История переводов"; helpText: "Открывает историю распознанного текста и переводов с поиском и копированием."; onClicked: historyWin.openWindow() }
                ActionButton { objectName: "settingsButton"; iconFile: "settings.svg"; text: "Настройки"; helpText: "Открывает настройки захвата, OCR, сервисов, оформления и сочетаний клавиш. Изменения сохраняются автоматически."; onClicked: settingsWin.openWindow() }
            }
        }
        Flow {
            id: actionFlow
            Layout.fillWidth: true
            spacing: ui.gap
            ActionButton { objectName: "selectWindowButton"; iconFile: "select-window.svg"; text: "Выбрать окно"; helpText: "Выберите окно игры щелчком. Для знакомой игры восстановится сохранённый профиль; Esc отменяет выбор."; onClicked: ctl.pickWindow() }
            ActionButton {
                id: startTranslation
                objectName: "startTranslationButton"
                iconFile: ctl.running ? "stop-translation.svg" : "start-translation.svg"
                activity: ctl.running ? "running" : "stopped"
                text: ctl.running ? "Остановить перевод" : "Запуск перевода"
                // The one action that starts work stands out while it can be pressed.
                highlighted: !ctl.running && ctl.hasRegion
                enabled: ctl.running || ctl.hasRegion
                helpText: ctl.running ? "Останавливает автоматическое распознавание и перевод. Области и профиль сохраняются." : "Запускает автоматическое распознавание текста в активных областях и отправляет его выбранному переводчику."
                UnavailableHint { control: startTranslation; feature: "Начать перевод"; reason: "Нет выделенной области захвата."; remedy: "Выберите окно игры и область с текстом." }
                onClicked: ctl.running ? ctl.stop() : ctl.start()
            }
        }
        // ── Capture area controls and the two display switches in one adaptive row (a Flow wraps when narrow;
        // nothing is placed by absolute coordinates).
        Flow {
            id: areaFlow
            objectName: "areaControls"
            Layout.fillWidth: true
            spacing: ui.gap
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
                objectName: "resetCaptureRegionButton"
                iconFile: "reset-capture-area.svg"
                text: "Сбросить область"
                enabled: ctl.hasRegion
                helpText: "Сбрасывает только границы активной области. Остальные области и настройки игры сохраняются."
                onClicked: ctl.resetRegion()
                UnavailableHint { control: resetCaptureRegion; feature: "Сбросить область"; reason: "Область ещё не выделена."; remedy: "Нажмите «Выбрать область захвата»." }
            }
            ComboBox {
                id: regionBox
                objectName: "regionBox"
                // Compact: as wide as its content within 150–260 px and never wider than the row.
                width: Math.min(areaFlow.width, Math.max(150, Math.min(260, implicitContentWidth + 48)))
                implicitHeight: ui.iconButtonSize
                readonly property var regions: settingsWin.current.capture.regions || []
                readonly property var labels: regions.map(r => (r.rect ? "" : "○ ") + r.name + (r.enabled ? "" : " (выкл.)"))
                model: labels
                currentIndex: Math.max(0, regions.findIndex(r => r.id === settingsWin.current.capture.active_region))
                onActivated: { settingsWin.activateRegion(currentIndex, true); settingsWin.apply() }
                // The stored value is untouched: only the picture of it is cut, the tooltip has all of it.
                contentItem: Text {
                    leftPadding: 8
                    rightPadding: regionBox.indicator.width + 8
                    text: regionBox.displayText
                    font: regionBox.font
                    color: regionBox.palette.buttonText
                    elide: Text.ElideRight
                    verticalAlignment: Text.AlignVCenter
                }
                delegate: ItemDelegate {
                    required property int index
                    required property string modelData
                    width: ListView.view ? ListView.view.width : implicitWidth
                    text: modelData
                    font: regionBox.font
                    highlighted: regionBox.highlightedIndex === index
                    contentItem: Text { text: parent.text; font: parent.font; color: parent.palette.text; elide: Text.ElideRight; verticalAlignment: Text.AlignVCenter }
                    ToolTip.visible: hovered && contentItem.truncated
                    ToolTip.text: modelData
                    ToolTip.delay: 400
                }
                popup.width: Math.max(width, Math.min(areaFlow.width, 360))
                Accessible.name: "Название области окна"
                Accessible.description: displayText
                HoverHint { control: regionBox; feature: "Активная область"; explanation: regionBox.displayText + "\n\nВыбирает и включает область для перевода. Кружок означает, что её границы ещё не выделены. Несколько областей можно включить в настройках захвата." }
            }
            // Two independent switches, one per way of showing a translation. Each owns its own visibility
            // (Controller.inplaceVisible / translationWindowVisible); the state is the `checked` look of the button.
            ActionButton {
                id: inplaceToggle
                objectName: "inplaceToggle"
                iconFile: "show-inplace-overlay.svg"
                checkable: true
                checked: ctl.inplaceVisible
                text: "Показывать поверх исходного текста"
                helpText: "Показывает или скрывает перевод поверх исходного текста. Распознавание и перевод продолжают работать в фоне."
                    + (root.inplaceActive ? "" : "\nСейчас выбран другой способ отображения: этот переключатель сработает, когда режим «Поверх исходного текста» будет включён в настройках.")
                onToggled: ctl.setInplaceVisibility(checked, "checkbox")
            }
            ActionButton {
                id: windowToggle
                objectName: "translationWindowToggle"
                iconFile: "show-translation-window.svg"
                checkable: true
                checked: ctl.translationWindowVisible
                text: "Показывать окно перевода"
                helpText: "Показывает или скрывает отдельное окно перевода. Распознавание и перевод продолжают работать в фоне."
                    + (root.windowActive ? "" : "\nСейчас выбран другой способ отображения: этот переключатель сработает, когда режим «Отдельное окно перевода» будет включён в настройках.")
                onToggled: ctl.setTranslationWindowVisibility(checked, "checkbox")
            }
        }
        Label {
            objectName: "displayNote"
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            visible: ctl.displayNote.length > 0
            text: "⚠ Поверх исходного текста недоступен: " + ctl.displayNote
            wrapMode: Text.Wrap
        }
        Label {
            objectName: "fallbackNote"
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            visible: root.inplaceActive && (root.degradedFields > 0 || root.droppedFields > 0)
            wrapMode: Text.Wrap
            text: "⚠ Упрощено полей: " + root.degradedFields + "; не показано: " + root.droppedFields + ". Недостаточно места в видимой части окна игры без перекрытия текста."
        }
        // ── Original / translation (see TranslationPanels: the text never decides the size of the window) ──
        TranslationPanels {
            id: textGrid
            objectName: "textPanels"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumWidth: 0
            Layout.preferredHeight: 0
            columns: root.textColumns
            gap: ui.gap
            radius: ui.radius
            descriptionFont: ui.description
            originalText: root.markup(ctl.original)
            originalFormat: root.format(ctl.original)
            translationText: root.markup(ctl.translation)
            translationFormat: root.format(ctl.translation)
            textColor: root.mainTextColor
            panelColor: root.mainBackground
            fontFamily: root.mainFamily
            fontSize: root.mainFontSize
        }
        // ── Bottom line: a short English message about the current action or event; the state light sits at its
        // right end. Nothing here can widen the window: the message is elided, its full text is in the tooltip.
        RowLayout {
            id: footer
            Layout.fillWidth: true
            spacing: ui.gap
            Label {
                id: statusLabel
                objectName: "statusMessage"
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                Layout.preferredWidth: 0
                readonly property string english: StatusText.english(ctl.status, ctl.statusKind)
                text: english
                textFormat: Text.PlainText
                elide: Text.ElideRight
                maximumLineCount: 1
                font.bold: ctl.statusKind === "error"
                Accessible.name: english
                HoverHint { control: statusLabel; feature: "Status"; explanation: statusLabel.english + (ctl.status !== statusLabel.english && ctl.status.length > 0 ? "\n\n" + ctl.status : "") }
            }
            Label {
                id: providerFallbackStatus
                visible: ["deepl", "microsoft"].includes(settingsWin.current.translation.service)
                text: (settingsWin.current.translation.service === "deepl" ? "DeepL" : "Microsoft") + " · резерв: Google"
                font: ui.description
                elide: Text.ElideRight
                Layout.maximumWidth: root.width / 3
                HoverHint { control: providerFallbackStatus; feature: "Резервный сервис"; explanation: "При ошибке запроса выбранного сервиса текст отправляется Google Translate. При отсутствующем API-ключе резерв не используется. Это настроенный порядок обработки, а не подтверждение использования резерва для текущего текста." }
            }
            ActionButton {
                visible: (ctl.statusKind === "error" || ctl.statusKind === "warning") && ctl.hasRegion
                text: "Повторить"
                helpText: "Повторяет неудавшееся распознавание и перевод; сбрасывает остановку автоматических повторов."
                onClicked: ctl.translateOnce()
            }
            StatusIndicator {
                id: stateLight
                objectName: "statusIndicator"
                Layout.alignment: Qt.AlignVCenter
                state: root.indicatorState
                reducedMotion: root.reducedMotion
                description: (root.indicatorState === "running" ? "Автоперевод работает"
                    : root.indicatorState === "processing" ? "Выполняется обработка (загрузка модели или подбор)"
                    : root.indicatorState === "error" ? "Ошибка: подробности в нижней строке и в журнале"
                    : "Автоперевод остановлен")
            }
        }
    }
}
