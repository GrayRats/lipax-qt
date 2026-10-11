"""Capture only user-authorized engine.exe/CrossxMars.exe windows through KWin.

Uses a transient KWin script for geometry/focus and authorized Spectacle for
client-area pixels. No desktop-entry or global settings changes. Artifacts stay
in project tmp/. This is a test harness, not an application capture backend.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time

from PySide6.QtCore import ClassInfo, QObject, QCoreApplication, Slot
from PySide6.QtDBus import QDBusConnection, QDBusInterface

ROOT = Path(__file__).resolve().parents[3]
ALLOWED = {"engine.exe", "CrossxMars.exe"}


@ClassInfo({"D-Bus Interface": "io.lipa.InplaceLiveTest"})
class Receiver(QObject):
    answer = None

    @Slot(str)
    def Result(self, value):
        self.answer = json.loads(value)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--capture", action="store_true")
    args = parser.parse_args()
    directory = ROOT / "tmp/rapid-live/capture"
    directory.mkdir(parents=True, exist_ok=True)
    pids = []
    for process in Path("/proc").iterdir():
        if not process.name.isdecimal():
            continue
        try:
            if (process / "comm").read_text().strip() in ALLOWED:
                pids.append(int(process.name))
        except (OSError, UnicodeError):
            pass
    assert pids, "no authorized game process is running"
    app = QCoreApplication([])
    bus = QDBusConnection.sessionBus()
    assert bus.isConnected(), "session D-Bus unavailable"
    receiver = Receiver()
    assert bus.registerObject("/io/lipa/InplaceLiveTest", receiver, QDBusConnection.ExportAllSlots)
    scripting = QDBusInterface("org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting", bus)

    def query(body):
        receiver.answer = None
        plugin = f"lipax-inplace-test-{os.getpid()}-{time.monotonic_ns()}"
        path = directory / (plugin + ".js")
        path.write_text(body + f'\ncallDBus({json.dumps(bus.baseService())}, "/io/lipa/InplaceLiveTest", "io.lipa.InplaceLiveTest", "Result", JSON.stringify(result));\n')
        os.chmod(path, 0o600)
        loaded = scripting.call("loadScript", str(path), plugin)
        assert loaded.arguments() and isinstance(loaded.arguments()[0], int), loaded.errorMessage()
        script_id = loaded.arguments()[0]
        assert script_id >= 0, "KWin refused test script"
        try:
            script = QDBusInterface("org.kde.KWin", f"/Scripting/Script{script_id}", "org.kde.kwin.Script", bus)
            script.call("run")
            deadline = time.monotonic() + 5
            while receiver.answer is None and time.monotonic() < deadline:
                app.processEvents()
                time.sleep(0.01)
            assert receiver.answer is not None, "KWin did not send window geometry"
            return receiver.answer
        finally:
            scripting.call("unloadScript", plugin)
            path.unlink(missing_ok=True)

    result = query('''function rect(r) { return [r.x,r.y,r.width,r.height]; }
const pids = ''' + json.dumps(pids) + ''';
const result = {previous: workspace.activeWindow ? workspace.activeWindow.internalId.toString() : null,
    screens: workspace.screens.map(s => rect(s.geometry)),
    windows: workspace.windowList().filter(w => pids.includes(w.pid)).map(w => ({
        uuid:w.internalId.toString(), pid:w.pid, caption:String(w.caption), resourceClass:String(w.resourceClass),
        client:rect(w.clientGeometry), frame:rect(w.frameGeometry), buffer:rect(w.bufferGeometry),
        minimized:w.minimized, outputScale:w.output ? w.output.scale : null}))};''')
    assert result["windows"], "KWin has no windows for the authorized process PIDs"
    (directory / "windows.json").write_text(json.dumps(result, indent=2, ensure_ascii=False))
    print(json.dumps(result, indent=2, ensure_ascii=False))
    if not args.capture:
        return
    env = os.environ.copy()
    # Spectacle gets isolated settings/data so its test run does not change user settings.
    env["XDG_CONFIG_HOME"] = str(directory / "config")
    env["XDG_DATA_HOME"] = str(directory / "data")
    env["QT_QPA_PLATFORM"] = "wayland"
    env.pop("QT_QUICK_BACKEND", None)
    try:
        for window in result["windows"]:
            active = query('const w = workspace.windowList().find(w => w.internalId.toString() === '
                + json.dumps(window["uuid"]) + '); if (w) { w.minimized = false; workspace.activeWindow = w; } '
                + 'const result = !!(w && workspace.activeWindow === w);')
            assert active, "authorized window is unavailable"
            time.sleep(0.3)
            name = Path(f'/proc/{window["pid"]}/comm').read_text().strip()
            target = directory / f"{name}-{window['pid']}.png"
            subprocess.run(["spectacle", "--background", "--activewindow", "--nonotify",
                "--no-decoration", "--no-shadow", "--output", str(target)], env=env, check=True, timeout=20)
            assert target.exists(), "Spectacle produced no game capture"
            window["capture"] = str(target)
    finally:
        for window in result["windows"]:
            if window["minimized"]:
                query('const w = workspace.windowList().find(w => w.internalId.toString() === '
                    + json.dumps(window["uuid"]) + '); if(w) w.minimized = true; const result = true;')
        if result["previous"]:
            query('const w = workspace.windowList().find(w => w.internalId.toString() === '
                + json.dumps(result["previous"]) + '); if(w) workspace.activeWindow = w; const result = true;')
        (directory / "windows.json").write_text(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
