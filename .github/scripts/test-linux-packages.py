# The release's check of its Debian and RPM packages, run on packages dpkg-deb or rpmbuild builds
# from trees laid out as debhelper or pitboard.spec lays out the release's, and on copies broken
# for each rule; the signing of the RPM packages; and the repository built and signed from such
# packages. Each format's cases need its packaging tools, which apps/linux/scripts/check.sh's
# packages-test step runs them with, on Debian 13 and on Fedora 43, and the repository's need
# install-deps.sh's repository set, on Ubuntu 26.04, as test-linux-repository.sh runs them.
import gzip
import hashlib
import importlib.util
import io
import lzma
import os
import shutil
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path
from xml.etree import ElementTree

REPO = Path(__file__).resolve().parents[2]
SCRIPTS = REPO / ".github" / "scripts"
CHECK = SCRIPTS / "check-linux-packages.sh"
REPOSITORY = SCRIPTS / "linux-repository.sh"
SIGN_REPOSITORY = SCRIPTS / "sign-linux-repository.sh"
SIGN_PACKAGES = SCRIPTS / "sign-linux-packages.sh"
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
APP_ID = "com.usepitboard.Pitboard"
ICON_SIZES = ("48x48", "64x64", "128x128")
REPOMD = {
    "repo": "http://linux.duke.edu/metadata/repo",
    "common": "http://linux.duke.edu/metadata/common",
}
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
PLASMA = "plasma-workspace (>= 4:6.4)"
RPM_PLASMA = "(plasma-workspace >= 6.4 or plasma6-workspace >= 6.4)"
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


# The Plasma widget's folder in the tree, as Meson installs it.
def widget_files(root, app):
    for source in sorted(path for path in app.PLASMOID.rglob("*") if path.is_file()):
        put(root, Path(app.WIDGET) / source.relative_to(app.PLASMOID), source.read_bytes())
    return root


# As dh_compress compresses: gzip -9n.
def compressed(data):
    return gzip.compress(data if isinstance(data, bytes) else data.encode(), 9, mtime=0)


# The tree as a Debian package with the control fields and the Description given, and the sums of
# its files dh_md5sums writes.
def deb_package(root, out, fields, description):
    files = sorted(
        path
        for path in root.rglob("*")
        if path.is_file() and path.relative_to(root).parts[0] != "DEBIAN"
    )
    sums = "".join(
        f"{hashlib.md5(path.read_bytes()).hexdigest()}  {path.relative_to(root)}\n"
        for path in files
    )
    control = "".join(f"{key}: {value}\n" for key, value in fields.items())
    put(root, "DEBIAN/md5sums", sums)
    put(root, "DEBIAN/control", f"{control}{description}\n")
    subprocess.run(
        ["dpkg-deb", "--root-owner-group", "--build", root, out], capture_output=True, check=True
    )


# The tree as an RPM package of the machine named, from a spec that installs it as it is, its
# licences as %license and its manual pages as rpmbuild compresses them. As pitboard.spec, nothing
# strips the stand-ins.
def rpm_package(root, top, fields, description, rpm_format=4, machine=None):
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
    if "BuildArch" in fields:
        requires += f"BuildArch: {fields['BuildArch']}\n"
    name = fields["Name"]
    spec = top / f"{name}.spec"
    put(
        top,
        spec.name,
        "%global debug_package %{nil}\n"
        "%global __brp_strip_comment_note %{nil}\n"
        f"Name: {name}\nVersion: {fields['Version']}\nRelease: {fields['Release']}\n"
        f"Summary: {fields['Summary']}\nLicense: Apache-2.0\n"
        f"URL: https://usepitboard.com\n{requires}"
        f"%description\n{description}\n"
        f"%install\ncp -a {root}/. %{{buildroot}}/\n"
        "%files\n" + "".join(f"{line}\n" for line in listed) + "%changelog\n"
        f"* Wed Oct 28 2026 {MAINTAINER} - {fields['Version']}-{fields['Release']}\n"
        f"- Pitboard {VERSION}.\n",
    )
    arguments = ["--define", f"_topdir {top}", "--define", f"_rpmformat {rpm_format}"]
    if machine is not None:
        arguments += ["--target", machine]
    subprocess.run(["rpmbuild", "-bb", *arguments, spec], capture_output=True, check=True)
    (built,) = (top / "RPMS").rglob("*.rpm")
    return built


