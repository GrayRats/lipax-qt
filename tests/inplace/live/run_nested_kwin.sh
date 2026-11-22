#!/usr/bin/env bash
# Live check of KWin window capture inside a NESTED virtual KWin (PLAN.md P-01, ROADMAP R-10 item 1).
#
# Starts `kwin_wayland --virtual` on a private D-Bus with its own XDG_* directories under tmp/, so the user's
# session, desktop entries and KDE service cache are not touched. A Konsole window is the capture target; the
# `geometry` example (lipa-core) captures it through ScreenShot2. ScreenShot2 is only allowed for a program whose
# desktop entry lists X-KDE-DBUS-Restricted-Interfaces and whose Exec is the program's path, so the script writes
# such an entry for the example binary into the isolated data directory and refreshes the isolated service cache.
#
#   tests/inplace/live/run_nested_kwin.sh              build the example if needed, run, print PASS/FAIL
#   LIPAX_NESTED_SCALE=1.0 LIPAX_NESTED_OUTPUTS=1 ...  other output scale / count (defaults: 1.5 and 2)
#   LIPAX_NESTED_TARGET=qml                            capture a 400x300 Qt window (it uses the fractional scale) instead of Konsole;
#                                                      PASS then also needs the frame to be 400*scale x 300*scale
#   LIPAX_NESTED_TARGET=wine                           capture a Wine window (Notepad, forces XWayland); prefix in tmp/wine-prefix
#   LIPAX_NESTED_XWAYLAND=1                            run the target as an X11 client (QT_QPA_PLATFORM=xcb) through XWayland
#   LIPAX_NESTED_KEEP=1                                keep the work directory (logs, capture.png)
#
# Exit code: 0 = captured a non-empty frame of the client window, 1 = capture failed or was refused, 2 = environment.
set -uo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.."
root=$PWD
work=$root/tmp/nested-kwin/work
scale=${LIPAX_NESTED_SCALE:-1.5}
outputs=${LIPAX_NESTED_OUTPUTS:-2}
client=${LIPAX_NESTED_TARGET:-konsole}
xwayland=${LIPAX_NESTED_XWAYLAND:-}
[[ $client == wine ]] && { xwayland=1; command -v wine >/dev/null || fail "wine not found"; }
fail() { echo "nested-kwin: $*" >&2; exit "${2:-2}"; }

for tool in kwin_wayland dbus-run-session busctl kbuildsycoca6 konsole; do
    command -v "$tool" >/dev/null 2>&1 || fail "$tool not found"
