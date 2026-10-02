import QtQuick

// Purple/black "missing texture" band along the edges (Source engine error look),
// shared by the translation frame and the region outlines.
Canvas {
    id: pattern
    property color color: "#ff00ff"
    property int band: 2
    property int tile: 12
    // Rounded outline: the band is clipped between two rounded rectangles.
    property real radius: 0
    onRadiusChanged: requestPaint()

    onColorChanged: requestPaint()
    onBandChanged: requestPaint()
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    onPaint: {
        const ctx = getContext("2d")
        ctx.reset()
        const b = Math.min(band, width / 2, height / 2)
        if (radius > 0) {
            ctx.beginPath()
            ctx.roundedRect(0, 0, width, height, radius, radius)
            const inner = Math.max(0, radius - b)
            ctx.roundedRect(b, b, width - 2 * b, height - 2 * b, inner, inner)
            ctx.fillRule = Qt.OddEvenFill
            ctx.clip()
        }
        ctx.fillStyle = "#000000"
        ctx.fillRect(0, 0, width, b); ctx.fillRect(0, height - b, width, b)
        ctx.fillRect(0, b, b, height - 2 * b); ctx.fillRect(width - b, b, b, height - 2 * b)
        ctx.fillStyle = color
        for (let x = 0; x < width; x += tile * 2) {
            ctx.fillRect(x, 0, tile, b); ctx.fillRect(x + tile, height - b, tile, b)
        }
        for (let y = b; y < height - b; y += tile * 2) {
            ctx.fillRect(0, y, b, tile); ctx.fillRect(width - b, y + tile, b, tile)
        }
    }
}
