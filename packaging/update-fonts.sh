#!/usr/bin/env bash
# Maintainers only: refresh the compressed fonts stored in the repository
# (crates/app/assets/fonts/*.xz) from the pinned upstream files in packaging/fonts.sources.
# Every download is verified against its sha256 first. Normal builds never touch the network:
# crates/app/build.rs unpacks the tracked *.xz.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
font_dir=crates/app/assets/fonts
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
while read -r name digest url; do
    [[ -z ${name:-} || $name == \#* ]] && continue
    curl --globoff --fail --location --retry 3 --output "$work/$name" "$url"
    printf '%s  %s\n' "$digest" "$work/$name" | sha256sum --check --status
    if [[ $name == *.txt ]]; then cp -- "$work/$name" "$font_dir/$name"
    else xz -9e -T0 -c "$work/$name" > "$font_dir/$name.xz"; fi
done < packaging/fonts.sources
