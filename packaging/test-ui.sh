#!/usr/bin/env bash
# Read-only UI matrix. Isolated KDE config; no changes to the user's desktop or font settings.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
lipax_results="$PWD/target/ui-checks"
mkdir -p "$lipax_results"
bash packaging/check-qt.sh
for lipax_case in 'universal-dark|Universal|Dark|1|BreezeDark|10' 'universal-light|Universal|Light|1.25|BreezeLight|10' 'fusion-large|Fusion|Light|1.5|BreezeLight|14' 'breeze-light|org.kde.breeze|Light|1|BreezeLight|11' 'breeze-dark|org.kde.breeze|Dark|2|BreezeDark|11' 'breeze-custom|org.kde.breeze|Dark|1.25|CachyOSNord|12'; do
    IFS='|' read -r lipax_name lipax_style lipax_theme lipax_scale lipax_scheme lipax_font <<< "$lipax_case"
    lipax_config="$lipax_results/$lipax_name/config"
    mkdir -p "$lipax_config"
    lipax_colors="/usr/share/color-schemes/$lipax_scheme.colors"
    if [[ ! -f $lipax_colors ]]; then
        echo "SKIP $lipax_name: $lipax_colors is not installed"
        continue
    fi
    cp "$lipax_colors" "$lipax_config/kdeglobals"
    # Replace existing General section name so font and ColorScheme have one authoritative group.
    sed -i 's/^\[General\]$/[SchemeMetadata]/' "$lipax_config/kdeglobals"
    printf '\n[General]\nColorScheme=%s\nfont=Noto Sans,%s,-1,5,50,0,0,0,0,0\n' "$lipax_scheme" "$lipax_font" >> "$lipax_config/kdeglobals"
    echo "Testing $lipax_name: $lipax_style, scale=$lipax_scale, font=$lipax_font"
    env XDG_CONFIG_HOME="$lipax_config" QT_QPA_PLATFORM="${LIPAX_TEST_PLATFORM:-offscreen}" QT_QPA_PLATFORMTHEME=kde \
        QT_QUICK_CONTROLS_STYLE="$lipax_style" QT_QUICK_CONTROLS_UNIVERSAL_THEME="$lipax_theme" QT_SCALE_FACTOR="$lipax_scale" \
        /usr/lib/qt6/bin/qmltestrunner -input crates/app/tests > "$lipax_results/$lipax_name.log" 2>&1 || {
        tail -30 "$lipax_results/$lipax_name.log"; exit 1;
    }
    if rg 'Binding loop|TypeError|ReferenceError|Cannot assign|is not unique' "$lipax_results/$lipax_name.log"; then exit 1; fi
    rg '^Totals:' "$lipax_results/$lipax_name.log"
done
