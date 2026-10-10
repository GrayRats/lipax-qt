#!/usr/bin/env bash
# Fail before cleaning/building; CXX-Qt's actual discovery is also checked by build.rs.
set -euo pipefail
lipax_qmake=${QMAKE:-/usr/bin/qmake6}
lipax_qt_version=$("$lipax_qmake" -query QT_VERSION)
IFS=. read -r lipax_qt_major lipax_qt_minor _ <<< "$lipax_qt_version"
if [[ $lipax_qt_major != 6 || ! $lipax_qt_minor =~ ^[0-9]+$ ]] || (( lipax_qt_minor < 12 )); then
    echo "LipaX requires system Qt >= 6.12 and < 7; found $lipax_qt_version via $lipax_qmake" >&2
    exit 1
fi
printf 'System Qt: %s (%s)\n' "$lipax_qt_version" "$lipax_qmake"
