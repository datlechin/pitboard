#!/bin/sh
# headless.sh <command> [argument...]: runs a command as the Linux app's tests and UI tests run,
# never on a desktop's own bus or display: in a headless mutter with one virtual monitor, on a
# private session bus, with every home, XDG folder and temporary folder fresh under one folder
# removed after, and with none of the caller's variables but PATH, TERM, Rust's, Cargo's,
# rustup's and nextest's, and the folder the pictures are kept in.
set -eu

if [ "$#" -eq 0 ]; then
  echo "usage: headless.sh <command> [argument...]" >&2
  exit 2
fi
if [ "$(id -u)" -eq 0 ]; then
  echo "headless.sh runs nothing as root: the Linux app and its tests run as a user" >&2
  exit 1
fi

root=$(mktemp -d)
trap 'chmod -R u+w "$root" 2>/dev/null; rm -rf "$root"' EXIT
for folder in home config data cache state runtime tmp pitboard claude codex; do
  mkdir "$root/$folder"
done
chmod 0700 "$root/runtime"

set -- dbus-run-session -- mutter --headless --wayland --no-x11 --virtual-monitor 1280x800 \
  -- "$@"
# Only a name the pattern matched reaches eval, which reads its value.
for name in $(env | awk -F= '/^(CARGO|RUSTUP|NEXTEST|__CARGO)_[A-Za-z0-9_]*=/ { print $1 }') \
  RUSTFLAGS RUSTDOCFLAGS RUST_BACKTRACE TERM PITBOARD_LINUX_PICTURES; do
  if eval "[ -n \"\${$name+set}\" ]"; then
    eval "set -- \"$name=\${$name}\" \"\$@\""
  fi
done

# GSK_RENDERER: docs/facts/linux-app.md, The headless display. ADW_DISABLE_PORTAL: libadwaita
# reads the colour scheme and accent from the GSettings in memory, as a desktop's portal would
# serve them, and not from whatever portal the private bus starts.
env -i PATH="$PATH" LANG=C.UTF-8 \
  HOME="$root/home" XDG_CONFIG_HOME="$root/config" XDG_DATA_HOME="$root/data" \
  XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" XDG_RUNTIME_DIR="$root/runtime" \
  TMPDIR="$root/tmp" PITBOARD_HOME="$root/pitboard" CLAUDE_CONFIG_DIR="$root/claude" \
  CODEX_HOME="$root/codex" GSETTINGS_BACKEND=memory ADW_DISABLE_PORTAL=1 GDK_BACKEND=wayland \
  GSK_RENDERER=opengl "$@"