def gpg(home, *arguments):
    return subprocess.run(
        ["gpg", "--batch", "--quiet", "--homedir", home, *arguments],
        capture_output=True,
        check=True,
    )


# A key laid out as the repository's, an RSA primary that only certifies and one signing subkey,
# neither with a passphrase, made in a fresh home. Its armored public key is written beside the
# home, and the case stops the home's agent once it is done.
def make_key(case, home, name):
    home.mkdir(mode=0o700)
    case.addClassCleanup(subprocess.run, ["gpgconf", "--homedir", home, "--kill", "all"])
    user = f"{name} <{name.lower()}@example.com>"
    gpg(home, "--passphrase", "", "--quick-gen-key", user, "rsa3072", "cert", "2y")
    listed = gpg(home, "--with-colons", "--list-keys").stdout.decode()
    primary = next(line.split(":")[9] for line in listed.splitlines() if line.startswith("fpr:"))
    gpg(home, "--passphrase", "", "--quick-add-key", primary, "rsa3072", "sign", "2y")
    public = home.parent / f"{home.name}.asc"
    public.write_bytes(gpg(home, "--armor", "--export", primary).stdout)
    return public


# A stand-in on PATH for a tool, as a script of the lines given.
def stand_in(folder, tool, *lines):
    folder.mkdir(exist_ok=True)
    put(folder, tool, "#!/bin/sh\n" + "".join(f"{line}\n" for line in lines), 0o755)
    return {**os.environ, "PATH": f"{folder}:{os.environ['PATH']}"}


# Each paragraph of a Debian control file, a field's later lines joined to its first.
def paragraphs(text):
    for block in text.strip().split("\n\n"):
        fields = {}
        for line in block.splitlines():
            if line[:1] in (" ", "\t"):
                fields[key] = f"{fields[key]}\n{line.strip()}".strip()
            else:
                key, _, value = line.partition(":")
                fields[key] = value.strip()
        yield fields


# The repository's metadata files by their type, as its repomd.xml names them.
def repodata(repository):
    root = ElementTree.parse(repository / "repodata" / "repomd.xml").getroot()
    return {
        data.get("type"): repository / data.find("repo:location", REPOMD).get("href")
        for data in root.findall("repo:data", REPOMD)
    }


