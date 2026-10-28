# The Linux app's build scripts, run on changelogs and templates written for each case.
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


if __name__ == "__main__":
    unittest.main()
