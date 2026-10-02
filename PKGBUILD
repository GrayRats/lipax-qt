# Maintainer: Владислав Александрович Зубков <satix@...>
pkgname=lipa
pkgver=0.3.0
pkgrel=1
pkgdesc="Game text translator for KDE Plasma/Wayland: screen text OCR (Tesseract) and live translation (Qt/QML + Rust)"
arch=('x86_64')
url="https://github.com/satix-one/lipa"
license=('MIT')
depends=(
    'qt6-base'
    'qt6-declarative'
    'layer-shell-qt'      # overlay и рамка выбора: wlr-layer-shell
    'tesseract'
    'tesseract-data-eng'
)
makedepends=('cargo' 'rust' 'qt6-base' 'qt6-declarative')
optdepends=(
    'tesseract-data-rus: распознавание русского текста'
    'tesseract-data-jpn: распознавание японского текста'
    'kwin: захват окна через ScreenShot2 (основной бэкенд, KDE Plasma 6)'
    'xdg-desktop-portal: запасной захват окна (portal + PipeWire)'
    'gst-plugin-pipewire: чтение кадров PipeWire для portal-захвата'
    'gst-plugins-good: PNG-кодирование кадров для portal-захвата'
    'polkit: установка языковых пакетов Tesseract из настроек (pkexec)'
)
source=("$pkgname-$pkgver.tar.gz::$url/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP')
# LTO ломает сборку cxx-qt, символы отладки Rust-бинарника не нужны.
options=('!lto' '!debug')

prepare() {
    cd "$pkgname-$pkgver"
    export RUSTUP_TOOLCHAIN=stable
    cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
    cd "$pkgname-$pkgver"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    # cxx-qt-build ищет Qt через qmake.
    export QMAKE=/usr/bin/qmake6
    cargo build --release --frozen -p lipa
}

check() {
    cd "$pkgname-$pkgver"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo test --frozen -p lipa-core
}

package() {
    cd "$pkgname-$pkgver"
    install -Dm755 "target/release/lipa" "$pkgdir/usr/bin/lipa"
    # KWin пускает приложение к ScreenShot2 только если Exec в .desktop совпадает с запущенным бинарником,
    # поэтому путь абсолютный.
    install -Dm644 "packaging/io.lipa.Translator.desktop" "$pkgdir/usr/share/applications/io.lipa.Translator.desktop"
    sed -i 's|^Exec=lipa$|Exec=/usr/bin/lipa|' "$pkgdir/usr/share/applications/io.lipa.Translator.desktop"
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
