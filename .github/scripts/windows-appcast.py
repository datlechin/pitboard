# Writes appcast-windows.xml, the feed every Release build of the Windows app reads through
# WinSparkle: one item per architecture of a full release, its MSI signed with the Windows update
# key. A beta or a release candidate gets no feed, so a beta updates itself only into a full
# release.
import base64
import re
import sys
from pathlib import Path
from xml.sax.saxutils import quoteattr

REPO = Path(__file__).resolve().parents[2]
APP = REPO / "apps" / "windows" / "Pitboard.App" / "Pitboard.App.csproj"
ARCHITECTURES = ("x64", "arm64")
# LAUNCHAFTERINSTALL has the MSI start Pitboard again once it has replaced it. /passive alone
# restarts Windows unasked when a file in use, such as the watch service's pitboard.exe, needs
# a restart to be replaced.
INSTALLER_ARGUMENTS = "/passive /norestart LAUNCHAFTERINSTALL=1"
FULL_RELEASE = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")


class Refused(Exception):
    pass


# The app's own floor, so a release that raises it is never offered to an older Windows.
def minimum_windows():
    found = re.search(
        r"<SupportedOSPlatformVersion>([0-9]+\.[0-9]+\.[0-9]+)(\.[0-9]+)?"
        r"</SupportedOSPlatformVersion>",
        APP.read_text(encoding="utf-8"),
    )
    if not found:
        raise Refused(f"{APP} names no SupportedOSPlatformVersion")
    return found.group(1)


def signature(text, name):
    try:
        raw = base64.b64decode(text, validate=True)
    except ValueError:
        raw = b""
    if len(raw) != 64:
        raise Refused(f"{name}: '{text}' is not an EdDSA signature, the base64 of 64 bytes")
    return text


def feed(version, downloads, folder, signatures):
    if not FULL_RELEASE.fullmatch(version):
        raise Refused(
            f"{version} is not a full release, X.Y.Z: only a full release has a Windows feed, "
            "which a beta reads to update into it"
        )
    if not downloads.startswith("https://") or not downloads.endswith("/"):
        raise Refused(f"{downloads} is not an https:// address ending in /")
    windows = minimum_windows()
    items = []
    for arch, signed in zip(ARCHITECTURES, signatures):
        name = f"Pitboard-v{version}-windows-{arch}.msi"
        msi = folder / name
        if not msi.is_file():
            raise Refused(f"no {name} in {folder}")
        items.append(
            "    <item>\n"
            f"      <title>Pitboard {version}</title>\n"
            f"      <sparkle:version>{version}</sparkle:version>\n"
            f"      <sparkle:shortVersionString>{version}</sparkle:shortVersionString>\n"
            f"      <sparkle:minimumSystemVersion>{windows}</sparkle:minimumSystemVersion>\n"
            f"      <enclosure url={quoteattr(downloads + name)} length=\"{msi.stat().st_size}\""
            f' type="application/x-msi" sparkle:os="windows-{arch}"'
            f" sparkle:installerArguments={quoteattr(INSTALLER_ARGUMENTS)}"
            f" sparkle:edSignature={quoteattr(signature(signed, name))}/>\n"
            "    </item>\n"
        )
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">\n'
        "  <channel>\n"
        "    <title>Pitboard for Windows</title>\n" + "".join(items) + "  </channel>\n"
        "</rss>\n"
    )


def main(argv):
    if len(argv) != 6:
        print(
            "usage: windows-appcast.py <version> <download address ending in /> <folder of MSIs>"
            " <x64 signature> <arm64 signature>",
            file=sys.stderr,
        )
        return 2
    try:
        written = feed(argv[1], argv[2], Path(argv[3]), argv[4:])
    except Refused as refused:
        print(refused, file=sys.stderr)
        return 1
    sys.stdout.write(written)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
