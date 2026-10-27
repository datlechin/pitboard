# packaging/install.sh, filled by fill-template.sh, run against stub releases under each shell here.
import hashlib
import io
import json
import os
import re
import shutil
import signal
import stat
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
TEMPLATE = REPO / "packaging" / "install.sh"
FILL = REPO / ".github" / "scripts" / "fill-template.sh"
CASKS = (REPO / "packaging" / "pitboard.rb", REPO / "packaging" / "pitboard-app.rb")
SCOOP = REPO / "packaging" / "scoop" / "pitboard.json"
VERSION = "9.8.7"
DOWNLOADS = "https://github.com/datlechin/pitboard/releases/download/"
UNAME = {
    "aarch64-apple-darwin": ("Darwin", "arm64"),
    "x86_64-apple-darwin": ("Darwin", "x86_64"),
    "aarch64-unknown-linux-musl": ("Linux", "aarch64"),
    "x86_64-unknown-linux-musl": ("Linux", "x86_64"),
}
TARGETS = tuple(UNAME)
WINDOWS_TARGETS = {"64bit": "x86_64-pc-windows-msvc", "arm64": "aarch64-pc-windows-msvc"}
LINUX = UNAME["x86_64-unknown-linux-musl"]
MAC = UNAME["aarch64-apple-darwin"]
INSTALLED = {
    "bin/pitboard": "pitboard",
    "share/man/man1/pitboard.1": "pitboard.1",
    "share/bash-completion/completions/pitboard": "completions/pitboard.bash",
    "share/zsh/site-functions/_pitboard": "completions/pitboard.zsh",
    "share/fish/vendor_completions.d/pitboard.fish": "completions/pitboard.fish",
}
TOOLS = ("chmod", "cp", "find", "gzip", "mkdir", "mktemp", "mv", "rm", "tar")
HASHERS = ("sha256sum", "shasum")

ROOT_REFUSAL = (
    "Pitboard changes nothing when it runs as root or with sudo, so this installs nothing. "
    "Run it as yourself."
)
LINUX_ROOT_REFUSAL = (
    "Pitboard changes nothing when it runs with sudo or as root in a folder another user owns, "
    "so this installs nothing. Run it as yourself."
)
WINDOWS = (
    "This script installs Pitboard on macOS and Linux. For Windows, see "
    "https://docs.usepitboard.com/install."
)
NO_ARGUMENTS = (
    "The installer takes no arguments. Set PITBOARD_INSTALL_DIR to choose where it installs."
)
NO_HOME = {
    None: "HOME is not set. Set it to a full path.",
    "": "HOME is empty. Set it to a full path.",
    "home": "HOME is `home`, which is not a full path. Set it to a full path.",
}
NO_DOWNLOADER = "The installer needs curl or wget, and neither is on PATH."
NO_HASHER = "The installer needs sha256sum or shasum to check the download, and neither is on PATH."
MISMATCH = (
    "The download does not match the checksum this installer carries, so nothing was installed."
)
DOES_NOT_RUN = "The downloaded command line does not run from {}, so nothing was installed."
CANNOT_WRITE = "This account cannot write to {}, so nothing was installed."
APP_NOTE = (
    "The Pitboard app carries its own command line, which updates with the app. This one "
    "updates when you run this installer again, so keep it at the app's version."
)
ZSH_HINT = (
    "zsh loads the completions once ~/.zshrc has these lines. Where it runs compinit already, "
    "add only the first, before it:\n"
    '  fpath=("{}/zsh/site-functions" $fpath)\n'
    "  autoload -Uz compinit && compinit\n"
)
NEXT = "Next, run `pitboard`: it says what is signed in and how to enrol it.\n"

WORK = None
SHELLS = []
REAL_HOME = None
GOOD = None
BROKEN = None
TAMPERED = None
EMPTY = None

# -B: the Python macOS ships writes its caches under HOME otherwise.
NET_SHIM = """#!{python} -B
import json, os, sys
tool = {tool!r}
args = sys.argv[1:]
try:
    stdin = sys.stdin.buffer.read()
except (AttributeError, OSError):
    stdin = b""
with open(os.environ["SHIM_LOG"], "a") as log:
    log.write(json.dumps({{"tool": tool, "args": args, "stdin": len(stdin)}}) + "\\n")
if os.environ.get("SHIM_HOLD"):
    os.spawnv(os.P_WAIT, {python!r}, [{python!r}, "-B", {hold!r}, os.environ["SHIM_HOLD"]])
url = args[-1] if args else ""
prefix = {prefix!r}
if url.startswith(prefix):
    path = os.path.join(os.environ["SHIM_SERVE"], url[len(prefix):])
    if os.path.isfile(path):
        with open(path, "rb") as served:
            sys.stdout.buffer.write(served.read())
        sys.exit(0)
if tool == "curl":
    sys.stderr.write("curl: (22) The requested URL returned error: 404\\n")
    sys.exit(22)
sys.stderr.write("wget: server returned error: HTTP/1.1 404 Not Found\\n")
sys.exit(8)
"""

# Holds a run where a test signals it, until the test makes <marker>.go or ten seconds pass.
HOLD = """import os, sys, time
held = sys.argv[1]
open(held, "w").close()
for _ in range(1000):
    if os.path.exists(held + ".go"):
        break
    time.sleep(0.01)
"""

# macOS mktemp puts a bare -d in the per-user temp dir and ignores TMPDIR.
MKTEMP = """#!/bin/sh
if [ "$*" = -d ]; then exec {real} -d "${{TMPDIR:?}}/tmp.XXXXXX"; fi
case $#:${{1:-}} in
  1:*/.pitboard.XXXXXX) exec {real} "$1" ;;
esac
echo "mktemp shim: unexpected $*" >&2
exit 1
"""

# A cp that fails as a full disk would, when it writes the staged copy.
FULL_CP = """#!/bin/sh
for last; do :; done
case $last in
  */.pitboard.*) echo "cp: $last: No space left on device" >&2; exit 1 ;;
esac
exec {real} "$@"
"""

BASE_SHIMS = {
    "uname": """#!/bin/sh
case $* in
  -s) printf '%s\\n' "$SHIM_UNAME_S" ;;
  -m) printf '%s\\n' "$SHIM_UNAME_M" ;;
  *) echo "uname shim: unexpected $*" >&2; exit 1 ;;
esac
""",
    "id": """#!/bin/sh
if [ "$*" != -u ]; then echo "id shim: unexpected $*" >&2; exit 1; fi
printf '%s\\n' "$SHIM_UID"
""",
    # Unset SHIM_TRANSLATED answers as an Intel Mac does, where the key does not exist.
    "sysctl": """#!/bin/sh
if [ "$*" = "-n sysctl.proc_translated" ] && [ -n "${SHIM_TRANSLATED:-}" ]; then
  printf '%s\\n' "$SHIM_TRANSLATED"
  exit 0
fi
echo "sysctl: unknown oid '${2:-}'" >&2
exit 1
""",
}


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def stub(target, runs):
    if not runs:
        return "#!/bin/sh\necho 'cannot execute binary file' >&2\nexit 126\n"
    return (
        "#!/bin/sh\n"
        f"# {target}\n"
        'if [ -n "${STUB_HOLD:-}" ]; then\n'
        f'  "{sys.executable}" -B "{WORK}/hold.py" "$STUB_HOLD"\n'
        "fi\n"
        'if IFS= read -r line; then echo "stdin: $line" >>"${STUB_LOG:-/dev/null}"; fi\n'
        'echo "$*" >>"${STUB_LOG:-/dev/null}"\n'
        'if [ -n "${STUB_LOG:-}" ]; then echo "$0" >>"$STUB_LOG.argv0"; fi\n'
        f'echo "pitboard {VERSION}"\n'
    )


