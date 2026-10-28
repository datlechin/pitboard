# The release workflow's scripts, run on what a tag and crates.io could hand them.
import base64
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import tomllib
import unittest
from pathlib import Path
from xml.etree import ElementTree

REPO = Path(__file__).resolve().parents[2]
CORE_UNCHANGED = REPO / ".github" / "scripts" / "core-unchanged.sh"
RELEASE_KIND = REPO / ".github" / "scripts" / "release-kind.sh"
BUILD_NUMBER = REPO / "apps" / "macos" / "scripts" / "build-number.sh"
WINGET_HASHES = REPO / ".github" / "scripts" / "check-winget-hashes.py"
WINDOWS_APPCAST = REPO / ".github" / "scripts" / "windows-appcast.py"
WINDOWS_VERSIONS = REPO / ".github" / "scripts" / "windows-versions.sh"
WINDOWS_UPDATE_KEY = REPO / ".github" / "scripts" / "windows-update-key.sh"
WINDOWS_UPDATE_SIGN = REPO / ".github" / "scripts" / "windows-update-sign.sh"
SESSION_GATE = REPO / ".github" / "scripts" / "session-gate.sh"
LINUX_VERSIONS = REPO / ".github" / "scripts" / "linux-versions.sh"
LINUX_REPOSITORY_KEY = REPO / ".github" / "scripts" / "linux-repository-key.sh"
PACK_COMMAND_LINE = REPO / ".github" / "scripts" / "pack-command-line.sh"
PACK_LINUX_SESSION_BUNDLE = REPO / ".github" / "scripts" / "pack-linux-session-bundle.sh"
LINUX_RELEASE = REPO / ".github" / "scripts" / "linux-release.sh"
WITH_LINUX_SIGNING_KEY = REPO / ".github" / "scripts" / "with-linux-signing-key.sh"
UPLOAD_LINUX_REPOSITORY = REPO / ".github" / "scripts" / "upload-linux-repository.sh"
REPOSITORY_KEY = REPO / "packaging" / "linux" / "pitboard-archive-keyring.asc"
WINDOWS_APP = REPO / "apps" / "windows" / "Pitboard.App" / "Pitboard.App.csproj"
PACKAGE_WINDOWS = REPO / ".github" / "scripts" / "package-windows.ps1"
SCRIPTS = REPO / ".github" / "scripts"
WORKFLOWS = REPO / ".github" / "workflows"
WINDOWS_TARGETS = ("x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc")
TARBALL_TARGETS = (
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
)
KEYS = ["version", "base", "kind", "prerelease", "windows", "crates", "latest", "signed"]
NOT_A_TAG = "is not a release tag: vX.Y.Z, vX.Y.Z-rcN or vX.Y.Z-beta.N"
NO_SIGNER = (
    "Pitboard for Windows has no signer yet, and a release candidate or final release is never "
    "unsigned."
)
SIGNPATH = "CN=SignPath Foundation, O=SignPath Foundation, L=Lewes, S=Delaware, C=US"
NORMALIZED = """[package]
name = "pitboard-core"
version = "0.15.0"

[dependencies.ureq]
version = "3"
features = [
    "rustls-no-provider",
    "platform-verifier",
]
default-features = false
"""
NO_BUILD_NUMBER = (
    "has no build number: it takes X.Y.Z or X.Y.Z-beta.N, with Y and Z at most 99 and N from 1 "
    "to 98"
)
NO_WINDOWS_VERSIONS = (
    "has no Windows version numbers: it takes X.Y.Z or X.Y.Z-beta.N, with X and Y at most 255, "
    "Z at most 65535 and N from 1 to 999"
)
ONLY_FROM_A_BETA = (
    "the session bundle is built only from a branch whose Pitboard version is 1.0.0-beta.1 or "
    "later"
)
NO_PACKAGE_VERSION = (
    "has no package version: it takes X.Y.Z, X.Y.Z-rcN or, from 1.0.0, X.Y.Z-beta.N with N from "
    "1 to 98"
)


def run(*args):
    return subprocess.run(["sh", *map(str, args)], capture_output=True, text=True)


# A case that needs a tool skips where it is missing, and fails where TEST_RELEASE_TOOLS names it,
# as CI's Linux leg does.
def needs(test, *tools):
    named = os.environ.get("TEST_RELEASE_TOOLS", "").split()
    for tool in tools:
        if shutil.which(tool) is None:
            if tool in named:
                test.fail(f"TEST_RELEASE_TOOLS names {tool}, which is not on PATH")
            test.skipTest(f"{tool} is not on PATH")


