#!/bin/sh
# install-deps.sh <ubuntu-26.04|debian-13|fedora-43|fedora-44> [packaging | repository]: as
# root, on a fresh image of the distribution named, what the Linux app's build, tests and checks
# take: the distribution's packages, the desktop's own fonts, rustup and the Cargo tools CI runs,
# and with `packaging`, what its packages' build and checks take beside. With `repository`, only
# what the repository takes and nothing of the app's: on Ubuntu 26.04 what builds, signs and
# checks it, on Fedora 43 what signs the RPM packages. An image's build and CI's containers run
# it. It changes the system it runs on, so never a machine of your own.
#
# Debian's and Ubuntu's packages come from one snapshot of their archives, so a run installs
# what the last change here was checked against. Fedora keeps none, so its packages are the
# release's and its updates' of the day (docs/facts/linux-app.md).
set -eu

snapshot=20261028T000000Z
rustup=1.29.2
# As ci.yml's other jobs install them.
nextest=0.9.146
hack=0.6.45

usage() {
  echo "usage: install-deps.sh <ubuntu-26.04|debian-13|fedora-43|fedora-44>" \
    "[packaging | repository]" >&2
  exit 2
}

[ "$#" -ge 1 ] && [ "$#" -le 2 ] || usage
distro=$1
kind=${2-}
case $kind in
  "" | packaging | repository) ;;
  *) usage ;;
esac
if [ "$(id -u)" -ne 0 ]; then
  echo "install-deps.sh installs packages, so it runs as root, in an image or a container" >&2
  exit 1
fi

# GVfs, as GNOME desktops run it: it changes the files GIO makes of links. util-linux for
# setpriv, which CI's jobs run the tests through.
dnf_packages='ca-certificates curl gcc pkgconf-pkg-config python3 gtk4-devel gtk4-devel-tools
  libadwaita-devel glib2-devel meson ninja-build desktop-file-utils appstream dbus-daemon
  at-spi2-core mutter mesa-dri-drivers mesa-libEGL librsvg2-tools adwaita-icon-theme
  hicolor-icon-theme adwaita-sans-fonts adwaita-mono-fonts util-linux gvfs'
apt_packages='curl gcc libc6-dev pkg-config python3 libgtk-4-dev libgtk-4-bin libadwaita-1-dev
  libglib2.0-dev-bin meson ninja-build desktop-file-utils appstream dbus-daemon at-spi2-core
  mutter libgl1-mesa-dri libegl-mesa0 librsvg2-bin librsvg2-common adwaita-icon-theme
  hicolor-icon-theme gvfs'
# With neither, on Fedora 44: Plasma's QML modules, Qt's qmllint and qmltestrunner, PyGObject
# for the stand-in for the app, and Breeze's icons and colour schemes, for the Plasma widget's
# checks.
#
# With `packaging`: on Debian 13, build-linux-packages.sh's debhelper and its completions add-on,
# musl for the command line and gpg for the keyring; check-linux-packages.sh's lintian; and rpm,
# whose comparison of versions test-release.py checks. On Fedora 43, rpmbuild and rpmlint, and
# rpmsign with gpg, which sign the RPM packages.
#
# With `repository`, on Ubuntu 26.04, whose appstream-generator alone of the floor's runs its
# rpmmd backend (docs/facts/linux-packages.md): apt-ftparchive, createrepo_c and
# appstream-generator, jq for its hints and librsvg's loader for the app's SVG icon; rpm to read
# the RPM packages; gpg to sign and sqv and gpgv to check; and appstream and python3 for
# test-linux-packages.py's cases of the repository. On Fedora 43, rpmsign with gpg.
extra=
case $distro:$kind in
  fedora-44:)
    extra='plasma-workspace libplasma kf6-kirigami qt6-qtdeclarative-devel python3-gobject-base
      breeze-icon-theme kf6-qqc2-desktop-style'
    ;;
  *:) ;;
  debian-13:packaging)
    extra='build-essential debhelper dh-shell-completions dpkg-dev lintian gnupg musl-tools rpm'
    ;;
  fedora-43:packaging) extra='rpm-build rpmlint rpm-sign gnupg2' ;;
  ubuntu-26.04:repository)
    apt_packages=
    extra='apt-utils createrepo-c appstream-generator jq librsvg2-common rpm gnupg sqv gpgv
      appstream python3'
    ;;
  fedora-43:repository)
    dnf_packages=
    extra='rpm-sign gnupg2'
    ;;
  *:packaging)
    echo "install-deps.sh has packaging for debian-13 and fedora-43, not $distro" >&2
    exit 2
    ;;
  *:repository)
    echo "install-deps.sh has the repository for ubuntu-26.04 and fedora-43, not $distro" >&2
    exit 2
    ;;
