# windows-pending.py, run against planted sources: each case is the files planted and the
# problems the script must report, as "path:line" and a piece of the message.
import importlib.util
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "windows-pending.py"
SPEC = importlib.util.spec_from_file_location("windows_pending", SCRIPT)
pending = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pending)

ACCEPTED = {
    "Claude Desktop is macOS only": None,
    "a keychain's ceiling is what `security` reads from stdin": None,
    "`SUDO_UID` is Unix's sign of sudo": None,
    "runs as a standard user only": None,
    "a gap being closed": 1,
}
UNSAID = {("crates/a/src/lib.rs", "waits")}
LIB = "crates/a/src/lib.rs"
TESTS = "crates/a/tests/t.rs"
CS = "apps/windows/Pitboard.Windows.Tests/T.cs"
NO_REASON = "compiled out on Windows, says no reason"


def ignored(attribute, name="a"):
    return f"#[test]\n{attribute}\nfn {name}() {{}}\n"


CASES = [
    (
        "an accepted ignore reason",
        {LIB: ignored('#[cfg_attr(windows, ignore = "Claude Desktop is macOS only: no link")]')},
        [],
    ),
    (
        "an ignore reason not accepted",
        {LIB: ignored('#[cfg_attr(windows, ignore = "flaky on Windows")]')},
        [(f"{LIB}:2", '"flaky on Windows", which windows-pending.py does not accept')],
    ),
    (
        "a W reason in Rust",
        {LIB: ignored('#[cfg_attr(\n    windows,\n    ignore = "W23a: Credential Manager"\n)]')},
        [(f"{LIB}:2", '"W23a: Credential Manager"')],
    ),
    (
        "a reason over two lines",
        {
            LIB: ignored(
                "#[cfg_attr(\n    windows,\n"
                "    ignore = \"a keychain's ceiling is what `security` reads \\\n"
                "              from stdin; Credential Manager's is 256 pieces\"\n)]"
            )
        },
        [],
    ),
    (
        "an ignore that holds off Windows only",
        {LIB: ignored('#[cfg_attr(target_os = "linux", ignore = "anything")]')},
        [],
    ),
    (
        "an ignore off macOS, so on Windows too",
        {LIB: ignored('#[cfg_attr(not(target_os = "macos"), ignore = "anything")]')},
        [(f"{LIB}:2", '"anything"')],
    ),
    (
        "an ignore a feature decides",
        {LIB: ignored('#[cfg_attr(all(windows, feature = "x"), ignore = "anything")]')},
        [(f"{LIB}:2", '"anything"')],
    ),
    (
        "an ignore with no reason, and one everywhere",
        {LIB: ignored("#[ignore]") + "\n" + ignored('#[ignore = "slow"]', "b")},
        [(f"{LIB}:2", "gives no reason"), (f"{LIB}:6", '"slow"')],
    ),
    (
        "a bare cfg(unix) test",
        {LIB: ignored("#[cfg(unix)]")},
        [(f"{LIB}:2", f"the test a, {NO_REASON}")],
    ),
    (
        "a bare cfg(target_os = macos) test",
        {LIB: '#[cfg(target_os = "macos")]\n#[test]\nfn a() {}\n'},
        [(f"{LIB}:1", f"the test a, {NO_REASON}")],
    ),
    (
        "a cfg(not(windows)) test with an accepted reason",
        {LIB: ignored("// windows: `SUDO_UID` is Unix's sign of sudo\n#[cfg(not(windows))]")},
        [],
    ),
    (
        "tests that run on Windows, or where a feature decides",
        {
            LIB: ignored("#[cfg(windows)]")
            + ignored("#[cfg(any(unix, windows))]", "b")
            + ignored('#[cfg(feature = "x")]', "c")
        },
        [],
    ),
    (
        "a test module compiled out",
        {LIB: "#[cfg(all(test, unix))]\nmod tests {\n    #[test]\n    fn a() {}\n}\n"},
        [(f"{LIB}:1", f"the test module tests, {NO_REASON}")],
    ),
    (
        "a module of platform code with its own tests",
        {LIB: '#[cfg(target_os = "macos")]\nmod keychain;\n'},
        [],
    ),
    (
        "a cfg on a statement in a test",
        {LIB: "#[test]\nfn a() {\n    #[cfg(unix)]\n    let x = 1;\n}\n"},
        [],
    ),
    (
        "Desktop early returns with no reason",
        {
            TESTS: "#[test]\nfn a() {\n    if !os::claude_desktop_runs_here() {\n"
            "        return;\n    }\n}\n\n"
            "#[test]\nfn b() {\n    if !ProviderId::Desktop.supported_on(OS) {\n"
            "        return;\n    }\n}\n"
        },
        [
            (f"{TESTS}:3", "the early return of a says no reason"),
            (f"{TESTS}:10", "the early return of b says no reason"),
        ],
    ),
    (
        "a Desktop early return with its reason",
        {
            TESTS: "#[test]\nfn a() {\n    // windows: Claude Desktop is macOS only\n"
            "    if !os::claude_desktop_runs_here() {\n        return;\n    }\n}\n"
        },
        [],
    ),
    (
        "an early return outside a test",
        {
            LIB: "fn a() {\n    if !ProviderId::Desktop.supported_on(OS) {\n"
            "        return;\n    }\n}\n"
        },
        [],
    ),
    (
        "early returns through cfg!",
        {
            LIB: "#[test]\nfn a() {\n    if cfg!(windows) {\n        return;\n    }\n}\n\n"
            "#[test]\nfn b() {\n    if !cfg!(unix) { return; }\n}\n\n"
            '#[test]\nfn c() {\n    if cfg!(target_os = "macos") { return; }\n}\n\n'
            '#[test]\nfn d() {\n    if cfg!(feature = "x") { return; }\n}\n'
        },
        [(f"{LIB}:3", "the early return of a"), (f"{LIB}:10", "the early return of b")],
    ),
    (
        "a reason above the wrong line",
        {LIB: "// windows: `SUDO_UID` is Unix's sign of sudo\n" + ignored("#[cfg(unix)]")},
        [
            (f"{LIB}:1", "is not right above a cfg or an early return"),
            (f"{LIB}:3", f"the test a, {NO_REASON}"),
        ],
    ),
    (
        "a reason line that is not accepted",
        {LIB: ignored("// windows: it fails there\n#[cfg(unix)]")},
        [(f"{LIB}:2", '"it fails there"')],
    ),
    (
        "an empty reason line",
        {LIB: ignored("// windows:\n#[cfg(unix)]")},
        [(f"{LIB}:2", "gives no reason")],
    ),
    (
        "an integration test file compiled out",
        {TESTS: '#![cfg(target_os = "macos")]\n\n#[test]\nfn a() {}\n'},
        [(f"{TESTS}:1", "this file, compiled out on Windows, says no reason")],
    ),
    (
        "a test binary's module compiled out",
        {"crates/a/tests/t/m.rs": '#![cfg(target_os = "macos")]\n\n#[test]\nfn a() {}\n'},
        [("crates/a/tests/t/m.rs:1", "this file, compiled out on Windows, says no reason")],
    ),
    (
        "integration test files with a reason, and for Windows",
        {
            TESTS: "// windows: a keychain's ceiling is what `security` reads from stdin\n"
            '#![cfg(target_os = "macos")]\n\n#[test]\nfn a() {}\n',
            "crates/a/tests/w.rs": "#![cfg(windows)]\n\n#[test]\nfn a() {}\n",
        },
        [],
    ),
    (
        "attributes in strings and comments",
        {
            LIB: 'const A: &str = "#[cfg(unix)] #[test] fn a() {} // windows: x";\n'
            '// #[cfg_attr(windows, ignore = "W1")]\n/* #[ignore] */\n'
            'const B: &str = r#"#[test]\n#[cfg(unix)]\nfn b() {}"#;\n'
        },
        [],
    ),
    (
        "a quote in a char literal and lifetimes",
        {
            LIB: "const Q: char = '\"';\nfn f<'a>(x: &'a str) -> &'a str { x }\n"
            + ignored("#[cfg(unix)]")
        },
        [(f"{LIB}:4", "the test a, compiled out on Windows")],
    ),
    (
        "a gap given past its ceiling",
        {
            LIB: ignored('#[cfg_attr(windows, ignore = "a gap being closed: one")]')
            + ignored('#[cfg_attr(windows, ignore = "a gap being closed: two")]', "b")
        },
        [("", '"a gap being closed" is given 2 times, more than its 1')],
    ),
    (
        "a test with no reason yet that is listed",
        {LIB: ignored("#[cfg(unix)]", "waits") + "\n" + ignored("#[cfg(unix)]", "other")},
        [(f"{LIB}:6", f"the test other, {NO_REASON}")],
    ),
    (
        "C# ignores with no reason or one not accepted",
        {
            CS: "[TestClass]\npublic sealed class T\n{\n"
            "    [TestMethod]\n    [Ignore]\n    public void A() { }\n\n"
            '    [TestMethod, Ignore("W14: waits")]\n    public void B() { }\n\n'
            '    [TestMethod]\n    [IgnoreAttribute("runs as a standard user only")]\n'
            "    public void C() { }\n}\n"
        },
        [(f"{CS}:5", "[Ignore] gives no reason"), (f"{CS}:8", '"W14: waits"')],
    ),
    (
        "C# OS conditions",
        {
            CS: "    [OSCondition(ConditionMode.Exclude, OperatingSystems.Windows, "
            'IgnoreMessage = "W14: x")]\n'
            "    [OSCondition(ConditionMode.Exclude, OperatingSystems.Windows)]\n"
            "    [OSCondition(OperatingSystems.Linux | OperatingSystems.OSX)]\n"
            '    [OSCondition(OperatingSystems.Windows, IgnoreMessage = "W14: runs here")]\n'
            "    [OSCondition(ConditionMode.Exclude, OperatingSystems.Linux)]\n"
            "    public void A() => Assert.AreEqual(new ResponseDecision.Ignore(), Decide());\n"
        },
        [
            (f"{CS}:1", '"W14: x"'),
            (f"{CS}:2", "an OSCondition that leaves Windows out gives no reason"),
            (f"{CS}:3", "an OSCondition that leaves Windows out gives no reason"),
        ],
    ),
    (
        "a C# condition's own IgnoreMessage",
        {
            CS: "    public Elevated(ConditionMode mode)\n        : base(mode) =>\n"
            "        IgnoreMessage = mode is ConditionMode.Include\n"
            '            ? "W14: elevated"\n            : "runs as a standard user only";\n\n'
            '    public override string GroupName => "Elevated";\n'
        },
        [(f"{CS}:3", '"W14: elevated"')],
    ),
    (
        "C# in build output and generated bindings",
        {
            "apps/windows/Pitboard.Core/obj/T.cs": "[Ignore]\n",
            "apps/windows/Pitboard.Core/Generated/T.cs": "[Ignore]\n",
        },
        [],
    ),
]


