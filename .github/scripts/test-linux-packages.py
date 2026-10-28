# The release's check of its Debian packages, run on packages dpkg-deb builds from trees laid out
# as debhelper lays out the release's, and on copies broken for each rule. It needs Debian's
# packaging tools, which apps/linux/scripts/check.sh's packages-test step runs it with.
import gzip
import hashlib
import importlib.util
import io
import os
import shutil
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SCRIPTS = REPO / ".github" / "scripts"
CHECK = SCRIPTS / "check-linux-packages.sh"
DEBIAN = REPO / "packaging" / "linux" / "debian"
KEY = REPO / "packaging" / "linux" / "pitboard-archive-keyring.asc"
APP_MANUAL = REPO / "apps" / "linux" / "data" / "pitboard-app.1"
TOOLS = ("dpkg", "dpkg-deb", "lintian", "gpg", "desktop-file-validate", "appstreamcli")
VERSION = "1.0.0-beta.7"
DEB = "1.0.0~beta.7"
MACHINES = {"amd64": "x86_64", "arm64": "aarch64"}
MAINTAINER = "Ngo Quoc Dat <datlechin@gmail.com>"
FLOOR = "libadwaita-1-0 (>= 1.7), libgtk-4-1 (>= 4.18), libglib2.0-0t64 (>= 2.84)"
PROGRAM = "usr/bin/pitboard"
BASH = "usr/share/bash-completion/completions/pitboard"
ZSH = "usr/share/zsh/vendor-completions/_pitboard"
FISH = "usr/share/fish/vendor_completions.d/pitboard.fish"
MANUAL = "usr/share/man/man1/pitboard.1.gz"
KEYRING = "usr/share/keyrings/pitboard-archive-keyring.pgp"
# A stand-in for the command line, and the man page and completions it writes.
STAND_IN = "#!/bin/sh\n# pitboard 1.0.0-beta.7\n"
WRITTEN = {
    "pitboard.1": '.TH pitboard 1 "pitboard 1.0.0-beta.7"\n.SH NAME\npitboard \\- switch\n',
    "completions/pitboard.bash": "# pitboard's bash completions\n",
    "completions/pitboard.zsh": "#compdef pitboard\n",
    "completions/pitboard.fish": "# pitboard's fish completions\n",
}


