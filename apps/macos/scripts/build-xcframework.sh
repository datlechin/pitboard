#!/bin/sh
# Builds the Rust the macOS app links, universal, each library with its generated Swift
# bindings, for the Swift package in apps/macos. Outputs are build products and are not
# committed:
#
# - PitboardFFI.xcframework and Sources/PitboardBindings: Pitboard's core, which the app
#   links.
# - PitboardShareFFI.xcframework and Sources/PitboardShareBindings: pitboard-share-ffi, the
#   check of a shared link, which the Share extension links.
#
# --fixture builds the core with its `fixture` feature, for a debug build and the UI tests to
# launch into a fixture: the worlds are Rust, and only a library built so has them. Its
# bindings are the same either way. scripts/build-app.sh never passes it, and checks.
set -eu

features=
case "${1:-}" in
    --fixture) features=fixture ;;
    "") ;;
    *)
        echo "usage: $0 [--fixture]" >&2
        exit 64
        ;;
esac

cd "$(dirname "$0")/../../.."
export MACOSX_DEPLOYMENT_TARGET=14.0
# The core links SQLite for Claude's cookie jar: the SDK's. libsqlite3-sys asks pkg-config
# even where SQLITE3_LIB_DIR is set, and takes another SQLite it knows, unless
# SQLITE3_NO_PKG_CONFIG is set too. scripts/build-app.sh checks what the app links.
SQLITE3_LIB_DIR="$(xcrun --show-sdk-path)/usr/lib"
SQLITE3_NO_PKG_CONFIG=1
export SQLITE3_LIB_DIR SQLITE3_NO_PKG_CONFIG
out=apps/macos/build
package=apps/macos
rm -rf "$out"

# build <crate> <module> <bindings> [<features>]: the crate's static library for both kinds
# of Mac, as <module>.xcframework, with its Swift bindings in Sources/<bindings>. The
# uniffi.toml of the crate whose exports it holds names the bindings' module: pitboard-model's
# for pitboard-ffi, pitboard-share-ffi's own. The headers sit in a folder named for the
# module, where Clang looks for its module map, so the two frameworks' maps never share a
# path when Xcode gathers their headers.
build() {
    crate=$1
    module=$2
    library=lib$(echo "$crate" | tr - _).a
    work=$out/$module
    generated=$package/Sources/$3
    rm -rf "$generated"
    mkdir -p "$work/bindings" "$work/headers/$module" "$generated"
    for target in aarch64-apple-darwin x86_64-apple-darwin; do
        cargo build --locked --release -p "$crate" --target "$target" \
            ${4:+--features "$4"}
    done
    lipo -create \
        "target/aarch64-apple-darwin/release/$library" \
        "target/x86_64-apple-darwin/release/$library" \
        -output "$work/$library"

    cargo run --locked --release -p uniffi-bindgen-swift -- \
        "target/aarch64-apple-darwin/release/$library" "$work/bindings" \
        --swift-sources --headers --modulemap \
        --module-name "$module" --modulemap-filename module.modulemap
    mv "$work/bindings"/*.h "$work/bindings/module.modulemap" "$work/headers/$module/"
    mv "$work/bindings"/*.swift "$generated/"

    rm -rf "$package/$module.xcframework"
    xcodebuild -create-xcframework \
        -library "$work/$library" -headers "$work/headers" \
        -output "$package/$module.xcframework"
}

build pitboard-ffi PitboardFFI PitboardBindings "$features"
build pitboard-share-ffi PitboardShareFFI PitboardShareBindings
