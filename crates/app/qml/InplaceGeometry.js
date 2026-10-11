.pragma library

// Input coordinates are fractions of the captured region; output is desktop logical px.
function desktopRect(geometry, entry) {
    if (!Array.isArray(geometry) || geometry.length !== 4 || !entry) return null
    const r = entry.rect, b = entry.box
    if (!r || !Array.isArray(b) || b.length !== 4) return null
    const values = geometry.concat([r.x, r.y, r.w, r.h], b)
    if (!values.every(v => typeof v === "number" && isFinite(v))) return null
    if (geometry[2] <= 0 || geometry[3] <= 0 || r.w <= 0 || r.h <= 0 || b[2] <= 0 || b[3] <= 0) return null
    // Intersect with both the capture region and window. A wholly outside field is hidden.
    const x0 = Math.max(0, r.x, r.x + b[0] * r.w)
    const y0 = Math.max(0, r.y, r.y + b[1] * r.h)
    const x1 = Math.min(1, r.x + r.w, r.x + (b[0] + b[2]) * r.w)
    const y1 = Math.min(1, r.y + r.h, r.y + (b[1] + b[3]) * r.h)
    if (x1 <= x0 || y1 <= y0) return null
    return [geometry[0] + x0 * geometry[2], geometry[1] + y0 * geometry[3],
            (x1 - x0) * geometry[2], (y1 - y0) * geometry[3]]
}
