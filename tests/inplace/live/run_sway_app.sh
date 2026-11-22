#!/usr/bin/env bash
# Live check of the WHOLE application on sway (ROADMAP R-14): `lipax` runs in a HEADLESS sway (no screen, no GPU, private
# runtime directory and D-Bus under tmp/), the window to read is a Konsole window chosen by its sway key (`sway:<id>`), and
# the application must capture it through grim, recognise text and learn the window's geometry through the sway IPC.
# Translation is `custom` without a URL: nothing leaves the machine (the translation fails by design).
#
#   tests/inplace/live/run_sway_app.sh          sway from PATH, else the packages unpacked in $LIPAX_SWAY_ROOT (see run_sway.sh)
#   LIPAX_BIN=...  LIPAX_SWAY_SCALE=1.5  LIPAX_SWAY_KEEP=1
#
# Exit code: 0 = frames captured, text recognised and the geometry known, 1 = not, 2 = environment.
set -uo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.."
root=$PWD
work=$root/tmp/sway-app
scale=${LIPAX_SWAY_SCALE:-1}
fail() { echo "sway-app: $*" >&2; exit "${2:-2}"; }
sway=$(command -v sway || true)
if [[ -z $sway && -n ${LIPAX_SWAY_ROOT:-} ]]; then
    sway=$LIPAX_SWAY_ROOT/usr/bin/sway; export LIPAX_SWAY_LIBS=$LIPAX_SWAY_ROOT/usr/lib
fi
[[ -x $sway ]] || fail "sway not found (set LIPAX_SWAY_ROOT to unpacked sway and wlroots packages)"
for tool in grim konsole dbus-run-session tesseract python3; do command -v "$tool" >/dev/null 2>&1 || fail "$tool not found"; done
bin=${LIPAX_BIN:-${CARGO_TARGET_DIR:-$root/target}/release/lipax}
[[ -x $bin ]] || fail "$bin not found (cargo build --release -p lipa, or set LIPAX_BIN)"

rm -rf -- "$work"; mkdir -p -m 700 "$work/run" "$work/config/lipa" "$work/data" "$work/cache"
cat > "$work/sway.conf" <<EOF
output HEADLESS-1 resolution 1920x1080 scale $scale
default_border none
for_window [app_id="org.kde.konsole"] floating enable, resize set 900 500, move position 100 100
EOF
cat > "$work/inner.sh" <<'INNER'
#!/usr/bin/env bash
set -u
work=$1; sway=$2; bin=$3
export XDG_RUNTIME_DIR=$work/run XDG_CONFIG_HOME=$work/config XDG_DATA_HOME=$work/data XDG_CACHE_HOME=$work/cache XDG_SESSION_TYPE=wayland
export WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman XDG_CURRENT_DESKTOP=sway
unset WAYLAND_DISPLAY DISPLAY SWAYSOCK KDE_FULL_SESSION
LD_LIBRARY_PATH=${LIPAX_SWAY_LIBS:-} "$sway" -c "$work/sway.conf" --unsupported-gpu >"$work/sway.log" 2>&1 &
trap 'kill $(jobs -p) 2>/dev/null' EXIT
for _ in $(seq 1 60); do ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock >/dev/null 2>&1 && [ -S "$XDG_RUNTIME_DIR/wayland-1" ] && break; sleep 0.25; done
export SWAYSOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock 2>/dev/null | head -1)
[ -n "$SWAYSOCK" ] || { echo "sway-app: sway did not start" >&2; exit 2; }
export WAYLAND_DISPLAY=wayland-1 QT_QPA_PLATFORM=wayland
konsole --separate -e bash -c 'clear; for i in 1 2 3 4; do echo "THE QUICK BROWN FOX JUMPS OVER THE LAZY DOG"; echo "Hello world, this is a sway test"; done; sleep 300' >"$work/konsole.log" 2>&1 &
id=
for _ in $(seq 1 40); do
    id=$(LD_LIBRARY_PATH=${LIPAX_SWAY_LIBS:-} "$(dirname "$sway")/swaymsg" -t get_tree -r 2>/dev/null | python3 -c "
import json, sys
def walk(n):
    for c in n.get('nodes', []) + n.get('floating_nodes', []): yield from walk(c)
    if n.get('app_id') == 'org.kde.konsole': yield n['id']
print(next(walk(json.load(sys.stdin)), ''))")
    [ -n "$id" ] && break; sleep 0.5
done
[ -n "$id" ] || { echo "sway-app: the Konsole window did not appear" >&2; exit 2; }
echo "sway-app: target window sway:$id"
cat > "$XDG_CONFIG_HOME/lipa/config.toml" <<TOML
schema_version = 2
[capture]
source = "auto"
[capture.window]
uuid = "sway:$id"
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
kill -0 "$app" 2>/dev/null && echo "sway-app: application still running after 25 s" || echo "sway-app: application EXITED early"
kill "$app" 2>/dev/null; wait "$app" 2>/dev/null
INNER
chmod +x "$work/inner.sh"
timeout 150 dbus-run-session -- "$work/inner.sh" "$work" "$sway" "$bin" >"$work/run.log" 2>&1
grep -E '^sway-app' "$work/run.log"
for proc in /proc/[0-9]*; do
    if { tr '\0' '\n' < "$proc/environ" | grep -qxF "XDG_RUNTIME_DIR=$work/run"; } 2>/dev/null; then kill "${proc#/proc/}" 2>/dev/null; fi
done
log=$work/app.log
frames=$(grep -c "capture frame received" "$log" 2>/dev/null || true)
recognised=$(grep -c "Ошибка перевода" "$log" 2>/dev/null || true)
geometry=$(grep -c "in-place fields published geometry=true\|frame_requested\|game geometry" "$log" 2>/dev/null || true)
size=$(grep -o "capture frame received[^ ]* region=subtitles width=[0-9]* height=[0-9]*" "$log" | head -1 | grep -o "width=[0-9]* height=[0-9]*")
echo "sway-app: frames received: ${frames:-0} (${size:-no size}), texts recognised: ${recognised:-0}"
if grep -q "application EXITED early" "$work/run.log"; then echo "sway-app: FAIL - the application exited early - see $log"; result=1
elif [[ ${frames:-0} -ge 1 && ${recognised:-0} -ge 1 ]]; then echo "sway-app: PASS (scale $scale)"; result=0
else echo "sway-app: FAIL - no frame was captured and recognised - see $log"; result=1; fi
[[ -n ${LIPAX_SWAY_KEEP:-} ]] || rm -rf -- "$work/run" "$work/cache" "$work/data"
exit $result
