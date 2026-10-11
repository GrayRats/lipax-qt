"""Real-image structural regression tests, without fabricated OCR ground truth.

Build inplace_bench first. References are opt-in: set LIPAX_VISUAL_REFERENCES to
an independently reviewed directory. Never approve a render automatically.
"""
import json
import os
from pathlib import Path
import subprocess
import time

import pytest
from PIL import Image, ImageChops, ImageDraw

ROOT = Path(__file__).resolve().parents[3]
ARTIFACTS = ROOT / "tmp" / "visual"
REGISTERED_FAMILIES = set()
SOURCES = sorted(p for p in (ROOT / "docs/screenshots/test-game").iterdir()
                 if p.is_file() and (p.suffix.lower() in {".png", ".jpg", ".jpeg"} or not p.suffix))


def source_coverage(lines, plate, width, height):
    """Coverage of OCR geometry, not independently annotated text accuracy."""
    if not lines:
        return None
    x, y, w, h = plate
    px, py, pr, pb = x * width, y * height, (x + w) * width, (y + h) * height
    total = sum(line["w"] * line["h"] for line in lines)
    if total <= 0:
        return None
    covered = sum(max(0, min(pr, line["x"] + line["w"]) - max(px, line["x"])) *
                  max(0, min(pb, line["y"] + line["h"]) - max(py, line["y"])) for line in lines)
    return covered / total


def test_source_coverage_detects_missing_or_displaced_overlay():
    lines = [dict(x=10, y=50, w=60, h=20)]
    assert source_coverage(lines, [0.1, 0.5, 0.6, 0.2], 100, 100) == 1
    assert source_coverage(lines, [0.1, 0.2, 0.6, 0.2], 100, 100) == 0
    assert source_coverage(lines, [0.1, 0.5, 0.3, 0.2], 100, 100) == 0.5
    assert source_coverage([], [0, 0, 1, 1], 100, 100) is None


def visual_difference(actual, expected, output):
    if actual.size != expected.size:
        raise ValueError("reference dimensions differ")
    diff = ImageChops.difference(actual.convert("RGB"), expected.convert("RGB"))
    diff.save(output)
    # Pin the renderer/fonts/DPI when approving references. Tolerance: channel
    # difference <= 8 on at least 99.9% of pixels (antialiasing only).
    bad = sum(max(pixel) > 8 for pixel in diff.getdata())
    return bad / (actual.width * actual.height)


@pytest.fixture(scope="session")
def qt():
    from PySide6.QtGui import QGuiApplication, QFontDatabase
    app = QGuiApplication.instance() or QGuiApplication([])
    for font in (ROOT / "crates/app/assets/fonts").iterdir():
        if font.suffix in {".ttf", ".otf", ".ttc"}:
            font_id = QFontDatabase.addApplicationFont(str(font))
            REGISTERED_FAMILIES.update(QFontDatabase.applicationFontFamilies(font_id))
    return app


