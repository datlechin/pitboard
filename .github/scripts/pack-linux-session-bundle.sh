#!/bin/sh
# pack-linux-session-bundle.sh <folder> <name> <out> <age> <recipient>: the Linux session bundle,
# as pack-session-bundle.ps1 packs Windows'. Every file in <folder> is listed in its SHA256SUMS,
# the folder is packed as <name> in one tar, and <age> encrypts the tar to the recipient as
# <out>/<name>.tar.age. Nothing else is left in <out>. Unless age's file reads as encrypted to an
# X25519 key, nothing is left there at all, and no SHA256SUMS in <folder>.
set -eu
# Bytes, wherever it runs: a-z is the 26 letters, and SHA256SUMS lists the files in byte order.
export LC_ALL=C

usage() {
  echo "usage: pack-linux-session-bundle.sh <folder> <name> <out> <age> <recipient>" >&2
  exit 2
}

refuse() {
  echo "$@" >&2
  exit 1
}

[ "$#" -eq 5 ] || usage
folder=$1
name=$2
out=$3
age=$4
recipient=$5
case ${name#session-build-} in
  "$name" | "" | *[!a-z0-9_-]*)
    refuse "$name is not a session bundle's name, which is session-build- and then a-z, 0-9, _" \
      "or -"
    ;;
esac

[ -d "$folder" ] || refuse "$folder is not a folder"
root=$(cd "$folder" && pwd)
sums=$root/SHA256SUMS
[ ! -e "$sums" ] || refuse "$root holds a SHA256SUMS already"
files=$(cd "$root" && find . -type f | sed 's|^\./||' | sort)
[ -n "$files" ] || refuse "$root holds nothing to pack"
# The session machine's check splits each line at its spaces.
spaced=$(cd "$root" && find . -name '*[[:space:]]*' | sed 's|^\./||')
if [ -n "$spaced" ]; then
  refuse "$(echo "$spaced" | sed 's/$/ has a space in its name, which SHA256SUMS cannot name/')"
fi
sealed=$out/$name.tar.age
[ ! -e "$sealed" ] || refuse "$sealed is there already"

# Neither the sums nor the sealed file was there, so a failure from here on takes both away.
stage=$(mktemp -d)
packed=
trap 'rm -rf "$stage"; [ -n "$packed" ] || rm -f "$sums" "$sealed"' EXIT
(cd "$root" && printf '%s\n' "$files" | while IFS= read -r file; do sha256sum -- "$file"; done) \
  >"$sums"

# tar follows the link, so each name in the tar starts with <name>. On a Mac, COPYFILE_DISABLE
# keeps bsdtar from packing a file's attributes as ._ entries.
ln -s "$root" "$stage/$name"
COPYFILE_DISABLE=1 tar -C "$stage" -chf "$stage/$name.tar" "$name"
mkdir -p "$out"
status=0
"$age" --encrypt --recipient "$recipient" --output "$sealed" "$stage/$name.tar" || status=$?
[ "$status" -eq 0 ] || refuse "age exited $status"
printf 'age-encryption.org/v1\n-> X25519 ' >"$stage/header"
if ! head -c "$(($(wc -c <"$stage/header")))" "$sealed" | cmp -s - "$stage/header"; then
  refuse "age wrote $sealed, which does not read as encrypted to an X25519 key, so nothing is" \
    "handed on"
fi
packed=1
echo "$sealed"
