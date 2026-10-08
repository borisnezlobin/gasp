#!/bin/zsh
# Builds apps/desktop/assets/AppIcon.icns, the Mac's Dock icons and the
# iPhone app's icons from the committed whale render.
#
#   apps/desktop/assets/icon/build_icon.sh
#
# Needs a Python with Pillow and numpy (set PYTHON to pick one) and macOS's iconutil.
# To re-render the whale itself, see render_whale.py.

set -euo pipefail

ICON_DIR="${0:A:h}"
ASSETS_DIR="${ICON_DIR:h}"
PYTHON="${PYTHON:-python3}"
WHALE="$ICON_DIR/whale-breach.png"

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
ICONSET="$WORK/AppIcon.iconset"
mkdir -p "$ICONSET"

"$PYTHON" "$ICON_DIR/compose_icon.py" "$WHALE" "$WORK/full.png" 38 0.98 0.47 0.46
"$PYTHON" "$ICON_DIR/compose_icon.py" "$WHALE" "$WORK/small.png" 38 1.06 0.46 0.45 --small

resize() {
  sips -z "$2" "$2" "$1" --out "$3" >/dev/null
}

for size in 16 32; do
  resize "$WORK/small.png" $size "$ICONSET/icon_${size}x${size}.png"
done
resize "$WORK/small.png" 32 "$ICONSET/icon_16x16@2x.png"
resize "$WORK/full.png" 64 "$ICONSET/icon_32x32@2x.png"
for size in 128 256 512; do
  resize "$WORK/full.png" $size "$ICONSET/icon_${size}x${size}.png"
  resize "$WORK/full.png" $((size * 2)) "$ICONSET/icon_${size}x${size}@2x.png"
done

iconutil --convert icns --output "$ASSETS_DIR/AppIcon.icns" "$ICONSET"
cp "$WORK/full.png" "$ICON_DIR/AppIcon-1024.png"
print "Wrote $ASSETS_DIR/AppIcon.icns"

# The Dock icons the Mac app picks between at runtime. The Dock draws at
# most 256 points on a 2x screen, so 512 pixels is all it needs.
DOCK_DIR="$ICON_DIR/dock"
BREACHING=(38 0.98 0.47 0.46)
dock_icon() {
  local name=$1; shift
  "$PYTHON" "$ICON_DIR/compose_icon.py" "$WHALE" "$WORK/$name.png" "$@"
  resize "$WORK/$name.png" 512 "$DOCK_DIR/$name.png"
}
mkdir -p "$DOCK_DIR"
dock_icon breaching-light $BREACHING
dock_icon breaching-dark $BREACHING --dark
dock_icon up-close-light --head
dock_icon up-close-dark --head --dark
print "Wrote the Dock icons in $DOCK_DIR"

IOS_ICONS="${ASSETS_DIR:h:h:h}/apps/ios/Gasp/Resources/AppIcon.xcassets"
"$PYTHON" "$ICON_DIR/compose_icon.py" "$WHALE" "$IOS_ICONS/AppIcon.appiconset/AppIcon-1024.png" 38 0.98 0.47 0.46 --ios
"$PYTHON" "$ICON_DIR/compose_icon.py" "$WHALE" "$IOS_ICONS/AppIcon.appiconset/AppIcon-1024-dark.png" 38 0.98 0.47 0.46 --ios --dark
"$PYTHON" "$ICON_DIR/compose_icon.py" "$WHALE" "$IOS_ICONS/AppIconHead.appiconset/AppIconHead-1024.png" --ios --head
"$PYTHON" "$ICON_DIR/compose_icon.py" "$WHALE" "$IOS_ICONS/AppIconHead.appiconset/AppIconHead-1024-dark.png" --ios --head --dark
print "Wrote the iPhone icons in $IOS_ICONS"