def render(qt, source, entries, directory):
    from PySide6.QtCore import QUrl
    from PySide6.QtQuick import QQuickView
    width, height = Image.open(source).size
    qml = directory / "render.qml"
    qml.write_text(f'''import QtQuick
import "{(ROOT / 'crates/app/qml').as_uri()}" as Lipa
Item {{
    width: {width}; height: {height}
    property bool collectDiagnostics: false
    readonly property string layoutReport: {{
        if (!collectDiagnostics) return "[]"
        const result = []
        for (let i = 0; i < blocks.count; ++i) {{
            const field = blocks.itemAt(i)
            const text = field.children[0].children.find(item => item.objectName === "inplaceText")
            result.push({{width: text.width, height: text.height, contentWidth: text.contentWidth,
                contentHeight: text.contentHeight, lines: text.lineCount, truncated: text.truncated}})
        }}
        return JSON.stringify(result)
    }}
    Image {{ anchors.fill: parent; source: "{source.as_uri()}" }}
    Repeater {{
        id: blocks
        model: testEntries
        delegate: Item {{
            required property var modelData
            x: modelData.box[0] * {width}; y: modelData.box[1] * {height}
            width: modelData.box[2] * {width}; height: modelData.box[3] * {height}
            Lipa.InplaceTranslationContent {{ anchors.fill: parent; entry: parent.modelData }}
        }}
    }}
}}''')
    view = QQuickView()
    view.setResizeMode(QQuickView.SizeRootObjectToView)
    view.rootContext().setContextProperty("testEntries", entries)
    start = time.perf_counter()
    view.setSource(QUrl.fromLocalFile(str(qml)))
    assert not view.errors(), [e.toString() for e in view.errors()]
    view.show()
    for _ in range(20):
        qt.processEvents()
        time.sleep(0.005)
    grabbed = view.grabWindow()
    assert not grabbed.isNull(), "Qt offscreen renderer returned no image"
    assert grabbed.save(str(directory / "rendered.png"))
    view.rootObject().setProperty("collectDiagnostics", True)
    qt.processEvents()
    diagnostics = json.loads(view.rootObject().property("layoutReport"))
    assert len(diagnostics) == len(entries)
    # Measure update/paint after initial item creation; keep the same delegates.
    start_update = time.perf_counter()
    view.rootContext().setContextProperty("testEntries", entries)
    qt.processEvents()
    view.grabWindow()
    update_ms = (time.perf_counter() - start_update) * 1000
    view.close()
    return (time.perf_counter() - start) * 1000, update_ms, diagnostics


