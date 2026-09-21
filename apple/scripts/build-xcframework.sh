#!/bin/sh
# Builds pitboard's core as PitboardFFI.xcframework, universal, with its generated Swift
# bindings, for apple/PitboardKit. Outputs are build products and are not committed.
set -eu

cd "$(dirname "$0")/../.."
export MACOSX_DEPLOYMENT_TARGET=14.0
out=apple/build
package=apple
generated=$package/Sources/PitboardBindings
rm -rf "$out"
mkdir -p "$out/bindings" "$out/headers" "$generated"

for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --locked --release -p pitboard-ffi --target "$target"
done
lipo -create \
    target/aarch64-apple-darwin/release/libpitboard_ffi.a \
    target/x86_64-apple-darwin/release/libpitboard_ffi.a \
    -output "$out/libpitboard_ffi.a"

cargo run --locked --release -p uniffi-bindgen-swift -- \
    target/aarch64-apple-darwin/release/libpitboard_ffi.a "$out/bindings" \
    --swift-sources --headers --modulemap \
    --module-name PitboardFFI --modulemap-filename module.modulemap
mv "$out/bindings"/*.h "$out/bindings/module.modulemap" "$out/headers/"
mv "$out/bindings"/*.swift "$generated/"

rm -rf "$package/PitboardFFI.xcframework"
xcodebuild -create-xcframework \
    -library "$out/libpitboard_ffi.a" -headers "$out/headers" \
    -output "$package/PitboardFFI.xcframework"
