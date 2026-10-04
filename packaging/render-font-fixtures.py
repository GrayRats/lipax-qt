#!/usr/bin/env python3
"""Render real text with real fonts into small PNG fixtures with known labels.

Maintainers only. The Rust tests (crates/core/tests/font_fixtures.rs) read the committed PNGs and
manifest.json; this script is only needed to regenerate or extend them. It needs Pillow with
FreeType and the fonts themselves: the bundled ones (crates/app/assets/fonts, unpacked by a
cargo build) and a few system fonts, which stand in for the unknown fonts of a game.

    python3 packaging/render-font-fixtures.py            # writes crates/core/tests/fixtures/fonts
    python3 packaging/render-font-fixtures.py /tmp/out   # elsewhere (for experiments)
    python3 packaging/render-font-fixtures.py --all /tmp/out   # also proprietary system fonts

Only freely licensed fonts go into the committed set. `--all` adds MS core fonts (Arial, Times,
Georgia, Verdana, Courier, Impact...) as a local, never committed hold-out check.
"""
import json
import os
import random
import sys
from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ARGS = [a for a in sys.argv[1:] if a != "--all"]
ALL = "--all" in sys.argv[1:]
OUT = ARGS[0] if ARGS else os.path.join(ROOT, "crates/core/tests/fixtures/fonts")
PROPRIETARY = {"Arial", "Arial-bold", "Arial-italic", "Verdana", "Trebuchet", "Times", "Times-bold", "Times-italic",
               "Georgia", "Courier", "Courier-bold", "Impact"}
BUNDLED = os.path.join(ROOT, "crates/app/assets/fonts")
SYSTEM = "/usr/share/fonts/TTF"
LIBERATION = "/usr/share/fonts/liberation"

LATIN = "Hello, how are you today? Quick brown fox"
CYRILLIC = "Привет, как твои дела сегодня? Быстрая лиса"
CJK = "你好，世界，这是一个测试文本，请看这里"

