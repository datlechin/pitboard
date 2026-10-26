import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  ADOPTION_CEILING_SECONDS,
  DECIDE_EVERY_SECONDS,
  FULL,
  HIGHEST,
  LOWEST,
  PERIOD,
  READ_EVERY_SECONDS,
  SAME_RESET,
  SETTLING_SECONDS,
  SIGN_IN_LASTS,
  START_STEP,
  advance,
  afterSwitch,
  atLimitNote,
  autoStandingSaid,
  clamped,
  countdown,
  decide,
  enroll,
  forget,
  model,
  nameProblem,
  nextWarmUp,
  notSwitching,
  quit,
  refresh,
  rename,
  reset,
  setChoice,
  setShare,
  shareOfLimit,
  signInAgain,
  start,
  use,
  warmNote,
  warmSaid,
  warmStanding,
  warmUp,
  type Decision,
  type Skip,
  type State,
  type WarmStanding,
} from "./model.ts";
import {
  capitalised,
  limitRow,
  menuBarTitle,
  menuLine,
  notes,
  statusLine,
  statusLines,
  trayTip,
  hm,
} from "./words.ts";
import { plainText } from "./changelog.ts";
import { DAY, HOUR, MINUTE, WEEK, clock, type Account, type Limit } from "../data/world.ts";
import { SITE_BANNED, bannedFromDocs, wordPattern } from "../../scripts/guard.mjs";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const core = (path: string) => repo(`crates/pitboard-core/src/${path}`);
const ffi = (path: string) => repo(`crates/pitboard-ffi/src/${path}`);
const autoswitch = core("duty/autoswitch.rs");
const advising = ffi("model/advising.rs");

// A Rust test the port answers to, which has to still be there.
const answersTo = (source: string, name: string) =>
  assert.ok(source.includes(`fn ${name}() {`), `the Rust test ${name} is still there`);

test("the model's constants are the core's", () => {
  assert.ok(autoswitch.includes(`pub const LOWEST: u8 = ${LOWEST};`));
  assert.ok(autoswitch.includes(`pub const HIGHEST: u8 = ${HIGHEST};`));
  assert.ok(autoswitch.includes("pub const DEFAULT: Threshold = Threshold(95);"));
  assert.equal(start().preferences.share, 95);
  assert.ok(autoswitch.includes(`pub(crate) const SETTLING_SECONDS: i64 = ${SETTLING_SECONDS};`));
  assert.ok(
    core("switch/mod.rs").includes(
      `pub const ADOPTION_CEILING_SECONDS: u32 = ${ADOPTION_CEILING_SECONDS};`,
    ),
  );
  assert.ok(ffi("model/advice.rs").includes(`const FULL: u8 = ${FULL};`));
  assert.ok(core("usage/mod.rs").includes(`const SAME_RESET: u64 = ${SAME_RESET};`));
  const cadence = core("duty/cadence.rs");
  assert.ok(cadence.includes(`pub const READ_EVERY_SECONDS: i64 = ${READ_EVERY_SECONDS};`));
  assert.ok(cadence.includes(`pub const DECIDE_EVERY_SECONDS: i64 = ${DECIDE_EVERY_SECONDS};`));
  const warm = core("duty/warm/decide.rs");
  assert.ok(warm.includes("const PERIOD: i64 = 5 * 3_600;"));
  assert.equal(PERIOD, 5 * HOUR);
  assert.ok(warm.includes(`const START_STEP: i64 = ${START_STEP};`));
  assert.ok(
    core("provider/claude/assumptions.rs").includes("`refresh_token_expires_in`, or 30 days on"),
  );
  assert.equal(SIGN_IN_LASTS, 30 * DAY);
  assert.ok(core("duty/warm/mod.rs").includes("from: 6 * 60,\n        to: 22 * 60,"));
});

test("the page starts on the hero's first frame, with the app's default choice made", () => {
  const s = start();
  assert.equal(s.now, clock.menuBar);
  assert.equal(s.readAt, clock.read);
  assert.deepEqual(s.inUse, { claude: "personal", codex: "main", desktop: "home" });
  assert.deepEqual(s.preferences, { atLimit: "notify", chosen: true, share: 95 });
  const preferences = ffi("model/preferences.rs");
  assert.ok(
    preferences.includes(
      "_ if self.auto_switch => AtLimit::Switch,\n            _ => AtLimit::Notify,",
    ),
  );
  assert.ok(!s.log.some((e) => e.at === clock.switched), "the demo makes the switch to work");
  assert.deepEqual([s.warm.from, s.warm.to], [7 * 60, 23 * 60]);
  assert.equal(menuBarTitle(s), "personal 73%");
});

