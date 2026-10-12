#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.."
export TMPDIR="$PWD/tmp" TMP="$PWD/tmp" TEMP="$PWD/tmp"
export XDG_CACHE_HOME="$PWD/tmp/xdg-cache" PYTHONPYCACHEPREFIX="$PWD/tmp/python-bytecode"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/tmp/cargo-target}"
# CI fetches dependencies in a separate step; local runs remain offline.
export CARGO_NET_OFFLINE="${CARGO_NET_OFFLINE:-true}"
export QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software OMP_THREAD_LIMIT=1
mkdir -p "$TMPDIR/inplace-tests"
echo "NOT RUN: legacy real Tesseract recognition test (this runner validates actual OCR with RapidOCR only)" >&2
cargo test --locked --workspace -- --skip real_tesseract_reports_lines_and_confidence > "$TMPDIR/inplace-tests/rust.log" 2>&1
cargo build --locked -p lipa-core --example inplace_bench > "$TMPDIR/inplace-tests/build.log" 2>&1
c++ -std=c++17 -O2 -fPIC -Icrates/app/src tests/inplace/qt/text_measure.cpp $(pkg-config --cflags --libs Qt6Gui) -o "$TMPDIR/inplace-tests/text-measure"
"$TMPDIR/inplace-tests/text-measure" > "$TMPDIR/inplace-tests/qt-measure.json"
c++ -std=c++17 -O2 -fPIC -Icrates/app/src tests/inplace/qt/measure_server.cpp $(pkg-config --cflags --libs Qt6Gui) -o "$TMPDIR/inplace-tests/measure-server"
export LIPAX_QT_MEASURE_SERVER="$TMPDIR/inplace-tests/measure-server"
"${QMLTESTRUNNER:-/usr/lib/qt6/bin/qmltestrunner}" -input tests/inplace/qml > "$TMPDIR/inplace-tests/qml.log" 2>&1
"${QMLTESTRUNNER:-/usr/lib/qt6/bin/qmltestrunner}" -input crates/app/tests > "$TMPDIR/inplace-tests/existing-qml.log" 2>&1
# Default remains real RapidOCR. CI without model provisioning explicitly selects 0.
export LIPAX_TEST_OCR="${LIPAX_TEST_OCR:-1}"
if [[ "$LIPAX_TEST_OCR" != 1 ]]; then
    echo "NOT RUN: RapidOCR model-backed screenshot checks (LIPAX_TEST_OCR=$LIPAX_TEST_OCR)" >&2
fi
cargo test --locked -p lipa-core --test inplace_placement_quality -- --ignored > "$TMPDIR/inplace-tests/qt-rust.log" 2>&1
if [[ "$LIPAX_TEST_OCR" == 1 ]]; then
    export LIPAX_RAPID_TEST_MODELS="${LIPAX_RAPID_TEST_MODELS:-$HOME/.local/share/LipaX/ocr-models}"
    export LIPAX_RAPID_TEST_THREADS="${LIPAX_RAPID_TEST_THREADS:-2}"
    # This filtered recognition test uses RapidOCR only. The legacy comparative
    # benchmark also invokes other OCR engines and is intentionally not selected.
    cargo test --locked -p lipa-core --test rapid_native reads_english_russian_and_japanese -- --ignored --nocapture --test-threads=1 > "$TMPDIR/inplace-tests/rapid-native.log" 2>&1
fi
if [[ -n "${LIPAX_BERGAMOT_TEST_BINARY:-}" && -n "${LIPAX_BERGAMOT_TEST_MODELS:-}" ]]; then
    cargo test --locked -p lipa-core --test bergamot_native -- --ignored > "$TMPDIR/inplace-tests/bergamot-native.log" 2>&1
else
    echo "NOT RUN: native Bergamot opt-in test (set LIPAX_BERGAMOT_TEST_BINARY and LIPAX_BERGAMOT_TEST_MODELS)" >&2
fi
python -m pytest -ra tests/inplace/python --basetemp="$TMPDIR/inplace-tests/pytest" -o cache_dir="$TMPDIR/pytest-cache" > "$TMPDIR/inplace-tests/python.log" 2>&1
