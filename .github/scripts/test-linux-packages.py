# The release's check of its Debian and RPM packages, run on packages dpkg-deb or rpmbuild builds
# from trees laid out as debhelper or pitboard.spec lays out the release's, and on copies broken
# for each rule. Each format's cases need its packaging tools, which apps/linux/scripts/check.sh's
# packages-test step runs them with, on Debian 13 and on Fedora 43.
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
SPEC = REPO / "packaging" / "linux" / "pitboard.spec"
KEY = REPO / "packaging" / "linux" / "pitboard-archive-keyring.asc"
APP_MANUAL = REPO / "apps" / "linux" / "data" / "pitboard-app.1"
VALIDATORS = ("desktop-file-validate", "appstreamcli")
VERSION = "1.0.0-beta.7"
DEB = "1.0.0~beta.7"
RPM_VERSION = "1.0.0~beta.7"
RPM = f"{RPM_VERSION}-1"
MACHINES = {"amd64": "x86_64", "arm64": "aarch64"}
MAINTAINER = "Ngo Quoc Dat <datlechin@gmail.com>"
FLOOR = "libadwaita-1-0 (>= 1.7), libgtk-4-1 (>= 4.18), libglib2.0-0t64 (>= 2.84)"
RPM_FLOOR = ("libadwaita-1.so.0()(64bit)", "libgtk-4.so.1()(64bit)", "libglib-2.0.so.0()(64bit)")
# The names Fedora's packages and openSUSE's provide at their versions, at the floor's.
RPM_FLOOR_VERSIONS = ("libadwaita >= 1.7", "gtk4 >= 4.18", "glib2 >= 2.84")
PROGRAM = "usr/bin/pitboard"
BASH = "usr/share/bash-completion/completions/pitboard"
ZSH = "usr/share/zsh/vendor-completions/_pitboard"
RPM_ZSH = "usr/share/zsh/site-functions/_pitboard"
FISH = "usr/share/fish/vendor_completions.d/pitboard.fish"
MANUAL = "usr/share/man/man1/pitboard.1.gz"
KEYRING = "usr/share/keyrings/pitboard-archive-keyring.pgp"
# A stand-in for the command line, and the man page and completions it writes.
STAND_IN = "#!/bin/sh\n# pitboard 1.0.0-beta.7\n"
# rpmlint reads the programs of an RPM package as the release's, so their stand-ins are programs
# too: the command line's static, linking nothing, not even a C library, and the app's
# position-independent and linking the system's C library, as Cargo links the app.
STAND_INS = {
    "pitboard": ("void _start(void) { for (;;) {} }\n", ["-static", "-nostdlib"]),
    "pitboard-app": ("int main(void) { return 0; }\n", ["-fPIE", "-pie"]),
}
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


# Each package's Summary and %description, as pitboard.spec writes them.
def spec_texts():
    lines = SPEC.read_text(encoding="utf-8").splitlines()
    summaries, descriptions = {}, {}
    package = "pitboard"
    for at, line in enumerate(lines):
        if line.startswith("%package -n "):
            package = line.removeprefix("%package -n ")
        elif line.startswith("Summary: "):
            summaries[package] = line.removeprefix("Summary: ")
        elif line.startswith("%description"):
            named = line.removeprefix("%description").strip().removeprefix("-n ") or "pitboard"
            end = next(i for i in range(at + 1, len(lines)) if lines[i].startswith("%"))
            descriptions[named] = "\n".join(lines[at + 1 : end]).strip()
    return summaries, descriptions


# needs(tools): skips the class where a tool is missing, and fails where
# TEST_LINUX_PACKAGES_TOOLS names a tool that is missing.
def needs(tools):
    required = os.environ.get("TEST_LINUX_PACKAGES_TOOLS", "").split()
    missing = [name for name in required if shutil.which(name) is None]
    if missing:
        raise RuntimeError(f"TEST_LINUX_PACKAGES_TOOLS names {' '.join(missing)}, not on PATH")
    absent = [name for name in tools if shutil.which(name) is None]
    if absent:
        raise unittest.SkipTest(f"no {', '.join(absent)} on PATH")


# The release's command line tarball for the machine, with the program and what it writes.
def command_line_tarball(work, machine, program):
    name = f"pitboard-v{VERSION}-{machine}-unknown-linux-musl"
    tarball = work / f"{name}.tar.gz"
    with tarfile.open(tarball, "w:gz") as tar:
        for path, data in {"pitboard": program, **WRITTEN}.items():
            data = data if isinstance(data, bytes) else data.encode()
            member = tarfile.TarInfo(f"{name}/{path}")
            member.size = len(data)
            member.mode = 0o755 if path == "pitboard" else 0o644
            tar.addfile(member, io.BytesIO(data))
    return tarball