const NOW = clock.menuBar;
const session = (percent: number, resetsAt: number | null = NOW + 2 * HOUR): Limit => ({
  percent,
  resetsAt,
  lengthSeconds: 5 * HOUR,
});
const weekly = (percent: number): Limit => ({
  percent,
  resetsAt: NOW + 3 * DAY,
  lengthSeconds: WEEK,
});
const scoped = (scope: string, percent: number): Limit => ({ ...weekly(percent), scope });
const row = (label: string, limits: Limit[]): Account => ({
  id: label,
  tool: "claude",
  label,
  email: `${label}@example.com`,
  plan: "Max 20x",
  limits,
});
const workAt = (percent: number) => row("work", [session(percent), weekly(20)]);
const spare = () => row("spare", [session(10), weekly(30)]);

// autoswitch.rs's tests' world: work in use, read now, among `rows`.
function rows(...claude: Account[]): State {
  const s = start();
  s.readAt = NOW;
  s.accounts = [...claude, ...s.accounts.filter((a) => a.tool !== "claude")];
  s.inUse.claude = "work";
  return s;
}
const to = (d: Decision) => (d.decided === "switch" ? d.to.label : null);

test("a_limit_below_the_share_is_left_alone_and_one_at_it_switches", () => {
  answersTo(autoswitch, "a_limit_below_the_share_is_left_alone_and_one_at_it_switches");
  const below = decide(rows(workAt(94.4), spare()), 95);
  assert.equal(below.decided, "below");
  assert.equal(below.decided === "below" && below.nearest?.percent, 94.4);
  const at = decide(rows(workAt(95), spare()), 95);
  assert.equal(to(at), "spare");
  assert.equal(at.decided === "switch" && at.limit.lengthSeconds, 5 * HOUR);
  assert.equal(to(decide(rows(workAt(94.5), spare()), 95)), "spare", "drawn at 95%");
});

test("an_account_with_any_limit_at_the_share_is_no_place_to_go", () => {
  answersTo(autoswitch, "an_account_with_any_limit_at_the_share_is_no_place_to_go");
  const full = () => row("spare", [session(0), weekly(95)]);
  const none = decide(rows(workAt(96), full()), 95);
  assert.equal(none.decided === "hold" && none.why.code, "no_room");
  const roomy = row("roomy", [session(60), weekly(10)]);
  assert.equal(to(decide(rows(workAt(96), full(), roomy), 95)), "roomy");
});

test("the_account_with_most_room_in_that_limit_is_chosen_then_the_least_full", () => {
  answersTo(autoswitch, "the_account_with_most_room_in_that_limit_is_chosen_then_the_least_full");
  const less = () => row("less", [session(50), weekly(10)]);
  const more = row("more", [session(20), weekly(80)]);
  assert.equal(to(decide(rows(workAt(96), less(), more), 95)), "more");
  const tie = row("tie", [session(50), weekly(5)]);
  assert.equal(to(decide(rows(workAt(96), less(), tie), 95)), "tie");
  const same = row("same", [session(50), weekly(10)]);
  assert.equal(to(decide(rows(workAt(96), less(), same), 95)), "less", "the first of two alike");
});

test("an_account_without_a_weekly_limit_for_all_models_is_chosen_first", () => {
  answersTo(autoswitch, "an_account_without_a_weekly_limit_for_all_models_is_chosen_first");
  const seat = (label: string, used: number) => row(label, [session(used), scoped("Fable", 0)]);
  const max = () => row("max", [session(0), weekly(10)]);
  assert.equal(to(decide(rows(workAt(96), max(), seat("seat", 90)), 95)), "seat");
  assert.equal(
    to(decide(rows(workAt(96), max(), seat("seat", 90), seat("fresher", 40)), 95)),
    "fresher",
  );
});

test("an_account_just_put_in_use_is_left_to_settle", () => {
  answersTo(autoswitch, "an_account_just_put_in_use_is_left_to_settle");
  const s = rows(workAt(96), spare());
  s.lastUsedAt.work = NOW - SETTLING_SECONDS + 1;
  const held = decide(s, 95);
  assert.deepEqual(held.decided === "hold" && held.why, { code: "settling", until: NOW + 1 });
  s.lastUsedAt.work = NOW - SETTLING_SECONDS;
  assert.equal(to(decide(s, 95)), "spare");
});

