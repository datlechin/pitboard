# The Linux app's build scripts, run on changelogs and templates written for each case, and the
# release's check of its install tree, run on a tree laid out as Meson installs it, with the
# Plasma widget and without, and on copies broken for each rule.
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from xml.etree import ElementTree

REPO = Path(__file__).resolve().parents[2]
DATA = REPO / "apps" / "linux" / "data"
RELEASES = DATA / "metainfo-releases.py"
TEMPLATE = DATA / "com.usepitboard.Pitboard.metainfo.xml.in"
CHECK = REPO / ".github" / "scripts" / "check-linux-app.sh"
APP_ID = "com.usepitboard.Pitboard"
VALIDATORS = ("desktop-file-validate", "appstreamcli")
SMALL_TEMPLATE = """<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>com.usepitboard.Pitboard</id>
  @RELEASES@
</component>
"""
CHANGELOG = """# Changelog

## [Unreleased]

### Added

- Something nobody has been given yet.

## [1.0.0] - 2026-11-30

### Changed

- **Breaking:** A change of behaviour.

## [1.0.0-beta.2] - 2026-10-19

_Read [the notes](https://docs.usepitboard.com/notes) first._

### Added

- `pitboard doctor` names the [watch service](https://docs.usepitboard.com/guides/watch), and
  the app's **Diagnostics** says so too.
- A second change.

### Fixed

- A fix.

## [0.15.0] - 2026-10-17

### Added

- A release from before the Linux app.

[1.0.0]: https://github.com/datlechin/pitboard/compare/v1.0.0-beta.2...v1.0.0
[1.0.0-beta.2]: https://github.com/datlechin/pitboard/compare/v0.15.0...v1.0.0-beta.2
[0.15.0]: https://github.com/datlechin/pitboard/compare/v0.14.0...v0.15.0
"""


def run(template, changelog):
    with tempfile.TemporaryDirectory() as folder:
        written = Path(folder) / "template.xml.in", Path(folder) / "CHANGELOG.md"
        written[0].write_text(template, encoding="utf-8")
        written[1].write_text(changelog, encoding="utf-8")
        return subprocess.run(
            [sys.executable, "-I", RELEASES, *written], capture_output=True, text=True
        )


def made(template, changelog):
    result = run(template, changelog)
    if result.returncode != 0:
        raise AssertionError(result.stderr)
    return ElementTree.fromstring(result.stdout)


def said(element):
    return "".join(element.itertext())


class MetainfoReleases(unittest.TestCase):
    def test_each_1_x_release_is_a_release_in_its_order_and_the_rest_are_left_out(self):
        component = made(SMALL_TEMPLATE, CHANGELOG)
        releases = component.findall("releases/release")
        self.assertEqual(
            [(r.get("version"), r.get("date"), r.get("type")) for r in releases],
            [("1.0.0", "2026-11-30", "stable"), ("1.0.0-beta.2", "2026-10-19", "development")],
        )
        self.assertNotIn("nobody has been given", said(component))
        self.assertNotIn("before the Linux app", said(component))

    def test_a_releases_bullets_become_items_under_the_name_of_their_kind(self):
        description = made(SMALL_TEMPLATE, CHANGELOG).findall("releases/release/description")[1]
        self.assertEqual(
            [
                (child.tag, [said(item) for item in child] if child.tag == "ul" else said(child))
                for child in description
            ],
            [
                ("p", "Read the notes first."),
                ("p", "Added"),
                (
                    "ul",
                    [
                        "pitboard doctor names the watch service, and the app's Diagnostics "
                        "says so too.",
                        "A second change.",
                    ],
                ),
                ("p", "Fixed"),
                ("ul", ["A fix."]),
            ],
        )

    def test_links_become_their_text_and_code_and_bold_their_own_marks(self):
        description = made(SMALL_TEMPLATE, CHANGELOG).findall("releases/release/description")[1]
        first = description.find("ul/li")
        self.assertEqual([child.tag for child in first], ["code", "em"])
        self.assertEqual(first.find("code").text, "pitboard doctor")
        self.assertEqual(first.find("em").text, "Diagnostics")
        self.assertEqual(description.find("p/em").text, "Read the notes first.")
        self.assertNotIn("https://", said(description))
        breaking = made(SMALL_TEMPLATE, CHANGELOG).find("releases/release")
        self.assertEqual(breaking.find("description/ul/li/em").text, "Breaking:")

    def test_what_appstream_cannot_hold_is_refused(self):
        for line in ("  - A list inside a list.", "```sh", "#### A heading", "> A quote"):
            changelog = CHANGELOG.replace("- A second change.", f"- A second change.\n{line}")
            result = run(SMALL_TEMPLATE, changelog)
            self.assertEqual(result.returncode, 1, line)
            self.assertIn("1.0.0-beta.2", result.stderr, line)
            self.assertEqual(result.stdout, "", line)

    # AppStream takes a web address in a description for a link it cannot be, in a code span too.
    def test_a_web_address_in_a_description_is_refused(self):
        for line in ("- Run `curl -fsSL https://example.com/x.sh | sh`.", "- See http://example.com"):
            changelog = CHANGELOG.replace("- A fix.", line)
            result = run(SMALL_TEMPLATE, changelog)
            self.assertEqual(result.returncode, 1, line)
            self.assertIn("1.0.0-beta.2 holds a web address", result.stderr, line)
            self.assertEqual(result.stdout, "", line)

    def test_a_section_with_nothing_under_its_kind_is_refused(self):
        changelog = CHANGELOG.replace("### Fixed\n\n- A fix.\n", "### Fixed\n")
        result = run(SMALL_TEMPLATE, changelog)
        self.assertEqual(result.returncode, 1)
        self.assertIn("1.0.0-beta.2 has an empty Fixed", result.stderr)

    def test_a_template_without_its_place_for_the_releases_is_refused(self):
        for template in (SMALL_TEMPLATE.replace("@RELEASES@", ""), SMALL_TEMPLATE * 2):
            result = run(template, CHANGELOG)
            self.assertEqual(result.returncode, 1)
            self.assertIn("@RELEASES@ once", result.stderr)

    def test_the_apps_metainfo_holds_every_1_x_release_of_the_changelog(self):
        changelog = (REPO / "CHANGELOG.md").read_text(encoding="utf-8")
        component = made(TEMPLATE.read_text(encoding="utf-8"), changelog)
        versions = [r.get("version") for r in component.findall("releases/release")]
        headings = [
            line.split("]")[0][4:]
            for line in changelog.split("\n")
            if line.startswith("## [") and not line.startswith(("## [Unreleased]", "## [0."))
        ]
        self.assertTrue(headings)
        self.assertEqual(versions, headings)
        self.assertEqual(component.find("id").text, "com.usepitboard.Pitboard")


