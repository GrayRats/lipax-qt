# Maintainer: Vladislav Zubkov <satix@...>
pkgname=lipa
pkgver=0.2.0
pkgrel=1
pkgdesc="Wayland screen translation tool using Quickshell"
arch=('x86_64')
url="https://github.com/satix-one/lipa"
license=('MIT')
depends=('quickshell' 'slurp' 'grim' 'tesseract' 'tesseract-data-rus' 'tesseract-data-eng')
makedepends=('cargo' 'rust')
source=("$pkgname-$pkgver.tar.gz::$url/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP')

build() {
    cd "$pkgname-$pkgver"
    cargo build --release --locked
}

package() {
    cd "$pkgname-$pkgver"
    # Ставим бинарник
    install -Dm755 "target/release/lipa" "$pkgdir/usr/bin/lipa"
    # Ставим переименованный QML-файл в системную папку
    install -Dm644 "lipa.qml" "$pkgdir/usr/share/lipa/lipa.qml"
}