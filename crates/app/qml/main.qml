import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import io.lipa

ApplicationWindow {
    id: root
    visible: true
    width: 560
    height: 440
    title: "LipaX — переводчик для игр"

    Controller {
        id: ctl
        onFrameRequested: (x, y, w, h) => selectionFrame.flash(x, y, w, h)
        onSelectRegionRequested: if (ctl.windowTitle.length > 0) regionWin.begin()
        onToggleOverlayRequested: overlayEnabled.checked = !overlayEnabled.checked
    }

    SettingsWindow { id: settingsWin; controller: ctl }
    RegionSelector { id: regionWin; controller: ctl }
    FrameOverlay { id: selectionFrame; settings: settingsWin.current }
    TranslationOverlay {
        id: overlay
        translation: ctl.translation
        visible: overlayEnabled.checked && ctl.translation.length > 0
        settings: settingsWin.current
        onMoved: (x, y) => { settingsWin.set("overlay_pos", [x, y]); settingsWin.apply() }
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
            Button {
                text: "Выбрать область перевода"
                Layout.fillWidth: true
                enabled: ctl.windowTitle.length > 0
                onClicked: regionWin.begin()
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
            CheckBox { id: overlayEnabled; text: "Overlay"; checked: true }
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
        Label { text: ctl.status; Layout.fillWidth: true; elide: Text.ElideRight; opacity: 0.7 }
    }
}
