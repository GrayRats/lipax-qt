#!/usr/bin/env bash
# Fresh build of LipaX: full `cargo clean` (the dependencies too) and a compile from scratch.
#
#   packaging/build-fresh.sh            release build
#   LIPAX_PROFILE=debug packaging/build-fresh.sh
#
# The result is target/<profile>/lipax. Sources of dependencies stay in the cargo cache; only their builds are redone.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

# The system rust may be broken against the system LLVM; rustup's stable is the supported toolchain.
if command -v rustup >/dev/null 2>&1 && [[ -z ${RUSTUP_TOOLCHAIN:-} ]]; then export RUSTUP_TOOLCHAIN=stable; fi

profile=${LIPAX_PROFILE:-release}
case $profile in
    release) flags=(--release) ;;
    debug) flags=() ;;
    *) echo "build-fresh: LIPAX_PROFILE must be debug or release" >&2; exit 1 ;;
esac

target=${CARGO_TARGET_DIR:-$PWD/target}
cargo clean

cargo build "${flags[@]}" -p lipa
ls -lh -- "$target/$profile/lipax"
