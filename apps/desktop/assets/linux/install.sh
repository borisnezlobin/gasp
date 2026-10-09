#!/bin/sh
# Installs Gasp from this folder for you alone, in ~/.local, with a
# launcher entry and icons:
#
#   ./install.sh              install, or update an earlier install
#   ./install.sh --uninstall  remove it again (your notes are never touched)
#
# Gasp updates itself from there when a new version is out.
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
LIB="$HOME/.local/lib/gasp"
BIN="$HOME/.local/bin"
ID=com.borisnezlobin.gasp

if [ "${1:-}" = "--uninstall" ]; then
  rm -f "$BIN/gasp" "$DATA/applications/$ID.desktop" "$DATA/metainfo/$ID.metainfo.xml"
  rm -rf "$LIB"
  for icon in "$DATA"/icons/hicolor/*/apps/$ID.png; do rm -f "$icon"; done
  command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$DATA/applications" || true
  echo "Gasp is uninstalled. Your notes and settings are where they were."
  exit 0
fi

mkdir -p "$LIB" "$BIN" "$DATA/applications" "$DATA/metainfo"
# A new file moved into place, so a running Gasp keeps the old one.
cp "$HERE/gasp" "$LIB/gasp.new" && chmod 755 "$LIB/gasp.new" && mv -f "$LIB/gasp.new" "$LIB/gasp"
cp "$HERE/THIRD_PARTY_NOTICES.txt" "$LIB/" 2>/dev/null || true
ln -sf "$LIB/gasp" "$BIN/gasp"

for folder in "$HERE"/share/icons/hicolor/*/apps; do
  size=$(basename "$(dirname "$folder")")
  mkdir -p "$DATA/icons/hicolor/$size/apps"
  cp "$folder/$ID.png" "$DATA/icons/hicolor/$size/apps/"
done
# The launcher runs the installed copy by its full path, so it works
# whether or not ~/.local/bin is on PATH.
sed "s|^Exec=gasp|Exec=$LIB/gasp|" "$HERE/share/applications/$ID.desktop" > "$DATA/applications/$ID.desktop"
cp "$HERE/share/metainfo/$ID.metainfo.xml" "$DATA/metainfo/"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$DATA/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$DATA/icons/hicolor" 2>/dev/null || true

echo "Gasp is installed. Open it from your apps, or run: $LIB/gasp"
case ":$PATH:" in
  *":$BIN:"*) ;;
  *) echo "(Add $BIN to your PATH to run it as just: gasp)" ;;
esac
