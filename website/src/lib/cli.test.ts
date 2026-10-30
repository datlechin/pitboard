import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  didYouMean,
  differing,
  helpLines,
  jaro,
  parse,
  readHelps,
  readTree,
  snapshot,
  swapped,
  usage,
  type Command,
  type Grammar,
} from "./clap.ts";
import { ascii, checks, choose, location, run, type Machine, type Step } from "./cli.ts";
import { moment } from "./words.ts";
import { taps } from "./shell.ts";
import { plainText } from "./changelog.ts";
import { rowsOf } from "./doctor.ts";
import { DAY, HOUR, MINUTE, clock } from "../data/world.ts";
import { advance, model, reset } from "./model.ts";
import { SITE_BANNED, bannedFromDocs, wordPattern } from "../../scripts/guard.mjs";
import type { Platform } from "./platform.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const snaps = (name: string) => repo(`crates/pitboard/src/snapshots/pitboard__tests__${name}.snap`);
// A Rust source as it reads once its string literals' line continuations are joined.
const joined = (path: string) => repo(path).replace(/\\\n\s*/g, "");

const grammar = (windows: boolean): Grammar => ({
  tree: readTree(snaps(windows ? "command_tree_on_windows" : "command_tree")),
  helps: readHelps(snaps(windows ? "help_in_80_columns_on_windows" : "help_in_80_columns")),
  checks,
});

const machine = (platform: Platform): Machine => ({
  platform,
  version: "1.0.0-beta.5",
  grammar: grammar(platform === "windows"),
  doctor: rowsOf(
    repo(
      `crates/pitboard/src/render/snapshots/pitboard__render__doctor__tests__doctor_on_a_healthy_${{ macos: "mac", windows: "pc", linux: "linux" }[platform]}.snap`,
    ),
  ),
  script: async () => null,
});

const text = (lines: string[]) =>
  lines.map((l) =>
    plainText(l)
      .replaceAll("&quot;", '"')
      .replaceAll("&lt;", "<")
      .replaceAll("&gt;", ">")
      .replaceAll("&amp;", "&"),
  );
const said = async (platform: Platform, typed: string) =>
  text((await run(machine(platform), typed.split(" ").filter(Boolean))).lines);

test("clap's suggestions score by strsim's jaro, as suggestions.rs's own tests do", () => {
  assert.deepEqual(didYouMean("tst", ["test", "possible", "values"]), ["test"]);
  assert.deepEqual(didYouMean("te", ["test", "temp", "possible", "values"]), ["test", "temp"]);
  assert.deepEqual(didYouMean("hahaahahah", ["test", "possible", "values"]), []);
  assert.deepEqual(
    didYouMean("alignmentScorr", [
      "test",
      "possible",
      "values",
      "alignmentStart",
      "alignmentScore",
    ]),
    ["alignmentStart", "alignmentScore"],
  );
  assert.equal(jaro("", ""), 1);
  assert.equal(jaro("a", ""), 0);
  assert.ok(Math.abs(jaro("martha", "marhta") - 0.944) < 0.001);
});

test("every usage error main.rs keeps a snapshot of reads as the snapshot, on every system", () => {
  const cases = snapshot(snaps("usage_errors"))
    .split(/^\$ pitboard /m)
    .filter(Boolean);
  assert.equal(cases.length, 12);
  for (const windows of [false, true])
    for (const c of cases) {
      const [typed, ...want] = c.replace(/\n$/, "").split("\n");
      const out = parse(grammar(windows), typed.split(" "));
      assert.equal(out.kind, "error", typed);
      if (out.kind === "error") assert.deepEqual(text(out.lines), want, typed);
    }
});

test("every help prints its snapshot, and its usage line is the one clap's usage.rs makes", () => {
  for (const windows of [false, true]) {
    const g = grammar(windows);
    const walk = (c: Command, path: string[]) => {
      const help = g.helps.get(path.join(" "))!;
      assert.ok(help, path.join(" "));
      for (const how of [["--help"], ["-h"]]) {
        const out = parse(g, [...path.slice(1), ...how]);
        assert.deepEqual(out, { kind: "help", path }, `${path.join(" ")} ${how}`);
      }
      assert.deepEqual(parse(g, ["help", ...path.slice(1)]), { kind: "help", path });
      assert.deepEqual(text(helpLines(g, path)), help);
      const line = help.find((l) => l.startsWith("Usage: "))!;
      const needsCommand = line.endsWith("<COMMAND>");
      assert.equal(plainText(usage({ command: c, path, needsCommand }, [])), line);
      for (const sub of c.commands) if (sub.name !== "help") walk(sub, [...path, sub.name]);
    };
    walk(g.tree, ["pitboard"]);
  }
});

