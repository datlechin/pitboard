# Homebrew formula for the tap datlechin/homebrew-tap.
#
# It builds from source on purpose. The released binaries are signed ad hoc, and a local
# build carries no quarantine flag at all. Update url, sha256 and version on each release
# with packaging/update-tap.sh.
class Pitboard < Formula
  desc "Park and restore your own Claude Code logins"
  homepage "https://github.com/datlechin/pitboard"
  url "https://github.com/datlechin/pitboard/archive/refs/tags/v0.1.2.tar.gz"
  sha256 "5e5a4fe59e2ade59e6dab973a5faa0011a7f8781a4e0aa3dfdad9e47457b0c4b"
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