# Ed25519 as RFC 8032's section 6 writes it, Sparkle's and WinSparkle's signatures, so a test
# signs with a key it makes and reads a signature back without a library.
ED_P = 2**255 - 19
ED_L = 2**252 + 27742317777372353535851937790883648493
ED_D = -121665 * pow(121666, ED_P - 2, ED_P) % ED_P
ED_ROOT = pow(2, (ED_P - 1) // 4, ED_P)


def ed_add(a, b):
    x1, y1, z1, t1 = a
    x2, y2, z2, t2 = b
    e1 = (y1 - x1) * (y2 - x2) % ED_P
    e2 = (y1 + x1) * (y2 + x2) % ED_P
    e3 = t1 * 2 * ED_D * t2 % ED_P
    e4 = z1 * 2 * z2 % ED_P
    e, f, g, h = e2 - e1, e4 - e3, e4 + e3, e2 + e1
    return (e * f % ED_P, g * h % ED_P, f * g % ED_P, e * h % ED_P)


def ed_times(n, point):
    total = (0, 1, 1, 0)
    while n:
        if n & 1:
            total = ed_add(total, point)
        point = ed_add(point, point)
        n >>= 1
    return total


def ed_x(y, sign):
    if y >= ED_P:
        return None
    xx = (y * y - 1) * pow(ED_D * y * y + 1, ED_P - 2, ED_P)
    if xx == 0:
        return None if sign else 0
    x = pow(xx, (ED_P + 3) // 8, ED_P)
    if (x * x - xx) % ED_P:
        x = x * ED_ROOT % ED_P
    if (x * x - xx) % ED_P:
        return None
    return ED_P - x if (x & 1) != sign else x


ED_BASE_Y = 4 * pow(5, ED_P - 2, ED_P) % ED_P
ED_BASE = (ed_x(ED_BASE_Y, 0), ED_BASE_Y, 1, ed_x(ED_BASE_Y, 0) * ED_BASE_Y % ED_P)


def ed_encode(point):
    x, y, z, _ = point
    inverse = pow(z, ED_P - 2, ED_P)
    x, y = x * inverse % ED_P, y * inverse % ED_P
    return (y | (x & 1) << 255).to_bytes(32, "little")


def ed_decode(raw):
    y = int.from_bytes(raw, "little")
    sign, y = y >> 255, y & ((1 << 255) - 1)
    x = ed_x(y, sign)
    return None if x is None else (x, y, 1, x * y % ED_P)


def ed_hash(*parts):
    return int.from_bytes(hashlib.sha512(b"".join(parts)).digest(), "little")


def ed_secret(seed):
    digest = hashlib.sha512(seed).digest()
    scalar = int.from_bytes(digest[:32], "little") & ((1 << 254) - 8) | (1 << 254)
    return scalar, digest[32:]


def ed_public(seed):
    return ed_encode(ed_times(ed_secret(seed)[0], ED_BASE))


def ed_sign(seed, message):
    scalar, prefix = ed_secret(seed)
    public = ed_encode(ed_times(scalar, ED_BASE))
    r = ed_hash(prefix, message) % ED_L
    nonce = ed_encode(ed_times(r, ED_BASE))
    s = (r + ed_hash(nonce, public, message) * scalar) % ED_L
    return nonce + s.to_bytes(32, "little")


def ed_verify(public, message, signature):
    if len(public) != 32 or len(signature) != 64:
        return False
    key, nonce = ed_decode(public), ed_decode(signature[:32])
    s = int.from_bytes(signature[32:], "little")
    if key is None or nonce is None or s >= ED_L:
        return False
    h = ed_hash(signature[:32], public, message) % ED_L
    left, right = ed_times(s, ED_BASE), ed_add(nonce, ed_times(h, key))
    return (left[0] * right[2] - right[0] * left[2]) % ED_P == 0 and (
        left[1] * right[2] - right[1] * left[2]
    ) % ED_P == 0


class CoreUnchanged(unittest.TestCase):
    def setUp(self):
        self.work = Path(tempfile.mkdtemp())

    def tearDown(self):
        shutil.rmtree(self.work)

    def crate(self, name, changes=None, top="pitboard-core-0.15.0"):
        files = {
            "src/lib.rs": "pub mod host;\n",
            "src/host/mod.rs": "pub fn os() {}\n",
            "Cargo.toml.orig": (
                '[package]\nname = "pitboard-core"\nversion = "0.15.0"\n\n'
                "[dependencies]\nureq.workspace = true\n"
            ),
            "Cargo.toml": NORMALIZED,
            "Cargo.lock": "version = 4\n",
            "README.md": "Pitboard\n",
            ".cargo_vcs_info.json": '{"git": {"sha1": "af8cdf19"}}\n',
        }
        files.update(changes or {})
        path = self.work / f"{name}.crate"
        with tarfile.open(path, "w:gz") as tar:
            for file, text in files.items():
                if text is None:
                    continue
                data = text.encode()
                info = tarfile.TarInfo(f"{top}/{file}")
                info.size = len(data)
                tar.addfile(info, io.BytesIO(data))
        return path

    def compared(self, published, packaged=None):
        return run(CORE_UNCHANGED, published, packaged or self.crate("packaged"))

    def test_the_same_source_and_manifests_pass(self):
        published = self.crate(
            "published",
            {
                "Cargo.lock": "version = 3\n",
                "README.md": "An older README\n",
                ".cargo_vcs_info.json": '{"git": {"sha1": "22722731"}}\n',
            },
        )
        done = self.compared(published)
        self.assertEqual(done.returncode, 0, done.stderr)

    def test_a_change_without_a_new_version_is_refused(self):
        for changes in (
            {"src/host/mod.rs": "pub fn os() -> u8 { 0 }\n"},
            {"src/host/mod.rs": None},
            {"src/extra.rs": "\n"},
            {"Cargo.toml.orig": '[package]\nname = "pitboard-core"\nversion.workspace = true\n'},
            # Only the root's [workspace.dependencies] moved: ureq lost a feature.
            {"Cargo.toml": NORMALIZED.replace('    "platform-verifier",\n', "")},
        ):
            with self.subTest(changes=list(changes)):
                done = self.compared(self.crate("published", changes))
                self.assertEqual(done.returncode, 1)
                self.assertIn(
                    "pitboard-core-0.15.0 on crates.io is not what this tree packages: raise the "
                    "version in crates/pitboard-core/Cargo.toml",
                    done.stderr,
                )

    def test_it_refuses_what_is_not_one_crate(self):
        empty = self.work / "empty.crate"
        with tarfile.open(empty, "w:gz"):
            pass
        unnamed = self.crate("unnamed", {"Cargo.toml.orig": None})
        pairs = ((empty, None), (unnamed, None), (self.crate("published"), empty))
        for published, packaged in pairs:
            with self.subTest(published=published.name, packaged=packaged and packaged.name):
                done = self.compared(published, packaged)
                self.assertEqual(done.returncode, 1)
                self.assertIn("is not one crate", done.stderr)

    def test_it_takes_two_arguments(self):
        done = run(CORE_UNCHANGED, self.crate("published"))
        self.assertEqual(done.returncode, 2)
        self.assertIn("usage:", done.stderr)


class ReleaseKind(unittest.TestCase):
    def setUp(self):
        self.work = Path(tempfile.mkdtemp())

    def tearDown(self):
        shutil.rmtree(self.work)

    def kind(self, tag, manifest, published=(), signer="signpath", subject=SIGNPATH):
        listed = self.work / "published"
        listed.write_text("".join(f"{line}\n" for line in published))
        return run(RELEASE_KIND, tag, manifest, listed, signer, subject)

    def outputs(self, tag, manifest, published=(), signer="signpath", subject=SIGNPATH):
        done = self.kind(tag, manifest, published, signer, subject)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(done.stderr, "")
        pairs = [line.split("=", 1) for line in done.stdout.splitlines()]
        self.assertEqual([key for key, _ in pairs], KEYS)
        return dict(pairs)

    def assertRefused(
        self, tag, manifest, message, published=(), signer="signpath", subject=SIGNPATH
    ):
        done = self.kind(tag, manifest, published, signer, subject)
        self.assertEqual(done.returncode, 1, done.stdout)
        self.assertEqual(done.stdout, "")
        self.assertIn(message, done.stderr)

    def test_each_kind_of_release(self):
        # tag, Cargo.toml, published full releases, kind, windows, crates, latest, with a signer set
        rows = [
            ("v0.15.0", "0.15.0", ["v0.14.0"], "full", "false", "true", "true"),
            ("v0.15.0-rc1", "0.15.0", ["v0.14.0"], "rc", "false", "false", "false"),
            ("v0.15.2", "0.15.2", ["v1.0.0"], "full", "false", "true", "false"),
            ("v1.0.0-beta.1", "1.0.0-beta.1", ["v0.15.0"], "beta", "true", "true", "false"),
            ("v1.0.0-rc1", "1.0.0", ["v0.15.0"], "rc", "true", "false", "false"),
            ("v1.0.0", "1.0.0", ["v0.15.2"], "full", "true", "true", "true"),
            ("v1.1.0-beta.1", "1.1.0-beta.1", ["v1.0.0"], "beta", "true", "true", "false"),
        ]
        for tag, manifest, published, kind, windows, crates, latest in rows:
            with self.subTest(tag=tag):
                self.assertEqual(
                    self.outputs(tag, manifest, published),
                    {
                        "version": tag[1:],
                        "base": manifest,
                        "kind": kind,
                        "prerelease": str(kind != "full").lower(),
                        "windows": windows,
                        "crates": crates,
                        "latest": latest,
                        "signed": windows,
                    },
                )

    def test_a_windows_release_is_signed_once_a_signer_is_set(self):
        # tag, Cargo.toml, signer, signed
        for tag, manifest, signer, signed in (
            ("v1.0.0-beta.1", "1.0.0-beta.1", "", "false"),
            ("v1.0.0-beta.2", "1.0.0-beta.2", "signpath", "true"),
            ("v1.0.0-rc1", "1.0.0", "sslcom", "true"),
            ("v1.0.0", "1.0.0", "signpath", "true"),
            ("v0.15.0", "0.15.0", "signpath", "false"),
            ("v0.15.0-rc1", "0.15.0", "", "false"),
        ):
            with self.subTest(tag=tag, signer=signer):
                self.assertEqual(self.outputs(tag, manifest, signer=signer)["signed"], signed)

    def test_a_windows_release_candidate_or_final_release_needs_a_signer(self):
        for tag, manifest in (("v1.0.0-rc1", "1.0.0"), ("v1.0.0", "1.0.0"), ("v2.3.4", "2.3.4")):
            with self.subTest(tag=tag):
                self.assertRefused(tag, manifest, NO_SIGNER, signer="")

    def test_a_signed_release_needs_the_subject_its_signer_signs_as(self):
        for tag, manifest in (
            ("v1.0.0-beta.2", "1.0.0-beta.2"),
            ("v1.0.0-rc1", "1.0.0"),
            ("v1.0.0", "1.0.0"),
        ):
            with self.subTest(tag=tag):
                self.assertRefused(
                    tag,
                    manifest,
                    "WINDOWS_SIGNER is signpath, and WINDOWS_SIGNER_SUBJECT names no subject to "
                    "check",
                    subject="",
                )
        # Nothing is signed, so there is no subject to check.
        for tag, manifest, signer in (
            ("v1.0.0-beta.1", "1.0.0-beta.1", ""),
            ("v0.15.0", "0.15.0", "signpath"),
        ):
            with self.subTest(tag=tag, signer=signer):
                self.assertEqual(
                    self.outputs(tag, manifest, signer=signer, subject="")["signed"], "false"
                )

    def test_only_a_full_release_nothing_published_outranks_is_latest(self):
        cases = [
            ("v0.15.1", ["v0.15.0", "v0.14.0"], "true"),
            ("v0.15.2", ["v1.0.0", "v0.15.1"], "false"),
            ("v1.0.1", ["v1.0.0", "v0.15.2"], "true"),
            ("v1.0.0", ["v1.0.0", "v0.15.2"], "true"),
            ("v0.9.0", ["v0.10.0"], "false"),
            ("v0.10.0", ["v0.9.0"], "true"),
            ("v2.0.0", [], "true"),
        ]
        for tag, published, latest in cases:
            with self.subTest(tag=tag, published=published):
                self.assertEqual(self.outputs(tag, tag[1:], published)["latest"], latest)

    def test_a_tag_or_manifest_that_disagrees_is_refused(self):
        for tag, manifest in (
            ("v1.0.0-beta.1", "1.0.0"),
            ("v1.0.0", "1.0.0-beta.1"),
            ("v1.0.0-rc1", "1.0.0-beta.1"),
            ("v1.0.0-rc1", "1.0.0-rc1"),
            ("v1.0.1", "1.0.0"),
        ):
            with self.subTest(tag=tag, manifest=manifest):
                self.assertRefused(
                    tag, manifest, f"tag {tag} does not match Cargo.toml's {manifest}"
                )
        self.assertRefused(
            "v0.15.0-beta.1",
            "0.15.0-beta.1",
            "v0.15.0-beta.1 is a beta of a 0.x release: betas are Windows betas, which start at "
            "1.0.0",
        )

    def test_a_tag_that_is_none_of_the_three_is_refused(self):
        for tag in (
            "v1.0.0-alpha.1",
            "v1.0.0-beta.1-rc1",
            "v1.0.0-beta.0",
            "v1.0.0-beta.99",
            "v1.0.0-beta.01",
            "v1.0.0-rc0",
            "v1.0.0-rc01",
            "v1.0.0-rc",
            "v1.0.0-",
            "v1.0",
            "v01.0.0",
            "1.0.0",
            "v",
            "",
            "v1.0.0$(id)",
            "v1.0.0`id`",
            "v1.0.0\nlatest=true",
            "v1.0.0 ",
        ):
            with self.subTest(tag=tag):
                self.assertRefused(tag, tag[1:], NOT_A_TAG)

    def test_a_published_full_release_with_another_tag_is_refused(self):
        self.assertRefused(
            "v1.0.1",
            "1.0.1",
            "the published full release nightly has no vX.Y.Z tag",
            ["v1.0.0", "nightly"],
        )

    def test_it_takes_five_arguments(self):
        listed = self.work / "published"
        listed.write_text("")
        for args in (("v1.0.0", "1.0.0", listed, ""), ("v1.0.0", "1.0.0", listed, "", "", "x")):
            with self.subTest(count=len(args)):
                done = run(RELEASE_KIND, *args)
                self.assertEqual(done.returncode, 2)
                self.assertIn("usage:", done.stderr)


class Binstall(unittest.TestCase):
    # As cargo-binstall 1.25.3 merges them (binstalk-types, PkgMeta::merge_overrides): a target's
    # override replaces pkg-url, pkg-fmt and bin-dir, and adds its disabled-strategies to the
    # package's.
    def resolved(self, target):
        crate = tomllib.loads((REPO / "crates" / "pitboard" / "Cargo.toml").read_text())
        meta = dict(crate["package"]["metadata"]["binstall"])
        override = meta.pop("overrides").get(target, {})
        disabled = set(meta.get("disabled-strategies", []))
        disabled |= set(override.get("disabled-strategies", []))
        meta.update({k: override[k] for k in ("pkg-url", "pkg-fmt", "bin-dir") if k in override})
        meta["disabled-strategies"] = disabled
        return meta

    def expand(self, template, target, version):
        workspace = tomllib.loads((REPO / "Cargo.toml").read_text())
        values = {
            "repo": workspace["workspace"]["package"]["repository"],
            "name": "pitboard",
            "version": version,
            "target": target,
            "bin": "pitboard",
            "binary-ext": ".exe" if "windows" in target else "",
        }
        return re.sub(r"\{ ([a-z-]+) \}", lambda m: values[m.group(1)], template)

    def test_windows_installs_the_zip_package_windows_writes(self):
        script = PACKAGE_WINDOWS.read_text()
        for line in (
            '$name = "pitboard-v$Version-$Triple"',
            '$zip = Join-Path $dist "$name.zip"',
            "$entries = 'pitboard.exe',",
        ):
            self.assertIn(line, script)
        version = "1.0.0-beta.1"
        for target in WINDOWS_TARGETS:
            with self.subTest(target=target):
                meta = self.resolved(target)
                name = f"pitboard-v{version}-{target}"
                self.assertEqual(
                    self.expand(meta["pkg-url"], target, version),
                    f"https://github.com/datlechin/pitboard/releases/download/v{version}/{name}.zip",
                )
                self.assertEqual(meta["pkg-fmt"], "zip")
                self.assertEqual(
                    self.expand(meta["bin-dir"], target, version), f"{name}/pitboard.exe"
                )
                self.assertEqual(meta["disabled-strategies"], {"quick-install"})

    def test_every_other_target_installs_its_tarball(self):
        version = "1.0.0"
        for target in TARBALL_TARGETS:
            with self.subTest(target=target):
                meta = self.resolved(target)
                name = f"pitboard-v{version}-{target}"
                self.assertEqual(
                    self.expand(meta["pkg-url"], target, version),
                    f"https://github.com/datlechin/pitboard/releases/download/v{version}/"
                    f"{name}.tar.gz",
                )
                self.assertEqual(meta["pkg-fmt"], "tgz")
                self.assertEqual(self.expand(meta["bin-dir"], target, version), f"{name}/pitboard")
                self.assertEqual(meta["disabled-strategies"], {"quick-install"})


class WingetHashes(unittest.TestCase):
    VERSION = "1.0.1"
    DOWNLOADS = f"https://github.com/datlechin/pitboard/releases/download/v{VERSION}/"
    ZIPS = {
        "x64": f"pitboard-v{VERSION}-x86_64-pc-windows-msvc.zip",
        "arm64": f"pitboard-v{VERSION}-aarch64-pc-windows-msvc.zip",
    }

    def setUp(self):
        self.work = Path(tempfile.mkdtemp())
        self.hashes = {name: f"{n:064x}" for n, name in enumerate(self.ZIPS.values(), 1)}
        self.sums = self.work / "SHA256SUMS"
        lines = [f"{h}  {name}" for name, h in self.hashes.items()]
        self.sums.write_text("\n".join([f"{0:064x}  install.sh", *lines]) + "\n")

    def tearDown(self):
        shutil.rmtree(self.work)

    # As komac 2.16.0 writes one into --output: manifests/<letter>/<publisher>/<name>/<version>.
    def manifests(self, installers):
        folder = self.work / "out" / "manifests" / "p" / "Pitboard" / "CLI" / self.VERSION
        folder.mkdir(parents=True)
        items = "".join(
            f"- Architecture: {arch}\n  InstallerUrl: {url}\n  InstallerSha256: {digest}\n"
            for arch, url, digest in installers
        )
        (folder / "Pitboard.CLI.installer.yaml").write_text(
            "# Created with komac v2.16.0\n"
            "# yaml-language-server: $schema=https://aka.ms/winget-manifest.installer.1.10.0"
            ".schema.json\n\n"
            f"PackageIdentifier: Pitboard.CLI\nPackageVersion: {self.VERSION}\n"
            "InstallerType: zip\nNestedInstallerType: portable\nNestedInstallerFiles:\n"
            f"- RelativeFilePath: pitboard-v{self.VERSION}-x86_64-pc-windows-msvc\\pitboard.exe\n"
            "  PortableCommandAlias: pitboard\n"
            f"Installers:\n{items}ManifestType: installer\nManifestVersion: 1.10.0\n"
        )
        (folder / "Pitboard.CLI.yaml").write_text("PackageIdentifier: Pitboard.CLI\n")
        return self.work / "out"

    def installers(self, **changed):
        return [
            (arch, self.DOWNLOADS + name, changed.get(arch, self.hashes[name].upper()))
            for arch, name in self.ZIPS.items()
        ]

    # A read that leaves the encoding to the locale fails: on windows-2025 that is cp1252, which
    # cannot decode every UTF-8 manifest komac writes.
    def checked(self, folder):
        return subprocess.run(
            [
                sys.executable,
                "-X",
                "warn_default_encoding",
                "-W",
                "error::EncodingWarning",
                str(WINGET_HASHES),
                str(folder),
                str(self.sums),
            ],
            capture_output=True,
            text=True,
        )

    def test_hashes_equal_to_the_release_lines_pass(self):
        done = self.checked(self.manifests(self.installers()))
        self.assertEqual(done.returncode, 0, done.stderr)
        for name in self.ZIPS.values():
            self.assertIn(f"ok: {name}", done.stdout)

    def test_a_changed_hash_fails(self):
        wrong = "F" * 64
        done = self.checked(self.manifests(self.installers(arm64=wrong)))
        self.assertEqual(done.returncode, 1)
        name = self.ZIPS["arm64"]
        self.assertIn(
            f"{name}: the manifest says {wrong}, and SHA256SUMS {self.hashes[name]}", done.stderr
        )

    def test_a_url_with_no_line_fails(self):
        installers = self.installers()
        installers[1] = ("arm64", self.DOWNLOADS + "pitboard-v1.0.1-arm64.zip", "A" * 64)
        done = self.checked(self.manifests(installers))
        self.assertEqual(done.returncode, 1)
        self.assertIn("SHA256SUMS has no line for pitboard-v1.0.1-arm64.zip", done.stderr)

    def test_an_installer_without_a_url_or_a_hash_fails(self):
        installers = self.installers()
        installers[0] = ("x64", self.DOWNLOADS + self.ZIPS["x64"], "")
        done = self.checked(self.manifests(installers))
        self.assertEqual(done.returncode, 1)
        self.assertIn("installer 1 has no InstallerSha256", done.stderr)

    def test_a_folder_with_no_installer_manifest_fails(self):
        (self.work / "empty").mkdir()
        done = self.checked(self.work / "empty")
        self.assertEqual(done.returncode, 1)
        self.assertIn("holds 0 installer manifests, not 1", done.stderr)

    def test_it_takes_two_arguments(self):
        done = subprocess.run(
            [sys.executable, str(WINGET_HASHES), str(self.work)], capture_output=True, text=True
        )
        self.assertEqual(done.returncode, 2)
        self.assertIn("usage:", done.stderr)


class WindowsAppcast(unittest.TestCase):
    VERSION = "1.0.1"
    DOWNLOADS = f"https://github.com/datlechin/pitboard/releases/download/v{VERSION}/"
    SPARKLE = "{http://www.andymatuschak.org/xml-namespaces/sparkle}"
    NOT_FULL = "is not a full release"

    def setUp(self):
        self.work = Path(tempfile.mkdtemp())
        self.seed = os.urandom(32)
        self.msis = {}
        for arch in ("x64", "arm64"):
            self.msis[arch] = self.work / f"Pitboard-v{self.VERSION}-windows-{arch}.msi"
            self.msis[arch].write_bytes(f"the {arch} MSI\n".encode() + os.urandom(100))

    def tearDown(self):
        shutil.rmtree(self.work)

    def signed(self, arch):
        return base64.b64encode(ed_sign(self.seed, self.msis[arch].read_bytes())).decode()

    def written(self, version=VERSION, downloads=DOWNLOADS, signatures=None):
        signatures = signatures or [self.signed("x64"), self.signed("arm64")]
        return subprocess.run(
            [sys.executable, WINDOWS_APPCAST, version, downloads, self.work, *signatures],
            capture_output=True,
            text=True,
        )

    def test_the_signatures_are_rfc_8032s(self):
        for seed, message, public, signature in (
            (
                "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
                "",
                "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
                "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155"
                "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
            ),
            (
                "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb",
                "72",
                "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c",
                "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da"
                "085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
            ),
        ):
            seed, message = bytes.fromhex(seed), bytes.fromhex(message)
            public, signature = bytes.fromhex(public), bytes.fromhex(signature)
            self.assertEqual(ed_public(seed), public)
            self.assertEqual(ed_sign(seed, message), signature)
            self.assertTrue(ed_verify(public, message, signature))
            self.assertFalse(ed_verify(public, message + b"!", signature))

    def test_each_architecture_gets_an_item_whose_msi_the_key_signed(self):
        done = self.written()
        self.assertEqual(done.returncode, 0, done.stderr)
        items = ElementTree.fromstring(done.stdout).find("channel").findall("item")
        self.assertEqual(len(items), 2)
        floor = re.search(
            r"<SupportedOSPlatformVersion>(\d+\.\d+\.\d+)", WINDOWS_APP.read_text()
        ).group(1)
        public = ed_public(self.seed)
        for item, arch, other in zip(items, ("x64", "arm64"), ("arm64", "x64")):
            with self.subTest(arch=arch):
                sparkle = self.SPARKLE
                self.assertEqual(item.findtext(f"{sparkle}version"), self.VERSION)
                self.assertEqual(item.findtext(f"{sparkle}shortVersionString"), self.VERSION)
                self.assertEqual(item.findtext(f"{sparkle}minimumSystemVersion"), floor)
                [enclosure] = item.findall("enclosure")
                msi = self.msis[arch].read_bytes()
                self.assertEqual(enclosure.get("url"), self.DOWNLOADS + self.msis[arch].name)
                self.assertEqual(enclosure.get("length"), str(len(msi)))
                self.assertEqual(enclosure.get(f"{sparkle}os"), f"windows-{arch}")
                self.assertEqual(
                    enclosure.get(f"{sparkle}installerArguments"),
                    "/passive /norestart LAUNCHAFTERINSTALL=1",
                )
                signature = base64.b64decode(enclosure.get(f"{sparkle}edSignature"))
                self.assertTrue(ed_verify(public, msi, signature))
                self.assertFalse(ed_verify(public, self.msis[other].read_bytes(), signature))

    def test_only_a_full_release_has_a_feed(self):
        for version in ("1.0.0-beta.1", "1.0.0-rc1", "1.0", "v1.0.0", "01.0.0", "1.0.0+abc"):
            with self.subTest(version=version):
                done = self.written(version=version)
                self.assertEqual(done.returncode, 1)
                self.assertEqual(done.stdout, "")
                self.assertIn(self.NOT_FULL, done.stderr)

    def test_a_signature_that_is_not_one_is_refused(self):
        for text in (
            "",
            "not base64!",
            base64.b64encode(os.urandom(63)).decode(),
            base64.b64encode(ed_public(self.seed)).decode(),
        ):
            with self.subTest(signature=text):
                done = self.written(signatures=[self.signed("x64"), text])
                self.assertEqual(done.returncode, 1)
                self.assertEqual(done.stdout, "")
                self.assertIn("is not an EdDSA signature", done.stderr)

    def test_a_missing_installer_is_refused(self):
        signatures = [self.signed("x64"), self.signed("arm64")]
        self.msis["arm64"].unlink()
        done = self.written(signatures=signatures)
        self.assertEqual(done.returncode, 1)
        self.assertIn(f"no Pitboard-v{self.VERSION}-windows-arm64.msi", done.stderr)

    def test_a_download_address_that_is_not_https_is_refused(self):
        for downloads in (self.DOWNLOADS.replace("https", "http"), self.DOWNLOADS.rstrip("/")):
            with self.subTest(downloads=downloads):
                done = self.written(downloads=downloads)
                self.assertEqual(done.returncode, 1)
                self.assertIn("is not an https:// address ending in /", done.stderr)

    def test_it_takes_five_arguments(self):
        done = subprocess.run(
            [sys.executable, WINDOWS_APPCAST, self.VERSION, self.DOWNLOADS, self.work, "x"],
            capture_output=True,
            text=True,
        )
        self.assertEqual(done.returncode, 2)
        self.assertIn("usage:", done.stderr)


class BuildNumber(unittest.TestCase):
    def test_numbers_rise_with_the_version(self):
        numbers = {
            "0.14.0": 140099,
            "0.15.0": 150099,
            "0.15.1": 150199,
            "1.0.0-beta.1": 1000001,
            "1.0.0-beta.2": 1000002,
            "1.0.0": 1000099,
            "1.0.1": 1000199,
            "1.1.0-beta.1": 1010001,
            "1.1.0-beta.2": 1010002,
            "1.99.99-beta.98": 1999998,
            "2.0.0": 2000099,
        }
        for version, number in numbers.items():
            with self.subTest(version=version):
                done = run(BUILD_NUMBER, version)
                self.assertEqual(done.returncode, 0, done.stderr)
                self.assertEqual(done.stdout, f"{number}\n")
        ordered = [n for _, n in sorted(numbers.items(), key=lambda kv: kv[1])]
        self.assertEqual(list(numbers.values()), ordered)
        self.assertEqual(len(set(ordered)), len(ordered))
        # 0.14.0 shipped as 1400, by the scheme before this one.
        self.assertGreater(numbers["0.15.0"], 1400)

    def test_a_version_it_cannot_number_is_refused(self):
        for version in (
            "1.100.0",
            "1.0.100",
            "1.0.0-beta.99",
            "1.0.0-beta.0",
            "1.0.0-rc1",
            "01.0.0",
            "1.01.0",
            "1.0",
            "",
            "1.0.0\n2.0.0",
        ):
            with self.subTest(version=version):
                done = run(BUILD_NUMBER, version)
                self.assertEqual(done.returncode, 1)
                self.assertEqual(done.stdout, "")
                self.assertIn(NO_BUILD_NUMBER, done.stderr)

    def test_it_takes_one_argument(self):
        done = run(BUILD_NUMBER)
        self.assertEqual(done.returncode, 2)
        self.assertIn("usage:", done.stderr)


class WindowsVersions(unittest.TestCase):
    # Cargo.toml's version: the MSI's ProductVersion, the version resource's FileVersion, the
    # sparse package's version. In release order.
    NUMBERS = {
        "1.0.0-beta.1": ("1.0.0.1", "1.0.0.1", "1.0.0.1"),
        "1.0.0": ("1.0.0", "1.0.0.1000", "1.0.0.1000"),
        "1.0.1": ("1.0.1", "1.0.1.1000", "1.0.1.1000"),
        "1.1.0-beta.2": ("1.1.0.2", "1.1.0.2", "1.1.0.2"),
    }

    def numbers(self, version):
        done = run(WINDOWS_VERSIONS, version)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(done.stderr, "")
        pairs = [line.split("=", 1) for line in done.stdout.splitlines()]
        self.assertEqual([key for key, _ in pairs], ["msi", "file", "package"])
        return tuple(value for _, value in pairs)

    @staticmethod
    def fields(number):
        return tuple(int(field) for field in number.split("."))

    def test_each_version_has_its_numbers(self):
        for version, numbers in {
            **self.NUMBERS,
            "0.15.0": ("0.15.0", "0.15.0.1000", "0.15.0.1000"),
            "255.255.65535-beta.999": ("255.255.65535.999",) * 3,
        }.items():
            with self.subTest(version=version):
                self.assertEqual(self.numbers(version), numbers)

    def test_each_number_rises_with_the_version(self):
        rows = [self.numbers(version) for version in self.NUMBERS]
        for before, after in zip(rows, rows[1:]):
            with self.subTest(before=before, after=after):
                msi, file, package = (self.fields(n) for n in after)
                self.assertGreater(file, self.fields(before[1]))
                self.assertGreater(package, self.fields(before[2]))
                # Windows Installer compares three fields, so a beta and its final are one version
                # there, which the MSI's MajorUpgrade replaces all the same.
                self.assertGreaterEqual(msi[:3], self.fields(before[0])[:3])
        self.assertEqual(self.fields(rows[0][0])[:3], self.fields(rows[1][0])[:3])

    def test_a_version_windows_cannot_number_is_refused(self):
        for version in (
            "256.0.0",
            "1.256.0",
            "1.0.65536",
            "1.0.0-beta.0",
            "1.0.0-beta.1000",
            "1.0.0-rc1",
            "1.0.0-alpha.1",
            "01.0.0",
            "1.0",
            "1.0.0.1",
            "",
            "1.0.0\n2.0.0",
        ):
            with self.subTest(version=version):
                done = run(WINDOWS_VERSIONS, version)
                self.assertEqual(done.returncode, 1)
                self.assertEqual(done.stdout, "")
                self.assertIn(NO_WINDOWS_VERSIONS, done.stderr)

    def test_it_takes_one_argument(self):
        done = run(WINDOWS_VERSIONS)
        self.assertEqual(done.returncode, 2)
        self.assertIn("usage:", done.stderr)


class SessionGate(unittest.TestCase):
    BRANCH = "refs/heads/release-1.0.0-beta.1"
    # The example in age's README, as age-keygen prints a public key.
    KEY = "age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p"

    def gated(self, ref, version, key=KEY, **flags):
        env = {
            name: value
            for name, value in os.environ.items()
            if name not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")
        }
        env.update(flags)
        return subprocess.run(
            ["sh", SESSION_GATE, ref, version, key], capture_output=True, text=True, env=env
        )

    def assertRefused(self, done, said):
        self.assertEqual(done.returncode, 1, done.stdout)
        self.assertEqual(done.stdout, "")
        self.assertIn(said, done.stderr)

    def test_a_branch_before_1_0_0_beta_1_is_refused(self):
        for version in ("0.14.0", "0.14.3", "0.99.0"):
            with self.subTest(version=version):
                done = self.gated("refs/heads/windows-ws-session-bundle", version)
                self.assertRefused(done, f"Pitboard is {version} on this branch: {ONLY_FROM_A_BETA}")

    def test_a_tag_or_another_ref_is_refused(self):
        for ref in ("refs/tags/v1.0.0-beta.1", "refs/pull/300/merge", "refs/heads/", ""):
            with self.subTest(ref=ref):
                self.assertRefused(self.gated(ref, "1.0.0-beta.1"), ONLY_FROM_A_BETA)

    def test_a_version_windows_cannot_number_is_refused(self):
        for version in ("1.0.0-rc1", "1.0", "", "1.0.0-beta.1\n0.1.0"):
            with self.subTest(version=version):
                self.assertRefused(self.gated(self.BRANCH, version), NO_WINDOWS_VERSIONS)

    def test_a_build_opened_with_the_cfg_is_refused(self):
        opened = {
            "RUSTFLAGS": "-C target-feature=+crt-static --cfg pitboard_unreleased_windows",
            "CARGO_ENCODED_RUSTFLAGS": "--cfg\x1fpitboard_unreleased_windows",
        }
        for name, flags in opened.items():
            with self.subTest(name=name):
                done = self.gated(self.BRANCH, "1.0.0-beta.1", **{name: flags})
                self.assertRefused(done, f"{name} opens the build to Windows before its release")

    def test_a_key_that_is_not_an_age_public_key_is_refused_and_never_printed(self):
        for key in (
            "",
            self.KEY[:-1],
            self.KEY + "q",
            self.KEY.upper(),
            # 1, b, i and o are not in Bech32's alphabet; a plugin's key holds a 1 after its name.
            self.KEY[:-1] + "b",
            "age1tag1" + self.KEY[8:],
            "AGE-SECRET-KEY-1QYQSZQGPQYQSZQGPQYQSZQGPQYQSZQGPQYQSZQGPQYQSZQGPQYQSZQGPQYQS3R2",
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl",
            f"{self.KEY}\n{self.KEY}",
        ):
            with self.subTest(key=key):
                done = self.gated(self.BRANCH, "1.0.0-beta.1", key)
                self.assertRefused(done, "is not an age public key")
                if key:
                    self.assertNotIn(key, done.stderr)

    def test_a_branch_at_a_beta_or_later_gives_both_msis_their_numbers_and_the_key(self):
        for version, msi, file, msi_b in (
            ("1.0.0-beta.1", "1.0.0.1", "1.0.0.1", "1.0.0.2"),
            ("1.1.0-beta.2", "1.1.0.2", "1.1.0.2", "1.1.0.3"),
            ("1.0.0", "1.0.0", "1.0.0.1000", "1.0.0.1001"),
        ):
            with self.subTest(version=version):
                done = self.gated(
                    self.BRANCH, version, RUSTFLAGS="-C target-feature=+crt-static"
                )
                self.assertEqual(done.returncode, 0, done.stderr)
                self.assertEqual(done.stderr, "")
                self.assertEqual(
                    done.stdout,
                    f"version={version}\nmsi={msi}\nfile={file}\npackage={file}\nmsi-b={msi_b}\n"
                    f"recipient={self.KEY}\n",
                )

    def test_it_takes_three_arguments(self):
        done = run(SESSION_GATE, self.BRANCH, "1.0.0-beta.1")
        self.assertEqual(done.returncode, 2)
        self.assertIn("usage:", done.stderr)


class WindowsUpdateKey(unittest.TestCase):
    KEY = base64.b64encode(ed_public(bytes(32))).decode()
    NEXT = base64.b64encode(ed_public(bytes(range(32)))).decode()

    def checked(self, version, now, previous, was, rotation=""):
        return run(WINDOWS_UPDATE_KEY, version, now, previous, was, rotation)

    def assertPasses(self, done, said):
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(done.stderr, "")
        self.assertIn(said, done.stdout)

    def assertRefused(self, done, said):
        self.assertEqual(done.returncode, 1, done.stdout)
        self.assertEqual(done.stdout, "")
        self.assertIn(said, done.stderr)

    def test_the_key_the_last_msi_shipped_passes(self):
        for version, previous in (("1.0.0-beta.2", "v1.0.0-beta.1"), ("1.0.0", "v1.0.0-rc1")):
            with self.subTest(version=version):
                done = self.checked(version, self.KEY, previous, self.KEY)
                self.assertPasses(done, f"the update key is the one {previous} shipped")

    def test_a_change_the_rotation_names_passes(self):
        done = self.checked("1.2.0", self.NEXT, "v1.1.0", self.KEY, "1.2.0")
        self.assertPasses(done, f"declared rotation: v1.1.0 shipped {self.KEY}, 1.2.0 ships {self.NEXT}")

    def test_a_change_nobody_declared_is_refused(self):
        # The second rung of a rotation ships the first rung's key, so a rotation that names
        # another version declares nothing for this one.
        for rotation in ("", "1.1.0", "1.2.0-rc1"):
            with self.subTest(rotation=rotation):
                done = self.checked("1.2.0", self.NEXT, "v1.1.0", self.KEY, rotation)
                self.assertRefused(done, f"this release's update key is {self.NEXT}, and v1.1.0 shipped")
                self.assertIn("WINDOWS_UPDATE_KEY_ROTATION to 1.2.0", done.stderr)

    def test_with_no_earlier_msi_there_is_nothing_to_compare(self):
        done = self.checked("1.0.0-beta.1", self.KEY, "", "")
        self.assertPasses(done, "no earlier release ships an MSI")

    def test_a_release_without_a_key_is_refused(self):
        for previous, was in (("", ""), ("v1.0.0", self.KEY)):
            with self.subTest(previous=previous):
                done = self.checked("1.0.1", "", previous, was)
                self.assertRefused(done, "carries no update key: WINDOWS_UPDATE_PUBLIC_KEY is not set")

    def test_an_earlier_msi_whose_key_is_not_read_is_refused(self):
        done = self.checked("1.0.1", self.KEY, "v1.0.0", "")
        self.assertRefused(done, "no update key was read from v1.0.0's MSI")

    def test_it_takes_five_arguments(self):
        done = run(WINDOWS_UPDATE_KEY, "1.0.0", self.KEY, "", "")
        self.assertEqual(done.returncode, 2)
        self.assertIn("usage:", done.stderr)


# sign_update and eddsa-verify.swift, which run only on macOS, as this file's Ed25519.
STAND_IN_SIGNER = """import base64, importlib.util, sys
spec = importlib.util.spec_from_file_location("release", {tests!r})
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)
if sys.argv[1] == "sign":
    seed = base64.b64decode(sys.stdin.read().strip())
    print(base64.b64encode(release.ed_sign(seed, open(sys.argv[-1], "rb").read())).decode())
else:
    public, signature, path = (base64.b64decode(sys.argv[2]), base64.b64decode(sys.argv[3]), sys.argv[4])
    sys.exit(0 if release.ed_verify(public, open(path, "rb").read(), signature) else 1)
"""


class WindowsUpdateSign(unittest.TestCase):
    DOWNLOADS = "https://github.com/datlechin/pitboard/releases/download/{tag}/"

    def setUp(self):
        self.work = Path(tempfile.mkdtemp())
        self.seed = os.urandom(32)
        self.public = base64.b64encode(ed_public(self.seed)).decode()
        stand_in = self.work / "stand-in.py"
        stand_in.write_text(STAND_IN_SIGNER.format(tests=str(Path(__file__).resolve())))
        self.sign = self.work / "sign_update"
        self.sign.write_text(f'#!/bin/sh\nexec "{sys.executable}" -B "{stand_in}" sign "$@"\n')
        self.sign.chmod(0o755)
        self.verifier = [sys.executable, "-B", str(stand_in), "verify"]
        self.feed = self.work / "appcast-windows.xml"

    def tearDown(self):
        shutil.rmtree(self.work)

    def signed(self, tag, latest, private=None, public=None):
        for arch in ("x64", "arm64"):
            (self.work / f"Pitboard-{tag}-windows-{arch}.msi").write_bytes(os.urandom(200))
        private = base64.b64encode(self.seed).decode() if private is None else private
        return subprocess.run(
            ["sh", WINDOWS_UPDATE_SIGN, tag, latest, public or self.public, self.work,
             self.DOWNLOADS.format(tag=tag), self.feed, self.sign, *self.verifier],
            input=private + "\n",
            capture_output=True,
            text=True,
        )

    def test_a_beta_and_a_release_candidate_check_both_msis_and_write_no_feed(self):
        for tag in ("v1.0.0-beta.1", "v1.0.0-rc1"):
            with self.subTest(tag=tag):
                done = self.signed(tag, "false")
                self.assertEqual(done.returncode, 0, done.stderr)
                self.assertIn("Only the latest full release has a feed", done.stdout)
                self.assertFalse(self.feed.exists())

    def test_a_private_key_that_is_not_the_public_keys_pair_fails_a_beta(self):
        other = base64.b64encode(ed_public(os.urandom(32))).decode()
        done = self.signed("v1.0.0-beta.1", "false", public=other)
        self.assertEqual(done.returncode, 1)
        self.assertIn("is not WINDOWS_UPDATE_PUBLIC_KEY's pair", done.stderr)
        self.assertFalse(self.feed.exists())

    def test_the_latest_full_release_writes_a_feed_whose_signatures_the_public_key_verifies(self):
        done = self.signed("v1.0.1", "true")
        self.assertEqual(done.returncode, 0, done.stderr)
        sparkle = "{http://www.andymatuschak.org/xml-namespaces/sparkle}"
        items = ElementTree.parse(self.feed).getroot().find("channel").findall("item")
        self.assertEqual(len(items), 2)
        for item, arch in zip(items, ("x64", "arm64")):
            enclosure = item.find("enclosure")
            msi = (self.work / f"Pitboard-v1.0.1-windows-{arch}.msi").read_bytes()
            signature = base64.b64decode(enclosure.get(f"{sparkle}edSignature"))
            self.assertTrue(ed_verify(ed_public(self.seed), msi, signature))

    def test_no_private_key_is_refused(self):
        done = self.signed("v1.0.0-beta.1", "false", private="")
        self.assertEqual(done.returncode, 1)
        self.assertIn("WINDOWS_UPDATE_PRIVATE_KEY is not set", done.stderr)

    def test_it_takes_its_arguments(self):
        done = run(WINDOWS_UPDATE_SIGN, "v1.0.0", "true", self.public, self.work)
        self.assertEqual(done.returncode, 2)
        self.assertIn("usage:", done.stderr)


class LinuxVersions(unittest.TestCase):
    # A tag's version and its packages', in release order.
    VERSIONS = {
        "1.0.0-beta.6": "1.0.0~beta.6",
        "1.0.0-beta.7": "1.0.0~beta.7",
        "1.0.0-beta.10": "1.0.0~beta.10",
        "1.0.0-rc1": "1.0.0~rc1",
        "1.0.0-rc2": "1.0.0~rc2",
        "1.0.0-rc10": "1.0.0~rc10",
        "1.0.0": "1.0.0",
        "1.0.1-beta.1": "1.0.1~beta.1",
        "1.0.1": "1.0.1",
        "1.10.0": "1.10.0",
    }

    def versions(self, version):
        done = run(LINUX_VERSIONS, version)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(done.stderr, "")
        pairs = [line.split("=", 1) for line in done.stdout.splitlines()]
        self.assertEqual([key for key, _ in pairs], ["deb", "rpm-version", "rpm-release"])
        return tuple(value for _, value in pairs)

    def test_each_kind_has_its_package_versions(self):
        for version, package in {
            **self.VERSIONS,
            "0.15.2": "0.15.2",
            "0.16.0-rc1": "0.16.0~rc1",
            "2.0.0-beta.98": "2.0.0~beta.98",
        }.items():
            with self.subTest(version=version):
                self.assertEqual(self.versions(version), (package, package, "1"))

    def test_package_versions_rise_with_the_release(self):
        needs(self, "dpkg", "rpm")
        rows = [self.versions(version) for version in self.VERSIONS]
        for before, after in zip(rows, rows[1:]):
            with self.subTest(before=before[0], after=after[0]):
                dpkg = subprocess.run(["dpkg", "--compare-versions", before[0], "lt", after[0]])
                self.assertEqual(dpkg.returncode, 0)
                # The comparison dnf and zypper make, which answers -1 where the first is older.
                vercmp = f"%{{lua:print(rpm.vercmp('{before[1]}', '{after[1]}'))}}"
                rpm = subprocess.run(["rpm", "--eval", vercmp], capture_output=True, text=True)
                self.assertEqual(rpm.returncode, 0, rpm.stderr)
                self.assertEqual(rpm.stdout.strip(), "-1")

    def test_a_version_it_cannot_number_is_refused(self):
        for version in (
            "1.0",
            "1.0.0.1",
            "01.0.0",
            "1.0.0-rc0",
            "1.0.0-rc01",
            "1.0.0-rc.1",
            "1.0.0-beta.0",
            "1.0.0-beta.99",
            "1.0.0-beta1",
            "1.0.0-alpha.1",
            "0.15.0-beta.1",
            "1.0.0~rc1",
            "1.0.0+1",
            "v1.0.0",
            "",
            "1.0.0\n2.0.0",
        ):
            with self.subTest(version=version):
                done = run(LINUX_VERSIONS, version)
                self.assertEqual(done.returncode, 1)
                self.assertEqual(done.stdout, "")
                self.assertIn(NO_PACKAGE_VERSION, done.stderr)

    def test_it_takes_one_argument(self):
        for args in ((), ("1.0.0", "1.0.1")):
            with self.subTest(args=args):
                done = run(LINUX_VERSIONS, *args)
                self.assertEqual(done.returncode, 2)
                self.assertIn("usage:", done.stderr)


class RepositoryKey(unittest.TestCase):
    PRIMARY = "22DCF748069770D807A1804F47C507760C9A97E3"
    SIGNING = "C348A22094AA62F6F3F4CBA14C7CAA49E81E0FF8"
    DAY = 86400
    # Every key here is made at the start of the day the tests run, in UTC, so each lasts to the
    # same second whatever the hour, and gpg, which judges a subkey expired by its own clock,
    # finds it as the case made it.
    MADE_AT = int(time.time()) // DAY * DAY

    def setUp(self):
        needs(self, "gpg")
        self.work = Path(tempfile.mkdtemp())
        home = self.work / "gnupg"
        home.mkdir(mode=0o700)
        self.tmp = self.work / "tmp"
        self.tmp.mkdir()
        self.env = {**os.environ, "GNUPGHOME": str(home)}
        self.made = 0

    def tearDown(self):
        subprocess.run(["gpgconf", "--kill", "all"], env=self.env, capture_output=True)
        shutil.rmtree(self.work)

    def gpg(self, *args, made=None, given=None):
        done = subprocess.run(
            ["gpg", "--batch", "--quiet", "--passphrase", "", "--faked-system-time",
             f"{self.MADE_AT if made is None else made}!", *args],
            input=given,
            capture_output=True,
            text=True,
            env=self.env,
        )
        self.assertEqual(done.returncode, 0, done.stderr)
        return done.stdout

    def key(self, usage="cert", lasts=10 * 365, subkeys=(("sign", 2 * 365),), sha1=(), made=0):
        # Each key lasts that many days from when it is made, or for ever where None, and the
        # primary is made that many days before MADE_AT. A subkey made earlier than MADE_AT names
        # the days before it third. sha1 names what is signed with SHA-1: the user ID's
        # self-signature, or each subkey's binding and back signature.
        def expiry(days):
            return "never" if days is None else f"seconds={days * self.DAY}"

        weak = ["--cert-digest-algo", "SHA1", "--allow-weak-key-signatures"]
        algo = "rsa2048" if sha1 else "ed25519"
        self.made += 1
        made = self.gpg(
            *(weak if "user ID" in sha1 else []),
            "--status-fd", "1",
            "--quick-gen-key", f"Test {self.made}", algo, usage, expiry(lasts),
            made=self.MADE_AT - made * self.DAY,
        )
        fingerprint = re.search(r"KEY_CREATED [PB] ([0-9A-F]{40})", made).group(1)
        for subkey_usage, subkey_lasts, *before in subkeys:
            self.gpg(
                *(weak if "binding" in sha1 else []),
                "--quick-add-key", fingerprint,
                "cv25519" if subkey_usage == "encr" and not sha1 else algo,
                subkey_usage, expiry(subkey_lasts),
                made=self.MADE_AT - (before[0] if before else 0) * self.DAY,
            )
        return fingerprint

    def exported(self, fingerprint):
        path = self.work / f"{fingerprint}.asc"
        self.gpg("--armor", "--output", str(path), "--yes", "--export", fingerprint)
        return path

    def subkeys(self, fingerprint):
        listed = self.gpg("--with-colons", "--with-subkey-fingerprints", "--list-keys", fingerprint)
        return re.findall(r"^fpr:+([0-9A-F]{40}):", listed, re.M)[1:]

    # Revokes the primary's nth subkey, from 1, as an owner revokes a lost one with gpg's
    # --edit-key: no reason given beside that one.
    def revoke(self, fingerprint, nth):
        self.gpg(
            "--command-fd", "0", "--edit-key", fingerprint,
            given=f"key {nth}\nrevkey\ny\n0\n\ny\nsave\n",
        )

    def at(self, days):
        return str(self.MADE_AT + days * self.DAY)

    def checked(self, key, now):
        done = subprocess.run(
            ["sh", LINUX_REPOSITORY_KEY, key, now],
            capture_output=True,
            text=True,
            env={**self.env, "TMPDIR": str(self.tmp)},
        )
        self.assertEqual(os.listdir(self.tmp), [], "it left its GNUPGHOME behind")
        return done

    def assertPasses(self, done):
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(done.stderr, "")
        return dict(line.split("=", 1) for line in done.stdout.splitlines())

    def assertRefused(self, done, said):
        self.assertEqual(done.returncode, 1, done.stdout)
        self.assertEqual(done.stdout, "")
        self.assertIn(said, done.stderr)

    def test_the_committed_key_is_the_owners(self):
        # 2026-10-28, when the owner made it.
        done = self.checked(REPOSITORY_KEY, "1793145600")
        self.assertPasses(done)
        # The signing subkey expires at 2028-10-27 11:00:12 UTC.
        self.assertEqual(
            done.stdout,
            f"primary={self.PRIMARY}\nsigning={self.SIGNING}\nexpires=1856257212\n",
        )
        listed = subprocess.run(
            ["gpg", "--batch", "--quiet", "--with-colons", "--with-subkey-fingerprints",
             "--show-keys", REPOSITORY_KEY],
            capture_output=True,
            text=True,
            env=self.env,
        )
        self.assertEqual(listed.returncode, 0, listed.stderr)
        records = [line.split(":") for line in listed.stdout.splitlines()]
        # RSA 4096 each: the primary certifies to 2036-10-25, and the subkey signs.
        self.assertEqual(
            [(r[0], r[2], r[3], r[6], r[11]) for r in records if r[0] in ("pub", "sub")],
            [("pub", "4096", "1", "2108545210", "cSC"), ("sub", "4096", "1", "1856257212", "s")],
        )
        self.assertEqual([r[9] for r in records if r[0] == "fpr"], [self.PRIMARY, self.SIGNING])
        # RELEASING.md's last second before CI fails, 2028-04-30 11:00:12 UTC.
        self.assertPasses(self.checked(REPOSITORY_KEY, "1840705212"))
        self.assertRefused(self.checked(REPOSITORY_KEY, "1840705213"), "has 179 days left")

    def test_a_signing_subkey_with_fewer_than_180_days_left_is_refused(self):
        key = self.exported(self.key(subkeys=(("sign", 400),)))
        found = self.assertPasses(self.checked(key, self.at(400 - 180)))
        self.assertEqual(found["primary"], key.stem)
        self.assertEqual(found["expires"], self.at(400))
        for now, left in (
            (str(self.MADE_AT + (400 - 180) * self.DAY + 1), "179 days"),
            (self.at(399), "1 day"),
            (self.at(400), "no day"),
            (self.at(2000), "no day"),
        ):
            with self.subTest(now=now):
                done = self.checked(key, now)
                self.assertRefused(
                    done, f"{found['signing']} has {left} left: extend the key 180 days ahead"
                )

    def test_a_primary_that_expires_first_shortens_its_subkey(self):
        primary = self.key(lasts=300, subkeys=(("sign", 2 * 365),))
        key = self.exported(primary)
        self.assertPasses(self.checked(key, self.at(300 - 180)))
        self.assertRefused(
            self.checked(key, self.at(300 - 179)),
            f"has 179 days left, until its primary {primary} expires",
        )

    def test_a_key_with_no_signing_subkey_or_two_is_refused(self):
        none = "has no signing subkey that is neither revoked nor expired"
        for subkeys, said in (
            ((), none),
            ((("encr", 400),), none),
            ((("sign", 400), ("sign", 800)), "has 2 signing subkeys"),
            ((("sign", 400), ("encr", 400), ("sign", 800)), "has 2 signing subkeys"),
            # Made 100 days ago for 30, so expired by gpg's clock.
            ((("sign", 30, 100),), none),
        ):
            with self.subTest(subkeys=subkeys):
                key = self.exported(self.key(subkeys=subkeys, made=100))
                self.assertRefused(self.checked(key, self.at(0)), said)

    def test_a_subkey_revoked_or_expired_beside_a_new_one_is_left_out(self):
        rotated = self.key(subkeys=(("sign", 2 * 365), ("sign", 2 * 365)), made=100)
        old, new = self.subkeys(rotated)
        self.revoke(rotated, 1)
        expired = self.key(subkeys=(("sign", 30, 100), ("sign", 2 * 365)), made=100)
        for primary, kept in ((rotated, new), (expired, self.subkeys(expired)[1])):
            with self.subTest(primary=primary):
                found = self.assertPasses(self.checked(self.exported(primary), self.at(0)))
                self.assertEqual((found["primary"], found["signing"]), (primary, kept))
                self.assertEqual(found["expires"], self.at(2 * 365))
        listed = self.gpg("--with-colons", "--list-keys", rotated)
        self.assertEqual([r.split(":")[1] for r in listed.splitlines() if r.startswith("sub:")],
                         ["r", "u"])
        self.assertNotEqual(old, new)

    def test_a_signing_subkey_that_never_expires_is_refused(self):
        key = self.exported(self.key(subkeys=(("sign", None),)))
        self.assertRefused(self.checked(key, self.at(0)), "never expires")

    def test_a_primary_that_signs_is_refused(self):
        primary = self.key(usage="sign")
        self.assertRefused(
            self.checked(self.exported(primary), self.at(0)), f"the primary key {primary} signs"
        )

    def test_a_signature_under_sha_256_is_refused(self):
        for sha1, refused in (
            (("user ID",), ["class 0x13: SHA-1"]),
            (("binding",), ["class 0x18: SHA-1", "class 0x19: SHA-1"]),
        ):
            with self.subTest(sha1=sha1):
                done = self.checked(self.exported(self.key(sha1=sha1)), self.at(0))
                self.assertRefused(done, "holds signatures under SHA-256, which apt's sqv refuses")
                self.assertEqual([line.strip() for line in done.stderr.splitlines()[1:]], refused)

    def test_a_file_that_holds_no_key_or_two_is_refused(self):
        two = self.work / "two.asc"
        two.write_text(
            self.exported(self.key()).read_text() + self.exported(self.key()).read_text()
        )
        not_a_key = self.work / "not-a-key.asc"
        not_a_key.write_text("Pitboard\n")
        for path, said in (
            (two, "holds 2 primary keys"),
            (not_a_key, "gpg read no key from"),
            (self.work / "missing.asc", "gpg read no key from"),
        ):
            with self.subTest(path=path.name):
                self.assertRefused(self.checked(path, self.at(0)), said)

    def test_it_takes_a_key_and_seconds_since_the_epoch(self):
        for args in (
            (REPOSITORY_KEY,),
            (REPOSITORY_KEY, "1793145600", "1793145601"),
            (REPOSITORY_KEY, "2026-10-28"),
            (REPOSITORY_KEY, ""),
            (REPOSITORY_KEY, "-1"),
            (REPOSITORY_KEY, "1793145600\n1856257212"),
        ):
            with self.subTest(args=args[1:]):
                done = run(LINUX_REPOSITORY_KEY, *args)
                self.assertEqual(done.returncode, 2)
                self.assertIn("usage: linux-repository-key.sh <public key> <seconds", done.stderr)


class PackCommandLine(unittest.TestCase):
    NAME = "pitboard-v1.0.0-beta.7-aarch64-unknown-linux-musl"
    # A stand-in for the command line, which writes its man page and completions as pitboard does.
    STAND_IN = """#!/bin/sh
case $1 in
  manpage) echo '.TH pitboard 1 "pitboard 1.0.0-beta.7"' ;;
  completions) echo "# pitboard's $2 completions" ;;
  *) exit 64 ;;
esac
"""

    def setUp(self):
        self.work = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.work)
        self.binary = self.work / "release" / "pitboard"
        self.binary.parent.mkdir()
        self.binary.write_text(self.STAND_IN)
        self.binary.chmod(0o755)
        self.out = self.work / "dist"

    def test_the_tarball_holds_the_command_line_its_man_page_and_completions(self):
        done = run(PACK_COMMAND_LINE, self.binary, self.NAME, self.out)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(os.listdir(self.out), [f"{self.NAME}.tar.gz"])
        with tarfile.open(self.out / f"{self.NAME}.tar.gz") as tar:
            files = {
                member.name.removeprefix(f"{self.NAME}/"): member
                for member in tar.getmembers()
                if member.isfile()
            }
            read = {name: tar.extractfile(member).read() for name, member in files.items()}
            self.assertEqual(
                {member.name for member in tar.getmembers() if member.isdir()},
                {self.NAME, f"{self.NAME}/completions"},
            )
        self.assertEqual(
            set(read),
            {
                "pitboard",
                "pitboard.1",
                "completions/pitboard.bash",
                "completions/pitboard.zsh",
                "completions/pitboard.fish",
                "LICENSE",
                "NOTICE",
                "README.md",
            },
        )
        self.assertEqual(read["pitboard"], self.binary.read_bytes())
        self.assertEqual(files["pitboard"].mode & 0o111, 0o111)
        self.assertEqual(read["pitboard.1"], b'.TH pitboard 1 "pitboard 1.0.0-beta.7"\n')
        for shell in ("bash", "zsh", "fish"):
            self.assertEqual(
                read[f"completions/pitboard.{shell}"], f"# pitboard's {shell} completions\n".encode()
            )
        for file in ("LICENSE", "NOTICE", "README.md"):
            self.assertEqual(read[file], (REPO / file).read_bytes())

    def test_a_command_line_that_writes_no_man_page_packs_nothing(self):
        self.binary.write_text(self.STAND_IN.replace("manpage)", "never)"))
        done = run(PACK_COMMAND_LINE, self.binary, self.NAME, self.out)
        self.assertNotEqual(done.returncode, 0)
        self.assertFalse(self.out.exists() and os.listdir(self.out))

    def test_it_takes_three_arguments(self):
        for args in ((self.binary, self.NAME), (self.binary, self.NAME, self.out, self.out)):
            with self.subTest(args=len(args)):
                done = run(PACK_COMMAND_LINE, *args)
                self.assertEqual(done.returncode, 2)
                self.assertIn("usage:", done.stderr)


class LinuxSessionBundle(unittest.TestCase):
    NAME = "session-build-aarch64-linux"
    FILES = (
        "pitboard-v1.0.0-beta.7-aarch64.rpm",
        "pitboard-app-v1.0.0-beta.7-aarch64.deb",
        "pitboard-v1.0.0-beta.7-aarch64.deb",
        "pitboard-archive-keyring-v1.0.0-beta.7-all.deb",
        "pitboard-app-v1.0.0-beta.7-aarch64.rpm",
    )
    HEADER = b"age-encryption.org/v1\n-> X25519 stand-in\n"
    # A stand-in for age on PATH that keeps what it was asked and writes age's header for an X25519
    # key before the tar, or with STAND_IN_AGE=plain the tar alone, or with STAND_IN_AGE=fails part
    # of it before failing.
    STAND_IN = r"""#!/bin/sh
printf '%s\n' "$*" >"$STAND_IN_AGE_ASKED"
case ${STAND_IN_AGE-} in
  plain) cat "$6" >"$5" ;;
  fails)
    head -c 64 "$6" >"$5"
    exit 1
    ;;
  *) { printf 'age-encryption.org/v1\n-> X25519 stand-in\n'; cat "$6"; } >"$5" ;;
esac
"""

    def setUp(self):
        self.work = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.work)
        tools = self.work / "bin"
        tools.mkdir()
        (tools / "age").write_text(self.STAND_IN)
        (tools / "age").chmod(0o755)
        self.asked = self.work / "asked"
        self.env = {
            **{name: value for name, value in os.environ.items() if name != "STAND_IN_AGE"},
            "PATH": f"{tools}{os.pathsep}{os.environ['PATH']}",
            "STAND_IN_AGE_ASKED": str(self.asked),
        }
        self.out = self.work / "sealed"
        self.sealed = self.out / f"{self.NAME}.tar.age"

    def folder(self, under, files):
        folder = self.work / under
        folder.mkdir()
        for name in files:
            path = folder / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"{name}\n")
        return folder

    def pack(self, folder, name=NAME, **env):
        return subprocess.run(
            ["sh", PACK_LINUX_SESSION_BUNDLE, folder, name, self.out, "age", SessionGate.KEY],
            capture_output=True,
            text=True,
            env={**self.env, **env},
        )

    def assertRefused(self, done, said, folder):
        self.assertEqual(done.returncode, 1, done.stdout)
        self.assertIn(said, done.stderr)
        self.assertFalse(self.asked.exists())
        self.assertFalse((folder / "SHA256SUMS").exists())

    def test_the_bundle_is_the_folder_summed_and_sealed_to_the_key(self):
        folder = self.folder("session", self.FILES)
        done = self.pack(folder)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(done.stdout, f"{self.sealed}\n")
        self.assertEqual(os.listdir(self.out), [self.sealed.name])
        asked = self.asked.read_text().split()
        self.assertEqual(
            asked[:5], ["--encrypt", "--recipient", SessionGate.KEY, "--output", str(self.sealed)]
        )
        self.assertEqual(len(asked), 6)
        self.assertEqual(Path(asked[5]).name, f"{self.NAME}.tar")
        self.assertFalse(Path(asked[5]).exists())
        listed = sorted(self.FILES)
        sums = "".join(
            f"{hashlib.sha256((folder / name).read_bytes()).hexdigest()}  {name}\n"
            for name in listed
        )
        self.assertEqual((folder / "SHA256SUMS").read_text(), sums)
        sealed = self.sealed.read_bytes()
        self.assertEqual(sealed[: len(self.HEADER)], self.HEADER)
        with tarfile.open(fileobj=io.BytesIO(sealed[len(self.HEADER) :])) as tar:
            members = tar.getmembers()
            packed = {m.name: tar.extractfile(m).read() for m in members if m.isfile()}
        self.assertEqual(
            packed,
            {
                f"{self.NAME}/{name}": (folder / name).read_bytes()
                for name in [*listed, "SHA256SUMS"]
            },
        )
        self.assertEqual([m.name for m in members if m.isdir()], [self.NAME])

    def test_a_folder_with_nothing_to_ship_is_refused(self):
        for under, files in (("empty", ()), ("hollow", ("deb/",))):
            with self.subTest(folder=under):
                folder = self.folder(under, ())
                for name in files:
                    (folder / name).mkdir()
                done = self.pack(folder)
                self.assertRefused(done, f"{folder} holds nothing to pack", folder)
                self.assertFalse(self.out.exists())

    def test_what_was_packed_already_is_refused(self):
        summed = self.folder("summed", (*self.FILES, "SHA256SUMS"))
        done = self.pack(summed)
        self.assertEqual(done.returncode, 1, done.stdout)
        self.assertIn(f"{summed} holds a SHA256SUMS already", done.stderr)
        self.assertEqual((summed / "SHA256SUMS").read_text(), "SHA256SUMS\n")
        self.assertFalse(self.asked.exists())
        self.out.mkdir()
        self.sealed.write_bytes(b"an earlier bundle")
        folder = self.folder("session", self.FILES)
        self.assertRefused(self.pack(folder), f"{self.sealed} is there already", folder)
        self.assertEqual(self.sealed.read_bytes(), b"an earlier bundle")

    def test_a_name_with_a_space_is_refused(self):
        for under, name in (
            ("space", "pitboard a.deb"),
            ("newline", "pitboard\na.deb"),
            ("tab", "deb\tfolder/pitboard.deb"),
        ):
            with self.subTest(name=name):
                folder = self.folder(under, (*self.FILES, name))
                done = self.pack(folder)
                self.assertRefused(done, "has a space in its name, which SHA256SUMS cannot name", folder)

    def test_a_name_that_is_not_a_session_bundles_is_refused(self):
        folder = self.folder("session", self.FILES)
        for name in (
            "session-build-",
            "session-build-Aarch64-linux",
            "build-aarch64-linux",
            "session-build-../aarch64-linux",
            "session-build-aarch64 linux",
        ):
            with self.subTest(name=name):
                done = self.pack(folder, name)
                self.assertRefused(done, f"{name} is not a session bundle's name", folder)

    def test_nothing_is_left_of_a_bundle_age_did_not_encrypt(self):
        for under, age, said in (
            ("plain", "plain", "does not read as encrypted to an X25519 key"),
            ("fails", "fails", "age exited 1"),
        ):
            with self.subTest(age=age):
                folder = self.folder(under, self.FILES)
                done = self.pack(folder, STAND_IN_AGE=age)
                self.assertEqual(done.returncode, 1, done.stdout)
                self.assertIn(said, done.stderr)
                self.assertEqual(done.stdout, "")
                self.assertEqual(os.listdir(self.out), [])
                self.assertFalse((folder / "SHA256SUMS").exists())
                self.assertFalse(Path(self.asked.read_text().split()[5]).exists())
                self.asked.unlink()

    def test_it_takes_five_arguments(self):
        folder = self.folder("session", self.FILES)
        given = (folder, self.NAME, self.out, "age", SessionGate.KEY)
        for args in (given[:4], (*given, "more")):
            with self.subTest(args=len(args)):
                done = run(PACK_LINUX_SESSION_BUNDLE, *args)
                self.assertEqual(done.returncode, 2)
                self.assertIn("usage:", done.stderr)
                self.assertFalse((folder / "SHA256SUMS").exists())


