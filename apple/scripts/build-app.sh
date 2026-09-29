#!/bin/sh
# Builds Pitboard.app, universal, from Pitboard.xcodeproj and the core's XCFramework, with
# the command line inside it. This is the one way the app is built: here, in CI and in a
# release.
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
# Sparkle decides what is newer by CFBundleVersion, so it counts up with the version
# rather than staying at whatever the project says.
rest=${version#*.}
build=$((${version%%.*} * 10000 + ${rest%%.*} * 100 + ${rest#*.}))

# This clears apple/build and makes it again, so nothing from a previous build survives.
./apple/scripts/build-xcframework.sh

# Unsigned, because the bundle is not finished: the command line, the icon and the man
# pages go in after this, and a signature covers what is inside it. Sparkle is the version
# Package.resolved pins or the build stops, since the app's bill of materials reads it from
# there. Packages are cloned under apple/build, where the release finds Sparkle's tools.
xcodebuild -project apple/Pitboard.xcodeproj -scheme Pitboard -configuration Release \
    -destination 'generic/platform=macOS' \
    -derivedDataPath apple/build/DerivedData \
    -clonedSourcePackagesDirPath apple/build/SourcePackages \
    -onlyUsePackageVersionsFromResolvedFile -quiet \
    ARCHS="arm64 x86_64" ONLY_ACTIVE_ARCH=NO \
    MARKETING_VERSION="$version" CURRENT_PROJECT_VERSION="$build" \
    CODE_SIGNING_ALLOWED=NO build
ditto apple/build/DerivedData/Build/Products/Release/Pitboard.app "$app"
[ -d "$app/Contents/Frameworks/Sparkle.framework" ] || {
    echo "Xcode did not embed Sparkle.framework" >&2
    exit 1
}
# The Share extension, which hands a page's claude.ai link to the app, built for both kinds
# of Mac like the app. The release claims pitboard://, which the extension, the Service and
# the bookmarklet all open; a debug build claims pitboard-debug:// instead.
appex=$app/Contents/PlugIns/PitboardShare.appex
[ -d "$appex" ] || {
    echo "Xcode did not embed PitboardShare.appex" >&2
    exit 1
}
for arch in arm64 x86_64; do
    lipo "$appex/Contents/MacOS/PitboardShare" -verify_arch "$arch"
done
scheme=$(plutil -extract CFBundleURLTypes.0.CFBundleURLSchemes.0 raw "$app/Contents/Info.plist")
[ "$scheme" = pitboard ] || {
    echo "the app claims $scheme://, not pitboard://" >&2
    exit 1
}

# The command line comes inside the app, so one update moves both, and the renewal
# schedule has a pitboard to run: the app has no renewal of its own.
for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --locked --release -p pitboard --target "$target"
done
cli=apple/build/cli-universal
lipo -create target/aarch64-apple-darwin/release/pitboard \
    target/x86_64-apple-darwin/release/pitboard -output "$cli"
# Helpers, which is where a bundle keeps a tool that is not its main program. In MacOS it
# would be the same file as Pitboard on a case-insensitive volume, and overwrite it.
mkdir -p "$app/Contents/Helpers"
cp "$cli" "$app/Contents/Helpers/pitboard"

mkdir -p "$app/Contents/Resources"
swift apple/scripts/make-icon.swift apple/build
iconutil --convert icns --output "$app/Contents/Resources/AppIcon.icns" \
    apple/build/AppIcon.iconset
# The Share menu shows the extension's own icon, which is the app's.
mkdir -p "$appex/Contents/Resources"
cp "$app/Contents/Resources/AppIcon.icns" "$appex/Contents/Resources/AppIcon.icns"

# The man page and completions for whatever links the command line onto PATH, written by
# the build that ships so they describe it, and before signing, since the app's signature
# covers its Resources. It runs from where lipo left it: macOS scans a bundle the first
# time a program inside it runs, and a write into the bundle during the scan fails.
mkdir -p "$app/Contents/Resources/man" "$app/Contents/Resources/completions"
"$cli" manpage > "$app/Contents/Resources/man/pitboard.1"
for shell in bash zsh fish; do
    "$cli" completions "$shell" > "$app/Contents/Resources/completions/pitboard.$shell"
done

if [ -n "${SPARKLE_PUBLIC_KEY:-}" ]; then
    plist=$app/Contents/Info.plist
    /usr/libexec/PlistBuddy -c "Add :SUPublicEDKey string $SPARKLE_PUBLIC_KEY" "$plist"
    /usr/libexec/PlistBuddy -c \
        "Add :SUFeedURL string ${SPARKLE_FEED_URL:-$FEED}" "$plist"
    /usr/libexec/PlistBuddy -c "Add :SUEnableAutomaticChecks bool true" "$plist"
fi

# Notarization wants the hardened runtime and a secure timestamp; an ad-hoc signature can
# have neither. Under the hardened runtime it would refuse to load its own framework,
# since ad-hoc signatures share no team.
if [ "$identity" = "-" ]; then
    options="--timestamp=none"
else
    options="--timestamp --options runtime"
fi
# Sparkle's helpers are signed before the framework, and the framework and the command
# line before the app: a signature covers what is inside it, so the inside has to be
# settled first. A bare executable has no Info.plist to take an identifier from, so the
# command line is given one under the app's.
# shellcheck disable=SC2086 # $options is a list of flags.
codesign --force $options --sign "$identity" -i com.usepitboard.Pitboard.cli \
    "$app/Contents/Helpers/pitboard"
for helper in "$app/Contents/Frameworks/Sparkle.framework/Versions/"*/XPCServices/*.xpc \
    "$app/Contents/Frameworks/Sparkle.framework/Versions/"*/Updater.app \
    "$app/Contents/Frameworks/Sparkle.framework/Versions/"*/Autoupdate; do
    # shellcheck disable=SC2086 # $options is a list of flags.
    [ -e "$helper" ] && codesign --force $options --sign "$identity" "$helper"
done
# shellcheck disable=SC2086 # $options is a list of flags.
codesign --force $options --sign "$identity" "$app/Contents/Frameworks/Sparkle.framework"
# An app extension must be sandboxed, or macOS refuses to run it, and a signature made
# without --entitlements carries none. It is signed before the app, and the app's own
# signature is made without --deep, so it keeps this one. The key is written with its dots
# escaped, which plutil otherwise reads as a path of four keys.
# shellcheck disable=SC2086 # $options is a list of flags.
codesign --force $options --sign "$identity" \
    --entitlements apple/ShareExtension/PitboardShare.entitlements "$appex"
sandboxed=$(codesign -d --entitlements - --xml "$appex" 2>/dev/null |
    plutil -extract 'com\.apple\.security\.app-sandbox' raw - 2>/dev/null || true)
[ "$sandboxed" = true ] || {
    echo "the share extension is not sandboxed" >&2
    exit 1
}
# shellcheck disable=SC2086
codesign --force $options --sign "$identity" "$app"
codesign --verify --strict --deep "$app"
echo "built $app ($version, $(lipo -archs "$app/Contents/MacOS/Pitboard"))"
