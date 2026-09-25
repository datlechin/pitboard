# Homebrew cask for the menu bar app, for the tap datlechin/homebrew-tap.
#
# This is the template. The release's `tap` job fills its placeholders the way it fills
# pitboard.rb's and commits the two in one commit, so the pair cannot be half updated. The
# download is the notarised build from the release, so Gatekeeper accepts it with no further
# step. Sparkle keeps it up to date afterwards, which is why auto_updates is set.
#
# The app carries the command line at Contents/Helpers/pitboard, with its man page and
# completions, and this links them where the pitboard cask would. They are paths into the
# bundle, so a Sparkle update moves the command line with the app.
cask "pitboard-app" do
  version "@VERSION@"
  sha256 "@SHA256_APP@"

  url "https://github.com/datlechin/pitboard/releases/download/v#{version}/Pitboard-v#{version}-macos.zip"
  name "pitboard"
  desc "Menu bar view of Claude Code and Codex account limits, and one click to switch"
  homepage "https://usepitboard.com/"

  auto_updates true
  conflicts_with cask: "pitboard"
  depends_on macos: :sonoma

  app "Pitboard.app"
  binary "#{appdir}/Pitboard.app/Contents/Helpers/pitboard"
  manpage "#{appdir}/Pitboard.app/Contents/Resources/man/pitboard.1"
  bash_completion "#{appdir}/Pitboard.app/Contents/Resources/completions/pitboard.bash"
  zsh_completion "#{appdir}/Pitboard.app/Contents/Resources/completions/pitboard.zsh"
  fish_completion "#{appdir}/Pitboard.app/Contents/Resources/completions/pitboard.fish"

  # The renewal schedule in zap and not uninstall, because Homebrew runs uninstall on every
  # upgrade and reinstall too. ~/.pitboard stays: it is the only index of the parked logins,
  # and without it they are left where nothing can name them. `pitboard uninstall` deletes
  # the logins and then the directory, so it has to come first.
  zap launchctl: "com.datlechin.pitboard.renew",
      trash:     [
        "~/Library/Application Support/com.usepitboard.Pitboard",
        "~/Library/Caches/com.usepitboard.Pitboard",
        "~/Library/HTTPStorages/com.usepitboard.Pitboard",
        "~/Library/HTTPStorages/com.usepitboard.Pitboard.binarycookies",
        "~/Library/Preferences/com.usepitboard.Pitboard.plist",
      ]
end
