# Homebrew cask for the command line on macOS and Linux, in the tap datlechin/homebrew-tap.
#
# pitboard's release writes the tap's copy from packaging/pitboard.rb in datlechin/pitboard,
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

  url "https://github.com/datlechin/pitboard/releases/download/v#{version}/pitboard-v#{version}-#{arch}-#{os}.tar.gz"
  name "pitboard"
  desc "Park and restore your own Claude Code and Codex logins"
  homepage "https://usepitboard.com/"

  # The app carries this same command line and links it to the same place.
  conflicts_with cask: "pitboard-app"

  binary "pitboard-v#{version}-#{arch}-#{os}/pitboard"
  manpage "pitboard-v#{version}-#{arch}-#{os}/pitboard.1"
  bash_completion "pitboard-v#{version}-#{arch}-#{os}/completions/pitboard.bash"
  zsh_completion "pitboard-v#{version}-#{arch}-#{os}/completions/pitboard.zsh"
  fish_completion "pitboard-v#{version}-#{arch}-#{os}/completions/pitboard.fish"

  # The renewal schedule, the launchd job on macOS and the systemd timer on Linux. In zap
  # and not uninstall, because Homebrew runs uninstall on every upgrade and reinstall too.
  # ~/.pitboard stays: it is the only index of the parked logins, and without it they are
  # left where nothing can name them. `pitboard uninstall` deletes the logins and then the
  # directory, so it has to come first.
  zap launchctl: "com.datlechin.pitboard.renew",
      trash:     [
        "~/.config/systemd/user/pitboard-renew.service",
        "~/.config/systemd/user/pitboard-renew.timer",
        "~/.config/systemd/user/timers.target.wants/pitboard-renew.timer",
      ]

  caveats <<~EOS
    On macOS the menu bar app is the pitboard-app cask, and it includes this command line.
    To switch to it, or to get back an app the old pitboard cask installed:
      brew uninstall --cask pitboard && brew install --cask datlechin/tap/pitboard-app

    To remove pitboard with the logins it parked, run `pitboard uninstall` before
    `brew uninstall`.
  EOS
end