def run_screenshot_pipeline(source, qt, translation=None, case=None, ocr_blocks=False, window=None, screens=None, translations=None):
    executable = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "tmp/cargo-target")) / "debug/examples/inplace_bench"
    assert executable.exists(), "build cargo example inplace_bench first"
    directory = ARTIFACTS / source.stem
    if case:
        directory = directory / case
    directory.mkdir(parents=True, exist_ok=True)
    original = Image.open(source).convert("RGBA")
    # Captured game frames are opaque. Normalize translucent screenshot borders
    # explicitly, rather than comparing RGB values that discard source alpha.
    original = Image.alpha_composite(Image.new("RGBA", original.size, (0, 0, 0, 255)), original)
    assert original.width > 0 and original.height > 0
    png = directory / "original.png"
    original.save(png)
    env = dict(os.environ, LIPAX_DIAGNOSTICS_DIR=str(directory))
    command = [str(executable), str(png)]
    if translation is not None:
        command.append("--translation=" + translation)
    if translations is not None:
        command.append("--translations=" + str(translations))
    if ocr_blocks:
        command.append("--ocr-blocks")
    if window:
        command.append("--window=" + ",".join(str(v) for v in window))
    if screens:
        command.append("--screens=" + json.dumps(screens))
    if os.environ.get("LIPAX_TEST_OCR") == "1":
        command.extend(["--ocr", "--lang=rus" if source.stem.endswith("-ru") else "--lang=eng"])
    data = json.loads(subprocess.check_output(command, env=env, text=True, timeout=120))
    if os.environ.get("LIPAX_TEST_OCR") == "1":
        assert data["ocr"]["engine"] == "rapidocr", "requested RapidOCR was not used"
    entries = data["placed"]
    if ocr_blocks:
        assert data["block_ocr"]
        assert any(block["accepted"] for block in data["block_ocr"])
        assert all(not entry["original"].startswith("fixture ") for entry in entries)
        accepted = {block["id"] for block in data["block_ocr"] if block["accepted"]}
        assert all(entry["block_id"] in accepted for entry in entries)
        assert all(block["confidence"] is None or block["confidence"] >= data["minimum_confidence"]
                   for block in data["block_ocr"] if block["accepted"])
    assert data["blocks"], "detector found no candidates in a text-bearing screenshot"
    assert entries, "all candidates dropped"
    assert data["hits"] > 0, "identical placement did not reuse cached metrics"
    boxes = ImageDraw.Draw(original)
    for block in data["blocks"]:
        r = block["rect"]
        assert 0 <= block["confidence"] <= 1
        boxes.rectangle((r["x"], r["y"], r["x"] + r["w"], r["y"] + r["h"]), outline="red", width=2)
    original.save(directory / "detected.png")
    for entry in entries:
        x, y, w, h = entry["box"]
        assert x >= -1e-6 and y >= -1e-6 and x+w <= 1+1e-6 and y+h <= 1+1e-6
        assert entry["font_px"] > 0
    if os.environ.get("LIPAX_QT_MEASURE_SERVER"):
        assert data["measurement"].startswith("Qt:"), "Qt measurement adapter was not used"
    render_ms, update_ms, diagnostics = render(qt, png, entries, directory)
    data.update(render_ms=render_ms, update_ms=update_ms, layout_diagnostics=diagnostics)
    if ocr_blocks:
        drawn = {entry["block_id"]: entry for entry in entries}
        dropped = {entry["block_id"]: entry for entry in data["dropped_details"]}
        data["overlay_quality"] = {
            "status": "diagnostic; OCR geometry is not independently annotated ground truth",
            "accepted": sum(block["accepted"] for block in data["block_ocr"]),
            "rendered": len(entries),
            "missing": [dict(id=block["id"], original=block["original"],
                confidence=block["confidence"], reason=dropped.get(block["id"], {}).get("reason", "not published"))
                for block in data["block_ocr"] if block["accepted"] and block["id"] not in drawn],
            "source_coverage": [dict(id=block["id"], original=block["original"],
                rendered=block["id"] in drawn, plate_coverage=source_coverage(block["lines"],
                    drawn[block["id"]]["box"] if block["id"] in drawn else [0, 0, 0, 0], original.width, original.height))
                for block in data["block_ocr"] if block["accepted"]],
            "fallbacks": [dict(id=entry["block_id"], mode=entry["degraded"])
                for entry in entries if entry.get("degraded")],
        }
    if ocr_blocks:
        # This is a diagnostic acceptance threshold on OCR-derived rectangles, not OCR accuracy.
        # Exact duplicated readings are reported separately rather than counted as lost dialogue.
        repeated = {entry["block_id"] for entry in data["dropped_details"]
                    if entry["reason"] == "повторный OCR того же текста"}
        failures = [entry["id"] for entry in data["overlay_quality"]["source_coverage"]
                    if entry["id"] not in repeated and len(entry["original"].split()) >= 3
                    and entry["plate_coverage"] is not None and entry["plate_coverage"] < 0.9]
        data["overlay_quality"]["placement_diagnostic"] = "FAIL" if failures else "no failure in this diagnostic"
        data["overlay_quality"]["uncovered_text_blocks"] = failures
    (directory / "metadata.json").write_text(json.dumps(data, indent=2, ensure_ascii=False))
    if data["measurement"].startswith("Qt:"):
        for entry, layout in zip(entries, diagnostics):
            if entry["wrap"] != "elide":
                assert not layout["truncated"], (entry["key"], layout)
                # Qt 6.12 reports a right/centre-aligned line two pixels wider than the same left-aligned line when
                # the font is italic with positive letter spacing (measured: 48.05 left, 50.05 right, in
                # InplaceTranslationContent). The glyphs are not clipped (inspected); the allowance is that
                # accounting artefact only, not a general widening of the tolerance.
                accounting = 2 if entry["alignment"] != "left" and entry["letter_spacing"] > 0 else 0
                assert layout["contentWidth"] <= layout["width"] + 1 + accounting, (entry["key"], layout)
                assert layout["contentHeight"] <= layout["height"] + 1, (entry["key"], layout)
    # Pixels outside all overlay rectangles must be unchanged. This checks
    # outlines/shadows as well as text, using the actual QML renderer.
    rendered = Image.open(directory / "rendered.png").convert("RGB")
    allowed = Image.new("L", rendered.size)
    draw = ImageDraw.Draw(allowed)
    for entry in entries:
        x, y, w, h = entry["box"]
        draw.rectangle((int(x*rendered.width)-1, int(y*rendered.height)-1, int((x+w)*rendered.width)+1, int((y+h)*rendered.height)+1), fill=255)
    diff = ImageChops.difference(rendered, Image.open(png).convert("RGB"))
    outside = Image.composite(Image.new("RGB", rendered.size), diff, allowed)
    outside.save(directory / "outside-difference.png")
    assert outside.getbbox() is None, "overlay changed pixels outside its allowed plates"
    if os.environ.get("LIPAX_SAVE_TRANSLATIONS") == "1":
        # Explicit user-requested permanent deliverables; intermediate artifacts stay in tmp/.
        destination = ROOT / "docs/screenshots/test-game/translations"
        destination.mkdir(parents=True, exist_ok=True)
        name = source.stem + ("--" + case if case else "")
        rendered.save(destination / (name + ".png"))
        report = dict(data, source=str(source), approved_reference=False,
                      translation_service="none: predefined deterministic test input")
        (destination / (name + ".json")).write_text(json.dumps(report, indent=2, ensure_ascii=False))
        (destination / (name + ".txt")).write_text("\n\n".join(
            f"[{entry['block_id']}] {entry['original']}\n→ {entry['text']}"
            for entry in entries), encoding="utf-8")
    references = os.environ.get("LIPAX_VISUAL_REFERENCES") if not case else None
    if references:
        expected = Path(references) / f"{source.stem}.png"
        assert expected.exists(), "missing approved reference"
        assert visual_difference(rendered, Image.open(expected), directory / "difference.png") <= 0.001

    return data


