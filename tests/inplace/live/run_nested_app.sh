#!/usr/bin/env bash
# Live check of the WHOLE application inside a NESTED virtual KWin (ROADMAP R-10 item 1, the part run_nested_kwin.sh
# leaves open): `lipax` itself starts, finds a client window through its KWin script, captures it through ScreenShot2
# and recognises it, with its own windows next to the target. Nothing of the user's session is touched: a private D-Bus,
# private XDG_* directories under tmp/, a settings file written for the run. The translation service is `custom` without
# a URL, so no text leaves the machine (the translation fails by design; only capture and recognition are checked).
#
#   tests/inplace/live/run_nested_app.sh               uses target/debug/lipax (or LIPAX_BIN)
#   LIPAX_NESTED_SCALE=1.0 LIPAX_NESTED_OUTPUTS=1 ...  other output scale / count (defaults: 1.5 and 2)
#   LIPAX_NESTED_KEEP=1                                keep the work directory (logs)
#
# Exit code: 0 = the application captured and recognised frames of the target window, 1 = it did not, 2 = environment.
set -uo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.."
root=$PWD
work=$root/tmp/nested-kwin/app
scale=${LIPAX_NESTED_SCALE:-1.5}
outputs=${LIPAX_NESTED_OUTPUTS:-2}
fail() { echo "nested-app: $*" >&2; exit "${2:-2}"; }

for tool in kwin_wayland dbus-run-session busctl kbuildsycoca6 konsole tesseract; do
    command -v "$tool" >/dev/null 2>&1 || fail "$tool not found"
