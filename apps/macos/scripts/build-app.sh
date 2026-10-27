#!/bin/sh
# Builds Pitboard.app, universal, from the Xcode project XcodeGen generates out of
# project.yml and the core's XCFramework, with the command line inside it. This is the one
# way the app is built: here, in CI and in a release.
#
# Signing: set SIGN_IDENTITY to a Developer ID Application identity to make a build others
# can run; without it the app is signed ad-hoc, which is enough on the machine that built it.
#
# Updates: set SPARKLE_PUBLIC_KEY to the EdDSA public key that signs the appcast, and
# SPARKLE_FEED_URL to override where it is read from. Without the key the app ships with no
# updater, which is what a build from a clone wants.
set -eu

cd "$(dirname "$0")/../../.."
export MACOSX_DEPLOYMENT_TARGET=14.0
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[ -n "$version" ] || { echo "no version in Cargo.toml" >&2; exit 1; }
identity=${SIGN_IDENTITY:--}
# GitHub serves the newest release's assets at a fixed address, so the feed has one even
# before the site does.
FEED=https://github.com/datlechin/pitboard/releases/latest/download/appcast.xml
app=apps/macos/build/Pitboard.app
# Sparkle decides what is newer by CFBundleVersion, so it counts up with the version
# rather than staying at whatever the project says.
build=$(./apps/macos/scripts/build-number.sh "$version")
# Apple documents CFBundleShortVersionString as three integers, so a beta's is its X.Y.Z
# alone. The app shows the whole version, from PitboardVersion.
short=${version%%-*}

# This clears apps/macos/build and makes it again, so nothing from a previous build survives.
# Never with --fixture: a fixture is a world for a debug build and the UI tests to launch
# into, and nothing that ships may hold one.
./apps/macos/scripts/build-xcframework.sh

# A fixture world's text, which only a core built with the `fixture` feature holds: a word
# of its stand-in pages and an account of its worlds; and the variable only a build for tests
# reads. Looked for in the core's library here, in the app once it is built, which links that
# library, and in the command line the app carries.
holds_a_fixture() {
    LC_ALL=C grep -a -q -e 'A Pitboard fixture page' -e 'dana@work.example' \
        -e 'PITBOARD_TEST_CLAUDE_DESKTOP' "$1"
}

# The core's SQLite has to be the system's: a pitboard that links Homebrew's needs Homebrew.
links_the_systems_sqlite() {
    for arch in arm64 x86_64; do
        linked=$(otool -arch "$arch" -L "$1" | awk 'tolower($1) ~ /sqlite/ {print $1}')
        [ "$linked" = /usr/lib/libsqlite3.dylib ] || {
            echo "$2 links a sqlite other than the system's" >&2
            exit 1
        }
    done
}
core_library=apps/macos/build/PitboardFFI/libpitboard_ffi.a
[ -s "$core_library" ] || {
    echo "there is no $core_library to look in" >&2
    exit 1
}
if holds_a_fixture "$core_library"; then
    echo "$core_library holds a fixture: it was built with --fixture" >&2
    exit 1
fi

# The project is generated, never edited, so the one built is the one project.yml says.
command -v xcodegen >/dev/null || {
    echo "XcodeGen generates the Xcode project: install it with \`brew install xcodegen\`" >&2
    exit 1
}
xcodegen generate --spec apps/macos/project.yml --quiet

# Unsigned, because the bundle is not finished: the command line and the man pages go in
# after this, and a signature covers what is inside it. Sparkle is the version and revision
# the committed Package.resolved pins or the build stops, since the app's bill of materials
# reads it from there. Packages are cloned under apps/macos/build, where the release finds
# Sparkle's tools.
xcodebuild -project apps/macos/Pitboard.xcodeproj -scheme Pitboard -configuration Release \
    -destination 'generic/platform=macOS' \
    -derivedDataPath apps/macos/build/DerivedData \
    -clonedSourcePackagesDirPath apps/macos/build/SourcePackages \
    -onlyUsePackageVersionsFromResolvedFile -quiet \
    ARCHS="arm64 x86_64" ONLY_ACTIVE_ARCH=NO \
    MARKETING_VERSION="$short" CURRENT_PROJECT_VERSION="$build" PITBOARD_VERSION="$version" \
    CODE_SIGNING_ALLOWED=NO build
ditto apps/macos/build/DerivedData/Build/Products/Release/Pitboard.app "$app"
sparkle=$app/Contents/Frameworks/Sparkle.framework
[ -d "$sparkle" ] || {
    echo "Xcode did not embed Sparkle.framework" >&2
    exit 1
}
# Sparkle's XPC services are for a sandboxed app. Pitboard is not sandboxed and sets neither
# SUEnableInstallerLauncherService nor SUEnableDownloaderService, so Sparkle downloads and
# starts its installer in process, and its sandboxing page says such an app may remove them.
rm -rf "$sparkle/XPCServices" "$sparkle/Versions/B/XPCServices"
# The Share extension is universal like the app, and a release names pitboard:// in every
# place the app and the extension read it from.
appex=$app/Contents/PlugIns/PitboardShare.appex
[ -d "$appex" ] || {
    echo "Xcode did not embed PitboardShare.appex" >&2
    exit 1
}
for arch in arm64 x86_64; do
    lipo "$appex/Contents/MacOS/PitboardShare" -verify_arch "$arch"
done
for scheme in \
    "$(plutil -extract CFBundleURLTypes.0.CFBundleURLSchemes.0 raw "$app/Contents/Info.plist")" \
    "$(plutil -extract PitboardURLScheme raw "$app/Contents/Info.plist")" \
    "$(plutil -extract PitboardURLScheme raw "$appex/Contents/Info.plist")"; do
    [ "$scheme" = pitboard ] || {
        echo "the app or its extension names $scheme://, not pitboard://" >&2
        exit 1
    }
