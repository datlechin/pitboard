import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { printed, sessions, type Session } from "./sessions.ts";
import { account } from "./world.ts";
import { plainText } from "../lib/changelog.ts";
import { platforms } from "../lib/platform.ts";
import { listed, menuLine, statusHeading, statusLine, statusLines } from "../lib/words.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const docs = (path: string) => repo(`docs/${path}`);

function block(page: string, first: string, length: number): string[] {
  const lines = page.split("\n");
  const start = lines.findIndex((l) => l.trim() === first);
  assert.ok(start >= 0, `the page prints ${first}`);
  const indent = lines[start].indexOf(first);
  return lines.slice(start, start + length).map((l) => l.slice(indent).trimEnd());
}

const text = (lines: string[]) => lines.map((l) => plainText(l).trimEnd());

// A Rust string literal as it reads once its line continuations are joined.
const joined = (source: string) => source.replace(/\\\n\s*/g, "");

test("pitboard after the switch prints the Quickstart's accounts", () => {
  const ours = text(statusLines("claude", "after"));
  assert.deepEqual(ours, block(docs("quickstart.mdx"), ours[0], ours.length));
});

test("pitboard before the switch prints what Check what each account has left prints", () => {
  const ours = text(statusLines("claude", "before"));
  assert.deepEqual(ours, block(docs("guides/usage.mdx"), ours[0], ours.length));
});

test("the status line before the switch is the Status line guide's", () => {
  assert.ok(docs("guides/status-line.mdx").includes(`\n${plainText(statusLine("before"))}\n`));
});

test("the menu says an account's limits as the usage guide does", () => {
  assert.ok(docs("guides/usage.mdx").includes(`**${menuLine(account("personal"), "after")}**`));
});

test("pitboard use work prints what the Quickstart prints", () => {
  const ours = text(printed(sessions.use, "macos"));
  assert.deepEqual(ours, block(docs("quickstart.mdx"), ours[0], ours.length));
});

test("pitboard watch starts with the line the command line writes", () => {
  const source = joined(repo("crates/pitboard/src/watch.rs"));
  assert.ok(source.includes("limit of the one in use reaches {}%{}. {}"));
  assert.ok(source.includes('"Stop with Ctrl-C."'));
  assert.ok(source.includes('", and warms up {} from {}"'));
  const [start, watching] = text(printed(sessions.watch, "macos"));
  assert.match(
    start,
    /^Watching Claude Code\. Pitboard switches to another of your accounts once a limit of the one in use reaches \d+%, and warms up \w+ from \d\d:\d\d to \d\d:\d\d\. Stop with Ctrl-C\.$/,
  );
  const unwarmed = start.replace(/, and warms up .+ to \d\d:\d\d/, "").replace(/\d+%/, "90%");
  assert.ok(docs("guides/automatic-switching.mdx").split("\n").includes(unwarmed));
  for (const p of platforms) {
    assert.deepEqual(text(printed(sessions.watch, p)), [start, watching], p);
  }
  const shape =
    /^\d\d:\d\d:\d\d Watching \w+: \d+% of its (5-hour|weekly) limit, as of \d\d:\d\d\.$/;
  assert.match(watching, shape);
  assert.ok(
    docs("guides/automatic-switching.mdx")
      .split("\n")
      .some((l) => shape.test(l.trim())),
  );
});

test("pitboard warm lists accounts as the warm-up guide does", () => {
  const ours = text(printed(sessions.warm, "macos"));
  const guide = block(docs("guides/warm-up.mdx"), ours[0], 3);
  assert.equal(ours[0], guide[0]);
  assert.match(ours[1], /^  \w+\s+on   warms up again at \d\d:\d\d$/);
  assert.match(guide[1], /^  \w+\s+on   warms up again at \d\d:\d\d$/);
  assert.match(ours[2], /^  \w+\s+off$/);
  assert.match(guide[2], /^  \w+\s+off$/);
});

test("pitboard log writes lines in the shape log shows", () => {
  const shape = /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\+07:00  [\w-]+\s+[\w/]+  ok$/;
  for (const p of platforms)
    for (const line of text(printed(sessions.log, p))) assert.match(line, shape);
  const page = docs("reference/maintenance-commands.mdx");
  assert.ok(page.split("\n").filter((l) => shape.test(l)).length >= 3);
});

test("pitboard heads Claude Desktop's accounts as Account commands does", () => {
  assert.ok(
    docs("reference/account-commands.mdx").includes(`\`${plainText(statusHeading("desktop"))}\``),
  );
});