test("a_limit_switched_away_from_is_switched_away_from_again_in_its_next_window", () => {
  answersTo(
    autoswitch,
    "a_limit_switched_away_from_is_switched_away_from_again_in_its_next_window",
  );
  const s = rows(workAt(96), spare());
  s.ledger["claude/work/session/"] = NOW + 2 * HOUR;
  const again = decide(s, 95);
  assert.equal(again.decided === "hold" && again.why.code, "already_switched");
  s.accounts[0].limits[0].resetsAt = NOW + 3 * HOUR;
  assert.equal(to(decide(s, 95)), "spare");
});

// The page's own state, with the limits of the Claude Code accounts set.
function reading(limits: Record<string, Limit[]>) {
  const s = model.get();
  model.set({
    ...s,
    accounts: s.accounts.map((a) =>
      a.tool !== "desktop" && limits[`${a.tool}/${a.label}`]
        ? { ...a, limits: limits[`${a.tool}/${a.label}`] }
        : a,
    ),
  });
}
const posted = () =>
  model.get().posted.map((p) => [p.title, p.subtitle, p.body, p.switchTo?.label ?? null]);

test("a_limit_at_the_share_is_notified_once_before_it_runs_out", () => {
  answersTo(advising, "a_limit_at_the_share_is_notified_once_before_it_runs_out");
  reset();
  reading({ "claude/personal": [session(94), weekly(20)] });
  refresh();
  assert.deepEqual(posted(), [], "below the share");
  reading({ "claude/personal": [session(96), weekly(20)] });
  refresh();
  const once = [
    [
      "personal has used 96% of its 5-hour limit",
      "Claude Code",
      "work has 88% of its own left.",
      "work",
    ],
  ];
  assert.deepEqual(posted(), once);
  assert.deepEqual(model.get().advice, [], "the window offers it at 100%");
  reading({ "claude/personal": [session(100), weekly(20)] });
  refresh();
  assert.deepEqual(posted(), once, "one limit, one notification");
  assert.deepEqual(
    model.get().advice.map((a) => a.instead),
    ["work"],
  );
});

test("a_limit_at_the_share_with_nowhere_to_go_says_when_it_resets", () => {
  answersTo(advising, "a_limit_at_the_share_with_nowhere_to_go_says_when_it_resets");
  reset();
  assert.equal(forget("claude", "work"), "ok");
  reading({ "claude/personal": [session(97, NOW + HOUR + 5 * MINUTE)] });
  refresh();
  assert.deepEqual(posted(), [
    ["personal has used 97% of its 5-hour limit", "Claude Code", "Resets in 1h 05m.", null],
  ]);
});

test("an_account_past_the_share_is_not_offered_at_the_share", () => {
  answersTo(advising, "an_account_past_the_share_is_not_offered_at_the_share");
  reset();
  reading({
    "claude/personal": [session(96, NOW + HOUR + 5 * MINUTE)],
    "claude/work": [session(98, NOW + 2 * HOUR + 30 * MINUTE)],
  });
  refresh();
  assert.deepEqual(posted(), [
    ["personal has used 96% of its 5-hour limit", "Claude Code", "Resets in 1h 05m.", null],
  ]);
});

test("do_nothing_notifies_nothing", () => {
  answersTo(advising, "do_nothing_notifies_nothing");
  reset();
  setChoice("nothing");
  reading({ "claude/personal": [session(100), weekly(20)] });
  refresh();
  assert.deepEqual(posted(), []);
  assert.deepEqual(
    model.get().advice.map((a) => a.instead),
    ["work"],
  );
});

test("switch_notifies_of_codex_alone, and Codex Plus has no room against Pro's weekly limit", () => {
  answersTo(advising, "switch_notifies_of_codex_alone");
  assert.ok(core("provider/codex/api.rs").includes("lists_every_limit: false,"));
  assert.ok(core("usage/mod.rs").includes("if !lists_every_limit {"));
  reset();
  setChoice("switch");
  reading({
    "claude/personal": [session(97), weekly(20)],
    "codex/main": [{ ...weekly(97), resetsAt: NOW + 5 * DAY + 18 * HOUR }],
  });
  refresh();
  const atLimit = model.get().posted.filter((p) => p.subtitle !== null);
  assert.deepEqual(
    atLimit.map((p) => [p.title, p.subtitle, p.body, p.switchTo]),
    [["main has used 97% of its weekly limit", "Codex", "Resets in 5d 18h.", null]],
  );
  assert.equal(model.get().inUse.claude, "work", "Claude Code was switched instead");
  assert.ok(model.get().posted.some((p) => p.title === "Switched Claude Code to work"));
});

