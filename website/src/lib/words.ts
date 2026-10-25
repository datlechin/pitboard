// The app's and the command line's words for world.ts, by crates/pitboard-core/src/words.rs,
// crates/pitboard-ffi/src/present and crates/pitboard/src/render/status.rs.

import {
  DAY,
  HOUR,
  MINUTE,
  WEEK,
  ZONE,
  account,
  share,
  toolOn,
  tools,
  warmUp,
  type Account,
  type Limit,
  type Logged,
  type Tool,
  type World,
} from "../data/world.ts";
import type { Platform } from "./platform.ts";

// Every value of the world a line puts into HTML: a label someone types is text.
const entities: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" };
export const escaped = (text: string) => text.replace(/[&<>"]/g, (c) => entities[c]);

// usage/mod.rs `whole`: Rust's round takes a half away from zero.
export const whole = (percent: number) =>
  percent < 0 ? -Math.round(-percent) : Math.round(percent);

export const figure = (percent: number) => `${whole(percent)}%`;

// words.rs `span`: "6d 4h", "2h 05m", "47m", "<1m".
export function span(seconds: number): string {
  const s = Math.max(0, seconds);
  const days = Math.floor(s / DAY);
  const hours = Math.floor((s % DAY) / HOUR);
  const minutes = Math.floor((s % HOUR) / MINUTE);
  if (days === 0 && hours === 0) return minutes === 0 ? "<1m" : `${minutes}m`;
  if (days === 0) return `${hours}h ${String(minutes).padStart(2, "0")}m`;
  return `${days}d ${hours}h`;
}

// words.rs `limit_column`, `limit_name` and `scoped_limit_name`, by the length a limit runs.
function lengthColumn(s: number): string {
  if (s === WEEK) return "week";
  if (s === DAY) return "day";
  if (s % DAY === 0) return `${s / DAY}d`;
  if (s % HOUR === 0) return `${s / HOUR}h`;
  if (s % MINUTE === 0) return `${s / MINUTE}m`;
  return `${s}s`;
}

export const limitColumn = (limit: Limit) =>
  limit.scope
    ? `${lengthColumn(limit.lengthSeconds)} · ${limit.scope}`
    : lengthColumn(limit.lengthSeconds);

export function limitName(limit: Limit): string {
  const s = limit.lengthSeconds;
  if (s === WEEK) return "weekly";
  if (s === DAY) return "daily";
  if (s % DAY === 0) return `${s / DAY}-day`;
  if (s % HOUR === 0) return `${s / HOUR}-hour`;
  if (s % MINUTE === 0) return `${s / MINUTE}-minute`;
  return `${s}-second`;
}

export const scopedLimitName = (limit: Limit) =>
  limit.scope ? `${limitName(limit)} ${limit.scope}` : limitName(limit);

// words.rs `resets`
export const resets = (at: number, now: number) =>
  at <= now ? "resetting now" : `resets in ${span(at - now)}`;

// usage/pace.rs: more than MARGIN points either side of even counts, and nothing is said of a
// window less than TOO_EARLY in.
export const MARGIN = 5;
export const TOO_EARLY = 0.03;

export type Standing = "over" | "under" | "even";
export interface Pace {
  expected: number;
  delta: number;
  standing: Standing;
  // over pace: when it runs out, from the moment it is told
  runsOutIn?: number;
}

// pace.rs `of`
export function paceOf(
  used: number,
  resetsAt: number | null,
  length: number,
  readAt: number,
  now: number,
): Pace | null {
  if (!(length > 0) || resetsAt === null) return null;
  const at = Math.min(readAt, now);
  const left = resetsAt - at;
  if (resetsAt <= now || left > length || used >= 100) return null;
  const gone = length - left;
  const part = gone / length;
  if (part < TOO_EARLY) return null;
  const share = Math.max(used, 0);
  const expected = part * 100;
  const delta = share - expected;
  if (delta > MARGIN) {
    const fromReading = Math.trunc(((100 - share) * gone) / share);
    return { expected, delta, standing: "over", runsOutIn: Math.max(fromReading - (now - at), 0) };
  }
  return { expected, delta, standing: delta < -MARGIN ? "under" : "even" };
}

// A limit's pace as the world read it, told at its moment.
export const pace = (limit: Limit, w: World) =>
  paceOf(limit.percent, limit.resetsAt, limit.lengthSeconds, w.readAt, w.now);

// words.rs `pace_column`
export function paceColumn(p: Pace): string {
  if (p.standing === "even") return "on pace";
  return `${whole(Math.abs(p.delta))}% ${p.standing} pace`;
}

// words.rs `time_to_run_out` and `runs_out`; present/words.rs `runs_out_in` for the menu.
export const timeToRunOut = (inSeconds: number) => (inSeconds >= MINUTE ? span(inSeconds) : null);

export function runsOut(limit: string, inSeconds: number): string {
  const left = timeToRunOut(inSeconds);
  return left
    ? `${limit} limit runs out in ${left} at this pace`
    : `${limit} limit is about to run out`;
}

export function runsOutIn(inSeconds: number): string {
  const left = timeToRunOut(inSeconds);
  return left ? `runs out in ${left}` : "about to run out";
}

// state.rs `RENEW_WITHIN` and `renewal_due`
export const RENEW_WITHIN = 3 * DAY;
export const renewalDue = (refreshExpiresAt: number, now: number) => {
  const left = refreshExpiresAt - now;
  return left >= 1 && left < RENEW_WITHIN;
};

// words.rs `parked_life`, `parked_life_column`, `session_life` and `session_life_column`
export function parkedLife(refreshExpiresAt: number | undefined, now: number): string | null {
  if (refreshExpiresAt === undefined) return null;
  const left = refreshExpiresAt - now;
  if (left <= 0) return "Its parked login has expired";
  const days = Math.floor(left / DAY);
  if (days === 0) return "Parked login good for under a day";
  return days === 1
    ? "Parked login good for 1 more day"
    : `Parked login good for ${days} more days`;
}

export function parkedLifeColumn(refreshExpiresAt: number, now: number): string {
  const left = refreshExpiresAt - now;
  if (left <= 0) return "expired";
  if (renewalDue(refreshExpiresAt, now)) return `expires in ${span(left)}`;
  return `good for ${span(left)}`;
}

export function sessionLife(sessionExpiresAt: number | undefined, now: number): string | null {
  if (sessionExpiresAt === undefined) return null;
  const left = sessionExpiresAt - now;
  if (left <= 0) return "Its sign-in may have lapsed";
  const days = Math.floor(left / DAY);
  if (days === 0) return "Its sign-in may lapse in under a day";
  return days === 1 ? "Its sign-in may lapse in 1 day" : `Its sign-in may lapse in ${days} days`;
}

export function sessionLifeColumn(sessionExpiresAt: number, now: number): string {
  const left = sessionExpiresAt - now;
  return left <= 0 ? "sign-in may have lapsed" : `sign-in may lapse in ${span(left)}`;
}

export const capitalised = (s: string) => s[0].toUpperCase() + s.slice(1);

// the app draws "y" in orange
export type Level = "g" | "y" | "r";
// words.rs `usage_level`, as its figure is drawn
export function level(percent: number): Level {
  const drawn = whole(percent);
  return drawn >= 90 ? "r" : drawn >= 70 ? "y" : "g";
}

// The world's clock, in its own zone: "20:51", and as each system's clock says the moment.
const local = (epoch: number) => new Date((epoch + ZONE) * 1000);
const two = (n: number) => String(n).padStart(2, "0");
const weekdays = "Sun Mon Tue Wed Thu Fri Sat".split(" ");
const months = "Jan Feb Mar Apr May Jun Jul Aug Sep Oct Nov Dec".split(" ");
const offset = `${ZONE < 0 ? "-" : "+"}${two(Math.floor(Math.abs(ZONE) / HOUR))}:${two((Math.abs(ZONE) % HOUR) / MINUTE)}`;

export const hm = (epoch: number) => {
  const d = local(epoch);
  return `${two(d.getUTCHours())}:${two(d.getUTCMinutes())}`;
};

// the macOS menu bar's "Tue 13 Oct 20:51"
export const menuBarClock = (epoch: number) => {
  const d = local(epoch);
  return `${weekdays[d.getUTCDay()]} ${d.getUTCDate()} ${months[d.getUTCMonth()]} ${hm(epoch)}`;
};

// the en-GB regional format's "d", under the taskbar's time
export const shortDate = (epoch: number) => {
  const d = local(epoch);
  return `${two(d.getUTCDate())}/${two(d.getUTCMonth() + 1)}/${d.getUTCFullYear()}`;
};

// audit.rs's timestamp, as `pitboard log` prints it
export const stamp = (epoch: number) => {
  const d = local(epoch);
  return `${d.getUTCFullYear()}-${two(d.getUTCMonth() + 1)}-${two(d.getUTCDate())}T${hm(epoch)}:${two(d.getUTCSeconds())}${offset}`;
};

export const isInUse = (a: Account, w: World) => w.inUse[a.tool] === a.label;

export const toolAccounts = (w: World, tool: Tool) => w.accounts.filter((a) => a.tool === tool);

export const toolsOn = (platform: Platform) =>
  tools.filter(([tool]) => toolOn[tool].includes(platform));

const toolName = (tool: Tool) => tools.find(([t]) => t === tool)![1];

// As every listing gives a tool's accounts: the one in use first (status.rs `assemble`).
export const listed = (w: World, tool: Tool) =>
  [...toolAccounts(w, tool)].sort((a, b) => Number(isInUse(b, w)) - Number(isInUse(a, w)));

// present/accounts.rs `in_place_of_limits` while a switch runs, and present/setup.rs
// `updated_menu` while a read does and after it (present/words.rs `updated`)
export const SWITCHING = "Switching…";
export const READING = "Reading…";
export const updated = (readAt: number) => `Updated ${hm(readAt)}`;

// account_windows/accounts.rs: one item opens a site's one account in place of a submenu
export const openSiteAs = (site: string, label: string) => `Open ${site} as ${label}`;

export const menuTitle = (a: Account) => (a.plan ? `${a.label} · ${a.plan}` : a.label);

// present/accounts.rs `runs_out_first`: which limit of the account in use runs out first at its
// pace, and in how many seconds.
export function runsOutFirst(a: Account, w: World): { limit: Limit; inSeconds: number } | null {
  if (!isInUse(a, w)) return null;
  let first: { limit: Limit; inSeconds: number } | null = null;
  for (const limit of a.limits) {
    const p = pace(limit, w);
    if (p?.runsOutIn === undefined) continue;
    if (!first || p.runsOutIn < first.inSeconds) first = { limit, inSeconds: p.runsOutIn };
  }
  return first;
}

// present/accounts.rs `limits_said`: the line under an account in the menu, or its email where
// nothing is read of it yet.
export function menuLine(a: Account, w: World): string {
  if (a.limits.length === 0) return a.tool === "desktop" ? "No usage read" : a.email;
  const first = runsOutFirst(a, w);
  const said = a.limits.map((l) => {
    const runs = first?.limit === l ? ` (${runsOutIn(first.inSeconds)})` : "";
    return `${scopedLimitName(l)} ${figure(l.percent)}${runs}`;
  });
  return capitalised(said.join(", "));
}

// present/accounts.rs `limit_row`: one limit as a row of its account's bars.
export interface LimitRow {
  short: string;
  percent: number;
  figure: string;
  level: Level;
  resets: string;
  pace: { expected: number; standing: Standing; said: string } | null;
}

export function limitRow(limit: Limit, w: World): LimitRow {
  const p = pace(limit, w);
  return {
    short: limitColumn(limit),
    percent: limit.percent,
    figure: figure(limit.percent),
    level: level(limit.percent),
    resets: limit.resetsAt === null ? "" : resets(limit.resetsAt, w.now),
    pace: p && { expected: p.expected, standing: p.standing, said: paceColumn(p) },
  };
}

// The in-use account closest to running out, a tie going to Claude Code (reference/mac-app#menu-bar-item).
export const fullest = (a: Account) => Math.max(...a.limits.map((l) => whole(l.percent)));
const inUseOf = (w: World, tool: Tool) => w.accounts.find((a) => a.tool === tool && isInUse(a, w))!;
const titled = (w: World) =>
  (["claude", "codex"] as Tool[])
    .map((tool) => inUseOf(w, tool))
    .reduce((best, a) => (fullest(a) > fullest(best) ? a : best));

export function menuBarTitle(w: World): string {
  const shown = titled(w);
  return `${shown.label} ${fullest(shown)}%`;
}

// MenuBarShows.swift: what Settings' Menu bar shows leaves of the title, the mark alone for Icon
// only, while its spoken name stays whole.
export function menuBarShown(w: World, shows: "nameAndUsage" | "usage" | "icon"): string {
  if (shows === "icon") return "";
  return shows === "usage" ? `${fullest(titled(w))}%` : menuBarTitle(w);
}

// The level of the menu bar's figure, which the Windows icon draws (present/accounts.rs `menu_bar`).
export const menuBarLevel = (w: World) => level(fullest(titled(w)));

// The menu bar item's spoken name: Pitboard, its title, and the tool where more than one is read
// (present/accounts.rs `menu_bar`).
export const menuBarSpoken = (w: World) =>
  ["Pitboard", menuBarTitle(w), toolName(titled(w).tool)].join(", ");

// An account's row and its button, as VoiceOver and Narrator say them (present/accounts.rs `item`
// and `action`).
export const accountSpoken = (a: Account, w: World) =>
  [a.label, a.plan, isInUse(a, w) ? "in use" : null].filter((s) => s !== null).join(", ");
export const useSpoken = (a: Account) => `Use ${a.label}`;

// present/words.rs `rounded_half_even`, `units` and `spoken_span`: "1 hour, 30 minutes".
const halfEven = (n: number, d: number) => {
  const whole = Math.floor(n / d);
  const twice = 2 * (n - whole * d);
  return twice < d ? whole : twice > d ? whole + 1 : whole + (whole & 1);
};
const units = (count: number, unit: string) => (count === 1 ? `1 ${unit}` : `${count} ${unit}s`);

export function spokenSpan(seconds: number): string {
  const s = Math.max(seconds, MINUTE);
  const minutes = halfEven(s, MINUTE);
  const counts: [number, number, string][] = [
    [Math.floor(minutes / (24 * 60)), DAY, "day"],
    [Math.floor((minutes % (24 * 60)) / 60), HOUR, "hour"],
    [minutes % 60, MINUTE, "minute"],
  ];
  const named = counts.filter(([count]) => count > 0).slice(0, 2);
  if (named.length === 1) return units(halfEven(s, named[0][1]), named[0][2]);
  const [[, first, firstUnit], [, second, secondUnit]] = named;
  let whole = Math.floor(s / first);
  let part = halfEven(s - whole * first, second);
  if (part * second === first) {
    whole += 1;
    part = 0;
  }
  return `${units(whole, firstUnit)}, ${units(part, secondUnit)}`;
}

// present/words.rs `spoken_limit`: a limit's row read as one sentence, from its name, its share,
// its pace and the seconds until it resets.
export function spokenLimitOf(
  name: string,
  percent: number,
  resettingIn: number | null,
  p: Pace | null,
): string {
  let said = `${name} limit, ${whole(percent)} percent used`;
  if (p) {
    const points = whole(Math.abs(p.delta));
    said +=
      p.standing === "even"
        ? ", on an even pace"
        : `, ${points} percent ${p.standing} an even pace`;
  }
  if (resettingIn === null) return said;
  return resettingIn <= 0
    ? `${said}, resetting now`
    : `${said}, resets in ${spokenSpan(resettingIn)}`;
}

export const spokenLimit = (limit: Limit, w: World) =>
  spokenLimitOf(
    scopedLimitName(limit),
    limit.percent,
    limit.resetsAt === null ? null : limit.resetsAt - w.now,
    pace(limit, w),
  );

// The icon's tooltip in the notification area: Pitboard, then each account in use with its tool and
// the line the menu shows under it (present/accounts.rs `menu_bar`).
export const trayTip = (w: World) =>
  [
    "Pitboard",
    ...(["claude", "codex"] as Tool[]).map((tool) => {
      const a = inUseOf(w, tool);
      return `${a.label} (${toolName(tool)}): ${menuLine(a, w)}`;
    }),
  ].join("\n");

// NotifyIcon.cs `Tip`: the Shell holds 127 UTF-16 units of the icon's tooltip, cut never inside a
// surrogate pair.
export const TIP_LENGTH = 127;
export const shellTip = (tip: string) =>
  tip.length <= TIP_LENGTH
    ? tip
    : tip.slice(0, /[\uD800-\uDBFF]/.test(tip[TIP_LENGTH - 1]) ? TIP_LENGTH - 1 : TIP_LENGTH);

// The notification once a limit of the account in use reaches the share (present/notices.rs
// `at_limit_notice`): personal's weekly limit, with work's room in it.
export const headsUpTitle = (label: string, limit: Limit, used: number) =>
  `${label} has used ${used}% of its ${scopedLimitName(limit)} limit`;
const weekly = (label: string) => account(label).limits.find((l) => l.lengthSeconds === WEEK)!;
export const headsUp = {
  account: "personal",
  title: headsUpTitle("personal", weekly("personal"), share),
  tool: "Claude Code",
  body: `work has ${100 - whole(weekly("work").percent)}% of its own left.`,
  button: "Switch",
};

// The limit that runs out first, in the sentence the window's note and `pitboard` say it in.
export function runsOutNote(a: Account, w: World): string | null {
  const first = runsOutFirst(a, w);
  return first ? runsOut(scopedLimitName(first.limit), first.inSeconds) : null;
}

// present/accounts.rs `stale_note`: when Pitboard last read a Claude Desktop login in use, which
// is also its item's tooltip in the menu.
export const staleNote = (a: Account, w: World) =>
  isInUse(a, w) && a.readAt !== undefined
    ? `Read ${span(w.now - a.readAt)} ago, with Claude closed`
    : null;

// present/accounts.rs `item`'s notes under an account in the window, then where it stands for
// warm-ups, where it is turned on for them.
export function notes(a: Account, w: World, warm: string | null): string[] {
  const on = isInUse(a, w);
  const runs = runsOutNote(a, w);
  return [
    staleNote(a, w),
    runs && capitalised(runs),
    on ? null : (parkedLife(a.refreshExpiresAt, w.now) ?? sessionLife(a.sessionExpiresAt, w.now)),
    warm,
  ].filter((n): n is string => n !== null);
}

// The world's warm-up note: personal's period runs until its five hours reset.
export function warmsAgainNote(a: Account): string | null {
  const period = a.limits.find((l) => l.lengthSeconds === 5 * HOUR && !l.scope);
  return a.tool === "claude" && warmUp.accounts.includes(a.label) && period?.resetsAt
    ? `Warms up again at ${hm(period.resetsAt)}`
    : null;
}

const tinted = (cls: string, html: string) => (html ? `<span class="${cls}">${html}</span>` : "");
const pad = (text: string, width: number) => text + " ".repeat(Math.max(0, width - text.length));

// crates/pitboard/src/ui.rs `bar`; a run of cells is one span, drawn as one block of its colour.
const cell = "bg-current [-webkit-text-fill-color:transparent]";
const shade = "bg-current/32 [-webkit-text-fill-color:transparent]";
function bar(limit: Limit, p: Pace | null): string {
  const width = 10;
  const filled = Math.min(width, Math.max(0, Math.round((limit.percent / 100) * width)));
  const cells = (from: number, to: number) => {
    const full = Math.min(Math.max(filled, from), to);
    return (
      tinted(`${level(limit.percent)} ${cell}`, "█".repeat(full - from)) +
      tinted(`dim ${shade}`, "░".repeat(to - full))
    );
  };
  if (p === null || p.standing === "even") return cells(0, width);
  const at = Math.min(Math.floor((p.expected / 100) * width), width - 1);
  return cells(0, at) + tinted(p.standing === "over" ? "r" : "g", "│") + cells(at + 1, width);
}

// A tool's heading in `pitboard`, with what is true of all its accounts said once on it.
export function statusHeading(tool: Tool): string {
  const name = `<b>${toolName(tool)}</b>`;
  return tool === "desktop"
    ? `${name}${tinted("dim", " · Pitboard reads no usage for Claude Desktop")}`
    : name;
}

function standingText(a: Account, on: boolean, now: number): string {
  if (on)
    return a.readAt !== undefined
      ? `${tinted("g", "signed in")} ${tinted("dim", `· read ${span(now - a.readAt)} ago`)}`
      : tinted("g", "signed in");
  const until =
    a.refreshExpiresAt !== undefined
      ? parkedLifeColumn(a.refreshExpiresAt, now)
      : a.sessionExpiresAt !== undefined
        ? sessionLifeColumn(a.sessionExpiresAt, now)
        : null;
  return until ? `${tinted("g", "ready")} ${tinted("dim", `· ${until}`)}` : tinted("g", "ready");
}

// `pitboard` for one tool's accounts, one HTML string a line, usage lines only where it is read.
export function statusLines(w: World, tool: Tool): string[] {
  const rows = listed(w, tool);
  const width = (of: (a: Account) => string[]) =>
    Math.max(0, ...w.accounts.flatMap(of).map((s) => s.length));
  const labelWidth = width((a) => [a.label]);
  const emailWidth = width((a) => [a.email]);
  const planWidth = width((a) => [a.plan ?? ""]);
  const nameWidth = width((a) => a.limits.map(limitColumn));
  const resetsWidth = width((a) =>
    a.limits.flatMap((l) => (l.resetsAt === null ? [] : [resets(l.resetsAt, w.now)])),
  );
  const lines: string[] = [];
  rows.forEach((a, i) => {
    if (i > 0 && a.limits.length > 0) lines.push("");
    const on = isInUse(a, w);
    lines.push(
      `${on ? tinted("g", "●") : tinted("dim", "○")} <b>${escaped(pad(a.label, labelWidth))}</b>  ${tinted("dim", escaped(pad(a.email, emailWidth)))}  ${tinted("dim", escaped(pad(a.plan ?? "", planWidth)))}  ${standingText(a, on, w.now)}`,
    );
    for (const l of a.limits) {
      const p = pace(l, w);
      const said = l.resetsAt === null ? "" : resets(l.resetsAt, w.now);
      const after = p
        ? `${tinted("dim", pad(said, resetsWidth))}  ${tinted(p.standing === "over" ? "r" : p.standing === "under" ? "g" : "dim", paceColumn(p))}`
        : tinted("dim", said);
      lines.push(
        `    ${pad(limitColumn(l), nameWidth)}  ${bar(l, p)}  ${tinted(level(l.percent), figure(l.percent).padStart(4))}  ${after}`,
      );
    }
    const note = runsOutNote(a, w);
    if (note) lines.push(`    ${pad("", nameWidth)}  ${tinted("r", note)}`);
  });
  return lines;
}

// reference/integration-commands#statusline
export function statusLine(w: World): string {
  const share = (l: Limit | undefined, marks: boolean) => {
    if (!l) return tinted("dim", "?");
    const s = pace(l, w)?.standing;
    const mark =
      marks && s === "over" ? tinted("r", "▲") : marks && s === "under" ? tinted("g", "▼") : "";
    return `${tinted(level(l.percent), figure(l.percent))}${mark}`;
  };
  const shares = (a: Account, marks: boolean) =>
    `${share(
      a.limits.find((l) => l.lengthSeconds === 5 * HOUR),
      marks,
    )}${tinted("dim", "·")}${share(
      a.limits.find((l) => l.lengthSeconds === WEEK),
      marks,
    )}`;
  const claude = toolAccounts(w, "claude");
  const current = inUseOf(w, "claude");
  const others = claude.filter((a) => a !== current);
  return [
    `<b>${escaped(current.label)}</b> ${shares(current, true)}`,
    ...others.map((a) => `${tinted("dim", escaped(a.label))} ${shares(a, false)}`),
  ].join("  ");
}

// The activity list's changes, newest first, in the app's words (present/words.rs `change_verb`,
// `change_outcome` and `change_caller`).
const verbs: Record<string, string> = {
  use: "Switch",
  enroll: "Enrol",
  "warm-up": "Warm-up",
  "warm-on": "Warm-up turned on",
  "warm-off": "Warm-up turned off",
  "warm-range": "Warm-up hours changed",
};
const outcomes: Record<string, string> = { ok: "Done" };
const callers = { app: "Pitboard app", cli: "Command line" };

// As each app dates a change: "13 Oct 2026 at 20:49" on macOS, and in the en-GB regional format's
// "g" on Windows, "13/10/2026 20:49".
const dated = (at: number, platform: Platform) => {
  const d = local(at);
  return platform === "windows"
    ? `${shortDate(at)} ${hm(at)}`
    : `${d.getUTCDate()} ${months[d.getUTCMonth()]} ${d.getUTCFullYear()} at ${hm(at)}`;
};

export const activity = (log: Logged[], platform: Platform) =>
  log
    .filter((e) => !e.on || e.on.includes(platform))
    .reverse()
    .map((e) => ({
      date: dated(e.at, platform),
      change: changeVerb(e.verb),
      account: e.subject,
      result: changeOutcome(e.outcome),
      by: callers[e.caller],
    }));

// The rest of present/words.rs `change_verb` and `change_outcome`, and its `readable` for a code
// they have no words for.
const moreVerbs: Record<string, string> = {
  switch: "Switch",
  "auto-switch": "Automatic switch",
  "auto-stay": "Automatic switch not made",
  forget: "Forget",
  rename: "Rename",
  renew: "Renew",
  abandon: "Give up on a switch",
  repair: "Repair",
  adopt: "Adopt",
  uninstall: "Uninstall",
  "in-use": "Login in use changed",
  stow: "Put away a login left in a file",
  park: "Park for a sign-in",
};
const moreOutcomes: Record<string, string> = {
  second_sign_in: "Second sign-in",
  warm_up_not_started: "No period started",
  warm_up_limit_reached: "A limit is used up",
  warm_up_login_refused: "Login refused",
  warm_up_unreachable: "Anthropic out of reach",
  warm_up_timed_out: "Took too long",
  warm_up_failed: "Claude Code failed",
};
const readable = (code: string) => capitalised(code.replaceAll("_", " "));
export const changeVerb = (verb: string) => verbs[verb] ?? moreVerbs[verb] ?? readable(verb);
export const changeOutcome = (outcome: string) =>
  outcomes[outcome] ?? moreOutcomes[outcome] ?? readable(outcome);

// time.rs `moment`: "20:51" today, "Oct 14 05:53" on another day.
export function moment(at: number, now: number): string {
  const day = (t: number) => Math.floor((t + ZONE) / DAY);
  const d = new Date((at + ZONE) * 1000);
  const two = (n: number) => String(n).padStart(2, "0");
  const hm = `${two(d.getUTCHours())}:${two(d.getUTCMinutes())}`;
  return day(at) === day(now) ? hm : `${months[d.getUTCMonth()]} ${d.getUTCDate()} ${hm}`;
}
