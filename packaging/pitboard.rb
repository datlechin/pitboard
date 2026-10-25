# Homebrew cask for the command line on macOS and Linux, in the tap datlechin/homebrew-tap.
#
# Pitboard's release writes the tap's copy from packaging/pitboard.rb in datlechin/pitboard,
# with the version and the checksums from the release's SHA256SUMS filled in. An edit made
# to the tap's copy is replaced at the next release.
#
# The download is the release's own tarball for the machine, attested, and on macOS signed
# and notarised, so nothing is built and nobody needs Rust.
cask "pitboard" do
  arch arm: "aarch64", intel: "x86_64"
  os macos: "apple-darwin", linux: "unknown-linux-musl"

  version "@VERSION@"
  sha256 arm:          "@SHA256_AARCH64_APPLE_DARWIN@",
         intel:        "@SHA256_X86_64_APPLE_DARWIN@",
         arm64_linux:  "@SHA256_AARCH64_UNKNOWN_LINUX_MUSL@",
         x86_64_linux: "@SHA256_X86_64_UNKNOWN_LINUX_MUSL@"

  # In zap and not uninstall, because Homebrew runs uninstall on every upgrade and reinstall
  # too. ~/.pitboard stays: it is the only index of the parked logins, so `pitboard uninstall`
  # has to delete them first. Up here because Homebrew's style puts blocks for one system
  # straight after the checksums.
  on_macos do
    zap launchctl: [
          "com.datlechin.pitboard.renew",
          "com.datlechin.pitboard.watch",
        ],
        trash:     "~/Library/Logs/Pitboard/watch.log",
        rmdir:     "~/Library/Logs/Pitboard"
  end
  # systemd keeps running a loaded unit whose files are gone. Homebrew runs the script without
  # XDG_RUNTIME_DIR, which systemctl --user needs to reach the user's manager.
  on_linux do
    zap script: {
          executable:   "/bin/sh",
          args:         [
            "-c",
            "for unit in pitboard-renew.timer pitboard-watch.service; do " \
            "test ! -e ~/.config/systemd/user/$unit || " \
            "XDG_RUNTIME_DIR=/run/user/$(id -u) systemctl --user disable --now $unit; done",
          ],
          must_succeed: false,
        },
        trash:  [
          "~/.config/systemd/user/default.target.wants/pitboard-watch.service",
          "~/.config/systemd/user/pitboard-renew.service",
          "~/.config/systemd/user/pitboard-renew.timer",
          "~/.config/systemd/user/pitboard-watch.service",
          "~/.config/systemd/user/timers.target.wants/pitboard-renew.timer",
        ]
  end

  url "https://github.com/datlechin/pitboard/releases/download/v#{version}/pitboard-v#{version}-#{arch}-#{os}.tar.gz"
  name "Pitboard"
  desc "Switch Claude Code, Codex and Claude Desktop between your own accounts"
  homepage "https://usepitboard.com/"

  # The app carries this same command line and links it to the same place. Homebrew 7.0.6 has
  # no conflicts_with formula:, so 0.3.0's formula cannot be named here.
  conflicts_with cask: "pitboard-app"

  binary "pitboard-v#{version}-#{arch}-#{os}/pitboard"
  manpage "pitboard-v#{version}-#{arch}-#{os}/pitboard.1"
  bash_completion "pitboard-v#{version}-#{arch}-#{os}/completions/pitboard.bash"
  zsh_completion "pitboard-v#{version}-#{arch}-#{os}/completions/pitboard.zsh"
  fish_completion "pitboard-v#{version}-#{arch}-#{os}/completions/pitboard.fish"

  caveats <<~EOS
    On macOS the menu bar app is the pitboard-app cask, and it includes this command line.
    To switch to it:
      brew uninstall --cask pitboard
      brew install --cask datlechin/tap/pitboard-app
    If this replaced an app the old pitboard cask installed, see
    https://docs.usepitboard.com/install/upgrade-from-0-3-0

    To remove Pitboard with the logins it parked, run `pitboard uninstall` before
    `brew uninstall`.
  EOS
end