def opened(path):
    data = path.read_bytes()
    if path.name.endswith(".zst"):
        from compression import zstd

        return zstd.decompress(data)
    if path.name.endswith(".gz"):
        return gzip.decompress(data)
    return data


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
            "pitboard-plasma-widget": self.widget_tree(),
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
            "pitboard-plasma-widget": {
                "Architecture": "all",
                "Depends": f"pitboard-app (= {DEB}), {PLASMA}",
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
        self.app.release_tree(root, widget=False)
        # lintian takes the tree's stand-in, which starts as an ELF file does, for a program
        # built for another architecture.
        put(root, self.app.PROGRAM, "#!/bin/sh\n# the release's program\n", 0o755)
        (root / self.app.MANUAL).unlink()
        put(root, f"{self.app.MANUAL}.gz", compressed(APP_MANUAL.read_bytes()))
        return root

    def widget_tree(self):
        return widget_files(self.tree("pitboard-plasma-widget"), self.app)

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
            named = f"{package}-v{version}-{self.fields[package]['Architecture']}.deb"
            deb_package(root, self.out / named, fields, self.descriptions[package])

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

    def test_a_widget_package_that_does_not_need_plasma_6_4_is_refused(self):
        package = f"pitboard-plasma-widget-v{VERSION}-all.deb"
        for plasma in (
            "",
            ", plasma-workspace",
            ", plasma-workspace (>= 4:6.3)",
            # Debian's Plasma is epoch 4, so 6.4 alone is below 4:5.27.
            ", plasma-workspace (>= 6.4)",
            ", plasma-workspace (>= 4:6.4) | plasma-desktop",
        ):
            with self.subTest(plasma=plasma):
                self.fresh()
                self.fields["pitboard-plasma-widget"]["Depends"] = f"pitboard-app (= {DEB}){plasma}"
                self.assertRefused(
                    f"{package} does not depend on plasma-workspace (>= 4:6.4), the first Plasma"
                )

    def test_a_widget_that_does_not_depend_on_its_own_app_is_refused(self):
        package = f"pitboard-plasma-widget-v{VERSION}-all.deb"
        for app in ("", f"pitboard-app (>= {DEB}), ", "pitboard-app (= 1.0.0~beta.6), "):
            with self.subTest(app=app):
                self.fresh()
                self.fields["pitboard-plasma-widget"]["Depends"] = f"{app}{PLASMA}"
                self.assertRefused(f"{package} does not depend on pitboard-app (= {DEB})")

    def test_a_widget_package_with_files_outside_its_folder_is_refused(self):
        package = f"pitboard-plasma-widget-v{VERSION}-all.deb"
        for path, says in (
            (
                "usr/share/plasma/plasmoids/org.example.Other/metadata.json",
                "is not in pitboard-plasma-widget's layout",
            ),
            ("usr/bin/pitboard-widget", "is not in pitboard-plasma-widget's layout"),
            (
                f"{self.app.WIDGET}/contents/ui/Extra.qml",
                f"{package}'s is not the release's install tree",
            ),
        ):
            with self.subTest(path=path):
                self.fresh()
                put(self.trees["pitboard-plasma-widget"], path, "\n")
                self.assertRefused(says)
        self.fresh()
        (self.trees["pitboard-plasma-widget"] / self.app.WIDGET / "metadata.json").unlink()
        self.assertRefused(f"{package} holds no /{self.app.WIDGET}/metadata.json")
        self.fresh()
        widget_files(self.trees["pitboard-app"], self.app)
        self.assertRefused(f"{self.app.WIDGET}/metadata.json is in more than one package")

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
        self.trees = {
            "pitboard": self.command_line_tree(),
            "pitboard-app": self.app_tree(),
            "pitboard-plasma-widget": widget_files(self.tree("pitboard-plasma-widget"), self.app),
        }
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
        self.fields["pitboard-plasma-widget"] |= {
            "Requires": [f"pitboard-app = {RPM}", RPM_PLASMA],
            "BuildArch": "noarch",
        }

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
        self.app.release_tree(root, widget=False)
        put(root, self.app.PROGRAM, self.programs["pitboard-app"], 0o755)
        return root

    def build(self, version=VERSION, rpm_format=4):
        self.out.mkdir(exist_ok=True)
        for package, root in self.trees.items():
            fields = {"Name": package, **self.fields[package]}
            top = self.work / "rpmbuild" / package
            built = rpm_package(root, top, fields, self.descriptions[package], rpm_format)
            machine = fields.get("BuildArch", self.machine)
            shutil.copy(built, self.out / f"{package}-v{version}-{machine}.rpm")

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
                machine = self.fields[package].get("BuildArch", self.machine)
                self.assertRefused(
                    f"{package}-v{VERSION}-{machine}.rpm is in RPM's format 6, not 4",
                    rpm_format=6,
                )

    def test_a_widget_package_that_does_not_need_plasma_6_4_is_refused(self):
        package = f"pitboard-plasma-widget-v{VERSION}-noarch.rpm"
        for plasma in (
            [],
            ["plasma-workspace >= 6.4"],
            ["plasma6-workspace >= 6.4"],
            ["(plasma-workspace >= 6.3 or plasma6-workspace >= 6.4)"],
            ["(plasma-workspace or plasma6-workspace >= 6.4)"],
        ):
            with self.subTest(plasma=plasma):
                self.fresh()
                requires = [f"pitboard-app = {RPM}", *plasma]
                self.fields["pitboard-plasma-widget"]["Requires"] = requires
                self.assertRefused(
                    f"{package} does not require (plasma-workspace >= 6.4 or plasma6-workspace >="
                )

    def test_a_widget_that_does_not_depend_on_its_own_app_is_refused(self):
        package = f"pitboard-plasma-widget-v{VERSION}-noarch.rpm"
        for app in ([], [f"pitboard-app >= {RPM}"], ["pitboard-app = 1.0.0~beta.6-1"]):
            with self.subTest(app=app):
                self.fresh()
                self.fields["pitboard-plasma-widget"]["Requires"] = [*app, RPM_PLASMA]
                self.assertRefused(f"{package} does not require pitboard-app = {RPM}")

    def test_a_widget_package_with_files_outside_its_folder_is_refused(self):
        package = f"pitboard-plasma-widget-v{VERSION}-noarch.rpm"
        for path, says in (
            (
                "usr/share/plasma/plasmoids/org.example.Other/metadata.json",
                "is not in pitboard-plasma-widget's layout",
            ),
            ("usr/bin/pitboard-widget", "is not in pitboard-plasma-widget's layout"),
            (
                f"{self.app.WIDGET}/contents/ui/Extra.qml",
                f"{package}'s is not the release's install tree",
            ),
        ):
            with self.subTest(path=path):
                self.fresh()
                put(self.trees["pitboard-plasma-widget"], path, "\n")
                self.assertRefused(says)
        self.fresh()
        (self.trees["pitboard-plasma-widget"] / self.app.WIDGET / "metadata.json").unlink()
        self.assertRefused(f"{package} holds no /{self.app.WIDGET}/metadata.json")
        self.fresh()
        widget_files(self.trees["pitboard-app"], self.app)
        self.assertRefused(f"{self.app.WIDGET}/metadata.json is in more than one package")


class Repository(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        needs(
            (
                "dpkg-deb",
                "rpmbuild",
                "apt-ftparchive",
                "createrepo_c",
                "modifyrepo_c",
                "appstream-generator",
                "appstreamcli",
                "jq",
                "gpg",
                "gpgv",
                "sqv",
            )
        )
        cls.app = app_tests()
        folder = tempfile.TemporaryDirectory()
        cls.addClassCleanup(folder.cleanup)
        cls.root = Path(folder.name)
        cls.home = cls.root / "gnupg"
        cls.key = make_key(cls, cls.home, "Repository")
        cls.beta = cls.release("1.0.0~beta.7")
        cls.final = cls.release("1.0.0")

    # A release's packages at the version, as the release names them, for each architecture: the
    # command line and the app of both formats, the app's metainfo edited where asked, and the
    # Plasma widget of both formats and Debian's keyring, for every architecture.
    @classmethod
    def release(cls, version, architectures=("amd64", "arm64"), metainfo=lambda text: text):
        work = Path(tempfile.mkdtemp(dir=cls.root))
        out = work / "packages"
        out.mkdir()
        tag = version.replace("~", "-")

        def tree(name):
            root = Path(tempfile.mkdtemp(dir=work))
            if name == "pitboard-app":
                cls.app.release_tree(root, widget=False)
                put(root, cls.app.PROGRAM, "#!/bin/sh\n# the release's program\n", 0o755)
                edited = metainfo((root / cls.app.METAINFO).read_text(encoding="utf-8"))
                if edited is None:
                    (root / cls.app.METAINFO).unlink()
                    (root / cls.app.LAUNCHER).unlink()
                else:
                    (root / cls.app.METAINFO).write_text(edited, encoding="utf-8")
            elif name == "pitboard":
                put(root, PROGRAM, STAND_IN, 0o755)
            elif name == "pitboard-plasma-widget":
                widget_files(root, cls.app)
            else:
                put(root, KEYRING, KEY.read_bytes())
            return root

        def deb(name, architecture, **fields):
            fields = {
                "Package": name,
                "Version": version,
                "Architecture": architecture,
                "Maintainer": MAINTAINER,
                **fields,
            }
            deb_package(tree(name), out / f"{name}-v{tag}-{architecture}.deb", fields,
                        f"Description: Pitboard's {name}")

        def rpm(name, machine, *requires):
            fields = {
                "Name": name,
                "Version": version,
                "Release": "1",
                "Summary": f"Pitboard's {name}",
                "Requires": list(requires),
            }
            if machine == "noarch":
                fields["BuildArch"] = machine
            top = Path(tempfile.mkdtemp(dir=work))
            built = rpm_package(
                tree(name), top, fields, f"Pitboard's {name}.",
                machine=None if machine == "noarch" else machine,
            )
            shutil.copy(built, out / f"{name}-v{tag}-{machine}.rpm")

        deb("pitboard-archive-keyring", "all", Source="pitboard")
        deb("pitboard-plasma-widget", "all", Source="pitboard",
            Depends=f"pitboard-app (= {version}), {PLASMA}")
        rpm("pitboard-plasma-widget", "noarch", f"pitboard-app = {version}-1", RPM_PLASMA)
        for architecture in architectures:
            machine = MACHINES[architecture]
            deb("pitboard", architecture)
            deb("pitboard-app", architecture, Source="pitboard", Depends=f"pitboard (= {version})")
            rpm("pitboard", machine)
            rpm("pitboard-app", machine, f"pitboard = {version}-1")
        return out

    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.work = Path(folder.name)

    def build(self, suites, packages):
        out = Path(tempfile.mkdtemp(dir=self.work)) / "repository"
        result = subprocess.run(
            ["sh", REPOSITORY, suites, packages, self.key, out], capture_output=True, text=True
        )
        return result, out

    def built(self, suites, packages):
        result, out = self.build(suites, packages)
        self.assertEqual(result.returncode, 0, result.stderr)
        return out

    def sign(self, tree, key, env=None):
        env = {**(env or os.environ), "GNUPGHOME": str(self.home)}
        return subprocess.run(
            ["sh", SIGN_REPOSITORY, tree, key], capture_output=True, text=True, env=env
        )

    def assertRefused(self, result, says):
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn(says, result.stderr)

    # The app's component in a catalog, which names its package.
    def the_app(self, catalog):
        components = ElementTree.fromstring(catalog).findall("component")
        (component,) = [found for found in components if found.findtext("id") == APP_ID]
        self.assertEqual(component.findtext("pkgname"), "pitboard-app")
        return component

    def test_stable_never_lists_a_beta_or_a_release_candidate(self):
        candidate = self.release("1.2.0~rc1", architectures=("amd64",))
        for packages, version in ((self.beta, "1.0.0~beta.7"), (candidate, "1.2.0~rc1")):
            for suites in ("stable", "stable beta", "beta stable"):
                with self.subTest(version=version, suites=suites):
                    result, out = self.build(suites, packages)
                    self.assertRefused(
                        result,
                        "the stable suite never lists a beta or a release candidate, and the "
                        f"release's packages are version {version}",
                    )
                    self.assertFalse(out.exists())
        out = self.built("beta", self.beta)
        self.assertTrue((out / "deb/dists/beta/Release").is_file())
        self.assertFalse((out / "deb/dists/stable").exists())
        out = self.built("stable", self.final)
        self.assertTrue((out / "deb/dists/stable/Release").is_file())

    def test_a_suite_lists_only_the_packages_it_was_given(self):
        out = self.built("beta", self.beta)
        version = "1.0.0~beta.7"
        self.assertEqual(sorted(path.name for path in (out / "deb/dists").iterdir()), ["beta"])
        self.assertEqual(sorted(path.name for path in (out / "rpm").iterdir()), ["beta"])
        filenames = {
            (name, architecture): f"pool/main/p/pitboard/{name}_{version}_{architecture}.deb"
            for name, architecture in [
                ("pitboard-archive-keyring", "all"),
                ("pitboard-plasma-widget", "all"),
                *[(name, a) for a in MACHINES for name in ("pitboard", "pitboard-app")],
            ]
        }
        pool = sorted(str(path.relative_to(out / "deb")) for path in out.glob("deb/pool/**/*.deb"))
        self.assertEqual(pool, sorted(filenames.values()))
        for architecture in MACHINES:
            index = out / f"deb/dists/beta/main/binary-{architecture}"
            listed = sorted(
                (fields["Package"], fields["Version"], fields["Filename"])
                for fields in paragraphs((index / "Packages").read_text(encoding="utf-8"))
            )
            self.assertEqual(
                listed,
                sorted(
                    (name, version, filename)
                    for (name, of), filename in filenames.items()
                    if of in (architecture, "all")
                ),
            )
            plain = (index / "Packages").read_bytes()
            self.assertEqual(gzip.decompress((index / "Packages.gz").read_bytes()), plain)
            self.assertEqual(lzma.decompress((index / "Packages.xz").read_bytes()), plain)
        for machine in MACHINES.values():
            repository = out / "rpm/beta" / machine
            primary = ElementTree.fromstring(opened(repodata(repository)["primary"]))
            listed = sorted(
                (
                    package.findtext("common:name", namespaces=REPOMD),
                    package.find("common:version", REPOMD).get("ver"),
                    package.find("common:version", REPOMD).get("rel"),
                    package.findtext("common:arch", namespaces=REPOMD),
                    package.find("common:location", REPOMD).get("href"),
                )
                for package in primary.findall("common:package", REPOMD)
            )
            # The widget, for every machine, is in each machine's repository.
            expected = [
                (name, version, "1", arch, f"Packages/{name}-{version}-1.{arch}.rpm")
                for name, arch in (
                    ("pitboard", machine),
                    ("pitboard-app", machine),
                    ("pitboard-plasma-widget", "noarch"),
                )
            ]
            self.assertEqual(listed, expected)
            self.assertEqual(
                sorted(f"Packages/{path.name}" for path in (repository / "Packages").iterdir()),
                [href for *_, href in expected],
            )
        mixed = self.work / "mixed"
        shutil.copytree(self.beta, mixed)
        shutil.copy(self.final / "pitboard-app-v1.0.0-amd64.deb", mixed)
        result, out = self.build("beta", mixed)
        self.assertRefused(result, "a suite lists one release")
        self.assertFalse(out.exists())

    # apt fetches a catalog's file only where Release names it uncompressed, as it does Packages.
    def test_the_release_file_names_every_index_and_catalog_with_its_hash(self):
        out = self.built("stable beta", self.final)
        for suite in ("stable", "beta"):
            with self.subTest(suite=suite):
                dists = out / "deb/dists" / suite
                (release,) = paragraphs((dists / "Release").read_text(encoding="utf-8"))
                self.assertEqual(
                    {key: release.get(key) for key in (
                        "Origin", "Label", "Suite", "Codename", "Architectures", "Components",
                        "Acquire-By-Hash", "Valid-Until",
                    )},
                    {
                        "Origin": "Pitboard",
                        "Label": "Pitboard",
                        "Suite": suite,
                        "Codename": suite,
                        "Architectures": "amd64 arm64",
                        "Components": "main",
                        "Acquire-By-Hash": "yes",
                        "Valid-Until": None,
                    },
                )
                self.assertEqual(
                    [key for key in ("MD5Sum", "SHA1", "SHA256", "SHA512") if key in release],
                    ["SHA256"],
                )
                named = {}
                for line in release["SHA256"].splitlines():
                    digest, size, name = line.split()
                    named[name] = (digest, int(size))
                served = {
                    str(path.relative_to(dists))
                    for path in dists.rglob("*")
                    if path.is_file() and "by-hash" not in path.parts and path.name != "Release"
                }
                self.assertEqual(set(named), served)
                catalogs = [f"Components-{arch}.yml" for arch in MACHINES]
                catalogs += [f"icons-{size}.tar" for size in ICON_SIZES]
                for name in (
                    "main/binary-amd64/Packages.xz",
                    "main/binary-arm64/Packages.xz",
                    "main/Contents-amd64.gz",
                    "main/Contents-arm64.gz",
                    *[f"main/dep11/{catalog}{end}" for catalog in catalogs for end in ("", ".gz")],
                ):
                    self.assertIn(name, named)
                for name, (digest, size) in named.items():
                    data = (dists / name).read_bytes()
                    self.assertEqual((hashlib.sha256(data).hexdigest(), len(data)), (digest, size))
                    by_hash = (dists / name).parent / "by-hash" / "SHA256" / digest
                    self.assertEqual(by_hash.read_bytes(), data, name)

    def test_an_index_the_committed_key_does_not_verify_stops_the_build(self):
        out = self.built("beta", self.beta)

        def signatures():
            return sorted(
                str(path.relative_to(out))
                for path in out.rglob("*")
                if path.name in ("InRelease", "Release.gpg")
                or path.name == "repomd.xml.asc"
            )

        result = self.sign(out, KEY)
        self.assertRefused(result, "could not sign")
        self.assertEqual(signatures(), [])
        for tool in ("sqv", "gpgv"):
            with self.subTest(tool=tool):
                env = stand_in(self.work / tool, tool, "exit 1")
                self.assertRefused(self.sign(out, self.key, env), "do not verify against")
                self.assertEqual(signatures(), [])
        result = self.sign(out, self.key)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            signatures(),
            [
                "deb/dists/beta/InRelease",
                "deb/dists/beta/Release.gpg",
                "rpm/beta/aarch64/repodata/repomd.xml.asc",
                "rpm/beta/x86_64/repodata/repomd.xml.asc",
            ],
        )
        keyring = out / "pitboard-archive-keyring.pgp"
        for signature, data in (
            ("deb/dists/beta/Release.gpg", "deb/dists/beta/Release"),
            ("rpm/beta/x86_64/repodata/repomd.xml.asc", "rpm/beta/x86_64/repodata/repomd.xml"),
        ):
            verified = subprocess.run(
                ["sqv", "--keyring", keyring, "--signature-file", out / signature, out / data],
                capture_output=True,
            )
            self.assertEqual(verified.returncode, 0, verified.stderr)
        self.assertRefused(self.sign(out, self.key), "a tree is signed once")

    def test_the_catalog_holds_the_app_with_its_icon(self):
        out = self.built("beta", self.beta)
        dep11 = out / "deb/dists/beta/main/dep11"
        for architecture in MACHINES:
            with self.subTest(architecture=architecture):
                catalog = self.work / f"Components-{architecture}.xml"
                subprocess.run(
                    ["appstreamcli", "convert", dep11 / f"Components-{architecture}.yml.gz",
                     catalog],
                    capture_output=True,
                    check=True,
                )
                icons = self.the_app(catalog.read_bytes()).findall("icon[@type='cached']")
                self.assertEqual(sorted(icon.get("width") for icon in icons), ["128", "48", "64"])
                for icon in icons:
                    size = f"{icon.get('width')}x{icon.get('height')}"
                    with tarfile.open(dep11 / f"icons-{size}.tar.gz") as tar:
                        self.assertIn(icon.text, tar.getnames())
        for machine in MACHINES.values():
            data = repodata(out / "rpm/beta" / machine)
            for catalog, tarball in (
                ("appstream", "appstream-icons"),
                ("appdata", "appdata-icons"),
            ):
                with self.subTest(machine=machine, catalog=catalog):
                    icons = self.the_app(opened(data[catalog])).findall("icon[@type='cached']")
                    self.assertEqual(len(icons), 3)
                    # A client takes an icon tarball whose name names no size to hold each size
                    # in its folder.
                    self.assertFalse([size for size in ICON_SIZES if size in data[tarball].name])
                    with tarfile.open(data[tarball]) as tar:
                        names = tar.getnames()
                    for icon in icons:
                        size = f"{icon.get('width')}x{icon.get('height')}"
                        self.assertIn(f"{size}/{icon.text}", names)

    def test_an_error_among_the_catalogs_hints_stops_the_build(self):
        packages = self.release(
            "1.0.0~beta.7", ("amd64",), lambda text: text.replace("</summary>", "", 1)
        )
        result, out = self.build("beta", packages)
        self.assertRefused(result, "appstream-generator found errors in the app's metadata")
        self.assertIn("pitboard-app/1.0.0~beta.7/amd64", result.stderr)
        self.assertFalse(out.exists())

    def test_a_catalog_without_the_app_stops_the_build(self):
        packages = self.release("1.0.0~beta.7", ("amd64",), lambda text: None)
        result, out = self.build("beta", packages)
        self.assertRefused(result, f"the amd64 catalog does not hold {APP_ID}")
        self.assertFalse(out.exists())


class RpmSigning(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        needs(("rpmbuild", "rpmsign", "rpmkeys", "gpg"))
        folder = tempfile.TemporaryDirectory()
        cls.addClassCleanup(folder.cleanup)
        root = Path(folder.name)
        cls.home = root / "gnupg"
        cls.key = make_key(cls, cls.home, "Repository")
        cls.other_home = root / "other"
        cls.other = make_key(cls, cls.other_home, "Other")
        cls.rpmsign = shutil.which("rpmsign")

    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.work = Path(folder.name)
        self.out = self.work / "packages"
        self.out.mkdir()
        root = self.work / "tree"
        put(root, PROGRAM, STAND_IN, 0o755)
        fields = {
            "Name": "pitboard",
            "Version": RPM_VERSION,
            "Release": "1",
            "Summary": "Pitboard's command line",
            "Requires": [],
        }
        built = rpm_package(root, self.work / "rpmbuild", fields, "Pitboard's command line.")
        self.package = self.out / f"pitboard-v{VERSION}-{built.name.split('.')[-2]}.rpm"
        shutil.copy(built, self.package)
        self.deb = self.out / f"pitboard-v{VERSION}-arm64.deb"
        self.deb.write_bytes(b"!<arch>\nnot read\n")

    def sign(self, env=None):
        env = {**(env or os.environ), "GNUPGHOME": str(self.home)}
        return subprocess.run(
            ["sh", SIGN_PACKAGES, self.out, self.key], capture_output=True, text=True, env=env
        )

    # rpm's verdict on the package with the key alone in its database, at the level that refuses
    # one no key signed.
    def verified(self, key):
        database = self.work / f"rpmdb-{key.stem}"
        subprocess.run(["rpmkeys", "--dbpath", database, "--import", key], check=True)
        return subprocess.run(
            ["rpmkeys", "--dbpath", database, "--define", "_pkgverify_level signature",
             "--checksig", self.package],
            capture_output=True,
        ).returncode == 0

    def test_the_releases_rpms_are_signed_and_its_debs_left_as_they_are(self):
        result = self.sign()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(self.verified(self.key))
        self.assertFalse(self.verified(self.other))
        self.assertEqual(self.deb.read_bytes(), b"!<arch>\nnot read\n")

    def test_an_unsigned_or_wrongly_signed_rpm_stops_the_signing(self):
        listed = gpg(self.other_home, "--with-colons", "--list-keys").stdout.decode()
        other = next(line.split(":")[9] for line in listed.splitlines() if line.startswith("fpr:"))
        for case, lines in (
            ("unsigned", ["exit 0"]),
            (
                "wrongly signed",
                [
                    f"GNUPGHOME={self.other_home} exec {self.rpmsign}"
                    f' "$@" --define "_gpg_name {other}"'
                ],
            ),
        ):
            with self.subTest(case=case):
                self.setUp()
                result = self.sign(stand_in(self.work / "bin", "rpmsign", *lines))
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn(f"{self.package} is not signed by {self.key}", result.stderr)
                self.assertFalse(self.verified(self.key))


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

    def test_the_repository_takes_its_suites_the_packages_a_key_and_a_folder(self):
        for arguments in (
            ("beta", "packages", "key.asc"),
            ("beta", "packages", "key.asc", "out", "more"),
            ("", "packages", "key.asc", "out"),
            ("stable,beta", "packages", "key.asc", "out"),
        ):
            with self.subTest(arguments=arguments):
                result = subprocess.run(
                    ["sh", REPOSITORY, *arguments], capture_output=True, text=True
                )
                self.assertEqual(result.returncode, 2)
                self.assertIn("usage: linux-repository.sh", result.stderr)
        for suites, says in (
            ("nightly", "nightly is not a suite of the repository, which has stable and beta"),
            ("beta beta", "beta is named twice"),
        ):
            with self.subTest(suites=suites):
                result = subprocess.run(
                    ["sh", REPOSITORY, suites, "packages", "key.asc", "out"],
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, 1)
                self.assertIn(says, result.stderr)

    def test_the_signing_scripts_take_a_folder_and_a_key(self):
        for script, usage in (
            (SIGN_REPOSITORY, "usage: sign-linux-repository.sh"),
            (SIGN_PACKAGES, "usage: sign-linux-packages.sh"),
        ):
            for arguments in (("tree",), ("tree", "key.asc", "more")):
                with self.subTest(script=script.name, arguments=arguments):
                    result = subprocess.run(
                        ["sh", script, *arguments], capture_output=True, text=True
                    )
                    self.assertEqual(result.returncode, 2)
                    self.assertIn(usage, result.stderr)


if __name__ == "__main__":
    unittest.main()
