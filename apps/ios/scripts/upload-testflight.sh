#!/usr/bin/env bash
# Archives the iPhone app, signs it for the App Store and uploads it to App
# Store Connect, where it reaches TestFlight once Apple has processed it.
#
#   apps/ios/scripts/upload-testflight.sh
#
# Signs and uploads with an App Store Connect API key when ASC_KEY_ID and
# ASC_ISSUER_ID are set, in the environment or in ~/.appstoreconnect/gasp.env
# (the key itself in ~/.appstoreconnect/private_keys/AuthKey_<id>.p8), and
# otherwise with the account Xcode is signed in to, whose session expires.
# Each build is numbered by the commit count, so every upload is newer than
# the last.
set -euo pipefail

ios_dir="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$ios_dir/../.." && pwd)"
build_dir="$repo/target/ios-archive"
build_number="$(git -C "$repo" rev-list --count HEAD)"

local_settings="$HOME/.appstoreconnect/gasp.env"
if [[ -z "${ASC_KEY_ID:-}" && -f "$local_settings" ]]; then
  # shellcheck source=/dev/null
  source "$local_settings"
fi

auth=()
if [[ -n "${ASC_KEY_ID:-}" && -n "${ASC_ISSUER_ID:-}" ]]; then
  auth=(
    -authenticationKeyPath "$HOME/.appstoreconnect/private_keys/AuthKey_${ASC_KEY_ID}.p8"
    -authenticationKeyID "$ASC_KEY_ID"
    -authenticationKeyIssuerID "$ASC_ISSUER_ID"
  )
fi

"$ios_dir/scripts/build-core.sh"
(cd "$ios_dir" && xcodegen generate --quiet)
rm -rf "$build_dir"

xcodebuild -project "$ios_dir/Gasp.xcodeproj" -scheme Gasp -configuration Release \
  -destination 'generic/platform=iOS' -archivePath "$build_dir/Gasp.xcarchive" \
  -allowProvisioningUpdates ${auth[@]+"${auth[@]}"} \
  CURRENT_PROJECT_VERSION="$build_number" archive

xcodebuild -exportArchive -archivePath "$build_dir/Gasp.xcarchive" \
  -exportOptionsPlist "$ios_dir/ExportOptions.plist" -exportPath "$build_dir/export" \
  -allowProvisioningUpdates ${auth[@]+"${auth[@]}"}

echo "Uploaded build $build_number. It shows in TestFlight once Apple has processed it."
