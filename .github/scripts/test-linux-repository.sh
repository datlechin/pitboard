#!/bin/sh
# test-linux-repository.sh <dir>: tests the repository linux-repository.sh builds from the
# release's Debian and RPM packages build-linux-packages.sh wrote to <dir>, with podman or else
# docker. First test-linux-packages.py's repository cases run in the repository's own image.
# Then packaging/linux/repository-test.Containerfile builds and signs the trees the test serves,
# with keys it makes and nothing outside the build holds, and a fresh Debian 13, Ubuntu 26.04,
# Fedora 43 and openSUSE Tumbleweed each set the repository up from them, while their images
# build, as the docs set up packages.usepitboard.com with only its address changed:
#
# - apt, dnf and zypper check the signatures and install pitboard-app from it, apt with
#   pitboard-archive-keyring by Recommends, and the catalog they stored holds the app;
# - each keeps the app it installed from indices another key signed, which offer a later one;
# - a beta channel takes the final release above its beta, which on apt brings the keyring with
#   the signing subkey extended, and apt updates with it after. What rpm keeps of the key once
#   the extended one signs is printed.
#
# Pitboard's programs run in the images only as uid 1000, with fresh homes and no network.
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: test-linux-repository.sh <dir>" >&2
  exit 2
fi
dir=$1
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)

if [ "$(id -u)" -eq 0 ]; then
  echo "test-linux-repository.sh runs as a user: only the package managers, in an image's build," \
    "run as root" >&2
  exit 1
fi
if command -v podman >/dev/null 2>&1; then
  engine=podman
elif command -v docker >/dev/null 2>&1; then
  engine=docker
else
  echo "test-linux-repository.sh builds the repository with podman or docker, and finds" \
    "neither" >&2
  exit 1
fi

set -- "$dir"/pitboard-v*.deb
if [ "$#" -ne 1 ] || [ ! -f "$1" ]; then
  echo "$dir holds no single pitboard-v<version>-<architecture>.deb" >&2
  exit 1
fi
version=${1##*/pitboard-v}
version=${version%-*.deb}
deb=$(printf '%s\n' "$version" | tr - '~')
# The suite the packages given go in, a beta's or a release candidate's never stable, and the
# test's own releases: a final above them, and a beta of it.
case $deb in
  *~*)
    suite=beta setup=pitboard-beta
    final=${deb%%~*}
    ;;
  *)
    suite=stable setup=pitboard
    final=$(printf '%s\n' "$deb" | awk -F. '{ print $1 "." $2 "." $3 + 1 }')
    ;;
esac
beta=$final~beta.1

ubuntu=docker.io/library/ubuntu:26.04@sha256:0e73b71479e7bc5f006bca872e25c3cd680dc052a0116f90d63ae363537db0dd
fedora=docker.io/library/fedora:43@sha256:a651ddf48ea28a06ed4e1e6519f51c9f47e7a5a138722ade87369b8fbb7e5b42

