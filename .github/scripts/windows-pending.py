# Fails on a test Windows leaves out without a reason ACCEPTED lists: a Rust ignore reason, a
# `// windows: <reason>` line right above a cfg or an early return, or a C# Ignore or IgnoreMessage.
import bisect
import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

# How a reason starts, and how many tests may give it: None for one that holds for good, a
# number for a gap being closed, which may only shrink.
ACCEPTED = {
    "Claude Desktop is macOS only": None,
    "a firmlink to the Data volume is macOS's own": None,
    "a keychain's ceiling is what `security` reads from stdin": None,
    "the login keychain is macOS's own": None,
    "`SUDO_UID` is Unix's sign of sudo": None,
    "`systemctl` is Linux's own": None,
    "macOS and Linux print this help": None,
    "the probe refuses off Windows": None,
    # CI runs the UI tests as the elevated job user and again as a standard user.
    "runs as an elevated user only": None,
    "runs as a standard user only": None,
    "a warm-up runs on macOS and Linux": 17,
    "pins the stores a change touches as macOS and Linux have them": 1,
}

# Tests compiled out on Windows that give no reason yet, by file and name. Each may only go.
UNSAID = {
    (
        "crates/pitboard-ffi/src/model/threaded.rs",
        "adding_codex_without_its_cli_says_so_before_sign_in",
    ),
}

RUST_SKIPPED = re.compile(
    r"(?P<comment>//[^\n]*|/\*.*?\*/)"
    r'|(?P<raw>(?<!\w)[bc]?r(?P<hashes>#*)".*?"(?P=hashes))'
    r'|(?P<string>(?:(?<!\w)[bc])?"(?:[^"\\]|\\.)*")'
    r"|(?P<char>'(?:[^'\\\n]|\\u\{[0-9a-fA-F]{1,6}\}|\\x[0-9a-fA-F]{2}|\\[^\n])')",
    re.S,
)
CS_SKIPPED = re.compile(
    r"(?P<comment>//[^\n]*|/\*.*?\*/)"
    r'|(?P<raw>\$*""".*?""")'
    r'|(?P<verbatim>(?:\$@|@\$|@)"(?:[^"]|"")*")'
    r'|(?P<string>\$?"(?:[^"\\\n]|\\.)*")'
    r"|(?P<char>'(?:[^'\\\n]|\\[^\n]+?)')",
    re.S,
)
BRACKETS = re.compile(r"[()\[\]{}]")
TOP_LEVEL = re.compile(r"[()\[\]{},]")
VALUE_END = re.compile(r"[()\[\]{},;]")
CFG_TOKEN = re.compile(r'\s*(?:(?P<string>"(?:[^"\\]|\\.)*")|(?P<word>\w+)|(?P<mark>[(),=]))')
VISIBILITY = r"(?:pub(?:\s*\([^)]*\))?\s+)?"
FN = re.compile(VISIBILITY + r'(?:(?:async|const|unsafe|extern\s+"[^"]*")\s+)*fn\s+(\w+)')
MOD = re.compile(VISIBILITY + r"mod\s+(\w+)\s*([;{])")
ATTRIBUTE = re.compile(r"#!?\[")
SPACE = re.compile(r"\s*")
EARLY = re.compile(r"\bif\s+(?P<cond>[^{};]*?)\s*\{\s*return\s*;?\s*\}")
PLATFORM_CALL = re.compile(r"(?:\w+::)*\w+_runs_here\(\)|[\w:]+\.supported_on\([^()]*\)")
CS_ATTRIBUTES = re.compile(r"(?m)^[ \t]*\[(?=[A-Za-z_])")
CS_ATTRIBUTE = re.compile(r"\s*(?:[\w.]+\.)?(\w+?)(?:Attribute)?\s*(?=\(|$)")
IGNORE_MESSAGE = re.compile(r"\bIgnoreMessage\s*=(?!=)")
WINDOWS_LINE = "// windows:"
INTEGRATION_TEST = re.compile(r"crates/[^/]+/tests/[^/]+\.rs")
SKIPPED_FOLDERS = {"target", "bin", "obj", "Generated", "node_modules"}