test("one limit of each tool is notified a pass, the 5-hour first, the weekly at the next read", () => {
  reset();
  setShare(50);
  assert.deepEqual(posted(), [
    [
      "personal has used 59% of its 5-hour limit",
      "Claude Code",
      "work has 88% of its own left.",
      "work",
    ],
  ]);
  advance(model.get().readAt + READ_EVERY_SECONDS);
  assert.deepEqual(posted().at(-1), [
    "personal has used 73% of its weekly limit",
    "Claude Code",
    "work has 60% of its own left.",
    "work",
  ]);
  advance(model.get().readAt + READ_EVERY_SECONDS);
  assert.equal(posted().length, 2);
});

test("Pitboard switches by itself only once the account has settled, then never back", () => {
  reset();
  assert.equal(use("claude", "work"), "ok");
  advance(NOW + 2);
  assert.equal(use("claude", "personal"), "ok");
  const putBack = model.get().now;
  setChoice("switch");
  setShare(50);
  const s = model.get();
  assert.equal(s.inUse.claude, "personal");
  assert.deepEqual(posted().at(-1), [
    "Claude Code wasn’t switched",
    null,
    "personal has used 73% of its weekly limit. The account was put in use less than 5 minutes ago.",
    null,
  ]);
  assert.equal(
    autoStandingSaid(s.autoStanding!),
    `personal has used 73% of its weekly limit. Pitboard is not switching: the account was put in use less than 5 minutes ago. It decides again at ${hm(putBack + SETTLING_SECONDS)}.`,
  );
  const told = model.get().posted.length;
  advance(putBack + SETTLING_SECONDS - 1);
  assert.equal(model.get().inUse.claude, "personal");
  assert.equal(model.get().posted.length, told, "said once");
  advance(putBack + SETTLING_SECONDS + DECIDE_EVERY_SECONDS);
  const after = model.get();
  assert.equal(after.inUse.claude, "work");
  assert.deepEqual(posted().at(-1), [
    "Switched Claude Code to work",
    null,
    "personal has used 73% of its weekly limit. Sessions already running follow within 33 seconds.",
    null,
  ]);
  assert.deepEqual(
    after.log.slice(-1).map((e) => [e.caller, e.verb, e.subject, e.outcome]),
    [["app", "auto-switch", "work", "ok"]],
  );
  advance(after.now + DECIDE_EVERY_SECONDS);
  assert.equal(
    autoStandingSaid(model.get().autoStanding!),
    `Watching work: 40% of its weekly limit, as of ${hm(model.get().readAt)}.`,
  );
});

test("a switch is the app's change: the notice counts down, the log and every figure follow", () => {
  reset();
  use("claude", "work");
  const s = model.get();
  assert.deepEqual(s.lastSwitch.claude, {
    from: "personal",
    to: "work",
    at: NOW,
    followsAt: NOW + ADOPTION_CEILING_SECONDS,
  });
  assert.equal(countdown(s.lastSwitch.claude!.followsAt!, s.now), "0:33");
  assert.equal(countdown(NOW + 2 * HOUR + 5, NOW), "2:00:05");
  assert.equal(countdown(NOW - 5, NOW), "0:00");
  assert.deepEqual(
    s.log.slice(-1).map((e) => [e.caller, e.verb, e.subject, e.outcome]),
    [["app", "use", "work", "ok"]],
  );
  assert.equal(menuBarTitle(s), "work 40%");
  assert.equal(use("claude", "work", "cli"), "already_active");
  assert.equal(use("codex", "nobody", "cli"), "account_unknown");
  assert.deepEqual(
    model
      .get()
      .log.slice(-2)
      .map((e) => [e.caller, e.subject, e.outcome]),
    [
      ["cli", "work", "already_active"],
      ["cli", "codex/nobody", "account_unknown"],
    ],
  );
});

