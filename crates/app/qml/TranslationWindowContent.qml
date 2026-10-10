import QtQuick

// Content of the translation window, shared by the pinned (layer-shell) and the floating
// (xdg_toplevel) surface. It never positions the window: it only draws and reports input.
//   LMB (floating) → moveRequested()   — the surface starts a native compositor move
//   MMB            → pinToggleRequested()
//   ✕ (floating)   → closeRequested()
Item {
    id: content
    property var settings: ({capture: {}, recognition: {}, translation: {}, translation_window: {}, appearance: {window: {}, inplace: {}}})
    property string translation: ""
    property string original: ""
    property bool pinned: false
    property bool previewOnly: false
    signal moveRequested()
    signal pinToggleRequested()
    signal closeRequested()

    readonly property bool floating: !pinned
    // An free window always shows its frame so it is clear that it can be dragged.
    readonly property bool frameVisible: floating || settings.translation_window.border_always !== false || frameTimer.running
    function flashFrame() { frameTimer.interval = Math.max(1, settings.translation_window.border_seconds || 5) * 1000; frameTimer.restart() }
    Timer { id: frameTimer }

    readonly property int frameWidth: Math.max(1, settings.translation_window.border_width || 2) + (pinned ? 0 : 2)
    readonly property int padding: (settings.appearance.window.padding !== undefined ? settings.appearance.window.padding : 16) + frameWidth
    readonly property int cornerRadius: pinned
        ? (settings.translation_window.pinned_corner_radius !== undefined ? settings.translation_window.pinned_corner_radius : 0)
        : (settings.translation_window.corner_radius !== undefined ? settings.translation_window.corner_radius : 12)
    readonly property int requestedFontSize: settings.appearance.window.font_size || 20
    readonly property bool autoShrink: settings.translation_window.auto_shrink !== false
    readonly property bool wrapOverflow: autoShrink && settings.appearance.window.text_wrap === false
        && fitProbe.fontInfo.pixelSize <= 14 && fitProbe.contentWidth > textScroll.width
    onTranslationChanged: textScroll.contentY = 0

    // ── Display style: blur / transparent / dim (or its light inverse) / solid ──
    readonly property string style: settings.appearance.window.background_style || "solid"
    readonly property bool inverse: style === "dim" && settings.appearance.window.dim_inverse === true
    // The dim style relies on the same compositor blur, under a denser tint so it reads as faint.
    readonly property bool blurBehind: (style === "blur" && settings.appearance.window.blur_enabled !== false) || style === "dim"
    readonly property color backgroundColor: style === "solid" ? (settings.appearance.window.background_color || "#181818")
        : inverse ? "#f2f2f2" : "#000000"
    readonly property real backgroundOpacity: style === "solid" ? (settings.translation_window.opacity !== undefined ? settings.translation_window.opacity : 0.85)
        : style === "blur" ? (settings.appearance.window.blur_tint !== undefined ? settings.appearance.window.blur_tint : 0.3)
        : style === "dim" ? (inverse ? 0.72 : 0.55)
        : 0
    readonly property color textColor: style === "solid" ? (settings.appearance.window.text_color || "#ffffff") : inverse ? "#141414" : "#ffffff"
    readonly property color originalColor: style === "solid" ? (settings.appearance.window.original_color || "#b0b0b0") : inverse ? "#4a4a4a" : "#d0d0d0"
    readonly property bool outlined: (style === "solid" || style === "blur") && settings.appearance.window.text_outline !== false

    // ── Input: the pin handle always receives input; the frame band only while visible ──
    readonly property int handleSize: 26
    readonly property int handleInset: 2 + Math.round(cornerRadius / 3)
    readonly property var handleRect: [width - handleSize - handleInset, handleInset, handleSize, handleSize]
    function inputRects() {
        const rects = [handleRect]
        if (frameVisible) {
            const band = Math.max(frameWidth, 10)
            rects.push([0, 0, width, band], [0, height - band, width, band],
                       [0, band, band, height - 2 * band], [width - band, band, band, height - 2 * band])
        }
        return rects
    }

    Rectangle {
        objectName: "translationWindowBackground"
        anchors.fill: parent
        radius: content.cornerRadius
        color: content.backgroundColor
        opacity: content.backgroundOpacity
    }
    Rectangle {
        objectName: "translationWindowBorder"
        anchors.fill: parent
        color: "transparent"
        visible: content.frameVisible && content.settings.translation_window.border_pattern !== true
        radius: content.cornerRadius
        border.width: content.frameWidth
        border.color: content.settings.translation_window.border_color || "#ff00ff"
        opacity: content.settings.translation_window.border_opacity !== undefined ? content.settings.translation_window.border_opacity : 0.65
    }
    ErrorPattern {
        anchors.fill: parent
        visible: content.frameVisible && content.settings.translation_window.border_pattern === true
        band: content.frameWidth
        radius: content.cornerRadius
        color: content.settings.translation_window.border_color || "#ff00ff"
        opacity: content.settings.translation_window.border_opacity !== undefined ? content.settings.translation_window.border_opacity : 0.65
    }
    // A bounded measurement item lets Qt choose the largest real-font size that fits.
    // The visible Text keeps its natural height, so overflow at 14 px remains scrollable.
    Text {
        id: fitProbe
        objectName: "translationWindowFitProbe"
        visible: false
        width: textScroll.width
        height: Math.max(1, textScroll.height - (originalText.visible ? originalText.implicitHeight + 4 : 0))
        text: content.translation
        textFormat: Text.PlainText
        wrapMode: content.settings.appearance.window.text_wrap !== false ? Text.Wrap : Text.NoWrap
        font.family: content.settings.appearance.window.font_family || "Inter"
        font.pixelSize: content.requestedFontSize
        font.bold: content.settings.appearance.window.font_bold === true
        font.italic: content.settings.appearance.window.font_italic === true
        lineHeight: content.settings.appearance.window.line_spacing || 1.0
        fontSizeMode: content.autoShrink ? Text.Fit : Text.FixedSize
        minimumPixelSize: Math.min(14, content.requestedFontSize)
    }
    Flickable {
        id: textScroll
        objectName: "translationWindowTextScroll"
        anchors.fill: parent
        anchors.margins: content.padding
        clip: true
        contentWidth: width
        contentHeight: Math.max(height, textColumn.implicitHeight)
        interactive: false
        boundsBehavior: Flickable.StopAtBounds
        Column {
            id: textColumn
            width: parent.width
            y: Math.max(0, (textScroll.height - implicitHeight) / 2)
            spacing: 4
            Text {
                id: originalText
                width: parent.width
                visible: content.settings.appearance.window.show_original === true && text.length > 0
                text: content.original
                textFormat: Text.PlainText
                color: content.originalColor
                wrapMode: translatedText.wrapMode
                elide: translatedText.elide
                maximumLineCount: content.settings.appearance.window.text_wrap === false ? 1 : 1000
                horizontalAlignment: translatedText.horizontalAlignment
                font.family: content.settings.appearance.window.original_font_family || translatedText.font.family
                font.pixelSize: content.settings.appearance.window.original_font_size || 14
                style: translatedText.style
                styleColor: translatedText.styleColor
            }
            Text {
                id: translatedText
                objectName: "translatedText"
                width: parent.width
                text: content.translation
                textFormat: Text.PlainText
                color: content.textColor
                wrapMode: content.settings.appearance.window.text_wrap !== false || content.wrapOverflow ? Text.Wrap : Text.NoWrap
                elide: content.autoShrink ? Text.ElideNone
                    : content.settings.appearance.window.text_wrap === false ? Text.ElideRight : Text.ElideNone
                lineHeight: content.settings.appearance.window.line_spacing || 1.0
                horizontalAlignment: content.settings.appearance.window.text_alignment === "left" ? Text.AlignLeft : content.settings.appearance.window.text_alignment === "right" ? Text.AlignRight : Text.AlignHCenter
                font.family: content.settings.appearance.window.font_family || "Inter"
                font.pixelSize: content.autoShrink ? fitProbe.fontInfo.pixelSize : content.requestedFontSize
                font.bold: content.settings.appearance.window.font_bold === true
                font.italic: content.settings.appearance.window.font_italic === true
                // Transparent style: a light shadow (no shaders, so it also renders without GPU).
                style: content.outlined ? Text.Outline : content.style === "transparent" ? Text.Raised : Text.Normal
                styleColor: content.style === "transparent" ? "#b0000000" : (content.settings.appearance.window.outline_color || "#000000")
            }
        }
    }
    // Pin handle: stays visible in every frame mode, so there is always a target for MMB.
    Rectangle {
        id: pinHandle
        visible: !content.previewOnly
        objectName: "pinHandle"
        x: content.handleRect[0]; y: content.handleRect[1]
        width: content.handleSize; height: content.handleSize; radius: width / 2
        color: "#80000000"
        border.width: 1
        border.color: content.settings.translation_window.border_color || "#ff00ff"
        opacity: content.pinned && !content.frameVisible ? 0.55 : 1
        Text {
            anchors.centerIn: parent
            text: content.pinned ? "●" : "✥"
            color: content.settings.translation_window.border_color || "#ff00ff"
            font.pixelSize: 14
        }
    }
    HoverHint { control: pinHandle; feature: content.pinned ? "Открепить окно" : "Закрепить окно"; explanation: "Средняя кнопка мыши переключает закрепление. Свободное окно можно перемещать мышью; закреплённое остаётся поверх игры." }
    HoverHint { control: closeButton; feature: "Скрыть перевод"; explanation: "Скрывает окно перевода. Распознавание продолжается; показать окно можно из главного окна или сочетанием клавиш." }
    Text {
        anchors.bottom: parent.bottom; anchors.horizontalCenter: parent.horizontalCenter
        text: "⌄"; color: content.settings.appearance.window.text_color || "white"
        visible: textScroll.contentHeight > textScroll.height + 1
    }
    MouseArea {
        id: dragArea
        objectName: "translationWindowDragArea"
        anchors.fill: parent
        enabled: !content.previewOnly
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton
        cursorShape: content.pinned ? Qt.ArrowCursor : Qt.SizeAllCursor
        onPressed: (m) => {
            if (m.button === Qt.MiddleButton) content.pinToggleRequested()
            // Pinned: no LMB dragging. Floating: the compositor moves the window.
            else if (content.floating) content.moveRequested()
        }
        onWheel: (wheel) => {
            if (content.floating) {
                textScroll.contentY = Math.max(0, Math.min(textScroll.contentY - wheel.angleDelta.y,
                                                             textScroll.contentHeight - textScroll.height))
                wheel.accepted = true
            } else wheel.accepted = false
        }
    }
    // Close button of the floating window; above the drag area so it gets the click.
    Rectangle {
        id: closeButton
        objectName: "closeButton"
        visible: content.floating && !content.previewOnly
        x: content.handleRect[0] - width - 4; y: content.handleRect[1]
        width: content.handleSize; height: content.handleSize; radius: width / 2
        color: closeArea.containsMouse ? "#c0d03030" : "#80000000"
        border.width: 1
        border.color: content.settings.translation_window.border_color || "#ff00ff"
        Text { anchors.centerIn: parent; text: "✕"; color: "#ffffff"; font.pixelSize: 13 }
        MouseArea {
            id: closeArea
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton
            onClicked: content.closeRequested()
        }
    }
}