# The text as written, and as code: every comment, string and char blanked to spaces, a
# string keeping its two quotes, so a bracket or a comma found in `code` is the code's own.
class Source:
    def __init__(self, root, path, skipped):
        self.path = path.relative_to(root).as_posix()
        self.text = path.read_text(encoding="utf-8-sig")
        self.starts = [0] + [m.end() for m in re.finditer("\n", self.text)]
        self.lines = self.text.split("\n")
        self.literals, self.literal_starts, self.comments = {}, [], []
        self.code = skipped.sub(self.blank, self.text)

    def blank(self, m):
        spaced = re.sub(r"[^\n]", " ", m.group(0))
        if m.group("comment") is not None:
            self.comments.append(m.start())
            return spaced
        if m.group("char") is not None:
            return spaced
        self.literals[m.start()] = m.group(0)
        self.literal_starts.append(m.start())
        return '"' + spaced[1:-1] + '"'

    def line(self, offset):
        return bisect.bisect_right(self.starts, offset)

    def closing(self, i):
        depth = 0
        for m in BRACKETS.finditer(self.code, i):
            depth += 1 if m.group(0) in "([{" else -1
            if depth == 0:
                return m.start()
        return len(self.code)

    def split(self, start, end):
        parts, depth, begun = [], 0, start
        for m in TOP_LEVEL.finditer(self.code, start, end):
            mark = m.group(0)
            if mark in "([{":
                depth += 1
            elif mark in ")]}":
                depth -= 1
            elif depth == 0:
                parts.append((begun, m.start()))
                begun = m.end()
        if self.code[begun:end].strip():
            parts.append((begun, end))
        return parts

    def strings_in(self, start, end):
        first = bisect.bisect_left(self.literal_starts, start)
        last = bisect.bisect_left(self.literal_starts, end)
        return [self.literals[at] for at in self.literal_starts[first:last]]

    def windows_line_above(self, offset):
        above = self.line(offset) - 1
        if above < 1:
            return None
        said = self.lines[above - 1].strip()
        if said.startswith(WINDOWS_LINE):
            return above, said[len(WINDOWS_LINE) :].strip()
        return None


def rust_string(literal):
    raw = re.fullmatch(r'[bc]?r(#*)"(.*)"\1', literal, re.S)
    if raw:
        return raw.group(2)
    body = literal[literal.index('"') + 1 : -1]
    body = re.sub(r"\\\n\s*", "", body)
    return re.sub(r"\\(.)", lambda m: {"n": "\n", "t": "\t"}.get(m.group(1), m.group(1)), body)


def cs_string(literal):
    literal = literal.lstrip("$@")
    if literal.startswith('"""'):
        return literal[3:-3].strip()
    return literal[1:-1].replace('""', '"').replace('\\"', '"')


# A cfg predicate's value on Windows: True, False, or None where a feature or the like decides.
def on_windows(predicate):
    tokens, at = [], 0
    while predicate[at:].strip():
        m = CFG_TOKEN.match(predicate, at)
        if not m:
            return None
        tokens.append(m.group("string") or m.group("word") or m.group("mark"))
        at = m.end()
    try:
        tree, used = parse_cfg(tokens, 0)
    except IndexError:
        return None
    return value_on_windows(tree) if used == len(tokens) else None


def parse_cfg(tokens, i):
    name = tokens[i]
    i += 1
    if i < len(tokens) and tokens[i] == "(":
        i += 1
        items = []
        while tokens[i] != ")":
            item, i = parse_cfg(tokens, i)
            items.append(item)
            if tokens[i] == ",":
                i += 1
        return (name, items), i + 1
    if i < len(tokens) and tokens[i] == "=":
        return (name, rust_string(tokens[i + 1])), i + 2
    return (name, None), i


def value_on_windows(tree):
    name, argument = tree
    if isinstance(argument, list):
        values = [value_on_windows(item) for item in argument]
        if name == "not" and len(values) == 1:
            return None if values[0] is None else not values[0]
        if name == "all":
            return False if False in values else (True if all(values) else None)
        if name == "any":
            return True if True in values else (False if all(v is False for v in values) else None)
        return None
    if argument is None:
        return {"windows": True, "unix": False, "test": True}.get(name)
    if name in ("target_os", "target_family"):
        return argument == "windows"
    return None


def both(a, b):
    if a is False or b is False:
        return False
    return True if a is True and b is True else None


class Attribute:
    def __init__(self, source, at, start, end, inner):
        self.source, self.at, self.start, self.end, self.inner = source, at, start, end, inner
        m = re.compile(r"\s*([\w:]+)\s*").match(source.code, start, end)
        self.path = m.group(1) if m else ""
        after = m.end() if m else start
        follows = source.code[after : after + 1]
        self.args = (after + 1, source.closing(after)) if follows == "(" else None
        self.value = source.strings_in(after, end) if follows == "=" else []

    def arguments(self):
        return self.source.split(*self.args) if self.args else []

    def text(self, span):
        return self.source.text[span[0] : span[1]]


def attribute_at(source, at):
    inner = source.code.startswith("#!", at)
    opening = at + (2 if inner else 1)
    close = source.closing(opening)
    return Attribute(source, at, opening + 1, close, inner), close + 1


