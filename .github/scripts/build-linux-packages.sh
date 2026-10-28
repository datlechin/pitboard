#!/bin/sh
# build-linux-packages.sh deb <command line tarball> <out>: a release's Debian packages, built
# as a user on the Debian it runs on, from the source tar `git archive` writes on stdin and the
# release's pitboard-v<version>-<x86_64|aarch64>-unknown-linux-musl.tar.gz. Each package goes to
# <out> as <package>-v<version>-<architecture>.deb, since GitHub renames a release file whose
# name holds the ~ of a Debian version.
set -eu

usage() {
  echo "usage: build-linux-packages.sh deb <command line tarball> <out>" >&2
  exit 2
}

refuse() {
  echo "$1" >&2
  exit 1
}

[ "$#" -eq 3 ] || usage
[ "$1" = deb ] || usage
tarball=$2
out=$3
here=$(cd "$(dirname "$0")" && pwd)

if [ "$(id -u)" -eq 0 ]; then
  refuse "build-linux-packages.sh builds as a user, never as root"
fi
name=$(basename "$tarball" .tar.gz)
case $name in
  pitboard-v*-x86_64-unknown-linux-musl) machine=x86_64 expected=amd64 ;;
  pitboard-v*-aarch64-unknown-linux-musl) machine=aarch64 expected=arm64 ;;
  *) refuse "$tarball is not a release's command line tarball for Linux" ;;
esac
version=${name#pitboard-v}
version=${version%-"$machine"-unknown-linux-musl}
versions=$(sh "$here/linux-versions.sh" "$version")
deb=$(printf '%s\n' "$versions" | sed -n 's/^deb=//p')
architecture=$(dpkg --print-architecture)
if [ "$architecture" != "$expected" ]; then
  refuse "$tarball holds the command line for $machine, and this is $architecture"
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
source=$work/pitboard
mkdir "$source"
tar -x -C "$source" -f -
if [ ! -d "$source/packaging/linux/debian" ]; then
  refuse "the source on stdin holds no packaging/linux/debian"
fi
cp -R "$source/packaging/linux/debian" "$source/debian"
mkdir "$source/debian/command-line"
tar -xzf "$tarball" -C "$source/debian/command-line" --strip-components=1 "$name"

# git archive gives each file its commit's time, and dpkg-buildpackage takes SOURCE_DATE_EPOCH
# from the changelog's date.
epoch=$(stat -c %Y "$source/Cargo.toml")
maintainer=$(sed -n 's/^Maintainer: //p' "$source/debian/control")
cat >"$source/debian/changelog" <<EOF
pitboard ($deb) stable; urgency=medium

  * Pitboard $version.

 -- $maintainer  $(date -u -R -d "@$epoch")
EOF

(cd "$source" && dpkg-buildpackage -b -us -uc)

mkdir -p "$out"
for package in "$work"/*.deb; do
  named=$(dpkg-deb -f "$package" Package)-v$version-$(dpkg-deb -f "$package" Architecture).deb
  cp "$package" "$out/$named"
done
