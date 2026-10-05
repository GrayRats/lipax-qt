#!/usr/bin/env bash
# KWin authorizes the executable path, not QGuiApplication's desktop ID.
# Register only this development binary; keep the installed launcher untouched.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
lipa_root=$PWD
if [[ -z ${LIPAX_LOCAL_BINARY:-} ]]; then
    cargo build -p lipa
fi
lipa_binary=$(realpath -- "${LIPAX_LOCAL_BINARY:-${CARGO_TARGET_DIR:-$lipa_root/target}/debug/lipax}")
[[ -x $lipa_binary ]] || { printf 'Not executable: %s\n' "$lipa_binary" >&2; exit 1; }
lipa_desktop_dir=${XDG_DATA_HOME:-$HOME/.local/share}/applications
mkdir -p -- "$lipa_desktop_dir"
python3 - "$lipa_binary" "$lipa_desktop_dir/io.lipa.Translator.Development.desktop" <<'PY'
import pathlib
import sys

binary, destination = sys.argv[1:]
# Desktop Entry Exec quoting has two levels: string escapes, then argument escapes.
argument = binary.replace('%', '%%')
for character in ('\\', '"', '`', '$'):
    argument = argument.replace(character, '\\' + character)
argument = ('"' + argument + '"').replace('\\', '\\\\')
pathlib.Path(destination).write_text(
    '[Desktop Entry]\nType=Application\nName=LipaX Development\nNoDisplay=true\n'
    f'Exec={argument}\nIcon=io.lipa.Translator\nTerminal=false\n'
    'X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2\n', encoding='utf-8')
PY
kbuildsycoca6
exec "$lipa_binary" "$@"
