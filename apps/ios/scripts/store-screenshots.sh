#!/usr/bin/env bash
# Captures the App Store screenshots from a 6.9" iPhone simulator, on the
# demo vault in apps/ios/store/vault, into target/store/raw. Run
# apps/ios/store/compose.py afterwards to lay them out with their captions.
# Simulator.app never opens.
#
#   apps/ios/scripts/store-screenshots.sh
set -euo pipefail

ios_dir="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$ios_dir/../.." && pwd)"
derived="$repo/target/ios-derived"
out="$repo/target/store/raw"
bundle=com.borisnezlobin.gasp
device_name="Gasp Store 6.9"

device="$(xcrun simctl list devices available | grep -F "$device_name (" | grep -oE '[0-9A-F-]{36}' | head -1 || true)"
if [[ -z "$device" ]]; then
  runtime="$(xcrun simctl list runtimes | grep -oE 'com\.apple\.CoreSimulator\.SimRuntime\.iOS-[0-9-]+' | tail -1)"
  device="$(xcrun simctl create "$device_name" com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro-Max "$runtime")"
fi

(cd "$ios_dir" && xcodegen generate --quiet)
xcodebuild -project "$ios_dir/Gasp.xcodeproj" -scheme Gasp -configuration Release \
  -destination "id=$device" -derivedDataPath "$derived" \
  ARCHS=arm64 ONLY_ACTIVE_ARCH=YES CODE_SIGNING_ALLOWED=NO -quiet build

xcrun simctl boot "$device" 2>/dev/null || true
xcrun simctl bootstatus "$device" -b >/dev/null
xcrun simctl install "$device" "$derived/Build/Products/Release-iphonesimulator/Gasp.app"
# Keeps iOS's first-run "slide to type" tip off the keyboard.
xcrun simctl spawn "$device" defaults write com.apple.keyboard.preferences DidShowContinuousPathIntroduction -bool true
xcrun simctl status_bar "$device" override --time "9:41" --dataNetwork wifi --wifiMode active \
  --wifiBars 3 --cellularMode active --cellularBars 4 --batteryState charged --batteryLevel 100

fresh_vault() {
  xcrun simctl terminate "$device" "$bundle" 2>/dev/null || true
  local data
  data="$(xcrun simctl get_app_container "$device" "$bundle" data)"
  rm -rf "$data/Documents/Vault" "$data/Library/Preferences/$bundle.plist"
  mkdir -p "$data/Documents"
  cp -R "$ios_dir/store/vault" "$data/Documents/Vault"
}

# note_link <path> [line]: a link that opens the note, with the cursor on a
# line counted from 1.
note_link() {
  local path="${1// /%20}"
  echo "gasp://open?path=$path${2:+&line=$2}"
}

launch() {
  xcrun simctl launch --terminate-running-process "$device" "$bundle" -skipWelcome YES "$@" >/dev/null
}

# shot <name> <light|dark> <note> <line or ""> [launch arguments…]
shot() {
  local name="$1" appearance="$2" note="$3" line="$4"
  shift 4
  xcrun simctl ui "$device" appearance "$appearance"
  launch -open "$(note_link "$note" "$line")" "$@"
  sleep 5
  xcrun simctl io "$device" screenshot "$out/$name.png" >/dev/null 2>&1
  echo "captured $name"
}

mkdir -p "$out"
fresh_vault
shot 1-note light "Field notes/Humpback song.md" ""
shot 2-typing light "Field notes/Humpback song.md" 1 -editing YES
shot 3-dark dark "Trips/Maui in February.md" ""
shot 4-files light "Field notes/Humpback song.md" "" -run sidebar.files.toggle
for note in "Reading list.md" "Trips/Maui in February.md" "Ideas.md" "Field notes/Cultural transmission.md"; do
  launch -open "$(note_link "$note")"
  sleep 3
done
shot 5-tabs light "Field notes/Humpback song.md" "" -run overview
xcrun simctl ui "$device" appearance light
xcrun simctl status_bar "$device" clear
xcrun simctl shutdown "$device"