test("help keeps clap's styles: headings and usage bold and underlined, literals bold", () => {
  const g = grammar(false);
  const lines = helpLines(g, ["pitboard", "watch"]);
  assert.ok(lines.includes("<b><u>Commands:</u></b>"));
  assert.ok(lines.includes("<b><u>Usage:</u></b> <b>pitboard watch</b> [OPTIONS] [COMMAND]"));
  assert.ok(lines.some((l) => l.startsWith("  <b>install</b>    Have this")));
  assert.ok(lines.some((l) => l.startsWith("      <b>--at</b> &lt;PERCENT&gt;  Switch")));
  assert.ok(lines.some((l) => l.startsWith("  <b>-h</b>, <b>--help</b>")));
  const examples = helpLines(g, ["pitboard"]);
  assert.ok(
    examples.includes("  <b>pitboard use work             </b>  Switch Claude Code to work"),
  );
  // main.rs `examples`: the command padded to 30 inside the literal style
  assert.ok(
    repo("crates/pitboard/src/main.rs").includes("{literal}{typed:<30}{literal:#}  {does}"),
  );
});

test("the value parsers are main.rs's and watch.rs's", () => {
  const main = joined("crates/pitboard/src/main.rs");
  const watch = joined("crates/pitboard/src/watch.rs");
  assert.ok(watch.includes("value_parser = clap::value_parser!(u8)"));
  assert.ok(
    watch.includes(".range(i64::from(Threshold::LOWEST)..=i64::from(Threshold::HIGHEST)),"),
  );
  assert.ok(main.includes("lines: usize,"));
  assert.ok(main.includes("#[arg(value_parser = new_label)]"));
  assert.ok(main.includes("#[arg(value_parser = label_to_enroll, help = LABEL_TO_ENROLL)]"));
  assert.deepEqual(Object.keys(checks).sort(), [
    "pitboard enroll label",
    "pitboard log lines",
    "pitboard rename to",
    "pitboard watch at",
    "pitboard watch install at",
  ]);
  const g = grammar(false);
  const refused = (typed: string) => {
    const out = parse(g, typed.split(" "));
    return out.kind === "error" ? text(out.lines)[0] : null;
  };
  assert.equal(
    refused("watch --at 040"),
    "error: invalid value '040' for '--at <PERCENT>': 40 is not in 50..=99",
  );
  assert.equal(refused("watch --at +60"), null);
  assert.equal(
    refused("log -n 99999999999999999999999"),
    "error: invalid value '99999999999999999999999' for '--lines <LINES>': number too large to fit in target type",
  );
  // label.rs `choose` and `check`
  const label = joined("crates/pitboard-core/src/label.rs");
  assert.ok(
    label.includes(
      'pub const ONE_WORD: &str = "a label must be one word, such as `personal` or `work`";',
    ),
  );
  assert.equal(
    choose("codx/b"),
    "`codx` is not a tool Pitboard knows. It knows: claude, codex, desktop",
  );
  assert.ok(label.includes('"`{prefix}` is not a tool Pitboard knows. It knows: {}",'));
  assert.equal(choose("codex/"), "`codex/` names a tool but no account. Try `codex/work`");
  assert.ok(label.includes('"`{typed}` names a tool but no account. Try `{}{SEPARATOR}work`",'));
  assert.deepEqual(choose("codex/main"), { tool: "codex", label: "main" });
});

