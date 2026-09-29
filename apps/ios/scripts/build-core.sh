#!/usr/bin/env bash
# Builds the Rust core for iPhones and the Apple-silicon simulator into
# apps/ios/Core/GaspCore.xcframework, and generates its Swift bindings
# into apps/ios/Core/Generated. Run it from anywhere, after any change to
# the Rust code, before building the app.
#
#   apps/ios/scripts/build-core.sh           # release build
#   apps/ios/scripts/build-core.sh --debug   # faster to build, slower to run
set -euo pipefail

ios_dir="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$ios_dir/../.." && pwd)"
core_dir="$ios_dir/Core"
work_dir="$repo/target/ios-core"
targets=(aarch64-apple-ios aarch64-apple-ios-sim)

profile=release
profile_dir=release
if [[ "${1:-}" == "--debug" ]]; then
  profile=dev
  profile_dir=debug
fi

# Xcode's build phases run without the shell's PATH.
export PATH="$HOME/.cargo/bin:$PATH"
# Match the app's deployment target, so the linker doesn't warn that the
# library was built for a newer iOS.
export IPHONEOS_DEPLOYMENT_TARGET=17.0

cd "$repo"
rustup target add "${targets[@]}" >/dev/null 2>&1

for target in "${targets[@]}"; do
  echo "Building the core for $target"
  cargo rustc --quiet -p editor-ffi --lib --crate-type staticlib \
    --profile "$profile" --target "$target"
done

library() {
  echo "$repo/target/$1/$profile_dir/libeditor_ffi.a"
}

echo "Generating the Swift bindings"
rm -rf "$work_dir"
mkdir -p "$work_dir/bindings" "$work_dir/headers"
cargo run --quiet -p uniffi-bindgen -- generate \
  --library "$(library aarch64-apple-ios-sim)" \
  --language swift \
  --out-dir "$work_dir/bindings"
cp "$work_dir/bindings/editor_ffiFFI.h" "$work_dir/headers/"
cp "$work_dir/bindings/editor_ffiFFI.modulemap" "$work_dir/headers/module.modulemap"

echo "Packing GaspCore.xcframework"
rm -rf "$core_dir/GaspCore.xcframework"
mkdir -p "$core_dir/Generated"
xcodebuild -create-xcframework \
  -library "$(library aarch64-apple-ios)" -headers "$work_dir/headers" \
  -library "$(library aarch64-apple-ios-sim)" -headers "$work_dir/headers" \
  -output "$core_dir/GaspCore.xcframework" >/dev/null
cp "$work_dir/bindings/editor_ffi.swift" "$core_dir/Generated/editor_ffi.swift"

echo "Done: $core_dir"