test("a Claude Desktop row ends as Account status says it does", () => {
  const page = docs("reference/account-status.mdx");
  for (const line of text(statusLines("desktop", "after"))) {
    const said = line.slice(line.search(/(signed in|ready)/));
    assert.ok(page.includes(`\`${said}\``), said);
  }
});

test("the menu says a Claude Desktop account has no usage read, as Account status does", () => {
  const [inUse] = listed("desktop", "after");
  assert.ok(
    docs("reference/account-status.mdx").includes(`**${menuLine(inUse, "after")}** in the menu`),
  );
});

test("pitboard use desktop/home prints what Claude Desktop's guide prints", () => {
  const ours = text(printed(sessions.useDesktop, "macos"));
  const theirs = block(
    docs("guides/claude-desktop.mdx"),
    "Switched to desktop/home; desktop/work is parked.",
    2,
  );
  const named = (lines: string[]) =>
    lines.map((l) => l.replace(/desktop\/\w+/g, "desktop/<label>"));
  assert.deepEqual(named(ours), named(theirs));
});

test("every listing gives the account in use first", () => {
  for (const moment of ["before", "after"] as const) {
    const [first] = listed("claude", moment);
    assert.equal(first.label, moment === "before" ? "personal" : "work");
  }
});

test("a session prints, on each system it runs on, its lines for every system and those for that one", () => {
  for (const [name, session] of Object.entries<Session>(sessions)) {
    for (const p of session.on ?? platforms) {
      const lines = printed(session, p);
      assert.ok(lines.length > 0, `${name} prints on ${p}`);
      assert.deepEqual(
        lines,
        session.output.filter((l) => !l.on || l.on.includes(p)).map((l) => l.html),
      );
    }
  }
});

test("pitboard lists Claude Desktop on macOS alone, where Pitboard supports it", () => {
  const source = repo("crates/pitboard-core/src/provider/mod.rs");
  assert.ok(source.includes("(ProviderId::Desktop, Os::MacOs) => true"));
  assert.ok(source.includes("(ProviderId::Desktop, Os::Linux | Os::Windows) => false"));
  for (const session of [sessions.status, sessions.statusBefore]) {
    const mac = text(printed(session, "macos"));
    const desktop = mac.indexOf(plainText(statusHeading("desktop")));
    assert.ok(desktop > 0);
    for (const p of ["windows", "linux"] as const) {
      assert.deepEqual(text(printed(session, p)), mac.slice(0, desktop - 1));
    }
  }
});

test("warm-ups run on every system, and Claude Desktop switches on macOS alone", () => {
  assert.ok(!repo("crates/pitboard-core/src/duty/warm/mod.rs").includes("Unbuilt"));
  assert.ok(!("on" in sessions.warm));
  const warm = (p: (typeof platforms)[number]) => text(printed(sessions.warm, p));
  assert.deepEqual(warm("windows"), warm("macos"));
  assert.match(warm("macos")[0], /, while the app or `pitboard watch` runs\.$/);
  assert.match(warm("linux")[0], /, while `pitboard watch` runs\.$/);
  assert.deepEqual(warm("linux").slice(1), warm("macos").slice(1));
  const source = repo("crates/pitboard/src/warm.rs");
  assert.ok(source.includes('Os::Linux => "`pitboard watch`"'));
  assert.ok(source.includes('Os::MacOs | Os::Windows => "the app or `pitboard watch`"'));
  assert.deepEqual(sessions.useDesktop.on, ["macos"]);
  assert.throws(() => printed(sessions.useDesktop, "linux"));
});

test("pitboard log lists only the changes each system makes, each verb padded to the longest", () => {
  const log = (p: (typeof platforms)[number]) => text(printed(sessions.log, p));
  assert.match(log("macos").join("\n"), /desktop\/home/);
  assert.match(log("macos").join("\n"), /warm-up/);
  assert.doesNotMatch(log("linux").join("\n"), /desktop\//);
  assert.match(log("linux").join("\n"), /warm-up/);
  assert.doesNotMatch(log("windows").join("\n"), /desktop\//);
  assert.match(log("windows").join("\n"), /warm-up/);
  assert.ok(repo("crates/pitboard/src/main.rs").includes("ui::pad(&e.verb, width)"));
  for (const p of platforms) {
    const verbs = log(p).map((l) => /^\S+ {2}(\S+ *) {2}\S+ {2}ok$/.exec(l)![1]);
    const width = Math.max(...verbs.map((v) => v.trim().length));
    for (const v of verbs) assert.equal(v.length, width, `${p}: "${v}"`);
  }
});
