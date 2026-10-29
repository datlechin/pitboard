#!/bin/sh
# test-linux-packages.sh <deb|rpm> <dir>: installs a release's Debian packages, the ones
# build-linux-packages.sh wrote to <dir>, on a fresh Debian 13 and a fresh Ubuntu 26.04, or its
# RPM packages on a fresh Fedora 43, Fedora 44 and openSUSE Tumbleweed, with podman or else
# docker. apt, dnf or zypper runs as root only while packaging/linux's install-test images build.
# The checks run as uid 1000 with fresh homes and no network: the command line says its version,
# the app's launcher and metainfo pass the system's own validators, the Plasma widget is where
# Plasma looks for it, and once the packages are removed, none of their files or folders is left
# that the system did not hold before or another package does not hold. Each system but Debian 13
# has its Plasma before the packages, as a Plasma desktop does. On Debian 13, whose Plasma is 6.3,
# apt must refuse the widget for it, and the test installs the rest.
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
removed() {
  for image in $images; do
    "$engine" image rm "$image" >/dev/null 2>&1 || true
  done
  images=
}
trap 'removed; rm -rf "$context"' EXIT
# Every package, and every package but the widget, for a system whose Plasma is under 6.4.
mkdir -p "$context/all/packages" "$context/app/packages"
cp "$dir"/*."$format" "$context/all/packages/"
cp "$dir"/*."$format" "$context/app/packages/"
rm "$context/app/packages"/pitboard-plasma-widget-*

# Each system's package manager, its image, and its Plasma's package, or `refused` where the
# widget needs a later Plasma.
# shellcheck disable=SC2016 # the container's shell expands them
if [ "$format" = deb ]; then
  packages='pitboard pitboard-app pitboard-archive-keyring'
  listed='dpkg-query -L'
  kept='dpkg-query -W -f "\${db:Status-Status}" "$package" 2>/dev/null | grep -q -v not-installed'
  owned='dpkg-query -S "$path"'
  set -- apt \
    docker.io/library/debian:13@sha256:8c4733a92aacd0bcff3779eadc35a4a5147f36cb20a5fcb56504cbc740212259 \
    refused \
    apt \
    docker.io/library/ubuntu:26.04@sha256:0e73b71479e7bc5f006bca872e25c3cd680dc052a0116f90d63ae363537db0dd \
    plasma-workspace
else
  packages='pitboard pitboard-app'
  listed='rpm -ql'
  kept='rpm -q --quiet "$package"'
  owned='rpm -qf "$path"'
  set -- dnf \
    docker.io/library/fedora:43@sha256:a651ddf48ea28a06ed4e1e6519f51c9f47e7a5a138722ade87369b8fbb7e5b42 \
    plasma-workspace \
    dnf \
    docker.io/library/fedora:44@sha256:43b29f65a41eb9c35e1cd5323e3bdf3b655c2357a9f4f1ff2f9c2798e5045d80 \
    plasma-workspace \
    zypper \
    docker.io/opensuse/tumbleweed@sha256:216f7358df3fabe995e8acb94343c19afc95875c5572617a4996d70149a5e83a \
    plasma6-workspace
fi

# Each path the packages named after the version list, and each folder above one, which an
# install may make. dpkg-query puts an empty line between two packages' lists. KPackage finds a
# widget by its id as the folder of that name under plasma/plasmoids.
# shellcheck disable=SC2016 # the container's shell expands them
installed='set -eu
  mkdir -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME" "$XDG_STATE_HOME"
  said=$(pitboard --version)
  if [ "$said" != "pitboard $1" ]; then
    echo "pitboard --version says $said, not pitboard $1" >&2
    exit 1
  fi
  shift
  desktop-file-validate /usr/share/applications/com.usepitboard.Pitboard.desktop >&2
  appstreamcli validate --no-net /usr/share/metainfo/com.usepitboard.Pitboard.metainfo.xml >&2
  case " $* " in
    *" pitboard-plasma-widget "*)
      widget=/usr/share/plasma/plasmoids/com.usepitboard.Pitboard/metadata.json
      grep -q "\"Id\": \"com.usepitboard.Pitboard\"" "$widget"
      ;;
  esac
  '"$listed"' "$@" | while IFS= read -r path; do
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
# Once the packages named are removed, the paths on stdin that are there and no package holds.
# shellcheck disable=SC2016 # the container's shell expands them
left='set -eu
  for package in "$@"; do
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
  plasma=$3
  shift 3
  distro=${base_image##*/}
  distro=${distro%%@*}
  tag=pitboard-linux-packages-test:$(echo "$distro" | tr : -)
  containerfile=$repo/packaging/linux/install-test-$manager.Containerfile
  echo "test-linux-packages.sh: $distro" >&2
  if [ "$plasma" = refused ]; then
    plasma=
    # The whole build's output, which holds apt's reason.
    if "$engine" build -f "$containerfile" --build-arg "BASE=$base_image" --target installed \
      "$context/all" >"$context/refused" 2>&1; then
      echo "$manager installed pitboard-plasma-widget on $distro, whose Plasma is under 6.4" >&2
      exit 1
    fi
    reason=$(grep -m 1 -o 'pitboard-plasma-widget : Depends: plasma-workspace (>= 4:6.4).*' \
      "$context/refused" || :)
    if [ -z "$reason" ]; then
      cat "$context/refused" >&2
      echo "$manager refused the packages on $distro, but not for the widget's Plasma" >&2
      exit 1
    fi
    echo "test-linux-packages.sh: $manager refuses on $distro: $reason" >&2
    named=$packages
    given=$context/app
  else
    named="$packages pitboard-plasma-widget"
    given=$context/all
  fi
  for stage in system installed removed; do
    images="$images $tag-$stage"
    "$engine" build -f "$containerfile" --build-arg "BASE=$base_image" \
      --build-arg "PLASMA=$plasma" --build-arg "PACKAGES=$named" --target "$stage" \
      -t "$tag-$stage" "$given"
  done
  # shellcheck disable=SC2086 # the package names are words
  paths=$(inside "$tag-installed" "$installed" "$base" $named)
  if [ -z "$paths" ]; then
    echo "$manager lists no file of the packages on $distro" >&2
    exit 1
  fi
  printf '%s\n' "$paths" | inside "$tag-system" "$there" >"$context/before"
  # shellcheck disable=SC2086 # the package names are words
  printf '%s\n' "$paths" | inside "$tag-removed" "$left" $named >"$context/after"
  stayed=$(LC_ALL=C comm -13 "$context/before" "$context/after")
  if [ -n "$stayed" ]; then
    echo "$stayed" | sed "s|\$| is left on $distro after the packages are removed|" >&2
    exit 1
  fi
  # With Plasma, each system's images take a gigabyte or more.
  removed
done
