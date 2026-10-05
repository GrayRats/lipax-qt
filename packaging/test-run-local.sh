#!/usr/bin/env bash
# Regression for run-local.sh: the desktop entry that lets KWin capture a local build.
# Runs in a scratch XDG_DATA_HOME with a stub kbuildsycoca6; the real KDE cache and launcher are not touched.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
script=$PWD/packaging/run-local.sh
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
mkdir -p "$scratch/bin" "$scratch/data" "$scratch/my build"
printf '#!/bin/sh\necho refreshed >> "%s/cache.log"\n' "$scratch" > "$scratch/bin/kbuildsycoca6"
printf '#!/bin/sh\necho "ran with: $*"\n' > "$scratch/fake-lipax"
cp "$scratch/fake-lipax" "$scratch/my build/lipax"
chmod +x "$scratch/bin/kbuildsycoca6" "$scratch/fake-lipax" "$scratch/my build/lipax"
export PATH="$scratch/bin:$PATH" XDG_DATA_HOME="$scratch/data" LIPAX_INSTANCE_ID="runlocal$$"
entry="$scratch/data/applications/io.lipa.Translator.Development.desktop"
run() { LIPAX_LOCAL_BINARY=$1 "$script" "${@:2}"; }

# 1. First run: the entry names the real path, is hidden, and allows ScreenShot2; the command runs with its arguments.
out=$(run "$scratch/fake-lipax" --show --flag)
grep -Fq 'ran with: --show --flag' <<<"$out"
grep -Fq "registered $scratch/fake-lipax" <<<"$out"
grep -Fxq "Exec=$scratch/fake-lipax" "$entry"
grep -Fxq 'NoDisplay=true' "$entry"
grep -Fxq 'X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2' "$entry"
[[ $(wc -l < "$scratch/cache.log") == 1 ]]

# 2. Same binary again: nothing is rewritten and the KDE cache is not rebuilt.
out=$(run "$scratch/fake-lipax")
! grep -Fq registered <<<"$out"
[[ $(wc -l < "$scratch/cache.log") == 1 ]]

# 3. Another binary: the entry follows it. A path with spaces is quoted, as the Desktop Entry spec wants.
run "$scratch/my build/lipax" >/dev/null
grep -Fxq "Exec=\"$scratch/my build/lipax\"" "$entry"
[[ $(wc -l < "$scratch/cache.log") == 2 ]]

# 3b. Reserved characters get the two levels of escaping the Desktop Entry spec asks for: \$ in the argument, the
# backslash doubled in the string. A percent sign is doubled.
mkdir -p "$scratch"/'we$ird%dir'
cp "$scratch/fake-lipax" "$scratch"/'we$ird%dir/lipax'
run "$scratch"/'we$ird%dir/lipax' >/dev/null
grep -Fxq "Exec=\"$scratch/we\\\\\$ird%%dir/lipax\"" "$entry"

# 4. A symlink is resolved: KWin compares the real path of the process.
ln -s "$scratch/fake-lipax" "$scratch/link"
run "$scratch/link" >/dev/null
grep -Fxq "Exec=$scratch/fake-lipax" "$entry"

# 5. Not an executable file: a clear error, the entry is left alone.
before=$(<"$entry")
if run "$scratch/missing" 2>"$scratch/err"; then echo "a missing binary must fail" >&2; exit 1; fi
grep -Fq 'not an executable file' "$scratch/err"
[[ $(<"$entry") == "$before" ]]

# 6. --unregister removes the entry and refreshes the cache.
"$script" --unregister >/dev/null
[[ ! -e $entry ]]
[[ $(wc -l < "$scratch/cache.log") == 5 ]]
echo "run-local.sh: ok"
