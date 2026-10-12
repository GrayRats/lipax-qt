"""Compare actual OCR/Rust/Qt/QML output with the user's illustrative examples.

Differences from a JPEG example are recorded, not automatically approved. The
focus rectangles are inspection aids, never supplied to OCR or placement.
"""
import argparse
import hashlib
import html
import json
import os
from pathlib import Path
import shutil

from PIL import Image, ImageChops
from test_inplace_screenshots import ROOT, run_screenshot_pipeline, qt as qt_fixture

MANIFEST = ROOT / "tests/inplace/fixtures/visual-examples.json"


def pair(directory, prefix, number):
    matches = [p for p in directory.iterdir() if p.is_file()
               and p.stem == f"dialogue-{prefix}-{number}"]
    if len(matches) != 1:
        raise ValueError(f"expected one {prefix} example {number}, found {matches}")
    return matches[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "tmp/placement-quality/examples")
    parser.add_argument("--scenes", nargs="*", type=int)
    args = parser.parse_args()
    output = args.output.resolve()
    if not output.is_relative_to((ROOT / "tmp").resolve()):
        parser.error("artifacts must stay below project tmp/")
    if not os.environ.get("LIPAX_QT_MEASURE_SERVER"):
        parser.error("requires the production Qt measurement helper")
    output.mkdir(parents=True, exist_ok=True)
    manifest = json.loads(MANIFEST.read_text())
    # Cropped native RapidOCR is exercised below; no additional full-frame OCR.
    os.environ.pop("LIPAX_TEST_OCR", None)
    qt = qt_fixture.__wrapped__()
    report = dict(purpose=manifest["purpose"], compositor_test=False,
                  exact_reference_approval=False, unmapped_readings="retain English test input; not claimed translated", scenes=[])
    for scene in manifest["scenes"]:
        number = scene["id"]
        if args.scenes is not None and number not in args.scenes:
            continue
        source = pair(ROOT / manifest["source_directory"], "en", number)
        reference = pair(ROOT / manifest["reference_directory"], "ru", number)
        destination = output / str(number)
        destination.mkdir(exist_ok=True)
        rules = [[scene["needle"], scene["translation"]]] + manifest["additional_translations"]
        (destination / "translations.json").write_text(json.dumps(rules, ensure_ascii=False))
        item = dict(scene=number, source=source.name, reference=reference.name,
                    source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
                    reference_sha256=hashlib.sha256(reference.read_bytes()).hexdigest(),
                    review=scene["review"], focus_for_inspection=scene["focus"])
        case = "reviewed-examples"
        try:
            data = run_screenshot_pipeline(source, qt, case=case, ocr_blocks=True,
                unmapped_original=True,
                translation_rules=destination / "translations.json")
            item["structural_checks"] = "PASS"
        except Exception as error:
            # Keep failures visible in the report; do not promote an image to a golden.
            item["structural_checks"] = "FAIL"
            item["error"] = str(error)
            metadata = ROOT / "tmp/visual" / source.stem / case / "metadata.json"
            data = json.loads(metadata.read_text()) if metadata.exists() else {}
        artifacts = ROOT / "tmp/visual" / source.stem / case
        for name in ["rendered.png", "metadata.json", "detected.png"]:
            if (artifacts / name).exists():
                shutil.copyfile(artifacts / name, destination / name)
        Image.open(source).convert("RGB").save(destination / "original.jpg", quality=95)
        Image.open(reference).convert("RGB").save(destination / "reference.jpg", quality=95)
        item["rendered_fields"] = len(data.get("placed", []))
        item["dropped"] = data.get("dropped_details", [])
        item["coverage_diagnostics"] = data.get("overlay_quality", {})
        item["repeat_ocr_jobs"] = data.get("repeat_ocr_jobs")
        matched = [e for e in data.get("placed", []) if e["text"] == scene["translation"]]
        item["focus_translation_rendered"] = bool(matched)
        item["focus_outcomes"] = [dict(key=e["key"], outcome=e["outcome"], coverage=e["source_coverage"],
                                     degraded=e.get("degraded")) for e in matched]
        if (destination / "rendered.png").exists():
            rendered = Image.open(destination / "rendered.png").convert("RGB")
            expected = Image.open(reference).convert("RGB")
            if rendered.size == expected.size:
                ImageChops.difference(rendered, expected).save(destination / "example-difference.png")
        report["scenes"].append(item)
        print(json.dumps({k: item[k] for k in ["scene", "structural_checks", "rendered_fields", "focus_translation_rendered"]}), flush=True)
        (output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2))
    body = ['<!doctype html><meta charset="utf-8"><title>LipaX placement review</title>',
            '<style>body{background:#17191d;color:#eee;font:16px sans-serif;margin:24px} '
            '.images{display:flex;gap:12px}.images a{width:33%}img{width:100%}pre{white-space:pre-wrap}</style>',
            '<h1>Проверка размещения LipaX</h1><p>Эталоны — примеры; попиксельная приёмка не выполнялась. '
            'OCR настоящий, переводы заданы для теста. QML отрисован вне композитора. '
            'Focus — область просмотра, она не передаётся алгоритму.</p>']
    for item in report["scenes"]:
        number = item["scene"]
        body.append(f'<h2>Сцена {number}: {html.escape(item["structural_checks"])}</h2>'
                    f'<p>{html.escape(item["review"])}</p><div class="images">')
        for filename, label in [("original.jpg", "Оригинал"), ("reference.jpg", "Пример"), ("rendered.png", "LipaX")]:
            if (output / str(number) / filename).exists():
                body.append(f'<a href="{number}/{filename}">{label}<img src="{number}/{filename}"></a>')
        body.append('</div><pre>' + html.escape(json.dumps(item, ensure_ascii=False, indent=2)) + '</pre>')
    (output / "index.html").write_text("\n".join(body))
    if any(item["structural_checks"] == "FAIL" for item in report["scenes"]):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
