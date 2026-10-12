"""Run the production geometry script in the current KWin session; no window mutations.

The probe has its own PID and cannot adopt application floating windows. It verifies
the actual KWin -> D-Bus geometry transport, not a mocked workspace API.
"""
import json
import os
from pathlib import Path
import time
import subprocess
import sys

from PySide6.QtCore import ClassInfo, QObject, QCoreApplication, Slot
from PySide6.QtDBus import QDBusConnection, QDBusInterface

ROOT = Path(__file__).resolve().parents[3]


@ClassInfo({"D-Bus Interface": "io.lipa.Geometry"})
class Receiver(QObject):
    ready = False
    error = None
    windows = {}

    @Slot(str, str)
    def Update(self, uuid, value):
        self.windows[uuid] = json.loads(value)

    @Slot(str, bool)
    def Focus(self, uuid, own):
        pass

    @Slot(str, bool)
    def Minimized(self, uuid, minimized):
        pass

    @Slot()
    def Ready(self):
        self.ready = True

    @Slot(str)
    def ScriptFailed(self, error):
        self.error = error


def main():
    app = QCoreApplication([])
    bus = QDBusConnection.sessionBus()
    assert bus.isConnected(), "session D-Bus unavailable"
    receiver = Receiver()
    assert bus.registerObject("/io/lipa/Geometry", receiver, QDBusConnection.ExportAllSlots)
    directory = ROOT / "tmp/overlay-bugfix"
    directory.mkdir(parents=True, exist_ok=True)
    monitor_file = (directory / "probe-dbus.log").open("w")
    monitor = subprocess.Popen(["dbus-monitor", "--session", "destination='" + bus.baseService() + "'"], stdout=monitor_file, stderr=monitor_file)
    time.sleep(0.1)
    plugin = f"lipax-geometry-probe-{os.getpid()}"
    path = directory / f"{plugin}.js"
    source = (ROOT / "crates/core/src/capture/geometry.js").read_text()
    source = source.replace("__DESTINATION__", bus.baseService()).replace("__PID__", str(os.getpid()))
    if "--minimal" in sys.argv:
        source = 'callDBus(' + json.dumps(bus.baseService()) + ', "/io/lipa/Geometry", "io.lipa.Geometry", "Ready");'
    path.write_text('try {\n' + source + '\n} catch(e) { callDBus('
                    + json.dumps(bus.baseService())
                    + ', "/io/lipa/Geometry", "io.lipa.Geometry", "ScriptFailed", String(e) + " line=" + e.lineNumber); }')
    scripting = QDBusInterface("org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting", bus)
    reply = scripting.call("loadScript", str(path), plugin)
    assert reply.arguments() and isinstance(reply.arguments()[0], int), reply.errorMessage()
    script_id = reply.arguments()[0]
    assert script_id >= 0
    try:
        reply = scripting.call("start")
        assert not reply.errorMessage(), reply.errorMessage()
        deadline = time.monotonic() + 5
        while not receiver.ready and receiver.error is None and time.monotonic() < deadline:
            app.processEvents()
            time.sleep(0.01)
        result = {"ready": receiver.ready, "error": receiver.error, "window_count": len(receiver.windows)}
        (directory / "kwin-geometry-probe.json").write_text(json.dumps(result, indent=2))
        print(json.dumps(result))
        assert receiver.error is None, receiver.error
        assert receiver.ready, "KWin did not announce Ready"
        if "--minimal" not in sys.argv:
            assert receiver.windows, "KWin did not deliver client geometry"
    finally:
        monitor.terminate()
        monitor.wait(timeout=3)
        monitor_file.close()
        scripting.call("unloadScript", plugin)
        path.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
