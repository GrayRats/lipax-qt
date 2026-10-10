import QtQuick
import QtQuick.Controls
import QtQuick.Window

// A window-level hover target also works for disabled controls and is clipped to
// the visible part of the control. It never intercepts clicks or focus.
Item {
    id: hint
    required property Item control
    required property string feature
    property bool active: true
    property string explanation: feature
    parent: control.Window.window ? control.Window.window.contentItem : null
    property bool moving: false
    Timer { id: settle; interval: 140; onTriggered: hint.moving = false }
    property rect controlBounds: Qt.rect(0, 0, 0, 0)
    function updatePosition() {
        if (!parent) return;
        const position = control.mapToItem(parent, 0, 0);
        let left = position.x, top = position.y;
        let right = left + control.width, bottom = top + control.height;
        for (let item = control.parent; item; item = item.parent) {
            if (!item.visible) { right = left; bottom = top; break; }
            if (item.clip || item === parent) {
                const origin = item.mapToItem(parent, 0, 0);
                left = Math.max(left, origin.x); top = Math.max(top, origin.y);
                right = Math.min(right, origin.x + item.width); bottom = Math.min(bottom, origin.y + item.height);
            }
            if (item === parent) break;
        }
        if (controlBounds.x !== left || controlBounds.y !== top || controlBounds.width !== Math.max(0, right - left) || controlBounds.height !== Math.max(0, bottom - top)) {
            moving = true; settle.restart();
        }
        controlBounds = Qt.rect(left, top, Math.max(0, right - left), Math.max(0, bottom - top));
    }
    Component.onCompleted: updatePosition()
    // Includes scrolling and ancestor layout changes without participating in their layout.
    Timer {
        interval: 30
        running: hint.control.visible && hint.active
        repeat: true
        onTriggered: hint.updatePosition()
    }
    x: controlBounds.x
    y: controlBounds.y
    width: controlBounds.width
    height: controlBounds.height
    z: 10000
    visible: control.visible && active && width > 0 && height > 0
    Accessible.role: Accessible.StaticText
    Accessible.name: feature
    Accessible.description: explanation
    HoverHandler {
        id: hover
    }
    readonly property bool tooltipVisible: visible && hover.hovered && !moving
    readonly property bool popupVisible: popup.visible
    ToolTip {
        id: popup
        objectName: hint.objectName + "Popup"
        visible: hint.tooltipVisible
        delay: 400
        timeout: -1
        text: hint.explanation
        font: hint.control.Window.window ? hint.control.Window.window.font : Qt.application.font
        margins: 8
        x: hint.parent ? Math.max(8 - hint.x, Math.min(0, hint.parent.width - hint.x - width - 8)) : 0
        y: hint.parent && hint.y + hint.height + height + 8 > hint.parent.height ? -height - 6 : hint.height + 6
        width: Math.min(420, implicitWidth, hint.parent ? hint.parent.width - 24 : 420)
        contentItem: Text {
            text: popup.text
            textFormat: Text.PlainText
            font: popup.font
            color: popup.palette.toolTipText
            wrapMode: Text.Wrap
        }
    }
}