class Release:
    def __init__(self, where, runs=True):
        self.serve = where / "serve"
        folder = self.serve / f"v{VERSION}"
        folder.mkdir(parents=True)
        self.members = {}
        lines = []
        for target in TARGETS:
            name = f"pitboard-v{VERSION}-{target}"
            members = {
                "pitboard": (stub(target, runs), 0o755),
                "pitboard.1": f'.TH pitboard 1  "pitboard {VERSION}"\n.\\" {target}\n',
                "completions/pitboard.bash": f"# {target}\ncomplete -F _pitboard pitboard\n",
                "completions/pitboard.zsh": f"#compdef pitboard\n# {target}\n",
                "completions/pitboard.fish": f"# {target}\ncomplete -c pitboard\n",
                "LICENSE": "MIT\n",
                "NOTICE": "Pitboard\n",
                "README.md": "# Pitboard\n",
            }
            self.members[target] = {}
            tarball = folder / f"{name}.tar.gz"
            with tarfile.open(tarball, "w:gz", format=tarfile.GNU_FORMAT) as tar:
                for directory in (name, f"{name}/completions"):
                    info = tarfile.TarInfo(directory)
                    info.type = tarfile.DIRTYPE
                    info.mode = 0o755
                    tar.addfile(info)
                for member, content in members.items():
                    text, mode = content if isinstance(content, tuple) else (content, 0o644)
                    data = text.encode()
                    self.members[target][member] = data
                    info = tarfile.TarInfo(f"{name}/{member}")
                    info.size = len(data)
                    info.mode = mode
                    info.uid = info.gid = 1001
                    info.mtime = 1760000000
                    tar.addfile(info, io.BytesIO(data))
            lines.append(f"{sha256(tarball)}  {tarball.name}")
        app = folder / f"Pitboard-v{VERSION}-macos.zip"
        app.write_bytes(b"a stand-in for the app\n")
        lines.append(f"{sha256(app)}  {app.name}")
        self.sums = where / "SHA256SUMS"
        self.sums.write_text("\n".join(lines) + "\n")
        self.script = where / "install.sh"
        done = fill(TEMPLATE, self.script, self.sums)
        if done.returncode != 0:
            raise RuntimeError(f"fill-template.sh could not fill {TEMPLATE}:\n{done.stderr}")


def fill(template, out, sums, version=VERSION):
    return subprocess.run(
        ["sh", str(FILL), str(template), str(out), version, str(sums)],
        capture_output=True,
        text=True,
    )


def find_shells():
    sh = shutil.which("sh")
    found = []
    for name, argv in (
        ("sh", ["sh"]),
        ("dash", ["dash"]),
        ("bash", ["bash"]),
        ("zsh", ["zsh"]),
        ("busybox", ["busybox", "sh"]),
    ):
        exe = shutil.which(argv[0])
        if exe is None:
            continue
        # dash and BusyBox act the same under any name, so a sh that is one of them ran them.
        if name in ("dash", "busybox") and sh and os.path.realpath(exe) == os.path.realpath(sh):
            continue
        found.append((name, [exe, *argv[1:]]))
    return found


def seen_by(shell, name, value):
    # zsh reads ${ZDOTDIR:-$HOME}/.zshenv, so the probe reads an empty folder's.
    empty = WORK / "probe"
    empty.mkdir(exist_ok=True)
    env = {"PATH": "/usr/bin:/bin", "HOME": str(empty), "ZDOTDIR": str(empty)}
    env.pop(name, None)
    if value is not None:
        env[name] = value
    probe = f'if [ "${{{name}+set}}" = set ]; then printf "%s" "${name}"; else printf "<unset>"; fi'
    seen = subprocess.run([*shell, "-c", probe], env=env, capture_output=True, text=True).stdout
    return None if seen == "<unset>" else seen


def net_shims(downloaders):
    folder = WORK / "shims" / ("+".join(downloaders) or "none")
    if not folder.exists():
        folder.mkdir(parents=True)
        for tool in downloaders:
            shim = folder / tool
            shim.write_text(
                NET_SHIM.format(
                    python=sys.executable, tool=tool, prefix=DOWNLOADS, hold=str(WORK / "hold.py")
                )
            )
            shim.chmod(0o755)
    return folder


def tools(hashers):
    folder = WORK / "tools" / ("+".join(hashers) or "none")
    if not folder.exists():
        folder.mkdir(parents=True)
        for tool in TOOLS + hashers:
            real = shutil.which(tool)
            if real is None:
                if tool in HASHERS:
                    continue
                raise RuntimeError(f"the tests need {tool} on PATH")
            if tool == "mktemp":
                (folder / tool).write_text(MKTEMP.format(real=real))
                (folder / tool).chmod(0o755)
            else:
                (folder / tool).symlink_to(real)
    return folder


# A test run in the background ignores SIGINT, and a shell cannot trap what it starts ignoring.
def default_signals():
    for number in (signal.SIGINT, signal.SIGHUP, signal.SIGTERM):
        signal.signal(number, signal.SIG_DFL)


def full_cp():
    folder = WORK / "shims" / "full"
    if not folder.exists():
        folder.mkdir(parents=True)
        (folder / "cp").write_text(FULL_CP.format(real=shutil.which("cp")))
        (folder / "cp").chmod(0o755)
    return folder


def snapshot(top, skip):
    seen = {}
    for dirpath, dirnames, filenames in os.walk(top):
        here = Path(dirpath)
        dirnames[:] = [d for d in dirnames if here / d not in skip]
        for entry in dirnames + filenames:
            path = here / entry
            st = path.lstat()
            seen[str(path)] = (st.st_mode, st.st_size, st.st_mtime_ns, st.st_ino)
    return seen


def entries(top):
    found = []
    for dirpath, dirnames, filenames in os.walk(top):
        here = Path(dirpath)
        for entry in dirnames + filenames:
            found.append(str((here / entry).relative_to(top)))
    return sorted(found)


class Result:
    def __init__(self, done, case):
        self.rc = done.returncode
        self.out = done.stdout.decode()
        self.err = done.stderr.decode()
        net = case.log / "net"
        self.requests = (
            [json.loads(line) for line in net.read_text().splitlines()] if net.exists() else []
        )
        ran = case.log / "stub"
        self.ran = ran.read_text().splitlines() if ran.exists() else []
        argv0 = case.log / "stub.argv0"
        self.argv0 = argv0.read_text().splitlines() if argv0.exists() else []
        self.entries = entries(case.home)
        self.files = sorted(e for e in self.entries if not (case.home / e).is_dir())


class Case:
    def __init__(self, test):
        self.test = test
        self.dir = Path(tempfile.mkdtemp(prefix="case.", dir=str(WORK / "cases")))
        self.home = self.dir / "home"
        self.tmp = self.dir / "tmp"
        self.log = self.dir / "log"
        for folder in (self.home, self.tmp, self.log):
            folder.mkdir()
        test.addCleanup(shutil.rmtree, str(self.dir), True)

    def environ(
        self,
        serve=None,
        uname=LINUX,
        uid="1000",
        translated=None,
        downloaders=("curl", "wget"),
        hashers=HASHERS,
        env=None,
        unset=(),
        before=(),
    ):
        path = [net_shims(downloaders), WORK / "shims" / "base", *before, tools(hashers)]
        environ = {
            "PATH": ":".join(str(p) for p in path),
            "HOME": str(self.home),
            "TMPDIR": str(self.tmp),
            "SHELL": "/bin/sh",
            "LC_ALL": "C",
            "SHIM_LOG": str(self.log / "net"),
            "SHIM_SERVE": str(serve or GOOD.serve),
            "SHIM_UNAME_S": uname[0],
            "SHIM_UNAME_M": uname[1],
            "SHIM_UID": uid,
            "STUB_LOG": str(self.log / "stub"),
        }
        if translated is not None:
            environ["SHIM_TRANSLATED"] = translated
        environ.update(env or {})
        for name in unset:
            environ.pop(name, None)
        return environ

    def run(self, script, shell, args=(), allowed=(), **kwargs):
        for old in self.log.iterdir():
            old.unlink()
        environ = self.environ(**kwargs)
        data = script if isinstance(script, bytes) else Path(script).read_bytes()
        # More after main, so a child that reads the script's stdin is caught doing it.
        data += b"\nexit 0\n" + (b"#" * 99 + b"\n") * 2000
        skip = {self.home, self.tmp, self.log, *allowed}
        before_run = snapshot(WORK, skip)
        done = subprocess.run(
            [*shell, "-s", *(["--", *args] if args else [])],
            input=data,
            env=environ,
            cwd=str(self.dir),
            capture_output=True,
            timeout=60,
        )
        self.test.assertEqual(snapshot(WORK, skip), before_run, "it wrote outside HOME")
        self.test.assertEqual(os.listdir(self.tmp), [], "it left files in TMPDIR")
        return Result(done, self)


