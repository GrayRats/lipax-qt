import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Qt.labs.platform as Platform
import io.lipa

ApplicationWindow {
    id: root
    objectName: "mainWindow"
    visible: true
    width: 820
    height: 620
    minimumWidth: 720
    minimumHeight: 520
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
    OcrPreviewWindow { id: ocrWin; controller: ctl; universalTheme: settingsWin.universalTheme }

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
        tooltip: "LipaX — " + (ctl.running ? "автоперевод запущен" : "автоперевод остановлен")
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
        anchors.fill: parent
        anchors.margins: 16
        spacing: 10

        GridLayout {
            columns: 2
            columnSpacing: 8
            rowSpacing: 8
            Layout.fillWidth: true

            Button {
                text: "Выбрать окно для захвата"
                Layout.fillWidth: true
                onClicked: ctl.pickWindow()
            }
            Label {
                Layout.fillWidth: true
                elide: Text.ElideRight
                text: ctl.windowTitle.length ? ctl.windowTitle : "окно не выбрано"
            }
            RowLayout {
                Layout.fillWidth: true
                ComboBox {
                    id: regionBox
                    Layout.preferredWidth: 140
                    readonly property var regions: settingsWin.current.capture.regions || []
                    model: regions.map(r => (r.rect ? "" : "○ ") + r.name + (r.enabled ? "" : " (выкл.)"))
                    currentIndex: Math.max(0, regions.findIndex(r => r.id === settingsWin.current.capture.active_region))
                    // Choosing a region activates it (and, by default, deactivates the others).
                    onActivated: { settingsWin.activateRegion(currentIndex, true); settingsWin.apply() }
                }
                Button {
                    id: selectCaptureRegion
                    text: "Выбрать область захвата"
                    Layout.fillWidth: true
                    enabled: ctl.windowTitle.length > 0
                    onClicked: regionWin.begin()
                    Accessible.description: selectRegionHint.accessibleExplanation
                    UnavailableHint { id: selectRegionHint; control: selectCaptureRegion; feature: "Выбрать область захвата"; reason: "Окно для захвата ещё не выбрано."; remedy: "Выберите окно для захвата." }
                }
            }
            RowLayout {
                Label { text: ctl.hasRegion ? "область захвата задана" : "область захвата не задана"; Layout.fillWidth: true }
                Button {
                    id: resetCaptureRegion
                    text: "Сбросить область захвата"; enabled: ctl.hasRegion; onClicked: ctl.resetRegion()
                    Accessible.description: resetRegionHint.accessibleExplanation
                    UnavailableHint { id: resetRegionHint; control: resetCaptureRegion; feature: "Сбросить область захвата"; reason: "Область захвата ещё не задана."; remedy: "Сначала выделите область с текстом." }
                }
            }
        }

        RowLayout {
            Button {
                id: startTranslation
                text: ctl.running ? "Остановить" : "Запустить"
                highlighted: ctl.running
                enabled: ctl.hasRegion
                Accessible.description: startTranslationHint.accessibleExplanation
                UnavailableHint { id: startTranslationHint; control: startTranslation; feature: "Запустить автоперевод"; reason: "Нет выделенной области захвата."; remedy: "Выберите окно и область с текстом." }
                onClicked: ctl.running ? ctl.stop() : ctl.start()
            }
            // Each renderer has its own switch; only the active one is shown.
            CheckBox {
                objectName: "translationWindowToggle"
                visible: root.windowActive
                text: "Отдельное окно перевода"
                checked: ctl.translationWindowVisible
                onToggled: ctl.setTranslationWindowVisibility(checked, "checkbox")
            }
            CheckBox {
                objectName: "inplaceToggle"
                visible: root.inplaceActive
                text: "Поверх исходного текста"
                checked: ctl.inplaceVisible
                onToggled: ctl.setInplaceVisibility(checked, "checkbox")
            }
            Item { Layout.fillWidth: true }
            Button { objectName: "ocrPreviewButton"; text: "Предпросмотр распознавания"; onClicked: ocrWin.openWindow() }
            Button { objectName: "historyButton"; text: "История переводов"; onClicked: historyWin.openWindow() }
            Button { text: "Настройки"; onClicked: settingsWin.openWindow() }
        }

        // Why the chosen display was replaced (e.g. portal capture has no window position).
        Label {
            objectName: "displayNote"
            Layout.fillWidth: true
            visible: ctl.displayNote.length > 0
            text: "Поверх исходного текста недоступен: " + ctl.displayNote
            wrapMode: Text.Wrap
            color: "#ffc23d"
            font.pixelSize: 12
        }
        // Not every field fitted over the game: simplified ones and ones that went to the translation window.
        Label {
            objectName: "fallbackNote"
            Layout.fillWidth: true
            visible: root.inplaceActive && (root.degradedFields > 0 || root.droppedFields > 0)
            wrapMode: Text.Wrap
            color: "#ffc23d"
            font.pixelSize: 12
            text: "Не все поля удалось показать поверх исходного текста"
                + (root.degradedFields > 0 ? ": упрощено или обрезано — " + root.degradedFields : "")
                + (root.droppedFields > 0 ? (root.degradedFields > 0 ? ", " : ": ") + "не показано — " + root.droppedFields : "")
                + ". Причина — нет места без перекрытия соседнего текста; в окно перевода текст не переносится."
        }
        Label { text: "Оригинал"; font.bold: true }
        ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: 90
            TextArea {
                objectName: "mainOriginalText"
                text: root.markup(ctl.original)
                textFormat: root.format(ctl.original)
                readOnly: true
                wrapMode: Text.Wrap
                color: root.mainTextColor
                font.family: root.mainFamily
                font.pixelSize: root.mainFontSize
                background: Rectangle { color: root.mainBackground; radius: 3 }
            }
        }
        Label { text: "Перевод"; font.bold: true }
        ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            TextArea {
                objectName: "mainTranslationText"
                text: root.markup(ctl.translation)
                textFormat: root.format(ctl.translation)
                readOnly: true
                wrapMode: Text.Wrap
                color: root.mainTextColor
                font.family: root.mainFamily
                font.pixelSize: root.mainFontSize
                background: Rectangle { color: root.mainBackground; radius: 3 }
            }
        }
        // Compact status line: one elided row, full text in the tooltip.
        Rectangle {
            id: statusBar
            readonly property bool failed: ctl.statusKind === "error"
            readonly property bool warned: ctl.statusKind === "warning"
            Layout.fillWidth: true
            implicitHeight: 24
            radius: 3
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: statusBar.failed ? "#a01818" : statusBar.warned ? "#3a3000" : "transparent" }
                GradientStop { position: 1; color: statusBar.failed ? "#b88a00" : "transparent" }
            }
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 6
                anchors.rightMargin: 2
                spacing: 6
                Label {
                    id: statusLabel
                    Layout.fillWidth: true
                    text: ctl.status
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    maximumLineCount: 1
                    font.pixelSize: 12
                    color: statusBar.failed ? "#ffffff" : statusBar.warned ? "#ffc23d" : palette.text
                    opacity: statusBar.failed || statusBar.warned ? 1 : 0.6
                    HoverHandler { id: statusHover }
                    ToolTip.visible: statusHover.hovered && truncated
                    ToolTip.text: ctl.status
                }
                ToolButton {
                    visible: (statusBar.failed || statusBar.warned) && ctl.hasRegion
                    text: "Повторить"
                    font.pixelSize: 12
                    implicitHeight: 22
                    padding: 2
                    onClicked: ctl.translateOnce()
                }
            }
        }
    }
}