class LinuxRelease(unittest.TestCase):
    # The Linux session's record as RELEASING.md has it written, its bundle's sentence wrapped.
    RECORD = """# The Linux session machines

Measured by hand on 2 November 2026 on the four machines. The bundle was
`linux-facts.yml` session run 41234567890 at 1a2b3c4d, the commit that holds
`Release {version}`, its sums checked on the Mac and on each machine.

The bundle was `linux-facts.yml` session run 41234567891 at 5e6f7a8b, the commit that holds
`Release 9.0.0`.
"""

    def setUp(self):
        self.work = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.work)
        self.record = self.work / "linux-session.md"

    def recorded(self, version="1.0.0-beta.8"):
        self.record.write_text(self.RECORD.format(version=version))

    def decided(self, version, kind, latest, published=()):
        lines = self.work / "published"
        lines.write_text("".join(f"{line}\n" for line in published))
        return run(LINUX_RELEASE, version, kind, latest, self.record, lines)

    def assertShips(self, done, linux, suites):
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(done.stderr, "")
        self.assertEqual(done.stdout, f"linux={linux}\nsuites={suites}\n")

    def assertRefused(self, done, said):
        self.assertEqual(done.returncode, 1, done.stdout)
        self.assertEqual(done.stdout, "")
        self.assertIn(said, done.stderr)

    def test_a_0_x_release_ships_no_linux_package(self):
        self.recorded()
        for version, kind, latest in (
            ("0.15.3", "full", "true"),
            ("0.15.3", "full", "false"),
            ("0.16.0-rc1", "rc", "false"),
        ):
            with self.subTest(version=version, latest=latest):
                done = self.decided(version, kind, latest, ["v1.0.0-beta.8 true true"])
                self.assertShips(done, "false", "")

    def test_without_a_session_record_nothing_ships(self):
        published = ["v0.15.2 false false", "v1.0.0-beta.7 true false", "v1.0.0-rc1 true false"]
        for version, kind, latest in (
            ("1.0.0-beta.8", "beta", "false"),
            ("1.0.0", "full", "true"),
        ):
            with self.subTest(version=version):
                self.assertShips(self.decided(version, kind, latest, published), "false", "")

    def test_a_missing_record_after_packages_shipped_is_refused(self):
        for shipped in ("v1.0.0-beta.8 true true", "v1.0.0-rc1 true true", "v1.0.0 false true"):
            with self.subTest(shipped=shipped):
                done = self.decided("1.0.1", "full", "true", ["v0.15.2 false false", shipped])
                self.assertRefused(
                    done, f"{self.record} is gone, and {shipped.split()[0]} shipped the Linux packages"
                )

    def test_a_record_that_names_no_bundle_or_a_later_one_is_refused(self):
        no_bundle = "names no bundle as 'The bundle was `linux-facts.yml` session run <run id> at"
        for record, said in (
            ("", no_bundle),
            ("# The Linux session machines\n", no_bundle),
            # The sentence in the second paragraph only.
            ("# The Linux session machines\n\nMeasured by hand.\n\n"
             + self.RECORD.split("\n\n", 1)[1], no_bundle),
            (self.RECORD.format(version="1.0.0-beta.8").replace("linux-facts", "windows-facts"),
             no_bundle),
            (self.RECORD.format(version="1.0.0-beta.8").replace("at 1a2b3c4d", "at main"),
             no_bundle),
            (self.RECORD.format(version="1.0.0-rc1"), "names the bundle of 1.0.0-rc1, which no"),
            (self.RECORD.format(version="0.15.2"), "names the bundle of 0.15.2, before 1.0.0-beta.1"),
            (self.RECORD.format(version="1.0.0-beta.9"),
             "names the bundle of 1.0.0-beta.9, above this release's 1.0.0-beta.8"),
        ):
            with self.subTest(record=record[-90:]):
                self.record.write_text(record)
                self.assertRefused(self.decided("1.0.0-beta.8", "beta", "false"), said)
        # An rc is measured on its release's bundle, and a later release on an earlier one.
        for version, kind, bundle in (
            ("1.0.0-rc1", "rc", "1.0.0"),
            ("1.0.0", "full", "1.0.0-beta.8"),
            ("1.10.0", "full", "1.9.0"),
        ):
            with self.subTest(version=version, bundle=bundle):
                self.recorded(bundle)
                self.assertEqual(self.decided(version, kind, "false").returncode, 0)

    def test_a_full_latest_release_writes_stable_and_beta_unless_a_beta_is_above_it(self):
        self.recorded()
        for published, suites in (
            ([], "stable beta"),
            (["v1.0.0-beta.8 true true", "v0.15.2 false false"], "stable beta"),
            (["v1.0.0-beta.8 true true", "v1.1.0-beta.1 true true"], "stable"),
            # A beta that shipped no packages is in no suite, and an rc never is.
            (["v1.1.0-beta.1 true false", "v1.1.0-rc1 true true"], "stable beta"),
            # This release, run again.
            (["v1.0.0 false true"], "stable beta"),
        ):
            with self.subTest(published=published):
                self.assertShips(self.decided("1.0.0", "full", "true", published), "true", suites)
        done = self.decided("1.0.1", "full", "false", ["v1.1.0 false true"])
        self.assertShips(done, "true", "")

    def test_a_beta_above_every_release_writes_beta_and_never_stable(self):
        self.recorded("1.0.0-beta.8")
        for version, published, suites in (
            ("1.0.0-beta.8", ["v0.15.2 false false", "v1.0.0-beta.7 true false"], "beta"),
            ("1.1.0-beta.1", ["v1.0.0 false true", "v1.0.0-beta.9 true true"], "beta"),
            ("1.1.0-beta.2", ["v1.1.0-beta.10 true true"], ""),
            ("1.1.0-beta.10", ["v1.1.0-beta.9 true true", "v1.1.0-rc1 true true"], "beta"),
            ("1.0.1-beta.1", ["v1.1.0 false true"], ""),
            ("1.1.0-beta.1", ["v1.1.0-beta.2 true true"], ""),
            ("1.1.0-beta.1", ["v1.1.0 false true"], ""),
        ):
            with self.subTest(version=version, published=published):
                done = self.decided(version, "beta", "false", published)
                self.assertShips(done, "true", suites)

    def test_a_release_candidate_writes_no_suite(self):
        self.recorded("1.0.0")
        for published in ([], ["v1.0.0-beta.8 true true"]):
            with self.subTest(published=published):
                self.assertShips(self.decided("1.0.0-rc1", "rc", "false", published), "true", "")

    def test_a_published_release_it_cannot_read_is_refused(self):
        self.recorded()
        for line in (
            "v1.0.0-beta.8 true",
            "v1.0.0-beta.8 true true more",
            "v1.0.0-beta.8 yes true",
            "1.0.0-beta.8 true true",
            "v1.0.0-beta.8 false true",
            "v1.0.0 true true",
        ):
            with self.subTest(line=line):
                done = self.decided("1.0.0", "full", "true", [line])
                self.assertRefused(done, f"cannot read the published release '{line}'")

    def test_a_release_it_was_not_given_is_refused(self):
        self.recorded()
        for version, kind, latest in (
            ("1.0.0", "beta", "true"),
            ("1.0.0-rc1", "full", "false"),
            ("1.0.0-beta.8", "rc", "false"),
            ("1.0.0", "full", "yes"),
            ("1.0", "full", "true"),
            ("1.0.0\n2.0.0", "full", "true"),
        ):
            with self.subTest(version=version, kind=kind, latest=latest):
                done = self.decided(version, kind, latest)
                self.assertRefused(done, "is not a release as release-kind.sh prints it")

    def test_it_takes_five_arguments(self):
        self.recorded()
        given = ("1.0.0", "full", "true", self.record, self.work / "published")
        for args in (given[:4], (*given, "more")):
            with self.subTest(args=len(args)):
                done = run(LINUX_RELEASE, *args)
                self.assertEqual(done.returncode, 2)
                self.assertIn("usage:", done.stderr)