test("clap's other refusals read as a debug build of pitboard printed them", () => {
  const g = grammar(false);
  const refused = (typed: string) => {
    const out = parse(g, typed.split(" "));
    return out.kind === "error" ? text(out.lines) : out;
  };
  const tail = ["", "For more information, try '--help'."];
  assert.deepEqual(refused("status --offline --fresh"), [
    "error: the argument '--offline' cannot be used with '--fresh'",
    "",
    "Usage: pitboard status --offline",
    ...tail,
  ]);
  assert.deepEqual(refused("rename"), [
    "error: the following required arguments were not provided:",
    "  <FROM>",
    "  <TO>",
    "",
    "Usage: pitboard rename <FROM> <TO>",
    ...tail,
  ]);
  assert.deepEqual(refused("schedule --json"), [
    "error: 'pitboard schedule' requires a subcommand but one was not provided",
    "  [subcommands: install, status, uninstall, help]",
    "",
    "Usage: pitboard schedule [OPTIONS] <COMMAND>",
    ...tail,
  ]);
  assert.deepEqual(refused("schedule"), {
    kind: "help",
    path: ["pitboard", "schedule"],
    missing: true,
  });
  assert.deepEqual(refused("use -v"), [
    "error: unexpected argument '-v' found",
    "",
    "  tip: to pass '-v' as a value, use '-- -v'",
    "",
    "Usage: pitboard use [OPTIONS] <LABEL>",
    ...tail,
  ]);
  assert.deepEqual(refused("watch --at 90 install"), [
    "error: the subcommand 'install' cannot be used with '--at'",
    "",
    "Usage: pitboard watch [OPTIONS] [COMMAND]",
    ...tail,
  ]);
  // main.rs `command_line`
  assert.ok(
    joined("crates/pitboard/src/main.rs").includes(
      "let message = format!(\"the subcommand '{what}' cannot be used with '--{given}'\");",
    ),
  );
  assert.deepEqual(refused("use work --json=x"), [
    "error: unexpected value 'x' for '--json' found; no more were expected",
    "",
    "Usage: pitboard use --json <LABEL>",
    ...tail,
  ]);
  assert.deepEqual(refused("help use extra"), [
    "error: unrecognized subcommand 'extra'",
    "",
    "Usage: pitboard use [OPTIONS] <LABEL>",
    ...tail,
  ]);
});

test("--json makes a usage error an envelope, as main.rs `parse` does", async () => {
  assert.deepEqual(await said("macos", "use --json --json"), [
    '{"v":1,"command":null,"ok":false,"data":null,"warnings":[],"error":{"code":"usage","message":"error: the argument \'--json\' cannot be used multiple times\\n\\nUsage: pitboard use [OPTIONS] <LABEL>\\n\\nFor more information, try \'--help\'.","cause":null}}',
  ]);
  assert.ok(
    joined("crates/pitboard/src/main.rs").includes(
      'let wants_json = std::env::args_os().any(|arg| arg == "--json");',
    ),
  );
  assert.equal(ascii({ label: "ví dụ 😀" }), '{"label":"v\\u00ed d\\u1ee5 \\ud83d\\ude00"}');
  // what a shell's && and || read
  assert.equal((await run(machine("macos"), ["use"])).failed, true);
  assert.equal((await run(machine("macos"), ["use", "nobody", "--json"])).failed, true);
  assert.equal((await run(machine("macos"), ["--help"])).failed, false);
});

test("pitboard and pitboard status print the accounts as status.rs does, read now", async () => {
  reset();
  const lines = await said("macos", "");
  assert.equal(lines[0], "Claude Code");
  assert.ok(lines.includes("Claude Desktop · Pitboard reads no usage for Claude Desktop"));
  const windows = await said("windows", "status");
  assert.ok(!windows.some((l) => l.includes("Claude Desktop")));
  // --offline says the pace of the last reading, as of when it was read
  const offline = await said("linux", "status --offline");
  assert.ok(offline.includes("          weekly limit runs out in 11h 04m at this pace"));
});

test("pitboard use switches in the one state, and says so as main.rs does", async () => {
  reset();
  assert.deepEqual(await said("macos", "use work"), [
    "Switched to work; personal is parked.",
    "Claude Code sessions already running follow within 33 seconds.",
  ]);
  const s = model.get();
  assert.equal(s.inUse.claude, "work");
  assert.deepEqual(s.log.at(-1), {
    at: s.now,
    caller: "cli",
    verb: "use",
    subject: "work",
    outcome: "ok",
  });
  assert.deepEqual(await said("macos", "use work"), ["work is already signed in."]);
  assert.deepEqual(await said("linux", "use codex/spare"), [
    "Switched to codex/spare; codex/main is parked.",
    "Restart any running `codex` for this to take effect. It will not pick the switch up on its own.",
  ]);
  assert.deepEqual(await said("macos", "use desktop/office"), [
    "Switched to desktop/office; desktop/home is parked.",
    "Claude opens signed in to it.",
  ]);
  const main = joined("crates/pitboard/src/main.rs");
  assert.ok(main.includes('format!("{} is already signed in.{named}\\n", paint(BOLD, &label)),'));
  assert.ok(main.includes('"Switched to {}; {} is parked.\\n{follows}",'));
  assert.ok(main.includes('format!("{said}. It will not pick the switch up on its own.\\n"),'));
});

