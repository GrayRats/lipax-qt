#!/usr/bin/env bash
# Puts the frames the reference measure needs (tests/reference_metric.rs) side by side as PNG in one directory:
#   orig/dialogue-en-N.png   the game frame (docs/screenshots/test-game)
#   ours/dialogue-en-N.png   the frame with our translation (docs/screenshots/test-game/translations)
#   ref/dialogue-ru-N.png    the reference (docs/screenshots/test-game/Эталоны)
# The references are mostly JPEG, which the Rust image crate of this build does not read: ImageMagick converts them
# once. Nothing is downloaded or installed. Usage: tests/inplace/prepare-references.sh [output dir]   (default tmp/work/refs)
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.."
command -v magick >/dev/null 2>&1 || { echo "prepare-references: ImageMagick (magick) is needed to convert the JPEG references" >&2; exit 1; }
out=${1:-tmp/work/refs}
shots=docs/screenshots/test-game
mkdir -p "$out/orig" "$out/ours" "$out/ref"
count=0
for ref in "$shots"/Эталоны/dialogue-ru-*.*; do
    [[ -f $ref ]] || continue
    name=${ref##*/}; n=${name#dialogue-ru-}; n=${n%.*}
    original=$(ls "$shots"/dialogue-en-"$n".* 2>/dev/null | head -n 1 || true)
    [[ -n $original ]] || { echo "dialogue $n: no original frame, skipped" >&2; continue; }
    magick "$ref" "$out/ref/dialogue-ru-$n.png"
    magick "$original" "$out/orig/dialogue-en-$n.png"
    if [[ -f $shots/translations/dialogue-en-$n.png ]]; then cp -f -- "$shots/translations/dialogue-en-$n.png" "$out/ours/dialogue-en-$n.png"; else echo "dialogue $n: no translated frame of ours" >&2; fi
    count=$((count + 1))
done
echo "prepared $count frames in $out"