class LinuxSigningKey(unittest.TestCase):
    PASSPHRASE = "a passphrase with spaces"
    # A stand-in for sign-linux-packages.sh and sign-linux-repository.sh: it keeps its GNUPGHOME
    # and environment, and signs its message with the subkey from that home, as gpg is asked
    # there, with nobody to ask for a passphrase.
    COMMAND = """printf '%s\\n' "$GNUPGHOME" >"$1"
env >"$2"
gpg --batch --yes --local-user "$3!" --output "$4.sig" --detach-sign "$4"
exit "${5:-0}"
"""

    def setUp(self):
        needs(self, "gpg", "gpgconf")
        self.work = Path(tempfile.mkdtemp(prefix="k"))
        self.addCleanup(shutil.rmtree, self.work)
        self.tmp = self.work / "t"
        self.tmp.mkdir()
        self.made = 0
        self.home = None

    def tearDown(self):
        for home in self.work.glob("maker-*"):
            subprocess.run(["gpgconf", "--kill", "all"], env={**os.environ, "GNUPGHOME": str(home)},
                           capture_output=True)

    def gpg(self, home, *args, given=None):
        done = subprocess.run(
            ["gpg", "--batch", "--quiet", "--pinentry-mode", "loopback", "--passphrase",
             self.PASSPHRASE, *args],
            input=given,
            capture_output=True,
            text=True,
            env={**os.environ, "GNUPGHOME": str(home)},
        )
        self.assertEqual(done.returncode, 0, done.stderr)
        return done.stdout

    # A key laid out as the repository's, made now, its secrets under PASSPHRASE: the public key,
    # the signing subkey's secret alone as PACKAGES_SIGNING_KEY holds it, and the whole secret.
    def key(self):
        self.made += 1
        home = self.work / f"maker-{self.made}"
        home.mkdir(mode=0o700)
        made = self.gpg(home, "--status-fd", "1", "--quick-gen-key", f"Test {self.made}",
                        "ed25519", "cert", "10y")
        primary = re.search(r"KEY_CREATED [PB] ([0-9A-F]{40})", made).group(1)
        self.gpg(home, "--quick-add-key", primary, "ed25519", "sign", "2y")
        listed = self.gpg(home, "--with-colons", "--with-subkey-fingerprints", "--list-keys")
        signing = re.findall(r"^fpr:+([0-9A-F]{40}):", listed, re.M)[1]
        public = self.work / f"{self.made}.asc"
        public.write_text(self.gpg(home, "--armor", "--export", primary))
        subkey = self.gpg(home, "--armor", "--export-secret-subkeys", f"{signing}!")
        whole = self.gpg(home, "--armor", "--export-secret-keys", primary)
        return home, primary, signing, public, subkey, whole

    def signed(self, public, secret, signing, passphrase=PASSPHRASE, exit=0, **env):
        self.message = self.work / "message"
        self.message.write_text("Release\n")
        self.kept, self.environment = self.work / "kept", self.work / "env"
        for path in (self.kept, self.environment, Path(f"{self.message}.sig")):
            path.unlink(missing_ok=True)
        given = {
            **{n: v for n, v in os.environ.items() if not n.startswith(("PACKAGES_", "GNUPGHOME"))},
            "TMPDIR": str(self.tmp),
            "PACKAGES_SIGNING_KEY": secret,
            "PACKAGES_SIGNING_PASSPHRASE": passphrase,
            **env,
        }
        done = subprocess.run(
            ["sh", WITH_LINUX_SIGNING_KEY, public, "sh", "-c", self.COMMAND, "sh", self.kept,
             self.environment, signing, self.message, str(exit)],
            capture_output=True,
            text=True,
            env={n: v for n, v in given.items() if v is not None},
        )
        self.assertEqual(os.listdir(self.tmp), [], "it left a GNUPGHOME behind")
        return done

    def assertRefused(self, done, said):
        self.assertEqual(done.returncode, 1, done.stdout)
        self.assertIn(said, done.stderr)
        self.assertFalse(self.kept.exists(), "the command ran")

    def test_the_command_signs_with_the_subkey_unasked_and_its_folder_goes(self):
        needs(self, "gpgv")
        _, _, signing, public, subkey, _ = self.key()
        done = self.signed(public, subkey, signing)
        self.assertEqual(done.returncode, 0, done.stderr)
        home = Path(self.kept.read_text().strip())
        self.assertEqual(home.parent, self.tmp)
        self.assertFalse(home.exists())
        environment = self.environment.read_text()
        self.assertNotIn("PACKAGES_SIGNING", environment)
        self.assertNotIn(self.PASSPHRASE, environment)
        keyring = self.work / "key.pgp"
        keyring.write_bytes(subprocess.run(["gpg", "--dearmor"], input=public.read_bytes(),
                                           capture_output=True, check=True).stdout)
        verified = subprocess.run(
            ["gpgv", "--keyring", keyring, f"{self.message}.sig", self.message],
            capture_output=True, text=True,
        )
        self.assertEqual(verified.returncode, 0, verified.stderr)

    def test_a_key_whose_subkey_was_replaced_signs_with_the_new_one(self):
        needs(self, "gpgv")
        home, primary, old, public, _, _ = self.key()
        # As RELEASING.md's Replace the signing subkey has the owner do it.
        self.gpg(home, "--command-fd", "0", "--edit-key", primary,
                 given="key 1\nrevkey\ny\n0\n\ny\nsave\n")
        self.gpg(home, "--quick-add-key", primary, "ed25519", "sign", "2y")
        listed = self.gpg(home, "--with-colons", "--with-subkey-fingerprints", "--list-keys")
        new = re.findall(r"^fpr:+([0-9A-F]{40}):", listed, re.M)[2]
        public.write_text(self.gpg(home, "--armor", "--export", primary))
        subkey = self.gpg(home, "--armor", "--export-secret-subkeys", f"{new}!")
        done = self.signed(public, subkey, new)
        self.assertEqual(done.returncode, 0, done.stderr)
        keyring = self.work / "key.pgp"
        keyring.write_bytes(subprocess.run(["gpg", "--dearmor"], input=public.read_bytes(),
                                           capture_output=True, check=True).stdout)
        verified = subprocess.run(
            ["gpgv", "--status-fd", "1", "--keyring", keyring, f"{self.message}.sig",
             self.message],
            capture_output=True, text=True,
        )
        self.assertEqual(verified.returncode, 0, verified.stderr)
        self.assertIn(f"VALIDSIG {new} ", verified.stdout)
        self.assertNotEqual(old, new)

    def test_the_commands_exit_is_its_own(self):
        _, _, signing, public, subkey, _ = self.key()
        done = self.signed(public, subkey, signing, exit=3)
        self.assertEqual(done.returncode, 3, done.stderr)
        self.assertFalse(Path(self.kept.read_text().strip()).exists())

    def test_a_secret_that_is_not_the_committed_subkey_is_refused(self):
        _, _, signing, public, _, _ = self.key()
        _, _, other_signing, _, other, _ = self.key()
        done = self.signed(public, other, other_signing)
        self.assertRefused(done, f"PACKAGES_SIGNING_KEY is not the signing subkey of {public}")
        self.assertIn(f"signing={other_signing}", done.stderr)
        self.assertIn(f"signing={signing}", done.stderr)

    def test_a_secret_with_an_older_binding_than_the_committed_key_is_refused(self):
        home, primary, signing, public, subkey, _ = self.key()
        # The key extended and committed, and PACKAGES_SIGNING_KEY not exported again.
        self.gpg(home, "--quick-set-expire", primary, "3y", signing)
        public.write_text(self.gpg(home, "--armor", "--export", primary))
        done = self.signed(public, subkey, signing)
        self.assertRefused(done, f"PACKAGES_SIGNING_KEY is not the signing subkey of {public}")
        self.assertIn("export it again", done.stderr)

    def test_a_secret_that_holds_the_primarys_is_refused(self):
        _, primary, signing, public, _, whole = self.key()
        done = self.signed(public, whole, signing)
        self.assertRefused(done, f"PACKAGES_SIGNING_KEY holds the secret of the primary {primary}")

    def test_a_passphrase_that_does_not_unlock_it_is_refused(self):
        _, _, signing, public, subkey, _ = self.key()
        done = self.signed(public, subkey, signing, passphrase="not the passphrase")
        self.assertRefused(done, f"PACKAGES_SIGNING_PASSPHRASE does not unlock {signing}")

    def test_a_missing_secret_or_passphrase_is_named(self):
        _, _, signing, public, subkey, _ = self.key()
        for name in ("PACKAGES_SIGNING_KEY", "PACKAGES_SIGNING_PASSPHRASE"):
            for value in (None, ""):
                with self.subTest(name=name, value=value):
                    done = self.signed(public, subkey, signing, **{name: value})
                    self.assertRefused(done, f"{name} is not set in the linux-packages environment")

    def test_it_takes_a_key_and_a_command(self):
        for args in ((), (REPOSITORY_KEY,)):
            with self.subTest(args=len(args)):
                done = run(WITH_LINUX_SIGNING_KEY, *args)
                self.assertEqual(done.returncode, 2)
                self.assertIn("usage:", done.stderr)