# group: sans | serif | slab | mono | condensed | display | cjk
# weight: 400 (regular), 700 (bold), 300 (light); italic: real italic file or synthetic shear
FONTS = [
    # bundled, variable (weight axis set explicitly)
    ("Inter", BUNDLED, "Inter.ttf", "sans", {"wght": 400}, False),
    ("Inter-bold", BUNDLED, "Inter.ttf", "sans", {"wght": 700}, False),
    ("Inter-light", BUNDLED, "Inter.ttf", "sans", {"wght": 300}, False),
    ("Roboto", BUNDLED, "Roboto.ttf", "sans", {"wght": 400}, False),
    ("Roboto-bold", BUNDLED, "Roboto.ttf", "sans", {"wght": 700}, False),
    ("NotoSans", BUNDLED, "NotoSans.ttf", "sans", {"wght": 400}, False),
    ("OpenSans", BUNDLED, "OpenSans.ttf", "sans", {"wght": 400}, False),
    ("Montserrat", BUNDLED, "Montserrat.ttf", "sans", {"wght": 400}, False),
    ("FiraSans", BUNDLED, "FiraSans-Regular.ttf", "sans", {}, False),
    ("FiraSans-bold", BUNDLED, "FiraSans-Bold.ttf", "sans", {}, False),
    ("NotoSerif", BUNDLED, "NotoSerif.ttf", "serif", {"wght": 400}, False),
    ("NotoSerif-bold", BUNDLED, "NotoSerif.ttf", "serif", {"wght": 700}, False),
    ("SourceSerif4", BUNDLED, "SourceSerif4.ttf", "serif", {"wght": 400}, False),
    ("Literata", BUNDLED, "Literata.ttf", "serif", {"wght": 400}, False),
    ("Lora", BUNDLED, "Lora.ttf", "serif", {"wght": 400}, False),
    ("EBGaramond", BUNDLED, "EBGaramond.ttf", "serif", {"wght": 400}, False),
    ("PTSerif", BUNDLED, "PTSerif-Regular.ttf", "serif", {}, False),
    ("PTSerif-bold", BUNDLED, "PTSerif-Bold.ttf", "serif", {}, False),
    ("RobotoSlab", BUNDLED, "RobotoSlab.ttf", "slab", {"wght": 400}, False),
    ("RobotoSlab-bold", BUNDLED, "RobotoSlab.ttf", "slab", {"wght": 700}, False),
    ("Bitter", BUNDLED, "Bitter.ttf", "slab", {"wght": 400}, False),
    ("JetBrainsMono", BUNDLED, "JetBrainsMono.ttf", "mono", {"wght": 400}, False),
    ("FiraCode", BUNDLED, "FiraCode.ttf", "mono", {"wght": 400}, False),
    ("SourceCodePro", BUNDLED, "SourceCodePro.ttf", "mono", {"wght": 400}, False),
    ("RobotoCondensed", BUNDLED, "RobotoCondensed.ttf", "condensed", {"wght": 400}, False),
    # system fonts standing in for unknown game fonts
    ("Arial", SYSTEM, "Arial.TTF", "sans", {}, False),
    ("Arial-bold", SYSTEM, "Arialbd.TTF", "sans", {}, False),
    ("Arial-italic", SYSTEM, "Ariali.TTF", "sans", {}, True),
    ("Verdana", SYSTEM, "Verdana.TTF", "sans", {}, False),
    ("Trebuchet", SYSTEM, "trebuc.ttf", "sans", {}, False),
    ("LiberationSans", LIBERATION, "LiberationSans-Regular.ttf", "sans", {}, False),
    ("DejaVuSans", SYSTEM, "DejaVuSans.ttf", "sans", {}, False),
    ("DejaVuSans-bold", SYSTEM, "DejaVuSans-Bold.ttf", "sans", {}, False),
    ("Times", SYSTEM, "Times.TTF", "serif", {}, False),
    ("Times-bold", SYSTEM, "Timesbd.TTF", "serif", {}, False),
    ("Times-italic", SYSTEM, "Timesi.TTF", "serif", {}, True),
    ("Georgia", SYSTEM, "Georgia.TTF", "serif", {}, False),
    ("LiberationSerif", LIBERATION, "LiberationSerif-Regular.ttf", "serif", {}, False),
    ("DejaVuSerif", SYSTEM, "DejaVuSerif.ttf", "serif", {}, False),
    ("Courier", SYSTEM, "cour.ttf", "mono", {}, False),
    ("Courier-bold", SYSTEM, "courbd.ttf", "mono", {}, False),
    ("LiberationMono", LIBERATION, "LiberationMono-Regular.ttf", "mono", {}, False),
    ("DejaVuSansMono", SYSTEM, "DejaVuSansMono.ttf", "mono", {}, False),
    ("Hack", SYSTEM, "Hack-Regular.ttf", "mono", {}, False),
    ("Impact", SYSTEM, "Impact.TTF", "display", {}, False),
    # more free fonts: unusual designs and several narrow ones (only one is bundled)
    ("Vera", SYSTEM, "Vera.ttf", "sans", {}, False),
    ("VeraSerif", SYSTEM, "VeraSe.ttf", "serif", {}, False),
    ("VeraMono", SYSTEM, "VeraMono.ttf", "mono", {}, False),
    ("FantasqueMono", SYSTEM, "FantasqueSansMNerdFontMono-Regular.ttf", "mono", {}, False),
    ("Meslo", SYSTEM, "MesloLGMNerdFontMono-Regular.ttf", "mono", {}, False),
    ("LiberationSans-bold", LIBERATION, "LiberationSans-Bold.ttf", "sans", {}, False),
    ("LiberationSerif-bold", LIBERATION, "LiberationSerif-Bold.ttf", "serif", {}, False),
    ("LiberationSerif-italic", LIBERATION, "LiberationSerif-Italic.ttf", "serif", {}, True),
    ("LiberationSans-italic", LIBERATION, "LiberationSans-Italic.ttf", "sans", {}, True),
    ("FiraSansCondensed", SYSTEM, "FiraSansCondensed-Regular.ttf", "condensed", {}, False),
    ("FiraSansCompressed", SYSTEM, "FiraSansCompressed-Regular.ttf", "condensed", {}, False),
    ("OpenSansCondensed", SYSTEM, "OpenSans-CondensedRegular.ttf", "condensed", {}, False),
    ("DejaVuSansCondensed", SYSTEM, "DejaVuSansCondensed.ttf", "sans", {}, False),  # only ~10% narrower
    ("FiraSans-light", SYSTEM, "FiraSans-Light.ttf", "sans", {}, False),
    ("FiraSans-heavy", SYSTEM, "FiraSans-Heavy.ttf", "sans", {}, False),
    ("OpenSans-light", SYSTEM, "OpenSans-Light.ttf", "sans", {}, False),
    ("NotoSansCJK", BUNDLED, "NotoSansCJK-VF.otf", "cjk", {"wght": 400}, False),
]