test("a name is one word without a slash, and a tool's two accounts never share one", () => {
  const label = core("label.rs");
  assert.ok(label.includes("if name.is_empty() {"));
  assert.ok(label.includes("name.chars().any(|c| c.is_whitespace() || c.is_control())"));
  assert.ok(label.includes("} else if name.contains(SEPARATOR) {"));
  assert.deepEqual(["", "a b", "tab\there", "a/b", "<b>x&y"].map(nameProblem), [
    "empty",
    "not_one_word",
    "not_one_word",
    "separator",
    null,
  ]);
  reset();
  assert.equal(rename("claude", "work", "personal"), "label_taken");
  assert.equal(rename("claude", "work", "a b"), "invalid_label");
  assert.equal(rename("claude", "personal", "<b>x&y"), "ok");
  const s = model.get();
  assert.equal(s.inUse.claude, "<b>x&y", "the account in use keeps being in use under its name");
  assert.ok(plainText(statusLine(s)).startsWith("<b>x&y "));
  assert.equal(rename("codex", "main", "work"), "ok", "another tool's name is free");
});

test("forgetting is refused for the account in use, and takes the rest out of every list", () => {
  reset();
  assert.equal(forget("claude", "personal"), "cannot_forget_active_account");
  assert.equal(forget("claude", "work"), "ok");
  const s = model.get();
  assert.ok(!s.accounts.some((a) => a.label === "work" && a.tool === "claude"));
  assert.deepEqual(s.windowsOpen, []);
  assert.deepEqual(
    s.log.slice(-2).map((e) => [e.verb, e.subject, e.outcome]),
    [
      ["forget", "personal", "cannot_forget_active_account"],
      ["forget", "work", "ok"],
    ],
  );
});

test("a simulated enrolment reads nothing until the next read, which finds nothing used", () => {
  reset();
  assert.equal(enroll("claude", "side", "app"), "ok");
  let side = model.get().accounts.find((a) => a.label === "side")!;
  assert.equal(side.email, "side@example.com");
  assert.equal(side.plan, "Max 20x");
  assert.equal(menuLine(side, model.get()), "side@example.com");
  advance(model.get().readAt + READ_EVERY_SECONDS);
  side = model.get().accounts.find((a) => a.label === "side")!;
  assert.deepEqual(
    side.limits.map((l) => limitRow(l, model.get())).map((r) => [r.short, r.figure, r.resets]),
    [
      ["5h", "0%", ""],
      ["week", "0%", "resets in 7d 0h"],
    ],
  );
  assert.equal(enroll("desktop", "studio"), "ok");
  assert.equal(model.get().inUse.desktop, "studio", "Claude quits signed in to it");
  assert.equal(enroll("claude", "side"), "label_taken");
});

test("an added Codex account says no lifetime for its parked login, as the core gives it none", () => {
  reset();
  assert.ok(
    core("provider/codex/engine.rs").includes(
      'access_token"]\n                .as_str()\n                .and_then(jwt::expires_at),\n            refresh_expires_at: None,',
    ),
  );
  assert.equal(enroll("codex", "extra"), "ok");
  const extra = model.get().accounts.find((a) => a.tool === "codex" && a.label === "extra")!;
  assert.equal(extra.refreshExpiresAt, undefined);
  assert.deepEqual(notes(extra, model.get(), null), []);
  assert.equal(enroll("claude", "side"), "ok");
  const side = model.get().accounts.find((a) => a.label === "side")!;
  assert.equal(side.refreshExpiresAt, model.get().now + SIGN_IN_LASTS);
});

test("signing in again gives a parked login another 30 days, said in whole days a second on", () => {
  reset();
  signInAgain("claude", "work");
  advance(model.get().now + 1);
  const s = model.get();
  const work = s.accounts.find((a) => a.label === "work")!;
  assert.ok(notes(work, s, null).includes("Parked login good for 29 more days"));
  signInAgain("claude", "personal");
  assert.equal(model.get().newLogin.claude?.label, "personal");
});

