import QtQuick

// Shared by the game surface and settings preview; Rust supplies the resolved entry.
Item {
    id: content
    property var entry: null
    readonly property var bg: entry ? entry.background : ({ mode: "transparent", color: "#000000", image: "" })

    Item {
        id: backgroundItem
        objectName: "inplaceBackground"
        anchors.fill: parent
        clip: true
        visible: content.bg.mode !== "transparent"
        // Inpaint + blur: a small reconstructed image stretched with smoothing.
        Image {
            objectName: "inplaceBackdrop"
            anchors.fill: parent
            visible: content.bg.mode === "inpaint_blur" && source.toString().length > 0
            source: content.bg.image || ""
            fillMode: Image.Stretch
            smooth: true
            cache: false
            opacity: content.bg.opacity === undefined ? 1 : content.bg.opacity
        }
        // Solid / adaptive padding fill: the colour sampled around the original glyphs.
        Rectangle {
            objectName: "inplaceFill"
            anchors.fill: parent
            visible: content.bg.mode === "solid_fill" || content.bg.mode === "adaptive_padding_fill"
            radius: Math.min(content.bg.radius === undefined ? 4 : content.bg.radius, height / 2)
            color: content.bg.color
            opacity: content.bg.opacity === undefined ? 1 : content.bg.opacity
        }
    }

    Text {
        anchors.fill: textItem
        anchors.leftMargin: 2
        anchors.topMargin: 2
        visible: !!(content.entry && content.entry.shadow)
        text: textItem.text
        font: textItem.font
        lineHeight: textItem.lineHeight
        wrapMode: textItem.wrapMode
        horizontalAlignment: textItem.horizontalAlignment
        verticalAlignment: textItem.verticalAlignment
        color: content.entry ? content.entry.outline_color : "#000000"
        opacity: 0.65
        clip: true
    }

    Repeater {
        model: 8
        delegate: Text {
            readonly property real distance: Math.max(1, Math.min(8, content.entry ? content.entry.outline_width || 1 : 1))
            readonly property var dx: [-1, 0, 1, -1, 1, -1, 0, 1]
            readonly property var dy: [-1, -1, -1, 0, 0, 1, 1, 1]
            x: textItem.x + dx[index] * distance
            y: textItem.y + dy[index] * distance
            width: textItem.width
            height: textItem.height
            visible: !!(content.entry && content.entry.outline && content.entry.outline_width > 1)
            text: textItem.text
            font: textItem.font
            lineHeight: textItem.lineHeight
            wrapMode: textItem.wrapMode
            horizontalAlignment: textItem.horizontalAlignment
            verticalAlignment: textItem.verticalAlignment
            color: content.entry ? content.entry.outline_color : "#000000"
            opacity: content.entry && content.entry.text_opacity !== undefined ? content.entry.text_opacity : 1
            clip: true
        }
    }

    Text {
        id: textItem
        objectName: "inplaceText"
        readonly property var inner: content.entry ? content.entry.inner : [0, 0, 0, 0]
        x: inner[0]
        y: inner[1]
        width: Math.max(1, parent.width - inner[0] - inner[2])
        height: Math.max(1, parent.height - inner[1] - inner[3])
        text: content.entry ? content.entry.text : ""
        textFormat: Text.PlainText
        color: content.entry ? content.entry.text_color : "#ffffff"
        opacity: content.entry && content.entry.text_opacity !== undefined ? content.entry.text_opacity : 1
        font.family: content.entry ? content.entry.font_family : Qt.application.font.family
        font.pixelSize: content.entry ? content.entry.font_px : 16
        font.weight: content.entry ? content.entry.font_weight : Font.Normal
        font.italic: !!(content.entry && content.entry.italic)
        font.letterSpacing: content.entry ? content.entry.letter_spacing : 0
        lineHeightMode: Text.ProportionalHeight
        lineHeight: content.entry ? content.entry.line_height : 1.0
        wrapMode: !content.entry ? Text.Wrap
            : content.entry.wrap === "anywhere" ? Text.WrapAnywhere
            : content.entry.wrap === "none" ? Text.NoWrap : Text.Wrap
        // Explicit overflow fallback: never drawn outside the field.
        elide: Text.ElideRight
        maximumLineCount: content.entry && content.entry.wrap === "elide" ? content.entry.max_lines : 10000
        clip: true
        horizontalAlignment: !content.entry ? Text.AlignHCenter
            : content.entry.alignment === "left" ? Text.AlignLeft
            : content.entry.alignment === "right" ? Text.AlignRight : Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        style: content.entry && content.entry.outline && (content.entry.outline_width === undefined || content.entry.outline_width <= 1) ? Text.Outline : Text.Normal
        styleColor: content.entry ? content.entry.outline_color : "#000000"
    }

}
