import QtQuick
import QtQuick.Controls
import QtQuick.Window

// A window-level companion of the disabled control: hover continues when that control
// cannot receive pointer events. This item never intercepts clicks or focus.
Item {
    id: hint
    required property Item control
    required property string feature
    required property string reason
    property string remedy: ""
    readonly property string explanation: feature + "\n\nФункция недоступна.\n\n" + reason + (remedy.length ? "\n\n" + remedy : "")
    parent: control.Window.window ? control.Window.window.contentItem : null
    property rect controlBounds: Qt.rect(0, 0, 0, 0)
    function updatePosition() {
        if (!parent) return;
        const position = control.mapToItem(parent, 0, 0);
        let left = position.x, top = position.y;
        let right = left + control.width, bottom = top + control.height;
        for (let item = control.parent; item; item = item.parent) {
            if (item.clip || item === parent) {
                const origin = item.mapToItem(parent, 0, 0);
                left = Math.max(left, origin.x); top = Math.max(top, origin.y);
                right = Math.min(right, origin.x + item.width); bottom = Math.min(bottom, origin.y + item.height);
            }
            if (item === parent) break;
        }
        controlBounds = Qt.rect(left, top, Math.max(0, right - left), Math.max(0, bottom - top));
    }
    Component.onCompleted: updatePosition()
    // Includes scrolling and ancestor layout changes without participating in their layout.
    Timer {
        interval: 30
        running: hint.control.visible && !hint.control.enabled
        repeat: true
        onTriggered: hint.updatePosition()
    }
    x: controlBounds.x
    y: controlBounds.y
    width: controlBounds.width
    height: controlBounds.height
    z: 10000
    visible: control.visible && !control.enabled && width > 0 && height > 0
    Accessible.role: Accessible.StaticText
    Accessible.name: feature
    Accessible.description: explanation
    HoverHandler {
        id: hover
    }
    readonly property bool tooltipVisible: visible && hover.hovered
    ToolTip.visible: tooltipVisible
    ToolTip.delay: 400
    ToolTip.text: explanation
}
