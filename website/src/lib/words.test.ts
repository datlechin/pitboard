import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  accountSpoken,
  appSpan,
  figure,
  hm,
  level,
  limitColumn,
  limitName,
  limitNotification,
  limitRow,
  menuBarClock,
  menuBarSpoken,
  menuLine,
  rowLines,
  warmsAgainNote,
  paceColumn,
  paceOf,
  parkedLifeColumn,
  RENEW_WITHIN,
  MARGIN,
  TOO_EARLY,
  resets,
  runsOut,
  runsOutFirst,
  runsOutNote,
  sessionLifeColumn,
  shortDate,
  signInExpires,
  span,
  spokenLimit,
  staleNote,
  statusLine,
  statusLines,
  switchSpoken,
  toolsOn,
  headsUp,
  whole,
} from "./words.ts";
import { plainText } from "./changelog.ts";
import {
  DAY,
  HOUR,
  MINUTE,
  WEEK,
  account,
  accounts,
  clock,
  moments,
  share,
  type Limit,
  type World,
} from "../data/world.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const core = (path: string) => repo(`crates/pitboard-core/src/${path}`);
const provider = core("provider/mod.rs");

// A Rust test's body, which has to still be there for the port to answer to it.
function rustTest(source: string, name: string): string {
  const start = source.indexOf(`fn ${name}()`);
  assert.ok(start >= 0, `the core still has ${name}`);
  const body = source.slice(start);
  const end = body.indexOf("#[test]");
  return (end < 0 ? body : body.slice(0, end)).replace(/\s+/g, " ");
}

// An integer as the Rust tests write one: `2 * 86_400 - 1`, `25 * DAY + 3 * HOUR`.
const NOW = 1_789_935_000;
const named = { DAY, HOUR, MINUTE, WEEK, RENEW_WITHIN, NOW };
function value(expression: string): number {
  const clean = expression.replace(/(\d)_(?=\d)/g, "$1").trim();
  assert.match(clean, /^[\w\s+\-*/()]+$/, expression);
  return Function(...Object.keys(named), `return (${clean});`)(...Object.values(named));
}

// Each `assert_eq!(call(…), "…")` of a test body, its argument and the words it expects.
function cases(body: string, call: string): [number, string][] {
  const found = [
    ...body.matchAll(
      new RegExp(
        `assert_eq!\\( ?${call}\\(([^()]*)\\)(?:\\.as_deref\\(\\))?, (?:Some\\()?"([^"]*)"\\)? ?\\);`,
        "g",
      ),
    ),
  ].map((m): [number, string] => [value(m[1]), m[2]]);
  assert.ok(found.length >= 4, `${call} has its cases`);
  return found;
}

const said = core("words.rs");

test("each system lists the tools Pitboard supports on it, Claude Desktop on macOS alone", () => {
  assert.ok(provider.includes("(ProviderId::Claude | ProviderId::Codex, _) => true,"));
  assert.ok(provider.includes("(ProviderId::Desktop, Os::MacOs) => true,"));
  assert.ok(provider.includes("(ProviderId::Desktop, Os::Linux | Os::Windows) => false,"));
  const listed = (p: Parameters<typeof toolsOn>[0]) => toolsOn(p).map(([tool]) => tool);
  assert.deepEqual(listed("macos"), ["claude", "codex", "desktop"]);
  assert.deepEqual(listed("windows"), ["claude", "codex"]);
  assert.deepEqual(listed("linux"), ["claude", "codex"]);
});

test("a figure's level steps where the core's usage_level does, at 70% and 90% as drawn", () => {
  assert.ok(said.includes("        90.. => UsageLevel::Out,\n        70.. => UsageLevel::Low,"));
  assert.ok(said.includes("    match whole(percent) {"));
  assert.deepEqual([0, 69.4, 69.5, 89.4, 89.5, 100, 130].map(level), [
    "g",
    "g",
    "y",
    "y",
    "r",
    "r",
    "r",
  ]);
});