esac

case $distro in
  ubuntu-26.04 | debian-13)
    export DEBIAN_FRONTEND=noninteractive
    # apt reads a snapshot over https, which a fresh image cannot check before it has the
    # certificates, taken from the archive the image names.
    apt-get install --update -y --no-install-recommends ca-certificates
    if [ "$kind" = repository ]; then
      fonts=
      set -- --snapshot "$snapshot"
    elif [ "$distro" = debian-13 ]; then
      # Debian names Cantarell where GNOME names Adwaita Sans, which Debian does not ship.
      fonts='fonts-cantarell fonts-dejavu-core'
      # trixie-updates' and trixie-security's Release files expire a week after the snapshot,
      # so apt keeps them past that, as snapshot.debian.org says; their signatures still hold.
      set -- --snapshot "$snapshot" -o Acquire::Check-Valid-Until=false
    else
      fonts='fonts-adwaita'
      set -- --snapshot "$snapshot"
    fi
    # shellcheck disable=SC2086 # each list is words
    apt-get install --update -y --no-install-recommends "$@" $apt_packages $fonts $extra
    ;;
  fedora-43 | fedora-44)
    # shellcheck disable=SC2086 # each list is words
    dnf install -y --repo fedora --repo updates --setopt=install_weak_deps=False $dnf_packages \
      $extra
    ;;
  *)
    echo "install-deps.sh knows ubuntu-26.04, debian-13, fedora-43 and fedora-44, not $distro" >&2
    exit 2
    ;;
esac

[ "$kind" != repository ] || exit 0

case $(uname -m) in
  x86_64)
    triple=x86_64-unknown-linux-gnu
    rustup_sum=5be191e8c70a5007074254cb74d78dbfdd25cd5a61c27de7b8e5410122ec24f4
    nextest_sum=682c21b777c333e96fd532e114d3a5a894e0729ab88d94c0a9f20f8419695428
    hack_sum=16394b932180a4ae509a22b3b8294fc2525573d8592420133c428457ed676d5d
    ;;
  aarch64)
    triple=aarch64-unknown-linux-gnu
    rustup_sum=a9c168dbea81a84671c6b7a5c1795cb0eb699020110d107ddfca427bd797e697
    nextest_sum=b2e33d7c72de7ade0ff7b3a948ac37516b24f8a836b7a8870c1f634a94be9de9
    hack_sum=821536b7dc4666764d36c09e665affcf98e3d69f5a99b171c03d47d3ec24d0e7
    ;;
  *)
    echo "install-deps.sh has no Rust for $(uname -m)" >&2
    exit 1
    ;;
esac

downloads=$(mktemp -d)
trap 'rm -rf "$downloads"' EXIT
fetch() {
  curl --proto '=https' --tlsv1.2 -sSfL -o "$downloads/$1" "$2"
  echo "$3  $downloads/$1" | sha256sum -c -
}
fetch rustup-init "https://static.rust-lang.org/rustup/archive/$rustup/$triple/rustup-init" \
  "$rustup_sum"
fetch nextest.tar.gz \
  "https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-$nextest/cargo-nextest-$nextest-$triple.tar.gz" \
  "$nextest_sum"
fetch hack.tar.gz \
  "https://github.com/taiki-e/cargo-hack/releases/download/v$hack/cargo-hack-$triple.tar.gz" \
  "$hack_sum"

# With no toolchain: rust-toolchain.toml names it.
chmod 0755 "$downloads/rustup-init"
RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo "$downloads/rustup-init" -y --no-modify-path \
  --profile minimal --default-toolchain none
# Outside CARGO_HOME, whose bin Swatinem/rust-cache restores.
tar -xzf "$downloads/nextest.tar.gz" -C /usr/local/bin cargo-nextest
tar -xzf "$downloads/hack.tar.gz" -C /usr/local/bin cargo-hack
