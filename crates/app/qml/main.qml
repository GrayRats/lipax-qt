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

    Controller {
        id: ctl
        onFrameRequested: (x, y, w, h) => selectionFrame.flash(x, y, w, h)
        onSelectRegionRequested: if (ctl.windowTitle.length > 0) regionWin.begin()
        onToggleOverlayRequested: overlayEnabled.checked = !overlayEnabled.checked
        onTogglePinRequested: overlay.pinToggled(!overlay.pinned)
    }

    SettingsWindow { id: settingsWin; controller: ctl; onSelectRegionRequested: regionWin.begin() }

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
        visible: overlayEnabled.checked && ctl.translation.length > 0 && !relocating
        settings: settingsWin.current
        onMoved: (x, y) => { settingsWin.set("overlay_pos", [x, y]); settingsWin.apply() }
        onPinToggled: (p) => { settingsWin.set("overlay_pinned", p); settingsWin.apply() }
        // Closing the floating window hides the translation; "Поверх игры" or Ctrl+Alt+H shows it again.
        onCloseRequested: overlayEnabled.checked = false
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
            Button {
                text: "Перевести сейчас"
                enabled: ctl.hasRegion
                onClicked: ctl.translateOnce()
            }
            CheckBox { id: overlayEnabled; text: "Поверх игры"; checked: true }
            Item { Layout.fillWidth: true }
            Button { text: "Настройки"; onClicked: settingsWin.openWindow() }
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
