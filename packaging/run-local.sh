#!/usr/bin/env bash
# Run a local build of LipaX so that KWin lets it capture windows.
#
# KWin allows org.kde.KWin.ScreenShot2 only to a program whose desktop file lists the interface in
# X-KDE-DBUS-Restricted-Interfaces and whose Exec is the path of the running executable. The package's
# desktop file names /usr/bin/lipax, so target/debug/lipax is refused with
# "ScreenShot2.Error.NoAuthorized". This script builds the application and registers a separate hidden
# desktop entry for the path of the local binary; the installed launcher stays untouched.
#
#   packaging/run-local.sh [lipax arguments]      build (debug) and run
#   LIPAX_PROFILE=release packaging/run-local.sh  build and run the release binary
#   LIPAX_LOCAL_BINARY=/full/path/to/lipax packaging/run-local.sh   do not build, run this binary
#   packaging/run-local.sh --unregister           remove the registration
#
# Close another LipaX first: a second start only forwards its command to the running one.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
root=$PWD
data_home=${XDG_DATA_HOME:-$HOME/.local/share}
entry="$data_home/applications/io.lipa.Translator.Development.desktop"

fail() { echo "run-local: $*" >&2; exit 1; }

# KWin reads desktop files through the KDE service cache.
refresh_cache() {
    if command -v kbuildsycoca6 >/dev/null 2>&1; then
        kbuildsycoca6 >/dev/null 2>&1 || echo "run-local: kbuildsycoca6 failed; run it by hand if KWin still refuses" >&2
    else
        echo "run-local: kbuildsycoca6 not found; log out and in again if KWin still refuses" >&2
    fi
}

if [[ ${1:-} == --unregister ]]; then
    rm -f -- "$entry"
    refresh_cache
    echo "run-local: removed $entry"
    exit 0
fi

binary=${LIPAX_LOCAL_BINARY:-}
if [[ -z $binary ]]; then
    # The system rust may be broken against the system LLVM; rustup's stable is the supported toolchain.
    if command -v rustup >/dev/null 2>&1 && [[ -z ${RUSTUP_TOOLCHAIN:-} ]]; then export RUSTUP_TOOLCHAIN=stable; fi
    target=${CARGO_TARGET_DIR:-$root/target}
    case ${LIPAX_PROFILE:-debug} in
        debug) cargo build -p lipa; binary=$target/debug/lipax ;;
        release) cargo build --release -p lipa; binary=$target/release/lipax ;;
        *) fail "LIPAX_PROFILE must be debug or release" ;;
    esac
fi
[[ -x $binary ]] || fail "not an executable file: $binary"
# KWin compares the real path of the process, so symlinks are resolved.
binary=$(realpath -- "$binary")
[[ $binary != *$'\n'* ]] || fail "the path contains a newline"

# A running instance owns the bus name; this start would only hand it a command.
bus_name=io.lipa.Translator${LIPAX_INSTANCE_ID:+.$LIPAX_INSTANCE_ID}
if command -v busctl >/dev/null 2>&1 && busctl --user list --no-legend 2>/dev/null | awk '{print $1}' | grep -qx -- "$bus_name"; then
    fail "another LipaX is running ($bus_name): close it first (lipax --quit)"
fi

# Desktop Entry spec: a percent sign is doubled; a path with reserved characters (space, quote, backslash,
# $, backtick, shell operators) is quoted, and the quoted string is escaped on two levels.
exec_path=${binary//%/%%}
if [[ $exec_path == *[[:space:]\"\'\\\>\<~\|\&\;\$\*\?\#\(\)\`]* ]]; then
    for character in '\' '"' '`' '$'; do exec_path=${exec_path//"$character"/"\\$character"}; done
    exec_path="\"$exec_path\""
    exec_path=${exec_path//\\/\\\\}
fi
content="[Desktop Entry]
Type=Application
Name=LipaX (local build)
Exec=$exec_path
NoDisplay=true
Icon=io.lipa.Translator
Terminal=false
X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2
"
mkdir -p -- "$(dirname -- "$entry")"
if [[ ! -f $entry ]] || [[ $(<"$entry") != "${content%$'\n'}" ]]; then
    printf '%s' "$content" > "$entry.tmp"
    mv -- "$entry.tmp" "$entry"
    echo "run-local: registered $binary for KWin screen capture"
    refresh_cache
fi

exec "$binary" "$@"
