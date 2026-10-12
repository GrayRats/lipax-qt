"""Real RapidOCR → production translator → Rust layout → QML images.

Uses local Bergamot by default; Google requires explicit authorization. Never injects generic translation text. The
result is an offscreen content render, not proof of compositor visibility.
"""
import argparse
import json
import os
from pathlib import Path
from test_inplace_screenshots import ROOT, SOURCES, qt as qt_fixture, run_screenshot_pipeline


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scenes", nargs="*", type=int)
    parser.add_argument("--save", action="store_true", help="save actual translation screenshots in docs/")
    parser.add_argument("--service", choices=["bergamot", "google"], default="bergamot")
    args = parser.parse_args()
    if not os.environ.get("LIPAX_QT_MEASURE_SERVER"):
        parser.error("requires the production Qt measurement helper")
    output = ROOT / "tmp/placement-quality/real-translations"
    output.mkdir(parents=True, exist_ok=True)
    os.environ.pop("LIPAX_TEST_OCR", None)
    if args.save:
        os.environ["LIPAX_SAVE_TRANSLATIONS"] = "1"
    qt = qt_fixture.__wrapped__()
    results = []
    for source in SOURCES:
        if not source.stem.startswith("dialogue-en-"):
            continue
        if args.scenes is not None and int(source.stem.rsplit("-", 1)[1]) not in args.scenes:
            continue
        try:
            data = run_screenshot_pipeline(source, qt, ocr_blocks=True,
                service_cache=output / f"{args.service}-en-ru-cache.json", translation_service=args.service)
            result = dict(source=source.name, status="PASS: structural render checks",
                rendered=len(data["placed"]), dropped=data["dropped_details"],
                translation_service=data["translation_service"], requests=data["translation_requests"],
                cached=data["translation_cache_hits"], quality=data["overlay_quality"],
                outcomes=[dict(id=e["block_id"], source=e["original"], translation=e["text"],
                    outcome=e["outcome"], font_px=e["font_px"], degraded=e.get("degraded")) for e in data["placed"]])
        except Exception as error:
            result = dict(source=source.name, status="FAIL", error=str(error))
        results.append(result)
        print(json.dumps({k: v for k, v in result.items() if k not in ["quality", "dropped", "outcomes"]}, ensure_ascii=False), flush=True)
        (output / "report.json").write_text(json.dumps(results, ensure_ascii=False, indent=2))
    if any(result["status"] == "FAIL" for result in results):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