def app_tests():
    spec = importlib.util.spec_from_file_location("test_linux_app", SCRIPTS / "test-linux-app.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# Each package's Description, as debian/control writes it.
def descriptions():
    found = {}
    for paragraph in (DEBIAN / "control").read_text(encoding="utf-8").split("\n\n"):
        lines = [line for line in paragraph.splitlines() if not line.startswith("#")]
        if not lines or not lines[0].startswith("Package: "):
            continue
        at = next(i for i, line in enumerate(lines) if line.startswith("Description: "))
        end = next((i for i in range(at + 1, len(lines)) if not lines[i].startswith(" ")), None)
        found[lines[0].removeprefix("Package: ")] = "\n".join(lines[at:end])
    return found


def put(root, path, data, mode=0o644):
    file = root / path
    file.parent.mkdir(parents=True, exist_ok=True)
    file.write_bytes(data if isinstance(data, bytes) else data.encode())
    file.chmod(mode)


# As dh_compress compresses: gzip -9n.
def compressed(data):
    return gzip.compress(data if isinstance(data, bytes) else data.encode(), 9, mtime=0)


class PackagesCheck(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        required = os.environ.get("TEST_LINUX_PACKAGES_TOOLS", "").split()
        missing = [name for name in required if shutil.which(name) is None]
        if missing:
            raise RuntimeError(f"TEST_LINUX_PACKAGES_TOOLS names {' '.join(missing)}, not on PATH")
        absent = [name for name in TOOLS if shutil.which(name) is None]
        if absent:
            raise unittest.SkipTest(f"no {', '.join(absent)} on PATH")
        cls.app = app_tests()
        cls.architecture = subprocess.run(
            ["dpkg", "--print-architecture"], capture_output=True, text=True, check=True
        ).stdout.strip()
        cls.descriptions = descriptions()
        cls.unarmored = subprocess.run(
            ["gpg", "--dearmor"], input=KEY.read_bytes(), capture_output=True, check=True
        ).stdout

    def setUp(self):
        self.fresh()

    # A command line tarball and the release's three package trees, each in a folder of its own.
    def fresh(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.work = Path(folder.name)
        self.out = self.work / "packages"
        name = f"pitboard-v{VERSION}-{MACHINES[self.architecture]}-unknown-linux-musl"
        self.tarball = self.work / f"{name}.tar.gz"
        with tarfile.open(self.tarball, "w:gz") as tar:
            for path, text in {"pitboard": STAND_IN, **WRITTEN}.items():
                member = tarfile.TarInfo(f"{name}/{path}")
                member.size = len(text.encode())
                member.mode = 0o755 if path == "pitboard" else 0o644
                tar.addfile(member, io.BytesIO(text.encode()))
        self.trees = {
            "pitboard": self.command_line_tree(),
            "pitboard-app": self.app_tree(),
            "pitboard-archive-keyring": self.keyring_tree(),
        }
        self.fields = {
            "pitboard": {
                "Architecture": self.architecture,
                "Recommends": "pitboard-archive-keyring",
            },
            "pitboard-app": {
                "Architecture": self.architecture,
                "Depends": f"pitboard (= {DEB}), {FLOOR}",
            },
            "pitboard-archive-keyring": {"Architecture": "all", "Multi-Arch": "foreign"},
        }

    def tree(self, package):
        root = self.work / "trees" / package
        put(root, f"usr/share/doc/{package}/copyright", (DEBIAN / "copyright").read_bytes())
        changelog = (
            f"pitboard ({DEB}) stable; urgency=medium\n\n  * Pitboard {VERSION}.\n\n"
            f" -- {MAINTAINER}  Wed, 28 Oct 2026 15:46:18 +0000\n"
        )
        put(root, f"usr/share/doc/{package}/changelog.gz", compressed(changelog))
        return root

    def command_line_tree(self):
        root = self.tree("pitboard")
        put(root, PROGRAM, STAND_IN, 0o755)
        put(root, MANUAL, compressed(WRITTEN["pitboard.1"]))
        put(root, BASH, WRITTEN["completions/pitboard.bash"])
        put(root, ZSH, WRITTEN["completions/pitboard.zsh"])
        put(root, FISH, WRITTEN["completions/pitboard.fish"])
        overrides = (DEBIAN / "pitboard.lintian-overrides").read_bytes()
        put(root, "usr/share/lintian/overrides/pitboard", overrides)
        return root

    def app_tree(self):
        root = self.tree("pitboard-app")
        self.app.release_tree(root)
        # lintian takes the tree's stand-in, which starts as an ELF file does, for a program
        # built for another architecture.
        put(root, self.app.PROGRAM, "#!/bin/sh\n# the release's program\n", 0o755)
        (root / self.app.MANUAL).unlink()
        put(root, f"{self.app.MANUAL}.gz", compressed(APP_MANUAL.read_bytes()))
        return root

    def keyring_tree(self):
        root = self.tree("pitboard-archive-keyring")
        put(root, KEYRING, self.unarmored)
        return root

    def build(self, version=VERSION):
        self.out.mkdir(exist_ok=True)
        for package, root in self.trees.items():
            files = sorted(path for path in root.rglob("*") if path.is_file())
            fields = {
                "Package": package,
                "Version": DEB,
                "Maintainer": MAINTAINER,
                "Installed-Size": str(sum(-(-path.stat().st_size // 1024) for path in files)),
                "Section": "utils",
                "Priority": "optional",
                "Homepage": "https://usepitboard.com",
                **self.fields[package],
            }
            control = "".join(f"{key}: {value}\n" for key, value in fields.items())
            sums = "".join(
                f"{hashlib.md5(path.read_bytes()).hexdigest()}  {path.relative_to(root)}\n"
                for path in files
            )
            put(root, "DEBIAN/control", f"{control}{self.descriptions[package]}\n")
            put(root, "DEBIAN/md5sums", sums)
            named = f"{package}-v{version}-{self.fields[package]['Architecture']}.deb"
            subprocess.run(
                ["dpkg-deb", "--root-owner-group", "--build", root, self.out / named],
                capture_output=True,
                check=True,
            )

    def check(self, *arguments):
        return subprocess.run(["sh", CHECK, *arguments], capture_output=True, text=True)

    def assertRefused(self, says):
        self.build()
        result = self.check(self.out, self.tarball, VERSION)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn(says, result.stderr)

    def test_the_releases_packages_pass(self):
        self.build()
        result = self.check(self.out, self.tarball, VERSION)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_command_line_that_is_not_the_tarballs_is_refused(self):
        package = f"pitboard-v{VERSION}-{self.architecture}.deb"
        for path, packed, data, mode in (
            (PROGRAM, "pitboard", STAND_IN + "# another build\n", 0o755),
            (BASH, "completions/pitboard.bash", "# other completions\n", 0o644),
            (FISH, "completions/pitboard.fish", "# other completions\n", 0o644),
            (ZSH, "completions/pitboard.zsh", "#compdef other\n", 0o644),
            (MANUAL, "pitboard.1", compressed(".TH other 1\n.SH NAME\nother \\- other\n"), 0o644),
        ):
            with self.subTest(path=path):
                self.fresh()
                put(self.trees["pitboard"], path, data, mode)
                self.assertRefused(
                    f"{package}'s /{path} is not the command line tarball's {packed}"
                )

    def test_an_app_that_does_not_depend_on_its_own_command_line_is_refused(self):
        package = f"pitboard-app-v{VERSION}-{self.architecture}.deb"
        for depends in (
            FLOOR,
            f"pitboard (>= {DEB}), {FLOOR}",
            f"pitboard (= 1.0.0~beta.6), {FLOOR}",
            f"pitboard (= {DEB}) | pitboard-other, {FLOOR}",
        ):
            with self.subTest(depends=depends):
                self.fresh()
                self.fields["pitboard-app"]["Depends"] = depends
                self.assertRefused(f"{package} does not depend on pitboard (= {DEB})")

    def test_an_app_without_the_floors_libraries_is_refused(self):
        package = f"pitboard-app-v{VERSION}-{self.architecture}.deb"
        glib = "libglib2.0-0t64 (>= 2.84)"
        for depends, library, floor in (
            (f"libadwaita-1-0 (>= 1.7), {glib}", "libgtk-4-1", "4.18"),
            (f"libadwaita-1-0 (>= 1.7), libgtk-4-1 (>= 4.16.0), {glib}", "libgtk-4-1", "4.18"),
            (f"libadwaita-1-0, libgtk-4-1 (>= 4.18.6), {glib}", "libadwaita-1-0", "1.7"),
            (
                "libadwaita-1-0 (>= 1.7), libgtk-4-1 (>= 4.18), libglib2.0-0t64 (<< 3)",
                "libglib2.0-0t64",
                "2.84",
            ),
        ):
            with self.subTest(depends=depends):
                self.fresh()
                self.fields["pitboard-app"]["Depends"] = f"pitboard (= {DEB}), {depends}"
                self.assertRefused(
                    f"{package} does not depend on {library} (>= {floor}), the floor's"
                )

    def test_a_file_outside_the_layout_is_refused(self):
        autostart = "etc/xdg/autostart/com.usepitboard.Pitboard.desktop"
        for package, path, says in (
            ("pitboard", "usr/share/doc/pitboard/README", "is not in pitboard's layout"),
            ("pitboard", "usr/sbin/pitboard", "is not in pitboard's layout"),
            (
                "pitboard-archive-keyring",
                "etc/apt/trusted.gpg.d/pitboard.gpg",
                "is not in pitboard-archive-keyring's layout",
            ),
            ("pitboard-app", autostart, "is not in the release's layout"),
            (
                "pitboard-app",
                "usr/share/doc/pitboard-app/README",
                "not its changelog and copyright alone",
            ),
            ("pitboard-app", PROGRAM, f"{PROGRAM} is in more than one package"),
        ):
            with self.subTest(package=package, path=path):
                self.fresh()
                put(self.trees[package], path, "\n")
                self.assertRefused(says)

    def test_a_keyring_that_is_not_the_committed_key_is_refused(self):
        package = f"pitboard-archive-keyring-v{VERSION}-all.deb"
        for data in (KEY.read_bytes(), self.unarmored[:-1]):
            with self.subTest(size=len(data)):
                self.fresh()
                put(self.trees["pitboard-archive-keyring"], KEYRING, data)
                self.assertRefused(
                    f"{package}'s /{KEYRING} is not packaging/linux/pitboard-archive-keyring.asc"
                )

    def test_a_package_lintian_warns_about_is_refused(self):
        (self.trees["pitboard"] / "usr/share/doc/pitboard/copyright").chmod(0o600)
        self.assertRefused("lintian warns about the packages")

    def test_a_version_other_than_the_releases_is_refused(self):
        self.build(version="1.0.0-beta.6")
        result = self.check(self.out, self.tarball, VERSION)
        self.assertEqual(result.returncode, 1, result.stderr)
        app = f"pitboard-app-v{VERSION}-{self.architecture}.deb"
        other = f"pitboard-app-v1.0.0-beta.6-{self.architecture}.deb"
        self.assertIn(f"{app} of the release is missing", result.stderr)
        self.assertIn(f"{other} is not a package of the release", result.stderr)
        self.fresh()
        self.fields["pitboard"]["Version"] = VERSION
        package = f"pitboard-v{VERSION}-{self.architecture}.deb"
        self.assertRefused(f"{package} is version {VERSION}, not {DEB}")

    def test_it_takes_three_arguments(self):
        for arguments in ((self.out, self.tarball), (self.out, self.tarball, VERSION, VERSION)):
            with self.subTest(arguments=len(arguments)):
                result = self.check(*arguments)
                self.assertEqual(result.returncode, 2)
                self.assertIn("usage: check-linux-packages.sh", result.stderr)


if __name__ == "__main__":
    unittest.main()
