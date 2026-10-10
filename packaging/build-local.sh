#!/usr/bin/env bash
# Build an Arch package from the current working tree, including uncommitted changes.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
bash packaging/check-qt.sh
lipa_root=$PWD
# PKGBUILD is what makepkg builds, so its pkgver is the version; Cargo.toml and Cargo.lock follow it.
lipa_version=$(sed -n 's/^pkgver=//p' PKGBUILD | head -1)
[[ $lipa_version =~ ^[0-9]+(\.[0-9]+)*$ ]] || { echo "build-local: no valid pkgver in PKGBUILD" >&2; exit 1; }
cargo_version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
if [[ $cargo_version != "$lipa_version" ]]; then
    echo "build-local: Cargo version $cargo_version -> $lipa_version (as in PKGBUILD)"
    sed -i "0,/^version = \".*\"/s//version = \"$lipa_version\"/" Cargo.toml
    sed -i "/^name = \"lipa\(-core\)\?\"\$/{n;s/^version = \".*\"/version = \"$lipa_version\"/}" Cargo.lock
fi
mkdir -p dist
# Archives and packages of other versions would only confuse makepkg and the install command.
find dist -maxdepth 1 -type f \( -name 'lipax-*.tar.gz' -o -name 'lipax-*.pkg.tar.*' \) ! -name "lipax-${lipa_version}[.-]*" -delete
# Explicit list avoids capturing credentials, target/, old archives or .git/.
# Only files required to build and package the application are included.
tar --exclude='__pycache__' --exclude='.qmlls.ini' --exclude='crates/app/assets/fonts/*.ttf' --exclude='crates/app/assets/fonts/*.otf' --transform="s,^,lipax-${lipa_version}/," \
    -czf "dist/lipax-${lipa_version}.tar.gz" Cargo.toml Cargo.lock LICENSE PKGBUILD README.md crates docs packaging
cp PKGBUILD dist/PKGBUILD
cd dist
export LIPA_LOCAL_SOURCE=1
export CARGO_TARGET_DIR="$lipa_root/target"
# Drop the previously extracted snapshot: removed/renamed files must not survive
# into the next build (for example layout.rs after migration to layout/mod.rs).
makepkg --force --cleanbuild "$@"