# No Cyrillic glyphs: the renderer would draw identical .notdef boxes, which look monospaced.
LATIN_ONLY = {"Vera", "VeraSerif", "VeraMono"}

AXIS = {"wght": b"Weight", "wdth": b"Width", "opsz": b"Optical Size"}


def load(path, size, axes):
    font = ImageFont.truetype(path, size)
    if axes:
        try:
            declared = font.get_variation_axes()
        except (OSError, AttributeError):
            declared = []
        values = []
        for a in declared:
            name = a["name"] if isinstance(a["name"], bytes) else str(a["name"]).encode()
            tag = next((k for k, v in AXIS.items() if v == name), None)
            values.append(axes.get(tag, a["default"]))
        font.set_variation_by_axes(values)
    return font


def render(font, text, light_on_dark, shear, seed):
    # Small greyscale frames with a soft vertical gradient (game backgrounds are never flat):
    # the files stay a few KiB each, so the committed set is small.
    w, h = 660, 76
    base = 34 if light_on_dark else 228
    ink = 245 if light_on_dark else 24
    img = Image.new("L", (w, h))
    px = img.load()
    for y in range(h):
        v = max(0, min(255, base + int(10 * (y / h) - 5)))
        for x in range(w):
            px[x, y] = v
    ImageDraw.Draw(img).text((14, 54), text, font=font, fill=ink, anchor="ls")
    if shear:
        img = img.transform(img.size, Image.AFFINE, (1, shear, -shear * 54, 0, 1, 0), resample=Image.BICUBIC, fillcolor=base)
    return img


def main():
    os.makedirs(OUT, exist_ok=True)
    manifest = []
    missing = []
    for index, (name, folder, file, group, axes, real_italic) in enumerate(FONTS):
        path = os.path.join(folder, file)
        if name in PROPRIETARY and not ALL:
            continue
        if not os.path.exists(path):
            missing.append(path)
            continue
        weight = axes.get("wght", 400)
        if name.endswith("-bold"):
            weight = 700
        if name.endswith("-light"):
            weight = 300
        if name.endswith("-italic") and "bold" not in name:
            weight = 400
        if name in ("Impact", "FiraSans-heavy"):
            weight = 800 if name == "FiraSans-heavy" else 700
        size = 30
        texts = [("latin", CJK if group == "cjk" else LATIN)]
        if group != "cjk" and name not in LATIN_ONLY:
            texts.append(("cyr", CYRILLIC))
        for script, text in texts:
            for dark in (True, False) if script == "latin" else (True,):
                # Synthetic slant on the regular files too: the game may use italics.
                variants = [("", 0.0, real_italic)]
                if script == "latin" and dark and not real_italic and group != "cjk":
                    variants.append(("-oblique", 0.21, True))
                for suffix, shear, italic in variants:
                    font = load(path, size, axes)
                    fixture = f"{name}{suffix}-{script}-{'dark' if dark else 'light'}.png"
                    render(font, text, dark, shear, index).save(os.path.join(OUT, fixture), optimize=True)
                    manifest.append({"file": fixture, "font": name, "group": group, "weight": weight,
                                     "italic": italic, "script": script, "light_on_dark": dark})
    with open(os.path.join(OUT, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1, ensure_ascii=False)
    total = sum(os.path.getsize(os.path.join(OUT, m["file"])) for m in manifest)
    print(f"{len(manifest)} fixtures, {total / 1024:.0f} KiB in {OUT}")
    if missing:
        print("missing fonts (skipped):", *missing, sep="\n  ")


if __name__ == "__main__":
    main()
