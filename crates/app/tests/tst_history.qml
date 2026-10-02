import QtQuick
import QtTest
import "../qml" as Lipa
TestCase {
    name: "HistoryWindow"
    when: windowShown
    QtObject {
        id: ctl
        property string historyJson: JSON.stringify([{timestamp: 1700000000000, region: "Субтитры", original: "Hello", translation: "Привет"}])
        property string copied: ""
        function copyText(text) { copied = text }
        function clearHistory() { historyJson = "[]" }
    }
    QtObject {
        id: form
        property var controller: ctl
        property var current: ({history_enabled: true, history_limit: 200})
        function set(key, value) { current = Object.assign({}, current, {[key]: value}) }
        function resetKeys(keys) { current = {history_enabled: true, history_persist: false, history_limit: 200} }
        function apply() {}
    }
    Lipa.HistoryWindow { id: history; settingsWindow: form }
    function visualChild(item, name) {
        if (item.objectName === name) return item
        for (const c of item.children || []) {
            const found = visualChild(c, name)
            if (found) return found
        }
        return null
    }
    function test_openCopyAndClear() {
        history.openWindow()
        wait(100)
        compare(history.history.length, 1)
        const copy = visualChild(history.contentItem, "copyTranslation")
        verify(copy !== null)
        copy.clicked()
        compare(ctl.copied, "Привет")
        findChild(history, "clearHistory").clicked()
        compare(history.history.length, 0)
        history.close()
    }
}
