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
workspace.windowAdded.connect(watchWindow);
workspace.windowList().forEach(watchWindow);
send("Ready");
