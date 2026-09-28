#!/usr/bin/env bash
# Copies a few notes from the synthetic corpus into the app as its sample
# vault, which the app copies into Documents on first launch. Xcode runs
# this as a build phase.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../../.." && pwd)"
corpus="$repo/fixtures/corpus"
destination="${TARGET_BUILD_DIR:?}/${UNLOCALIZED_RESOURCES_FOLDER_PATH:?}/SampleVault"

notes=(
  "Course Notes/Classical Mechanics/Entropy.md"
  "On Formal Model.md"
  "On Stronger Chapter.md"
  "Summary.md"
  "Reading/Obvious Draft.md"
  "Lemma.md"
  "Physics/Rigid Bodies.md"
)

rm -rf "$destination"
for note in "${notes[@]}"; do
  mkdir -p "$destination/$(dirname "$note")"
  cp "$corpus/$note" "$destination/$note"
done
