import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: win
    property var settingsWindow
    property var controller: settingsWindow ? settingsWindow.controller : null
    readonly property var current: settingsWindow ? settingsWindow.current : ({})
    readonly property var history: { try { return JSON.parse(controller.historyJson || "[]") } catch (e) { return [] } }
    title: "История переводов — LipaX"
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
        text: "Сбросить настройки истории"
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
            FieldLabel { text: "Вести историю" }
            Switch { checked: win.current.history_enabled !== false; onToggled: win.set("history_enabled", checked) }
            FieldLabel { text: "Сохранять историю между запусками" }
            Switch { checked: win.current.history_persist === true; onToggled: win.set("history_persist", checked) }
            FieldLabel { text: "Хранить записей" }
            SpinBox {
                from: 10; to: 1000; stepSize: 10; editable: true; Layout.fillWidth: true; Layout.minimumWidth: 0
                value: win.current.history_limit || 200
                onValueModified: win.set("history_limit", value)
            }
            RowLayout {
                Layout.columnSpan: 2; Layout.fillWidth: true; Layout.minimumWidth: 0
                Label { text: "Записей: " + win.history.length; Layout.fillWidth: true; opacity: 0.7 }
                Button { objectName: "clearHistory"; id: clearTranslationHistory; Accessible.description: clearTranslationHistoryHint.explanation; UnavailableHint { id: clearTranslationHistoryHint; control: clearTranslationHistory; feature: "Очистить историю переводов"; reason: "История переводов пуста."; remedy: "Записи появятся после перевода текста." } text: "Очистить историю"; enabled: win.history.length > 0; onClicked: win.controller.clearHistory() }
            }
            ResetButton { keys: ["history_enabled", "history_persist", "history_limit"] }
            Label {
                Layout.columnSpan: 2; visible: win.history.length === 0; opacity: 0.6
                text: "История пуста"
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
                                Layout.fillWidth: true; Layout.minimumWidth: 0; elide: Text.ElideRight; opacity: 0.6; font.pixelSize: 12
                                text: new Date(modelData.timestamp).toLocaleString(Qt.locale(), "dd.MM HH:mm:ss") + " · " + modelData.region
                            }
                            Button { objectName: "copyTranslation"; text: "Копировать"; flat: true; onClicked: win.controller.copyText(modelData.translation) }
                            Button { text: "Оригинал"; flat: true; onClicked: win.controller.copyText(modelData.original) }
                        }
                        Label { Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; opacity: 0.7; text: modelData.original }
                        Label { Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; text: modelData.translation }
                    }
                }
            }
            }
        }
}