done
for bundle in "$app" "$appex"; do
    said=$(plutil -extract CFBundleDisplayName raw "$bundle/Contents/Info.plist")
    [ "$said" = Pitboard ] || {
        echo "$bundle is named $said, not Pitboard" >&2
        exit 1
    }
done
for bundle in "$app" "$appex"; do
    said=$(plutil -extract CFBundleShortVersionString raw "$bundle/Contents/Info.plist")
    [ "$said" = "$short" ] || {
        echo "$bundle says it is $said, not $short" >&2
        exit 1
    }
done
said=$(plutil -extract PitboardVersion raw "$app/Contents/Info.plist" || true)
[ "$said" = "$version" ] || {
    echo "the app shows ${said:-no version}, not $version" >&2
    exit 1
}
# Xcode compiles AppIcon.icon into Assets.car for macOS 26 and later, and into AppIcon.icns
# for earlier systems, in the app and in the extension, whose own icon the Share menu shows.
for bundle in "$app" "$appex"; do
    [ -s "$bundle/Contents/Resources/AppIcon.icns" ] || {
        echo "Xcode did not compile AppIcon.icon into $bundle" >&2
        exit 1
    }
    [ "$(plutil -extract CFBundleIconFile raw "$bundle/Contents/Info.plist")" = AppIcon ] || {
        echo "$bundle/Contents/Info.plist does not name AppIcon.icns" >&2
        exit 1
    }
done

links_the_systems_sqlite "$app/Contents/MacOS/Pitboard" "the app"

# The SDK's SQLite. libsqlite3-sys asks pkg-config even where SQLITE3_LIB_DIR is set, and
# takes another SQLite it knows, unless SQLITE3_NO_PKG_CONFIG is set too.
# links_the_systems_sqlite stops a build that links another all the same.
SQLITE3_LIB_DIR="$(xcrun --show-sdk-path)/usr/lib"
SQLITE3_NO_PKG_CONFIG=1
export SQLITE3_LIB_DIR SQLITE3_NO_PKG_CONFIG
# The command line comes inside the app, so one update moves both, and the renewal
# schedule has a Pitboard to run: the app has no renewal of its own.
for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --locked --release -p pitboard --target "$target"
done
cli=apps/macos/build/cli-universal
lipo -create target/aarch64-apple-darwin/release/pitboard \
    target/x86_64-apple-darwin/release/pitboard -output "$cli"
links_the_systems_sqlite "$cli" "the pitboard in the app"
if holds_a_fixture "$cli"; then
    echo "the pitboard in the app is a build for tests" >&2
    exit 1
fi
# Helpers, which is where a bundle keeps a tool that is not its main program. In MacOS it
# would be the same file as Pitboard on a case-insensitive volume, and overwrite it.
mkdir -p "$app/Contents/Helpers"
cp "$cli" "$app/Contents/Helpers/pitboard"

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
for helper in "$sparkle/Versions/B/Updater.app" "$sparkle/Versions/B/Autoupdate"; do
    # shellcheck disable=SC2086 # $options is a list of flags.
    codesign --force $options --sign "$identity" "$helper"
done
# shellcheck disable=SC2086 # $options is a list of flags.
codesign --force $options --sign "$identity" "$sparkle"
# An app extension must be sandboxed, or macOS refuses to run it, and a signature made
# without --entitlements carries none. It is signed before the app, and the app's own
# signature is made without --deep, so it keeps this one. The key is written with its dots
# escaped, which plutil otherwise reads as a path of four keys.
# shellcheck disable=SC2086 # $options is a list of flags.
codesign --force $options --sign "$identity" \
    --entitlements apps/macos/ShareExtension/PitboardShare.entitlements "$appex"
sandboxed=$(codesign -d --entitlements - --xml "$appex" 2>/dev/null |
    plutil -extract 'com\.apple\.security\.app-sandbox' raw - 2>/dev/null || true)
[ "$sandboxed" = true ] || {
    echo "the share extension is not sandboxed" >&2
    exit 1
}
# The extension checks a link with pitboard-share-ffi and links nothing of the core, whose
# bindings would bring every export in. The app links the core and not the extension's
# library: each Rust static library carries its own copy of Rust's standard library, and two
# must never meet in one binary. Each binary must also hold the library it links, so one
# with no symbols to read fails here rather than passing.
links() {
    nm -a "$1" | grep -q "$2"
}
extension_binary=$appex/Contents/MacOS/PitboardShare
app_binary=$app/Contents/MacOS/Pitboard
links "$extension_binary" uniffi_pitboard_share_ffi_ || {
    echo "the share extension does not link pitboard-share-ffi" >&2
    exit 1
}
if links "$extension_binary" uniffi_pitboard_ffi_; then
    echo "the share extension links the core" >&2
    exit 1
fi
links "$app_binary" uniffi_pitboard_ffi_ || {
    echo "the app does not link the core" >&2
    exit 1
}
if links "$app_binary" uniffi_pitboard_share_ffi_; then
    echo "the app links the share extension's library" >&2
    exit 1
fi
if holds_a_fixture "$app_binary"; then
    echo "the app holds a fixture" >&2
    exit 1
fi
# shellcheck disable=SC2086
codesign --force $options --sign "$identity" "$app"
codesign --verify --strict --deep "$app"
echo "built $app ($version, $(lipo -archs "$app/Contents/MacOS/Pitboard"))"
