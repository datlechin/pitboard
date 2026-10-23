#!/bin/sh
# Installs the Pitboard command line: https://docs.usepitboard.com/install
set -eu
# Braced, so the shell reads it all before running any, and a download cut short runs nothing.
{
VERSION="@VERSION@"
SHA256_AARCH64_APPLE_DARWIN="@SHA256_AARCH64_APPLE_DARWIN@"
SHA256_X86_64_APPLE_DARWIN="@SHA256_X86_64_APPLE_DARWIN@"
SHA256_AARCH64_UNKNOWN_LINUX_MUSL="@SHA256_AARCH64_UNKNOWN_LINUX_MUSL@"
SHA256_X86_64_UNKNOWN_LINUX_MUSL="@SHA256_X86_64_UNKNOWN_LINUX_MUSL@"

say() {
  printf '%s\n' "$*"
}

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

check_arguments() {
  if [ "$#" -gt 0 ]; then
    fail "The installer takes no arguments. Set PITBOARD_INSTALL_DIR to choose where it installs."
  fi
}

# Each path, or the folder it would be made in, is root's.
roots_own() {
  for place in "$@"; do
    while [ ! -e "$place" ]; do
      place=${place%/*}
      place=${place:-/}
    done
    owned=$(find -H "$place" -prune -user 0 2>/dev/null) || owned=
    if [ -z "$owned" ]; then
      return 1
    fi
  done
}

# On Linux root may act where it owns each folder, as on a machine whose only user is root.
check_user() {
  uid=$(id -u)
  if [ "$system" = Linux ]; then
    if [ -n "${SUDO_UID:-}" ] || { [ "$uid" = 0 ] && ! roots_own "${home:-/}" "$bin" "$share"; }; then
      fail "Pitboard changes nothing when it runs with sudo or as root in a folder another user" \
        "owns, so this installs nothing. Run it as yourself."
    fi
  elif [ "$uid" = 0 ] || [ -n "${SUDO_UID:-}" ]; then
    fail "Pitboard changes nothing when it runs as root or with sudo, so this installs nothing." \
      "Run it as yourself."
  fi
}

find_target() {
  system=$(uname -s)
  machine=$(uname -m)
  case $system in
    Darwin) vendor_os=apple-darwin ;;
    Linux) vendor_os=unknown-linux-musl ;;
    MINGW* | MSYS* | CYGWIN*)
      fail "This script installs Pitboard on macOS and Linux." \
        "For Windows, see https://docs.usepitboard.com/install."
      ;;
    *) fail "Pitboard has no build for $system." ;;
  esac
  case $machine in
    arm64 | aarch64) cpu=aarch64 ;;
    x86_64 | amd64) cpu=x86_64 ;;
    *) fail "Pitboard has no build for $system on $machine." ;;
  esac
  # A shell under Rosetta says x86_64 on Apple silicon, where the arm64 build is the one to run.
  if [ "$system" = Darwin ] && [ "$cpu" = x86_64 ] &&
    [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then
    cpu=aarch64
  fi
  target=$cpu-$vendor_os
  case $target in
    aarch64-apple-darwin) sum=$SHA256_AARCH64_APPLE_DARWIN ;;
    x86_64-apple-darwin) sum=$SHA256_X86_64_APPLE_DARWIN ;;
    aarch64-unknown-linux-musl) sum=$SHA256_AARCH64_UNKNOWN_LINUX_MUSL ;;
    x86_64-unknown-linux-musl) sum=$SHA256_X86_64_UNKNOWN_LINUX_MUSL ;;
  esac
}

find_places() {
  if [ -z "${HOME+set}" ]; then
    fail "HOME is not set. Set it to a full path."
  fi
  case $HOME in
    /*) ;;
    "") fail "HOME is empty. Set it to a full path." ;;
    *) fail "HOME is \`$HOME\`, which is not a full path. Set it to a full path." ;;
  esac
  # Trailing slashes go, as dirname takes them, so share lands beside bin; / stays /.
  home=${HOME%"${HOME##*[!/]}"}
  bin=${PITBOARD_INSTALL_DIR-$home/.local/bin}
  case $bin in
    /*) ;;
    "") fail "PITBOARD_INSTALL_DIR is empty. Set it to a full path, or unset it." ;;
    *)
      fail "PITBOARD_INSTALL_DIR is \`$bin\`, which is not a full path." \
        "Set it to a full path, or unset it."
      ;;
  esac
  bin=${bin%"${bin##*[!/]}"}
  bin=${bin:-/}
  share=${bin%/*}/share
}

find_tools() {
  if command -v curl >/dev/null 2>&1; then
    downloader=curl
  elif command -v wget >/dev/null 2>&1; then
    downloader=wget
  else
    fail "The installer needs curl or wget, and neither is on PATH."
  fi
  if command -v sha256sum >/dev/null 2>&1; then
    hasher=sha256sum
  elif command -v shasum >/dev/null 2>&1; then
    hasher=shasum
  else
    fail "The installer needs sha256sum or shasum to check the download, and neither is on PATH."
  fi
}

# Through stdout, as a snap's curl may not open files, and -q, so a .curlrc cannot write elsewhere.
download() {
  if [ "$downloader" = curl ]; then
    curl -q --proto '=https' --tlsv1.2 -fsSL "$1" </dev/null >"$2"
  else
    wget -q -O - "$1" </dev/null >"$2"
  fi
}

checksum_of() {
  if [ "$hasher" = sha256sum ]; then
    got=$(sha256sum "$1")
  else
    got=$(shasum -a 256 "$1")
  fi
  got=${got%% *}
}

# Every folder is made and checked first, so one it cannot write leaves the old copy as it was.
make_places() {
  if ! mkdir -p "$bin" || ! staged=$(mktemp "$bin/.pitboard.XXXXXX"); then
    fail "This account cannot write to $bin, so nothing was installed."
  fi
  for dir in "$share/man/man1" "$share/bash-completion/completions" "$share/zsh/site-functions" \
    "$share/fish/vendor_completions.d"; do
    if ! mkdir -p "$dir" || [ ! -w "$dir" ]; then
      fail "This account cannot write to $dir, so nothing was installed."
    fi
  done
}

# Checked in $bin, as /tmp may be noexec, and renamed in, so a running copy never sees half a file.
install_command_line() {
  cp "$1/pitboard" "$staged"
  chmod 755 "$staged"
  if ! "$staged" --version </dev/null >/dev/null; then
    fail "The downloaded command line does not run from $bin, so nothing was installed."
  fi
  mv -f "$staged" "$bin/pitboard"
  staged=
}

# Chained, as set -e is off under if; removed first, so a link in its place is not written through.
place() {
  rm -f "$2" && cp "$1" "$2" && chmod 644 "$2"
}

in_profile() {
  case $1 in
    "$home"/*) printf '%s' "\$HOME/${1#"$home"/}" ;;
    *) printf '%s' "$1" ;;
  esac
}

path_hint() {
  shown=$(in_profile "$bin")
  login=${SHELL:-}
  rc=.profile
  after="open another terminal"
  case ${login##*/} in
    zsh) rc=.zshrc ;;
    bash) if [ "$system" = Darwin ]; then rc=.bash_profile; else rc=.bashrc; fi ;;
    fish) ;;
    # A terminal on Linux starts a shell that does not read ~/.profile, and a login does.
    *) if [ "$system" != Darwin ]; then after="log in again"; fi ;;
  esac
  if [ "${login##*/}" = fish ]; then
    line="fish_add_path \"$shown\""
  else
    line="echo 'export PATH=\"$shown:\$PATH\"' >> ~/$rc"
  fi
  say "$bin is not on your PATH. To put it there, run this, then $after:"
  say "  $line"
}