test("a label pitboard cannot use is refused in error.rs's words, and the refusal logged", async () => {
  reset();
  assert.deepEqual(await said("windows", "use nobody"), [
    "error: no account is enrolled as `nobody`. Enrolled: `claude/personal`, `claude/work`, `codex/main`, `codex/spare`. Run `pitboard enroll nobody --sign-in` to add it.",
  ]);
  assert.deepEqual(model.get().log.at(-1)?.outcome, "account_unknown");
  assert.deepEqual(await said("macos", "use codx/main"), [
    "error: `codx` is not a tool Pitboard knows. It knows: claude, codex, desktop.",
  ]);
  await said("macos", "rename codex/main work");
  assert.deepEqual(await said("macos", "use work"), [
    "error: `work` is enrolled for more than one tool: `claude/work`, `codex/work`. Say which one.",
  ]);
  const error = joined("crates/pitboard-core/src/error.rs");
  for (const words of [
    '"no account is enrolled as `{label}`.{enrolled} Run `pitboard enroll {label} --sign-in` to add it."',
    '"`{typed}` is not a tool Pitboard knows. It knows: {}.",',
    '"`{label}` is enrolled for more than one tool: {}. Say which one.",',
    '#[error("`{label}` is signed in; switch to another account before forgetting it.")]',
    '#[error("`{label}` already refers to {who}. Choose a different label.")]',
    'write!(f, " Enrolled: {}.", labels.join(", "))',
  ])
    assert.ok(error.includes(words), words);
});

test("pitboard forget asks first at a terminal, and reads y, Y or yes", async () => {
  reset();
  const m = machine("macos");
  const step = await run(m, ["forget", "work"]);
  assert.equal(
    step.ask?.question,
    "Forget work and delete its parked login? Adding it again needs a browser sign-in. [y/N] ",
  );
  assert.deepEqual(step.ask!.answer("no").lines, []);
  assert.ok(model.get().accounts.some((a) => a.label === "work"));
  const again = await run(m, ["forget", "work"]);
  assert.deepEqual(text(again.ask!.answer("yes").lines), ["Forgot work (me@company.com)."]);
  assert.ok(!model.get().accounts.some((a) => a.label === "work"));
  assert.deepEqual(text((await run(m, ["forget", "personal", "-y"])).lines), [
    "error: `personal` is signed in; switch to another account before forgetting it.",
  ]);
  assert.deepEqual(text((await run(m, ["forget", "codex/spare", "--json"])).lines), [
    '{"v":1,"command":"forget","ok":false,"data":null,"warnings":[],"error":{"code":"confirmation_needed","message":"`pitboard forget` asks before it deletes anything, and can ask only in a terminal without `--json`. Nothing was changed. Run it in a terminal, or add `--yes`.","cause":null}}',
  ]);
  const words = joined("crates/pitboard-core/src/words.rs");
  assert.ok(
    words.includes(
      '"Forget {label} and delete its parked login? Adding it again needs a browser sign-in. [y/N] "',
    ),
  );
  assert.ok(
    joined("crates/pitboard/src/main.rs").includes('matches!(answer.trim(), "y" | "Y" | "yes")'),
  );
  assert.ok(
    joined("crates/pitboard-core/src/error.rs").includes(
      '"`pitboard {command}` asks before it deletes anything, and can ask only in a terminal without `--json`. Nothing was changed. Run it in a terminal, or add `--yes`."',
    ),
  );
});

