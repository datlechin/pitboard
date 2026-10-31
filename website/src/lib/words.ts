// The app's and the command line's words for world.ts, by crates/pitboard-core/src/words.rs,
// crates/pitboard-model/src/present and crates/pitboard/src/render/status.rs.

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
import type { AutoStanding, Preferences, Skip, WarmStanding } from "./model.ts";
import { fronts, type Front, type Platform } from "./platform.ts";

// autoswitch.rs `SETTLING_SECONDS` and switch/mod.rs `ADOPTION_CEILING_SECONDS`, which the model's
// rules count and these words say.
export const SETTLING_SECONDS = 300;
export const ADOPTION_CEILING_SECONDS = 33;

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

// words.rs `time_to_run_out` and `runs_out`
export const timeToRunOut = (inSeconds: number) => (inSeconds >= MINUTE ? span(inSeconds) : null);

export function runsOut(limit: string, inSeconds: number): string {
  const left = timeToRunOut(inSeconds);
  return left
    ? `${limit} limit runs out in ${left} at this pace`
    : `${limit} limit is about to run out`;
}

// state.rs `RENEW_WITHIN` and `renewal_due`
export const RENEW_WITHIN = 3 * DAY;
export const renewalDue = (refreshExpiresAt: number, now: number) => {
  const left = refreshExpiresAt - now;
  return left >= 1 && left < RENEW_WITHIN;
};

