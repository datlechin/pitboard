#!/bin/sh
# pack-command-line.sh <binary> <name> <out>: the command line as a release ships it,
# <out>/<name>.tar.gz, one folder <name> holding the binary as pitboard, the man page and the
# bash, zsh and fish completions it writes, and the repository's LICENSE, NOTICE and README.md.
# Whatever packs the command line packs it here, so a Linux package holds the release's file.
set -eu

if [ "$#" -ne 3 ]; then
  echo "usage: pack-command-line.sh <binary> <name> <out>" >&2
  exit 2
fi
binary=$1
name=$2
out=$3
repo=$(cd "$(dirname "$0")/../.." && pwd)

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/$name/completions"
cp "$binary" "$stage/$name/pitboard"
cp "$repo/LICENSE" "$repo/NOTICE" "$repo/README.md" "$stage/$name/"
"$stage/$name/pitboard" manpage >"$stage/$name/pitboard.1"
for shell in bash zsh fish; do
  "$stage/$name/pitboard" completions "$shell" >"$stage/$name/completions/pitboard.$shell"
done
# On a person's Mac files carry com.apple.provenance, which bsdtar packs as ._ entries unless
# COPYFILE_DISABLE is set. GNU tar packs no attributes.
COPYFILE_DISABLE=1 tar -C "$stage" -czf "$stage/$name.tar.gz" "$name"
mkdir -p "$out"
mv "$stage/$name.tar.gz" "$out/"