def check(*arguments):
    return subprocess.run(["sh", CHECK, *arguments], capture_output=True, text=True)


def put(root, path, data, mode=0o644):
    file = root / path
    file.parent.mkdir(parents=True, exist_ok=True)
    file.write_bytes(data if isinstance(data, bytes) else data.encode())
    file.chmod(mode)


# As dh_compress compresses: gzip -9n.
def compressed(data):
    return gzip.compress(data if isinstance(data, bytes) else data.encode(), 9, mtime=0)


class DebianPackagesCheck(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        needs(("dpkg", "dpkg-deb", "lintian", "gpg", *VALIDATORS))
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
        self.tarball = command_line_tarball(self.work, MACHINES[self.architecture], STAND_IN)
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

    def assertRefused(self, says):
        self.build()
        result = check("deb", self.out, self.tarball, VERSION)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn(says, result.stderr)

    def test_the_releases_packages_pass(self):
        self.build()
        result = check("deb", self.out, self.tarball, VERSION)
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
                "not usr/share/doc/pitboard-app/changelog.gz usr/share/doc/pitboard-app/copyright",
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
        result = check("deb", self.out, self.tarball, VERSION)
        self.assertEqual(result.returncode, 1, result.stderr)
        app = f"pitboard-app-v{VERSION}-{self.architecture}.deb"
        other = f"pitboard-app-v1.0.0-beta.6-{self.architecture}.deb"
        self.assertIn(f"{app} of the release is missing", result.stderr)
        self.assertIn(f"{other} is not a package of the release", result.stderr)
        self.fresh()
        self.fields["pitboard"]["Version"] = VERSION
        package = f"pitboard-v{VERSION}-{self.architecture}.deb"
        self.assertRefused(f"{package} is version {VERSION}, not {DEB}")



class RpmPackagesCheck(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        needs(("rpm", "rpmbuild", "rpmlint", "rpm2archive", "gcc", *VALIDATORS))
        cls.app = app_tests()
        cls.machine = subprocess.run(
            ["rpm", "--eval", "%{_arch}"], capture_output=True, text=True, check=True
        ).stdout.strip()
        cls.summaries, cls.descriptions = spec_texts()
        cls.programs = {}
        with tempfile.TemporaryDirectory() as folder:
            for package, (text, flags) in STAND_INS.items():
                source = Path(folder) / f"{package}.c"
                source.write_text(text, encoding="utf-8")
                program = Path(folder) / package
                subprocess.run(
                    ["gcc", *flags, "-s", "-o", program, source], capture_output=True, check=True
                )
                cls.programs[package] = program.read_bytes()

    def setUp(self):
        self.fresh()

    # A command line tarball and the release's two package trees, each in a folder of its own,
    # as Meson and the spec's %install lay them out, before rpmbuild compresses the man pages.
    def fresh(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.work = Path(folder.name)
        self.out = self.work / "packages"
        self.tarball = command_line_tarball(self.work, self.machine, self.programs["pitboard"])
        self.trees = {"pitboard": self.command_line_tree(), "pitboard-app": self.app_tree()}
        self.fields = {
            package: {
                "Version": RPM_VERSION,
                "Release": "1",
                "Summary": self.summaries[package],
                "Requires": [],
            }
            for package in self.trees
        }
        self.fields["pitboard-app"]["Requires"] = [
            f"pitboard = {RPM}", *RPM_FLOOR, *RPM_FLOOR_VERSIONS
        ]

    def tree(self, package):
        root = self.work / "trees" / package
        for name in ("LICENSE", "NOTICE"):
            put(root, f"usr/share/licenses/{package}/{name}", (REPO / name).read_bytes())
        return root

    def command_line_tree(self):
        root = self.tree("pitboard")
        put(root, PROGRAM, self.programs["pitboard"], 0o755)
        put(root, MANUAL.removesuffix(".gz"), WRITTEN["pitboard.1"])
        put(root, BASH, WRITTEN["completions/pitboard.bash"])
        put(root, RPM_ZSH, WRITTEN["completions/pitboard.zsh"])
        put(root, FISH, WRITTEN["completions/pitboard.fish"])
        return root

    def app_tree(self):
        root = self.tree("pitboard-app")
        self.app.release_tree(root)
        put(root, self.app.PROGRAM, self.programs["pitboard-app"], 0o755)
        return root

    # Each tree as an RPM package of its own, from a spec that installs it as it is.
    def build(self, version=VERSION, rpm_format=4):
        self.out.mkdir(exist_ok=True)
        for package, root in self.trees.items():
            fields = self.fields[package]
            listed = []
            files = sorted(path.relative_to(root) for path in root.rglob("*") if path.is_file())
            for path in files:
                if path.parts[:3] == ("usr", "share", "licenses"):
                    listed.append(f"%license /{path}")
                elif path.parts[:3] == ("usr", "share", "man"):
                    listed.append(f"/{path}*")
                else:
                    listed.append(f"/{path}")
            requires = "".join(f"Requires: {name}\n" for name in fields["Requires"])
            top = self.work / "rpmbuild" / package
            spec = top / f"{package}.spec"
            # As pitboard.spec, nothing strips the stand-ins.
            put(
                top,
                spec.name,
                "%global debug_package %{nil}\n"
                "%global __brp_strip_comment_note %{nil}\n"
                f"Name: {package}\nVersion: {fields['Version']}\nRelease: {fields['Release']}\n"
                f"Summary: {fields['Summary']}\nLicense: Apache-2.0\n"
                f"URL: https://usepitboard.com\n{requires}"
                f"%description\n{self.descriptions[package]}\n"
                f"%install\ncp -a {root}/. %{{buildroot}}/\n"
                "%files\n" + "".join(f"{line}\n" for line in listed) + "%changelog\n"
                f"* Wed Oct 28 2026 {MAINTAINER} - {fields['Version']}-{fields['Release']}\n"
                f"- Pitboard {VERSION}.\n",
            )
            subprocess.run(
                [
                    "rpmbuild",
                    "-bb",
                    "--define",
                    f"_topdir {top}",
                    "--define",
                    f"_rpmformat {rpm_format}",
                    spec,
                ],
                capture_output=True,
                check=True,
            )
            (built,) = (top / "RPMS").rglob("*.rpm")
            shutil.copy(built, self.out / f"{package}-v{version}-{self.machine}.rpm")

    def assertRefused(self, says, **built):
        self.build(**built)
        result = check("rpm", self.out, self.tarball, VERSION)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn(says, result.stderr)

    def test_the_releases_packages_pass(self):
        self.build()
        result = check("rpm", self.out, self.tarball, VERSION)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_command_line_that_is_not_the_tarballs_is_refused(self):
        package = f"pitboard-v{VERSION}-{self.machine}.rpm"
        for path, packed, data, mode in (
            (PROGRAM, "pitboard", self.programs["pitboard"] + b"\0", 0o755),
            (BASH, "completions/pitboard.bash", "# other completions\n", 0o644),
            (FISH, "completions/pitboard.fish", "# other completions\n", 0o644),
            (RPM_ZSH, "completions/pitboard.zsh", "#compdef other\n", 0o644),
            (MANUAL.removesuffix(".gz"), "pitboard.1", ".TH other 1\n.SH NAME\nother\n", 0o644),
        ):
            with self.subTest(path=path):
                self.fresh()
                put(self.trees["pitboard"], path, data, mode)
                shown = MANUAL if path.startswith("usr/share/man/") else path
                self.assertRefused(
                    f"{package}'s /{shown} is not the command line tarball's {packed}"
                )

    def test_an_app_that_does_not_depend_on_its_own_command_line_is_refused(self):
        package = f"pitboard-app-v{VERSION}-{self.machine}.rpm"
        for requires in (
            [],
            [f"pitboard >= {RPM}"],
            [f"pitboard = {RPM_VERSION}"],
            ["pitboard = 1.0.0~beta.6-1"],
            [f"(pitboard = {RPM} or pitboard-other)"],
        ):
            with self.subTest(requires=requires):
                self.fresh()
                self.fields["pitboard-app"]["Requires"] = [
                    *requires, *RPM_FLOOR, *RPM_FLOOR_VERSIONS
                ]
                self.assertRefused(f"{package} does not require pitboard = {RPM}")

    def test_an_app_without_the_floors_libraries_is_refused(self):
        package = f"pitboard-app-v{VERSION}-{self.machine}.rpm"
        for library in RPM_FLOOR:
            for stated in ([], [library.removesuffix("(64bit)")]):
                with self.subTest(library=library, stated=stated):
                    self.fresh()
                    others = [name for name in RPM_FLOOR if name != library]
                    requires = [f"pitboard = {RPM}", *others, *stated, *RPM_FLOOR_VERSIONS]
                    self.fields["pitboard-app"]["Requires"] = requires
                    self.assertRefused(f"{package} does not require {library}, the floor's")
        # The sonames alone hold no floor: an rpm system with GTK 4.16 meets them.
        for floor in RPM_FLOOR_VERSIONS:
            name, _, least = floor.split()
            for stated in ([], [name], [f"{name} >= 1"], [f"({floor} or {name}-other)"]):
                with self.subTest(floor=floor, stated=stated):
                    self.fresh()
                    others = [other for other in RPM_FLOOR_VERSIONS if other != floor]
                    requires = [f"pitboard = {RPM}", *RPM_FLOOR, *others, *stated]
                    self.fields["pitboard-app"]["Requires"] = requires
                    self.assertRefused(f"{package} does not require {floor}, the floor's")

    def test_an_app_that_requires_more_than_the_floor_passes(self):
        self.fields["pitboard-app"]["Requires"] = [
            f"pitboard = {RPM}", *RPM_FLOOR, "libadwaita >= 1.8", "gtk4 >= 4.18.6", "glib2 >= 2.84"
        ]
        self.build()
        result = check("rpm", self.out, self.tarball, VERSION)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_file_outside_the_layout_is_refused(self):
        autostart = "etc/xdg/autostart/com.usepitboard.Pitboard.desktop"
        documents = "usr/share/licenses/pitboard-app/LICENSE usr/share/licenses/pitboard-app/NOTICE"
        for package, path, says in (
            ("pitboard", "usr/share/doc/pitboard/README", "is not in pitboard's layout"),
            ("pitboard", "usr/sbin/pitboard", "is not in pitboard's layout"),
            ("pitboard", ZSH, f"{ZSH} is not in pitboard's layout"),
            ("pitboard-app", autostart, "is not in the release's layout"),
            ("pitboard-app", "usr/share/doc/pitboard-app/README", f"not {documents}"),
            ("pitboard-app", PROGRAM, f"{PROGRAM} is in more than one package"),
        ):
            with self.subTest(package=package, path=path):
                self.fresh()
                put(self.trees[package], path, "\n")
                self.assertRefused(says)

    def test_a_package_rpmlint_warns_about_is_refused(self):
        self.fields["pitboard"]["Summary"] += "."
        self.assertRefused("rpmlint warns about the packages")

    def test_a_version_other_than_the_releases_is_refused(self):
        self.build(version="1.0.0-beta.6")
        result = check("rpm", self.out, self.tarball, VERSION)
        self.assertEqual(result.returncode, 1, result.stderr)
        app = f"pitboard-app-v{VERSION}-{self.machine}.rpm"
        other = f"pitboard-app-v1.0.0-beta.6-{self.machine}.rpm"
        self.assertIn(f"{app} of the release is missing", result.stderr)
        self.assertIn(f"{other} is not a package of the release", result.stderr)
        package = f"pitboard-v{VERSION}-{self.machine}.rpm"
        for field, value, says in (
            ("Version", "1.0.0~beta.6", f"1.0.0~beta.6-1, not {RPM}"),
            ("Release", "2", f"{RPM_VERSION}-2, not {RPM}"),
        ):
            with self.subTest(field=field):
                self.fresh()
                self.fields["pitboard"][field] = value
                self.assertRefused(f"{package} is version {says}")

    def test_a_package_in_rpm_format_6_is_refused(self):
        for package in self.trees:
            with self.subTest(package=package):
                self.assertRefused(
                    f"{package}-v{VERSION}-{self.machine}.rpm is in RPM's format 6, not 4",
                    rpm_format=6,
                )


class Arguments(unittest.TestCase):
    def test_it_takes_a_format_and_three_arguments(self):
        for arguments in (
            ("deb", "packages", "tarball"),
            ("rpm", "packages", "tarball", VERSION, VERSION),
            ("apk", "packages", "tarball", VERSION),
            ("packages", "tarball", VERSION),
        ):
            with self.subTest(arguments=arguments):
                result = check(*arguments)
                self.assertEqual(result.returncode, 2)
                self.assertIn("usage: check-linux-packages.sh", result.stderr)


if __name__ == "__main__":
    unittest.main()
