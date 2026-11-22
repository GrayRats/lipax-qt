#!/usr/bin/env bash
# Live check of `keepFloating` of the KWin script (PLAN.md P-06, ROADMAP R-13): in a NESTED KWin a window that is made
# fullscreen, maximised or tiled must be put back at once. The function is cut out of crates/core/src/capture/geometry.js (between
# its BEGIN/END markers) and applied to a Konsole window, so the code that ships is the code that is tested.
#
#   LIPAX_FP_CONTROL=1 ...                         control run WITHOUT keepFloating: it must FAIL (the test can see the difference)
#   tests/inplace/live/run_floating_policy.sh      Exit code: 0 = every state was undone, 1 = one was not, 2 = environment.
set -uo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.."
root=$PWD
work=$root/tmp/floating-policy
fail() { echo "floating-policy: $*" >&2; exit "${2:-2}"; }
for tool in kwin_wayland dbus-run-session busctl konsole; do command -v "$tool" >/dev/null 2>&1 || fail "$tool not found"; done
rm -rf -- "$work"; mkdir -p -m 700 "$work/run" "$work/data" "$work/config" "$work/cache"
{
    echo "const LIPAX_CONTROL = $([ -n "${LIPAX_FP_CONTROL:-}" ] && echo true || echo false);"
    sed -n '/^\/\/ BEGIN keepFloating/,/^\/\/ END keepFloating/p' crates/core/src/capture/geometry.js
    cat <<'JS'
function report(step, window) { print("FP " + step + " fullScreen=" + window.fullScreen + " tile=" + (window.tile ? "set" : "null")); }
// A state change is applied when the client has acknowledged it: wait for turns of the event loop (a chain of D-Bus pings).
function later(turns, next) {
    if (turns <= 0) { next(); return; }
    callDBus("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "GetId", function() { later(turns - 1, next); });
}
const WAIT = 300;
const target = workspace.windowList().filter(function(w) { return String(w.caption).indexOf("fp-target") >= 0; })[0];
if (!target) { print("FP no-target"); } else {
    let maximizedEvents = 0;
    target.maximizedChanged.connect(function() { maximizedEvents++; });
    if (!LIPAX_CONTROL) keepFloating(target);
    workspace.activeWindow = target;
    // One action at a time, each given time to be applied and (with keepFloating) undone. KWin applies a state after the client
    // acknowledged it, and keepFloating undoes it a few turns after the signal: the state is read after both.
    workspace.slotWindowQuickTileLeft();
    later(WAIT, function() {
        report("after-tile", target);
        later(WAIT, function() {
            workspace.slotWindowFullScreen();
            later(WAIT, function() {
                later(WAIT, function() {
                    report("after-fullscreen", target);
                    workspace.slotWindowMaximize();
                    later(WAIT, function() {
                        later(WAIT, function() {
                            print("FP after-maximize events=" + maximizedEvents + " geometry=" + Math.round(target.frameGeometry.width) + "x" + Math.round(target.frameGeometry.height));
                            print("FP done");
                        });
                    });
                });
            });
        });
    });
}
JS
} > "$work/policy.js"
cat > "$work/inner.sh" <<'INNER'
#!/usr/bin/env bash
set -u
work=$1
export XDG_RUNTIME_DIR=$work/run XDG_DATA_HOME=$work/data XDG_CONFIG_HOME=$work/config XDG_CACHE_HOME=$work/cache XDG_SESSION_TYPE=wayland
export QT_LOGGING_TO_CONSOLE=1 QT_LOGGING_RULES="js.debug=true"
unset WAYLAND_DISPLAY DISPLAY
kwin_wayland --virtual --socket fp-nest --width 1920 --height 1080 --no-lockscreen --no-global-shortcuts --no-kactivities >"$work/kwin.log" 2>&1 &
trap 'kill $(jobs -p) 2>/dev/null' EXIT
for _ in $(seq 1 60); do [ -S "$XDG_RUNTIME_DIR/fp-nest" ] && break; sleep 0.25; done
[ -S "$XDG_RUNTIME_DIR/fp-nest" ] || { echo "floating-policy: KWin did not create its socket" >&2; exit 2; }
sleep 2
export WAYLAND_DISPLAY=fp-nest QT_QPA_PLATFORM=wayland
konsole --separate -p tabtitle=fp-target -e bash -c 'printf "\033]0;fp-target\007"; sleep 60' >"$work/konsole.log" 2>&1 &
sleep 4
busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting loadScript ss "$work/policy.js" fp >/dev/null
busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting start >/dev/null
sleep 25
INNER
timeout 150 dbus-run-session -- bash "$work/inner.sh" "$work" >"$work/run.log" 2>&1
for proc in /proc/[0-9]*; do
    if { tr '\0' '\n' < "$proc/environ" | grep -qxF "XDG_RUNTIME_DIR=$work/run"; } 2>/dev/null; then kill "${proc#/proc/}" 2>/dev/null; fi
done
grep -a "FP " "$work/kwin.log" | sed 's/^js: //; s/^/floating-policy: /'
if ! grep -aq "FP done" "$work/kwin.log"; then echo "floating-policy: the script did not run to the end (see $work/kwin.log)"; exit 1; fi
bad=$(grep -a "FP after-fullscreen\|FP after-tile" "$work/kwin.log" | grep -c "fullScreen=true\|tile=set")
# A maximise that was undone is two events (on, off); one event means it stayed.
events=$(grep -a "FP after-maximize" "$work/kwin.log" | grep -o 'events=[0-9]*' | head -1 | cut -d= -f2)
[ "${events:-0}" -ge 2 ] || bad=$((bad + 1))
if [ "$bad" = 0 ]; then echo "floating-policy: PASS"; exit 0; else echo "floating-policy: FAIL ($bad states were not undone)"; exit 1; fi
