#!/usr/bin/env bash
# Builds the app, installs it on an iPhone simulator and launches it,
# without bringing Simulator.app forward. Run build-core.sh first. Pass a
# simulator's name or UDID to pick one; otherwise the first available
# iPhone is used. Run `open -a Simulator` to watch it.
#
#   apps/ios/scripts/run-simulator.sh
#   apps/ios/scripts/run-simulator.sh "iPhone 17 Pro"
set -euo pipefail

ios_dir="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$ios_dir/../.." && pwd)"
derived="$repo/target/ios-derived"

udid_pattern='[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}'
device="$(xcrun simctl list devices available | grep -E "^\s+${1:-iPhone}" | grep -m1 -oE "$udid_pattern" || true)"
if [[ "${1:-}" =~ ^$udid_pattern$ ]]; then
  device="$1"
fi
if [[ -z "$device" ]]; then
  echo "No iPhone simulator found. Create one in Xcode's Devices and Simulators window." >&2
  exit 1
fi

cd "$ios_dir"
xcodegen generate --quiet
xcodebuild -project Gasp.xcodeproj -scheme Gasp -configuration Debug \
  -destination "platform=iOS Simulator,id=$device" -derivedDataPath "$derived" \
  -quiet build

xcrun simctl boot "$device" 2>/dev/null || true
xcrun simctl install "$device" "$derived/Build/Products/Debug-iphonesimulator/Gasp.app"
xcrun simctl launch "$device" com.borisnezlobin.gasp