test("a figure is rounded as Rust rounds, a half away from zero", () => {
  assert.ok(
    core("usage/mod.rs").includes(
      "pub fn whole(percent: f64) -> i64 {\n    percent.round() as i64",
    ),
  );
  assert.deepEqual([17.5, -17.5, 0.4, -0.5, 59].map(whole), [18, -18, 0, -1, 59]);
});

test("spans read as words.rs's spans_read_at_a_glance says", () => {
  for (const [seconds, words] of cases(rustTest(said, "spans_read_at_a_glance"), "span"))
    assert.equal(span(seconds), words, String(seconds));
});

test("resets read as words.rs's a_reset_is_said_in_the_largest_units_that_fit says", () => {
  const body = rustTest(said, "a_reset_is_said_in_the_largest_units_that_fit");
  assert.ok(body.includes("let resets = |left: i64| resets(NOW + left, NOW);"));
  for (const [left, words] of cases(body, "resets")) assert.equal(resets(NOW + left, NOW), words);
});

test("a limit is named for its length, as words.rs's a_limit_is_named_for_its_length says", () => {
  const body = rustTest(said, "a_limit_is_named_for_its_length");
  const rows = [...body.matchAll(/\(([\d_]+), "([^"]+)", "([^"]+)"\)/g)];
  assert.ok(rows.length >= 8);
  for (const [, length, column, sentence] of rows) {
    const limit = { percent: 0, resetsAt: NOW, lengthSeconds: value(length) };
    assert.equal(limitColumn(limit), column, length);
    assert.equal(limitName(limit), sentence, length);
  }
});

