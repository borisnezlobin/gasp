#!/bin/zsh
# Builds Gasp.app for Apple Silicon and Intel Macs, signs it, and packages it as a .dmg.
#
#   scripts/package-macos.sh
#
# Signing uses DEVELOPER_ID when it's set, for example
#   DEVELOPER_ID="Developer ID Application: Your Name (TEAMID)"
# and otherwise signs ad hoc, which only runs on this Mac.
# Notarizing needs a notarytool profile saved with `xcrun notarytool store-credentials`;
# set NOTARY_PROFILE to its name. Output goes to target/package/.
#
# The disk image opens to a designed window (see make_styled_dmg below).
# Laying it out has Finder do the work, so the first run asks whether this
# terminal may control Finder (System Settings > Privacy & Security >
# Automation); the volume's icon needs SetFile from the Command Line
# Tools. If the layout fails, the image is made plain, as before.

set -euo pipefail

APP_NAME="Gasp"
BUNDLE_ID="com.borisnezlobin.gasp"
EXECUTABLE="gasp"
MINIMUM_MACOS="12.0"
TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)

REPO_ROOT="${0:A:h:h}"
OUT_DIR="$REPO_ROOT/target/package"
APP_DIR="$OUT_DIR/$APP_NAME.app"

step() { print -P "%B==> $1%b"; }

workspace_version() {
  sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' "$REPO_ROOT/Cargo.toml"
}

build_universal_binary() {
  local target binaries=()
  for target in $TARGETS; do
    rustup target add "$target" >/dev/null
    step "Building for $target"
    MACOSX_DEPLOYMENT_TARGET="$MINIMUM_MACOS" \
      cargo build --profile dist --locked -p gasp-desktop --target "$target" --manifest-path "$REPO_ROOT/Cargo.toml"
    binaries+=("$REPO_ROOT/target/$target/dist/$EXECUTABLE")
  done
  mkdir -p "$APP_DIR/Contents/MacOS"
  lipo -create -output "$APP_DIR/Contents/MacOS/$EXECUTABLE" $binaries
}

write_info_plist() {
  local version="$1" build_number="$2"
  cat >"$APP_DIR/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleExecutable</key><string>$EXECUTABLE</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$build_number</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>LSMinimumSystemVersion</key><string>$MINIMUM_MACOS</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
PLIST
}

copy_resources() {
  local assets="$REPO_ROOT/apps/desktop/assets"
  mkdir -p "$APP_DIR/Contents/Resources"
  cp "$assets/AppIcon.icns" "$APP_DIR/Contents/Resources/AppIcon.icns"
  cp "$assets/icon/THIRD_PARTY_NOTICES.txt" "$APP_DIR/Contents/Resources/THIRD_PARTY_NOTICES.txt"
}

sign() {
  local target="$1"
  if [[ -n "${DEVELOPER_ID:-}" ]]; then
    codesign --force --timestamp --options runtime --sign "$DEVELOPER_ID" "$target"
  else
    codesign --force --sign - "$target"
  fi
}

# The disk image opens to a designed window: the app surfacing out of
# the page's text lines with "Double-click to open" beside it, and an
# Applications alias off to the right for people who'd rather drag.
# Opened, the app offers to move itself to Applications. The background
# is drawn by apps/desktop/assets/dmg/make_background.py; the sizes and
# positions here are its WIDTH, HEIGHT, ICON_SIZE, APP_CENTER and
# APPLICATIONS_CENTER and must stay in step with it.
DMG_ART="$REPO_ROOT/apps/desktop/assets/dmg"
DMG_WIDTH=660
DMG_HEIGHT=368
DMG_ICON_SIZE=128
DMG_TEXT_SIZE=13
DMG_APP_POSITION="196, 138"
DMG_APPLICATIONS_POSITION="548, 138"
# Where the window opens on screen, and the height of its title bar,
# which the window's bounds include.
DMG_WINDOW_ORIGIN=(200 120)
DMG_TITLE_BAR=28
# Room left in the writable image for Finder's .DS_Store.
DMG_SPARE_KB=20480

# Makes the plain image: the app and an Applications alias, laid out
# however Finder likes. Used when the styled one can't be made, such as
# when this terminal may not control Finder.
make_plain_dmg() {
  local dmg="$1" staging="$2"
  hdiutil create -quiet -volname "$APP_NAME" -srcfolder "$staging" -format UDZO "$dmg"
}

make_dmg() {
  local dmg="$1" staging="$OUT_DIR/dmg-staging"
  rm -rf "$staging" "$dmg"
  mkdir -p "$staging"
  cp -R "$APP_DIR" "$staging/"
  ln -s /Applications "$staging/Applications"
  if ! make_styled_dmg "$dmg" "$staging"; then
    print "The styled disk image failed; making a plain one instead." >&2
    rm -f "$dmg"
    rm -rf "$staging/.background" "$staging/.VolumeIcon.icns"
    make_plain_dmg "$dmg" "$staging"
  fi
  rm -rf "$staging"
}