// words.rs `parked_life_column` and `session_life_column`
export function parkedLifeColumn(refreshExpiresAt: number, now: number): string {
  const left = refreshExpiresAt - now;
  if (left <= 0) return "expired";
  if (renewalDue(refreshExpiresAt, now)) return `expires in ${span(left)}`;
  return `good for ${span(left)}`;
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
// Windows' calendar flyout's date, in the en-GB format the taskbar's date is in
const weekdaysLong = "Sunday Monday Tuesday Wednesday Thursday Friday Saturday".split(" ");
const monthsLong =
  "January February March April May June July August September October November December".split(
    " ",
  );
export const calendarDate = (epoch: number) => {
  const d = local(epoch);
  return `${weekdaysLong[d.getUTCDay()]} ${d.getUTCDate()} ${monthsLong[d.getUTCMonth()]}`;
};

// MacLocalTime `date` and WindowsLocalTime.cs `Date` in en-GB, the culture's day before its month
// abbreviated: "9 Nov"
export const dayAndMonth = (epoch: number) => {
  const d = local(epoch);
  return `${d.getUTCDate()} ${months[d.getUTCMonth()]}`;
};

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
// `updated_menu` while a read does and after it (present/words/accounts.rs `updated`)
export const SWITCHING = "Switching…";
export const READING = "Reading…";
export const updated = (readAt: number) => `Updated ${hm(readAt)}`;

// account_windows/accounts.rs: one item opens a site's one account in place of a submenu
export const openSiteAs = (site: string, label: string) => `Open ${site} as ${label}`;

// status.rs `Row::lasts`: which limit of the account in use runs out first at its pace, and in
// how many seconds, which `pitboard` says under its bars.
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

// present/words/mod.rs `titled`: title case on macOS and in GNOME's header capitalisation,
// sentence case on Windows.
export const titled = (front: Front, title: string, sentence: string) =>
  front === "windows" ? sentence : title;

// present/words/mod.rs `settings`: Apple's Settings… in a menu, and GNOME's Preferences.
export const settings = (front: Front, menuItem: boolean) =>
  front === "gnome" ? "Preferences" : front === "macos" && menuItem ? "Settings…" : "Settings";

// account_windows/mod.rs `this_machine`: one front on every Linux desktop, so no desktop's name.
export const thisMachine = (front: Front) =>
  ({ macos: "this Mac", windows: "this PC", gnome: "this computer" })[front];

// present/words/setup.rs `where_it_lives` and `CLOSING_QUITS`: where no status item keeps
// Pitboard running, as no stock GNOME has one, closing its window quits it.
export const CLOSING_QUITS = "Pitboard quits once its window is closed.";
export function whereItLives(front: Front, statusItem: boolean): string {
  if (front === "windows")
    return "Pitboard’s icon is in the notification area. Select ^ to find it.";
  if (front === "macos") return "Pitboard lives in the menu bar.";
  return statusItem
    ? "Pitboard keeps running once its window is closed, and its icon stays with your other status icons."
    : `${CLOSING_QUITS} Preferences can keep it watching your limits after that.`;
}

export const uncapitalised = (s: string) => s.charAt(0).toLowerCase() + s.slice(1);

// present/words/accounts.rs `span`: "2 hr 9 min", "1 day 3 hr", "under a minute".
export function appSpan(seconds: number): string {
  const minutes = Math.floor(Math.max(seconds, 0) / MINUTE);
  const [days, hours, rest] = [
    Math.floor(minutes / 1440),
    Math.floor(minutes / 60) % 24,
    minutes % 60,
  ];
  if (days === 0 && hours === 0) return rest === 0 ? "under a minute" : `${rest} min`;
  if (days === 0) return rest === 0 ? `${hours} hr` : `${hours} hr ${rest} min`;
  const day = days === 1 ? "1 day" : `${days} days`;
  return hours === 0 ? day : `${day} ${hours} hr`;
}

// present/mod.rs `clock_of`: a moment as the app's clock says it, with its weekday on another day
// than `now`, in the world's en-GB regional format.
export function clockAt(at: number, now: number): string {
  const day = (t: number) => Math.floor((t + ZONE) / DAY);
  return day(at) === day(now) ? hm(at) : `${weekdays[local(at).getUTCDay()]} ${hm(at)}`;
}

// present/words/accounts.rs `limit_short` and `limit_title`: "5-hour" and "Weekly (Sonnet 4.5)"
// beside a bar, "5-hour limit" and "Weekly limit (Sonnet 4.5)" where a limit is named alone.
export const limitShort = (limit: Limit) =>
  limit.scope ? `${capitalised(limitName(limit))} (${limit.scope})` : capitalised(limitName(limit));
export const limitTitle = (limit: Limit) =>
  limit.scope
    ? `${capitalised(limitName(limit))} limit (${limit.scope})`
    : `${capitalised(limitName(limit))} limit`;

// present/accounts.rs `limit_row`'s reset: "Resets 22:00", "Resets Mon 14:50", "Resetting now".
export const resetsSaid = (at: number | null, now: number) =>
  at === null ? "" : at <= now ? "Resetting now" : `Resets ${clockAt(at, now)}`;

// present/words/accounts.rs `sign_in_expires`: a saved sign-in due to expire, or a Claude Desktop
// one that may have lapsed.
export function signInExpires(left: number, lapses: boolean): string {
  if (left <= 0) return lapses ? "Sign-in may have expired" : "Sign-in expired";
  const days = Math.floor(left / DAY);
  const may = lapses ? "may expire" : "expires";
  if (days === 0) return `Sign-in ${may} today`;
  return days === 1 ? `Sign-in ${may} in 1 day` : `Sign-in ${may} in ${days} days`;
}

// status.rs `Standing::LoginExpired`: a saved sign-in past its life, which only a sign-in brings
// back (present/words/accounts.rs `sign_in_standing`).
export const SIGN_IN_EXPIRED = "Sign-in expired";
export const signInExpired = (a: Account, w: World) =>
  !isInUse(a, w) && a.refreshExpiresAt !== undefined && a.refreshExpiresAt <= w.now;

// present/accounts.rs `limits_said`, with `in_place_of_limits` before it: the line under an account
// in the menu, "5-hour 59% used, Weekly 73%", the last limit it reached until that resets, or what
// stands in their way.
export function menuLine(a: Account, w: World, front: Front = "macos"): string {
  if (signInExpired(a, w)) return SIGN_IN_EXPIRED;
  if (a.limits.length === 0) return a.tool === "desktop" ? "No usage read" : a.email;
  // Rust's `max_by_key` keeps the last of the latest
  const reached = a.limits
    .filter((l) => (l.resetsAt === null || l.resetsAt > w.now) && l.percent >= 100)
    .reduce<Limit | null>(
      (last, l) => (!last || (l.resetsAt ?? Infinity) >= (last.resetsAt ?? Infinity) ? l : last),
      null,
    );
  if (reached)
    return reached.resetsAt === null
      ? `${limitTitle(reached)} reached`
      : `${limitTitle(reached)} reached, resets ${clockAt(reached.resetsAt, w.now)}`;
  return limitsLine(
    front,
    a.limits.map((l) => [limitShort(l), figure(l.percent)]),
  );
}

// present/words/accounts.rs `limits_line`: "5-hour 42% used, Weekly 12%", in sentence case after
// the first on Windows and GNOME, whose guidelines give it to text that runs on.
export const limitsLine = (front: Front, limits: [name: string, figure: string][]) =>
  limits
    .map(([name, figure], i) =>
      i === 0
        ? `${name} ${figure} used`
        : `${front === "macos" ? name : uncapitalised(name)} ${figure}`,
    )
    .join(", ");

// present/words/accounts.rs `named_in_menu`: "work" on macOS, whose subtitle says the rest; "work
// (42%)" on Windows and in a status item's menu on Linux, whose entries have no subtitle.
export const namedInMenu = (front: Front, title: string, aside: string | null) =>
  front !== "macos" && aside ? `${title} (${uncapitalised(aside)})` : title;

// present/accounts.rs `item`'s menu title: an account's label, then the figure the menu bar would
// show of it, or why it can't be switched to.
export function menuTitle(a: Account, w: World, front: Front, switching = false): string {
  const aside = switching
    ? SWITCHING
    : signInExpired(a, w)
      ? SIGN_IN_EXPIRED
      : a.limits.length > 0
        ? `${fullest(a)}%`
        : null;
  return namedInMenu(front, a.label, aside);
}

// present/accounts.rs `limit_row`: one limit as a row of its account's bars, and as one sentence
// (present/words/accounts.rs `spoken_limit`).
export interface LimitRow {
  short: string;
  percent: number;
  figure: string;
  level: Level;
  resets: string;
  spoken: string;
}

export function limitRow(limit: Limit, w: World): LimitRow {
  const said = resetsSaid(limit.resetsAt, w.now);
  return {
    short: limitShort(limit),
    percent: limit.percent,
    figure: figure(limit.percent),
    level: level(limit.percent),
    resets: said,
    spoken: spokenLimit(limitTitle(limit), limit.percent, said),
  };
}

export const spokenLimit = (title: string, percent: number, resets: string) => {
  const used = `${title}, ${whole(percent)} percent used`;
  return resets ? `${used}, ${uncapitalised(resets)}` : used;
};

// The in-use account closest to running out, a tie going to Claude Code (reference/mac-app#menu-bar-item).
export const fullest = (a: Account) => Math.max(...a.limits.map((l) => whole(l.percent)));
const inUseOf = (w: World, tool: Tool) => w.accounts.find((a) => a.tool === tool && isInUse(a, w))!;
const barAccount = (w: World) =>
  (["claude", "codex"] as Tool[])
    .map((tool) => inUseOf(w, tool))
    .reduce((best, a) => (fullest(a) > fullest(best) ? a : best));

export function menuBarTitle(w: World): string {
  const shown = barAccount(w);
  return `${shown.label} ${fullest(shown)}%`;
}

// MenuBarShows.swift: what Settings' Menu bar shows leaves of the title, the mark alone for Icon
// only, while its spoken name stays whole.
export function menuBarShown(w: World, shows: "nameAndUsage" | "usage" | "icon"): string {
  if (shows === "icon") return "";
  return shows === "usage" ? `${fullest(barAccount(w))}%` : menuBarTitle(w);
}

// The level of the menu bar's figure, which the Windows icon draws (present/accounts.rs `status_item`).
export const menuBarLevel = (w: World) => level(fullest(barAccount(w)));

// The menu bar item's spoken name: Pitboard, its title, and the tool where more than one is read
// (present/words/status_item.rs `bar_spoken`).
export const menuBarSpoken = (w: World, attention = false) =>
  [
    "Pitboard",
    menuBarTitle(w),
    toolName(barAccount(w).tool),
    ...(attention ? ["needs attention"] : []),
  ].join(", ");

// An account's row and its button, as VoiceOver and Narrator say them (present/words/accounts.rs
// `row_spoken` and `switch_account`).
export const accountSpoken = (a: Account, w: World) =>
  [
    a.label,
    a.plan,
    isInUse(a, w) ? "in use" : null,
    signInExpired(a, w) ? uncapitalised(SIGN_IN_EXPIRED) : null,
  ]
    .filter((s) => s !== null)
    .join(", ");
export const switchSpoken = (a: Account) => `Switch to ${a.label}`;

// A status item's tooltip, the Windows icon's and the Linux tray's: Pitboard, whether it needs
// attention, then each account in use with its tool and the line the menu shows under it, going on
// in lower case after its colon (present/words/status_item.rs `bar_tip`, from present/accounts.rs
// `summary`).
export const barTip = (w: World, front: Front, attention = false) =>
  [
    "Pitboard",
    ...(attention ? ["Needs attention"] : []),
    ...(["claude", "codex"] as Tool[]).map((tool) => {
      const a = inUseOf(w, tool);
      return `${a.label} (${toolName(tool)}): ${uncapitalised(menuLine(a, w, front))}`;
    }),
  ].join("\n");

// NotifyIcon.cs `Tip`: the Shell holds 127 UTF-16 units of the icon's tooltip, cut never inside a
// surrogate pair.
export const TIP_LENGTH = 127;
export const shellTip = (tip: string) =>
  tip.length <= TIP_LENGTH
    ? tip
    : tip.slice(0, /[\uD800-\uDBFF]/.test(tip[TIP_LENGTH - 1]) ? TIP_LENGTH - 1 : TIP_LENGTH);

// A notification's words as each front says them, worked out once for every front.
export type Said = Record<Front, string>;
export const eachFront = (words: (front: Front) => string) =>
  Object.fromEntries(fronts.map((f) => [f, words(f)])) as Said;

// present/words/notices.rs `limit_notification`: "5-Hour Limit Almost Reached", "Weekly limit
// reached".
export function limitNotification(limit: Limit, reached: boolean): Said {
  const headed = limitName(limit).split("-").map(capitalised).join("-");
  const title = `${headed} Limit${limit.scope ? ` (${limit.scope})` : ""}`;
  return eachFront((front) =>
    reached
      ? titled(front, `${title} Reached`, `${limitTitle(limit)} reached`)
      : titled(front, `${title} Almost Reached`, `${limitTitle(limit)} almost reached`),
  );
}

// present/words/notices.rs `switch_button`: macOS draws the button its notification's category
// registered, and a toast or a GNOME notification takes any.
export const switchButton = (label: string) =>
  eachFront((front) => (front === "macos" ? "Switch" : `Switch to ${label}`));
export const SWITCH_BACK = eachFront((front) => titled(front, "Switch Back", "Switch back"));

// present/words/notices.rs `has_used_share` and `other_used`: the at-limit notification's body.
export const hasUsedShare = (label: string, used: string) => `${label} has used ${used}.`;
export const otherUsed = (label: string, left: number | null) =>
  left === null ? `${label} has no such limit.` : `${label} has used ${Math.max(100 - left, 0)}%.`;

// The notification once a limit of the account in use reaches the share (present/notices.rs
// `at_limit_notice`): personal's weekly limit, with what work has used of its own.
const weekly = (label: string) => account(label).limits.find((l) => l.lengthSeconds === WEEK)!;
const workLeft = 100 - whole(weekly("work").percent);
export const headsUp = {
  account: "personal",
  title: limitNotification(weekly("personal"), false),
  tool: "Claude Code",
  body: `${hasUsedShare("personal", `${share}%`)} ${otherUsed("work", workLeft)}`,
  button: switchButton("work"),
};

// The limit that runs out first, in the sentence `pitboard` says it in.
export function runsOutNote(a: Account, w: World): string | null {
  const first = runsOutFirst(a, w);
  return first ? runsOut(scopedLimitName(first.limit), first.inSeconds) : null;
}

// present/words/accounts.rs `read_ago`: when Pitboard last read a Claude Desktop login in use,
// which is also its item's tooltip in the menu.
export const staleNote = (a: Account, w: World) =>
  isInUse(a, w) && a.readAt !== undefined
    ? `Read ${appSpan(w.now - a.readAt)} ago, with Claude closed`
    : null;

// present/accounts.rs `item`'s lines under an account in the window: when a Claude Desktop login in
// use was read, a saved sign-in that has expired or soon does, and where it stands for warm-ups,
// where it is turned on for them.
export function rowLines(a: Account, w: World, warm: string | null): string[] {
  const parked = !isInUse(a, w);
  const ended =
    parked && a.refreshExpiresAt !== undefined
      ? a.refreshExpiresAt <= w.now || renewalDue(a.refreshExpiresAt, w.now)
        ? signInExpires(a.refreshExpiresAt - w.now, false)
        : null
      : parked && a.sessionExpiresAt !== undefined && a.sessionExpiresAt <= w.now
        ? signInExpires(a.sessionExpiresAt - w.now, true)
        : null;
  return [staleNote(a, w), ended, warm].filter((n): n is string => n !== null);
}

// present/accounts.rs `item`'s notes, as the macOS window's row draws the most pressing alone: a
// saved sign-in that has expired, or one that soon does, or a Claude Desktop one that may have
// lapsed.
export function rowNote(a: Account, w: World): string | null {
  if (signInExpired(a, w)) return SIGN_IN_EXPIRED;
  if (isInUse(a, w)) return null;
  if (a.refreshExpiresAt !== undefined)
    return renewalDue(a.refreshExpiresAt, w.now)
      ? signInExpires(a.refreshExpiresAt - w.now, false)
      : null;
  return a.sessionExpiresAt !== undefined && a.sessionExpiresAt <= w.now
    ? signInExpires(a.sessionExpiresAt - w.now, true)
    : null;
}

// An account's one note in the Windows window, its most pressing: what rowLines says but its
// warm-up, which its details say (present/accounts.rs `notes`).
export const windowsRowNote = (a: Account, w: World) => rowLines(a, w, null)[0] ?? null;

// The inspector's Sign-In: in use, how long a saved sign-in is valid for, or that it expired
// (present/accounts.rs `item`'s `sign_in`, present/words/accounts.rs `saved_until`).
export function signInSaid(a: Account, w: World): string | null {
  if (isInUse(a, w)) return "In use";
  if (signInExpired(a, w)) return SIGN_IN_EXPIRED;
  if (a.refreshExpiresAt !== undefined)
    return `Saved sign-in valid until ${dayAndMonth(a.refreshExpiresAt)}`;
  return a.sessionExpiresAt !== undefined
    ? `Saved sign-in may expire after ${dayAndMonth(a.sessionExpiresAt)}`
    : null;
}

// present/words/accounts.rs `resets_in`, beside a limit's reset in the inspector and in an opened
// row: "in 2 hr 9 min".
export const resetsIn = (at: number | null, now: number) =>
  at !== null && at > now ? `in ${appSpan(at - now)}` : "";

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

// The activity list's changes, newest first, in the app's words (present/words/machine.rs
// `change_verb`, `change_outcome` and `change_caller`).
const verbs: Record<string, string> = {
  use: "Switch",
  enroll: "Add",
  "warm-up": "Warm-up",
  "warm-on": "Warm-up turned on",
  "warm-off": "Warm-up turned off",
  "warm-range": "Warm-up hours changed",
};
const outcomes: Record<string, string> = { ok: "Done" };
const callers = { app: "Pitboard app", cli: "Command line" };

// As each app dates a change: "13 Oct 2026 at 20:49" on macOS, in the en-GB regional format's "g"
// on Windows, "13/10/2026 20:49", and on Linux as local_time.rs gives GNOME's English form on the
// 24-hour clock, "Oct 13, 2026, 20:49".
const dated = (at: number, platform: Platform) => {
  const d = local(at);
  if (platform === "windows") return `${shortDate(at)} ${hm(at)}`;
  if (platform === "linux")
    return `${months[d.getUTCMonth()]} ${d.getUTCDate()}, ${d.getUTCFullYear()}, ${hm(at)}`;
  return `${d.getUTCDate()} ${months[d.getUTCMonth()]} ${d.getUTCFullYear()} at ${hm(at)}`;
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

// The rest of present/words/machine.rs `change_verb` and `change_outcome`, and its `readable` for a
// code they have no words for.
const moreVerbs: Record<string, string> = {
  switch: "Switch",
  "auto-switch": "Automatic switch",
  "auto-stay": "Automatic switch not made",
  forget: "Remove",
  rename: "Rename",
  renew: "Sign-in kept fresh",
  abandon: "Cancel a switch",
  repair: "Repair",
  adopt: "Adopt",
  uninstall: "Uninstall",
  "in-use": "Sign-in in use changed",
  stow: "Clean up a leftover sign-in file",
  park: "Sign out for a sign-in",
};
const moreOutcomes: Record<string, string> = {
  second_sign_in: "Second sign-in",
  warm_up_not_started: "Warm-up didn’t start",
  warm_up_limit_reached: "Limit reached",
  warm_up_login_refused: "Sign-in refused",
  parked_login_expired: "Sign-in expired",
  parked_login_refused: "Sign-in no longer works",
  nothing_parked: "Not signed in",
  already_enrolled: "Already added",
  live_account_not_enrolled: "Account in use not added",
  login_replaced: "Sign-in replaced",
  no_login: "No sign-in",
  already_parked: "Already saved",
  parked_now: "Saved",
  park_kept: "Saved sign-in kept",
  warm_up_unreachable: "Anthropic out of reach",
  warm_up_timed_out: "Took too long",
  warm_up_failed: "Claude Code failed",
};
const readable = (code: string) => capitalised(code.replaceAll("_", " "));
export const changeVerb = (verb: string) => verbs[verb] ?? moreVerbs[verb] ?? readable(verb);
export const changeOutcome = (outcome: string) =>
  outcomes[outcome] ?? moreOutcomes[outcome] ?? readable(outcome);

// The core's words for what switching by itself came to (words.rs `not_switching`,
// `not_watching`, `share_of_limit`).
export const shareOfLimit = (l: Limit) => `${figure(l.percent)} of its ${scopedLimitName(l)} limit`;
// words.rs `listed`: "a", "a and b", "a, b and c".
export const joined = (names: string[]) =>
  names.length < 2 ? names.join("") : `${names.slice(0, -1).join(", ")} and ${names.at(-1)}`;

export function notSwitching(why: Skip, share: number): string {
  switch (why.code) {
    case "no_room": {
      const room = `no other Claude Code account has room below ${share}% in every limit`;
      return why.unread.length
        ? `${room}, and there is no reading of ${joined(why.unread)} from Anthropic yet`
        : room;
    }
    case "already_switched":
      return "this limit was switched away from once already, and is not again before it resets";
    case "settling":
      return `the account was put in use less than ${SETTLING_SECONDS / 60} minutes ago`;
  }
}

export const notWatching = (account: string) =>
  `there is no reading of ${account} from Anthropic yet`;

// present/words/mod.rs `sentence`: a clause as a sentence.
export const sentence = (text: string) => {
  const said = capitalised(text);
  return /[.!?…]$/.test(said) ? said : `${said}.`;
};

// What the line under the setting says (words.rs `watching`, `auto_switched`, `auto_skipped`).
export function autoStandingSaid(standing: AutoStanding): string {
  switch (standing.came) {
    case "watching": {
      let said = `Watching ${standing.account}`;
      if (standing.used) said += `: ${standing.used}`;
      if (standing.asOf !== null) said += `, as of ${hm(standing.asOf)}`;
      return `${said}.`;
    }
    case "switched":
      return `Switched Claude Code from ${standing.from} to ${standing.to}: ${standing.from} has used ${standing.used}.`;
    case "skipped": {
      const said = `${standing.from} has used ${standing.used}. Pitboard is not switching: ${standing.why}.`;
      return standing.until === null ? said : `${said} It decides again at ${hm(standing.until)}.`;
    }
    case "not-watching":
      return `Pitboard is not switching Claude Code: ${standing.why}.`;
  }
}

// present/machine.rs `at_limit`'s note under the choice.
export function atLimitNote(p: Preferences): string | null {
  switch (p.atLimit) {
    case "nothing":
      return null;
    case "notify":
      return `Pitboard notifies you once a limit of the account in use reaches ${p.share}%, while the app is open.`;
    case "switch":
      return `Pitboard switches Claude Code at ${p.share}%, and its running sessions follow within about ${ADOPTION_CEILING_SECONDS} seconds.`;
  }
}

// words.rs `after_a_switch`, for what a switch means to what already runs.
export function afterSwitch(tool: Tool, sessions: string): string {
  switch (tool) {
    case "claude":
      return `${sessions} already running follow within ${ADOPTION_CEILING_SECONDS} seconds`;
    case "codex":
      return "Restart any running `codex` for this to take effect";
    case "desktop":
      return "Claude opens signed in to it";
  }
}

// words.rs `warm_standing`, with the app's clock or the command line's.
export function warmSaid(standing: WarmStanding, moment: (at: number) => string = hm): string {
  switch (standing.stands) {
    case "due":
      return "warms up now";
    case "running":
      if (standing.next === standing.until) return `warms up again at ${moment(standing.next)}`;
      return standing.next === null
        ? `has its 5-hour period running until ${moment(standing.until)}`
        : `has its 5-hour period running until ${moment(standing.until)}, and warms up again at ${moment(standing.next)}`;
    case "resting":
      return `warms up at ${moment(standing.at)}`;
    case "held":
      switch (standing.why) {
        case "no_reading":
          return "waits for a reading from Anthropic";
        case "nothing_to_start":
          return "has no 5-hour limit to start";
        case "parked_login_expired":
          return "needs signing in again: its parked login has expired";
        case "limit_used_up": {
          const l = standing.limit!;
          const name = scopedLimitName(l);
          return l.resetsAt !== null
            ? `has its ${name} limit used up until ${moment(l.resetsAt)}`
            : `has its ${name} limit used up`;
        }
      }
  }
}

// present/words/machine.rs `warm_standing`: the core's words, but for a saved sign-in past its
// life.
export const appWarmSaid = (standing: WarmStanding) =>
  standing.stands === "held" && standing.why === "parked_login_expired"
    ? "can’t warm up: its sign-in expired"
    : warmSaid(standing);

// time.rs `moment`: "20:51" today, "Oct 14 05:53" on another day.
export function moment(at: number, now: number): string {
  const day = (t: number) => Math.floor((t + ZONE) / DAY);
  const d = new Date((at + ZONE) * 1000);
  const two = (n: number) => String(n).padStart(2, "0");
  const hm = `${two(d.getUTCHours())}:${two(d.getUTCMinutes())}`;
  return day(at) === day(now) ? hm : `${months[d.getUTCMonth()]} ${d.getUTCDate()} ${hm}`;
}
