#!/usr/bin/env bash
# Smoke-test the packaged release binary without installing it or using real user settings.
set -euo pipefail
[[ $# == 1 ]] || { echo "Usage: $0 path/to/lipax.pkg.tar.zst" >&2; exit 2; }
if [[ ${LIPAX_PACKAGE_TEST_SESSION:-0} != 1 ]]; then
    exec dbus-run-session -- env LIPAX_PACKAGE_TEST_SESSION=1 bash "$0" "$1"
fi
lipa_package=$(realpath -- "$1")
lipa_test_dir=$(mktemp -d "${TMPDIR:-/tmp}/lipax-package.XXXXXX")
lipa_pid=
cleanup() {
    local status=$?
    if [[ -n $lipa_pid ]] && kill -0 "$lipa_pid" 2>/dev/null; then
        kill "$lipa_pid"
        wait "$lipa_pid" || true
    fi
    if (( status == 0 )); then
        rm -rf -- "$lipa_test_dir"
    else
        echo "Package test failed; artifacts: $lipa_test_dir" >&2
        [[ ! -f $lipa_test_dir/run.log ]] || cat "$lipa_test_dir/run.log" >&2
    fi
}
trap cleanup EXIT
mkdir -p "$lipa_test_dir/root" "$lipa_test_dir/config" "$lipa_test_dir/data" "$lipa_test_dir/cache"
bsdtar -xf "$lipa_package" -C "$lipa_test_dir/root"
lipa_binary="$lipa_test_dir/root/usr/bin/lipax"
[[ -x $lipa_binary ]]
lipa_engine="$lipa_test_dir/root/usr/lib/lipax/bergamot"
[[ -x $lipa_engine ]]
"$lipa_engine" --help > "$lipa_test_dir/bergamot-help.log"
ldd "$lipa_engine" > "$lipa_test_dir/bergamot-libraries.log"
if grep -q 'not found' "$lipa_test_dir/bergamot-libraries.log"; then cat "$lipa_test_dir/bergamot-libraries.log" >&2; exit 1; fi
[[ $(readlink "$lipa_test_dir/root/usr/bin/lipa") == lipax ]]
[[ -s $lipa_test_dir/root/usr/share/doc/lipax/Bergamot.md ]]
[[ -s $lipa_test_dir/root/usr/share/applications/io.lipa.Translator.desktop ]]
ldd "$lipa_binary" > "$lipa_test_dir/libraries.log"
if grep -q 'not found' "$lipa_test_dir/libraries.log"; then cat "$lipa_test_dir/libraries.log" >&2; exit 1; fi
# RapidOCR opens ONNX Runtime at run time (ort, load-dynamic): lipax must not link it, the package carries no model,
# only the catalog compiled into the binary.
if grep -q onnxruntime "$lipa_test_dir/libraries.log"; then echo "lipax links ONNX Runtime directly" >&2; exit 1; fi
if [[ -n $(find "$lipa_test_dir/root" -name '*.onnx' -print -quit) ]]; then echo "OCR models must not be packaged" >&2; exit 1; fi
grep -aq 'RapidAI/RapidOCR/resolve/' "$lipa_binary"
[[ -s $lipa_test_dir/root/usr/share/doc/lipax/RapidOCR.md ]]
# The optional runtime (onnxruntime-cpu), when installed, must resolve all its libraries.
lipa_ort=
for lipa_candidate in /usr/lib/libonnxruntime.so.1 /usr/lib/libonnxruntime.so; do
    if [[ -e $lipa_candidate ]]; then lipa_ort=$lipa_candidate; break; fi
done
if [[ -n $lipa_ort ]]; then
    ldd "$lipa_ort" > "$lipa_test_dir/onnxruntime-libraries.log"
    if grep -q 'not found' "$lipa_test_dir/onnxruntime-libraries.log"; then cat "$lipa_test_dir/onnxruntime-libraries.log" >&2; exit 1; fi
else
    echo "onnxruntime is not installed: the RapidOCR library check is skipped" >&2
fi
"$lipa_binary" --version
export QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software RUST_LOG=info
export XDG_CONFIG_HOME="$lipa_test_dir/config" XDG_DATA_HOME="$lipa_test_dir/data" XDG_CACHE_HOME="$lipa_test_dir/cache"
export LIPAX_FONT_DIR="$lipa_test_dir/root/usr/share/lipax/fonts" LIPAX_INSTANCE_ID="pkgtest$$"
"$lipa_binary" > "$lipa_test_dir/run.log" 2>&1 &
lipa_pid=$!
for _ in $(seq 100); do
    kill -0 "$lipa_pid" 2>/dev/null || { wait "$lipa_pid"; exit 1; }
    grep -q 'bundled families=' "$lipa_test_dir/run.log" && break
    sleep 0.1
done
for lipa_action in --settings --show --stop-autotranslate --quit; do
    timeout 10s "$lipa_binary" "$lipa_action"
done
for _ in $(seq 100); do
    kill -0 "$lipa_pid" 2>/dev/null || break
    sleep 0.1
done
if kill -0 "$lipa_pid" 2>/dev/null; then echo "Packaged application did not quit" >&2; exit 1; fi
wait "$lipa_pid"
lipa_pid=
grep -q 'LipaX завершён' "$lipa_test_dir/run.log"
grep -q 'bundled families= .*Inter' "$lipa_test_dir/run.log"
if grep -E 'ReferenceError|TypeError|QQmlApplicationEngine failed|module .* is not installed|Cannot load library|QThread: Destroyed|error while loading shared' "$lipa_test_dir/run.log"; then exit 1; fi
echo "Package smoke test passed: libraries, bundled fonts, QML startup, settings, commands and clean exit."
