#!/bin/sh
# Assembles Pitboard.app, universal, from the Swift package and the core's XCFramework.
#
# Signing: set SIGN_IDENTITY to a Developer ID Application identity to make a build others
# can run; without it the app is signed ad-hoc, which is enough on the machine that built it.
set -eu

cd "$(dirname "$0")/../.."
export MACOSX_DEPLOYMENT_TARGET=14.0
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[ -n "$version" ] || { echo "no version in Cargo.toml" >&2; exit 1; }
identity=${SIGN_IDENTITY:--}
app=apple/build/Pitboard.app

./apple/scripts/build-xcframework.sh
swift build --package-path apple --configuration release \
    --arch arm64 --arch x86_64 --product Pitboard
binary=$(swift build --package-path apple --configuration release \
    --arch arm64 --arch x86_64 --show-bin-path)/Pitboard

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/Pitboard"
# The plist carries a placeholder version; the crate's is the one that ships.
sed "s/>0\.0\.0</>$version</" apple/Resources/Info.plist > "$app/Contents/Info.plist"
printf 'APPL????' > "$app/Contents/PkgInfo"

# The hardened runtime is what notarization requires, so it is on from the start.
codesign --force --options runtime --timestamp=none \
    --sign "$identity" "$app"
codesign --verify --strict "$app"
echo "built $app ($version, $(lipo -archs "$app/Contents/MacOS/Pitboard"))"
