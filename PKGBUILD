# Maintainer: YOUR_NAME <YOUR_EMAIL>
pkgname=proton-drive
pkgver=0.1.0
pkgrel=1
pkgdesc="Proton Drive desktop client for KDE Plasma"
arch=('x86_64')
url="https://github.com/YOUR_USERNAME/proton-drive"
license=('GPL-3.0-or-later')
depends=('dbus')
makedepends=('rust' 'cargo')
source=("$pkgname-$pkgver.tar.gz::$url/archive/v$pkgver.tar.gz")
sha256sums=('SKIP')

prepare() {
    cd "$pkgname-$pkgver"
    cargo fetch --locked --target "$CARCH-unknown-linux-gnu"
}

build() {
    cd "$pkgname-$pkgver"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --frozen --release --bin pdrive --bin pdrive-daemon
}

check() {
    cd "$pkgname-$pkgver"
    cargo test --frozen --lib -p pdrive-core
}

package() {
    cd "$pkgname-$pkgver"
    install -Dm755 "target/release/pdrive"        "$pkgdir/usr/bin/pdrive"
    install -Dm755 "target/release/pdrive-daemon"  "$pkgdir/usr/bin/pdrive-daemon"
    install -Dm644 "assets/pdrive.desktop"         "$pkgdir/usr/share/applications/pdrive.desktop"
    install -Dm644 "assets/icons/pdrive.svg"       "$pkgdir/usr/share/icons/hicolor/scalable/apps/pdrive.svg"
    install -Dm644 "systemd/pdrive.service"        "$pkgdir/usr/lib/systemd/user/pdrive.service"
}