test("warm-ups start a period as the last one resets, within the hours set", () => {
  const decided = core("duty/warm/decide.rs");
  assert.ok(decided.includes("if range.holds(until, zone) {\n            Some(until)"));
  assert.ok(decided.includes("if asked == Asked::Now {\n        return Standing::Due;"));
  reset();
  const s = model.get();
  const personal = s.accounts.find((a) => a.label === "personal")!;
  assert.equal(warmNote(s, personal), "Warms up again at 22:00");
  assert.equal(nextWarmUp(s), "Next warm-up: personal at 22:00.");
  assert.equal(warmUp("personal"), false, "Warm Up Now waits while a period runs");
  advance(clock.warmsAgain + DECIDE_EVERY_SECONDS);
  const after = model.get();
  assert.deepEqual(
    after.log.filter((e) => e.verb === "warm-up").map((e) => [hm(e.at), e.caller, e.subject]),
    [
      ["17:00", "app", "personal"],
      ["22:00", "app", "personal"],
    ],
  );
  const now = after.accounts.find((a) => a.label === "personal")!;
  assert.deepEqual(warmStanding(after, now, "by-itself"), {
    stands: "running",
    until: clock.warmsAgain + PERIOD,
    next: clock.warmsAgain + 9 * HOUR,
  });
  assert.equal(
    warmNote(after, now),
    "Has its 5-hour period running until 03:00, and warms up again at 07:00",
  );
});

test("a share is clamped to 50 to 99, and every change of it is a choice made", () => {
  assert.deepEqual([0, 49.4, 50, 77.6, 99, 120].map(clamped), [50, 50, 50, 78, 99, 99]);
  const machine = ffi("present/machine.rs").replace(/\\\n\s*/g, "");
  assert.ok(
    machine.includes("once a limit of the account in use reaches {at}%, while the app is open."),
  );
  assert.equal(
    atLimitNote({ atLimit: "notify", chosen: true, share: 90 }),
    "Pitboard notifies you once a limit of the account in use reaches 90%, while the app is open.",
  );
  assert.equal(atLimitNote({ atLimit: "nothing", chosen: true, share: 90 }), null);
});

test("Quit stops the app's reads and notifications, and Reset brings every figure back", () => {
  reset();
  quit();
  setShare(50);
  advance(NOW + HOUR);
  assert.deepEqual(model.get().posted, []);
  assert.equal(model.get().readAt, clock.read);
  reset();
  assert.deepEqual(model.get(), start());
});

test("a view hears each action once", () => {
  reset();
  let heard = 0;
  const stop = model.listen(() => heard++);
  use("claude", "work");
  setChoice("switch");
  stop();
  assert.equal(heard, 2);
});

// Everything the model puts into words, across every change a visitor can make: what the app
// says, and what a terminal prints, which is set in the monospaced face alone.
function everything(): { said: string[]; printed: string[] } {
  const said = new Set<string>();
  const printed = new Set<string>();
  const hear = (s: State) => {
    for (const p of s.posted) [p.title, p.subtitle ?? "", p.body].forEach((t) => said.add(t));
    if (s.autoStanding) said.add(autoStandingSaid(s.autoStanding));
    const note = atLimitNote(s.preferences);
    if (note) said.add(note);
    const next = nextWarmUp(s);
    if (next) said.add(next);
    said.add(menuBarTitle(s));
    trayTip(s)
      .split("\n")
      .forEach((t) => said.add(t));
    for (const a of s.accounts) {
      said.add(menuLine(a, s));
      notes(a, s, warmNote(s, a)).forEach((n) => said.add(n));
      for (const l of a.limits) {
        const r = limitRow(l, s);
        [r.short, r.figure, r.resets, r.pace?.said ?? ""].forEach((t) => said.add(t));
      }
    }
    for (const tool of ["claude", "codex", "desktop"] as const)
      statusLines(s, tool).forEach((l) => printed.add(plainText(l)));
    printed.add(plainText(statusLine(s)));
    for (const e of s.log) [e.verb, e.subject, e.outcome].forEach((t) => printed.add(t));
  };
  const scenes: (() => void)[] = [
    () => advance(clock.warmsAgain + DAY),
    () => {
      setChoice("switch");
      advance(NOW + 2 * HOUR);
    },
    () => {
      use("claude", "work");
      use("claude", "personal");
      setChoice("switch");
      setShare(50);
      advance(NOW + 10 * MINUTE);
    },
    () => {
      enroll("claude", "side");
      use("claude", "side");
      setChoice("switch");
      advance(NOW + 6 * MINUTE);
    },
    () => {
      setChoice("switch");
      reading({ "claude/work": [session(96), weekly(96)] });
      setShare(60);
    },
    () => {
      reading({ "claude/personal": [session(100), weekly(20)] });
      refresh();
    },
    () => {
      reading({ "codex/main": [weekly(97)] });
      refresh();
      use("codex", "spare");
    },
    () => {
      rename("claude", "work", "job");
      forget("codex", "spare");
      enroll("codex", "extra");
      enroll("desktop", "studio");
      signInAgain("desktop", "office");
      signInAgain("claude", "personal");
      warmUp("job");
      setChoice("nothing");
      setShare(99);
    },
  ];
  for (const scene of scenes) {
    reset();
    scene();
    hear(model.get());
  }
  // and each of the word functions in every variant, whether a scene reaches it or not
  const limit = session(97, NOW + HOUR);
  const standings: WarmStanding[] = [
    { stands: "due" },
    { stands: "running", until: NOW + HOUR, next: NOW + HOUR },
    { stands: "running", until: NOW + HOUR, next: null },
    { stands: "running", until: NOW + HOUR, next: NOW + 9 * HOUR },
    { stands: "resting", at: NOW + 10 * HOUR },
    { stands: "held", why: "no_reading" },
    { stands: "held", why: "nothing_to_start" },
    { stands: "held", why: "parked_login_expired" },
    { stands: "held", why: "limit_used_up", limit },
    { stands: "held", why: "limit_used_up", limit: { ...limit, resetsAt: null } },
  ];
  standings.forEach((w) => said.add(capitalised(warmSaid(w))));
  const skips: Skip[] = [
    { code: "no_room", unread: [] },
    { code: "no_room", unread: ["side", "codex/extra"] },
    { code: "already_switched" },
    { code: "settling", until: NOW + MINUTE },
  ];
  for (const why of skips) {
    const used = shareOfLimit(limit);
    said.add(
      autoStandingSaid({
        came: "skipped",
        from: "personal",
        used,
        why: notSwitching(why, 95),
        until: why.code === "settling" ? why.until : null,
      }),
    );
  }
  said.add(autoStandingSaid({ came: "watching", account: "work", used: null, asOf: null }));
  said.add(
    autoStandingSaid({
      came: "not-watching",
      why: "there is no reading of side from Anthropic yet",
    }),
  );
  for (const atLimit of ["nothing", "notify", "switch"] as const)
    said.add(atLimitNote({ atLimit, chosen: true, share: 95 }) ?? "");
  for (const tool of ["claude", "codex", "desktop"] as const)
    said.add(afterSwitch(tool, "Claude Code sessions"));
  [0, 33, HOUR + 5].forEach((left) => said.add(countdown(NOW + left, NOW)));
  said.delete("");
  printed.delete("");
  return { said: [...said], printed: [...printed] };
}