def real_home():
    found = {}
    for rel in INSTALLED:
        try:
            st = (Path(os.path.expanduser("~")) / ".local" / rel).lstat()
            found[rel] = (st.st_ino, st.st_size, st.st_mtime_ns)
        except FileNotFoundError:
            found[rel] = None
    return found


def setUpModule():
    global WORK, SHELLS, REAL_HOME
    REAL_HOME = real_home()
    WORK = Path(tempfile.mkdtemp(prefix="test-install."))
    (WORK / "cases").mkdir()
    base = WORK / "shims" / "base"
    base.mkdir(parents=True)
    for name, text in BASE_SHIMS.items():
        (base / name).write_text(text)
        (base / name).chmod(0o755)
    (WORK / "hold.py").write_text(HOLD)
    SHELLS = find_shells()
    required = os.environ.get("TEST_INSTALL_SHELLS", "").split()
    missing = [name for name in required if shutil.which(name) is None]
    if missing:
        raise RuntimeError(f"TEST_INSTALL_SHELLS names {' '.join(missing)}, not on PATH")
    names = ", ".join(f"{name} ({os.path.realpath(argv[0])})" for name, argv in SHELLS)
    print(f"shells: {names}", file=sys.stderr)


def tearDownModule():
    shutil.rmtree(str(WORK), True)
    if real_home() != REAL_HOME:
        raise AssertionError("the real home's ~/.local changed while the tests ran")


