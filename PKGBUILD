# Maintainer: eldergod1800
pkgname=proton-drive
pkgver=0.1.6
pkgrel=1
pkgdesc="Proton Drive desktop client for KDE Plasma"
arch=('x86_64')
url="https://github.com/eldergod1800/proton-drive"
license=('GPL-3.0-or-later')
depends=('dbus' 'python-gobject' 'webkit2gtk-4.1')
makedepends=('rust' 'cargo' 'cmake' 'clang')
source=("$pkgname-$pkgver.tar.gz::$url/archive/v$pkgver.tar.gz")
sha256sums=('5096c85704da6d7076556248f611881af0796672374f558425695c6628ab320c')

prepare() {
    cd "$pkgname-$pkgver"
    cargo fetch --locked --target "$CARCH-unknown-linux-gnu"
}

build() {
    cd "$pkgname-$pkgver"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    # Arch system LDFLAGS conflict with aws-lc-sys static linking
    unset LDFLAGS
    export CC=clang
    export CXX=clang++
    cargo build --release --locked --bin pdrive --bin pdrive-daemon
}

check() {
    cd "$pkgname-$pkgver"
    cargo test --lib -p pdrive-core
}

package() {
    cd "$pkgname-$pkgver"
    install -Dm755 "target/release/pdrive"        "$pkgdir/usr/bin/pdrive"
    install -Dm755 "target/release/pdrive-daemon"  "$pkgdir/usr/bin/pdrive-daemon"
    install -Dm644 "assets/pdrive.desktop"         "$pkgdir/usr/share/applications/pdrive.desktop"
    install -Dm644 "assets/icons/pdrive.svg"       "$pkgdir/usr/share/icons/hicolor/scalable/apps/pdrive.svg"
    install -Dm644 "systemd/pdrive.service"        "$pkgdir/usr/lib/systemd/user/pdrive.service"
}
