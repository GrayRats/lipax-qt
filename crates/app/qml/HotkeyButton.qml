import QtQuick
import QtQuick.Controls

// Поле захвата сочетания: нажмите кнопку, затем само сочетание. Esc — отмена, Backspace/Delete — очистить.
// Строка совпадает с форматом ядра («Ctrl+Alt+P»); без модификатора принимаются только F1–F24.
Button {
    id: btn
    property string value: ""
    property bool conflict: false
    signal edited(string value)

    text: capturing ? "Нажмите сочетание…" : (value.length ? value : "не назначено")
    property bool capturing: false
    highlighted: capturing
    palette.buttonText: conflict ? "#d9534f" : undefined
    focusPolicy: Qt.StrongFocus
    onClicked: { capturing = true; forceActiveFocus() }
    onActiveFocusChanged: if (!activeFocus) capturing = false

    function keyName(k) {
        if (k >= Qt.Key_A && k <= Qt.Key_Z) return String.fromCharCode(k)
        if (k >= Qt.Key_0 && k <= Qt.Key_9) return String.fromCharCode(k)
        if (k >= Qt.Key_F1 && k <= Qt.Key_F24) return "F" + (k - Qt.Key_F1 + 1)
        if (k === Qt.Key_Space) return "Space"
        if (k === Qt.Key_Tab) return "Tab"
        if (k === Qt.Key_Return || k === Qt.Key_Enter) return "Return"
        return ""
    }

    Keys.onPressed: (e) => {
        if (!capturing) return
        e.accepted = true
        if (e.key === Qt.Key_Escape) { capturing = false; return }
        if (e.key === Qt.Key_Backspace || e.key === Qt.Key_Delete) { capturing = false; edited(""); return }
        const name = keyName(e.key)
        if (name === "") return // отдельный модификатор или неподдерживаемая клавиша: ждём дальше
        const m = e.modifiers
        const mods = []
        if (m & Qt.ControlModifier) mods.push("Ctrl")
        if (m & Qt.AltModifier) mods.push("Alt")
        if (m & Qt.ShiftModifier) mods.push("Shift")
        if (m & Qt.MetaModifier) mods.push("Meta")
        if (mods.length === 0 && !name.startsWith("F")) return
        capturing = false
        edited(mods.concat([name]).join("+"))
    }
}
