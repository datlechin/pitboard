#!/bin/sh
# build-linux-packages.sh <deb|rpm> <command line tarball> <out>: a release's Debian or RPM
# packages, built as a user on the Debian or Fedora it runs on, from the source tar `git archive`
# writes on stdin and the release's pitboard-v<version>-<x86_64|aarch64>-unknown-linux-musl.tar.gz.
# Each package goes to <out> as <package>-v<version>-<architecture>.<deb|rpm>, since GitHub
# renames a release file whose name holds the ~ of a package version.
set -eu

usage() {
  echo "usage: build-linux-packages.sh <deb|rpm> <command line tarball> <out>" >&2
  exit 2
}

refuse() {
  echo "$1" >&2
  exit 1
}

[ "$#" -eq 3 ] || usage
format=$1
tarball=$2
out=$3
case $format in
  deb | rpm) ;;
  *) usage ;;
esac
here=$(cd "$(dirname "$0")" && pwd)

if [ "$(id -u)" -eq 0 ]; then
  refuse "build-linux-packages.sh builds as a user, never as root"
fi
name=$(basename "$tarball" .tar.gz)
case $name in
  pitboard-v*-x86_64-unknown-linux-musl) machine=x86_64 deb_architecture=amd64 ;;
  pitboard-v*-aarch64-unknown-linux-musl) machine=aarch64 deb_architecture=arm64 ;;
  *) refuse "$tarball is not a release's command line tarball for Linux" ;;
esac
version=${name#pitboard-v}
version=${version%-"$machine"-unknown-linux-musl}
versions=$(sh "$here/linux-versions.sh" "$version")
if [ "$format" = deb ]; then
  expected=$deb_architecture
  architecture=$(dpkg --print-architecture)
else
  expected=$machine
  architecture=$(rpm --eval '%{_arch}')
fi
if [ "$architecture" != "$expected" ]; then
  refuse "$tarball holds the command line for $machine, and this is $architecture"
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$out"

build_deb() {
  deb=$(printf '%s\n' "$versions" | sed -n 's/^deb=//p')
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

  for package in "$work"/*.deb; do
    named=$(dpkg-deb -f "$package" Package)-v$version-$(dpkg-deb -f "$package" Architecture).deb
    cp "$package" "$out/$named"
  done
}

build_rpm() {
  top=$work/rpmbuild
  mkdir -p "$top/SOURCES" "$top/SPECS" "$work/commit"
  cat >"$top/SOURCES/pitboard.tar"
  cp "$tarball" "$top/SOURCES/"
  if ! tar -x -C "$top/SPECS" -f "$top/SOURCES/pitboard.tar" --strip-components=2 \
    packaging/linux/pitboard.spec 2>/dev/null; then
    refuse "the source on stdin holds no packaging/linux/pitboard.spec"
  fi
  # git archive gives each file its commit's time.
  tar -x -C "$work/commit" -f "$top/SOURCES/pitboard.tar" Cargo.toml
  SOURCE_DATE_EPOCH=$(stat -c %Y "$work/commit/Cargo.toml")
  export SOURCE_DATE_EPOCH

  # RPM 6 builds format 6 where a distribution does not say 4, and Tumbleweed's RPM 4.20 reads 4.
  # A release's packages carry no build machine's name or time.
  rpmbuild -bb \
    --define "_topdir $top" \
    --define "_rpmformat 4" \
    --define "_buildhost reproducible" \
    --define "use_source_date_epoch_as_buildtime 1" \
    --define "build_mtime_policy clamp_to_source_date_epoch" \
    --define "pitboard_version $version" \
    --define "pitboard_rpm_version $(printf '%s\n' "$versions" | sed -n 's/^rpm-version=//p')" \
    --define "pitboard_rpm_release $(printf '%s\n' "$versions" | sed -n 's/^rpm-release=//p')" \
    --define "pitboard_date $(date -u -d "@$SOURCE_DATE_EPOCH" '+%a %b %d %Y')" \
    "$top/SPECS/pitboard.spec"

  for package in "$top"/RPMS/*/*.rpm; do
    named=$(rpm -qp --qf '%{NAME}-v'"$version"'-%{ARCH}.rpm' "$package")
    cp "$package" "$out/$named"
  done
}

"build_$format"
