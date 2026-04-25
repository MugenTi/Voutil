# Maintainer: MugenTi <mugen.tiguan@gmail.com>
pkgname=voutil
pkgver=0.9.2
pkgrel=1
depends=('aom' 'libwebp' 'expat' 'freetype2' 'fontconfig' 'libx11' 'libxkbcommon' 'libxcb')
makedepends=('rust' 'cargo' 'nasm' 'cmake')
arch=('i686' 'x86_64' 'armv6h' 'armv7h')
pkgdesc="A no-nonsense hardware-accelerated image viewer"
url="https://github.com/MugenTi/voutil"
source=("$pkgname-$pkgver.tar.gz::https://github.com/MugenTi/${pkgname}/archive/refs/tags/${pkgver}.tar.gz")
sha512sums=('SKIP')
license=('MIT')
options=('!lto')

build() {
    export RUSTUP_TOOLCHAIN=stable
    cd "$srcdir/$pkgname-$pkgver"
    cargo build --locked --release
}

package() {
    cd "$srcdir/$pkgname-$pkgver"
    install -Dm755 target/release/voutil "${pkgdir}/usr/bin/${pkgname}"
    install -Dm644 res/icons/icon.png "${pkgdir}/usr/share/icons/hicolor/128x128/apps/${pkgname}.png"
    install -Dm644 res/voutil.desktop "${pkgdir}/usr/share/applications/${pkgname}.desktop"
    install -Dm644 LICENSE -t "${pkgdir}/usr/share/licenses/${pkgname}"
    install -Dm644 README.md -t "${pkgdir}/usr/share/doc/${pkgname}"
}
