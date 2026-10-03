pkgname=LipaXQT
pkgver=0.3.0
pkgrel=6
pkgdesc="LipaX — game text OCR and live translation for KDE Plasma / Wayland"
arch=('x86_64')
url="https://github.com/GrayRats/lipax-qt"
license=('MIT')
depends=('qt6-base' 'qt6-declarative' 'qt6-svg' 'layer-shell-qt' 'kwindowsystem' 'tesseract' 'tesseract-data-eng')
makedepends=('cargo' 'rust')
provides=('lipa')
conflicts=('lipa')
optdepends=(
    'python: PaddleOCR in a separate venv (see /usr/share/doc/LipaXQT/PaddleOCR.md)'
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
    cargo clean -p lipa -p lipa-core
}

build() {
    _project_dir
    cargo build --release --frozen -p lipa
}

check() {
    _project_dir
    cargo test --frozen --workspace
    QT_QPA_PLATFORM=offscreen QT_QUICK_CONTROLS_STYLE=Universal QT_QUICK_CONTROLS_UNIVERSAL_THEME=Dark \
        /usr/lib/qt6/bin/qmltestrunner -input crates/app/tests
}

package() {
    _project_dir
    # Keep the existing executable and desktop ID for settings and KWin integration.
    install -Dm755 "$CARGO_TARGET_DIR/release/lipa" "$pkgdir/usr/bin/lipa"
    ln -s lipa "$pkgdir/usr/bin/lipax"
    install -Dm644 packaging/io.lipa.Translator.desktop "$pkgdir/usr/share/applications/io.lipa.Translator.desktop"
    sed -i 's|^Exec=lipa$|Exec=/usr/bin/lipa|' "$pkgdir/usr/share/applications/io.lipa.Translator.desktop"
    install -Dm644 crates/app/assets/lipa.svg "$pkgdir/usr/share/icons/hicolor/scalable/apps/io.lipa.Translator.svg"
    for doc in PaddleOCR ARCHITECTURE IMPROVEMENTS LOGGING; do
        install -Dm644 "docs/$doc.md" "$pkgdir/usr/share/doc/$pkgname/$doc.md"
    done
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