PROGRAM = "usr/bin/pitboard-app"
LAUNCHER = f"usr/share/applications/{APP_ID}.desktop"
SERVICE = f"usr/share/dbus-1/services/{APP_ID}.service"
METAINFO = f"usr/share/metainfo/{APP_ID}.metainfo.xml"
SEARCH = f"usr/share/gnome-shell/search-providers/{APP_ID}.search-provider.ini"
RUNNER = f"usr/share/krunner/dbusplugins/{APP_ID}.desktop"
MANUAL = "usr/share/man/man1/pitboard-app.1"
WIDGET = f"usr/share/plasma/plasmoids/{APP_ID}"
PLASMOID = REPO / "apps" / "linux" / "plasmoid" / APP_ID


def release_tree(root, widget=True):
    def put(path, text):
        (root / path).parent.mkdir(parents=True, exist_ok=True)
        (root / path).write_text(text, encoding="utf-8")

    put(PROGRAM, "\x7fELF the release's program\n")
    (root / PROGRAM).chmod(0o755)
    for source, path in ((f"{APP_ID}.desktop.in", LAUNCHER), (f"{APP_ID}.service.in", SERVICE)):
        put(path, (DATA / source).read_text(encoding="utf-8").replace("@BINDIR@", "/usr/bin"))
    made = subprocess.run(
        [sys.executable, "-I", RELEASES, TEMPLATE, REPO / "CHANGELOG.md"],
        capture_output=True,
        text=True,
        check=True,
    )
    put(METAINFO, made.stdout)
    put(SEARCH, (DATA / f"{APP_ID}.search-provider.ini").read_text(encoding="utf-8"))
    put(RUNNER, (DATA / "krunner" / f"{APP_ID}.desktop").read_text(encoding="utf-8"))
    put(MANUAL, (DATA / "pitboard-app.1").read_text(encoding="utf-8"))
    # The development build's icon is in the GResource alone.
    for icon in sorted((DATA / "icons").rglob("*.svg")):
        if ".Devel." not in icon.name:
            path = Path("usr/share/icons/hicolor") / icon.relative_to(DATA / "icons")
            put(path, icon.read_text(encoding="utf-8"))
    # Meson's plasma_widget, which a package can leave off.
    if widget:
        for source in sorted(path for path in PLASMOID.rglob("*") if path.is_file()):
            put(Path(WIDGET) / source.relative_to(PLASMOID), source.read_text(encoding="utf-8"))


