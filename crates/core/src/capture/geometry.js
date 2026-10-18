// Geometry in logical desktop coordinates: client area (no server decorations), frame (with
// decorations) and buffer (with client-side shadows). The capture is matched against them.
function send(method, ...args) {
    callDBus("__DESTINATION__", "/io/lipa/Geometry", "io.lipa.Geometry", method, ...args);
}
function rect(g) {
    return [g.x, g.y, g.width, g.height];
}
// Which window has the focus (and whether it is one of LipaX's own, which must not hide the overlay) and
// which windows are minimised: the overlay is shown only over a game that can be seen and used.
function sendFocus() {
    const active = workspace.activeWindow;
    send("Focus", active ? active.internalId.toString() : "", !!(active && active.pid === LIPA_PID));
}
function watchWindow(window) {
    const uuid = window.internalId.toString();
    // Who the window is (class and title): after the game is restarted it has a new uuid, and LipaX finds it again by this.
    function sendIdentity() { send("Identity", uuid, String(window.resourceClass), String(window.caption)); }
    if (window.captionChanged) window.captionChanged.connect(sendIdentity);
    sendIdentity();
    function sendMinimized() { send("Minimized", uuid, !!window.minimized); }
    if (window.minimizedChanged) window.minimizedChanged.connect(sendMinimized);
    sendMinimized();
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
// Exact title of FloatingTranslationSurface.qml; the main window ("LipaX — переводчик для игр")
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
    sendFocus();
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
    return screens().some(function(s) {
        const r = s.geometry;
        // A partly off-screen frameless window can still be dragged from its visible body.
        return Math.min(g.x + g.width, r.x + r.width) - Math.max(g.x, r.x) >= 24
            && Math.min(g.y + g.height, r.y + r.height) - Math.max(g.y, r.y) >= 24;
    });
}
function reportFloating(window, reason) {
    const g = window.frameGeometry, o = window.output;
    send("FloatingMoved", JSON.stringify({ x: g.x, y: g.y, w: g.width, h: g.height, output: o ? o.name : "",
        output_x: o ? o.geometry.x : 0, output_y: o ? o.geometry.y : 0 }), reason);
}
function adoptFloating(window) {
    window.keepAbove = true;
    let userMoved = false;
    let closed = false;
    window.closed.connect(function() { closed = true; });
    window.interactiveMoveResizeStarted.connect(function() { userMoved = true; });
    // Also consume the guard if KWin mapped the window without activating it. This prevents
    // a later manual task-manager click from being mistaken for the automatic show.
    restoreAutomaticFocus(window);
    if (window.windowShown) window.windowShown.connect(function() { restoreAutomaticFocus(window); });
    callDBus("__DESTINATION__", "/io/lipa/Geometry", "io.lipa.Geometry", "FloatingPlacement", function(json) {
        // A delayed D-Bus answer must never override a gesture the user already started.
        if (closed || userMoved) return;
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
                // Monitor removed: keep KWin's usable initial placement, with no app anchoring.
                reportFloating(window, "restore_geometry_rejected_unreachable");
                return;
            }
            window.frameGeometry = g;
            const actual = window.frameGeometry;
            reportFloating(window, actual.x === g.x && actual.y === g.y ? reason : "restore_geometry_not_confirmed");
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
sendFocus();
send("Ready");
