#!/bin/sh
# Builds what `cargo nextest run` is given, then runs it as AGENTS.md's On Linux runs a test:
# every home and XDG folder fresh, the runtime folder 0700, on a private bus and no display.
# Cargo and rustup keep the homes they were installed in.
set -eu

export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
cargo nextest run --no-run "$@"

fresh=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/pitboard-homes.XXXXXX")
trap 'rm -rf "$fresh"' EXIT
for folder in home pitboard claude codex config data cache state runtime; do
  mkdir "$fresh/$folder"
done
chmod 700 "$fresh/runtime"
export HOME="$fresh/home" PITBOARD_HOME="$fresh/pitboard" CLAUDE_CONFIG_DIR="$fresh/claude" \
  CODEX_HOME="$fresh/codex" XDG_CONFIG_HOME="$fresh/config" XDG_DATA_HOME="$fresh/data" \
  XDG_CACHE_HOME="$fresh/cache" XDG_STATE_HOME="$fresh/state" XDG_RUNTIME_DIR="$fresh/runtime"
unset DISPLAY WAYLAND_DISPLAY
dbus-run-session -- cargo nextest run "$@"
