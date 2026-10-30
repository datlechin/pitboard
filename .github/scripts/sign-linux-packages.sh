#!/bin/sh
# sign-linux-packages.sh <dir> <public key>: signs each RPM package in <dir> with the key's
# signing subkey, from the GNUPGHOME the caller made, which holds its secret and can use it
# unasked, then checks each against an rpm database of its own that holds that key alone, with
# rpm's level that refuses a package no key in it signed. A Debian package is left as it is: apt
# trusts it through the signed index. It runs on Fedora 43, which built the packages, with
# install-deps.sh's repository set.
set -eu
export LC_ALL=C

usage() {
  echo "usage: sign-linux-packages.sh <dir> <public key>" >&2
  exit 2
}

refuse() {
  echo "$@" >&2
  exit 1
}

[ "$#" -eq 2 ] || usage
dir=$1
key=$2
here=$(cd "$(dirname "$0")" && pwd)
[ -d "$dir" ] || refuse "$dir is not a folder"

# linux-repository-key.sh refuses a key the repository cannot sign with today.
fields=$(sh "$here/linux-repository-key.sh" "$key" "$(date -u +%s)")
signing=$(printf '%s\n' "$fields" | sed -n 's/^signing=//p')

set -- "$dir"/*.rpm
[ -f "$1" ] || refuse "$dir holds no RPM package to sign"

database=$(mktemp -d)
trap 'rm -rf "$database"' EXIT
rpmkeys --dbpath "$database" --import "$key"

for package in "$@"; do
  rpmsign --addsign --define "_gpg_name $signing!" "$package" >&2
done
for package in "$@"; do
  rpmkeys --dbpath "$database" --define '_pkgverify_level signature' --checksig "$package" >&2 ||
    refuse "$package is not signed by $key, so the release's RPM packages are not"
done
