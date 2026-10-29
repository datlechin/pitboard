#!/bin/sh
# Meson's step for the program: cargo-build.sh <Cargo.toml> <target folder> <output>. Cargo
# builds the release in Meson's build folder, with what Meson set in the environment, and the
# program goes where Meson installs its output from.
set -eu

cargo build --locked --release --manifest-path "$1" --target-dir "$2" -p pitboard-linux
cp "$2/release/pitboard-app" "$3"
