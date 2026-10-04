// Geometry in logical desktop coordinates: client area (no server decorations), frame (with
// decorations) and buffer (with client-side shadows). The capture is matched against them.
function send(method, ...args) {
    callDBus("__DESTINATION__", "/io/lipa/Geometry", "io.lipa.Geometry", method, ...args);
}
function rect(g) {
    return [g.x, g.y, g.width, g.height];
}
function watchWindow(window) {
    const uuid = window.internalId.toString();
    function update() {
        send("Update", uuid, JSON.stringify([rect(window.clientGeometry), rect(window.frameGeometry), rect(window.bufferGeometry)]));
    }
    window.clientGeometryChanged.connect(update);
    if (window.frameGeometryChanged) window.frameGeometryChanged.connect(update);
    if (window.bufferGeometryChanged) window.bufferGeometryChanged.connect(update);
    window.closed.connect(function() { send("Update", uuid, "null"); });
    update();
}
// ── LipaX floating translation window (an ordinary xdg_toplevel) ──
// Applications cannot position xdg_toplevel windows on Wayland; KWin can. The script restores
// the saved position, keeps the window above others and reports where the user left it.
// Snapping/tiling while the user drags is KWin's own behaviour and is not changed here.
const LIPA_PID = __PID__;
// Exact title of FloatingOverlayWindow.qml; the main window ("LipaX — переводчик для игр")
// must never match.
const FLOATING_CAPTION = "LipaX · окно перевода";
function isFloating(window) {
    return window.pid === LIPA_PID && String(window.caption) === FLOATING_CAPTION;
}
// A normal xdg_toplevel belongs in the task manager, but KWin may activate it when it
// appears. LipaX explicitly arms the next automatic show before mapping the surface;
// this D-Bus guard is consumed once. Later task-manager or direct clicks retain focus.
let previousActive = workspace.activeWindow;
function restoreAutomaticFocus(window) {
    if (!isFloating(window)) return;
    callDBus("__DESTINATION__", "/io/lipa/Geometry", "io.lipa.Geometry", "ConsumeFloatingFocusRestore", function(armed) {
        if (!armed || workspace.activeWindow !== window) return;
        if (previousActive && previousActive !== window && workspace.windowList().includes(previousActive)) {
            workspace.activeWindow = previousActive;
        }
    });
}
workspace.windowActivated.connect(function(window) {
    if (window && isFloating(window)) restoreAutomaticFocus(window);
    else if (window) previousActive = window;
});
workspace.windowRemoved.connect(function(window) {
    if (previousActive === window) previousActive = null;
});
function screens() {
    return workspace.screens || [];
}
function reachable(g) {
    const cx = g.x + g.width / 2, cy = g.y + g.height / 2;
    return screens().some(function(s) {
        const r = s.geometry;
        return cx >= r.x && cx < r.x + r.width && cy >= r.y && cy < r.y + r.height;
    });
}
function reportFloating(window, reason) {
    const g = window.frameGeometry, o = window.output;
    send("FloatingMoved", JSON.stringify({ x: g.x, y: g.y, w: g.width, h: g.height, output: o ? o.name : "",
        output_x: o ? o.geometry.x : 0, output_y: o ? o.geometry.y : 0 }), reason);
}
function adoptFloating(window) {
    window.keepAbove = true;
    // Also consume the guard if KWin mapped the window without activating it. This prevents
    // a later manual task-manager click from being mistaken for the automatic show.
    restoreAutomaticFocus(window);
    if (window.windowShown) window.windowShown.connect(function() { restoreAutomaticFocus(window); });
    callDBus("__DESTINATION__", "/io/lipa/Geometry", "io.lipa.Geometry", "FloatingPlacement", function(json) {
        let p = null;
        try { p = JSON.parse(json); } catch (e) {}
        if (p) {
            const size = window.frameGeometry;
            // Same output if it still exists (layout may have moved it), else absolute position.
            const s = p.output ? screens().find(function(o) { return o.name === p.output; }) : null;
            let g = null;
            if (s && p.rx !== undefined) g = { x: s.geometry.x + p.rx, y: s.geometry.y + p.ry, width: size.width, height: size.height };
            else if (p.x !== undefined) g = { x: p.x, y: p.y, width: size.width, height: size.height };
            if (!g) { reportFloating(window, "placed_by_kwin"); return; }
            let reason = "restore_geometry";
            if (!reachable(g)) {
                // Monitor removed or layout changed: move just enough to be visible.
                const s = window.output || screens()[0];
                if (s) {
                    g.x = s.geometry.x + Math.max(0, (s.geometry.width - g.width) / 2);
                    g.y = s.geometry.y + Math.max(0, (s.geometry.height - g.height) / 2);
                }
                reason = "restore_geometry_sanitized";
            }
            window.frameGeometry = g;
            reportFloating(window, reason);
        }
    });
    window.interactiveMoveResizeFinished.connect(function() { reportFloating(window, "move_finished"); });
}

workspace.windowAdded.connect(function(window) {
    watchWindow(window);
    if (isFloating(window)) adoptFloating(window);
});
workspace.windowList().forEach(function(window) {
    watchWindow(window);
    if (isFloating(window)) adoptFloating(window);
});
send("Ready");