# Builds a writable image, has Finder lay out its window, and compresses
# it. Every step is checked by hand: errexit doesn't apply inside a
# function called as a condition.
make_styled_dmg() {
  local dmg="$1" staging="$2" writable="$OUT_DIR/$APP_NAME-writable.dmg" mount size_kb
  mkdir -p "$staging/.background" || return 1
  tiffutil -cathidpicheck "$DMG_ART/background.png" "$DMG_ART/background@2x.png" \
    -out "$staging/.background/background.tiff" || return 1
  cp "$REPO_ROOT/apps/desktop/assets/AppIcon.icns" "$staging/.VolumeIcon.icns" || return 1
  size_kb=$(( $(du -sk "$staging" | cut -f1) + DMG_SPARE_KB ))
  rm -f "$writable"
  hdiutil create -quiet -volname "$APP_NAME" -srcfolder "$staging" -fs HFS+ \
    -format UDRW -size "${size_kb}k" "$writable" || return 1
  # attach prints tab-separated lines; the mounted volume's ends in its path.
  mount=$(hdiutil attach -readwrite -noverify -noautoopen "$writable" \
    | awk -F'\t' '$NF ~ "^/" { mount = $NF } END { print mount }')
  if [[ -z "$mount" ]]; then
    rm -f "$writable"
    return 1
  fi
  local styled=0
  # Finder finds the volume by its name, so another one by that name
  # would be styled instead.
  if [[ "${mount:t}" != "$APP_NAME" ]]; then
    print "Another volume named $APP_NAME is mounted; eject it to get the styled window." >&2
    styled=1
  fi
  (( styled == 0 )) && { style_volume "$mount" || styled=1 }
  sync
  hdiutil detach -quiet "$mount" || hdiutil detach -quiet -force "$mount" || styled=1
  if (( styled == 0 )); then
    hdiutil convert -quiet "$writable" -format UDZO -imagekey zlib-level=9 -o "$dmg" || styled=1
  fi
  rm -f "$writable"
  return $styled
}

# Gives the mounted volume its icon and has Finder save the window's
# look into the volume's .DS_Store.
style_volume() {
  local mount="$1"
  if command -v SetFile >/dev/null; then
    SetFile -c icnC "$mount/.VolumeIcon.icns" && SetFile -a C "$mount" || return 1
  else
    print "SetFile isn't installed (xcode-select --install); the volume keeps the plain icon." >&2
  fi
  local left=${DMG_WINDOW_ORIGIN[1]} top=${DMG_WINDOW_ORIGIN[2]}
  local right=$(( left + DMG_WIDTH )) bottom=$(( top + DMG_HEIGHT + DMG_TITLE_BAR ))
  step "Laying out the disk image window (Finder may ask to be controlled)"
  osascript <<APPLESCRIPT || return 1
tell application "Finder"
  set theVolume to disk "$APP_NAME"
  open theVolume
  set theWindow to container window of theVolume
  set current view of theWindow to icon view
  set toolbar visible of theWindow to false
  set statusbar visible of theWindow to false
  set bounds of theWindow to {$left, $top, $right, $bottom}
  set viewOptions to icon view options of theWindow
  set arrangement of viewOptions to not arranged
  set icon size of viewOptions to $DMG_ICON_SIZE
  set text size of viewOptions to $DMG_TEXT_SIZE
  set label position of viewOptions to bottom
  set shows item info of viewOptions to false
  set background picture of viewOptions to file ".background:background.tiff" of theVolume
  set position of item "$APP_NAME.app" of theVolume to {$DMG_APP_POSITION}
  set position of item "Applications" of theVolume to {$DMG_APPLICATIONS_POSITION}
  close theWindow
  open theVolume
  update theVolume without registering applications
  delay 2
  close theWindow
end tell
APPLESCRIPT
  # Finder writes .DS_Store when it gets round to it.
  local waited=0
  while [[ ! -f "$mount/.DS_Store" ]] && (( waited < 10 )); do
    sleep 1
    (( waited += 1 ))
  done
  [[ -f "$mount/.DS_Store" ]] || return 1
  chmod -Rf go-w "$mount" 2>/dev/null
  return 0
}

notarize() {
  local dmg="$1"
  if [[ -z "${NOTARY_PROFILE:-}" || -z "${DEVELOPER_ID:-}" ]]; then
    step "Skipping notarization (set DEVELOPER_ID and NOTARY_PROFILE to notarize)"
    return 0
  fi
  step "Notarizing (this usually takes a few minutes)"
  xcrun notarytool submit "$dmg" --keychain-profile "$NOTARY_PROFILE" --wait
  xcrun stapler staple "$dmg"
}

main() {
  local version build_number dmg
  version=$(workspace_version)
  build_number=$(git -C "$REPO_ROOT" rev-list --count HEAD)
  dmg="$OUT_DIR/$APP_NAME-$version.dmg"

  rm -rf "$APP_DIR"
  build_universal_binary
  write_info_plist "$version" "$build_number"
  copy_resources
  step "Signing"
  sign "$APP_DIR"
  codesign --verify --strict "$APP_DIR"
  step "Packaging $dmg"
  make_dmg "$dmg"
  [[ -n "${DEVELOPER_ID:-}" ]] && sign "$dmg"
  notarize "$dmg"
  step "Done: $dmg"
}

main "$@"