done
bin=${LIPAX_BIN:-${CARGO_TARGET_DIR:-$root/target}/debug/lipax}
[[ -x $bin ]] || fail "$bin not found (cargo build, or set LIPAX_BIN)"
bin=$(realpath -- "$bin")   # KWin compares the real path of the process
[[ $bin != *[[:space:]\"\'\\\$\`\%]* ]] || fail "the binary path contains characters that need quoting: $bin"

rm -rf -- "$work"; mkdir -p -m 700 "$work/run" "$work/data/applications" "$work/config/lipa" "$work/cache"
cat > "$work/data/applications/lipax-nested-app.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=LipaX nested app test
Exec=$bin
NoDisplay=true
Terminal=false
X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2
DESKTOP

inner=$work/inner.sh
cat > "$inner" <<'INNER'
#!/usr/bin/env bash
set -u
work=$1; bin=$2; scale=$3; outputs=$4
export XDG_RUNTIME_DIR=$work/run XDG_DATA_HOME=$work/data XDG_CONFIG_HOME=$work/config XDG_CACHE_HOME=$work/cache
export XDG_DATA_DIRS="$work/data:/usr/local/share:/usr/share" XDG_SESSION_TYPE=wayland
unset WAYLAND_DISPLAY DISPLAY KDE_FULL_SESSION
kbuildsycoca6 >"$work/sycoca.log" 2>&1
kwin_wayland --virtual --socket lipax-nest --width 1920 --height 1080 --output-count "$outputs" \
    --no-lockscreen --no-global-shortcuts --no-kactivities >"$work/kwin.log" 2>&1 &
trap 'kill $(jobs -p) 2>/dev/null' EXIT
for _ in $(seq 1 60); do [ -S "$XDG_RUNTIME_DIR/lipax-nest" ] && break; sleep 0.25; done
[ -S "$XDG_RUNTIME_DIR/lipax-nest" ] || { echo "nested-app: KWin did not create its socket" >&2; exit 2; }
sleep 2
export WAYLAND_DISPLAY=lipax-nest QT_QPA_PLATFORM=wayland
# `--scale` of a virtual KWin only enlarges the mode at output scale 1 (measured 2026-11-22): the real scale is set here.
if [ "$scale" != 1 ] && [ "$scale" != 1.0 ]; then
    for name in $(kscreen-doctor -j 2>/dev/null | python3 -c "import sys,json; print(' '.join(o['name'] for o in json.load(sys.stdin)['outputs'] if o['enabled']))"); do
        kscreen-doctor "output.$name.scale.$scale" >>"$work/scale.log" 2>&1
    done
    sleep 1
fi
# Large plain text: the check is about the capture path, not about OCR quality.
konsole --separate -e bash -c 'clear; for i in 1 2 3 4; do echo "THE QUICK BROWN FOX JUMPS OVER THE LAZY DOG"; echo "Hello world, this is a nested test"; done; sleep 300' >"$work/konsole.log" 2>&1 &
uuid=
for _ in $(seq 1 40); do
    uuid=$(busctl --user call org.kde.KWin /WindowsRunner org.kde.krunner1 Match s "sleep" 2>/dev/null | grep -oE '\{[0-9a-f-]{36}\}' | head -1)
    [ -n "$uuid" ] && break; sleep 0.5
done
[ -n "$uuid" ] || { echo "nested-app: no client window appeared in the nested KWin" >&2; exit 2; }
echo "nested-app: target window $uuid"
cat > "$XDG_CONFIG_HOME/lipa/config.toml" <<TOML
schema_version = 2
[capture]
source = "kwin"
[capture.window]
uuid = "$uuid"
resource_class = "org.kde.konsole"
caption = "konsole"
[[capture.regions]]
id = "subtitles"
name = "Subtitles"
enabled = true
rect = { x = 0.0, y = 0.0, w = 1.0, h = 1.0 }
[recognition]
engine = "tesseract"
language = "eng"
[translation]
service = "custom"
auto_translate = true
TOML
RUST_LOG=debug "$bin" --start-autotranslate >"$work/app.log" 2>&1 &
app=$!
sleep 25
kill -0 "$app" 2>/dev/null && echo "nested-app: application still running after 25 s" || echo "nested-app: application EXITED early"
kill "$app" 2>/dev/null; wait "$app" 2>/dev/null
INNER
chmod +x "$inner"

timeout 240 dbus-run-session -- "$inner" "$work" "$bin" "$scale" "$outputs" >"$work/run.log" 2>&1
grep -E '^nested-app' "$work/run.log"
for proc in /proc/[0-9]*; do
    if { tr '\0' '\n' < "$proc/environ" | grep -qxF "XDG_RUNTIME_DIR=$work/run"; } 2>/dev/null; then kill "${proc#/proc/}" 2>/dev/null; fi
done
log=$work/app.log
frames=$(grep -c "capture frame received" "$log" 2>/dev/null || true)
# Recognition happened when the (deliberately unconfigured) translation was asked for text.
recognised=$(grep -c "Ошибка перевода" "$log" 2>/dev/null || true)
denied=$(grep -ci "NoAuthorized\|не разрешил" "$log" 2>/dev/null || true)
echo "nested-app: frames received: ${frames:-0}, texts recognised: ${recognised:-0}, authorization refusals: ${denied:-0}"
grep -E "KWin|ScreenShot2" "$log" | grep -iE "error|warn|ошибк" | head -5
if grep -q "application EXITED early" "$work/run.log"; then
    echo "nested-app: FAIL - the application exited early - see $log"; result=1
elif [[ ${denied:-0} -gt 0 ]]; then
    echo "nested-app: FAIL - KWin refused the capture - see $log"; result=1
elif [[ ${frames:-0} -ge 1 && ${recognised:-0} -ge 1 ]]; then
    echo "nested-app: PASS (frames captured by the application: $frames, scale $scale, $outputs outputs)"; result=0
else
    echo "nested-app: FAIL - no frame was captured and recognised - see $log"; result=1
fi
[[ -n ${LIPAX_NESTED_KEEP:-} ]] || rm -rf -- "$work/run" "$work/cache" "$work/config" "$work/data"
exit $result
