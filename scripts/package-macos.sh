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
      cargo build --profile dist --locked -p editor-desktop --target "$target" --manifest-path "$REPO_ROOT/Cargo.toml"
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

make_dmg() {
  local dmg="$1" staging="$OUT_DIR/dmg-staging"
  rm -rf "$staging" "$dmg"
  mkdir -p "$staging"
  cp -R "$APP_DIR" "$staging/"
  ln -s /Applications "$staging/Applications"
  hdiutil create -quiet -volname "$APP_NAME" -srcfolder "$staging" -format UDZO "$dmg"
  rm -rf "$staging"
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
