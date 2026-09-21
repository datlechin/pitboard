#!/bin/sh
# Points the tap at a release. Run it after the release workflow finishes:
#
#   ./packaging/update-tap.sh v0.1.3 [path-to-tap-clone]
#
# It rewrites the formula and cask from the release's own checksums, so a typo in a version
# or a hash is not possible.
set -eu

tag=${1:?usage: update-tap.sh <tag> [tap-directory]}
tap=${2:-$HOME/Workspaces/homebrew-tap}
version=${tag#v}
repo=datlechin/pitboard
cd "$(dirname "$0")/.."

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
curl -fsSL -o "$work/source.tar.gz" "https://github.com/$repo/archive/refs/tags/$tag.tar.gz"
curl -fsSL -o "$work/app.zip" \
    "https://github.com/$repo/releases/download/$tag/Pitboard-$tag-macos.zip"
source_sha=$(shasum -a 256 "$work/source.tar.gz" | cut -d' ' -f1)
app_sha=$(shasum -a 256 "$work/app.zip" | cut -d' ' -f1)

mkdir -p "$tap/Formula" "$tap/Casks"
sed -e "s|/archive/refs/tags/v[0-9.]*\.tar\.gz|/archive/refs/tags/$tag.tar.gz|" \
    -e "s|^  sha256 \".*\"|  sha256 \"$source_sha\"|" \
    packaging/pitboard.rb > "$tap/Formula/pitboard.rb"
sed -e "s|^  version \".*\"|  version \"$version\"|" \
    -e "s|^  sha256 \".*\"|  sha256 \"$app_sha\"|" \
    packaging/pitboard-cask.rb > "$tap/Casks/pitboard.rb"

echo "tap updated to $tag in $tap"
echo "  formula sha256 $source_sha"
echo "  cask    sha256 $app_sha"
