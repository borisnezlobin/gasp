#!/bin/bash
# Builds Gasp for Linux and packages it two ways, in target/package/:
#
#   Gasp-<version>-linux-<arch>.tar.gz   any distribution; its install.sh
#                                        installs to ~/.local, where the app
#                                        updates itself
#   gasp_<version>_<debarch>.deb         Debian, Ubuntu and their relatives
#
#   scripts/package-linux.sh
#
# The binary needs the glibc of the machine it's built on or newer, so
# releases are built on the oldest supported Ubuntu (see
# .github/workflows/release-linux.yml). GLIBC_MAX, when set, fails the
# build if the binary asks for a newer glibc than that, such as 2.35.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="$REPO_ROOT/target/package"
ID="com.borisnezlobin.gasp"
ASSETS="$REPO_ROOT/apps/desktop/assets"

step() { printf '\033[1m==> %s\033[0m\n' "$1"; }

VERSION="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' "$REPO_ROOT/Cargo.toml")"
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64) DEB_ARCH=amd64 ;;
  aarch64) DEB_ARCH=arm64 ;;
  *) echo "unsupported architecture $ARCH" >&2; exit 1 ;;
esac

step "Building gasp $VERSION for $ARCH"
cargo build --profile dist --locked -p gasp-desktop --manifest-path "$REPO_ROOT/Cargo.toml"
BINARY="$REPO_ROOT/target/dist/gasp"

NEEDED_GLIBC="$(objdump -T "$BINARY" | grep -oE 'GLIBC_[0-9]+\.[0-9]+' | sed 's/GLIBC_//' | sort -uV | tail -1)"
echo "needs glibc $NEEDED_GLIBC or newer"
if [ -n "${GLIBC_MAX:-}" ] && [ "$(printf '%s\n%s\n' "$GLIBC_MAX" "$NEEDED_GLIBC" | sort -V | tail -1)" != "$GLIBC_MAX" ]; then
  echo "the binary needs glibc $NEEDED_GLIBC, newer than GLIBC_MAX=$GLIBC_MAX" >&2
  exit 1
fi

# Lays out the launcher entry, AppStream data and icons under $1/share.
install_share() {
  local share="$1/share"
  install -Dm644 "$ASSETS/linux/$ID.desktop" "$share/applications/$ID.desktop"
  install -Dm644 "$ASSETS/linux/$ID.metainfo.xml" "$share/metainfo/$ID.metainfo.xml"
  for png in "$ASSETS"/icon/linux/*.png; do
    local size
    size="$(basename "$png" .png)"
    install -Dm644 "$png" "$share/icons/hicolor/${size}x${size}/apps/$ID.png"
  done
}

rm -rf "$OUT_DIR/linux"
mkdir -p "$OUT_DIR/linux"

step "Packaging the tarball"
TAR_NAME="Gasp-$VERSION-linux-$ARCH"
TAR_DIR="$OUT_DIR/linux/$TAR_NAME"
install -Dm755 "$BINARY" "$TAR_DIR/gasp"
install -Dm755 "$ASSETS/linux/install.sh" "$TAR_DIR/install.sh"
install -Dm644 "$ASSETS/icon/THIRD_PARTY_NOTICES.txt" "$TAR_DIR/THIRD_PARTY_NOTICES.txt"
install_share "$TAR_DIR"
tar -C "$OUT_DIR/linux" --owner=0 --group=0 -czf "$OUT_DIR/$TAR_NAME.tar.gz" "$TAR_NAME"

step "Packaging the .deb"
DEB_DIR="$OUT_DIR/linux/deb"
install -Dm755 "$BINARY" "$DEB_DIR/usr/bin/gasp"
install_share "$DEB_DIR/usr"
install -Dm644 "$ASSETS/icon/THIRD_PARTY_NOTICES.txt" "$DEB_DIR/usr/share/doc/gasp/copyright"
mkdir -p "$DEB_DIR/DEBIAN"
INSTALLED_KB="$(du -sk "$DEB_DIR/usr" | cut -f1)"
# GPUI draws with Vulkan and opens the Vulkan loader, Wayland and
# fontconfig at runtime, so they're named here though the binary doesn't
# link them.
cat > "$DEB_DIR/DEBIAN/control" <<EOF
Package: gasp
Version: $VERSION
Architecture: $DEB_ARCH
Maintainer: Boris Nezlobin <me@borisnezlobin.com>
Installed-Size: $INSTALLED_KB
Depends: libc6 (>= $NEEDED_GLIBC), libgcc-s1, zlib1g, libxcb1, libxkbcommon0, libxkbcommon-x11-0, libdbus-1-3, libvulkan1, libwayland-client0, libfontconfig1, libfreetype6
Recommends: mesa-vulkan-drivers | vulkan-icd
Section: editors
Priority: optional
Homepage: https://gaspmd.com
Description: Fast, native Markdown editor for your notes
 Gasp is a fast, native Markdown editor for an Obsidian vault, with live
 preview, math, search across every note, and sync through GitHub.
EOF
DEB_NAME="gasp_${VERSION}_${DEB_ARCH}.deb"
dpkg-deb --root-owner-group --build "$DEB_DIR" "$OUT_DIR/$DEB_NAME" >/dev/null

step "Done"
cd "$OUT_DIR"
sha256sum "$TAR_NAME.tar.gz" "$DEB_NAME" | tee "$OUT_DIR/linux/SHA256SUMS"
