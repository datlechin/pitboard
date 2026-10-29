#!/bin/sh
# in-container.sh <ubuntu-26.04|debian-13|fedora-44> [step...]: the Linux app's checks on a
# Linux of their own, with podman or else docker, as CI runs them: check.sh's steps, each in a
# fresh container of the image apps/linux/containers holds for the distribution, as the image's
# uid 1000, on no network. With no step, the steps the distribution's CI job runs.
#
# A container sees the repository read only at /src, this machine's Cargo registry read only,
# and the volume pitboard-linux-app-target, which keeps Cargo's and Meson's folders per
# distribution and Cargo's git checkouts, and nothing else of this machine. Its homes are its
# own. Only `cargo fetch`, before the steps, has the network, for the git checkouts: the
# registry is this machine's Cargo's to fill, which `cargo metadata` does here first.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)

if [ "$#" -eq 0 ]; then
  echo "usage: in-container.sh <ubuntu-26.04|debian-13|fedora-44> [step...]" >&2
  exit 2
fi
distro=$1
shift
containerfile=$repo/apps/linux/containers/$distro.Containerfile
if [ ! -f "$containerfile" ]; then
  echo "apps/linux/containers holds no image for $distro" >&2
  exit 2
fi
if [ "$(id -u)" -eq 0 ]; then
  echo "in-container.sh runs nothing as root: the Linux app's checks run as a user" >&2
  exit 1
fi
if [ "$#" -eq 0 ]; then
  case $distro in
    ubuntu-26.04) set -- lint test ui release ;;
    debian-13) set -- lint test floor ;;
    *) set -- lint test ;;
  esac
fi
if command -v podman >/dev/null 2>&1; then
  engine=podman
elif command -v docker >/dev/null 2>&1; then
  engine=docker
else
  echo "in-container.sh runs the checks with podman or docker, and finds neither" >&2
  exit 1
fi

made_from=$(cat "$containerfile" "$here/install-deps.sh" | cksum | cut -d ' ' -f 1)
image=pitboard-linux-app:$distro-$made_from
if ! "$engine" image inspect "$image" >/dev/null 2>&1; then
  "$engine" build -f "$containerfile" -t "$image" "$here"
fi

cargo metadata --locked --format-version 1 --manifest-path "$repo/Cargo.toml" >/dev/null
registry=${CARGO_HOME:-$HOME/.cargo}/registry

# Each container makes its homes, and keeps Cargo's git checkouts in the volume.
# shellcheck disable=SC2016 # the container's shell expands it
setup='mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME" "$XDG_STATE_HOME" \
    "$XDG_RUNTIME_DIR" /target/cargo-git
  chmod 0700 "$XDG_RUNTIME_DIR"
  ln -s /target/cargo-git "$CARGO_HOME/git"
  exec "$@"'

# inside <online|offline> <command...>: the command in a fresh container.
inside() {
  network=$1
  shift
  set -- "$image" sh -c "$setup" sh "$@"
  if [ "$network" = offline ]; then
    set -- --network none --env CARGO_NET_OFFLINE=true "$@"
  fi
  # The release's link takes most of 8 GiB, so a small machine sets CARGO_BUILD_JOBS=1.
  if [ -n "${CARGO_BUILD_JOBS-}" ]; then
    set -- --env "CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS" "$@"
  fi
  "$engine" run --rm --user 1000:1000 \
    --mount "type=bind,src=$repo,dst=/src,readonly" \
    --tmpfs /cargo:uid=1000,gid=1000 \
    --mount "type=bind,src=$registry,dst=/cargo/registry,readonly" \
    --mount type=volume,src=pitboard-linux-app-target,dst=/target \
    --workdir /src --env CARGO_HOME=/cargo --env "CARGO_TARGET_DIR=/target/$distro" \
    --env CARGO_INCREMENTAL=0 --env "RUSTFLAGS=-D warnings" \
    --env HOME=/tmp/home --env XDG_CONFIG_HOME=/tmp/home/.config \
    --env XDG_DATA_HOME=/tmp/home/.local/share --env XDG_CACHE_HOME=/tmp/home/.cache \
    --env XDG_STATE_HOME=/tmp/home/.local/state --env XDG_RUNTIME_DIR=/tmp/runtime \
    "$@"
}

inside online cargo fetch --locked
for step in "$@"; do
  echo "in-container.sh: $distro $step" >&2
  inside offline apps/linux/scripts/check.sh "$step"
done
