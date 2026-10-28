#!/bin/sh
# test-linux-packages.sh <deb|rpm> <dir>: installs a release's Debian packages, the ones
# build-linux-packages.sh wrote to <dir>, on a fresh Debian 13 and a fresh Ubuntu 26.04, or its
# RPM packages on a fresh Fedora 43, Fedora 44 and openSUSE Tumbleweed, with podman or else
# docker. apt, dnf or zypper runs as root only while packaging/linux's install-test images build.
# The checks run as uid 1000 with fresh homes and no network: the command line says its version,
# the app's launcher and metainfo pass the system's own validators, and once the packages are
# removed, none of their files or folders is left that the system did not hold before or another
# package does not hold.
set -eu

if [ "$#" -ne 2 ] || { [ "$1" != deb ] && [ "$1" != rpm ]; }; then
  echo "usage: test-linux-packages.sh <deb|rpm> <dir>" >&2
  exit 2
fi
format=$1
dir=$2
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)

if [ "$(id -u)" -eq 0 ]; then
  echo "test-linux-packages.sh runs as a user: only the package manager, in an image's build," \
    "runs as root" >&2
  exit 1
fi
if command -v podman >/dev/null 2>&1; then
  engine=podman
elif command -v docker >/dev/null 2>&1; then
  engine=docker
else
  echo "test-linux-packages.sh installs the packages with podman or docker, and finds neither" >&2
  exit 1
fi

set -- "$dir"/pitboard-v*."$format"
if [ "$#" -ne 1 ] || [ ! -f "$1" ]; then
  echo "$dir holds no single pitboard-v<version>-<architecture>.$format" >&2
  exit 1
fi
version=${1##*/pitboard-v}
version=${version%-*."$format"}
# The command line says its manifest's version, which a release candidate's tag goes beyond.
base=${version%-rc*}

context=$(mktemp -d)
images=
cleanup() {
  for image in $images; do
    "$engine" image rm "$image" >/dev/null 2>&1 || true
  done
  rm -rf "$context"
}
trap cleanup EXIT
mkdir "$context/packages"
cp "$dir"/*."$format" "$context/packages/"

# shellcheck disable=SC2016 # the container's shell expands them
if [ "$format" = deb ]; then
  packages='pitboard pitboard-app pitboard-archive-keyring'
  list="dpkg-query -L $packages"
  kept='dpkg-query -W -f "\${db:Status-Status}" "$package" 2>/dev/null | grep -q -v not-installed'
  owned='dpkg-query -S "$path"'
  set -- apt \
    docker.io/library/debian:13@sha256:8c4733a92aacd0bcff3779eadc35a4a5147f36cb20a5fcb56504cbc740212259 \
    apt \
    docker.io/library/ubuntu:26.04@sha256:0e73b71479e7bc5f006bca872e25c3cd680dc052a0116f90d63ae363537db0dd
else
  packages='pitboard pitboard-app'
  list="rpm -ql $packages"
  kept='rpm -q --quiet "$package"'
  owned='rpm -qf "$path"'
  set -- dnf \
    docker.io/library/fedora:43@sha256:a651ddf48ea28a06ed4e1e6519f51c9f47e7a5a138722ade87369b8fbb7e5b42 \
    dnf \
    docker.io/library/fedora:44@sha256:43b29f65a41eb9c35e1cd5323e3bdf3b655c2357a9f4f1ff2f9c2798e5045d80 \
    zypper \
    docker.io/opensuse/tumbleweed@sha256:216f7358df3fabe995e8acb94343c19afc95875c5572617a4996d70149a5e83a
fi

# Each path the packages list, and each folder above one, which an install may make. dpkg-query
# puts an empty line between two packages' lists.
# shellcheck disable=SC2016 # the container's shell expands them
installed='set -eu
  mkdir -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME" "$XDG_STATE_HOME"
  said=$(pitboard --version)
  if [ "$said" != "pitboard $1" ]; then
    echo "pitboard --version says $said, not pitboard $1" >&2
    exit 1
  fi
  desktop-file-validate /usr/share/applications/com.usepitboard.Pitboard.desktop >&2
  appstreamcli validate --no-net /usr/share/metainfo/com.usepitboard.Pitboard.metainfo.xml >&2
  '"$list"' | while IFS= read -r path; do
    [ -n "$path" ] || continue
    while [ "$path" != / ]; do
      printf "%s\n" "$path"
      path=$(dirname "$path")
    done
  done | LC_ALL=C sort -u'
# Of the paths on stdin, those that are there.
# shellcheck disable=SC2016 # the container's shell expands them
there='set -eu
  while IFS= read -r path; do
    if [ -e "$path" ] || [ -L "$path" ]; then
      printf "%s\n" "$path"
    fi
  done'
# Once the packages are removed, the paths on stdin that are there and no package holds.
# shellcheck disable=SC2016 # the container's shell expands them
left='set -eu
  for package in '"$packages"'; do
    if '"$kept"'; then
      echo "$package is still installed" >&2
      exit 1
    fi
  done
  while IFS= read -r path; do
    if { [ -e "$path" ] || [ -L "$path" ]; } && ! '"$owned"' >/dev/null 2>&1; then
      printf "%s\n" "$path"
    fi
  done'

# inside <image> <script> [argument...]: the script in a fresh container of the image, as uid
# 1000, with no network and homes of its own.
inside() {
  image=$1
  script=$2
  shift 2
  "$engine" run --rm -i --user 1000:1000 --network none \
    --env HOME=/tmp/home --env XDG_CONFIG_HOME=/tmp/home/.config \
    --env XDG_DATA_HOME=/tmp/home/.local/share --env XDG_CACHE_HOME=/tmp/home/.cache \
    --env XDG_STATE_HOME=/tmp/home/.local/state \
    "$image" sh -c "$script" sh "$@"
}

while [ "$#" -gt 0 ]; do
  manager=$1
  base_image=$2
  shift 2
  distro=${base_image##*/}
  distro=${distro%%@*}
  tag=pitboard-linux-packages-test:$(echo "$distro" | tr : -)
  echo "test-linux-packages.sh: $distro" >&2
  for stage in system installed removed; do
    images="$images $tag-$stage"
    "$engine" build -f "$repo/packaging/linux/install-test-$manager.Containerfile" \
      --build-arg "BASE=$base_image" --target "$stage" -t "$tag-$stage" "$context"
  done
  paths=$(inside "$tag-installed" "$installed" "$base")
  if [ -z "$paths" ]; then
    echo "$manager lists no file of the packages on $distro" >&2
    exit 1
  fi
  printf '%s\n' "$paths" | inside "$tag-system" "$there" >"$context/before"
  printf '%s\n' "$paths" | inside "$tag-removed" "$left" >"$context/after"
  stayed=$(LC_ALL=C comm -13 "$context/before" "$context/after")
  if [ -n "$stayed" ]; then
    echo "$stayed" | sed "s|\$| is left on $distro after the packages are removed|" >&2
    exit 1
  fi
done
