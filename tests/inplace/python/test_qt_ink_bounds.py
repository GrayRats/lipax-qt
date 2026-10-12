"""Production Qt measurement versus actual QML italic alignment geometry.

The negative-spacing Source Serif case reproduces screenshot 10's narrow field.
This is a synthetic fitting regression; it performs no OCR or translation.
"""
import json
import math
import os
import subprocess

from PIL import Image
import pytest

from test_inplace_screenshots import ROOT, qt, render


def measure(requests):
    executable = os.environ.get("LIPAX_QT_MEASURE_SERVER")
    if not executable:
        pytest.skip("NOT RUN: production Qt measurement server is required")
    result = subprocess.run([executable, str(ROOT / "crates/app/assets/fonts")],
        input="".join(json.dumps(row) + "\n" for row in requests),
        text=True, capture_output=True, check=True, timeout=30)
    rows = [json.loads(line) for line in result.stdout.splitlines()]
    assert len(rows) == len(requests)
    return rows


def request(text, width=40, wrap=0):
    return {"operation": "measure", "text": text, "family": "Source Serif 4",
            "weight": 200, "italic": True, "px": 9,
            "letter_spacing": -0.38910049200057983, "width": width, "wrap": wrap}


def test_right_aligned_italic_ink_is_included_before_fitting(qt):
    text = "Проверка переноса: длинная тестовая строка. Hello 日本語"
    measured = measure([request(text)])[0]
    directory = ROOT / "tmp/inplace-tests/ink-bounds"
    directory.mkdir(parents=True, exist_ok=True)
    source = directory / "source.png"
    Image.new("RGB", (98, 164), (24, 32, 48)).save(source)
    entry = {"key": "source10-negative-italic", "box": [0, 0, 1, 1],
        "inner": [28, 28, 28, 28], "layout_width": 40, "text": text,
        "font_family": "Source Serif 4", "font_weight": 200, "font_px": 9,
        "italic": True, "letter_spacing": -0.38910049200057983,
        "line_height": 0.8999999761581421, "alignment": "right", "wrap": "word",
        "max_lines": 9, "outline": False, "shadow": False,
        "text_color": "#ffffff", "background": {"mode": "solid_fill",
        "color": "#182030", "opacity": 1, "radius": 0}}
    _, _, narrow = render(qt, source, [entry], directory)
    assert measured["width"] >= narrow[0]["contentWidth"] - 1e-6, (measured, narrow)
    assert measured["width"] > 40, "the old helper falsely accepted this narrow field"
    entry["layout_width"] = math.ceil(measured["width"])
    corrected = directory / "corrected"
    corrected.mkdir(exist_ok=True)
    _, _, fitted = render(qt, source, [entry], corrected)
    assert not fitted[0]["truncated"]
    # Preserve the existing screenshot suite's one-pixel rounding allowance.
    assert fitted[0]["contentWidth"] <= fitted[0]["width"] + 1, fitted
    assert fitted[0]["contentHeight"] <= fitted[0]["height"] + 1, fitted


def test_unwrapped_word_measurement_remains_finite():
    word, hard_breaks = measure([request("Проверка", wrap=2), request("one\ntwo\n", wrap=2)])
    assert 0 < word["width"] < 100, word
    assert word["lines"] == 1
    assert 0 < hard_breaks["width"] < 100, hard_breaks
    assert hard_breaks["lines"] == 3
