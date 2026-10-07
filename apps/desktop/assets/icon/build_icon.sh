#!/bin/zsh
# Builds apps/desktop/assets/AppIcon.icns and the iPhone app's icon from the
# committed whale render.
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

IOS_ICON="${ASSETS_DIR:h:h:h}/apps/ios/Gasp/Resources/AppIcon.xcassets/AppIcon.appiconset/AppIcon-1024.png"
"$PYTHON" "$ICON_DIR/compose_icon.py" "$WHALE" "$IOS_ICON" 38 0.98 0.47 0.46 --ios
print "Wrote $IOS_ICON"