def run(files, accepted=ACCEPTED, unsaid=UNSAID):
    with tempfile.TemporaryDirectory() as scratch:
        root = Path(scratch)
        for path, text in files.items():
            (root / path).parent.mkdir(parents=True, exist_ok=True)
            (root / path).write_text(text, encoding="utf-8")
        return pending.check(root, accepted, unsaid)


class WindowsPending(unittest.TestCase):
    def test_cases(self):
        for name, files, expected in CASES:
            with self.subTest(name):
                problems = run(files).problems
                self.assertEqual(len(problems), len(expected), "\n".join(problems))
                for where, fragment in expected:
                    prefix = f"{where}: " if where else ""
                    found = [p for p in problems if p.startswith(prefix) and fragment in p]
                    self.assertTrue(found, f"no {where} {fragment!r} in:\n" + "\n".join(problems))

    def test_a_reason_no_test_gives_and_a_ceiling_above_its_count_are_noted(self):
        result = run(
            {LIB: ignored('#[cfg_attr(windows, ignore = "a gap being closed")]')},
            accepted={"a gap being closed": 2, "gone": None},
        )
        self.assertEqual(result.problems, [])
        self.assertEqual(
            result.notes(),
            [
                '"a gap being closed" is given 1 times: lower its ceiling to 1',
                'no test gives "gone" now: take it out of ACCEPTED',
                f"{LIB}: waits is not compiled out on Windows now: take it out of UNSAID",
            ],
        )

    def test_the_counts_are_by_reason(self):
        result = run(
            {
                LIB: ignored('#[cfg_attr(windows, ignore = "Claude Desktop is macOS only: one")]')
                + "#[test]\nfn b() {\n    // windows: Claude Desktop is macOS only\n"
                "    if !os::claude_desktop_runs_here() {\n        return;\n    }\n}\n"
            }
        )
        self.assertEqual(result.counts["Claude Desktop is macOS only"], 2)
        self.assertEqual(result.counts["`SUDO_UID` is Unix's sign of sudo"], 0)


if __name__ == "__main__":
    unittest.main()
