#!/usr/bin/env bash
# Populate the ignored development font directory from pinned, checked upstream files.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
font_dir=crates/app/assets/fonts
mkdir -p "$font_dir"
while read -r name digest url; do
    [[ -z ${name:-} || $name == \#* ]] && continue
    dest="$font_dir/$name"
    if [[ ! -f $dest ]] || ! printf '%s  %s\n' "$digest" "$dest" | sha256sum --check --status; then
        curl --globoff --fail --location --retry 3 --output "$dest.tmp" "$url"
        printf '%s  %s\n' "$digest" "$dest.tmp" | sha256sum --check --status
        mv -- "$dest.tmp" "$dest"
    fi
done < packaging/fonts.sources
