#!/usr/bin/env python3
"""Render game-like scenes with real fonts for the end-to-end tests of the in-place engine.

Maintainers only (see render-font-fixtures.py). Scenes are drawn with the freely licensed
bundled fonts, so the committed PNGs carry no proprietary glyphs.

    python3 packaging/render-scene-fixtures.py            # crates/core/tests/fixtures/scenes
"""
import json
import math
import os
import sys
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "crates/core/tests/fixtures/scenes")
FONTS = os.path.join(ROOT, "crates/app/assets/fonts")
W, H = 960, 300


def font(file, size, wght=None):
    f = ImageFont.truetype(os.path.join(FONTS, file), size)
    if wght:
        axes = f.get_variation_axes()
        f.set_variation_by_axes([wght if a["name"] in (b"Weight", "Weight") else a["default"] for a in axes])
    return f


def sky(top, bottom):
    img = Image.new("RGB", (W, H))
    px = img.load()
    for y in range(H):
        t = y / H
        c = tuple(int(top[i] * (1 - t) + bottom[i] * t) for i in range(3))
        for x in range(W):
            # Soft hills so the background is not a flat gradient.
            hill = 1 + 0.06 * math.sin(x / 70.0 + y / 90.0)
            px[x, y] = tuple(max(0, min(255, int(v * hill))) for v in c)
    return img


def panel(img, box, fill, radius=14, border=None):
    over = Image.new("RGBA", img.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(over)
    d.rounded_rectangle(box, radius=radius, fill=fill, outline=border, width=2 if border else 0)
    return Image.alpha_composite(img.convert("RGBA"), over).convert("RGB")


def dialogue():
    img = panel(sky((92, 118, 150), (42, 52, 40)), (40, 40, W - 40, H - 30), (18, 20, 32, 215), border=(190, 160, 90, 255))
    d = ImageDraw.Draw(img)
    d.text((70, 82), "Elder Maren", font=font("Lora.ttf", 28, 700), fill=(240, 196, 92), anchor="ls")
    body = font("PTSerif-Regular.ttf", 26)
    for i, line in enumerate(["The road north is closed. Bandits have taken", "the old bridge, and no caravan has passed", "through the valley in days. Will you help us?"]):
        d.text((70, 128 + i * 34), line, font=body, fill=(238, 236, 228), anchor="ls")
    for x, label in ((70, "Accept"), (250, "Decline")):
        d.rounded_rectangle((x, 232, x + 150, 272), radius=8, fill=(52, 60, 84), outline=(150, 160, 190), width=2)
        d.text((x + 75, 260), label, font=font("Inter.ttf", 22, 600), fill=(236, 240, 250), anchor="ms")
    return img


def subtitles():
    img = sky((196, 214, 232), (120, 140, 96))
    d = ImageDraw.Draw(img)
    f = font("Roboto.ttf", 34, 500)
    for i, line in enumerate(["I never thought we would", "make it out of the canyon alive."]):
        d.text((W // 2, 226 + i * 42), line, font=f, fill=(255, 255, 255), anchor="ms", stroke_width=3, stroke_fill=(0, 0, 0))
    return img


def menu():
    img = panel(sky((22, 26, 44), (10, 12, 22)), (60, 30, 420, H - 30), (0, 0, 0, 120), radius=6)
    d = ImageDraw.Draw(img)
    f = font("JetBrainsMono.ttf", 26)
    for i, label in enumerate(["> New Game", "  Load Game", "  Options", "  Quit"]):
        d.text((100, 84 + i * 56), label, font=f, fill=(120, 230, 170) if i == 0 else (200, 205, 215), anchor="ls")
    return img


SCENES = {
    # expected: the number of fields, which of them are multi-line, and what the font should look like
    "dialogue": (dialogue, {"fields": 4, "multiline": 1, "kinds": ["serif", "serif", "sans", "sans"]}),
    "subtitles": (subtitles, {"fields": 1, "multiline": 1, "kinds": ["sans"]}),
    "menu": (menu, {"fields": 4, "multiline": 0, "kinds": ["mono", "mono", "mono", "mono"]}),
}


def main():
    os.makedirs(OUT, exist_ok=True)
    manifest = {}
    for name, (draw, expect) in SCENES.items():
        draw().save(os.path.join(OUT, f"{name}.png"), optimize=True)
        manifest[name] = expect
    with open(os.path.join(OUT, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1)
    total = sum(os.path.getsize(os.path.join(OUT, n + ".png")) for n in SCENES)
    print(f"{len(SCENES)} scenes, {total / 1024:.0f} KiB in {OUT}")


if __name__ == "__main__":
    main()
