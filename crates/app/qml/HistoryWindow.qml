import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Controls.Universal

ApplicationWindow {
    id: win
    UiTheme { id: ui; surface: win.palette.window; textScale: (win.current.general || {}).ui_text_scale || "normal" }
    font: ui.body
    property var settingsWindow
    Universal.theme: settingsWindow ? settingsWindow.universalTheme : Universal.Dark
    property var controller: settingsWindow ? settingsWindow.controller : null
    readonly property var current: settingsWindow ? settingsWindow.current : ({})
    readonly property var history: { try { return JSON.parse(controller.historyJson || "[]") } catch (e) { return [] } }
    title: qsTr("История переводов — LipaX")
    width: 820; height: 600
    minimumWidth: 680; minimumHeight: 440
    visible: false
    function openWindow() { show(); raise(); requestActivate() }
    function set(key, value) { settingsWindow.set(key, value) }
    function resetKeys(keys) { settingsWindow.resetKeys(keys) }
    onClosing: settingsWindow.apply()
    component FieldLabel: Label { Layout.preferredWidth: 230; wrapMode: Text.Wrap }
    component ResetButton: Button {
        property var keys: []
        Layout.columnSpan: 2
        text: qsTr("Сбросить настройки истории")
        onClicked: win.resetKeys(keys)
    }
        ScrollView {
            id: page5
            objectName: "settingsPage5"
            anchors.fill: parent
            anchors.margins: 16
            contentWidth: availableWidth
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            GridLayout {
                width: page5.availableWidth - 16
                columns: 2
                columnSpacing: 24
                rowSpacing: 12
            FieldLabel { text: qsTr("Вести историю") }
            Switch { checked: win.current.history_enabled !== false; onToggled: win.set("history_enabled", checked) }
            FieldLabel { text: qsTr("Сохранять историю между запусками") }
            Switch { checked: win.current.history_persist === true; onToggled: win.set("history_persist", checked) }
            FieldLabel { text: qsTr("Хранить записей") }
            SpinBox {
                from: 10; to: 1000; stepSize: 10; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                value: win.current.history_limit || 200
                onValueModified: win.set("history_limit", value)
            }
            RowLayout {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0
                Label { text: qsTr("Записей: ") + win.history.length; Layout.fillWidth: true; opacity: 0.7 }
                Button { objectName: "clearHistory"; id: clearTranslationHistory; Accessible.description: clearTranslationHistoryHint.accessibleExplanation; UnavailableHint { id: clearTranslationHistoryHint; control: clearTranslationHistory; feature: qsTr("Очистить историю переводов"); reason: qsTr("История переводов пуста."); remedy: qsTr("Записи появятся после перевода текста.") } text: qsTr("Очистить историю"); enabled: win.history.length > 0; onClicked: win.controller.clearHistory() }
            }
            ResetButton { keys: ["history_enabled", "history_persist", "history_limit"] }
            Label {
                Layout.columnSpan: 2; visible: win.history.length === 0; opacity: 0.6
                text: qsTr("История пуста")
            }
            Repeater {
                // Newest first; long histories render the most recent 200 entries.
                model: win.history.slice(-200).reverse()
                delegate: Frame {
                    required property var modelData
                    Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0
                    ColumnLayout {
                        width: parent.width
                        spacing: 4
                        RowLayout {
                            Layout.fillWidth: true
                            Label {
                                Layout.fillWidth: true; Layout.minimumWidth: 0; elide: Text.ElideRight; opacity: 0.6; font.pointSize: ui.description.pointSize
                                text: new Date(modelData.timestamp).toLocaleString(Qt.locale(), "dd.MM HH:mm:ss") + " · " + modelData.region
                            }
                            Button { objectName: "copyTranslation"; text: qsTr("Копировать"); flat: true; onClicked: win.controller.copyText(modelData.translation) }
                            Button { text: qsTr("Оригинал"); flat: true; onClicked: win.controller.copyText(modelData.original) }
                        }
                        Label { Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; opacity: 0.7; text: modelData.original }
                        Label { Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; text: modelData.translation }
                    }
                }
            }
            }
        }
}