done
if command -v rustup >/dev/null 2>&1 && [[ -z ${RUSTUP_TOOLCHAIN:-} ]]; then export RUSTUP_TOOLCHAIN=stable; fi
target=${CARGO_TARGET_DIR:-$root/target}
example=$target/release/examples/geometry
[[ -x $example ]] || cargo build --release -p lipa-core --example geometry || fail "cannot build the geometry example"
example=$(realpath -- "$example")   # KWin compares the real path of the process
[[ $example != *[[:space:]\"\'\\\$\`\%]* ]] || fail "the example path contains characters that need quoting: $example"

rm -rf -- "$work"; mkdir -p -m 700 "$work/run" "$work/data/applications" "$work/config" "$work/cache"
cat > "$work/data/applications/lipax-nested-test.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=LipaX nested test
Exec=$example
NoDisplay=true
Terminal=false
X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2
EOF

inner=$work/inner.sh
cat > "$inner" <<'INNER'
#!/usr/bin/env bash
set -u
work=$1; example=$2; scale=$3; outputs=$4; target=$5; xwayland=$6
export XDG_RUNTIME_DIR=$work/run XDG_DATA_HOME=$work/data XDG_CONFIG_HOME=$work/config XDG_CACHE_HOME=$work/cache
export XDG_DATA_DIRS="$work/data:/usr/local/share:/usr/share" XDG_SESSION_TYPE=wayland
unset WAYLAND_DISPLAY DISPLAY KDE_FULL_SESSION
kbuildsycoca6 >"$work/sycoca.log" 2>&1
xw=; before=$(ls /tmp/.X11-unix 2>/dev/null); export before; [ -n "$xwayland" ] && xw="--xwayland"
# `--scale` of a virtual KWin only makes the mode `scale` times larger at output scale 1 (measured with kscreen-doctor, 2026-11-22):
# the real output scale is set below through KWin's output management.
kwin_wayland --virtual --socket lipax-nest --width 1920 --height 1080 --output-count "$outputs" \
    $xw --no-lockscreen --no-global-shortcuts --no-kactivities >"$work/kwin.log" 2>&1 &
kwin=$!
trap 'kill $(jobs -p) 2>/dev/null' EXIT
for _ in $(seq 1 60); do [ -S "$XDG_RUNTIME_DIR/lipax-nest" ] && break; sleep 0.25; done
[ -S "$XDG_RUNTIME_DIR/lipax-nest" ] || { echo "nested-kwin: KWin did not create its socket" >&2; exit 2; }
sleep 2
export WAYLAND_DISPLAY=lipax-nest QT_QPA_PLATFORM=wayland
if [ "$scale" != 1 ] && [ "$scale" != 1.0 ]; then
    for name in $(kscreen-doctor -j 2>/dev/null | python3 -c "import sys,json; print(' '.join(o['name'] for o in json.load(sys.stdin)['outputs'] if o['enabled']))"); do
        kscreen-doctor "output.$name.scale.$scale" >>"$work/scale.log" 2>&1
    done
    sleep 1
fi
match=sleep
if [ "$target" = wine ]; then
    display=
    for _ in $(seq 1 40); do
        display=$(comm -13 <(printf '%s\n' $before | sort) <(ls /tmp/.X11-unix 2>/dev/null | sort) | head -1); [ -n "$display" ] && break; sleep 0.25
    done
    export DISPLAY=":${display#X}" WINEPREFIX="${work%/tmp/*}/tmp/wine-prefix" WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml="
    unset WAYLAND_DISPLAY QT_QPA_PLATFORM
    wine notepad >"$work/wine.log" 2>&1 &
    match=Notepad
elif [ "$target" = qml ]; then
    cat > "$work/target.qml" <<'EOF'
import QtQuick
import QtQuick.Window
Window {
    visible: true; width: 400; height: 300; title: "lipax-nested-target"
    Component.onCompleted: console.warn("target-screen dpr=" + Screen.devicePixelRatio + " logical=" + Screen.width + "x" + Screen.height)
    Rectangle { anchors.fill: parent; color: "#2060c0"
        Rectangle { x: 20; y: 20; width: 160; height: 100; color: "#f0c040" } }
}
EOF
    match=lipax-nested-target
    if [ -n "$xwayland" ]; then
        # Xwayland starts with KWin; its display name is announced by KWin on the bus's environment.
        # KWin starts a rootless Xwayland on a free display: it is the socket that appeared after KWin started.
        display=
        for _ in $(seq 1 40); do
            display=$(comm -13 <(printf '%s\n' $before | sort) <(ls /tmp/.X11-unix 2>/dev/null | sort) | head -1)
            [ -n "$display" ] && break; sleep 0.25
        done
        echo "nested-kwin: Xwayland display :${display#X}"
        export QT_QPA_PLATFORM=xcb DISPLAY=":${display#X}"
        unset WAYLAND_DISPLAY
    fi
    /usr/lib/qt6/bin/qml "$work/target.qml" >"$work/target.log" 2>&1 &
else
    konsole --separate -e sleep 120 >"$work/konsole.log" 2>&1 &
fi
uuid=
for _ in $(seq 1 40); do
    # KWin's WindowsRunner answers krunner `Match`; ids look like `0_{uuid}`.
    uuid=$(busctl --user call org.kde.KWin /WindowsRunner org.kde.krunner1 Match s "$match" 2>/dev/null \
        | grep -oE '\{[0-9a-f-]{36}\}' | head -1)
    [ -n "$uuid" ] && break; sleep 0.5
done
[ -n "$uuid" ] || { echo "nested-kwin: no client window appeared in the nested KWin" >&2; exit 2; }
echo "nested-kwin: client window $uuid"
timeout 60 "$example" "$uuid" "$work/capture.png"
echo "nested-kwin: geometry example exit code $?"
kscreen-doctor -j 2>/dev/null | python3 -c "import sys,json; [print('nested-kwin: output', o['name'], 'scale', o['scale'], 'size', o['size']) for o in json.load(sys.stdin)['outputs'] if o['enabled']]" 
INNER
chmod +x "$inner"

# Output goes to a file, not a pipe: services that the private bus activates (portals) can outlive the run and
# would keep a pipe open for ever. They are stopped below by their XDG_RUNTIME_DIR.
timeout 180 dbus-run-session -- "$inner" "$work" "$example" "$scale" "$outputs" "$client" "$xwayland" >"$work/run.log" 2>&1
grep -E '^nested-kwin|^client geometry|^window frames|^capture:' "$work/run.log"
for proc in /proc/[0-9]*; do
    # Only own processes have a readable environ; the redirection error of the others is silenced by the group.
    if { tr '\0' '\n' < "$proc/environ" | grep -qxF "XDG_RUNTIME_DIR=$work/run"; } 2>/dev/null; then kill "${proc#/proc/}" 2>/dev/null; fi
done
if [[ $client == qml ]] && grep -q "capture: [1-9][0-9]*x[1-9][0-9]*" "$work/run.log" && [[ -s $work/capture.png ]]; then
    # The buffer of a window is the size of its client geometry times the output scale (native resolution); Qt draws its own
    # frame in a nested KWin without decorations, so the expected size is taken from what KWin reports, not from the QML size.
    got=$(grep -o 'capture: [0-9x]*' "$work/run.log" | head -1)
    want=$(python3 - "$work/run.log" "$scale" <<'PY'
import re, sys
log, scale = open(sys.argv[1]).read(), float(sys.argv[2])
m = re.search(r"client geometry: Some\(WindowGeometry \{ x: [-\d.]+, y: [-\d.]+, w: ([\d.]+), h: ([\d.]+)", log)
print("capture: %dx%d" % (round(float(m.group(1)) * scale), round(float(m.group(2)) * scale)) if m else "capture: unknown")
PY
)
    if [[ $got == "$want" ]]; then echo "nested-kwin: PASS ($got = client geometry x scale $scale, qml target, $outputs outputs${xwayland:+, XWayland})"; result=0
    else echo "nested-kwin: FAIL - the qml target frame is '$got', expected '$want' (see $work/run.log)"; result=1; fi
elif grep -q "capture: [1-9][0-9]*x[1-9][0-9]*" "$work/run.log" && [[ -s $work/capture.png ]]; then
    echo "nested-kwin: PASS ($(grep -o 'capture: [0-9x]*' "$work/run.log" | head -1), scale $scale, $outputs outputs)"; result=0
elif grep -qi "NoAuthorized\|не разрешил" "$work/run.log"; then
    echo "nested-kwin: FAIL - KWin refused the capture (authorization) - see $work/run.log"; result=1
else
    echo "nested-kwin: FAIL - no frame captured - see $work/run.log"; result=1
fi
[[ -n ${LIPAX_NESTED_KEEP:-} ]] || rm -rf -- "$work/run" "$work/cache" "$work/config" "$work/data"
exit $result