class UploadLinuxRepository(unittest.TestCase):
    ENDPOINT = "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com"
    SECRETS = {
        "PACKAGES_R2_ACCESS_KEY_ID": "an access key id",
        "PACKAGES_R2_SECRET_ACCESS_KEY": "a secret access key",
        "PACKAGES_R2_ENDPOINT": ENDPOINT,
    }
    HASH = "0" * 63
    # A tree as linux-repository.sh lays it out and sign-linux-repository.sh signs it, by class.
    PACKAGES = (
        "deb/pool/main/p/pitboard/pitboard_1.0.0~beta.8_arm64.deb",
        "deb/pool/main/p/pitboard/pitboard-app_1.0.0~beta.8_arm64.deb",
        "deb/pool/main/p/pitboard/pitboard-archive-keyring_1.0.0~beta.8_all.deb",
        "rpm/beta/aarch64/Packages/pitboard-1.0.0~beta.8-1.aarch64.rpm",
        "rpm/beta/aarch64/Packages/pitboard-app-1.0.0~beta.8-1.aarch64.rpm",
    )
    HASHED = (
        f"deb/dists/beta/main/binary-arm64/by-hash/SHA256/{HASH}1",
        f"deb/dists/beta/main/binary-amd64/by-hash/SHA256/{HASH}2",
        f"deb/dists/beta/main/by-hash/SHA256/{HASH}3",
        f"deb/dists/beta/main/dep11/by-hash/SHA256/{HASH}4",
        f"rpm/beta/aarch64/repodata/{HASH}5-primary.xml.zst",
        f"rpm/beta/aarch64/repodata/{HASH}6-appstream.xml.gz",
        f"rpm/beta/x86_64/repodata/{HASH}7-primary.xml.zst",
    )
    INDICES = (
        "deb/dists/beta/main/binary-arm64/Packages",
        "deb/dists/beta/main/binary-arm64/Packages.xz",
        "deb/dists/beta/main/binary-amd64/Packages.gz",
        "deb/dists/beta/main/Contents-arm64.gz",
        "deb/dists/beta/main/dep11/Components-arm64.yml",
        "deb/dists/beta/main/dep11/icons-64x64.tar",
    )
    SIGNED = (
        "deb/dists/beta/Release",
        "deb/dists/beta/Release.gpg",
        "deb/dists/beta/InRelease",
        "rpm/beta/aarch64/repodata/repomd.xml.asc",
        "rpm/beta/aarch64/repodata/repomd.xml",
        "rpm/beta/x86_64/repodata/repomd.xml.asc",
        "rpm/beta/x86_64/repodata/repomd.xml",
    )
    SETUP = (
        "pitboard-archive-keyring.asc",
        "pitboard-archive-keyring.pgp",
        "pitboard-beta.repo",
        "pitboard-beta.sources",
        "pitboard.pref",
        "pitboard.repo",
        "pitboard.sources",
    )
    KEPT = "public, max-age=31536000, immutable"
    # A stand-in for the AWS CLI on PATH that keeps each call's arguments and the credentials it
    # was given, one JSON line a call, and fails the call STAND_IN_AWS_FAILS counts to.
    STAND_IN = f"""#!{sys.executable}
import json, os, sys
with open(os.environ["STAND_IN_AWS_ASKED"], "a") as asked:
    asked.write(json.dumps({{
        "args": sys.argv[1:],
        "id": os.environ.get("AWS_ACCESS_KEY_ID"),
        "secret": os.environ.get("AWS_SECRET_ACCESS_KEY"),
        "leaked": sorted(n for n in os.environ if n.startswith("PACKAGES_R2_")),
    }}) + "\\n")
with open(os.environ["STAND_IN_AWS_ASKED"]) as asked:
    calls = len(asked.readlines())
sys.exit(1 if str(calls) == os.environ.get("STAND_IN_AWS_FAILS") else 0)
"""

    def setUp(self):
        self.work = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.work)
        tools = self.work / "bin"
        tools.mkdir()
        (tools / "aws").write_text(self.STAND_IN)
        (tools / "aws").chmod(0o755)
        self.asked = self.work / "asked"
        self.env = {
            **{n: v for n, v in os.environ.items() if not n.startswith(("PACKAGES_", "AWS_"))},
            **self.SECRETS,
            "PATH": f"{tools}{os.pathsep}{os.environ['PATH']}",
            "STAND_IN_AWS_ASKED": str(self.asked),
        }
        self.tree = self.work / "tree"
        for name in (*self.PACKAGES, *self.HASHED, *self.INDICES, *self.SIGNED, *self.SETUP):
            path = self.tree / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"{name}\n")

    def uploaded(self, **env):
        done = subprocess.run(
            ["sh", UPLOAD_LINUX_REPOSITORY, self.tree],
            capture_output=True,
            text=True,
            env={**self.env, **env},
        )
        calls = []
        if self.asked.exists():
            calls = [json.loads(line) for line in self.asked.read_text().splitlines()]
        return done, calls

    # Each call as the file it put and its Cache-Control, each to the same name in the bucket.
    def puts(self, calls):
        puts = []
        for call in calls:
            args = call["args"]
            self.assertEqual(args[:2], ["s3", "cp"], args)
            name = Path(args[2]).relative_to(self.tree).as_posix()
            self.assertEqual(
                args[3:],
                [
                    f"s3://pitboard-packages/{name}",
                    "--endpoint-url", self.ENDPOINT,
                    "--region", "auto",
                    "--cache-control", args[-2],
                    "--no-progress",
                ],
            )
            puts.append((name, args[-2]))
        return puts

    def order(self):
        done, calls = self.uploaded()
        self.assertEqual(done.returncode, 0, done.stderr)
        return [name for name, _ in self.puts(calls)]

    def test_files_go_up_before_the_indices_that_name_them(self):
        classes = (self.PACKAGES, self.HASHED, self.INDICES, self.SIGNED, self.SETUP)
        steps = [next(i for i, names in enumerate(classes) if name in names) for name in self.order()]
        self.assertEqual(steps, sorted(steps))

    def test_the_signed_indices_go_up_last(self):
        order = self.order()
        self.assertEqual(sorted(order[-len(self.SETUP) :]), sorted(self.SETUP))
        # A suite's Release before its signatures, and each repomd.xml after its own.
        self.assertEqual(
            order[-len(self.SETUP) - len(self.SIGNED) : -len(self.SETUP)],
            [
                "deb/dists/beta/Release",
                "deb/dists/beta/Release.gpg",
                "deb/dists/beta/InRelease",
                "rpm/beta/aarch64/repodata/repomd.xml.asc",
                "rpm/beta/aarch64/repodata/repomd.xml",
                "rpm/beta/x86_64/repodata/repomd.xml.asc",
                "rpm/beta/x86_64/repodata/repomd.xml",
            ],
        )

    def test_only_what_never_changes_is_kept_a_year(self):
        done, calls = self.uploaded()
        self.assertEqual(done.returncode, 0, done.stderr)
        for name, kept in self.puts(calls):
            with self.subTest(name=name):
                lasting = name in self.PACKAGES or name in self.HASHED
                self.assertEqual(kept, self.KEPT if lasting else "no-cache")

    def test_nothing_is_deleted(self):
        done, calls = self.uploaded()
        self.assertEqual(done.returncode, 0, done.stderr)
        names = [name for name, _ in self.puts(calls)]
        self.assertEqual(
            sorted(names),
            sorted((*self.PACKAGES, *self.HASHED, *self.INDICES, *self.SIGNED, *self.SETUP)),
        )
        for call in calls:
            self.assertFalse({"rm", "mv", "sync", "--delete", "--recursive"} & set(call["args"]))
            self.assertEqual((call["id"], call["secret"]), ("an access key id", "a secret access key"))
            self.assertEqual(call["leaked"], [])

    def test_a_failed_upload_stops_before_the_indices(self):
        done, calls = self.uploaded(STAND_IN_AWS_FAILS="2")
        self.assertNotEqual(done.returncode, 0)
        self.assertEqual(len(calls), 2)
        self.assertTrue(all(name in self.PACKAGES for name, _ in self.puts(calls)))

    def test_an_unsigned_tree_is_refused(self):
        for signature in ("deb/dists/beta/InRelease", "deb/dists/beta/Release.gpg",
                          "rpm/beta/x86_64/repodata/repomd.xml.asc"):
            with self.subTest(signature=signature):
                (self.tree / signature).rename(self.work / "kept")
                done, calls = self.uploaded()
                (self.work / "kept").rename(self.tree / signature)
                self.assertEqual(done.returncode, 1, done.stdout)
                self.assertIn(f"{self.tree}/{signature} is missing", done.stderr)
                self.assertEqual(calls, [])

    def test_a_missing_secret_or_endpoint_is_named(self):
        for name in self.SECRETS:
            for value in (None, ""):
                with self.subTest(name=name, value=value):
                    env = {**self.env}
                    if value is None:
                        del env[name]
                    else:
                        env[name] = value
                    done = subprocess.run(
                        ["sh", UPLOAD_LINUX_REPOSITORY, self.tree],
                        capture_output=True,
                        text=True,
                        env=env,
                    )
                    self.assertEqual(done.returncode, 1, done.stdout)
                    self.assertIn(f"{name} is not set in the linux-packages environment", done.stderr)
                    self.assertFalse(self.asked.exists())
        for endpoint in (
            "http://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com",
            "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com/pitboard-packages",
            "https://s3.amazonaws.com",
            "https://0123456789abcdef.r2.cloudflarestorage.com",
        ):
            with self.subTest(endpoint=endpoint):
                done, calls = self.uploaded(PACKAGES_R2_ENDPOINT=endpoint)
                self.assertEqual(done.returncode, 1, done.stdout)
                self.assertIn("PACKAGES_R2_ENDPOINT is not an R2 account's endpoint", done.stderr)
                self.assertEqual(calls, [])

    def test_it_takes_one_argument(self):
        for args in ((), (self.tree, self.tree)):
            with self.subTest(args=len(args)):
                done = run(UPLOAD_LINUX_REPOSITORY, *args)
                self.assertEqual(done.returncode, 2)
                self.assertIn("usage:", done.stderr)


