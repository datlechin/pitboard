import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { printed, sessions, type Session } from "./sessions.ts";
import { account, moments } from "./world.ts";
import { plainText } from "../lib/changelog.ts";
import { platforms, type Platform } from "../lib/platform.ts";
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
  const ours = text(statusLines(moments.after, "claude"));
  assert.deepEqual(ours, block(docs("quickstart.mdx"), ours[0], ours.length));
});

test("pitboard before the switch prints what Check what each account has left prints", () => {
  const ours = text(statusLines(moments.before, "claude"));
  assert.deepEqual(ours, block(docs("guides/usage.mdx"), ours[0], ours.length));
});

test("the status line before the switch is the Status line guide's", () => {
  assert.ok(
    docs("guides/status-line.mdx").includes(`\n${plainText(statusLine(moments.before))}\n`),
  );
});

test("the menu says an account's limits as the usage guide does, on each system", () => {
  const guide = docs("guides/usage.mdx");
  const personal = account("personal");
  assert.ok(guide.includes(`**${menuLine(personal, moments.after)}**`));
  assert.ok(guide.includes(`**${menuLine(personal, moments.after, "windows")}**`));
  assert.ok(guide.includes(`**personal: ${menuLine(personal, moments.after, "windows")}**`));
});

test("pitboard use work prints what the Quickstart prints", () => {
  const ours = text(printed(sessions.use, "macos"));
  assert.deepEqual(ours, block(docs("quickstart.mdx"), ours[0], ours.length));
});

test("pitboard watch install prints what watch.rs writes, and where each system keeps the service", () => {
  const source = joined(repo("crates/pitboard/src/watch.rs"));
  const words = repo("crates/pitboard-core/src/words.rs").replace(/=\s+"/g, '= "');
  const [first, ...rest] = text(printed(sessions.watchInstall, "linux"));
  assert.ok(source.includes(`"${first.replace(/\d+%/, "{}%")}\\n{}{location}\\n\\n{}\\n",`));
  assert.ok(source.includes(`"${rest[0]}\\n"`));
  const terms = rest.at(-1)!;
  const [said, url] = terms.split(": ");
  assert.ok(words.includes(`pub const TERMS_MAY_COVER_THIS: &str = "${said}";`));
  assert.ok(words.includes(`pub const IS_THIS_ALLOWED_URL: &str = "${url}";`));

  const where = (p: Platform) => text(printed(sessions.watchInstall, p)).at(-3)!;
  assert.ok(
    repo("crates/pitboard-core/src/host/macos/launchd.rs").includes(
      'const WATCH_LABEL: &str = "com.datlechin.pitboard.watch";',
    ),
  );
  assert.equal(where("macos"), "/Users/me/Library/LaunchAgents/com.datlechin.pitboard.watch.plist");
  const systemd = repo("crates/pitboard-core/src/host/linux/systemd.rs");
  assert.ok(systemd.includes('const WATCH: &str = "pitboard-watch.service";'));
  assert.ok(systemd.includes('.map_or_else(|| ctx.home().join(".config"), Path::to_path_buf)'));
  assert.ok(systemd.includes('.join("systemd/user")'));
  assert.equal(where("linux"), "/home/me/.config/systemd/user/pitboard-watch.service");
  const task = repo("crates/pitboard-core/src/host/task.rs");
  const sid = /const SID: &str = "(S-[\d-]+)";/.exec(task)![1];
  assert.ok(task.includes('Job::Watch => format!("Watch {sid}"),'));
  assert.ok(task.includes('path(Job::Watch, SID), format!(r"\\Pitboard\\Watch {SID}")'));
  assert.equal(where("windows"), `\\Pitboard\\Watch ${sid}`);

  // the reference's sample, from macOS at --at 90
  const macos = text(printed(sessions.watchInstall, "macos")).map((l) => l.replace(/\d+%/, "90%"));
  assert.deepEqual(macos, block(docs("reference/account-commands.mdx"), macos[0], macos.length));
  for (const p of platforms) {
    const lines = text(printed(sessions.watchInstall, p));
    assert.equal(lines.length, p === "linux" ? 5 : 4, p);
    assert.equal(lines.at(-2), "", p);
  }
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
  for (const line of text(statusLines(moments.after, "desktop"))) {
    const said = line.slice(line.search(/(signed in|ready)/));
    assert.ok(page.includes(`\`${said}\``), said);
  }
});

test("the menu says a Claude Desktop account has no usage read, as Account status does", () => {
  const [inUse] = listed(moments.after, "desktop");
  assert.ok(
    docs("reference/account-status.mdx").includes(
      `**${menuLine(inUse, moments.after)}** in the menu`,
    ),
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
    const [first] = listed(moments[moment], "claude");
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
  for (const p of platforms) assert.deepEqual(warm(p), warm("macos"), p);
  assert.match(warm("macos")[0], /, while the app or `pitboard watch` runs\.$/);
  assert.ok(
    repo("crates/pitboard/src/warm.rs").includes(
      'pub(crate) const WARMERS: &str = "the app or `pitboard watch`";',
    ),
  );
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
