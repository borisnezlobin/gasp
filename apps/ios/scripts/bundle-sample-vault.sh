#!/usr/bin/env bash
# Copies the welcome tour's sample notes, the same ones the Mac offers,
# into the app as its sample vault. "Try the sample vault" writes them into
# a new folder in Documents. Xcode runs this as a build phase.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../../.." && pwd)"
sample="$repo/apps/desktop/assets/tour/sample"
destination="${TARGET_BUILD_DIR:?}/${UNLOCALIZED_RESOURCES_FOLDER_PATH:?}/SampleVault"

rm -rf "$destination"
mkdir -p "$destination"
cp "$sample"/*.md "$destination/"
