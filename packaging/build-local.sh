#!/usr/bin/env bash
# Build an Arch package from the current working tree, including uncommitted changes.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
lipa_root=$PWD
lipa_version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
./packaging/fetch-fonts.sh
mkdir -p dist
# Explicit list avoids capturing credentials, target/, old archives or .git/.
# Planning files (such as PLAN.md) are not required to build the application.
tar --exclude='__pycache__' --exclude='.qmlls.ini' --exclude='crates/app/assets/fonts' --transform="s,^,lipax-${lipa_version}/," \
    -czf "dist/lipax-${lipa_version}.tar.gz" Cargo.toml Cargo.lock LICENSE PKGBUILD README.md crates docs packaging
cp PKGBUILD dist/PKGBUILD
cp packaging/fonts.sources dist/fonts.sources
cp -f crates/app/assets/fonts/* dist/
cd dist
export LIPA_LOCAL_SOURCE=1
export CARGO_TARGET_DIR="$lipa_root/target"
# Drop the previously extracted snapshot: removed/renamed files must not survive
# into the next build (for example layout.rs after migration to layout/mod.rs).
makepkg --force --cleanbuild "$@"