test("pitboard rename renames in the one state, and logs both names as service.rs does", async () => {
  reset();
  assert.deepEqual(await said("macos", "rename work job"), [
    "Renamed work to job (me@company.com).",
  ]);
  assert.equal(model.get().log.at(-1)?.subject, "work -> job");
  assert.ok(joined("crates/pitboard-core/src/service.rs").includes('&format!("{from} -> {to}"),'));
  assert.deepEqual(await said("macos", "rename job personal"), [
    "error: `personal` already refers to me@example.com. Choose a different label.",
  ]);
  assert.deepEqual(model.get().log.at(-1)?.outcome, "label_taken");
  assert.deepEqual(await said("macos", "rename job codex/x"), [
    "error: `job` is a Claude Code account, and a rename cannot move it to Codex. Sign in to that tool and enrol the account there instead.",
  ]);
});

test("pitboard log lists the newest entries oldest first, each verb padded to the longest", async () => {
  reset();
  await said("macos", "use work");
  assert.deepEqual(await said("macos", "log -n 2"), [
    "2026-10-13T18:46:12+07:00  use  desktop/home  ok",
    "2026-10-13T20:51:00+07:00  use  work  ok",
  ]);
  assert.deepEqual(await said("windows", "log -n 2"), [
    "2026-10-13T17:00:04+07:00  warm-up  personal  ok",
    "2026-10-13T20:51:00+07:00  use      work  ok",
  ]);
  assert.deepEqual(await said("windows", "log -n 0"), ["Pitboard has not changed anything yet."]);
  assert.ok(
    joined("crates/pitboard/src/main.rs").includes('"Pitboard has not changed anything yet.\\n"'),
  );
});

test("pitboard warm says each Claude Code account's standing with the command line's clock", async () => {
  reset();
  assert.deepEqual(await said("macos", "warm"), [
    "Pitboard warms up these accounts from 07:00 to 23:00, while the app or `pitboard watch` runs.",
    "  personal  on   warms up again at 22:00",
    "  work      off",
  ]);
  assert.deepEqual((await said("linux", "warm"))[0], (await said("macos", "warm"))[0]);
  assert.deepEqual(await said("macos", "warm now personal"), [
    "personal's 5-hour period runs already, until 22:00.",
  ]);
  assert.deepEqual(await said("macos", "warm now codex/main"), [
    "error: `codex/main` is a Codex account, and Pitboard warms up Claude Code accounts only: when a Codex period starts has not been measured.",
  ]);
  assert.deepEqual(await said("macos", "warm on work"), [
    "Pitboard warms up work from now on, within the span `pitboard warm` shows.",
  ]);
  assert.deepEqual(model.get().log.at(-1)?.verb, "warm-on");
  assert.deepEqual(await said("macos", "warm on work"), ["work was warmed up already."]);
  assert.deepEqual(await said("macos", "warm range 06:00 22:00"), [
    "Pitboard warms up accounts from 06:00 to 22:00.",
  ]);
  assert.equal(model.get().log.at(-1)?.subject, "06:00-22:00");
  assert.deepEqual(await said("macos", "warm range 7 22"), [
    "error: `7 22`: give each time as hours and minutes, such as 06:00 or 22:30",
  ]);
  const warm = joined("crates/pitboard/src/warm.rs");
  for (const words of [
    '"Pitboard warms up these accounts from {}, while {} runs.\\n",',
    'pub(crate) const WARMERS: &str = "the app or `pitboard watch`";',
    '"{}\'s 5-hour period runs already, until {}.\\n",',
    '"Pitboard warms up {} from now on, within the span `pitboard warm` shows.\\n",',
    '(true, false) => format!("{} was warmed up already.\\n", paint(BOLD, label)),',
    'format!("Pitboard warms up accounts from {range}.\\n"),',
  ])
    assert.ok(warm.includes(words), words);
  assert.equal(moment(clock.warned, clock.menuBar), "Oct 14 05:53");
  assert.equal(moment(clock.menuBar + HOUR, clock.menuBar), "21:51");
});

