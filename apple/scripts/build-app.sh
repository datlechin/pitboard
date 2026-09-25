#!/bin/sh
# Assembles Pitboard.app, universal, from the Swift package and the core's XCFramework, with
# the command line inside it.
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

# This clears apple/build and makes it again, so nothing from a previous build survives.
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

# The command line comes inside the app, so one update moves both, and the renewal
# schedule has a pitboard to run: the app has no renewal of its own. Not named
# pitboard-universal, which on a case-insensitive volume is the file above.
for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --locked --release -p pitboard --target "$target"
done
cli=apple/build/cli-universal
lipo -create target/aarch64-apple-darwin/release/pitboard \
    target/x86_64-apple-darwin/release/pitboard -output "$cli"

mkdir -p "$app/Contents/MacOS" "$app/Contents/Helpers" "$app/Contents/Resources" \
    "$app/Contents/Frameworks"
cp "$binary" "$app/Contents/MacOS/Pitboard"
# Helpers, which is where a bundle keeps a tool that is not its main program. In MacOS it
# would be the same file as Pitboard on a case-insensitive volume, and overwrite it.
cp "$cli" "$app/Contents/Helpers/pitboard"
# The plist carries a placeholder version; the crate's is the one that ships.
sed "s/>0\.0\.0</>$version</" apple/Resources/Info.plist > "$app/Contents/Info.plist"
printf 'APPL????' > "$app/Contents/PkgInfo"
# Sparkle decides what is newer by CFBundleVersion, so it counts up with the version
# rather than staying at whatever the template says.
rest=${version#*.}
build=$((${version%%.*} * 10000 + ${rest%%.*} * 100 + ${rest#*.}))
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $build" "$app/Contents/Info.plist"

swift apple/scripts/make-icon.swift apple/build
iconutil --convert icns --output "$app/Contents/Resources/AppIcon.icns" \
    apple/build/AppIcon.iconset

# The man page and completions for whatever links the command line onto PATH, written by
# the build that ships so they describe it, and before signing, since the app's signature
# covers its Resources. It runs from where lipo left it: macOS scans a bundle the first
# time a program inside it runs, and a write into the bundle during the scan fails.
mkdir -p "$app/Contents/Resources/man" "$app/Contents/Resources/completions"
"$cli" manpage > "$app/Contents/Resources/man/pitboard.1"
for shell in bash zsh fish; do
    "$cli" completions "$shell" > "$app/Contents/Resources/completions/pitboard.$shell"
done

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
# shellcheck disable=SC2086
codesign --force $options --sign "$identity" "$app"
codesign --verify --strict --deep "$app"
echo "built $app ($version, $(lipo -archs "$app/Contents/MacOS/Pitboard"))"