@pytest.mark.parametrize("source", SOURCES, ids=lambda p: p.name)
def test_real_screenshot_pipeline(source, qt):
    run_screenshot_pipeline(source, qt)


@pytest.mark.parametrize("case,translation", [
    ("long-dialogue", "Очень длинный перевод с переносами и читаемым запасным вариантом. " * 20),
    ("long-word", "supercalifragilisticexpialidocious" * 20),
    ("hard-breaks", "\nHello\r\nПривет\n\n日本語\n"),
    ("mixed-shaping", "a\u0301 e\u0301 office العربية Привет 日本語 中文 한글"),
])
def test_qt_fitting_difficult_translations(case, translation, qt):
    if not os.environ.get("LIPAX_QT_MEASURE_SERVER"):
        pytest.skip("requires the shared production Qt measurement server")
    # Use real background/geometry; text is deterministic and does not call a service.
    run_screenshot_pipeline(ROOT / "docs/screenshots/test-game/dialogue-en-13.png", qt, translation, case)


def test_recognized_blocks_drive_qt_overlay(qt):
    if not os.environ.get("LIPAX_QT_MEASURE_SERVER"):
        pytest.skip("requires the shared production Qt measurement server")
    run_screenshot_pipeline(ROOT / "docs/screenshots/test-game/dialogue-en-13.png", qt,
        case="recognized-blocks", ocr_blocks=True)


