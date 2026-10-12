"""Actual XWayland LMB drag/resize of the application's floating top-level.

Native Wayland pointer gestures require a human; this test uses XTest only with
the application's XCB windows. It never claims to test native Wayland dragging.
"""
import os
from pathlib import Path
import subprocess
import time
import tomllib
import sys

ROOT = Path(__file__).resolve().parents[3]


def main():
    output = ROOT / "tmp/overlay-bugfix" / f"floating-xwayland-{os.getpid()}"
    config = output / "config/lipa"
    config.mkdir(parents=True)
    settings_file = config / "config.toml"
    settings_file.write_text('''schema_version = 2
[general]
close_to_tray = false
remember_window_geometry = true
[translation_window]
mode = "floating"
size = [600, 180]
''')
    env = os.environ.copy()
    env.update(QT_QPA_PLATFORM="xcb", XDG_CONFIG_HOME=str(config.parent),
               LIPAX_INSTANCE_ID=f"floating_test_{os.getpid()}", RUST_LOG="debug",
               LIPAX_TEST_WITH_OVERLAY="1", LIPAX_TEST_CLOSE_AFTER_MS="30000")
    def xdo(*args):
        return subprocess.check_output(["xdotool", *map(str, args)], env=env, text=True).strip()
    def geometry():
        values = dict(line.split("=", 1) for line in xdo("getwindowgeometry", "--shell", window_id).splitlines())
        return tuple(int(values[k]) for k in ("X", "Y", "WIDTH", "HEIGHT"))
    with (output / "application.log").open("w") as log:
        process = subprocess.Popen([str(ROOT / "target/debug/lipax")], env=env, stdout=log, stderr=log)
        try:
            # The lifecycle test supplies window text halfway to its scheduled close.
            deadline = time.monotonic() + 22
            window_id = None
            while time.monotonic() < deadline:
                found = subprocess.run(["xdotool", "search", "--all", "--onlyvisible", "--pid", str(process.pid), "--name", "^LipaX · окно перевода$"], capture_output=True, text=True)
                if found.returncode == 0 and found.stdout.strip():
                    window_id = found.stdout.strip().splitlines()[0]
                    break
                time.sleep(0.1)
            assert window_id, "floating XWayland window was not shown"
            observations = []
            for dx, dy in [(250, 200), (1300, 500), (500, 350)]:
                before = geometry()
                xdo("mousemove", "--window", window_id, 100, 60)
                xdo("mousedown", 1)
                time.sleep(0.1)
                xdo("mousemove", "--sync", dx + 100, dy + 60)
                time.sleep(0.2)
                xdo("mouseup", 1)
                time.sleep(0.4)
                after = geometry()
                if after == before:
                    raise RuntimeError("UNVERIFIED: KWin ended the move without accepting XTest pointer motion; use a physical pointer to validate dragging")
                assert abs(after[0] - dx) < 8 and abs(after[1] - dy) < 8, (before, after, dx, dy)
                time.sleep(0.4)
                assert geometry() == after, "application repositioned a completed drag"
                saved = tomllib.loads(settings_file.read_text())["translation_window"]["floating_geometry"]
                assert abs(saved["x"] - after[0]) < 8 and abs(saved["y"] - after[1]) < 8
                observations.append(after)
            before = geometry()
            xdo("mousemove", "--window", window_id, before[2] - 5, before[3] - 5)
            xdo("mousedown", 1)
            time.sleep(0.1)
            xdo("mousemove", "--sync", before[0] + before[2] + 100, before[1] + before[3] + 70)
            time.sleep(0.2)
            xdo("mouseup", 1)
            time.sleep(0.8)
            resized = geometry()
            assert resized[:2] == before[:2], (before, resized)
            assert resized[2] > before[2] and resized[3] > before[3], (before, resized)
            # MMB round trip uses the production Controller and the persistent surfaces.
            xdo("mousemove", "--window", window_id, 100, 60)
            xdo("click", 2)
            time.sleep(0.4)
            assert tomllib.loads(settings_file.read_text())["translation_window"]["mode"] == "pinned"
            # Pinned input keeps a top-right handle. Its position is independent (100,100).
            xdo("mousemove", 100 + 600 - 20, 100 + 20)
            xdo("click", 2)
            time.sleep(0.5)
            assert tomllib.loads(settings_file.read_text())["translation_window"]["mode"] == "floating"
            assert geometry() == resized, "mode round trip lost floating geometry"
            observations.append(resized)
            print({"drag_positions": observations, "log": str(output / "application.log")})
        finally:
            if process.poll() is None:
                subprocess.run([str(ROOT / "target/debug/lipax"), "--quit"], env=env, check=True, timeout=5)
            process.wait(timeout=8)


if __name__ == "__main__":
    try:
        main()
    except RuntimeError as error:
        print(error, file=sys.stderr)
        sys.exit(77)
