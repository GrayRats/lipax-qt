#!/usr/bin/env bash
# Build an Arch package from the current working tree, including uncommitted changes.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
lipa_root=$PWD
lipa_version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
mkdir -p dist
# Explicit list avoids capturing credentials, target/, old archives or .git/.
tar --exclude='__pycache__' --exclude='.qmlls.ini' --transform="s,^,LipaXQT-${lipa_version}/," \
    -czf "dist/LipaXQT-${lipa_version}.tar.gz" Cargo.toml Cargo.lock LICENSE PKGBUILD README.md PLAN.md crates docs packaging
cp PKGBUILD dist/PKGBUILD
cd dist
export LIPA_LOCAL_SOURCE=1
export CARGO_TARGET_DIR="$lipa_root/target"
makepkg --force "$@"
