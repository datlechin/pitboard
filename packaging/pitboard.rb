# Homebrew formula for a personal tap, such as datlechin/homebrew-pitboard.
#
# It builds from source on purpose. Without a Developer ID certificate a prebuilt binary
# cannot be notarised, and Gatekeeper would warn on first launch; a local build carries no
# quarantine flag. Replace the url and sha256 with those of a tagged release.
class Pitboard < Formula
  desc "Park and restore your own Claude Code logins"
  homepage "https://github.com/datlechin/pitboard"
  url "https://github.com/datlechin/pitboard/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "REPLACE_WITH_THE_RELEASE_TARBALL_SHA256"
  license "Apache-2.0"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: "crates/pitboard")
    generate_completions_from_executable(bin/"pitboard", "completions")
    (man1/"pitboard.1").write Utils.safe_popen_read(bin/"pitboard", "manpage")
  end

  test do
    assert_match "pitboard", shell_output("#{bin}/pitboard --version")
  end
end
