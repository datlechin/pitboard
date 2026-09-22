# Homebrew formula for the tap datlechin/homebrew-tap.
#
# This is the template. The release's `tap` job writes the version and the checksum into it
# and commits the result to the tap, so the two lines below are never the ones anybody
# installs and this file makes no claim about any release. The checksum it writes is the
# line the release put in SHA256SUMS, over the source tarball it published itself.
#
# It builds from source, so one formula serves Linux and both macOS architectures without a
# bottle for each.
class Pitboard < Formula
  desc "Park and restore your own Claude Code logins"
  homepage "https://usepitboard.com"
  version "0.0.0"
  url "https://github.com/datlechin/pitboard/releases/download/v#{version}/pitboard-v#{version}-source.tar.gz"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"
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
