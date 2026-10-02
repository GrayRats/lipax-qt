// Geometry is reported in logical desktop coordinates, without server decorations.
function send(method, ...args) {
    callDBus("__DESTINATION__", "/io/lipa/Geometry", "io.lipa.Geometry", method, ...args);
}
function watchWindow(window) {
    const uuid = window.internalId.toString();
    function update() {
        const g = window.clientGeometry;
        send("Update", uuid, JSON.stringify([g.x, g.y, g.width, g.height]));
    }
    window.clientGeometryChanged.connect(update);
    window.closed.connect(function() { send("Update", uuid, "[0,0,0,0]"); });
    update();
}
workspace.windowAdded.connect(watchWindow);
workspace.windowList().forEach(watchWindow);
send("Ready");
