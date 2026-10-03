import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import io.lipa

ApplicationWindow {
    id: root
    visible: true
    width: 820
    height: 620
    minimumWidth: 720
    minimumHeight: 520
    title: "LipaX — переводчик для игр"

    HistoryWindow { id: historyWin; settingsWindow: settingsWin }

    Controller {
        id: ctl
        onFrameRequested: (x, y, w, h) => selectionFrame.flash(x, y, w, h)
        onSelectRegionRequested: if (ctl.windowTitle.length > 0) regionWin.begin()
        onTogglePinRequested: ctl.setOverlayPinned(!overlay.pinned, "hotkey")
    }

    SettingsWindow { id: settingsWin; controller: ctl; onSelectRegionRequested: regionWin.begin() }

    // The two renderers have separate state owned by the Controller: which one runs
    // (effectiveDisplay, with a logged fallback for portal capture) and their own visibility.
    readonly property bool inplaceActive: ctl.effectiveDisplay === "inplace"
    readonly property bool windowActive: ctl.effectiveDisplay === "window"
    readonly property var inplaceEntries: { try { return JSON.parse(ctl.inplaceJson || "[]") } catch (e) { return [] } }
    // Keyed by "region:field": a stable field keeps its window; only its properties update.
    readonly property string inplaceKeys: JSON.stringify(inplaceEntries.map(e => e.key))
    Instantiator {
        model: JSON.parse(root.inplaceKeys)
        delegate: InplaceText {
            required property string modelData
            settings: settingsWin.current
            entry: root.inplaceEntries.find(e => e.key === modelData) || null
            gameGeometry: ctl.gameGeometry
            visible: root.inplaceActive && ctl.inplaceVisible && !relocating && !!desktopRect && !!entry && entry.text.length > 0
            // MMB hides only the in-place translation; the translation window is untouched.
            onHideRequested: ctl.setInplaceVisibility(false, "mmb")
        }
    }

    // One outline per active region with an area; each has its own timer.
    property bool started: false
    Timer { interval: 1500; running: true; onTriggered: root.started = true }
    readonly property string frameRegionIds:
        JSON.stringify((settingsWin.current.regions || []).filter(r => r.enabled && r.rect).map(r => r.id))
    Instantiator {
        model: JSON.parse(root.frameRegionIds)
        delegate: RegionFrame {
            required property string modelData
            settings: settingsWin.current
            region: (settingsWin.current.regions || []).find(r => r.id === modelData) || null
            gameGeometry: ctl.gameGeometry
            pinned: settingsWin.current.overlay_pinned === true
            flashOnCreate: root.started
        }
    }
    RegionSelector { id: regionWin; controller: ctl }
    FrameOverlay { id: selectionFrame; settings: settingsWin.current }
    TranslationOverlay {
        id: overlay
        controller: ctl
        translation: ctl.translation
        original: ctl.original
        gameGeometry: ctl.gameGeometry
        shown: root.windowActive && ctl.windowOverlayVisible && ctl.translation.length > 0
        settings: settingsWin.current
        // Pin state and the pinned placement are decided in Rust (pinned lands where floating was).
        onPinToggled: (p) => ctl.setOverlayPinned(p, "mmb")
        // Closing hides only the translation window; "Поверх игры" or Ctrl+Alt+H shows it again.
        onCloseRequested: ctl.setWindowOverlayVisibility(false, "close_button")
        // Any change of the selected areas (KWin or portal) briefly reveals a hidden frame.
        readonly property string areasKey: JSON.stringify((settingsWin.current.regions || []).map(r => [r.enabled, r.rect]))
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
                text: "Выбрать окно"
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
                    readonly property var regions: settingsWin.current.regions || []
                    model: regions.map(r => (r.rect ? "" : "○ ") + r.name + (r.enabled ? "" : " (выкл.)"))
                    currentIndex: Math.max(0, regions.findIndex(r => r.id === settingsWin.current.active_region))
                    // Choosing a region activates it (and, by default, deactivates the others).
                    onActivated: { settingsWin.activateRegion(currentIndex, true); settingsWin.apply() }
                }
                Button {
                    text: "Выбрать область"
                    Layout.fillWidth: true
                    enabled: ctl.windowTitle.length > 0
                    onClicked: regionWin.begin()
                }
            }
            RowLayout {
                Label { text: ctl.hasRegion ? "область задана" : "область не задана"; Layout.fillWidth: true }
                Button { text: "Сбросить"; enabled: ctl.hasRegion; onClicked: ctl.resetRegion() }
            }
        }

        RowLayout {
            Button {
                text: ctl.running ? "Остановить" : "Запустить"
                highlighted: ctl.running
                enabled: ctl.hasRegion
                onClicked: ctl.running ? ctl.stop() : ctl.start()
            }
            // Each renderer has its own switch; only the active one is shown.
            CheckBox {
                objectName: "windowOverlayToggle"
                visible: root.windowActive
                text: "Поверх игры"
                checked: ctl.windowOverlayVisible
                onToggled: ctl.setWindowOverlayVisibility(checked, "checkbox")
            }
            CheckBox {
                objectName: "inplaceToggle"
                visible: root.inplaceActive
                text: "Перевод поверх оригинала"
                checked: ctl.inplaceVisible
                onToggled: ctl.setInplaceVisibility(checked, "checkbox")
            }
            Item { Layout.fillWidth: true }
            Button { objectName: "historyButton"; text: "История"; onClicked: historyWin.openWindow() }
            Button { text: "Настройки"; onClicked: settingsWin.openWindow() }
        }

        // Why the chosen display was replaced (e.g. portal capture has no window position).
        Label {
            objectName: "displayNote"
            Layout.fillWidth: true
            visible: ctl.displayNote.length > 0
            text: "Перевод поверх оригинала недоступен: " + ctl.displayNote
            wrapMode: Text.Wrap
            color: "#ffc23d"
            font.pixelSize: 12
        }
        Label { text: "Оригинал"; font.bold: true }
        ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: 90
            TextArea { text: ctl.original; readOnly: true; wrapMode: Text.Wrap }
        }
        Label { text: "Перевод"; font.bold: true }
        ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            TextArea {
                text: ctl.translation
                readOnly: true
                wrapMode: Text.Wrap
                font.pixelSize: settingsWin.current.font_size || 20
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