def ignores(source, attribute, holds=True):
    if attribute.path == "ignore":
        reasons = [rust_string(s) for s in attribute.value]
        yield holds, (reasons[0] if reasons else None)
    elif attribute.path == "cfg_attr" and attribute.args:
        spans = attribute.arguments()
        if not spans:
            return
        holds = both(holds, on_windows(attribute.text(spans[0])))
        for span in spans[1:]:
            nested = Attribute(source, attribute.at, span[0], span[1], attribute.inner)
            yield from ignores(source, nested, holds)


def compiled_out(attribute):
    spans = attribute.arguments()
    return len(spans) == 1 and on_windows(attribute.text(spans[0])) is False


def returns_on_windows(cond):
    negated = cond.startswith("!")
    cond = cond.lstrip("!").strip()
    if PLATFORM_CALL.fullmatch(cond):
        return True
    cfg = re.fullmatch(r"cfg!\((.*)\)", cond, re.S)
    value = on_windows(cfg.group(1)) if cfg else None
    return value is not None and value != negated


class Check:
    def __init__(self, accepted, unsaid):
        self.accepted = accepted
        self.unsaid = set(unsaid)
        self.unsaid_used = set()
        self.problems = []
        self.counts = {reason: 0 for reason in accepted}

    def problem(self, source, offset, message):
        self.problems.append(f"{source.path}:{source.line(offset)}: {message}")

    def reason(self, source, offset, reason, what):
        if not reason:
            self.problem(source, offset, f"{what} gives no reason")
            return
        known = [prefix for prefix in self.accepted if reason.startswith(prefix)]
        if not known:
            self.problem(
                source,
                offset,
                f'{what} gives "{reason}", which windows-pending.py does not accept: make it '
                "pass on Windows, or give a reason ACCEPTED lists",
            )
            return
        self.counts[max(known, key=len)] += 1

    def left_out(self, source, offset, said, name, what):
        found = source.windows_line_above(offset)
        if found:
            above, reason = found
            said.add(above)
            where = source.starts[above - 1]
            self.reason(source, where, reason, f"the `{WINDOWS_LINE}` line of {what}")
        elif (source.path, name) in self.unsaid:
            self.unsaid_used.add((source.path, name))
        else:
            self.problem(
                source,
                offset,
                f"{what} says no reason: put `{WINDOWS_LINE} <reason>` on the line right above it",
            )

    def rust(self, source):
        said = set()
        integration = INTEGRATION_TEST.fullmatch(source.path)
        at = 0
        while True:
            m = ATTRIBUTE.search(source.code, at)
            if not m:
                break
            run, after = [], m.start()
            while True:
                attribute, after = attribute_at(source, after)
                run.append(attribute)
                gap = SPACE.match(source.code, after).end()
                if not ATTRIBUTE.match(source.code, gap):
                    break
                after = gap
            at = after
            item = SPACE.match(source.code, after).end()
            self.rust_run(source, run, item, integration, said)
        for start in source.comments:
            if source.text.startswith(WINDOWS_LINE, start) and source.line(start) not in said:
                self.problem(
                    source,
                    start,
                    f"`{WINDOWS_LINE}` is not right above a cfg or an early return that leaves "
                    "a test out on Windows",
                )

    def rust_run(self, source, run, item, integration, said):
        for attribute in run:
            for holds, reason in ignores(source, attribute):
                if holds is not False:
                    self.reason(source, attribute.at, reason, "an ignore that holds on Windows")
        if integration:
            for attribute in run:
                if attribute.inner and attribute.path == "cfg" and compiled_out(attribute):
                    what = "this file, compiled out on Windows,"
                    self.left_out(source, attribute.at, said, None, what)
        outer = [a for a in run if not a.inner]
        test = any(a.path == "test" or a.path.endswith("::test") for a in outer)
        fn = FN.match(source.code, item)
        module = MOD.match(source.code, item)
        if fn and test:
            name = fn.group(1)
            what = f"the test {name}, compiled out on Windows,"
            body = source.code.find("{", fn.end())
            self.early_returns(source, body, source.closing(body), name, said)
        elif module and self.test_module(source, outer, module):
            name = module.group(1)
            what = f"the test module {name}, compiled out on Windows,"
        else:
            return
        for attribute in outer:
            if attribute.path == "cfg" and compiled_out(attribute):
                self.left_out(source, attribute.at, said, name, what)

    def test_module(self, source, outer, module):
        if module.group(1) == "tests":
            return True
        for attribute in outer:
            cfg = source.code[attribute.start : attribute.end]
            if attribute.path == "cfg" and re.search(r"\btest\b", cfg):
                return True
        if module.group(2) == "{":
            opening = module.end() - 1
            return "#[test]" in source.code[opening : source.closing(opening)]
        return False

    def early_returns(self, source, start, end, name, said):
        for m in EARLY.finditer(source.code, start, end):
            cond = source.text[m.start("cond") : m.end("cond")].strip()
            if returns_on_windows(cond):
                self.left_out(source, m.start(), said, name, f"the early return of {name}")

    def csharp(self, source):
        conditions = []
        for m in CS_ATTRIBUTES.finditer(source.code):
            opening = m.end() - 1
            for start, end in source.split(opening + 1, source.closing(opening)):
                head = CS_ATTRIBUTE.match(source.code[start:end])
                if not head:
                    continue
                if head.group(1) == "Ignore":
                    reasons = [cs_string(s) for s in source.strings_in(start, end)]
                    self.reason(source, start, reasons[0] if reasons else None, "[Ignore]")
                elif head.group(1) == "OSCondition":
                    conditions.append((start, end))
                    inside = start + head.end()
                    args = []
                    if source.code[inside : inside + 1] == "(":
                        args = source.split(inside + 1, source.closing(inside))
                    self.os_condition(source, start, args)
        for m in IGNORE_MESSAGE.finditer(source.code):
            if not any(start <= m.start() < end for start, end in conditions):
                reasons = [cs_string(s) for s in source.strings_in(m.end(), self.ends(source, m))]
                for reason in reasons or [None]:
                    self.reason(source, m.start(), reason, "an IgnoreMessage")

    # Where the value assigned at `m` ends: a `;`, `,` or closing bracket of the code around it.
    def ends(self, source, m):
        depth = 0
        for mark in VALUE_END.finditer(source.code, m.end()):
            c = mark.group(0)
            if c in "([{":
                depth += 1
            elif c in ")]}" and depth > 0:
                depth -= 1
            elif depth == 0:
                return mark.start()
        return len(source.code)

    def os_condition(self, source, start, args):
        exclude, systems, message = False, set(), None
        for span in args:
            text = source.code[span[0] : span[1]].strip()
            if text.startswith("IgnoreMessage"):
                strings = source.strings_in(*span)
                message = cs_string(strings[0]) if strings else None
            elif "ConditionMode.Exclude" in text:
                exclude = True
            elif "OperatingSystems." in text:
                systems |= set(re.findall(r"OperatingSystems\.(\w+)", text))
        if ("Windows" in systems) == exclude:
            self.reason(source, start, message, "an OSCondition that leaves Windows out")

    def ceilings(self):
        for reason, ceiling in self.accepted.items():
            if ceiling is not None and self.counts[reason] > ceiling:
                self.problems.append(
                    f'"{reason}" is given {self.counts[reason]} times, more than its {ceiling}: '
                    "a gap being closed takes no new test"
                )

    def notes(self):
        said = []
        for reason, ceiling in self.accepted.items():
            count = self.counts[reason]
            if count == 0:
                said.append(f'no test gives "{reason}" now: take it out of ACCEPTED')
            elif ceiling is not None and count < ceiling:
                said.append(f'"{reason}" is given {count} times: lower its ceiling to {count}')
        for path, name in sorted(self.unsaid - self.unsaid_used):
            said.append(f"{path}: {name} is not compiled out on Windows now: take it out of UNSAID")
        return said