class LinuxJobs(unittest.TestCase):
    # release.yml runs on a tag alone, and linux-packages takes deployments from v* tags only, so
    # what these hold of the workflows' text is what keeps the repository key and the bucket's
    # token off every pull request and branch.
    SECRETS = (
        "PACKAGES_SIGNING_KEY",
        "PACKAGES_SIGNING_PASSPHRASE",
        "PACKAGES_R2_ACCESS_KEY_ID",
        "PACKAGES_R2_SECRET_ACCESS_KEY",
        "PACKAGES_R2_ENDPOINT",
    )
    CACHES = re.compile(
        r"actions/cache\b|Swatinem/rust-cache|sccache|^\s*cache:\s*(?!\"?false\"?\s*$)\S|--cache-from",
        re.M,
    )

    # Each job of a workflow, by name, as the lines of its block.
    @staticmethod
    def jobs(workflow):
        lines = (WORKFLOWS / workflow).read_text().splitlines()
        start = lines.index("jobs:") + 1
        jobs, name = {}, None
        for line in lines[start:]:
            found = re.match(r"^  ([A-Za-z0-9_-]+):\s*$", line)
            if found:
                name = found.group(1)
                jobs[name] = []
            elif name and (not line.strip() or line.startswith("   ")):
                jobs[name].append(line)
        return {name: "\n".join(block) for name, block in jobs.items()}

    @staticmethod
    def needs(block):
        found = re.search(r"^    needs:\s*(.+)$", block, re.M)
        if not found:
            return []
        return [n.strip() for n in found.group(1).strip("[] ").split(",") if n.strip()]

    @staticmethod
    def environment(block):
        found = re.search(r"^    environment:\s*(\S*)\s*$(?:\n      name:\s*(\S+))?", block, re.M)
        return found and (found.group(1) or found.group(2))

    def test_only_linux_sign_and_linux_repository_reach_linux_packages(self):
        reaching = sorted(
            (workflow.name, job)
            for workflow in WORKFLOWS.glob("*.yml")
            for job, block in self.jobs(workflow.name).items()
            if self.environment(block) == "linux-packages"
        )
        self.assertEqual(reaching, [("release.yml", "linux-repository"), ("release.yml", "linux-sign")])
        text = (WORKFLOWS / "release.yml").read_text()
        self.assertIn('on:\n  push:\n    tags: ["v*"]\n\n', text)
        self.assertNotRegex(text, r"(?m)^  (pull_request|workflow_dispatch|workflow_call|schedule)")

    def test_no_job_on_the_linux_signing_path_takes_a_cache(self):
        jobs = self.jobs("release.yml")
        self.assertLessEqual({"linux-sign", "linux-repository"}, set(jobs))
        path, waiting = set(), ["linux-sign", "linux-repository"]
        while waiting:
            job = waiting.pop()
            if job not in path:
                path.add(job)
                waiting.extend(self.needs(jobs[job]))
        self.assertLessEqual({"guard", "build", "linux-build", "linux-sign", "publish"}, path)
        for job in sorted(path):
            with self.subTest(job=job):
                block = jobs[job]
                self.assertIsNone(self.CACHES.search(block), job)
                # The action restores main's Cargo cache unless told not to.
                uncached = r"uses: \./\.github/actions/rust\n\s+with:\n\s+cache: \"?false\"?"
                self.assertEqual(
                    block.count("uses: ./.github/actions/rust"), len(re.findall(uncached, block))
                )

    def test_publish_waits_for_the_signed_packages(self):
        jobs = self.jobs("release.yml")
        publish = jobs["publish"]
        self.assertIn("linux-sign", self.needs(publish))
        condition = re.search(r"^    if: >-\n((?:      .*\n)+)", publish + "\n", re.M).group(1)
        condition = " ".join(condition.split())
        self.assertIn(
            "((needs.guard.outputs.linux == 'false' && needs.linux-sign.result == 'skipped') || "
            "needs.linux-sign.result == 'success')",
            condition,
        )
        self.assertIn("pattern: dist-*", publish)
        uploads = {
            job: re.findall(r"uses: actions/upload-artifact@\S+.*\n\s+with:\n\s+name: (.+)", block)
            for job, block in jobs.items()
        }
        self.assertEqual(
            uploads["linux-build"], ["unsigned-linux-${{ matrix.format }}-${{ matrix.arch }}"]
        )
        handing = [job for job, names in uploads.items() if any("dist-linux" in n for n in names)]
        self.assertEqual(handing, ["linux-sign"])
        self.assertIn("linux-sign", self.needs(jobs["linux-repository"]) + self.needs(publish))
        self.assertIn("publish", self.needs(jobs["linux-repository"]))

    def test_no_workflow_but_release_names_the_packages_secrets(self):
        for workflow in sorted(WORKFLOWS.glob("*.yml")):
            text = workflow.read_text()
            with self.subTest(workflow=workflow.name):
                self.assertNotIn("secrets: inherit", text)
                if workflow.name != "release.yml":
                    for name in self.SECRETS:
                        self.assertNotIn(name, text)
        jobs = self.jobs("release.yml")
        self.assertLessEqual({"linux-sign", "linux-repository"}, set(jobs))
        for job, block in jobs.items():
            named = sorted(name for name in self.SECRETS if name in block)
            with self.subTest(job=job):
                if job == "linux-sign":
                    self.assertEqual(named, ["PACKAGES_SIGNING_KEY", "PACKAGES_SIGNING_PASSPHRASE"])
                elif job == "linux-repository":
                    self.assertEqual(named, sorted(self.SECRETS))
                else:
                    self.assertEqual(named, [])


