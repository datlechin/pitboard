# Homebrew cask for the menu bar app, for the tap datlechin/homebrew-tap.
#
# The download is the notarised build from the release, so Gatekeeper accepts it with no
# further step. Sparkle keeps it up to date afterwards, which is why auto_updates is set.
cask "pitboard" do
  version "0.1.2"
  sha256 "b556d0983206d455fda28f1561e58e4f30cae5f8c9b706ba665fc52674c61769"

  url "https://github.com/datlechin/pitboard/releases/download/v#{version}/Pitboard-v#{version}-macos.zip"
  name "pitboard"
  desc "Menu bar view of every Claude account's limits, and one click to switch"
  homepage "https://github.com/datlechin/pitboard"

  auto_updates true
  depends_on macos: :sonoma
  # The app shows accounts and switches between them; enrolling one is still the command
  # line's job, and the app tells people to run it.
  depends_on formula: "pitboard"

  app "Pitboard.app"

  zap trash: [
    "~/.pitboard",
    "~/Library/Application Support/com.usepitboard.Pitboard",
    "~/Library/Caches/com.usepitboard.Pitboard",
    "~/Library/Preferences/com.usepitboard.Pitboard.plist",
  ]
end