def sources(root, folder, suffix):
    base = root / folder
    if not base.is_dir():
        return
    for directory, folders, files in os.walk(base):
        folders[:] = sorted(f for f in folders if f not in SKIPPED_FOLDERS and f[0] != ".")
        for name in sorted(files):
            if name.endswith(suffix):
                yield Path(directory) / name


def check(root, accepted=ACCEPTED, unsaid=UNSAID):
    result = Check(accepted, unsaid)
    for path in sources(root, "crates", ".rs"):
        result.rust(Source(root, path, RUST_SKIPPED))
    for path in sources(root, "apps/windows", ".cs"):
        result.csharp(Source(root, path, CS_SKIPPED))
    result.ceilings()
    return result


def main():
    result = check(Path(sys.argv[1]) if len(sys.argv) > 1 else REPO)
    for problem in result.problems:
        print(problem, file=sys.stderr)
    note = "::warning::" if os.environ.get("GITHUB_ACTIONS") == "true" else "note: "
    for said in result.notes():
        print(f"{note}{said}")
    for reason, count in result.counts.items():
        ceiling = result.accepted[reason]
        print(f"{count:4} {reason}" + ("" if ceiling is None else f", at most {ceiling}"))
    for path, name in sorted(result.unsaid_used):
        print(f"   1 {path}: {name}, no reason yet")
    return 1 if result.problems else 0


if __name__ == "__main__":
    sys.exit(main())