context=$(mktemp -d)
images=
cleanup() {
  for image in $images; do
    "$engine" image rm "$image" >/dev/null 2>&1 || true
  done
  rm -rf "$context"
}
trap cleanup EXIT
mkdir "$context/packages" "$context/repo"
cp "$dir"/*.deb "$dir"/*.rpm "$context/packages/"
# What the images' steps run, as the repository lays it out.
(cd "$repo" && tar -cf "$context/repo.tar" apps/linux/scripts/install-deps.sh \
  .github/scripts/linux-repository.sh .github/scripts/sign-linux-repository.sh \
  .github/scripts/sign-linux-packages.sh .github/scripts/linux-repository-key.sh \
  packaging/linux/repository packaging/linux/repository-test.sh packaging/linux/pitboard*.sources \
  packaging/linux/pitboard.pref packaging/linux/pitboard*.repo)
tar -xf "$context/repo.tar" -C "$context/repo"

# build <tag> <containerfile> <target> [<build-arg>...]: the image, removed at the end.
build() {
  tag=pitboard-linux-repository-test:$1
  containerfile=$repo/packaging/linux/$2
  target=$3
  shift 3
  for argument in "$@"; do
    set -- "$@" --build-arg "$argument"
    shift
  done
  images="$images $tag"
  "$engine" build -f "$containerfile" --target "$target" -t "$tag" "$@" "$context" >&2
}

# inside <image> <script> [argument...]: the script in a fresh container of the image, as uid
# 1000, with no network and homes of its own, and the repository read only at /src.
inside() {
  image=$1
  script=$2
  shift 2
  "$engine" run --rm --user 1000:1000 --network none \
    --mount "type=bind,src=$repo,dst=/src,readonly" --workdir /src \
    --env HOME=/tmp/home --env XDG_CONFIG_HOME=/tmp/home/.config \
    --env XDG_DATA_HOME=/tmp/home/.local/share --env XDG_CACHE_HOME=/tmp/home/.cache \
    --env XDG_STATE_HOME=/tmp/home/.local/state \
    "pitboard-linux-repository-test:$image" sh -c "mkdir -p \"\$HOME\"; $script" sh "$@"
}

echo "test-linux-repository.sh: the repository's cases" >&2
build tools repository-test.Containerfile tools "UBUNTU=$ubuntu" "FEDORA=$fedora"
inside tools 'TEST_LINUX_PACKAGES_TOOLS="dpkg-deb rpmbuild apt-ftparchive createrepo_c
  modifyrepo_c appstream-generator appstreamcli jq gpg gpgv sqv" \
  python3 .github/scripts/test-linux-packages.py Repository'

echo "test-linux-repository.sh: the trees" >&2
build trees repository-test.Containerfile trees "UBUNTU=$ubuntu" "FEDORA=$fedora" \
  "SUITE=$suite" "BETA=$beta" "FINAL=$final"

# The checks once an image has set the repository up, which the image's own files hold, as uid
# 1000. The package manager's version of pitboard-app is the release's.
# shellcheck disable=SC2016 # the container's shell expands them
installed_apt='set -eu
  said=$(dpkg-query -W -f "\${Version} \${db:Status-Status}" pitboard-app)
  [ "$said" = "$1 installed" ] || {
    echo "dpkg has pitboard-app $said, not $1 installed" >&2
    exit 1
  }'
# shellcheck disable=SC2016 # the container's shell expands them
recommended='set -eu
  said=$(dpkg-query -W -f "\${db:Status-Status}" pitboard-archive-keyring 2>/dev/null) || :
  [ "$said" = installed ] || {
    echo "apt did not bring pitboard-archive-keyring with pitboard-app" >&2
    exit 1
  }'
# shellcheck disable=SC2016 # the container's shell expands them
installed_rpm='set -eu
  said=$(rpm -q --qf "%{VERSION}-%{RELEASE}" pitboard-app)
  [ "$said" = "$1-1" ] || { echo "rpm has pitboard-app $said, not $1-1" >&2; exit 1; }'
# The app in the catalog the system stored, with its package and its icon, as a software centre
# reads it, from a cache made afresh rather than one the image's build left.
# shellcheck disable=SC2016 # the container's shell expands them
catalog='set -eu
  details=$(appstreamcli get --no-cache --details com.usepitboard.Pitboard)
  printf "%s\n" "$details" >&2
  printf "%s\n" "$details" | grep -q "^Package: *pitboard-app$" || {
    echo "the catalog does not give pitboard-app as com.usepitboard.Pitboard'\''s package" >&2
    exit 1
  }
  printf "%s\n" "$details" | grep -q "^Icon: " || {
    echo "the catalog gives com.usepitboard.Pitboard no icon" >&2
    exit 1
  }'
# shellcheck disable=SC2016 # the container's shell expands them
followed_apt='set -eu
  cmp -s /usr/share/keyrings/pitboard-archive-keyring.pgp /srv/final/pitboard-archive-keyring.pgp || {
    echo "the keyring is not the final release'\''s, with the subkey extended" >&2
    exit 1
  }'
# What rpm keeps of the test's key: its signing subkey's expiry, as gpg reads the key rpm stored.
# shellcheck disable=SC2016 # the container's shell expands them
kept='set -eu
  for key in $(rpm -q gpg-pubkey); do
    rpm -q --qf "%{DESCRIPTION}" "$key" >/tmp/key.asc
    listed=$(gpg --batch --quiet --with-colons --show-keys /tmp/key.asc 2>/dev/null) || continue
    case $listed in
      *":Pitboard test <test@example.com>:"*) ;;
      *) continue ;;
    esac
    expires=$(printf "%s\n" "$listed" | grep "^sub:" | cut -d : -f 7)
    echo "$key: its signing subkey expires $(date -u -d "@$expires" +%F)"
  done'

set -- apt "docker.io/library/debian:13@sha256:8c4733a92aacd0bcff3779eadc35a4a5147f36cb20a5fcb56504cbc740212259" \
  apt "docker.io/library/ubuntu:26.04@sha256:0e73b71479e7bc5f006bca872e25c3cd680dc052a0116f90d63ae363537db0dd" \
  dnf "$fedora" \
  zypper "docker.io/opensuse/tumbleweed@sha256:216f7358df3fabe995e8acb94343c19afc95875c5572617a4996d70149a5e83a"
while [ "$#" -gt 0 ]; do
  manager=$1
  base=$2
  shift 2
  distro=${base##*/}
  distro=$(echo "${distro%%@*}" | tr : -)
  echo "test-linux-repository.sh: $distro" >&2
  for stage in installed refused followed; do
    build "$distro-$stage" "repository-test-$manager.Containerfile" "$stage" \
      "TREES=pitboard-linux-repository-test:trees" "BASE=$base" "SETUP=$setup"
  done
  if [ "$manager" = apt ]; then
    inside "$distro-installed" "$installed_apt" "$deb"
    inside "$distro-installed" "$recommended"
    inside "$distro-refused" "$installed_apt" "$deb"
    inside "$distro-followed" "$installed_apt" "$final"
    inside "$distro-followed" "$followed_apt"
  else
    inside "$distro-installed" "$installed_rpm" "$deb"
    inside "$distro-refused" "$installed_rpm" "$deb"
    inside "$distro-followed" "$installed_rpm" "$final"
    inside "$distro-installed" "$kept" | sed "s/^/$distro, before: /" >&2
    inside "$distro-followed" "$kept" | sed "s/^/$distro, after the extended key's release: /" >&2
  fi
  inside "$distro-installed" "$catalog"
done
