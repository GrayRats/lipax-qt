#!/usr/bin/env bash
# Regression: a normal QWindow::close() of the main window exits with either overlay role alive.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
lipa_binary=${1:-target/debug/lipax}
lipa_test_root="${CARGO_TARGET_DIR:-$PWD/target}/lifecycle-test"
for lipa_mode in floating pinned; do
    lipa_config="$lipa_test_root/$lipa_mode/config"
    mkdir -p "$lipa_config/lipa"
    if [[ $lipa_mode == pinned ]]; then
        printf 'overlay_pinned = true\n' > "$lipa_config/lipa/config.toml"
    else
        printf 'overlay_pinned = false\n' > "$lipa_config/lipa/config.toml"
    fi
    lipa_log="$lipa_test_root/$lipa_mode.log"
    if ! timeout 15s env QT_QPA_PLATFORM=offscreen RUST_LOG=debug \
        XDG_CONFIG_HOME="$lipa_config" LIPAX_TEST_WITH_OVERLAY=1 LIPAX_TEST_CLOSE_AFTER_MS=1400 \
        "$lipa_binary" > "$lipa_log" 2>&1; then
        cat "$lipa_log"
        exit 1
    fi
    if [[ $lipa_mode == pinned ]]; then
        grep -Fq 'reason=pinned_surface_created' "$lipa_log"
    else
        grep -Fq 'window_role=xdg_toplevel' "$lipa_log"
    fi
    grep -Fq 'LipaX завершён' "$lipa_log"
done
