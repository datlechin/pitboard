# Homebrew cask for the menu bar app, for the tap datlechin/homebrew-tap.
#
# This is the template. The release's `tap` job writes the version and the checksum into it
# and commits the result to the tap, in the same commit as the formula, so the pair cannot
# be half updated. The download is the notarised build from the release, so Gatekeeper
# accepts it with no further step. Sparkle keeps it up to date afterwards, which is why
# auto_updates is set.
cask "pitboard" do
  version "0.0.0"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"

  url "https://github.com/datlechin/pitboard/releases/download/v#{version}/Pitboard-v#{version}-macos.zip"
  name "pitboard"
  desc "Menu bar view of every Claude account's limits, and one click to switch"
  homepage "https://usepitboard.com"

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