class InstallScript(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        global GOOD, BROKEN, TAMPERED, EMPTY
        releases = WORK / "releases"
        GOOD = Release(releases / "good")
        BROKEN = Release(releases / "broken", runs=False)
        TAMPERED = releases / "tampered"
        shutil.copytree(str(GOOD.serve), str(TAMPERED))
        for tarball in TAMPERED.rglob("*.tar.gz"):
            with open(tarball, "ab") as f:
                f.write(b"\0")
        EMPTY = releases / "empty"
        EMPTY.mkdir()

    def case(self):
        return Case(self)

    def each_shell(self):
        self.assertTrue(SHELLS, "no shell to run the installer under")
        for name, argv in SHELLS:
            with self.subTest(shell=name):
                yield argv

    def assertInstalled(self, case, result, target=TARGETS[3], prefix=".local"):
        self.assertEqual(result.rc, 0, result.err)
        self.assertEqual(result.err, "")
        url = f"{DOWNLOADS}v{VERSION}/pitboard-v{VERSION}-{target}.tar.gz"
        self.assertEqual(len(result.requests), 1, result.requests)
        request = result.requests[0]
        self.assertEqual(request["args"][-1], url)
        self.assertEqual(request["stdin"], 0, "the downloader could read the script")
        args = request["args"]
        if request["tool"] == "curl":
            self.assertEqual(args[0], "-q", "curl read a .curlrc")
            self.assertEqual(args[args.index("--proto") + 1], "=https")
            self.assertIn("--tlsv1.2", args)
            self.assertIn("-fsSL", args)
            self.assertNotIn("-o", args)
        else:
            self.assertEqual(args, ["-q", "-O", "-", url])
        self.assertEqual(result.ran, ["--version"], "pitboard ran other than with --version")
        bindir = case.home / prefix / "bin"
        self.assertEqual(len(result.argv0), 1, result.argv0)
        # Run from its staged copy, as /tmp may be noexec.
        staged = "^" + re.escape(str(bindir)) + r"/\.pitboard\.[A-Za-z0-9]{6}$"
        self.assertRegex(result.argv0[0], staged)
        expected = {f"{prefix}/{rel}": member for rel, member in INSTALLED.items()}
        self.assertEqual(result.files, sorted(expected))
        for rel, member in expected.items():
            path = case.home / rel
            mode = stat.S_IMODE(path.lstat().st_mode)
            self.assertTrue(stat.S_ISREG(path.lstat().st_mode), rel)
            self.assertEqual(mode, 0o755 if member == "pitboard" else 0o644, rel)
            self.assertEqual(path.read_bytes(), GOOD.members[target][member], rel)
        self.assertIn(f"Downloading {url}\n", result.out)
        share = case.home / prefix / "share"
        self.assertIn(
            f"Installed Pitboard {VERSION} at {bindir}/pitboard, and its man page and completions "
            f"in {share}.\n",
            result.out,
        )

    def assertRefused(self, result, message):
        self.assertEqual(result.rc, 1, result.err)
        self.assertEqual(result.err, f"error: {message}\n")
        self.assertEqual(result.out, "")
        self.assertEqual(result.requests, [])
        self.assertEqual(result.ran, [])
        self.assertEqual(result.entries, [])

    def assertNothingInstalled(self, result, message):
        self.assertEqual(result.rc, 1)
        self.assertTrue(result.err.endswith(f"error: {message}\n"), result.err)
        self.assertEqual(result.entries, [])

    def test_installs_the_command_line_its_man_page_and_completions_for_each_target(self):
        for shell in self.each_shell():
            for target in TARGETS:
                with self.subTest(target=target):
                    case = self.case()
                    result = case.run(GOOD.script, shell, uname=UNAME[target])
                    self.assertInstalled(case, result, target)

    def test_a_second_run_renames_a_new_copy_over_the_old_one(self):
        for shell in self.each_shell():
            case = self.case()
            bindir = case.home / ".local" / "bin"
            bindir.mkdir(parents=True)
            (bindir / "pitboard").write_text("#!/bin/sh\necho pitboard 0.0.1\n")
            first = (bindir / "pitboard").stat().st_ino
            self.assertInstalled(case, case.run(GOOD.script, shell))
            second = (bindir / "pitboard").stat().st_ino
            self.assertInstalled(case, case.run(GOOD.script, shell))
            third = (bindir / "pitboard").stat().st_ino
            self.assertEqual(len({first, second, third}), 3)
            self.assertEqual(os.listdir(bindir), ["pitboard"])

    def test_a_link_in_place_is_replaced_and_not_written_through(self):
        for shell in self.each_shell():
            case = self.case()
            app = case.home / "Pitboard.app"
            app.mkdir()
            for rel in ("bin/pitboard", "share/man/man1/pitboard.1"):
                (app / rel.replace("/", "-")).write_text("the app's own\n")
                link = case.home / ".local" / rel
                link.parent.mkdir(parents=True, exist_ok=True)
                link.symlink_to(app / rel.replace("/", "-"))
            result = case.run(GOOD.script, shell)
            self.assertEqual(result.rc, 0, result.err)
            for rel in ("bin/pitboard", "share/man/man1/pitboard.1"):
                self.assertFalse((case.home / ".local" / rel).is_symlink(), rel)
                self.assertEqual((app / rel.replace("/", "-")).read_text(), "the app's own\n")

    def test_a_download_that_does_not_match_installs_nothing(self):
        for shell in self.each_shell():
            case = self.case()
            result = case.run(GOOD.script, shell, serve=TAMPERED)
            self.assertEqual(result.err, f"error: {MISMATCH}\n")
            self.assertNothingInstalled(result, MISMATCH)
            self.assertEqual(result.ran, [])

    def test_a_failed_download_installs_nothing(self):
        for shell in self.each_shell():
            case = self.case()
            result = case.run(GOOD.script, shell, serve=EMPTY)
            self.assertIn("404", result.err)
            self.assertNothingInstalled(
                result, "The download failed, so nothing was installed."
            )

    def test_a_cut_script_runs_nothing(self):
        text = GOOD.script.read_text()
        self.assertTrue(text.endswith('\nmain "$@"\n}\n'), "main is not the last line in braces")
        # Past the brace, every cut is one the shell refuses to run.
        brace = text.index("\n{\n") + 1
        cuts = {brace, brace + 1}
        offset = 0
        body = None
        for line in text.splitlines(keepends=True):
            if body is None:
                cuts.add(offset)
                if re.match(r"[a-z_]+\(\) \{$", line):
                    cuts.add(offset + len(line) - 1)
                    body = offset + len(line)
            elif line == "}\n":
                cuts.add((body + offset) // 2)
                cuts.add(offset)
                body = None
            offset += len(line)
        self.assertIsNone(body, "a function is not closed")
        self.assertGreater(len(cuts), 40)
        cuts.add(text.index('SHA256_AARCH64_APPLE_DARWIN="') + 40)
        last = text.rindex('main "$@"')
        cuts.update(last + len(part) for part in ("main", 'main "$', 'main "$@'))
        cuts.add(len(text) - 2)
        for shell in self.each_shell():
            case = self.case()
            for cut in sorted(cuts):
                with self.subTest(cut=cut):
                    result = case.run(text[:cut].encode(), shell)
                    self.assertEqual(result.requests, [])
                    self.assertEqual(result.ran, [])
                    self.assertEqual(result.entries, [])
                    self.assertEqual(result.out, "")
                    if cut > brace:
                        self.assertNotEqual(result.rc, 0)

    def test_root_and_sudo_are_refused(self):
        for shell in self.each_shell():
            for uname, refusal in ((MAC, ROOT_REFUSAL), (LINUX, LINUX_ROOT_REFUSAL)):
                result = self.case().run(GOOD.script, shell, uname=uname, env={"SUDO_UID": "501"})
                self.assertRefused(result, refusal)
            # "/" is root's, as no test's own home is.
            result = self.case().run(GOOD.script, shell, uname=MAC, uid="0", env={"HOME": "/"})
            self.assertRefused(result, ROOT_REFUSAL)
            # Set but empty, SUDO_UID says nothing, as Pitboard reads it.
            case = self.case()
            self.assertInstalled(case, case.run(GOOD.script, shell, env={"SUDO_UID": ""}))

    # As on a machine whose only user is root. `su -m` keeps the caller's HOME, and plain `su`
    # the caller's PITBOARD_INSTALL_DIR.
    def test_on_linux_root_installs_only_into_folders_root_owns(self):
        for shell in self.each_shell():
            if os.geteuid() == 0:
                case = self.case()
                self.assertInstalled(case, case.run(GOOD.script, shell, uid="0"))
                continue
            # Root's own folders pass, and this run, not root, cannot write there.
            absent = "/pitboard-test-absent/bin"
            result = self.case().run(
                GOOD.script, shell, uid="0", env={"HOME": "/", "PITBOARD_INSTALL_DIR": absent}
            )
            self.assertNothingInstalled(result, CANNOT_WRITE.format(absent))
            case = self.case()
            theirs = str(case.home / ".local" / "bin")
            for env in ({}, {"HOME": "/", "PITBOARD_INSTALL_DIR": theirs}):
                with self.subTest(env=env):
                    result = case.run(GOOD.script, shell, uid="0", env=env)
                    self.assertRefused(result, LINUX_ROOT_REFUSAL)

    def test_an_argument_is_refused(self):
        for shell in self.each_shell():
            for args in (["--help"], ["-y"], [""]):
                with self.subTest(args=args):
                    result = self.case().run(GOOD.script, shell, args=args)
                    self.assertRefused(result, NO_ARGUMENTS)

    def test_the_install_dir_must_be_a_full_path(self):
        refusals = {
            "": "PITBOARD_INSTALL_DIR is empty. Set it to a full path, or unset it.",
            **{
                value: f"PITBOARD_INSTALL_DIR is `{value}`, which is not a full path. "
                "Set it to a full path, or unset it."
                for value in ("bin", "./bin", "~/bin", "bin/")
            },
        }
        for shell in self.each_shell():
            for value, message in refusals.items():
                with self.subTest(value=value):
                    result = self.case().run(
                        GOOD.script, shell, env={"PITBOARD_INSTALL_DIR": value}
                    )
                    self.assertRefused(result, message)

    def test_root_as_the_install_dir_is_refused_for_what_it_is(self):
        if os.geteuid() == 0:
            self.skipTest("root writes anywhere")
        for shell in self.each_shell():
            for value in ("/", "//"):
                with self.subTest(value=value):
                    result = self.case().run(
                        GOOD.script, shell, env={"PITBOARD_INSTALL_DIR": value}
                    )
                    self.assertNothingInstalled(result, CANNOT_WRITE.format("/"))

    def test_the_install_dir_is_honoured_with_share_beside_it(self):
        for shell in self.each_shell():
            for slash in ("", "/", "//"):
                with self.subTest(slash=slash):
                    case = self.case()
                    bindir = case.home / "opt" / "pb" / "bin"
                    result = case.run(
                        GOOD.script, shell, env={"PITBOARD_INSTALL_DIR": f"{bindir}{slash}"}
                    )
                    self.assertInstalled(case, result, prefix="opt/pb")
                    self.assertIn('export PATH="$HOME/opt/pb/bin:$PATH"', result.out)

    def test_an_install_dir_outside_home_is_named_as_it_is(self):
        for shell in self.each_shell():
            case = self.case()
            elsewhere = case.dir / "elsewhere"
            elsewhere.mkdir()
            result = case.run(
                GOOD.script,
                shell,
                env={"PITBOARD_INSTALL_DIR": str(elsewhere / "bin")},
                allowed=(elsewhere,),
            )
            self.assertEqual(result.rc, 0, result.err)
            self.assertEqual(result.entries, [])
            self.assertEqual(
                entries(elsewhere),
                sorted(
                    {"bin", "share", "share/man", "share/man/man1", "share/bash-completion",
                     "share/bash-completion/completions", "share/zsh", "share/zsh/site-functions",
                     "share/fish", "share/fish/vendor_completions.d", *INSTALLED}
                ),
            )
            self.assertIn(f"echo 'export PATH=\"{elsewhere}/bin:$PATH\"' >> ~/.profile", result.out)

    def test_a_directory_it_cannot_write_installs_nothing(self):
        if os.geteuid() == 0:
            self.skipTest("root writes anywhere")
        for shell in self.each_shell():
            case = self.case()
            locked = case.home / "locked"
            locked.mkdir()
            locked.chmod(0o555)
            self.addCleanup(locked.chmod, 0o755)
            result = case.run(
                GOOD.script, shell, env={"PITBOARD_INSTALL_DIR": str(locked / "bin")}
            )
            message = CANNOT_WRITE.format(f"{locked}/bin")
            self.assertEqual(result.rc, 1)
            self.assertTrue(result.err.endswith(f"error: {message}\n"), result.err)
            self.assertEqual(result.entries, ["locked"])

    def assertOldCopyKept(self, case, result, before):
        bindir = case.home / ".local" / "bin"
        self.assertEqual(result.rc, 1, result.err)
        self.assertNotIn("Installed", result.out)
        self.assertEqual(os.listdir(bindir), ["pitboard"])
        now = (bindir / "pitboard").stat()
        self.assertEqual((now.st_ino, now.st_mtime_ns), (before.st_ino, before.st_mtime_ns))
        self.assertEqual((bindir / "pitboard").read_text(), "#!/bin/sh\necho pitboard 0.0.1\n")

    def old_copy(self, case):
        bindir = case.home / ".local" / "bin"
        bindir.mkdir(parents=True)
        (bindir / "pitboard").write_text("#!/bin/sh\necho pitboard 0.0.1\n")
        (bindir / "pitboard").chmod(0o755)
        return (bindir / "pitboard").stat()

    def test_a_share_it_cannot_write_leaves_the_old_copy(self):
        for shell in self.each_shell():
            for kind in ("file", "locked"):
                with self.subTest(share=kind):
                    if kind == "locked" and os.geteuid() == 0:
                        self.skipTest("root writes anywhere")
                    case = self.case()
                    before = self.old_copy(case)
                    share = case.home / ".local" / "share"
                    if kind == "file":
                        share.write_text("a file\n")
                    else:
                        share.mkdir()
                        share.chmod(0o555)
                        self.addCleanup(share.chmod, 0o755)
                    result = case.run(GOOD.script, shell)
                    self.assertOldCopyKept(case, result, before)
                    message = CANNOT_WRITE.format(f"{share}/man/man1")
                    self.assertTrue(result.err.endswith(f"error: {message}\n"), result.err)

    def test_a_staged_copy_it_cannot_write_leaves_the_old_copy(self):
        for shell in self.each_shell():
            case = self.case()
            before = self.old_copy(case)
            result = case.run(GOOD.script, shell, before=(full_cp(),))
            self.assertOldCopyKept(case, result, before)
            self.assertIn("No space left on device", result.err)

    def test_a_man_page_it_cannot_write_after_the_command_line_says_so(self):
        for shell in self.each_shell():
            case = self.case()
            share = case.home / ".local" / "share"
            (share / "man" / "man1" / "pitboard.1").mkdir(parents=True)
            result = case.run(GOOD.script, shell)
            self.assertEqual(result.rc, 1, result.err)
            bindir = case.home / ".local" / "bin"
            message = (
                f"Installed Pitboard {VERSION} at {bindir}/pitboard, but its man page and "
                f"completions could not be written to {share}."
            )
            self.assertTrue(result.err.endswith(f"error: {message}\n"), result.err)
            self.assertNotIn("Installed", result.out)
            self.assertEqual(os.listdir(bindir), ["pitboard"])
            self.assertEqual(
                (bindir / "pitboard").read_bytes(), GOOD.members[TARGETS[3]]["pitboard"]
            )

    def test_home_must_be_absolute(self):
        for shell in self.each_shell():
            for value in (None, "", "home"):
                with self.subTest(home=value):
                    # zsh fills an unset HOME from the passwd entry, which is the real home.
                    if seen_by(shell, "HOME", value) != value:
                        self.skipTest(f"{shell[0]} changes HOME={value!r} before the script runs")
                    if value is None:
                        result = self.case().run(GOOD.script, shell, unset=["HOME"])
                    else:
                        result = self.case().run(GOOD.script, shell, env={"HOME": value})
                    self.assertRefused(result, NO_HOME[value])

    def test_a_home_with_a_trailing_slash_is_read_without_it(self):
        for shell in self.each_shell():
            case = self.case()
            bindir = case.home / ".local" / "bin"
            result = case.run(
                GOOD.script, shell, env={"HOME": f"{case.home}/"}, before=(bindir,)
            )
            self.assertInstalled(case, result)
            self.assertNotIn("PATH", result.out)
            self.assertNotIn("first", result.out)

    def test_systems_with_no_build_are_refused(self):
        refusals = {
            ("MINGW64_NT-10.0-26100", "x86_64"): WINDOWS,
            ("MSYS_NT-10.0-26100", "x86_64"): WINDOWS,
            ("CYGWIN_NT-10.0-26100", "x86_64"): WINDOWS,
            ("Linux", "armv7l"): "Pitboard has no build for Linux on armv7l.",
            ("Linux", "i686"): "Pitboard has no build for Linux on i686.",
            ("Darwin", "i386"): "Pitboard has no build for Darwin on i386.",
            ("FreeBSD", "amd64"): "Pitboard has no build for FreeBSD.",
        }
        for shell in self.each_shell():
            for uname, message in refusals.items():
                with self.subTest(uname=uname):
                    self.assertRefused(self.case().run(GOOD.script, shell, uname=uname), message)

    def test_rosetta_picks_the_arm_build(self):
        for shell in self.each_shell():
            for translated, target in (
                ("1", "aarch64-apple-darwin"),
                ("0", "x86_64-apple-darwin"),
                (None, "x86_64-apple-darwin"),
            ):
                with self.subTest(translated=translated):
                    case = self.case()
                    result = case.run(
                        GOOD.script, shell, uname=("Darwin", "x86_64"), translated=translated
                    )
                    self.assertInstalled(case, result, target)

    def test_amd64_is_x86_64(self):
        for shell in self.each_shell():
            case = self.case()
            result = case.run(GOOD.script, shell, uname=("Linux", "amd64"))
            self.assertInstalled(case, result, "x86_64-unknown-linux-musl")

    def test_wget_alone_downloads(self):
        for shell in self.each_shell():
            case = self.case()
            result = case.run(GOOD.script, shell, downloaders=("wget",))
            self.assertEqual(result.requests[0]["tool"], "wget")
            self.assertInstalled(case, result)

    def test_curl_is_used_before_wget(self):
        for shell in self.each_shell():
            case = self.case()
            result = case.run(GOOD.script, shell)
            self.assertEqual([r["tool"] for r in result.requests], ["curl"])
            self.assertInstalled(case, result)

    def test_no_downloader_is_refused(self):
        for shell in self.each_shell():
            self.assertRefused(self.case().run(GOOD.script, shell, downloaders=()), NO_DOWNLOADER)

    def test_no_checksum_tool_is_refused(self):
        for shell in self.each_shell():
            self.assertRefused(self.case().run(GOOD.script, shell, hashers=()), NO_HASHER)

    def test_shasum_alone_checks_the_download(self):
        if shutil.which("shasum") is None:
            self.skipTest("no shasum here")
        for shell in self.each_shell():
            case = self.case()
            self.assertInstalled(case, case.run(GOOD.script, shell, hashers=("shasum",)))
            result = self.case().run(GOOD.script, shell, hashers=("shasum",), serve=TAMPERED)
            self.assertNothingInstalled(result, MISMATCH)

    def test_a_command_line_that_does_not_run_leaves_the_old_copy(self):
        for shell in self.each_shell():
            case = self.case()
            self.assertInstalled(case, case.run(GOOD.script, shell))
            local = case.home / ".local"
            before = {rel: ((local / rel).read_bytes(), (local / rel).stat()) for rel in INSTALLED}
            result = case.run(BROKEN.script, shell, serve=BROKEN.serve)
            self.assertEqual(result.rc, 1)
            message = DOES_NOT_RUN.format(local / "bin")
            self.assertTrue(result.err.endswith(f"error: {message}\n"), result.err)
            self.assertEqual(sorted(result.files), sorted(f".local/{rel}" for rel in INSTALLED))
            for rel, (data, st) in before.items():
                now = (local / rel).stat()
                self.assertEqual((local / rel).read_bytes(), data, rel)
                self.assertEqual((now.st_ino, now.st_mtime_ns), (st.st_ino, st.st_mtime_ns), rel)

    def test_a_path_hint_for_each_shell(self):
        export = "echo 'export PATH=\"$HOME/.local/bin:$PATH\"' >> "
        terminal = "open another terminal"
        login_again = "log in again"
        hints = (
            ("/bin/zsh", LINUX, export + "~/.zshrc", terminal),
            ("/bin/zsh", MAC, export + "~/.zshrc", terminal),
            ("/bin/bash", MAC, export + "~/.bash_profile", terminal),
            ("/usr/bin/bash", LINUX, export + "~/.bashrc", terminal),
            ("/opt/homebrew/bin/fish", MAC, 'fish_add_path "$HOME/.local/bin"', terminal),
            ("/usr/bin/fish", LINUX, 'fish_add_path "$HOME/.local/bin"', terminal),
            ("/bin/dash", LINUX, export + "~/.profile", login_again),
            ("/bin/ksh", MAC, export + "~/.profile", terminal),
            ("", LINUX, export + "~/.profile", login_again),
            (None, LINUX, export + "~/.profile", login_again),
        )
        for shell in self.each_shell():
            for login, uname, line, after in hints:
                with self.subTest(login=login, uname=uname):
                    # bash fills an unset SHELL from the passwd entry.
                    if seen_by(shell, "SHELL", login) != login:
                        self.skipTest(f"{shell[0]} changes SHELL={login!r} before the script runs")
                    case = self.case()
                    if login is None:
                        result = case.run(GOOD.script, shell, uname=uname, unset=["SHELL"])
                    else:
                        result = case.run(GOOD.script, shell, uname=uname, env={"SHELL": login})
                    self.assertEqual(result.rc, 0, result.err)
                    self.assertIn(
                        f"{case.home}/.local/bin is not on your PATH. To put it there, run this, "
                        f"then {after}:\n  {line}\n",
                        result.out,
                    )

    def test_a_zsh_login_hears_the_lines_that_load_the_completions(self):
        logins = (("/bin/zsh", True), ("/usr/bin/bash", False), ("/bin/sh", False))
        for shell in self.each_shell():
            for login, told in logins:
                for prefix in (".local", "opt/pb"):
                    with self.subTest(login=login, prefix=prefix):
                        if seen_by(shell, "SHELL", login) != login:
                            self.skipTest(
                                f"{shell[0]} changes SHELL={login!r} before the script runs"
                            )
                        case = self.case()
                        env = {"SHELL": login}
                        if prefix != ".local":
                            env["PITBOARD_INSTALL_DIR"] = str(case.home / prefix / "bin")
                        result = case.run(GOOD.script, shell, env=env)
                        self.assertInstalled(case, result, prefix=prefix)
                        hint = ZSH_HINT.format(f"$HOME/{prefix}/share")
                        self.assertEqual(hint in result.out, told, result.out)
                        self.assertEqual("fpath" in result.out, told, result.out)
                # zsh's own fpath holds a prefix's site-functions outside the home, such as
                # /usr/local's.
                with self.subTest(login=login, prefix="outside the home"):
                    if seen_by(shell, "SHELL", login) != login:
                        self.skipTest(f"{shell[0]} changes SHELL={login!r} before the script runs")
                    case = self.case()
                    usr = case.dir / "usr"
                    usr.mkdir()
                    result = case.run(
                        GOOD.script,
                        shell,
                        env={"SHELL": login, "PITBOARD_INSTALL_DIR": str(usr / "local" / "bin")},
                        allowed=(usr,),
                    )
                    self.assertEqual(result.rc, 0, result.err)
                    self.assertTrue((usr / "local" / "share" / "zsh" / "site-functions").is_dir())
                    self.assertNotIn("fpath", result.out)

    def test_it_ends_with_the_command_to_run_next(self):
        for shell in self.each_shell():
            for target in ("x86_64-unknown-linux-musl", "aarch64-apple-darwin"):
                with self.subTest(target=target):
                    case = self.case()
                    result = case.run(GOOD.script, shell, uname=UNAME[target])
                    self.assertInstalled(case, result, target)
                    self.assertTrue(result.out.endswith(NEXT), result.out)
                    self.assertEqual(result.out.count(NEXT), 1)

    def test_no_path_hint_when_the_directory_is_on_path(self):
        for shell in self.each_shell():
            for given, entry in (("", ""), ("", "/"), ("", "//"), ("//", "")):
                with self.subTest(given=given, entry=entry):
                    case = self.case()
                    bindir = case.home / ".local" / "bin"
                    env = {"PITBOARD_INSTALL_DIR": f"{bindir}{given}"} if given else None
                    result = case.run(GOOD.script, shell, env=env, before=(f"{bindir}{entry}",))
                    self.assertInstalled(case, result)
                    self.assertNotIn("PATH", result.out)
                    self.assertNotIn("first", result.out)

    def test_no_path_hint_when_a_link_to_the_directory_is_on_path(self):
        for shell in self.each_shell():
            for slash in ("", "//"):
                with self.subTest(slash=slash):
                    case = self.case()
                    target = case.home / "dotfiles" / "bin"
                    target.mkdir(parents=True)
                    (case.home / ".local").mkdir()
                    (case.home / ".local" / "bin").symlink_to(target)
                    result = case.run(GOOD.script, shell, before=(f"{target}{slash}",))
                    self.assertEqual(result.rc, 0, result.err)
                    self.assertEqual(
                        (target / "pitboard").read_bytes(), GOOD.members[TARGETS[3]]["pitboard"]
                    )
                    self.assertNotIn("PATH", result.out)
                    self.assertNotIn("first", result.out)

    def test_another_pitboard_earlier_on_path_is_named(self):
        for shell in self.each_shell():
            case = self.case()
            other = case.dir / "other"
            other.mkdir()
            (other / "pitboard").write_text("#!/bin/sh\necho pitboard 0.1.0\n")
            (other / "pitboard").chmod(0o755)
            bindir = case.home / ".local" / "bin"
            result = case.run(GOOD.script, shell, before=(other, bindir))
            self.assertInstalled(case, result)
            self.assertIn(
                f"Your shell runs {other}/pitboard first. Remove that copy, or put {bindir} before "
                "it on PATH.\n",
                result.out,
            )
            self.assertNotIn("is not on your PATH", result.out)
            result = case.run(GOOD.script, shell, before=(bindir, other))
            self.assertInstalled(case, result)
            self.assertNotIn("first", result.out)

    # /Applications is this machine's own, so with the app there the case with none expects it.
    def test_the_app_note_on_a_mac_with_the_app(self):
        real_app = os.path.isdir("/Applications/Pitboard.app")
        for shell in self.each_shell():
            case = self.case()
            (case.home / "Applications" / "Pitboard.app").mkdir(parents=True)
            result = case.run(GOOD.script, shell, uname=MAC)
            self.assertEqual(result.rc, 0, result.err)
            self.assertEqual(result.out.count(APP_NOTE), 1)
            result = case.run(GOOD.script, shell, uname=LINUX)
            self.assertNotIn(APP_NOTE, result.out)
            result = self.case().run(GOOD.script, shell, uname=MAC)
            self.assertEqual(APP_NOTE in result.out, real_app)

    def test_a_signal_leaves_no_staged_copy_or_temp_dir(self):
        signals = (
            (signal.SIGINT, os.killpg),
            (signal.SIGTERM, os.killpg),
            (signal.SIGHUP, os.killpg),
            (signal.SIGTERM, os.kill),
        )
        for shell in self.each_shell():
            # Held in the download, then with the staged copy in place.
            for hold in ("SHIM_HOLD", "STUB_HOLD"):
                for number, send in signals:
                    with self.subTest(hold=hold, signal=number.name, to=send.__name__):
                        case = self.case()
                        held = case.log / "held"
                        proc = subprocess.Popen(
                            [*shell, "-s"],
                            stdin=subprocess.PIPE,
                            stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL,
                            env=case.environ(env={hold: str(held)}),
                            cwd=str(case.dir),
                            start_new_session=True,
                            preexec_fn=default_signals,
                        )
                        proc.stdin.write(GOOD.script.read_bytes())
                        proc.stdin.close()
                        deadline = time.monotonic() + 30
                        while not held.exists():
                            if time.monotonic() > deadline:
                                proc.kill()
                                self.fail("the run never reached the hold")
                            time.sleep(0.01)
                        send(proc.pid, number)
                        (case.log / "held.go").touch()
                        self.assertNotEqual(proc.wait(timeout=30), 0)
                        self.assertEqual(
                            [e for e in entries(case.home) if not (case.home / e).is_dir()], []
                        )
                        self.assertEqual(os.listdir(case.tmp), [])


class FillTemplate(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.dir = WORK / "fill"
        cls.dir.mkdir()
        cls.names = (
            [f"pitboard-v{VERSION}-{t}.tar.gz" for t in TARGETS]
            + [f"pitboard-v{VERSION}-{t}.zip" for t in WINDOWS_TARGETS.values()]
            + [f"Pitboard-v{VERSION}-macos.zip"]
        )
        cls.hashes = {name: hashlib.sha256(name.encode()).hexdigest() for name in cls.names}
        cls.sums = cls.write_sums("SHA256SUMS", [f"{h}  {n}" for n, h in cls.hashes.items()])

    @classmethod
    def write_sums(cls, name, lines):
        path = cls.dir / name
        path.write_text("\n".join(lines) + "\n")
        return path

    def template(self, name, text):
        path = self.dir / name
        path.write_text(text)
        return path

    def assertFails(self, template, message, sums=None):
        out = self.dir / "out"
        done = fill(template, out, sums or self.sums)
        self.assertNotEqual(done.returncode, 0)
        self.assertIn(message, done.stderr)
        self.assertFalse(out.exists(), "a failed fill left its output")
        self.assertEqual([p.name for p in self.dir.glob("out*")], [])

    def test_both_casks_fill_with_nothing_left(self):
        for cask in CASKS:
            with self.subTest(cask=cask.name):
                out = self.dir / cask.name
                done = fill(cask, out, self.sums)
                self.assertEqual(done.returncode, 0, done.stderr)
                text = out.read_text()
                self.assertIsNone(re.search(r"@[A-Z0-9_]+@", text))
                self.assertIn(f'version "{VERSION}"', text)
        arm = (self.dir / "pitboard.rb").read_text()
        for target in TARGETS:
            self.assertIn(f'"{self.hashes[f"pitboard-v{VERSION}-{target}.tar.gz"]}"', arm)
        app = (self.dir / "pitboard-app.rb").read_text()
        self.assertIn(f'sha256 "{self.hashes[f"Pitboard-v{VERSION}-macos.zip"]}"', app)

    def test_the_command_line_cask_sends_an_old_install_to_a_docs_page_that_exists(self):
        caveats = CASKS[0].read_text().split("caveats <<~EOS\n", 1)[1].split("  EOS\n", 1)[0]
        self.assertNotIn("--formula", caveats)
        pages = re.findall(r"https://docs\.usepitboard\.com/(\S+)", caveats)
        self.assertEqual(pages, ["install/upgrade-from-0-3-0"])
        self.assertTrue((REPO / "docs" / f"{pages[0]}.mdx").is_file())

    # A list of tools goes stale as Pitboard switches more of them.
    def test_neither_cask_names_a_tool_in_its_description(self):
        for cask in CASKS:
            desc = re.search(r'^  desc "([^"]*)"$', cask.read_text(), re.M)[1]
            for tool in ("Claude", "Codex", "ChatGPT"):
                with self.subTest(cask=cask.name, tool=tool):
                    self.assertNotIn(tool, desc)

    def test_the_install_script_fills_and_parses(self):
        out = self.dir / "install.sh"
        sums = self.write_sums(
            "no-app", [f"{h}  {n}" for n, h in self.hashes.items() if n.endswith(".tar.gz")]
        )
        done = fill(TEMPLATE, out, sums)
        self.assertEqual(done.returncode, 0, done.stderr)
        text = out.read_text()
        self.assertIsNone(re.search(r"@[A-Z0-9_]+@", text))
        self.assertIn(f'\nVERSION="{VERSION}"\n', text)
        for target in TARGETS:
            name = "SHA256_" + target.upper().replace("-", "_")
            digest = self.hashes[f"pitboard-v{VERSION}-{target}.tar.gz"]
            self.assertIn(f'\n{name}="{digest}"\n', text)
        self.assertEqual(subprocess.run(["sh", "-n", str(out)]).returncode, 0)

    def test_the_scoop_manifest_fills_and_parses(self):
        out = self.dir / "pitboard.json"
        done = fill(SCOOP, out, self.sums)
        self.assertEqual(done.returncode, 0, done.stderr)
        manifest = json.loads(out.read_text())
        self.assertEqual(manifest["version"], VERSION)
        self.assertEqual(manifest["license"], "Apache-2.0")
        self.assertEqual(manifest["bin"], "pitboard.exe")
        # The release is the bucket's only writer, as it is the tap's.
        self.assertNotIn("checkver", manifest)
        self.assertNotIn("autoupdate", manifest)
        self.assertEqual(set(manifest["architecture"]), set(WINDOWS_TARGETS))
        for arch, target in WINDOWS_TARGETS.items():
            with self.subTest(arch=arch):
                name = f"pitboard-v{VERSION}-{target}"
                self.assertEqual(
                    manifest["architecture"][arch],
                    {
                        "url": f"{DOWNLOADS}v{VERSION}/{name}.zip",
                        "hash": self.hashes[f"{name}.zip"],
                        "extract_dir": name,
                    },
                )

    # Scoop 0.6.0 prints notes after the install with $dir as the app's current folder
    # (lib/install.ps1 show_notes and link_current), which an update keeps.
    def test_the_scoop_manifest_says_how_to_load_the_powershell_completions(self):
        out = self.dir / "pitboard.json"
        done = fill(SCOOP, out, self.sums)
        self.assertEqual(done.returncode, 0, done.stderr)
        notes = json.loads(out.read_text())["notes"]
        self.assertIn('. "$dir\\completions\\pitboard.ps1"', notes)
        self.assertIn("$PROFILE", notes[0])

    def test_the_scoop_manifest_needs_both_zips(self):
        for target in WINDOWS_TARGETS.values():
            missing = f"pitboard-v{VERSION}-{target}.zip"
            with self.subTest(missing=missing):
                sums = self.write_sums(
                    "no-zip", [f"{h}  {n}" for n, h in self.hashes.items() if n != missing]
                )
                message = f"SHA256SUMS does not have exactly one line for {missing}:"
                self.assertFails(SCOOP, message, sums)

    def test_a_missing_line_fails(self):
        missing = f"pitboard-v{VERSION}-x86_64-apple-darwin.tar.gz"
        sums = self.write_sums(
            "missing", [f"{h}  {n}" for n, h in self.hashes.items() if n != missing]
        )
        message = f"SHA256SUMS does not have exactly one line for {missing}:"
        self.assertFails(CASKS[0], message, sums)

    def test_a_duplicate_line_fails(self):
        twice = f"Pitboard-v{VERSION}-macos.zip"
        lines = [f"{h}  {n}" for n, h in self.hashes.items()]
        sums = self.write_sums("twice", lines + [f"{self.hashes[twice]}  {twice}"])
        self.assertFails(CASKS[1], f"SHA256SUMS does not have exactly one line for {twice}:", sums)

    def test_a_line_that_is_not_a_sha256_fails(self):
        short = f"pitboard-v{VERSION}-aarch64-apple-darwin.tar.gz"
        lines = [f"{h}  {n}" for n, h in self.hashes.items() if n != short]
        sums = self.write_sums("short", lines + [f"{self.hashes[short][:63]}  {short}"])
        self.assertFails(CASKS[0], f"SHA256SUMS does not have exactly one line for {short}:", sums)

    def test_an_unknown_placeholder_fails(self):
        template = self.template(
            "unknown.rb", 'version "@VERSION@"\nsha256 "@SHA256_RISCV64_UNKNOWN_LINUX_GNU@"\n'
        )
        self.assertFails(template, f"{template} has a placeholder this job does not fill")

    def test_a_value_out_of_quotes_fails(self):
        template = self.template("bare.rb", "version @VERSION@\n")
        self.assertFails(template, f"@VERSION@ was not replaced in {template}")

    def test_a_template_with_no_version_fails(self):
        template = self.template("noversion.rb", 'sha256 "@SHA256_APP@"\n')
        self.assertFails(template, f"{template} has no @VERSION@")

    def test_a_version_is_only_what_a_version_holds(self):
        for version, fills in (
            (f"{VERSION}-rc1", True),
            (f"{VERSION}-rc.1+b5", True),
            (f"{VERSION}-rc$(touch${{IFS}}x)", False),
            (f"{VERSION}-rc`id`", False),
            (f"{VERSION}-rc;id", False),
            ("", False),
        ):
            with self.subTest(version=version):
                lines = [f"{h}  {n.replace(VERSION, version)}" for n, h in self.hashes.items()]
                sums = self.write_sums("odd", lines)
                out = self.dir / "odd.sh"
                done = fill(TEMPLATE, out, sums, version=version)
                if fills:
                    self.assertEqual(done.returncode, 0, done.stderr)
                    self.assertIn(f'\nVERSION="{version}"\n', out.read_text())
                    out.unlink()
                else:
                    self.assertNotEqual(done.returncode, 0)
                    self.assertIn(f'"{version}" is not a version', done.stderr)
                    self.assertFalse(out.exists(), "a refused version left its output")

    def test_it_takes_four_arguments(self):
        done = subprocess.run(["sh", str(FILL), str(CASKS[0])], capture_output=True, text=True)
        self.assertEqual(done.returncode, 2)
        self.assertIn("usage:", done.stderr)


HOST = REPO / "crates" / "pitboard-core" / "src" / "host"
UNITS = "~/.config/systemd/user"


def constants(path, pattern):
    return re.findall(rf'^const \w+: &str = "({pattern})";$', path.read_text(), re.M)


def directive(name, stanza):
    found = re.search(rf"\b{name}:\s*(\[[^\]]*\]|\"[^\"]*\")", stanza)
    return re.findall(r'"([^"]*)"', found.group(1)) if found else []


class CaskZap(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        launchd = HOST / "macos" / "launchd.rs"
        cls.labels = constants(launchd, r"com\.datlechin\.pitboard\.[\w.]+")
        logs = re.search(r'home\.join\("(Library/Logs/[^"]+)"\)', launchd.read_text()).group(1)
        cls.logs = f"~/{logs}"
        log = constants(launchd, r"[\w.]+\.log")[0]
        cls.log = f"{cls.logs}/{log}"
        cls.units = constants(HOST / "linux" / "systemd.rs", r"pitboard-[\w-]+\.(?:service|timer)")
        cli = CASKS[0].read_text()
        cls.zaps = {
            "pitboard on macOS": re.search(r"^  on_macos do\n(.*?)^  end$", cli, re.M | re.S)[1],
            "pitboard-app": re.search(r"^  zap (.*?)^end$", CASKS[1].read_text(), re.M | re.S)[1],
        }
        cls.linux = re.search(r"^  on_linux do\n(.*?)^  end$", cli, re.M | re.S)[1]

    def test_the_core_names_the_jobs_this_reads(self):
        self.assertEqual(
            sorted(self.labels), ["com.datlechin.pitboard.renew", "com.datlechin.pitboard.watch"]
        )
        self.assertEqual(self.log, "~/Library/Logs/Pitboard/watch.log")
        self.assertEqual(
            sorted(self.units),
            ["pitboard-renew.service", "pitboard-renew.timer", "pitboard-watch.service"],
        )

    def test_each_macos_zap_takes_away_every_launchd_job_and_the_log(self):
        for cask, zap in self.zaps.items():
            with self.subTest(cask=cask):
                self.assertEqual(sorted(directive("launchctl", zap)), sorted(self.labels))
                self.assertIn(self.log, directive("trash", zap))
                self.assertEqual(directive("rmdir", zap), [self.logs])

    # Uninstall Pitboard… in the app trashes the same folders of its own, for its own bundle id.
    def test_the_app_trashes_what_the_app_casks_zap_does_of_its_library(self):
        removal = REPO / "apps" / "macos" / "Sources" / "PitboardApp" / "System" / "AppRemoval.swift"
        listed = re.search(
            r"func leftInLibrary\(by domain: String\) -> \[String\] \{\s*\[(.*?)\]",
            removal.read_text(),
            re.S,
        )[1]
        folders = re.findall(r'"([^"]*)"', listed.replace("\\(domain)", "com.usepitboard.Pitboard"))
        # The app removes its Application Support folder and defaults itself, and the core the log.
        elsewhere = {
            "~/Library/Application Support/com.usepitboard.Pitboard",
            "~/Library/Preferences/com.usepitboard.Pitboard.plist",
            self.log,
        }
        # macOS makes the extensions' folders and the App Group's: the core deletes the glance.
        made = {
            "~/Library/Application Scripts/com.usepitboard.Pitboard.share",
            "~/Library/Application Scripts/com.usepitboard.Pitboard.widgets",
            "~/Library/Containers/com.usepitboard.Pitboard.share",
            "~/Library/Containers/com.usepitboard.Pitboard.widgets",
            "~/Library/Group Containers/D7HJ5TFYCU.com.usepitboard.Pitboard",
        }
        trash = set(directive("trash", self.zaps["pitboard-app"]))
        self.assertLessEqual(elsewhere | made, trash)
        self.assertEqual(trash - elsewhere - made, {f"~/Library/{folder}" for folder in folders})

    def test_the_linux_zap_trashes_every_unit_and_its_link(self):
        trash = directive("trash", self.linux)
        for unit in self.units:
            self.assertIn(f"{UNITS}/{unit}", trash)
        self.assertIn(f"{UNITS}/timers.target.wants/pitboard-renew.timer", trash)
        self.assertIn(f"{UNITS}/default.target.wants/pitboard-watch.service", trash)

    # A stand-in answers for systemctl, so no test reaches this machine's own.
    def test_the_linux_zap_stops_each_unit_systemd_starts_before_its_files_go(self):
        script = re.search(r"\bargs:\s*\[(.*?)\]", self.linux, re.S)[1]
        parts = re.findall(r'"((?:[^"\\]|\\.)*)"', script)
        self.assertEqual(parts[0], "-c")
        script = "".join(parts[1:])
        work = Path(tempfile.mkdtemp(dir=WORK))
        bin_dir = work / "bin"
        bin_dir.mkdir()
        asked = work / "asked"
        (bin_dir / "systemctl").write_text(f'#!/bin/sh\necho "$*" >> "{asked}"\n')
        (bin_dir / "systemctl").chmod(0o755)
        for installed, expected in [
            ([], []),
            (
                self.units,
                ["--user disable --now pitboard-renew.timer", "--user disable --now pitboard-watch.service"],
            ),
        ]:
            with self.subTest(installed=installed):
                home = Path(tempfile.mkdtemp(dir=work))
                folder = home / ".config" / "systemd" / "user"
                folder.mkdir(parents=True)
                for unit in installed:
                    (folder / unit).write_text("")
                asked.unlink(missing_ok=True)
                done = subprocess.run(
                    ["/bin/sh", "-c", script],
                    env={"HOME": str(home), "PATH": f"{bin_dir}:/usr/bin:/bin"},
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(done.returncode, 0, done.stderr)
                said = asked.read_text().splitlines() if asked.exists() else []
                self.assertEqual(sorted(said), expected)


if __name__ == "__main__":
    unittest.main()