test("pitboard watch install, status and uninstall keep the watch service in the one state", async () => {
  reset();
  assert.deepEqual(await said("macos", "watch status"), [
    "Nothing watches Claude Code in the background here.",
    "Run `pitboard watch install` to change that.",
  ]);
  const installed = await said("linux", "watch install --at 90");
  assert.ok(installed[0].includes("reaches 90%"));
  assert.equal(installed[2], location("linux", "watch")[1]);
  assert.deepEqual(await said("windows", "watch status"), [
    "Pitboard watches Claude Code in the background, and switches once a limit of the account in use reaches 90%.",
    "\\Pitboard\\Watch S-1-5-21-1004-1005-1006-1001",
  ]);
  assert.deepEqual(await said("macos", "watch uninstall"), [
    "Stopped watching Claude Code in the background.",
  ]);
  assert.deepEqual(await said("macos", "watch uninstall --json"), [
    '{"v":1,"command":"watch","ok":true,"data":{"installed":false,"removed":false},"warnings":[],"error":null}',
  ]);
  const watch = joined("crates/pitboard/src/watch.rs");
  for (const words of [
    '"Nothing watches Claude Code in the background here.\\n{}\\n",',
    '"Run `pitboard watch install` to change that.".to_string()',
    '"Pitboard watches Claude Code in the background{}.\\n{location}\\n{}",',
    '", and switches once a limit of the account in use reaches {}%",',
    '"Stopped watching Claude Code in the background.\\n".to_string()',
    '"Nothing was watching Claude Code in the background.\\n".to_string()',
  ])
    assert.ok(watch.includes(words), words);
});

test("pitboard watch says what it decides as watch.rs does, and switches at its share", async () => {
  reset();
  const step = await run(machine("macos"), ["watch", "--at", "70"]);
  assert.deepEqual(text(step.lines), [
    "Watching Claude Code. Pitboard switches to another of your accounts once a limit of the one in use reaches 70%, and warms up personal from 07:00 to 23:00. Stop with Ctrl-C.",
  ]);
  assert.equal(model.get().watching, 70);
  advance(clock.menuBar + 30);
  const lines = text(step.runs!.next(model.get()));
  assert.deepEqual(lines, [
    "20:51:30 Switched Claude Code from personal to work: personal has used 73% of its weekly limit. Claude Code sessions already running follow within 33 seconds.",
  ]);
  assert.equal(model.get().inUse.claude, "work");
  assert.deepEqual(model.get().log.at(-1)?.verb, "auto-switch");
  advance(clock.menuBar + 60);
  assert.deepEqual(text(step.runs!.next(model.get())), [
    "20:52:00 Watching work: 40% of its weekly limit, as of 20:50.",
  ]);
  advance(clock.menuBar + 90);
  assert.deepEqual(step.runs!.next(model.get()), []);
  step.runs!.stop();
  assert.equal(model.get().watching, null);
  const watch = joined("crates/pitboard/src/watch.rs");
  assert.ok(
    watch.includes(
      '"Watching Claude Code. Pitboard switches to another of your accounts once a limit of the one in use reaches {}%{}. {}\\n",',
    ),
  );
  assert.ok(watch.includes('", and warms up {} from {}",'));
  const words = joined("crates/pitboard-core/src/words.rs");
  assert.ok(words.includes('"Switched Claude Code from {from} to {to}: {}.",'));
});

test("pitboard watch --once decides once, as the watch does", async () => {
  reset();
  assert.deepEqual(await said("macos", "watch --once"), [
    "Watching personal: 73% of its weekly limit, as of 20:50.",
  ]);
  assert.deepEqual(await said("macos", "watch --once --at 60"), [
    "Switched Claude Code from personal to work: personal has used 73% of its weekly limit. Claude Code sessions already running follow within 33 seconds.",
  ]);
  assert.equal(model.get().inUse.claude, "work");
});

test("pitboard enroll --sign-in says what it opens, and waits", async () => {
  reset();
  const step = await run(machine("macos"), ["enroll", "side", "--sign-in"]);
  assert.deepEqual(text(step.lines), [
    "Opening Claude Code's sign-in. Sign in as the account to add; the account in use now stays signed in.",
  ]);
  assert.ok(step.runs);
  const again = await run(machine("linux"), ["enroll", "codex/spare", "--sign-in"]);
  assert.ok(text(again.lines)[0].startsWith("Opening Codex's sign-in. Sign in as me@company.com;"));
  assert.ok(
    joined("crates/pitboard/src/main.rs").includes(
      '"Opening {}\'s sign-in. Sign in as {who}; the account in use now stays signed in.",',
    ),
  );
  assert.deepEqual(await said("macos", "enroll personal"), [
    "Enrolled personal (me@example.com), the account signed in now.",
    "Add another without signing out of it: pitboard enroll <label> --sign-in",
  ]);
  assert.deepEqual(await said("macos", "enroll other"), [
    "error: me@example.com is already enrolled as `personal`. To add a different account, run `pitboard enroll <label> --sign-in`.",
  ]);
});

