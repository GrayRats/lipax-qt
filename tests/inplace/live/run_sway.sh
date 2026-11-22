#!/usr/bin/env bash
# Live check of the sway backend (ROADMAP R-14 step 3) in a HEADLESS sway: no screen, no GPU (`WLR_BACKENDS=headless`,
# pixman renderer), a private runtime directory under tmp/. A Qt Wayland window (a small QML window of a known size) is the
# target; `sway_probe` lists it through the sway IPC, reads its state and grabs it through grim.
#
#   tests/inplace/live/run_sway.sh                 uses `sway` from PATH, else the packages unpacked in $LIPAX_SWAY_ROOT
#   LIPAX_SWAY_SCALE=1.5 ...                       output scale (default 1)
#   LIPAX_SWAY_KEEP=1                              keep the work directory (logs, capture.png)
#
# `sway` and `wlroots` need not be installed: download `pacman -Sp sway` into a directory, unpack the packages with bsdtar and
# point LIPAX_SWAY_ROOT at it (the script then sets LD_LIBRARY_PATH). Exit code: 0 = captured the window at the right size,
# 1 = the check failed, 2 = environment (missing sway, grim, qml, or the compositor did not start).
set -uo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.."
root=$PWD
work=$root/tmp/sway-live
scale=${LIPAX_SWAY_SCALE:-1}
fail() { echo "sway-live: $*" >&2; exit "${2:-2}"; }

sway=$(command -v sway || true)
if [[ -z $sway && -n ${LIPAX_SWAY_ROOT:-} ]]; then
    sway=$LIPAX_SWAY_ROOT/usr/bin/sway; export LD_LIBRARY_PATH=$LIPAX_SWAY_ROOT/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
fi
[[ -x $sway ]] || fail "sway not found (set LIPAX_SWAY_ROOT to unpacked sway and wlroots packages)"
for tool in grim; do command -v "$tool" >/dev/null 2>&1 || fail "$tool not found"; done
qml=/usr/lib/qt6/bin/qml; [[ -x $qml ]] || qml=$(command -v qml6 || command -v qml || true); [[ -x $qml ]] || fail "the qml runtime was not found"
if command -v rustup >/dev/null 2>&1 && [[ -z ${RUSTUP_TOOLCHAIN:-} ]]; then export RUSTUP_TOOLCHAIN=stable; fi
example=${CARGO_TARGET_DIR:-$root/target}/release/examples/sway_probe
[[ -x $example ]] || cargo build --release -p lipa-core --example sway_probe || fail "cannot build the sway_probe example"

rm -rf -- "$work"; mkdir -p -m 700 "$work"
cat > "$work/config" <<EOF
output HEADLESS-1 resolution 1920x1080 scale $scale
default_border none
for_window [title="lipax-sway-target"] floating enable
EOF
# A 400x300 window of two colours; Qt uses the fractional scale of the output if the compositor offers one.
cat > "$work/target.qml" <<'EOF'
import QtQuick
import QtQuick.Window
Window {
    visible: true; width: 400; height: 300; title: "lipax-sway-target"
    Rectangle { anchors.fill: parent; color: "#2060c0"
        Rectangle { x: 20; y: 20; width: 160; height: 100; color: "#f0c040" } }
}
EOF
export XDG_RUNTIME_DIR=$work WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman
unset WAYLAND_DISPLAY DISPLAY SWAYSOCK
"$sway" -c "$work/config" --unsupported-gpu >"$work/sway.log" 2>&1 &
pid=$!
trap 'kill $(jobs -p) 2>/dev/null' EXIT
for _ in $(seq 1 60); do ls "$work"/sway-ipc.*.sock >/dev/null 2>&1 && [ -S "$work/wayland-1" ] && break; sleep 0.25; done
export SWAYSOCK=$(ls "$work"/sway-ipc.*.sock 2>/dev/null | head -1)
[[ -n $SWAYSOCK ]] || fail "sway did not start (see $work/sway.log)"
export WAYLAND_DISPLAY=wayland-1 QT_QPA_PLATFORM=wayland
"$qml" "$work/target.qml" >"$work/qml.log" 2>&1 &
for _ in $(seq 1 40); do "$example" lipax-sway-target "$work/capture.png" >"$work/probe.log" 2>&1; [ $? -ne 2 ] && break; sleep 0.5; done
cat "$work/probe.log"
size=$(grep -o 'capture: [0-9]*x[0-9]*' "$work/probe.log" | head -1)
want="capture: $(python3 -c "print(f'{round(400*$scale)}x{round(300*$scale)}')")"
result=1
if [[ $size == "$want" ]] && grep -q "unknown window: WindowGone" "$work/probe.log" && grep -q "^region:" "$work/probe.log"; then
    echo "sway-live: PASS ($size, scale $scale)"; result=0
else
    echo "sway-live: FAIL (got '${size:-nothing}', expected '$want') - see $work"
fi
[[ -n ${LIPAX_SWAY_KEEP:-} ]] || rm -rf -- "$work"/sway-ipc.* "$work"/wayland-1*
exit $result
