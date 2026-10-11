import QtQuick
import QtQuick.Controls

// SVGs use Qt Quick Controls' palette-aware icon tint, including disabled/highlighted states.
Button {
    id: button
    property string helpText: ""
    property string iconFile: ""
    // Only the translation action opts into this explicit runtime indicator.
    property string activity: ""
    property real pulse: 0
    UiTheme { id: metrics; surface: button.palette.window }
    readonly property color runningColor: metrics.success
    readonly property color stoppedColor: metrics.warning
    SequentialAnimation {
        running: button.activity === "running" && button.visible && button.Window.window && button.Window.window.visible
        loops: Animation.Infinite
        onRunningChanged: if (!running) button.pulse = 0
        NumberAnimation { target: button; property: "pulse"; from: 0; to: 1; duration: 750; easing.type: Easing.InOutSine }
        NumberAnimation { target: button; property: "pulse"; from: 1; to: 0; duration: 750; easing.type: Easing.InOutSine }
    }
    display: iconFile.length ? AbstractButton.IconOnly : AbstractButton.TextOnly
    icon.source: iconFile.length ? (Qt.resolvedUrl(".").toString().startsWith("qrc:")
        ? "qrc:/lipa/icons/" + iconFile : "../assets/" + iconFile) : ""
    icon.width: metrics.iconSize
    icon.height: metrics.iconSize
    icon.color: activity === "running" ? Qt.rgba(runningColor.r, runningColor.g, runningColor.b, 0.45 + pulse * 0.55)
        : activity === "stopped" ? stoppedColor : highlighted ? palette.highlightedText : palette.buttonText
    implicitHeight: Math.max(metrics.iconButtonSize, implicitContentHeight + topPadding + bottomPadding)
    implicitWidth: iconFile.length ? implicitHeight : implicitContentWidth + leftPadding + rightPadding
    // Icon buttons share one look: transparent until hovered, a soft fill when pressed or checked, a ring for
    // keyboard focus and for the checked state. Text buttons get a plain framed background of the same family.
    readonly property bool iconOnly: iconFile.length > 0
    opacity: enabled || !iconOnly ? 1 : 0.45
    background: Rectangle {
        radius: metrics.radius
        color: button.down ? Qt.alpha(button.palette.buttonText, 0.2)
            : button.checked ? Qt.alpha(button.palette.highlight, 0.28)
            : button.hovered ? Qt.alpha(button.palette.buttonText, button.iconOnly ? 0.1 : 0.06)
            : button.iconOnly ? "transparent" : button.palette.button
        border.width: button.visualFocus ? 2 : button.checked || !button.iconOnly ? 1 : 0
        border.color: button.visualFocus ? button.palette.highlight
            : button.checked ? Qt.alpha(button.palette.highlight, 0.7) : Qt.alpha(button.palette.buttonText, 0.3)
    }
    Accessible.role: Accessible.Button
    Accessible.name: text
    Accessible.description: helpText
    Accessible.checkable: checkable
    Accessible.checked: checked
    Accessible.focusable: true
    HoverHint {
        control: button
        feature: button.text
        explanation: button.text + (button.checkable ? (button.checked ? " — включено" : " — выключено") : "")
            + (button.helpText.length > 0 ? "\n\n" + button.helpText : "")
        active: (button.helpText.length > 0 || button.iconOnly) && (button.enabled || button.iconOnly)
    }
}
