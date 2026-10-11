#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.."
export TMPDIR="$PWD/tmp" TMP="$PWD/tmp" TEMP="$PWD/tmp"
export XDG_CACHE_HOME="$PWD/tmp/xdg-cache" PYTHONPYCACHEPREFIX="$PWD/tmp/python-bytecode"
export CARGO_TARGET_DIR="$PWD/tmp/cargo-target"
export QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software OMP_THREAD_LIMIT=1
mkdir -p "$TMPDIR/inplace-tests"
cargo test -p lipa-core --offline > "$TMPDIR/inplace-tests/rust.log" 2>&1
cargo build -p lipa-core --example inplace_bench --offline > "$TMPDIR/inplace-tests/build.log" 2>&1
c++ -std=c++17 -O2 -fPIC -Icrates/app/src tests/inplace/qt/text_measure.cpp $(pkg-config --cflags --libs Qt6Gui) -o "$TMPDIR/inplace-tests/text-measure"
"$TMPDIR/inplace-tests/text-measure" > "$TMPDIR/inplace-tests/qt-measure.json"
c++ -std=c++17 -O2 -fPIC -Icrates/app/src tests/inplace/qt/measure_server.cpp $(pkg-config --cflags --libs Qt6Gui) -o "$TMPDIR/inplace-tests/measure-server"
export LIPAX_QT_MEASURE_SERVER="$TMPDIR/inplace-tests/measure-server"
/usr/lib/qt6/bin/qmltestrunner -input tests/inplace/qml > "$TMPDIR/inplace-tests/qml.log" 2>&1
/usr/lib/qt6/bin/qmltestrunner -input crates/app/tests > "$TMPDIR/inplace-tests/existing-qml.log" 2>&1
LIPAX_TEST_OCR=1 python -m pytest tests/inplace/python --basetemp="$TMPDIR/inplace-tests/pytest" -o cache_dir="$TMPDIR/pytest-cache" > "$TMPDIR/inplace-tests/python.log" 2>&1