@pytest.mark.parametrize("process", ["engine.exe", "CrossxMars.exe"])
def test_recorded_game_capture_pipeline(process, qt):
    manifest = Path(os.environ.get("LIPAX_LIVE_CAPTURE_MANIFEST", ROOT / "tests/inplace/fixtures/windows.json"))
    data = json.loads(manifest.read_text())
    window = next(w for w in data["windows"] if Path(w["capture"]).name.startswith(process + "-"))
    source = manifest.parent / window["capture"]
    assert any(source.resolve().is_relative_to((ROOT / folder).resolve())
               for folder in ["tmp", "tests/inplace/fixtures"])
    # This observed session is at 1:1 capture/logical scale; do not silently treat
    # a physical capture with another scale as logical render pixels.
    assert Image.open(source).size == tuple(window["client"][2:]), "live fixture needs explicit DPR conversion"
    translation_file = ROOT / "tests/inplace/fixtures/translations.json"
    result = run_screenshot_pipeline(source, qt, case="live-rapidocr", ocr_blocks=True,
        window=window["client"], screens=data.get("screens"), translations=translation_file)
    translations = json.loads(translation_file.read_text())
    assert result["repeat_ocr_jobs"] == 0, "identical frame reruns confirmed empty or settled OCR fields"
    assert all(entry["original"] in translations and entry["text"] == translations[entry["original"]]
               for entry in result["placed"]), "fixture reading changed or a partial crop was translated"
    if process == "engine.exe":
        # This recorded frame was visually inspected. Its dialogue is one three-line paragraph; the detector used to
        # cut the last line off as a second field and now keeps the paragraph together. Verify the whole paragraph
        # is read, rendered over its source and not given a beside-original label.
        dialogue = [entry for entry in result["overlay_quality"]["source_coverage"]
                    if "Considering the amount" in entry["original"] and "king-sized bed" in entry["original"]]
        assert len(dialogue) == 1, "recorded engine.exe dialogue is one complete reading"
        assert all(entry["rendered"] and entry["plate_coverage"] >= 0.9 for entry in dialogue)
        ids = {entry["id"] for entry in dialogue}
        assert all(entry.get("degraded") != "beside-original" for entry in result["placed"] if entry["block_id"] in ids)
        assert all(entry["original"] != "0" for entry in result["ocr_history"] if entry["accepted"]), \
            "the halftone dot field must no longer be read as a text block"
    else:
        required = [entry for entry in result["overlay_quality"]["source_coverage"]
                    if entry["original"] == "Mika" or (entry["original"].startswith("Morning, Ella.")
                    and entry["original"].endswith("but I have the right to be angry!"))
                    or entry["original"].startswith("Fuel heating status:")]
        assert len(required) == 3, "name, dialogue and HUD must have complete source readings"
        assert all(entry["rendered"] and entry["plate_coverage"] >= 0.9 for entry in required)
        hud = next(entry for entry in required if entry["original"].startswith("Fuel heating status:"))
        assert ":1123" in hud["original"], "joined crop reused the old reading and lost the IP suffix"
        assert any(entry["scan"] > 0 and entry["original"] == hud["original"]
                   for entry in result["ocr_history"]), "joined HUD must be read again with RapidOCR"
        assert any(entry["scan"] > 0 and entry["original"] == "Mika" for entry in result["ocr_history"])
        ids = {entry["id"] for entry in required}
        assert all(entry.get("degraded") != "beside-original" for entry in result["placed"] if entry["block_id"] in ids)
        assert not result["overlay_quality"]["uncovered_text_blocks"]
        # The low-contrast HUD readout the global threshold used to miss is now a block, read and drawn over.
        readout = [entry for entry in result["overlay_quality"]["source_coverage"] if entry["original"] == "rSpeed: 5 ms Temp: 20 C"]
        assert len(readout) == 1 and readout[0]["rendered"] and readout[0]["plate_coverage"] >= 0.9


def test_visual_comparison_rejects_a_real_regression():
    ARTIFACTS.mkdir(parents=True, exist_ok=True)
    expected = Image.new("RGB", (40, 40), "black")
    actual = expected.copy()
    ImageDraw.Draw(actual).rectangle((0, 0, 20, 20), fill="white")
    assert visual_difference(actual, expected, ARTIFACTS / "comparator-negative.png") > 0.001
    assert visual_difference(expected, expected, ARTIFACTS / "comparator-identical.png") == 0


def test_bundled_multilingual_glyphs_are_present(qt):
    from PySide6.QtGui import QFont, QRawFont, QFontDatabase
    for family, text in [("Noto Sans", "HelloПривет"), ("Noto Sans CJK SC", "日本語中文한글")]:
        assert family in REGISTERED_FAMILIES, f"bundled family not registered: {family}"
        font = QFont(family)
        font.setPixelSize(20)
        raw = QRawFont.fromFont(font)
        assert raw.isValid()
        assert all(raw.supportsCharacter(ord(character)) for character in text), f"missing glyph in {family}"