class ScriptCalls(unittest.TestCase):
    # Most of release.yml runs only on a tag, so a parameter made mandatory and a caller left behind
    # would fail first in a published release.
    CALL = re.compile(
        r"(?:^|&\s*|-File\s+)(?:\"[^\"]*/|\.\./\.\./|\./)?\.github/scripts/([\w-]+)\.ps1\"?"
        r"|&\s*\(Join-Path\s+\$\w+\s+'([\w-]+)\.ps1'\)"
    )
    RUN = re.compile(r"^(\s*)(?:-\s+)?run:\s*(.*)$")

    def mandatory(self):
        found = {}
        for script in SCRIPTS.glob("*.ps1"):
            text = script.read_text()
            names = re.findall(r"^\s*\[Parameter\(Mandatory\)\].*\$(\w+),?\s*$", text, re.M)
            if names:
                found[script.stem] = names
        return found

    def commands(self, lines, folded):
        commands, current = [], ""
        for line in lines:
            stripped = line.strip()
            if not stripped:
                if current:
                    commands.append(current)
                current = ""
                continue
            if stripped.startswith("#"):
                continue
            current = f"{current} {stripped}" if current else stripped
            if not folded and not stripped.endswith(("`", " \\")):
                commands.append(current)
                current = ""
        if current:
            commands.append(current)
        return commands

    def blocks(self):
        for workflow in sorted(WORKFLOWS.glob("*.yml")):
            lines = workflow.read_text().splitlines()
            for at, line in enumerate(lines):
                found = self.RUN.match(line)
                if not found:
                    continue
                indent, value = len(found.group(1)), found.group(2).strip()
                if value not in ("|", "|-", ">", ">-"):
                    yield workflow.name, at + 1, [value], False
                    continue
                block = []
                for inner in lines[at + 1 :]:
                    if inner.strip() and len(inner) - len(inner.lstrip()) <= indent:
                        break
                    block.append(inner)
                yield workflow.name, at + 1, block, value.startswith(">")
        for script in sorted(SCRIPTS.glob("*.ps1")):
            yield script.name, 1, script.read_text().splitlines(), False

    def test_every_call_of_a_script_names_each_of_its_mandatory_parameters(self):
        mandatory = self.mandatory()
        self.assertIn("Completions", mandatory["check-windows-msi"])
        calls, missing = 0, []
        for name, at, block, folded in self.blocks():
            text = "\n".join(block)
            for command in self.commands(block, folded):
                for call in self.CALL.finditer(command):
                    script = call.group(1) or call.group(2)
                    if script not in mandatory:
                        continue
                    calls += 1
                    rest = command[call.end() :]
                    given = {g.lower() for g in re.findall(r"-(\w+)", rest)}
                    for splat in re.findall(r"(?<![\w$])@(\w+)", rest):
                        table = re.search(rf"\${splat}\s*=\s*@{{([^}}]*)}}", text)
                        if table:
                            given |= {k.lower() for k in re.findall(r"(\w+)\s*=", table.group(1))}
                        given |= {k.lower() for k in re.findall(rf"\${splat}\.(\w+)\s*=", text)}
                    lacking = [p for p in mandatory[script] if p.lower() not in given]
                    if lacking:
                        missing.append(f"{name}:{at} {script}.ps1 without -{', -'.join(lacking)}")
        self.assertGreater(calls, 20)
        self.assertEqual(missing, [])


if __name__ == "__main__":
    unittest.main()