test("everything the model says keeps the site's rules: no dash, a committed face, no banned word", () => {
  const face = (name: string) =>
    new Set<number>(
      JSON.parse(
        readFileSync(new URL(`../assets/fonts/${name}.codepoints.json`, import.meta.url), "utf8"),
      ),
    );
  const text = face("Inter-4.1-opsz-wght");
  const mono = face("JetBrainsMono-2.304-wght");
  const banned = wordPattern([...bannedFromDocs(repo("docs/AGENTS.md")), ...SITE_BANNED]);
  const source = ["crates/pitboard-core/src/words.rs", "crates/pitboard-ffi/src/present/words.rs"]
    .map(repo)
    .join("\n");
  const { said, printed } = everything();
  assert.ok(said.length > 100, `${said.length} strings`);
  for (const [t, faces] of [
    ...said.map((t): [string, Set<number>[]] => [t, [text, mono]]),
    ...printed.map((t): [string, Set<number>[]] => [t, [mono]]),
  ]) {
    assert.doesNotMatch(t, /[–—]/, t);
    // the guard keeps a program's own words, so a banned word is only ever one the core says
    for (const m of t.matchAll(banned)) {
      const phrase = `${t.slice(0, m.index).trim().split(/\s+/).at(-1)} ${m[0]}`;
      assert.ok(source.includes(phrase), `"${m[0]}" in "${t}"`);
    }
    for (const ch of t) {
      const cp = ch.codePointAt(0)!;
      if (cp >= 0x20 && ch !== " ")
        assert.ok(
          faces.every((f) => f.has(cp)),
          `U+${cp.toString(16)} "${ch}" in "${t}"`,
        );
    }
  }
});

test("anything done on the page marks it changed until Reset, and time passing does not", () => {
  reset();
  advance(NOW + READ_EVERY_SECONDS + DECIDE_EVERY_SECONDS);
  assert.equal(model.get().changed, false, "a read and a look are the app's own");
  setShare(90);
  assert.equal(model.get().changed, true);
  reset();
  assert.equal(model.get().changed, false);
});