test("the schedule, renew, stow, repair, abandon and adopt say what the world's machine holds", async () => {
  reset();
  assert.deepEqual(await said("macos", "renew"), ["No parked login was due."]);
  assert.deepEqual(await said("linux", "schedule install"), [
    "Parked logins will be renewed daily.",
    "/home/me/.config/systemd/user/pitboard-renew.timer",
    "",
    "It renews your own parked logins and does nothing else: it never switches account and never asks Anthropic for usage.",
  ]);
  assert.equal(model.get().schedule, true);
  assert.deepEqual(await said("macos", "schedule status"), [
    "Parked logins are renewed every 1d 0h by this computer's own scheduler.",
    "/Users/me/Library/LaunchAgents/com.datlechin.pitboard.renew.plist",
  ]);
  assert.deepEqual(await said("windows", "schedule uninstall"), [
    "Stopped renewing parked logins on a schedule. They are renewed when you run `pitboard`, and otherwise not.",
  ]);
  assert.deepEqual(await said("windows", "schedule uninstall"), ["There was nothing scheduled."]);
  const main = joined("crates/pitboard/src/main.rs");
  for (const words of [
    '"Parked logins will be renewed daily.\\n{location}\\n\\nIt renews your own parked logins and does nothing else: it never switches account and never asks Anthropic for usage.\\n"',
    '"There was nothing scheduled.\\n".to_string()',
    '"There is no interrupted switch to give up on.\\n"',
    '"Every parked login here is accounted for.\\n".to_string()',
    '"This Pitboard directory was already written on this computer.\\n".into(),',
  ])
    assert.ok(main.includes(words), words);
  assert.ok(
    joined("crates/pitboard-core/src/words.rs").includes(
      '"Nothing is left behind the store Claude Code keeps its login in, so there is nothing to put away."',
    ),
  );
  assert.ok(
    repo("crates/pitboard-core/src/host/macos/launchd.rs").includes(
      'const LABEL: &str = "com.datlechin.pitboard.renew";',
    ),
  );
  assert.ok(
    repo("crates/pitboard-core/src/host/linux/systemd.rs").includes(
      'const TIMER: &str = "pitboard-renew.timer";',
    ),
  );
  assert.ok(
    repo("crates/pitboard-core/src/host/task.rs").includes('Job::Renew => format!("Renew {sid}"),'),
  );
});

test("pitboard completions with --json says it has no JSON form, as main.rs does", async () => {
  assert.deepEqual(await said("macos", "completions zsh --json"), [
    '{"v":1,"command":"generate","ok":false,"data":{},"warnings":[],"error":{"code":"output_is_not_a_report","message":"this command writes a generated file to stdout, so it has no JSON form"}}',
  ]);
});

test("pitboard doctor prints each system's snapshot, as the page's state stands at the read", async () => {
  for (const [platform, kept] of [
    ["macos", "mac"],
    ["windows", "pc"],
    ["linux", "linux"],
  ] as const) {
    reset();
    // the snapshot's moment: the read before the hero's demo, personal in use
    model.setKey("now", clock.read);
    await said(platform, "use personal");
    model.setKey("lastSwitch", {});
    model.setKey("now", clock.read);
    const printed = snapshot(
      repo(
        `crates/pitboard/src/render/snapshots/pitboard__render__doctor__tests__doctor_on_a_healthy_${kept}.snap`,
      ),
    );
    assert.deepEqual(await said(platform, "doctor"), printed.split("\n"), platform);
  }
});

