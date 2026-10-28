#!/bin/sh
# check.sh <step> [argument...]: one of the Linux app's checks, as CI's jobs and in-container.sh
# run it on a Linux with install-deps.sh's packages, from the repository's root.
#
#   lint                      clippy
#   test                      every GtkBuilder file through gtk4-builder-tool, then the tests,
#                             built here, each under headless.sh
#   archive <file>            the tests built into a nextest archive, for another user to run
#   test <archive> <folder>   the same with that archive's tests, unpacked in the folder
#   release                   Meson's release build, its tests, its install tree, and
#                             check-linux-app.sh on that tree
#   floor                     the app on the Rust its manifest names
#   scripts                   test-linux-app.py, check-linux-app.sh's cases included
#
# Meson's build folder and its install tree are `meson` and `destdir` in Cargo's target folder.
# The build folder holds Cargo's release target, so a run sets it up again and never removes it.
set -eu

repo=$(cd "$(dirname "$0")/../../.." && pwd)
cd "$repo"
target=${CARGO_TARGET_DIR:-$repo/target}

usage() {
  echo "usage: check.sh <lint | test [<archive> <folder>] | archive <file> | release | floor |" \
    "scripts>" >&2
  exit 2
}

[ "$#" -ge 1 ] || usage
step=$1
shift
case $step in
  lint)
    [ "$#" -eq 0 ] || usage
    cargo clippy -p pitboard-linux --all-targets --locked
    ;;
  test)
    case $# in
      0) set -- cargo nextest run --locked -p pitboard-linux ;;
      2)
        set -- cargo-nextest nextest run --archive-file "$1" --workspace-remap "$repo" \
          --extract-to "$2"
        ;;
      *) usage ;;
    esac
    # gtk4-builder-tool validates only on a display, which headless.sh gives a user alone.
    apps/linux/scripts/headless.sh find apps/linux -name '*.ui' \
      -exec gtk4-builder-tool validate {} +
    apps/linux/scripts/headless.sh "$@"
    ;;
  archive)
    [ "$#" -eq 1 ] || usage
    cargo nextest archive --locked -p pitboard-linux --archive-file "$1"
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
  *) usage ;;
esac
