#!/bin/sh
# Assembles Pitboard.app, universal, from the Swift package and the core's XCFramework.
#
# Signing: set SIGN_IDENTITY to a Developer ID Application identity to make a build others
# can run; without it the app is signed ad-hoc, which is enough on the machine that built it.
#
# Updates: set SPARKLE_PUBLIC_KEY to the EdDSA public key that signs the appcast, and
# SPARKLE_FEED_URL to override where it is read from. Without the key the app ships with no
# updater, which is what a build from a clone wants.
set -eu

cd "$(dirname "$0")/../.."
export MACOSX_DEPLOYMENT_TARGET=14.0
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[ -n "$version" ] || { echo "no version in Cargo.toml" >&2; exit 1; }
identity=${SIGN_IDENTITY:--}
# GitHub serves the newest release's assets at a fixed address, so the feed has one even
# before the site does.
FEED=https://github.com/datlechin/pitboard/releases/latest/download/appcast.xml
app=apple/build/Pitboard.app
rm -rf "$app"
mkdir -p apple/build

./apple/scripts/build-xcframework.sh
# One build per architecture, then lipo: `--arch x --arch y` builds through Xcode's build
# system instead, which does not find the core's static library in every toolchain.
slices=""
for triple in arm64-apple-macosx x86_64-apple-macosx; do
    swift build --package-path apple --configuration release --triple "$triple" --product Pitboard
    built=$(swift build --package-path apple --configuration release --triple "$triple" \
        --show-bin-path)/Pitboard
    cp "$built" "apple/build/Pitboard-$triple"
    slices="$slices apple/build/Pitboard-$triple"
done
binary=apple/build/Pitboard-universal
# shellcheck disable=SC2086 # the slices are paths this script just made.
lipo -create $slices -output "$binary"

mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$app/Contents/Frameworks"
cp "$binary" "$app/Contents/MacOS/Pitboard"
# The plist carries a placeholder version; the crate's is the one that ships.
sed "s/>0\.0\.0</>$version</" apple/Resources/Info.plist > "$app/Contents/Info.plist"
printf 'APPL????' > "$app/Contents/PkgInfo"

swift apple/scripts/make-icon.swift apple/build
iconutil --convert icns --output "$app/Contents/Resources/AppIcon.icns" \
    apple/build/AppIcon.iconset

sparkle=$(find apple/.build/artifacts -type d -name Sparkle.framework | head -1)
[ -n "$sparkle" ] || { echo "Sparkle.framework not built" >&2; exit 1; }
ditto "$sparkle" "$app/Contents/Frameworks/Sparkle.framework"

if [ -n "${SPARKLE_PUBLIC_KEY:-}" ]; then
    plist=$app/Contents/Info.plist
    /usr/libexec/PlistBuddy -c "Add :SUPublicEDKey string $SPARKLE_PUBLIC_KEY" "$plist"
    /usr/libexec/PlistBuddy -c \
        "Add :SUFeedURL string ${SPARKLE_FEED_URL:-$FEED}" "$plist"
    /usr/libexec/PlistBuddy -c "Add :SUEnableAutomaticChecks bool true" "$plist"
fi

# Notarization wants the hardened runtime and a secure timestamp; an ad-hoc signature can
# have neither — under the hardened runtime it would refuse to load its own framework,
# since ad-hoc signatures share no team.
if [ "$identity" = "-" ]; then
    options="--timestamp=none"
else
    options="--timestamp --options runtime"
fi
# Sparkle's helpers are signed before the framework, and the framework before the app:
# a signature covers what is inside it, so the inside has to be settled first.
for helper in "$app/Contents/Frameworks/Sparkle.framework/Versions/"*/XPCServices/*.xpc \
    "$app/Contents/Frameworks/Sparkle.framework/Versions/"*/Updater.app \
    "$app/Contents/Frameworks/Sparkle.framework/Versions/"*/Autoupdate; do
    # shellcheck disable=SC2086 # $options is a list of flags.
    [ -e "$helper" ] && codesign --force $options --sign "$identity" "$helper"
done
# shellcheck disable=SC2086 # $options is a list of flags.
codesign --force $options --sign "$identity" "$app/Contents/Frameworks/Sparkle.framework"
# shellcheck disable=SC2086
codesign --force $options --sign "$identity" "$app"
codesign --verify --strict --deep "$app"
echo "built $app ($version, $(lipo -archs "$app/Contents/MacOS/Pitboard"))"
