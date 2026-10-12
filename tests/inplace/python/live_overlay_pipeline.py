"""Live KWin capture -> RapidOCR -> local Bergamot -> application GUI regression.

Only the generated source window is captured or moved. Config, logs and field
buffers stay under tmp/overlay-bugfix. Requires a lifecycle-test application
build and its normal ScreenShot2 authorization; never bypasses authorization.
"""
import json
import os
from pathlib import Path
import subprocess
import time
import re
import sys
import hashlib

from PySide6.QtCore import ClassInfo, QObject, QUrl, Slot
from PySide6.QtGui import QGuiApplication
from PySide6.QtQml import QQmlComponent, QQmlEngine
from PySide6.QtDBus import QDBusConnection, QDBusInterface

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "tmp/overlay-bugfix" / ("live-pipeline-" + os.environ.get("QT_QPA_PLATFORM", "wayland") + "-" + str(os.getpid()))


@ClassInfo({"D-Bus Interface": "io.lipa.OverlayTest"})
class Receiver(QObject):
    answer = None

    @Slot(str)
    def Result(self, value):
        self.answer = json.loads(value)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    executable = Path(os.environ.get("LIPAX_TEST_EXECUTABLE", ROOT / "target/debug/lipax")).resolve()
    bergamot = Path(os.environ.get("BERGAMOT_BINARY", ROOT / "target/bergamot-build/app/bergamot")).resolve()
    models = Path(os.environ.get("LIPAX_BERGAMOT_TEST_MODELS", Path.home() / ".local/share/LipaX/bergamot-models/en-ru")).resolve()
    assert executable.is_file() and os.access(executable, os.X_OK), "build a lifecycle-test executable first"
    assert bergamot.is_file() and os.access(bergamot, os.X_OK), "local Bergamot executable is required"
    assert (models / "en-ru.yml").is_file(), "installed English→Russian model is required"
    app = QGuiApplication([])
    engine = QQmlEngine()
    component = QQmlComponent(engine)
    component.setData(b'''import QtQuick
import QtQuick.Window
Window {
    width: 900; height: 500; visible: true; title: "LipaX Overlay Regression Source"
    color: "#182030"
    Text { objectName: "sourceText"; x: 70; y: 120; text: "Welcome to the test game."
        font.pixelSize: 36; color: "white" }
    Text { x: 70; y: 300; text: "Please choose your language."
        font.pixelSize: 30; color: "white" }
}''', QUrl())
    window = component.create()
    assert window, [error.toString() for error in component.errors()]
    bus = QDBusConnection.sessionBus()
    receiver = Receiver()
    assert bus.registerObject("/io/lipa/OverlayTest", receiver, QDBusConnection.ExportAllSlots)
    scripting = QDBusInterface("org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting", bus)

    def settle(seconds):
        until = time.monotonic() + seconds
        while time.monotonic() < until:
            app.processEvents()
            time.sleep(0.01)

    def query(body):
        receiver.answer = None
        plugin = f"lipax-overlay-test-{os.getpid()}-{time.monotonic_ns()}"
        path = OUT / f"{plugin}.js"
        path.write_text(body + '\ncallDBus(' + json.dumps(bus.baseService())
                        + ', "/io/lipa/OverlayTest", "io.lipa.OverlayTest", "Result", JSON.stringify(result));')
        reply = scripting.call("loadScript", str(path), plugin)
        assert reply.arguments() and reply.arguments()[0] >= 0, reply.errorMessage()
        try:
            reply = scripting.call("start")
            assert not reply.errorMessage(), reply.errorMessage()
            until = time.monotonic() + 5
            while receiver.answer is None and time.monotonic() < until:
                settle(0.01)
            assert receiver.answer is not None, "KWin test query failed"
            return receiver.answer
        finally:
            scripting.call("unloadScript", plugin)
            path.unlink(missing_ok=True)

    settle(0.5)
    find_source = 'const w = workspace.windowList().find(w => w.pid === ' + str(os.getpid()) + ' && String(w.caption) === "LipaX Overlay Regression Source");'
    source = query(find_source + 'const result = w ? {uuid:w.internalId.toString(), geometry:[w.clientGeometry.x,w.clientGeometry.y,w.clientGeometry.width,w.clientGeometry.height]} : null;')
    assert source, "KWin cannot find the generated source window"
    config = OUT / "config/lipa"
    config.mkdir(parents=True, exist_ok=True)
    (config / "config.toml").write_text('''schema_version = 2
display_mode = "inplace"
history_enabled = false
[general]
remember_window_geometry = false
close_to_tray = false
notify_errors = false
notify_retries = false
[capture]
source = "kwin"
region_frame_mode = "off"
[capture.window]
uuid = ''' + json.dumps(source["uuid"]) + '''
resource_class = "overlay-regression"
caption = "LipaX Overlay Regression Source"
[capture.region]
x = 0.0
y = 0.0
w = 1.0
h = 1.0
[recognition]
engine = "rapidocr"
language = "eng"
rapid_threads = 2
rapid_use_gpu = false
interval_ms = 500
debounce_ms = 150
[translation]
service = "bergamot"
target_language = "ru"
source_language = { mode = "explicit", language = "en" }
bergamot_binary = ''' + json.dumps(str(bergamot)) + '''
bergamot_models_dir = ''' + json.dumps(str(models)) + '''
[appearance.inplace]
only_when_active = false
''')
    env = os.environ.copy()
    env.update(XDG_CONFIG_HOME=str(config.parent),
               TMPDIR=str(ROOT / "tmp"), TMP=str(ROOT / "tmp"), TEMP=str(ROOT / "tmp"),
               XDG_CACHE_HOME=str(ROOT / "tmp/xdg-cache"),
               PYTHONPYCACHEPREFIX=str(ROOT / "tmp/python-bytecode"),
               QT_QPA_PLATFORM=os.environ.get("QT_QPA_PLATFORM", "wayland"),
               BERGAMOT_BINARY=str(bergamot), BERGAMOT_MODELS_DIR=str(models),
               RUST_LOG="debug", LIPAX_INSTANCE_ID=f"overlay_test_{os.getpid()}",
               LIPAX_TEST_CLOSE_AFTER_MS="30000", LIPAX_TEST_INPLACE_SNAPSHOTS=str(OUT))
    log_path = OUT / "application.log"
    with log_path.open("w") as log:
        process = subprocess.Popen([str(executable), "--start-autotranslate"], env=env, stdout=log, stderr=log)
        sampler = subprocess.Popen([sys.executable, str(Path(__file__).with_name("sample_pipeline_resources.py")),
            str(process.pid), "--seconds=30", "--preview-json=" + str(OUT / "preview.json"),
            "--output=" + str(OUT / "resources")], env=env, stdout=log, stderr=log)
        try:
            settle(8)
            log_before = log_path.read_text()
            assert "Event::Inplace delivered on GUI thread" in log_before, log_before[-4500:]
            assert "stage=frame_submitted" in log_before, "no QML frame submitted"
            assert "service=Bergamot" in log_before, "configured local translation provider did not run"
            assert "source_language=\"en\"" in log_before and "target_language=\"ru\"" in log_before
            # Only the test-owned source window is moved through KWin's compositor API.
            moved = query(find_source + 'const g=w.frameGeometry; w.frameGeometry={x:200,y:180,width:g.width,height:g.height}; const result=true;')
            assert moved
            settle(2)
            log_moved = log_path.read_text()
            assert log_moved.count("Запрос перевода") == log_before.count("Запрос перевода"), "source movement triggered retranslation"
            assert log_moved.count("stage=frame_submitted") > log_before.count("stage=frame_submitted"), "source movement did not redraw fields"
            window.setWidth(1000)
            window.setHeight(560)
            settle(2)
            text = window.findChild(QObject, "sourceText")
            text.setProperty("text", "The translation has changed.")
            settle(5)
            changed = log_path.read_text()
            assert "new_translations=1" in changed[len(log_moved):], "changed source was not translated and published"
            # With one screen, make our fixture cover the capture output. The compositor
            # screenshot then contains only the test scene and its actual layer surfaces.
            if len(app.screens()) == 1:
                window.showFullScreen()
                query(find_source + 'workspace.windowList().filter(m => m.pid === ' + str(process.pid)
                      + ' && String(m.caption).indexOf("LipaX") === 0).forEach(m => {m.minimized=true;}); workspace.activeWindow=w; const result=true;')
                settle(3)
                composite = OUT / "compositor.png"
                subprocess.run(["spectacle", "--background", "--fullscreen", "--nonotify", "--output", str(composite)],
                               env=env, check=True, timeout=10)
                assert composite.exists(), "no compositor screenshot"
                # Compare the submitted field buffer with the same rectangle in KWin's
                # composited output. This detects invisible/mispositioned real surfaces.
                from PIL import Image, ImageChops, ImageStat
                records = re.findall(r'snapshot saved= true geometry= QRect\((-?\d+),(-?\d+) (\d+)x(\d+)\) path= ([^\n]+)', log_path.read_text())
                assert len(records) >= 2
                latest = records[-2:]
                assert len({record[:4] for record in latest}) == 2, "two independent field surfaces were not submitted"
                output = Image.open(composite).convert("RGB")
                screen = app.screens()[0]
                scale = screen.devicePixelRatio()
                differences = []
                for x, y, width, height, filename in latest:
                    field = Image.open(filename.split(" component=", 1)[0].strip()).convert("RGB")
                    left = round((int(x) - screen.geometry().x()) * scale)
                    top = round((int(y) - screen.geometry().y()) * scale)
                    crop = output.crop((left, top, left + field.width, top + field.height))
                    diff = ImageChops.difference(crop, field)
                    mean = sum(ImageStat.Stat(diff).mean) / 3
                    assert mean < 3, f"field is not visible at its compositor coordinates: difference={mean}"
                    differences.append(mean)
                print(json.dumps({"composited_field_differences": differences, "screenshot": str(composite)}))
            subprocess.run([str(executable), "--quit"], env=env, check=True, timeout=5)
            deadline = time.monotonic() + 8
            while process.poll() is None and time.monotonic() < deadline:
                settle(0.05)
            assert process.poll() is not None, "application did not shut down after --quit"
            assert process.returncode == 0
        finally:
            if process.poll() is None:
                subprocess.run([str(executable), "--quit"], env=env, timeout=5)
                process.wait(timeout=8)
            sampler.wait(timeout=5)
            window.close()
    from PIL import Image
    snapshots = list(OUT.glob("field-*.png"))
    assert snapshots, "no real QQuickWindow field buffers saved"
    for path in snapshots:
        image = Image.open(path).convert("RGBA")
        assert image.width > 1 and image.height > 1
        assert image.getchannel("A").getextrema()[1] > 0, "fully transparent field buffer"
        assert len(image.getcolors(image.width * image.height) or []) > 2, "no glyphs in field buffer"
        # The generated source uses white text on a flat dark background. Rounded
        # plate edges alone can produce >2 colours, so require opaque light ink too.
        light_ink = sum(a > 200 and min(r, g, b) > 150 for r, g, b, a in image.getdata())
        assert light_ink >= 20, "field contains a plate but no visible foreground glyphs"
    preview = json.loads((OUT / "preview.json").read_text())
    assert preview and any(region.get("timings") for region in preview), "actual pipeline stage timings were not exported"
    result = {"source": source, "field_buffers": len(snapshots), "result": "passed", "log": str(log_path),
        "executable": str(executable), "binary_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
        "ocr": "rapidocr", "translation": "local Bergamot en→ru", "models": str(models),
        "qt_platform": env["QT_QPA_PLATFORM"],
        "screens": [{"name": screen.name(), "geometry": [screen.geometry().x(), screen.geometry().y(), screen.geometry().width(), screen.geometry().height()],
                     "dpr": screen.devicePixelRatio()} for screen in app.screens()],
        "actual_pipeline_preview": preview,
        "resource_summary": json.loads((OUT / "resources/summary.json").read_text()),
        "measurement_conditions": "lifecycle-test build; 500ms field-buffer readbacks and preview export enabled; resources include this probe overhead",
        "presentation_latency": "not measured; compositor screenshot verifies field visibility and coordinates only"}
    (OUT / "result.json").write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