class InstallTreeCheck(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        required = os.environ.get("TEST_LINUX_APP_TOOLS", "").split()
        missing = [name for name in required if shutil.which(name) is None]
        if missing:
            raise RuntimeError(f"TEST_LINUX_APP_TOOLS names {' '.join(missing)}, not on PATH")
        absent = [name for name in VALIDATORS if shutil.which(name) is None]
        if absent:
            raise unittest.SkipTest(f"no {' or '.join(absent)} on PATH")

    def tree(self, widget=True):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        root = Path(folder.name) / "destdir"
        release_tree(root, widget)
        return root

    def check(self, *arguments):
        return subprocess.run(["sh", CHECK, *arguments], capture_output=True, text=True)

    def edited(self, path, old, new):
        root = self.tree()
        text = (root / path).read_text(encoding="utf-8")
        self.assertIn(old, text)
        (root / path).write_text(text.replace(old, new, 1), encoding="utf-8")
        return root

    def assertRefused(self, root, says):
        result = self.check(root)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn(says, result.stderr)

    def test_the_tree_meson_installs_passes(self):
        result = self.check(self.tree())
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_tree_that_leaves_the_widget_to_a_package_of_its_own_passes(self):
        result = self.check(self.tree(widget=False))
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_program_built_with_the_fixtures_is_refused(self):
        root = self.tree()
        with open(root / PROGRAM, "a", encoding="utf-8") as program:
            program.write("dana@work.example\n")
        self.assertRefused(root, "was built without the fixture feature and holds a fixture")

    def test_a_file_that_names_the_debug_build_is_refused(self):
        for path, old, new in (
            (PROGRAM, "program", f"program {APP_ID}.Devel"),
            (PROGRAM, "program", "program pitboard-debug"),
            (LAUNCHER, "pitboard;", "pitboard;x-scheme-handler/pitboard-debug;"),
        ):
            with self.subTest(path=path, new=new):
                self.assertRefused(self.edited(path, old, new), f"{path} names the debug build")

    def test_a_file_outside_the_layout_is_refused(self):
        for path in (
            "usr/share/doc/pitboard-app/copyright",
            f"usr/share/icons/hicolor/scalable/apps/{APP_ID}.Devel.svg",
            "etc/xdg/autostart/com.usepitboard.Pitboard.desktop",
            f"{WIDGET}/contents/ui/CompactRepresentation.qml",
        ):
            with self.subTest(path=path):
                root = self.tree()
                (root / path).parent.mkdir(parents=True, exist_ok=True)
                (root / path).write_text("\n", encoding="utf-8")
                self.assertRefused(root, f"{path} is not in the release's layout")

    def test_a_file_of_the_layout_left_out_is_refused(self):
        for path in (RUNNER, PROGRAM, f"{WIDGET}/contents/ui/Feed.qml"):
            with self.subTest(path=path):
                root = self.tree()
                (root / path).unlink()
                self.assertRefused(root, f"{path} of the release's layout is missing")

    def test_a_launcher_or_runner_desktop_file_validate_refuses_is_refused(self):
        for path in (LAUNCHER, RUNNER):
            with self.subTest(path=path):
                root = self.edited(path, "Name=Pitboard\n", "")
                self.assertRefused(root, f"desktop-file-validate refuses {path}")

    def test_a_metainfo_appstreamcli_refuses_is_refused(self):
        summary = "  <summary>Switch AI coding tools between your accounts</summary>\n"
        self.assertRefused(self.edited(METAINFO, summary, ""), f"appstreamcli refuses {METAINFO}")

    def test_a_service_that_does_not_start_the_program_as_a_service_is_refused(self):
        for old, new in ((" --gapplication-service", ""), ("/usr/bin/", "/usr/local/bin/")):
            with self.subTest(new=new):
                self.assertRefused(
                    self.edited(SERVICE, old, new),
                    f"{SERVICE} does not start /usr/bin/pitboard-app --gapplication-service",
                )

    def test_a_tree_that_claims_another_name_or_scheme_is_refused(self):
        self.assertRefused(
            self.edited(SERVICE, f"Name={APP_ID}", "Name=com.usepitboard.Other"),
            f"{SERVICE} is for com.usepitboard.Other, not {APP_ID}",
        )
        self.assertRefused(
            self.edited(LAUNCHER, "pitboard;", "pitboard;x-scheme-handler/https;"),
            f"{LAUNCHER} handles x-scheme-handler/pitboard;x-scheme-handler/https;, "
            "not pitboard:// alone",
        )

    def test_a_search_that_would_start_pitboard_is_refused(self):
        for old, new in (("AutoStart=false", "AutoStart=true"), ("AutoStart=false\n", "")):
            with self.subTest(new=new):
                self.assertRefused(
                    self.edited(SEARCH, old, new), f"{SEARCH} has GNOME Shell start Pitboard"
                )
        self.assertRefused(
            self.edited(RUNNER, f"Service={APP_ID}*", f"Service={APP_ID}"),
            f"{RUNNER} has KRunner start Pitboard",
        )

    def test_a_widget_of_another_id_is_refused(self):
        metadata = f"{WIDGET}/metadata.json"
        self.assertRefused(
            self.edited(metadata, f'"Id": "{APP_ID}"', '"Id": "com.example.Other"'),
            f"{metadata} is the widget com.example.Other, not {APP_ID}",
        )
        self.assertRefused(
            self.edited(metadata, f'"Id": "{APP_ID}",', ""),
            f"{metadata} is the widget of no id, not {APP_ID}",
        )

    def test_no_tree_is_refused(self):
        usage = self.check()
        self.assertEqual(usage.returncode, 2)
        self.assertIn("usage: check-linux-app.sh <destdir>", usage.stderr)
        with tempfile.TemporaryDirectory() as empty:
            nothing = self.check(empty)
        self.assertEqual(nothing.returncode, 1)
        self.assertIn(f"there is no install tree under {empty}", nothing.stderr)


if __name__ == "__main__":
    unittest.main()
