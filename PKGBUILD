pkgname=lipax
pkgver=1.3.5
pkgrel=1
pkgdesc="LipaX — game text OCR and live translation for KDE Plasma / Wayland"
arch=('x86_64')
url="https://github.com/GrayRats/lipax-qt"
license=('MIT' 'OFL-1.1' 'Apache-2.0' 'MPL-2.0' 'BSD-3-Clause')  # Apache-2.0: Roboto Slab; the other bundled fonts are OFL-1.1
depends=('qt6-base>=6.12' 'qt6-declarative>=6.12' 'qt6-svg>=6.12' 'layer-shell-qt' 'kwindowsystem' 'tesseract' 'tesseract-data-eng' 'pcre2')
# xz: the bundled fonts are stored compressed and unpacked by crates/app/build.rs
makedepends=('cargo' 'rust' 'xz' 'git' 'cmake' 'ninja')
provides=('LipaXQT' 'lipa')
conflicts=('LipaXQT' 'lipa')
optdepends=(
    'qqc2-breeze-style: Breeze style for Qt Quick Controls'
    'python: PaddleOCR in a separate venv (see /usr/share/doc/lipax/PaddleOCR.md)'
    'onnxruntime: RapidOCR (PP-OCRv5) and MeikiOCR engines, loaded at run time; models are downloaded in Settings (see /usr/share/doc/lipax/RapidOCR.md and MeikiOCR.md)'
    'tesseract-data-rus: Russian OCR'
    'tesseract-data-jpn: Japanese OCR'
    'kwin: window capture with ScreenShot2 and client geometry'
    'xdg-desktop-portal: portal capture'
    'pipewire: portal video stream'
    'gstreamer: gst-launch-1.0 for portal capture'
    'gst-plugin-pipewire: pipewiresrc for portal capture'
    'gst-plugins-base: videoconvert for portal capture'
    'gst-plugins-good: PNG encoding for portal capture'
    'polkit: installing Tesseract languages from settings'
)
# build-local.sh supplies a snapshot, including uncommitted source changes.
# Running makepkg directly in the checkout builds that checkout instead.
if [[ ${LIPA_LOCAL_SOURCE:-0} == 1 ]]; then
    source=("$pkgname-$pkgver.tar.gz")
    sha256sums=('SKIP')
else
    source=()
    sha256sums=()
fi
options=('!lto' '!debug')

_project_dir() {
    if [[ ${LIPA_LOCAL_SOURCE:-0} == 1 ]]; then
        cd "$srcdir/$pkgname-$pkgver"
    else
        cd "$startdir"
    fi
    export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$srcdir/target}"
    export QMAKE=/usr/bin/qmake6
}

prepare() {
    _project_dir
    cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
    # Snapshots preserve source mtimes. With a shared target directory Cargo could
    # otherwise reuse a newer binary built from an older snapshot. Keep dependency
    # caches, but always rebuild our two crates (including their tests).
    bash packaging/build-bergamot.sh prepare "$CARGO_TARGET_DIR"
    cargo clean -p lipa -p lipa-core
}

build() {
    _project_dir
    bash packaging/build-bergamot.sh build "$CARGO_TARGET_DIR"
    cargo build --release --frozen -p lipa
}

check() {
    _project_dir
    cargo test --frozen --workspace
    cargo build --frozen -p lipa --features lifecycle-test
    bash packaging/test-process-exit.sh "$CARGO_TARGET_DIR/debug/lipax"
    bash packaging/test-run-local.sh
    QT_QPA_PLATFORM=offscreen QT_QUICK_CONTROLS_STYLE=Universal QT_QUICK_CONTROLS_UNIVERSAL_THEME=Dark \
        /usr/lib/qt6/bin/qmltestrunner -input crates/app/tests
}

package() {
    _project_dir
    # Preserve the desktop ID and old executable alias for settings and KWin integration.
    install -Dm755 "$CARGO_TARGET_DIR/release/lipax" "$pkgdir/usr/bin/lipax"
    ln -s lipax "$pkgdir/usr/bin/lipa"
    install -Dm755 "$CARGO_TARGET_DIR/bergamot-build/app/bergamot" "$pkgdir/usr/lib/lipax/bergamot"
    # Ship licence notices for the engine and its statically linked dependencies.
    while IFS= read -r -d '' _notice; do
        _relative=${_notice#"$CARGO_TARGET_DIR/bergamot-source/"}
        install -Dm644 "$_notice" "$pkgdir/usr/share/licenses/$pkgname/bergamot/$_relative"
    done < <(find "$CARGO_TARGET_DIR/bergamot-source" -type f \( -iname 'license*' -o -iname 'copying*' -o -iname 'notice*' \) -not -path '*/.git/*' -print0)
    install -Dm644 packaging/io.lipa.Translator.desktop "$pkgdir/usr/share/applications/io.lipa.Translator.desktop"
    install -Dm644 crates/app/assets/lipa.svg "$pkgdir/usr/share/icons/hicolor/scalable/apps/io.lipa.Translator.svg"
    for doc in PaddleOCR RapidOCR MeikiOCR Bergamot ARCHITECTURE IMPROVEMENTS LOGGING; do
        install -Dm644 "docs/$doc.md" "$pkgdir/usr/share/doc/$pkgname/$doc.md"
    done
    # Fonts were unpacked from assets/fonts/*.xz by build.rs; licences are shipped with them.
    for _font in crates/app/assets/fonts/*.ttf crates/app/assets/fonts/*.otf; do
        install -Dm644 "$_font" "$pkgdir/usr/share/lipax/fonts/${_font##*/}"
    done
    for _licence in crates/app/assets/fonts/*-OFL.txt crates/app/assets/fonts/*-LICENSE.txt; do
        install -Dm644 "$_licence" "$pkgdir/usr/share/licenses/$pkgname/fonts/${_licence##*/}"
    done
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