test("pitboard doctor follows a switch, a rename, a forget and an account added", async () => {
  reset();
  await said("macos", "use work");
  await said("macos", "rename codex/spare extra");
  let lines = await said("macos", "doctor");
  // a check by its mark and whole name
  const row = (start: string) => lines.find((l) => l.startsWith(`${start}  `));
  assert.match(row("✓ identity")!, /me@company\.com {2}· {2}org e4f8a2c6-/);
  assert.match(row("✓ in use")!, /`work`, as Anthropic said \d\d:\d\d$/);
  assert.match(row("✓ credential")!, /refresh 7f344b8dffee70ba {2}· {2}good for 26d \dh/);
  assert.match(row("✓ account personal")!, /parked, good for 26d \dh$/);
  assert.ok(row("✓ account codex/extra"));
  await said("macos", "forget personal --yes");
  lines = await said("macos", "doctor");
  assert.equal(row("✓ accounts"), "✓ accounts                           5 enrolled");
  assert.ok(!row("✓ account personal"));
});

test("the scripts each system's release would write: kept, or the kept one in that system's words", () => {
  const off = readTree(snaps("command_tree"));
  const on = readTree(snaps("command_tree_on_windows"));
  const pairs = differing(off, on);
  assert.equal(pairs.length, 6);
  const zsh = snapshot(snaps("completions_zsh"));
  const powershell = snapshot(snaps("completions_powershell_on_windows"));
  for (const [script, toWindows] of [
    [zsh, true],
    [snapshot(snaps("completions_fish")), true],
    [snapshot(snaps("completions_elvish")), true],
    [powershell, false],
  ] as const) {
    const changed = swapped(script, pairs, toWindows);
    for (const [a, b] of pairs) {
      const from = toWindows ? a : b;
      const to = toWindows ? b : a;
      assert.ok(!changed.includes(from) || to.includes(from), from);
      if (script.includes(from)) assert.ok(changed.includes(to), to);
    }
  }
  assert.ok(swapped(zsh, pairs, true).includes("behind Credential Manager, then delete the file"));
});

// Everything the command line prints across the commands a visitor may type.
test("everything the command line prints keeps the site's rules: no dash, a committed face", async () => {
  const mono = new Set<number>(
    JSON.parse(
      readFileSync(
        new URL("../assets/fonts/JetBrainsMono-2.304-wght.codepoints.json", import.meta.url),
        "utf8",
      ),
    ),
  );
  const banned = wordPattern([...bannedFromDocs(repo("docs/AGENTS.md")), ...SITE_BANNED]);
  const source = [
    "crates/pitboard/src/main.rs",
    "crates/pitboard/src/vault.rs",
    "crates/pitboard/src/warm.rs",
    "crates/pitboard/src/watch.rs",
    "crates/pitboard-core/src/words.rs",
    "crates/pitboard-core/src/error.rs",
  ]
    .map(joined)
    .join("\n");
  const typed = [
    "",
    "--help",
    "use work",
    "use nobody",
    "rename codex/main main2",
    "log",
    "warm",
    "warm now work",
    "watch install",
    "watch status",
    "watch --once",
    "schedule status",
    "renew",
    "stow",
    "repair",
    "abandon",
    "adopt",
    "doctor",
    "statusline",
    "enroll x --sign-in",
    "lst",
    "use",
  ];
  const all = new Set<string>();
  for (const platform of ["macos", "windows", "linux"] as const) {
    reset();
    for (const t of typed) {
      const step: Step = await run(machine(platform), t.split(" ").filter(Boolean));
      text(step.lines).forEach((l) => all.add(l));
    }
    const help = grammar(platform === "windows").helps;
    for (const lines of help.values()) lines.forEach((l) => all.add(l));
  }
  assert.ok(all.size > 200, `${all.size} lines`);
  for (const line of all) {
    assert.doesNotMatch(line, /[\u2013\u2014]/, line);
    for (const ch of line) {
      const cp = ch.codePointAt(0)!;
      if (cp >= 0x20 && ch !== " ") assert.ok(mono.has(cp), `U+${cp.toString(16)} in "${line}"`);
    }
    for (const m of line.matchAll(banned))
      assert.ok(source.includes(m[0]), `"${m[0]}" in "${line}"`);
  }
  void DAY;
  void MINUTE;
});

test("each command a phone taps runs on its system without a usage error", async () => {
  for (const platform of ["macos", "windows", "linux"] as const) {
    reset();
    for (const tapped of taps("work")) {
      if (tapped === "clear") continue;
      const lines = await said(platform, tapped.replace(/^pitboard ?/, ""));
      assert.ok(lines.length > 0, tapped);
      assert.ok(!lines[0].startsWith("error:"), `${platform}: ${tapped}`);
    }
  }
});
