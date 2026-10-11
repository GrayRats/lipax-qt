import QtQuick
import QtQuick.Controls

// The selected process, underlined (a text decoration: the name itself is untouched). One line: the font shrinks
// step by step down to `minimumPixelSize` (8 px) to fit; if it still does not fit the text is elided on the right
// and the tooltip carries the full name. Never asks its parent for more width than it is given.
Item {
    id: root
    property string text: ""
    property string tooltip: text
    // Underline only a real process name, not the "select a window" placeholder.
    property bool underline: true
    // Show the tooltip on every hover, not only when the text was shrunk or cut.
    property bool alwaysHint: false
    property int basePixelSize: 16
    property int minimumPixelSize: 8
    readonly property int fittedPixelSize: fit(width, text, basePixelSize)
    readonly property bool shrunk: fittedPixelSize < basePixelSize
    readonly property bool elided: label.truncated
    property font baseFont
    implicitWidth: 0
    clip: true
    // The text at the base size; text width is proportional to the size, so the largest size that fits follows from
    // one measurement (no measuring inside a binding). Rounded down: a slightly small title beats an elided one.
    TextMetrics {
        id: baseMetrics
        font: Qt.font({family: root.baseFont.family, pixelSize: root.basePixelSize, weight: root.baseFont.weight, underline: root.underline})
        text: root.text
    }
    function fit(available, string, base) {
        const natural = baseMetrics.advanceWidth
        if (available <= 0 || string.length === 0 || natural <= available) return base
        return Math.max(minimumPixelSize, Math.min(base, Math.floor(base * available / natural)))
    }
    readonly property font fittedFont: Qt.font({family: baseFont.family, pixelSize: fittedPixelSize, weight: baseFont.weight, underline: underline})
    implicitHeight: baseMetrics.height
    Label {
        id: label
        width: parent.width
        anchors.verticalCenter: parent.verticalCenter
        text: root.text
        font: root.fittedFont
        elide: Text.ElideRight
        maximumLineCount: 1
        wrapMode: Text.NoWrap
        Accessible.name: root.tooltip
    }
    HoverHint { control: root; feature: root.text; explanation: root.tooltip; active: root.text.length > 0 && (root.alwaysHint || root.shrunk || root.elided) }
}
