#!/bin/sh
# Takes the documentation's pictures of the menu and the window from the debug build in the
# twoTools fixture, with the UI test ScreenshotTests, and writes them at the sizes the docs
# use. With --figures <dir>, it also writes every picture the test kept into <dir>, at full
# size and under the name the test gave it, the scenes usepitboard.com's figures are checked
# against among them. Needs what the UI tests need: build-xcframework.sh --fixture, then
# xcodegen. Each test runs in Light, then in Dark, and XCTest leaves this Mac in Dark; the docs
# take the Light pictures.
set -eu

usage() {
    echo "usage: $0 [--figures <dir>]" >&2
    exit 64
}

figures=
case $# in
0) ;;
2)
    [ "$1" = --figures ] || usage
    mkdir -p "$2"
    figures=$(cd "$2" && pwd)
    ;;
*) usage ;;
esac

cd "$(dirname "$0")/../../.."
build=apps/macos/build
debug=$build/UITestData/Build/Products/Debug
results=$build/Screenshots.xcresult
attachments=$(mktemp -d)
lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister

# Building registers the debug build's scheme and Share extension with macOS until undone.
unregister() {
    for appex in "$debug/PitboardShare.appex" \
        "$debug/Pitboard.app/Contents/PlugIns/PitboardShare.appex"; do
        pluginkit -r "$appex" 2>/dev/null || true
    done
    for app in "$debug"/Pitboard.app/Contents/Frameworks/Sparkle.framework/Versions/*/Updater.app \
        "$debug/Pitboard.app" "$debug/PitboardUITests-Runner.app"; do
        if [ -d "$app" ]; then "$lsregister" -u "$app" 2>/dev/null || true; fi
    done
    rm -rf "$attachments"
}
trap unregister EXIT

rm -rf "$results"
xcodebuild test -project apps/macos/Pitboard.xcodeproj -scheme Pitboard \
    -destination 'platform=macOS' -derivedDataPath "$build/UITestData" \
    -clonedSourcePackagesDirPath "$build/SourcePackages" \
    -only-testing PitboardUITests/ScreenshotTests -resultBundlePath "$results"
xcrun xcresulttool export attachments --path "$results" --output-path "$attachments" >/dev/null

# The manifest has an entry for each test, and each names the files its test kept by the
# names the test gave them, followed by "_".
manifest=$attachments/manifest.json
kept=$attachments/kept
t=0
while plutil -extract "$t" raw "$manifest" >/dev/null 2>&1; do
    i=0
    while name=$(plutil -extract "$t.attachments.$i.suggestedHumanReadableName" raw \
        "$manifest" 2>/dev/null); do
        file=$(plutil -extract "$t.attachments.$i.exportedFileName" raw "$manifest")
        echo "${name%%_*} $attachments/$file" >>"$kept"
        i=$((i + 1))
    done
    t=$((t + 1))
done

picture() {
    file=$(awk -v name="$1" '$1 == name { print $2; exit }' "$kept" 2>/dev/null)
    if [ -z "$file" ]; then
        echo "the test kept no picture called $1" >&2
        exit 1
    fi
    echo "$file"
}

menu=$(picture macos-menu-accounts)
window=$(picture macos-window-accounts)
sips --resampleWidth 600 "$menu" --out docs/images/menu.png >/dev/null
cp docs/images/menu.png .github/media/menu.png
sips --resampleWidth 1680 "$window" --out docs/images/window.png >/dev/null

if [ -n "$figures" ]; then
    while read -r name file; do
        cp "$file" "$figures/$name.png"
    done <"$kept"
    echo "Kept $(wc -l <"$kept" | tr -d ' ') pictures in $figures"
fi