# zsh's own fpath holds no folder under the home, and holds prefixes such as /usr/local's.
zsh_hint() {
  say "zsh loads the completions once ~/.zshrc has these lines." \
    "Where it runs compinit already, add only the first, before it:"
  say "  fpath=(\"$(in_profile "$share")/zsh/site-functions\" \$fpath)"
  say "  autoload -Uz compinit && compinit"
}

app_note() {
  for app in "$@"; do
    if [ -d "$app" ]; then
      say "The Pitboard app carries its own command line, which updates with the app." \
        "This one updates when you run this installer again, so keep it at the app's version."
      return 0
    fi
  done
}

cleanup() {
  if [ -n "$tmp" ]; then
    rm -rf "$tmp"
  fi
  if [ -n "$staged" ]; then
    rm -f "$staged"
  fi
}

main() {
  # Otherwise zsh ties the EXIT trap to main, and an exit under set -e can skip it.
  if [ -n "${ZSH_VERSION-}" ]; then setopt POSIX_TRAPS; fi
  check_arguments "$@"
  find_target
  find_places
  check_user
  find_tools

  tmp=
  staged=
  trap cleanup EXIT
  trap 'exit 1' HUP INT TERM
  tmp=$(mktemp -d)

  name=pitboard-v$VERSION-$target
  url=https://github.com/datlechin/pitboard/releases/download/v$VERSION/$name.tar.gz
  say "Downloading $url"
  if ! download "$url" "$tmp/$name.tar.gz"; then
    fail "The download failed, so nothing was installed."
  fi
  checksum_of "$tmp/$name.tar.gz"
  if [ "$got" != "$sum" ]; then
    fail "The download does not match the checksum this installer carries," \
      "so nothing was installed."
  fi
  tar -xzf "$tmp/$name.tar.gz" -C "$tmp"

  make_places
  install_command_line "$tmp/$name"
  if ! place "$tmp/$name/pitboard.1" "$share/man/man1/pitboard.1" ||
    ! place "$tmp/$name/completions/pitboard.bash" "$share/bash-completion/completions/pitboard" ||
    ! place "$tmp/$name/completions/pitboard.zsh" "$share/zsh/site-functions/_pitboard" ||
    ! place "$tmp/$name/completions/pitboard.fish" "$share/fish/vendor_completions.d/pitboard.fish"
  then
    fail "Installed Pitboard $VERSION at $bin/pitboard, but its man page and completions" \
      "could not be written to $share."
  fi
  say "Installed Pitboard $VERSION at $bin/pitboard, and its man page and completions in $share."

  found=$(command -v pitboard 2>/dev/null) || found=
  if [ -z "$found" ]; then
    path_hint
  elif ! [ "$found" -ef "$bin/pitboard" ]; then
    case ":$PATH:" in
      *":$bin:"* | *":$bin/:"*) ;;
      *) path_hint ;;
    esac
    say "Your shell runs $found first. Remove that copy, or put $bin before it on PATH."
  fi
  login=${SHELL:-}
  case ${login##*/}:$share in
    zsh:"$home"/*) zsh_hint ;;
  esac
  if [ "$system" = Darwin ]; then
    app_note /Applications/Pitboard.app "$home/Applications/Pitboard.app"
  fi
  say "Next, run \`pitboard\`: it says what is signed in and how to enrol it."
}

main "$@"
}
