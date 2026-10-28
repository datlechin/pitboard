#!/bin/sh
# check.sh <step> [argument...]: one of the Linux app's checks, as CI's jobs and in-container.sh
# run it on a Linux with install-deps.sh's packages, from the repository's root.
#
#   lint                      clippy, with the fixtures and without, as Meson builds the release
#   test                      the tests but the UI tests and the pictures, built here with the
#                             fixtures, under headless.sh
#   ui                        the UI tests and the pictures, which need GTK 4.22, the pictures
#                             kept in $PITBOARD_LINUX_PICTURES, else linux-pictures in Cargo's
#                             target folder
#   archive <file>            the tests built into a nextest archive, for another user to run
#   test|ui <archive> <folder>
#                             the same with that archive's tests, unpacked in the folder over
#                             what an earlier step unpacked there
#   release                   Meson's release build, its tests, its install tree, and
#                             check-linux-app.sh on that tree
#   floor                     the app on the Rust its manifest names
#   scripts                   test-linux-app.py, check-linux-app.sh's cases included
#   widget                    the Plasma widget: qmllint, then its tests under headless.sh, which
#                             need Plasma 6.4's QML modules, the pictures kept as ui keeps its own
#   command-line <out>        the command line built for musl and packed into <out> as a
#                             release builds and packs it
#   deb <tarball> <out>       the Debian packages built into <out> from the source tar on stdin
#                             and the command line tarball, then check-linux-packages.sh on them
#   rpm <tarball> <out>       the same for the RPM packages
#   packages-test             test-linux-packages.py, with the packaging tools of this system's
#                             format, Debian's or Fedora's
#
# Meson's build folder and its install tree are `meson` and `destdir` in Cargo's target folder.
# The build folder holds Cargo's release target, so a run sets it up again and never removes it.
set -eu

repo=$(cd "$(dirname "$0")/../../.." && pwd)
cd "$repo"
target=${CARGO_TARGET_DIR:-$repo/target}

usage() {
  echo "usage: check.sh <lint | test [<archive> <folder>] | ui [<archive> <folder>] |" \
    "archive <file> | release | floor | scripts | widget | command-line <out> |" \
    "<deb|rpm> <tarball> <out> | packages-test>" >&2
  exit 2
}

# The version Cargo.toml's [workspace.package] names, as release.yml's guard reads it.
manifest_version() {
  sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml
}

[ "$#" -ge 1 ] || usage
step=$1
shift
case $step in
  lint)
    [ "$#" -eq 0 ] || usage
    cargo clippy -p pitboard-linux --all-targets --locked
    cargo clippy -p pitboard-linux --all-targets --locked --features fixture
    ;;
  test | ui)
    on_screen='binary_id(pitboard-linux::ui) | binary_id(pitboard-linux::pictures)'
    case $step in
      test) filter="not ($on_screen)" ;;
      ui)
        filter=$on_screen
        PITBOARD_LINUX_PICTURES=${PITBOARD_LINUX_PICTURES:-$target/linux-pictures}
        export PITBOARD_LINUX_PICTURES
        mkdir -p "$PITBOARD_LINUX_PICTURES"
        ;;
    esac
    case $# in
      0) set -- cargo nextest run --locked -p pitboard-linux --features fixture ;;
      2)
        set -- cargo-nextest nextest run --archive-file "$1" --workspace-remap "$repo" \
          --extract-to "$2" --extract-overwrite
        ;;
      *) usage ;;
    esac
    apps/linux/scripts/headless.sh "$@" -E "$filter"
    ;;
  archive)
    [ "$#" -eq 1 ] || usage
    cargo nextest archive --locked -p pitboard-linux --features fixture --archive-file "$1"
    ;;
  release)
    [ "$#" -eq 0 ] || usage
    build=$target/meson
    if [ -d "$build/meson-private" ]; then
      meson setup --reconfigure "$build" apps/linux --prefix=/usr
    else
      meson setup "$build" apps/linux --prefix=/usr
    fi
    meson compile -C "$build"
    meson test -C "$build" --print-errorlogs
    rm -rf "$target/destdir"
    meson install -C "$build" --destdir "$target/destdir"
    sh .github/scripts/check-linux-app.sh "$target/destdir"
    ;;
  floor)
    [ "$#" -eq 0 ] || usage
    cargo hack check --rust-version -p pitboard-linux --all-targets --locked
    ;;
  scripts)
    [ "$#" -eq 0 ] || usage
    TEST_LINUX_APP_TOOLS='desktop-file-validate appstreamcli' \
      python3 .github/scripts/test-linux-app.py
    ;;
  widget)
    [ "$#" -eq 0 ] || usage
    plasmoid=apps/linux/plasmoid
    "$(qtpaths6 --query QT_INSTALL_BINS)/qmllint" --max-warnings 0 \
      "$plasmoid"/com.usepitboard.Pitboard/contents/ui/*.qml "$plasmoid/tests/tst_widget.qml"
    PITBOARD_LINUX_PICTURES=${PITBOARD_LINUX_PICTURES:-$target/linux-pictures}
    export PITBOARD_LINUX_PICTURES
    apps/linux/scripts/headless.sh sh "$plasmoid/tests/run-tests.sh"
    ;;
  command-line)
    [ "$#" -eq 1 ] || usage
    triple=$(uname -m)-unknown-linux-musl
    # ring compiles C, and needs a compiler that targets musl.
    env "CC_$(echo "$triple" | tr - _)=musl-gcc" \
      cargo build --release --locked -p pitboard --target "$triple"
    sh .github/scripts/pack-command-line.sh "$target/$triple/release/pitboard" \
      "pitboard-v$(manifest_version)-$triple" "$1"
    ;;
  deb | rpm)
    [ "$#" -eq 2 ] || usage
    sh .github/scripts/build-linux-packages.sh "$step" "$1" "$2"
    sh .github/scripts/check-linux-packages.sh "$step" "$2" "$1" "$(manifest_version)"
    ;;
  packages-test)
    [ "$#" -eq 0 ] || usage
    # shellcheck source=/dev/null # the system's, read as os-release(5) says a shell may
    . /etc/os-release
    case $ID in
      debian) tools='dpkg dpkg-deb lintian gpg' ;;
      fedora) tools='rpm rpmbuild rpmlint rpm2archive' ;;
      *)
        echo "check.sh packages-test runs on Debian or Fedora, not $ID" >&2
        exit 2
        ;;
    esac
    TEST_LINUX_PACKAGES_TOOLS="$tools desktop-file-validate appstreamcli" \
      python3 .github/scripts/test-linux-packages.py
    ;;
  *) usage ;;
esac