test("a pace is said as words.rs's a_pace_is_said_as_over_under_or_on says", () => {
  const body = rustTest(said, "a_pace_is_said_as_over_under_or_on");
  const rows = [...body.matchAll(/pace_column\(&pace\((-?[\d.]+), Standing::(\w+)[^"]*"([^"]+)"/g)];
  assert.equal(rows.length, 4);
  for (const [, delta, standing, words] of rows) {
    const p = { expected: 40, delta: Number(delta), standing: standing.toLowerCase() as "over" };
    assert.equal(paceColumn(p), words);
  }
});

test("a limit running out is said as words.rs's a_limit_running_out_is_named_with_when says", () => {
  const body = rustTest(said, "a_limit_running_out_is_named_with_when");
  const rows = [...body.matchAll(/runs_out\("([^"]+)", ([^)]+)\), "([^"]+)"/g)];
  assert.equal(rows.length, 3);
  for (const [, limit, seconds, words] of rows) assert.equal(runsOut(limit, value(seconds)), words);
});

test("a parked login's life in a column reads as words.rs's a_parked_logins_life_in_a_column_says_when_it_expires says", () => {
  const column = rustTest(said, "a_parked_logins_life_in_a_column_says_when_it_expires");
  for (const [left, words] of cases(column, "life"))
    assert.equal(parkedLifeColumn(NOW + left, NOW), words);
  assert.ok(core("state.rs").includes("pub(crate) const RENEW_WITHIN: i64 = 3 * 86_400;"));
  assert.equal(RENEW_WITHIN, 3 * DAY);
});

test("a Claude Desktop sign-in's life in a column is hedged, as words.rs's a_session_life_is_hedged_and_never_says_expired says", () => {
  const body = rustTest(said, "a_session_life_is_hedged_and_never_says_expired");
  for (const [left, words] of cases(body, "column"))
    assert.equal(sessionLifeColumn(NOW + left, NOW), words);
  for (const left of [-DAY, 0, 30, DAY, 40 * DAY]) {
    const column = sessionLifeColumn(NOW + left, NOW);
    assert.ok(!column.includes("expire"), column);
  }
});

const pace = core("usage/pace.rs");
// pace.rs's own helper: a window of `length` with `gone` of it gone by, read now
const paced = (used: number, gone: number, length: number) =>
  paceOf(used, NOW + length - gone, length, NOW, NOW);

test("pace.rs's margin and early part are the port's", () => {
  assert.ok(pace.includes("pub(crate) const MARGIN: f64 = 5.0;"));
  assert.ok(pace.includes("pub(crate) const TOO_EARLY: f64 = 0.03;"));
  assert.equal(MARGIN, 5);
  assert.equal(TOO_EARLY, 0.03);
});

test("a_limit_is_measured_against_the_part_of_its_window_gone_by", () => {
  rustTest(pace, "a_limit_is_measured_against_the_part_of_its_window_gone_by");
  const half = paced(50, WEEK / 2, WEEK)!;
  assert.ok(Math.abs(half.expected - 50) < 1e-9);
  assert.equal(half.standing, "even");
  const busy = paced(33, 10 * HOUR + 18 * 60, WEEK)!;
  assert.ok(Math.abs(busy.expected - 6.13) < 0.01);
  assert.equal(busy.standing, "over");
  assert.equal(paced(10, WEEK / 2, WEEK)!.standing, "under");
  assert.equal(paced(54.9, WEEK / 2, WEEK)!.standing, "even");
  assert.equal(paced(44.9, WEEK / 2, WEEK)!.standing, "under");
});

test("a_limit_faster_than_even_says_when_it_runs_out", () => {
  rustTest(pace, "a_limit_faster_than_even_says_when_it_runs_out");
  const gone = 10 * HOUR;
  assert.equal(paced(33, gone, WEEK)!.runsOutIn, Math.trunc((67 * gone) / 33));
  assert.ok(paced(55.1, (5 * HOUR) / 2, 5 * HOUR)!.runsOutIn! < (5 * HOUR) / 2);
});

test("a_pace_is_said_only_where_it_means_something", () => {
  rustTest(pace, "a_pace_is_said_only_where_it_means_something");
  assert.equal(paceOf(40, NOW + DAY, 0, NOW, NOW), null);
  assert.equal(paceOf(40, NOW, WEEK, NOW, NOW), null);
  assert.equal(paceOf(40, NOW - 60, WEEK, NOW, NOW), null);
  assert.equal(paceOf(40, NOW + WEEK + 60, WEEK, NOW, NOW), null);
  assert.equal(paced(100, DAY, WEEK), null);
  assert.equal(paced(104, DAY, WEEK), null);
  const early = Math.trunc(TOO_EARLY * 5 * HOUR);
  assert.equal(paced(20, early - 1, 5 * HOUR), null);
  assert.notEqual(paced(20, early + 1, 5 * HOUR), null);
});

test("a_readings_pace_is_its_own_and_only_counts_down", () => {
  rustTest(pace, "a_readings_pace_is_its_own_and_only_counts_down");
  const read = NOW - HOUR;
  const resetsAt = NOW + WEEK - 11 * HOUR;
  const then = paceOf(33, resetsAt, WEEK, read, read)!;
  const later = paceOf(33, resetsAt, WEEK, read, NOW)!;
  assert.equal(later.expected, then.expected);
  assert.equal(later.runsOutIn, then.runsOutIn! - HOUR);
  assert.equal(paceOf(90, NOW + 3 * DAY, WEEK, NOW - 3 * DAY, NOW)!.runsOutIn, 0);
  assert.equal(paceOf(33, NOW - 1, WEEK, NOW - DAY, NOW), null);
  assert.deepEqual(
    paceOf(33, resetsAt, WEEK, NOW + HOUR, NOW),
    paceOf(33, resetsAt, WEEK, NOW, NOW),
  );
});

test("an_untouched_limit_is_under_pace", () => {
  rustTest(pace, "an_untouched_limit_is_under_pace");
  assert.equal(paced(0, DAY, WEEK)!.standing, "under");
  assert.equal(paced(-1, DAY, WEEK)!.standing, "under");
});

test("the world's times give every word the figures were drawn with", () => {
  const before = moments.before;
  // the app's bars, each reset at a clock time
  const row = (label: string, tool: "claude" | "codex" = "claude") =>
    account(label, tool).limits.map((l) => {
      const r = limitRow(l, before);
      return [r.short, r.figure, r.resets];
    });
  assert.deepEqual(row("personal"), [
    ["5-hour", "59%", "Resets 22:00"],
    ["Weekly", "73%", "Resets Mon 14:50"],
  ]);
  assert.deepEqual(row("work"), [
    ["5-hour", "12%", "Resets 23:50"],
    ["Weekly", "40%", "Resets Fri 01:00"],
  ]);
  assert.deepEqual(row("main", "codex"), [["Weekly", "7%", "Resets Mon 15:00"]]);
  assert.deepEqual(row("spare", "codex"), [["5-hour", "0%", "Resets Wed 01:50"]]);
  // the terminal's columns, with their pace
  const columns = (label: string, tool: "claude" | "codex" = "claude") =>
    account(label, tool).limits.map((l) => {
      const p = paceOf(l.percent, l.resetsAt, l.lengthSeconds, before.readAt, before.now);
      return [
        limitColumn(l),
        figure(l.percent),
        resets(l.resetsAt!, before.now),
        p ? paceColumn(p) : "",
        p && whole(p.expected),
      ];
    });
  assert.deepEqual(columns("personal"), [
    ["5h", "59%", "resets in 1h 10m", "18% under pace", 77],
    ["week", "73%", "resets in 5d 18h", "55% over pace", 18],
  ]);
  assert.deepEqual(columns("work"), [
    ["5h", "12%", "resets in 3h 00m", "28% under pace", 40],
    ["week", "40%", "resets in 2d 4h", "29% under pace", 69],
  ]);
  assert.deepEqual(columns("main", "codex"), [
    ["week", "7%", "resets in 5d 18h", "11% under pace", 18],
  ]);
  assert.deepEqual(columns("spare", "codex"), [["5h", "0%", "resets in 5h 00m", "", null]]);
  assert.equal(
    runsOutNote(account("personal"), before),
    "weekly limit runs out in 11h 05m at this pace",
  );
  assert.equal(menuLine(account("personal"), before), "5-hour 59% used, Weekly 73%");
  assert.equal(menuLine(account("personal"), before, "windows"), "5-hour 59% used, weekly 73%");
  assert.equal(runsOutFirst(account("personal"), moments.after), null);
  const all = (w: World) => accounts.map((a) => rowLines(a, w, warmsAgainNote(a)));
  assert.deepEqual(all(before), [
    ["Warms up again at 22:00"],
    [],
    [],
    [],
    ["Read 2 hr 5 min ago, with Claude closed"],
    [],
  ]);
  const column = (label: string) =>
    plainText(statusLines(before, label === "office" ? "desktop" : "claude").join("\n"));
  assert.match(column("work"), /ready · good for 26d 4h/);
  assert.match(column("office"), /signed in · read 2h 05m ago/);
  assert.match(column("office"), /ready · sign-in may lapse in 25d 3h/);
  assert.deepEqual(
    [menuBarClock(clock.menuBar), menuBarClock(clock.warned), hm(clock.read)],
    ["Tue 13 Oct 20:51", "Wed 14 Oct 05:53", "20:50"],
  );
  assert.deepEqual(
    [hm(clock.menuBar), shortDate(clock.menuBar), hm(clock.warned), shortDate(clock.warned)],
    ["20:51", "13/10/2026", "05:53", "14/10/2026"],
  );
});

test("the notification comes when personal's weekly limit, at its pace, reaches the share", () => {
  const week = account("personal").limits.find((l) => l.lengthSeconds === WEEK)!;
  const runs = runsOutFirst(account("personal"), moments.before)!;
  assert.equal(runs.limit, week);
  // from the menu bar's clock, the part of what is left that takes it to the share
  const left = ((share - week.percent) / (100 - week.percent)) * runs.inSeconds;
  const minute = (epoch: number) => Math.floor(epoch / MINUTE);
  assert.equal(minute(clock.menuBar + left), minute(clock.warned));
  assert.deepEqual(headsUp.title, limitNotification(week, false));
  assert.equal(headsUp.body, `personal has used ${share}%. work has used 40%.`);
  const ffi = repo("crates/pitboard-ffi/src/present/words.rs");
  assert.ok(ffi.includes('assert_eq!(has_used_share("work", "95%"), "work has used 95%.");'));
  assert.ok(ffi.includes('assert_eq!(other_used("spare", Some(80)), "spare has used 20%.");'));
});

test("a label, email or plan a visitor types stays text in every line words.ts writes as HTML", () => {
  const typed = ["<b>x&y", "<style>*{color:red}", '"'];
  const [label, email, plan] = typed;
  const w: World = {
    ...moments.after,
    accounts: accounts.map((a) =>
      a.tool === "claude" && a.label === "work" ? { ...a, label, email, plan } : a,
    ),
    inUse: { ...moments.after.inUse, claude: label },
  };
  const html = [...statusLines(w, "claude"), statusLine(w)];
  // past the lines' own markup, nothing is left that could open a tag or close an attribute
  for (const line of html)
    assert.doesNotMatch(line.replace(/<\/?b>|<span class="[^"<>]*">|<\/span>/g, ""), /[<>"]/);
  const text = plainText(html.join("\n"));
  for (const t of typed) assert.ok(text.includes(t), t);
  for (const t of [label, email]) assert.ok(!html.join("\n").includes(t), t);
});

const ffiWords = repo("crates/pitboard-ffi/src/present/words.rs");
const ffiAccounts = repo("crates/pitboard-ffi/src/present/accounts.rs");

// Each `assert_eq!(call(args), "…")` of a test body, its arguments as written and the words.
const calls = (body: string, call: string) =>
  [
    ...body.matchAll(
      new RegExp(
        `assert_eq!\\( ?${call}\\(([^()]*(?:\\([^()]*\\)[^()]*)*)\\), "([^"]*)" ?\\);`,
        "g",
      ),
    ),
  ].map((m) => [m[1], m[2]] as const);

test("a reset counts down and a read says when as present/words.rs's tests of its span say", () => {
  const body = rustTest(ffiWords, "a_reset_is_counted_down_in_hours_and_minutes");
  const counted = [...body.matchAll(/\(([\d_ *+]+), "in ([^"]+)"\)/g)];
  assert.ok(counted.length >= 10, `${counted.length} spans`);
  for (const [, seconds, words] of counted) assert.equal(appSpan(value(seconds)), words, seconds);
  const read = rustTest(ffiWords, "an_account_is_said_in_these_words");
  assert.ok(read.includes('"Read 2 hr 5 min ago, with Claude closed"'));
  assert.ok(read.includes('"Read under a minute ago, with Claude closed"'));
  assert.equal(
    staleNote(account("home", "desktop"), moments.before),
    "Read 2 hr 5 min ago, with Claude closed",
  );
});

test("a limit is spoken as present/words.rs's a_limit_is_spoken_as_one_sentence says", () => {
  const body = rustTest(ffiWords, "a_limit_is_spoken_as_one_sentence");
  const spoken: [Parameters<typeof spokenLimit>, string][] = [
    [["5-hour limit", 42, "Resets 3:45 PM"], "5-hour limit, 42 percent used, resets 3:45 PM"],
    [["30-minute limit", 12, ""], "30-minute limit, 12 percent used"],
    [
      ["Weekly limit (Fable)", 98, "Resetting now"],
      "Weekly limit (Fable), 98 percent used, resetting now",
    ],
  ];
  for (const [args, words] of spoken) {
    assert.ok(body.includes(`"${words}"`), `the app says "${words}"`);
    assert.equal(spokenLimit(...args), words);
  }
  assert.equal(
    limitRow(account("personal").limits[1], moments.before).spoken,
    "Weekly limit, 73 percent used, resets Mon 14:50",
  );
});

test("a limit's names, reset and menu line are present/words.rs's an_account_is_said_in_these_words", () => {
  const body = rustTest(ffiWords, "an_account_is_said_in_these_words");
  for (const words of [
    "5-hour limit",
    "Weekly limit (Fable)",
    "Resets 3:45 PM",
    "Resets Mon 9:00 AM",
    "Resetting now",
    "5-hour limit reached, resets 6:46 PM",
    "Weekly limit reached",
    "5-hour 42% used, Weekly 12%, Weekly (Fable) 3%",
    "5-hour 42% used, weekly 12%, weekly (Fable) 3%",
    "Weekly 12% used",
  ])
    assert.ok(body.includes(`"${words}"`), words);
  const at = (lengthSeconds: number, percent: number, scope?: string): Limit => ({
    percent,
    resetsAt: NOW + HOUR,
    lengthSeconds,
    scope,
  });
  const three = {
    ...account("personal"),
    limits: [at(5 * HOUR, 42), at(WEEK, 12), at(WEEK, 3, "Fable")],
  };
  const w: World = { ...moments.before, now: NOW, accounts: [three] };
  assert.equal(menuLine(three, w), "5-hour 42% used, Weekly 12%, Weekly (Fable) 3%");
  assert.equal(menuLine(three, w, "windows"), "5-hour 42% used, weekly 12%, weekly (Fable) 3%");
  const weekly = { ...three, limits: [at(WEEK, 12)] };
  assert.equal(menuLine(weekly, w, "windows"), "Weekly 12% used");
  const accountsRs = repo("crates/pitboard-ffi/src/present/accounts.rs");
  assert.ok(accountsRs.includes("return words::limit_reached(&title, clock.as_deref());"));
  const reached = { ...three, limits: [at(5 * HOUR, 100), at(WEEK, 40)] };
  assert.equal(menuLine(reached, w), `5-hour limit reached, resets ${hm(NOW + HOUR)}`);
  assert.equal(
    menuLine(
      { ...reached, refreshExpiresAt: NOW - 1 },
      { ...w, inUse: { ...w.inUse, claude: "work" } },
    ),
    "Sign-in expired",
  );
});

test("a saved sign-in says when it expires, as present/words.rs's a_saved_sign_in_says_when_it_expires says", () => {
  const body = rustTest(ffiWords, "a_saved_sign_in_says_when_it_expires");
  const found = calls(body, "sign_in_expires");
  assert.ok(found.length >= 6, `${found.length} cases`);
  for (const [args, words] of found) {
    const [left, lapses] = args.split(/,\s*/);
    assert.equal(signInExpires(value(left), lapses === "true"), words, args);
  }
});

test("a notification is titled as present/words.rs's a_notification_is_titled_as_each_system_titles_one says", () => {
  const body = rustTest(ffiWords, "a_notification_is_titled_as_each_system_titles_one")
    .replace(/\( /g, "(")
    .replace(/ \)/g, ")");
  const at = (lengthSeconds: number, scope?: string): Limit => ({
    percent: 95,
    resetsAt: null,
    lengthSeconds,
    scope,
  });
  for (const [limit, reached] of [
    [at(5 * HOUR), false],
    [at(WEEK, "Fable"), true],
  ] as const) {
    const titled = limitNotification(limit, reached);
    assert.ok(body.includes(`pair("${titled.macos}", "${titled.windows}")`), titled.macos);
  }
  assert.deepEqual(limitNotification(at(WEEK), false), {
    macos: "Weekly Limit Almost Reached",
    windows: "Weekly limit almost reached",
  });
});

test("the menu bar item, a row and its button are spoken as present/words.rs says them", () => {
  for (const line of [
    "let mut said = vec![PITBOARD];",
    "said.push(shown);",
    "said.extend(tool);",
    "let mut said = vec![name.to_owned()];",
    "said.extend(plan.map(str::to_owned));",
    'said.push("in use".into());',
    "said.extend(sign_in.map(uncapitalised));",
    "Some(name) => switch_to(name),",
  ])
    assert.ok(ffiWords.includes(line), line);
  for (const call of ["words::bar_spoken(", "words::row_spoken(", "words::switch_account(Some("])
    assert.ok(ffiAccounts.includes(call), call);
  assert.equal(menuBarSpoken(moments.before), "Pitboard, personal 73%, Claude Code");
  assert.equal(menuBarSpoken(moments.after), "Pitboard, work 40%, Claude Code");
  assert.equal(accountSpoken(account("work"), moments.after), "work, Max 20x, in use");
  assert.equal(accountSpoken(account("home", "desktop"), moments.after), "home, in use");
  assert.equal(accountSpoken(account("spare", "codex"), moments.after), "spare, Plus");
  const lapsed = { ...moments.after, now: account("personal").refreshExpiresAt! };
  assert.equal(accountSpoken(account("personal"), lapsed), "personal, Max 20x, sign-in expired");
  assert.equal(switchSpoken(account("personal")), "Switch to personal");
});
