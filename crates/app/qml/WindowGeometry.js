.pragma library

// Restoring a remembered window onto the screens that exist now. Pure functions: logical desktop pixels in,
// logical desktop pixels out. `screens` are `{name, x, y, width, height, availableWidth, availableHeight, scale}`;
// the available area is anchored at the screen origin (panels are not known to the client).

function available(screen) {
    return { x: screen.x, y: screen.y,
             w: Math.min(screen.width, screen.availableWidth || screen.width),
             h: Math.min(screen.height, screen.availableHeight || screen.height) }
}

function valid(record) {
    return !!record && [record.x, record.y, record.w, record.h].every(Number.isFinite) && record.w >= 1 && record.h >= 1
}

// The screen a record belongs to: the same name; else the screen holding most of its rectangle; else the first.
function pickScreen(record, screens) {
    if (screens.length === 0) return null
    const named = screens.find(s => s.name === record.screen && record.screen !== "")
    if (named) return named
    let best = null, bestArea = 0
    for (const s of screens) {
        const a = available(s)
        const w = Math.min(record.x + record.w, a.x + a.w) - Math.max(record.x, a.x)
        const h = Math.min(record.y + record.h, a.y + a.h) - Math.max(record.y, a.y)
        if (w > 0 && h > 0 && w * h > bestArea) { best = s; bestArea = w * h }
    }
    return best || screens[0]
}

// Where to put the window: the size is limited to the screen's available area (and kept at least `minW × minH`),
// the position is moved inside it. If the screen is not the one the record was made on (or its geometry or
// scale changed), the record's position is not trusted: the window is centred on the chosen screen.
// `canPosition` is false where the compositor decides the position (Wayland): only the size and the screen are kept.
function restore(record, screens, minW, minH, canPosition) {
    if (!valid(record) || screens.length === 0) return null
    const screen = pickScreen(record, screens)
    const a = available(screen)
    const w = Math.max(Math.min(minW, a.w), Math.min(record.w, a.w))
    const h = Math.max(Math.min(minH, a.h), Math.min(record.h, a.h))
    const same = screen.name === record.screen
        && (record.screen_w === undefined || (record.screen_w === screen.width && record.screen_h === screen.height))
        && (!record.scale || Math.abs(record.scale - (screen.scale || 1)) < 0.01)
    const trusted = same || (record.screen === "" && pickScreen(record, screens) === screen)
    let x = trusted ? record.x : a.x + (a.w - w) / 2
    let y = trusted ? record.y : a.y + (a.h - h) / 2
    x = Math.max(a.x, Math.min(x, a.x + a.w - w))
    y = Math.max(a.y, Math.min(y, a.y + a.h - h))
    return { x: Math.round(x), y: Math.round(y), w: Math.round(w), h: Math.round(h),
             screen: screen.name, position: !!canPosition, moved: x !== record.x || y !== record.y || w !== record.w || h !== record.h }
}
