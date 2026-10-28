#!/bin/sh
# test-linux-packages.sh <dir>: installs a release's Debian packages, the ones
# build-linux-packages.sh wrote to <dir>, on a fresh Debian 13 and a fresh Ubuntu 26.04, with
# podman or else docker. apt runs as root only while packaging/linux/install-test.Containerfile's
# images build. The checks run as uid 1000 with fresh homes and no network: the command line says
# its version, the app's launcher and metainfo pass the system's own validators, and once the
# packages are purged, none of their files is left.
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: test-linux-packages.sh <dir>" >&2
  exit 2
fi
dir=$1
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)

if [ "$(id -u)" -eq 0 ]; then
  echo "test-linux-packages.sh runs as a user: only apt, in an image's build, runs as root" >&2
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

set -- "$dir"/pitboard-v*.deb
if [ "$#" -ne 1 ] || [ ! -f "$1" ]; then
  echo "$dir holds no single pitboard-v<version>-<architecture>.deb" >&2
  exit 1
fi
version=${1##*/pitboard-v}
version=${version%-*.deb}
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
cp "$dir"/*.deb "$context/"

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
  dpkg-query -L pitboard pitboard-app pitboard-archive-keyring |
    while IFS= read -r path; do [ -d "$path" ] || printf "%s\n" "$path"; done'
# shellcheck disable=SC2016 # the container's shell expands them
removed='set -eu
  for package in pitboard pitboard-app pitboard-archive-keyring; do
    if dpkg-query -W -f "\${db:Status-Status}" "$package" 2>/dev/null | grep -q -v not-installed
    then
      echo "$package is still installed" >&2
      exit 1
    fi
  done
  left=0
  while IFS= read -r path; do
    if [ -e "$path" ] || [ -L "$path" ]; then
      echo "$path is left after the purge" >&2
      left=1
    fi
  done
  exit "$left"'

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

for base_image in \
  docker.io/library/debian:13@sha256:8c4733a92aacd0bcff3779eadc35a4a5147f36cb20a5fcb56504cbc740212259 \
  docker.io/library/ubuntu:26.04@sha256:0e73b71479e7bc5f006bca872e25c3cd680dc052a0116f90d63ae363537db0dd; do
  distro=${base_image##*/}
  distro=${distro%%@*}
  tag=pitboard-linux-packages-test:$(echo "$distro" | tr : -)
  echo "test-linux-packages.sh: $distro" >&2
  for stage in installed removed; do
    images="$images $tag-$stage"
    "$engine" build -f "$repo/packaging/linux/install-test.Containerfile" \
      --build-arg "BASE=$base_image" --target "$stage" -t "$tag-$stage" "$context"
  done
  files=$(inside "$tag-installed" "$installed" "$base")
  if [ -z "$files" ]; then
    echo "dpkg lists no file of the packages on $distro" >&2
    exit 1
  fi
  printf '%s\n' "$files" | inside "$tag-removed" "$removed"
done
