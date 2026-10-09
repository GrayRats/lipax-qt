#!/usr/bin/env python3
"""Refresh the bundled RapidOCR (PP-OCRv5 ONNX) model catalog from the RapidAI repository on ModelScope.

Only metadata is read (file list with sizes and SHA-256); no model is downloaded. The URLs are pinned to a tag, so a
later change of the repository cannot break the hashes. `--listing FILE` reads a saved answer of the files API instead
of asking the network.
"""
import argparse
import json
from pathlib import Path
from urllib.request import urlopen

REPOSITORY = "RapidAI/RapidOCR"
TAG = "v3.10.0"
LISTING = f"https://www.modelscope.cn/api/v1/models/{REPOSITORY}/repo/files?Revision={TAG}&Recursive=true"
RESOLVE = f"https://www.modelscope.cn/models/{REPOSITORY}/resolve/{TAG}/"

DET = {"mobile": "onnx/PP-OCRv5/det/ch_PP-OCRv5_det_mobile.onnx", "server": "onnx/PP-OCRv5/det/ch_PP-OCRv5_det_server.onnx"}
CLS = {"mobile": "onnx/PP-OCRv5/cls/ch_PP-LCNet_x0_25_textline_ori_cls_mobile.onnx",
       "server": "onnx/PP-OCRv5/cls/ch_PP-LCNet_x1_0_textline_ori_cls_server.onnx"}

# script, variant, Russian label, Tesseract language codes. The recognizer of PP-OCRv5 `ch` reads simplified and
# traditional Chinese, Japanese and English; the other scripts have a mobile recognizer only.
MODELS = [
    ("en", "mobile", "английский", ["eng"]),
    ("latin", "mobile", "латиница (европейские языки)", [
        "deu", "fra", "spa", "ita", "por", "pol", "nld", "swe", "dan", "nor", "ces", "slk", "slv", "hrv", "hun", "tur",
        "ind", "msa", "est", "lit", "isl", "bos", "sqi", "afr", "lat", "cym", "gle"]),
    ("eslav", "mobile", "кириллица: русский, украинский, белорусский", ["rus", "ukr", "bel"]),
    ("cyrillic", "mobile", "кириллица: болгарский, сербский и др.", ["bul", "srp", "mkd", "kaz", "mon"]),
    ("ch", "mobile", "китайский и японский", ["chi_sim", "chi_tra", "jpn"]),
    ("ch", "server", "китайский и японский (server)", ["chi_sim", "chi_tra", "jpn"]),
    ("korean", "mobile", "корейский", ["kor"]),
    ("el", "mobile", "греческий", ["ell"]),
    ("th", "mobile", "тайский", ["tha"]),
    ("arabic", "mobile", "арабское письмо", ["ara", "fas", "urd"]),
    ("devanagari", "mobile", "деванагари", ["hin", "mar", "nep", "san"]),
    ("ta", "mobile", "тамильский", ["tam"]),
    ("te", "mobile", "телугу", ["tel"]),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--listing", type=Path, help="saved answer of the ModelScope files API")
    args = parser.parse_args()
    if args.listing:
        data = json.loads(args.listing.read_text(encoding="utf-8"))
    else:
        with urlopen(LISTING, timeout=60) as response:
            data = json.load(response)
    files = {f["Path"]: f for f in data["Data"]["Files"] if f.get("Type") == "blob"}

    def entry(path):
        f = files[path]
        if len(f.get("Sha256") or "") != 64 or not f.get("Size"):
            raise RuntimeError(f"{path}: no size or SHA-256 in the listing")
        return {"name": path.rsplit("/", 1)[1], "size": f["Size"], "sha256": f["Sha256"], "url": RESOLVE + path}

    models = []
    for script, variant, label, languages in MODELS:
        rec = f"onnx/PP-OCRv5/rec/{script}_PP-OCRv5_rec_{variant}.onnx"
        dictionary = f"paddle/PP-OCRv5/rec/{script}_PP-OCRv5_rec_{variant}/"
        dictionary += "ppocrv5_dict.txt" if script == "ch" else f"ppocrv5_{script}_dict.txt"
        models.append({
            "id": f"{script}-{variant}", "script": script, "variant": variant, "label": label, "languages": languages,
            "version": f"PP-OCRv5 ({REPOSITORY} {TAG})", "license": "Apache-2.0",
            "files": {"det": entry(DET[variant]), "cls": entry(CLS[variant]), "rec": entry(rec), "dict": entry(dictionary)},
        })
    target = Path(__file__).resolve().parents[1] / "crates/core/src/ocr/ocr_models_catalog.json"
    target.write_text(json.dumps(models, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"Wrote {len(models)} model sets to {target}")


if __name__ == "__main__":
    main()
