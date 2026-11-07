// The one state every live figure reads, as pitboard-model keeps it, and the core's rules that
// change it: crates/pitboard-core/src/duty/autoswitch.rs decides switches, the model's
// model/advice.rs notifications, duty/warm warm-ups. Figures subscribe to `model`; the actions
// only write.
import { map } from "nanostores";
import {
  DAY,
  HOUR,
  WEEK,
  MINUTE,
  ZONE,
  accounts as drawn,
  clock,
  inUse as drawnInUse,
  logged,
  openWindows,
  share as defaultShare,
  tools,
  warmUp as drawnWarmUp,
  type Account,
  type Limit,
  type Logged,
  type Tool,
  type World,
} from "../data/world.ts";
import {
  ADOPTION_CEILING_SECONDS,
  SETTLING_SECONDS,
  SWITCH_BACK,
  appWarmSaid,
  capitalised,
  figure,
  fiveHoursUsed,
  hasUsedShare,
  hm,
  limitNotification,
  otherUsed,
  resets,
  resetsFirstSaid,
  afterSwitch,
  notSwitching,
  notWatching,
  sentence,
  shareOfLimit,
  switchButton,
  titled,
  eachFront,
  whole,
  type Said,
} from "./words.ts";
import type { System } from "./platform.ts";

export { ADOPTION_CEILING_SECONDS, SETTLING_SECONDS };

// autoswitch.rs `Threshold`; advice.rs `FULL`; usage/mod.rs `SAME_RESET`; duty/cadence.rs's reads
// and looks.
export const LOWEST = 50;
export const HIGHEST = 99;
export const FULL = 100;
export const SAME_RESET = 60;
export const READ_EVERY_SECONDS = 300;
export const DECIDE_EVERY_SECONDS = 30;
// warm/decide.rs: a period runs five hours from the start of its first request's ten minutes.
export const PERIOD = 5 * HOUR;
export const START_STEP = 10 * MINUTE;
// claude/assumptions.rs: a sign-in's refresh token lasts 30 days where the answer gives none.
export const SIGN_IN_LASTS = 30 * DAY;

export type AtLimit = "nothing" | "notify" | "switch";
// usage/order.rs `Order`, by its code
export type Order = "most-left" | "resets-soonest";
export type Caller = "app" | "cli";

export interface Preferences {
  atLimit: AtLimit;
  // whether anybody chose, which hides the setup step that asks (present/setup.rs)
  chosen: boolean;
  share: number;
  weeklyShare?: number;
  weeklyOff?: boolean;
  order: Order;
}

export interface LastSwitch {
  from: string;
  to: string;
  at: number;
  // when sessions already open are sure to follow: Claude Code's alone
  followsAt: number | null;
  // a switch Pitboard made by itself offers Switch Back, with when the limit it left resets
  back?: { resetsAt: number | null };
  // and why it made it
  because?: Because;
}

// launch.rs `Because`: what was used of the limit at the share, or each week's reset before it
export type Because = { used: string } | { to: number | null; from: number | null };

// advice.rs `Advice`: an account in use has used a limit up, and another has room.
export interface Advice {
  tool: Tool;
  ran: string;
  limit: Limit;
  instead: string;
  left: number | null;
}

// present/notices.rs `Notice`, as the system's notification shows it: its button switches to
// an account, Switch at a limit and Switch Back after a switch Pitboard made by itself.
export interface Posted {
  id: string;
  at: number;
  title: Said;
  subtitle: string | null;
  body: string;
  button: { title: Said; tool: Tool; label: string } | null;
  opens: "accounts" | "limits";
}

// What switching Claude Code by itself last came to (ffi `AutoStanding`).
export type AutoStanding =
  | { came: "watching"; account: string; used: string | null; asOf: number | null }
  | { came: "switched"; from: string; to: string; because: Because }
  | { came: "skipped"; from: string; used: string; why: string; until: number | null }
  | { came: "not-watching"; why: string };

export interface State extends World {
  // something was done on the page since the start or Reset, which time passing is not
  changed: boolean;
  // the app runs: Quit stops it until Reset
  running: boolean;
  // when each account was last put in use, by id (state::Account::last_used_at)
  lastUsedAt: Record<string, number>;
  preferences: Preferences;
  // each limit notified, by tool, account id, kind and scope, at its reset (advice::Told)
  told: Record<string, number>;
  toldThisLaunch: Record<string, number>;
  advice: Advice[];
  lastSwitch: Partial<Record<Tool, LastSwitch>>;
  // a sign-in that put a new login in use; one inside Claude to an account not in use before
  // opens Claude signed in to it, which is said as a switch
  newLogin: Partial<Record<Tool, { label: string; at: number; added?: boolean; opens?: boolean }>>;
  // the account Switch Back went back to, about which nothing says Claude Code was not switched
  // for settling or for having switched once until the limit left resets (state.rs `SwitchedBack`)
  switchedBack: { from: string; until: number | null } | null;
  // limits switched away from, by account id, kind and scope, at their reset
  ledger: Record<string, number>;
  // autoswitch.rs `Ledger::early`: by account moved to, its week's reset or 0, and when
  early: Record<string, { week: number; at: number }>;
  // reasons not to switch already told, until the next switch
  autoTold: string[];
  autoStanding: AutoStanding | null;
  // warm/mod.rs `Range` in minutes of the day, the accounts on by id, and when each last warmed
  warm: { from: number; to: number; on: string[]; started: Record<string, number> };
  watchService: { at: number; order: Order; weekly?: number | null; switching?: boolean } | null;
  // a `pitboard watch` left running in a terminal, at the share it switches at, in its order
  watching: { at: number; order: Order; weekly?: number | null; switching?: boolean } | null;
  // the daily renewal schedule `pitboard schedule install` puts in place
  schedule: boolean;
  log: Logged[];
  posted: Posted[];
  // accounts with a window of their site open, by id
  windowsOpen: string[];
  // the claude.ai link shared from a browser that waits in macOS's Open Link window for an
  // account, until one is chosen or the window goes (windows.rs `arrival`)
  linkWaiting: boolean;
  // the Linux app's window, which a status item keeps Pitboard running without: Close puts it
  // away and the tray's Open Pitboard brings it back (application.rs `activate`)
  windowShown: boolean;
  decidedAt: number;
  // a switch or a read the app has under way: "Switching…" and "Reading…" while it runs
  switching: { id: string; token: number } | null;
  reading: number | null;
  // a sign-in the app has under way, Claude Desktop's saving the account in use first; a new
  // account's label is picked once it is added
  signing: {
    tool: Tool;
    label: string | null;
    again: boolean;
    phase: "saving" | "waiting";
    token: number;
  } | null;
  // the account just added, which Name This Account offers to rename (present/sheets.rs `Name`)
  naming: { tool: Tool; label: string; email: string } | null;
  // the tools whose step asking for a second account somebody put off (present/setup.rs)
  declinedSecond: Tool[];
  // Settings' General, which is each app's own and not the core's: opening at sign-in, what the
  // macOS menu bar item shows, and macOS's updater, each at its default (reference/settings.mdx)
  general: {
    opensAtLogin: boolean;
    menuBarShows: "nameAndUsage" | "usage" | "icon";
    checksForUpdates: boolean;
    installsUpdates: boolean;
  };
}

const minutes = (hm: string) => {
  const [h, m] = hm.split(":").map(Number);
  return h * 60 + m;
};

// The hero's first frame: personal in use at 20:51, the app's defaults chosen, and the log
// without the switch to work the demo makes.
export function start(): State {
  const accounts = structuredClone(drawn);
  const id = (label: string) => accounts.find((a) => a.label === label)!.id;
  return {
    now: clock.menuBar,
    readAt: clock.read,
    accounts,
    inUse: { ...drawnInUse.before },
    changed: false,
    running: true,
    lastUsedAt: {},
    preferences: { atLimit: "notify", chosen: true, share: defaultShare, order: "most-left" },
    told: {},
    toldThisLaunch: {},
    advice: [],
    lastSwitch: {},
    newLogin: {},
    switchedBack: null,
    ledger: {},
    early: {},
    autoTold: [],
    autoStanding: null,
    warm: {
      from: minutes(drawnWarmUp.from),
      to: minutes(drawnWarmUp.to),
      on: drawnWarmUp.accounts.map(id),
      started: { [id("personal")]: clock.warmed },
    },
    watchService: null,
    watching: null,
    schedule: false,
    log: logged.filter((e) => e.at !== clock.switched).map((e) => ({ ...e })),
    posted: [],
    windowsOpen: openWindows.map(id),
    linkWaiting: true,
    windowShown: true,
    decidedAt: clock.menuBar,
    switching: null,
    reading: null,
    signing: null,
    naming: null,
    declinedSecond: [],
    general: {
      opensAtLogin: false,
      menuBarShows: "nameAndUsage",
      checksForUpdates: true,
      installsUpdates: false,
    },
  };
}

export const model = map<State>(start());

// Every action changes a copy and puts it in the store whole, so a view sees one change.
function change<T>(make: (s: State) => T, done = true): T {
  const s = structuredClone(model.get());
  const out = make(s);
  if (done) s.changed = true;
  model.set(s);
  return out;
}

// state::Key::typed: a Claude Code account by its label, any other with its tool.
export const typed = (a: { tool: Tool; label: string }) =>
  a.tool === "claude" ? a.label : `${a.tool}/${a.label}`;

const toolName = (tool: Tool) => tools.find(([t]) => t === tool)![1];
const find = (s: State, tool: Tool, label: string) =>
  s.accounts.find((a) => a.tool === tool && a.label === label);
const current = (s: State, tool: Tool) => find(s, tool, s.inUse[tool]);

function record(s: State, caller: Caller, verb: string, subject: string, outcome = "ok") {
  s.log.push({ at: s.now, caller, verb, subject, outcome });
}

// usage/mod.rs `Window::used`: a limit past its reset counts as reset before a reading says so.
export const used = (l: Limit, now: number) =>
  l.resetsAt !== null && l.resetsAt <= now ? 0 : l.percent;
const usedUp = (l: Limit, now: number) => used(l, now) >= 100;
const reached = (percent: number, share: number) => whole(percent) >= share;
export const sameReset = (a: number, b: number) => Math.abs(a - b) < SAME_RESET;

// usage::LimitKind: explicit service names where present, otherwise the drawn period.
const kind = (l: Limit) =>
  l.kind === "five_hour"
    ? "session"
    : l.kind === "seven_day"
      ? "weekly_all"
      : (l.kind ??
        (l.lengthSeconds === 5 * HOUR
          ? "session"
          : l.lengthSeconds === WEEK
            ? l.scope
              ? "weekly_scoped"
              : "weekly_all"
            : `period_${l.lengthSeconds}`));
export const weeklyShare = (p: Pick<Preferences, "share" | "weeklyShare">) =>
  p.weeklyShare ?? p.share;
export const weeklyThreshold = (p: Pick<Preferences, "share" | "weeklyShare" | "weeklyOff">) =>
  p.weeklyOff ? null : weeklyShare(p);
export const limitShare = (l: Limit, share: number, weekly: number | null) =>
  kind(l).startsWith("weekly_") ? weekly : share;
const meets = (l: Limit, now: number, share: number, weekly: number | null) => {
  const at = limitShare(l, share, weekly);
  return at !== null && reached(used(l, now), at);
};
const sameLimit = (a: Limit, b: Limit) => kind(a) === kind(b) && a.scope === b.scope;
const limitKey = (tool: Tool, id: string, l: Limit) => `${tool}/${id}/${kind(l)}/${l.scope ?? ""}`;

// Anthropic's answer lists every limit an account has; OpenAI's may leave some out (codex/api.rs).
const listsEveryLimit = (tool: Tool) => tool === "claude";

// A parked login that still renews (ParkedLogin::restorable_at).
const switchable = (a: Account, now: number) =>
  a.refreshExpiresAt === undefined || a.refreshExpiresAt > now;

// usage/order.rs `Week`: when what an account has left of its week is lost, in epoch seconds
export type Week = number | "free" | "unknown";

// usage/order.rs `Week::of`: a reading that may leave limits out says nothing of one it leaves out
export function weekOf(limits: Limit[], everyLimit: boolean, now: number): Week {
  const week = limits.find((l) => !l.scope && kind(l) === "weekly_all");
  if (!week) return everyLimit ? "free" : "unknown";
  return week.resetsAt !== null && week.resetsAt > now ? week.resetsAt : "unknown";
}
export const resetsAtOf = (week: Week) => (typeof week === "number" ? week : null);

// usage/order.rs `Week::comes_before`: never between two without a week, nor with one unknown
export const comesBefore = (a: Week, b: Week) =>
  (a === "free" && b !== "free") ||
  (typeof a === "number" && typeof b === "number" && a < b && !sameReset(a, b));

// usage/order.rs `Week::free_first` and `Week::sooner`
const freeFirst = (a: Week, b: Week) => Number(a !== "free") - Number(b !== "free");
const rank = (week: Week) => (week === "free" ? 0 : week === "unknown" ? 2 : 1);
const sooner = (a: Week, b: Week) =>
  typeof a === "number" && typeof b === "number"
    ? sameReset(a, b)
      ? 0
      : a - b
    : rank(a) - rank(b);

interface Room {
  used: number | null;
  fullest: number;
  week: Week;
}

// usage/order.rs `below`: room below the share, away from `limit` where there is one
function below(
  theirs: Limit[],
  everyLimit: boolean,
  mine: Limit[],
  limit: Limit | null,
  share: number,
  now: number,
  weekly: number | null = share,
): Room | null {
  if (theirs.length === 0 || theirs.some((l) => usedUp(l, now) || meets(l, now, share, weekly)))
    return null;
  const given = (had: Limit) => theirs.find((l) => sameLimit(l, had));
  const gives = (k: string) => theirs.some((l) => !l.scope && kind(l) === k);
  const other = (l: Limit) => !limit || !sameLimit(l, limit);
  if (!everyLimit) {
    const overall = gives("session") && gives("weekly_all");
    const unknown = (had: Limit) => !given(had) && !(overall && had.scope && other(had));
    if ([...(limit ? [limit] : []), ...mine].some(unknown)) return null;
  }
  const fullest = theirs.filter(other).reduce((most, l) => Math.max(most, used(l, now)), 0);
  const of = limit && given(limit);
  return { used: of ? used(of, now) : null, fullest, week: weekOf(theirs, everyLimit, now) };
}

// usage/order.rs `room`
export const room = (
  theirs: Limit[],
  everyLimit: boolean,
  mine: Limit[],
  limit: Limit,
  share: number,
  now: number,
  weekly: number | null = share,
) => below(theirs, everyLimit, mine, limit, share, now, weekly);

// usage/order.rs `FRESH`: budget.rs `UNREACHABLE_MOST`
export const FRESH = 900;

// usage/order.rs `five_hours`
export const fiveHours = (limits: Limit[], now: number) =>
  limits
    .filter((l) => !l.scope && kind(l) === "session")
    .reduce((most, l) => Math.max(most, used(l, now)), 0);

// usage/order.rs `resets_first`, of a reading the page read at `readAt`
export function resetsFirst(
  theirs: Limit[],
  mine: Limit[],
  everyLimit: boolean,
  share: number,
  now: number,
  readAt: number,
  weekly: number | null = share,
): Room | null {
  const fresh = now - readAt <= FRESH;
  const first = comesBefore(weekOf(theirs, everyLimit, now), weekOf(mine, everyLimit, now));
  if (!fresh || !first || fiveHours(theirs, now) > fiveHours(mine, now)) return null;
  return below(theirs, everyLimit, mine, null, share, now, weekly);
}

// usage/order.rs `preferred`: the first of those alike
export function preferred<T>(order: Order, candidates: [T, Room][]): [T, Room] | null {
  let best: [T, Room] | null = null;
  for (const c of candidates) {
    if (!best) {
      best = c;
      continue;
    }
    const [, a] = c;
    const [, b] = best;
    const by =
      (order === "most-left" ? freeFirst(a.week, b.week) : sooner(a.week, b.week)) ||
      (a.used ?? 0) - (b.used ?? 0) ||
      a.fullest - b.fullest;
    if (by < 0) best = c;
  }
  return best;
}

export type Skip =
  | { code: "no_room"; unread: string[] }
  | { code: "already_switched" }
  | { code: "settling"; until: number };

export type Decision =
  | { decided: "no-reading" }
  | { decided: "below"; nearest: Limit | null; asOf: number }
  | { decided: "hold"; limit: Limit; why: Skip }
  | { decided: "switch"; to: Account; limit: Limit }
  // autoswitch.rs `Why::ResetsFirst`: a move before the share, by each account's week
  | { decided: "switch"; to: Account; limit: null; first: { to: Week; from: Week } };

// The fuller of two limits first, then the one resetting sooner.
const fuller = (a: Limit, b: Limit, now: number) =>
  used(b, now) - used(a, now) || (a.resetsAt ?? Infinity) - (b.resetsAt ?? Infinity);

// autoswitch.rs `goes`: the other Claude Code accounts that can be switched to
const goes = (s: State, from: Account) =>
  s.accounts.filter((a) => a.tool === "claude" && a !== from && switchable(a, s.now));

// autoswitch.rs `settling`: until when the account in use settles, where it was put in use lately
function settling(s: State, from: Account): number | null {
  const putInUse = s.lastUsedAt[from.id];
  if (putInUse === undefined || s.now - putInUse < 0 || s.now - putInUse >= SETTLING_SECONDS)
    return null;
  return putInUse + SETTLING_SECONDS;
}

// autoswitch.rs `Ledger::open_early`: an entry of an earlier week, or no longer kept, is not one
function openEarly(s: State, id: string, week: Week): boolean {
  const entry = s.early[id];
  if (!entry) return true;
  const kept =
    entry.week > s.now ||
    sameReset(entry.week, s.now) ||
    (entry.week === 0 && s.now - entry.at < 7 * DAY);
  return !kept || !sameReset(entry.week, resetsAtOf(week) ?? 0);
}

// autoswitch.rs `early`: a switch on the page is never refused, so no early move waits
function early(
  s: State,
  from: Account,
  share: number,
  order: Order,
  weekly: number | null,
): Decision | null {
  if (order !== "resets-soonest" || settling(s, from) !== null) return null;
  const best = preferred(
    "resets-soonest",
    goes(s, from).flatMap((a): [Account, Room][] => {
      if (a.unread) return [];
      const r = resetsFirst(a.limits, from.limits, true, share, s.now, s.readAt, weekly);
      return r && openEarly(s, a.id, r.week) ? [[a, r]] : [];
    }),
  );
  if (!best) return null;
  const [to, r] = best;
  const first = { to: r.week, from: weekOf(from.limits, true, s.now) };
  return { decided: "switch", to, limit: null, first };
}

// autoswitch.rs `decide`, for the Claude Code account in use at `share`, in `order`.
export function decide(
  s: State,
  share: number,
  order: Order = "most-left",
  weekly: number | null = share,
): Decision {
  const now = s.now;
  const from = current(s, "claude");
  if (!from || from.unread) return { decided: "no-reading" };
  const at = (l: Limit) => s.ledger[limitKey("claude", from.id, l)];
  const left = (l: Limit) => {
    const was = at(l);
    return was !== undefined && sameReset(was, l.resetsAt ?? 0);
  };
  const reachedLimits = from.limits
    .filter((l) => meets(l, now, share, weekly))
    .sort((a, b) => fuller(a, b, now));
  if (reachedLimits.length === 0) {
    const moved = early(s, from, share, order, weekly);
    if (moved) return moved;
    const running = from.limits
      .filter((l) => l.resetsAt === null || l.resetsAt > now)
      .sort((a, b) => fuller(a, b, now));
    return { decided: "below", nearest: running[0] ?? null, asOf: s.readAt };
  }
  const limit = reachedLimits.find((l) => !left(l));
  if (!limit)
    return { decided: "hold", limit: reachedLimits[0], why: { code: "already_switched" } };
  const until = settling(s, from);
  if (until !== null) return { decided: "hold", limit, why: { code: "settling", until } };
  const best = preferred(
    order,
    goes(s, from).flatMap((a): [Account, Room][] => {
      if (a.unread) return [];
      const r = room(a.limits, listsEveryLimit("claude"), from.limits, limit, share, now, weekly);
      return r ? [[a, r]] : [];
    }),
  );
  if (best) return { decided: "switch", to: best[0], limit };
  return {
    decided: "hold",
    limit,
    why: {
      code: "no_room",
      unread: goes(s, from)
        .filter((a) => a.unread)
        .map(typed),
    },
  };
}

function post(s: State, notice: Omit<Posted, "at">) {
  if (!s.running) return;
  s.posted.push({ ...notice, at: s.now });
}

// advice.rs `spare`: the account of the same tool there is room on for a switch away from `ran`.
function spare(s: State, from: Account, ran: Limit, share: number, weekly: number | null = share) {
  const best = preferred(
    s.preferences.order,
    s.accounts.flatMap((a): [Account, Room][] => {
      if (a.tool !== from.tool || a === from || a.unread || !switchable(a, s.now)) return [];
      const r = room(a.limits, listsEveryLimit(a.tool), from.limits, ran, share, s.now, weekly);
      return r ? [[a, r]] : [];
    }),
  );
  return (
    best && { account: best[0], left: best[1].used === null ? null : 100 - whole(best[1].used) }
  );
}

// The account in use of each tool that reads limits, in the order the tools are listed.
const readTools = (s: State) =>
  tools.flatMap(([tool]) => {
    const a = current(s, tool);
    return a && tool !== "desktop" ? [a] : [];
  });

// state.rs `advise`: the window's advice at 100%, and a notification once a limit reaches the
// share (advice.rs `HeadsUp::about`, notices.rs `at_limit_notice`), one limit per tool a pass.
function advise(s: State) {
  const now = s.now;
  const told = (book: Record<string, number>, a: Account, l: Limit) => {
    const at = book[limitKey(a.tool, a.id, l)];
    return at !== undefined && sameReset(at, l.resetsAt ?? 0);
  };
  const fresh: Advice[] = [];
  for (const a of readTools(s)) {
    for (const l of a.limits.filter((l) => usedUp(l, now))) {
      if (told(s.toldThisLaunch, a, l)) continue;
      const instead = spare(s, a, l, FULL);
      if (!instead) continue;
      s.toldThisLaunch[limitKey(a.tool, a.id, l)] = l.resetsAt ?? 0;
      fresh.push({
        tool: a.tool,
        ran: a.label,
        limit: l,
        instead: instead.account.label,
        left: instead.left,
      });
      break;
    }
  }
  const standing = [
    ...fresh,
    ...s.advice.filter((old) => {
      const a = current(s, old.tool);
      return (
        a?.label === old.ran &&
        a.limits.some(
          (l) =>
            sameLimit(l, old.limit) &&
            usedUp(l, now) &&
            sameReset(l.resetsAt ?? 0, old.limit.resetsAt ?? 0),
        )
      );
    }),
  ];
  s.advice = tools.flatMap(([tool]) => standing.filter((a) => a.tool === tool).slice(0, 1));

  const { atLimit, share } = s.preferences;
  const weekly = weeklyThreshold(s.preferences);
  if (atLimit === "nothing" || !s.running) return;
  for (const a of readTools(s)) {
    const l = a.limits.find((l) => meets(l, now, share, weekly) && !told(s.told, a, l));
    if (!l || (atLimit === "switch" && a.tool === "claude")) continue;
    s.told[limitKey(a.tool, a.id, l)] = l.resetsAt ?? 0;
    const instead = spare(s, a, l, share, weekly);
    const then = instead
      ? otherUsed(instead.account.label, instead.left)
      : l.resetsAt !== null
        ? sentence(resets(l.resetsAt, now))
        : "";
    post(s, {
      id: `${limitKey(a.tool, a.label, l)}-${l.resetsAt ?? 0}`,
      title: limitNotification(l, usedUp(l, now)),
      subtitle: toolName(a.tool),
      body: `${hasUsedShare(a.label, figure(l.percent))} ${then}`.trimEnd(),
      button: instead && {
        title: switchButton(instead.account.label),
        tool: a.tool,
        label: instead.account.label,
      },
      opens: "accounts",
    });
  }
  tellFirst(s);
}

// present/words/notices.rs `resets_first_title`
const RESETS_FIRST = eachFront((f) =>
  titled(f, "Another Account Resets First", "Another account resets first"),
);

// state.rs `tell_first` and advice.rs `First::about`, told once for the week it is about
function tellFirst(s: State) {
  const { atLimit, share, order } = s.preferences;
  const weekly = weeklyThreshold(s.preferences);
  if (order !== "resets-soonest") return;
  const now = s.now;
  for (const a of readTools(s)) {
    if (a.unread || (atLimit === "switch" && a.tool === "claude")) continue;
    if (a.limits.some((l) => meets(l, now, share, weekly))) continue;
    const every = listsEveryLimit(a.tool);
    const best = preferred(
      "resets-soonest",
      s.accounts.flatMap((o): [Account, Room][] => {
        if (o.tool !== a.tool || o === a || o.unread || !switchable(o, now)) return [];
        const r = resetsFirst(o.limits, a.limits, every, share, now, s.readAt, weekly);
        return r ? [[o, r]] : [];
      }),
    );
    if (!best) continue;
    const [instead, r] = best;
    // its own week, or for an account without one, the week of the account in use
    const week = resetsAtOf(r.week) ?? resetsAtOf(weekOf(a.limits, every, now));
    if (week === null) continue;
    const key = `${a.tool}/${instead.label}/first/`;
    const told = s.told[key];
    if (told !== undefined && sameReset(told, week)) continue;
    s.told[key] = week;
    post(s, {
      id: `${key}-${week}`,
      title: RESETS_FIRST,
      subtitle: toolName(a.tool),
      body: `${resetsFirstSaid(instead.label, a.label, r.week !== "free")} ${fiveHoursUsed(figure(fiveHours(instead.limits, now)))}`,
      button: {
        title: switchButton(instead.label),
        tool: a.tool,
        label: instead.label,
      },
      opens: "accounts",
    });
  }
}

// present/words/notices.rs `not_switched` and `not_switching`: the titles of a switch Pitboard did
// not make by itself, and of its not judging one.
const NOT_SWITCHED = eachFront((f) =>
  titled(f, "Automatic Switch Skipped", "Automatic switch skipped"),
);
const NOT_SWITCHING = eachFront((f) =>
  titled(f, "Automatic Switching Paused", "Automatic switching paused"),
);

// state.rs `look_to_switch` and its outcomes: what switching by itself came to, said under the
// setting, and each reason not to said once until the next switch.
function lookToSwitch(s: State) {
  const app = s.running && s.preferences.atLimit === "switch";
  const policy = app
    ? {
        at: s.preferences.share,
        order: s.preferences.order,
        weekly: weeklyThreshold(s.preferences),
        switching: true,
      }
    : s.watchService?.switching === false
      ? s.watching
      : (s.watchService ?? s.watching);
  if (!policy || policy.switching === false) return;
  const share = policy.at;
  const weekly = policy.weekly === undefined ? share : policy.weekly;
  const decision = decide(s, share, policy.order, weekly);
  const from = current(s, "claude");
  // switch/auto.rs `met_a_limit`: the account in use at a limit may be moved to early again
  if (decision.decided === "hold" || (decision.decided === "switch" && decision.limit))
    delete s.early[from!.id];
  const stands = (standing: AutoStanding, told?: string, notice?: Omit<Posted, "at">) => {
    if (!app) return;
    s.autoStanding = standing;
    if (told && notice && !s.autoTold.includes(told)) {
      s.autoTold.push(told);
      post(s, notice);
    }
  };
  switch (decision.decided) {
    case "no-reading": {
      const why = notWatching(from ? typed(from) : "");
      stands({ came: "not-watching", why }, `not-watching/no_reading/${from?.label}`, {
        id: `auto-not-watching/no_reading/${from?.label}`,
        title: NOT_SWITCHING,
        subtitle: toolName("claude"),
        body: sentence(why),
        button: null,
        opens: "limits",
      });
      return;
    }
    case "below":
      stands({
        came: "watching",
        account: from!.label,
        used: decision.nearest && shareOfLimit(decision.nearest),
        asOf: decision.asOf,
      });
      return;
    case "hold": {
      const { limit, why } = decision;
      const key = `skipped/${from!.label}/${kind(limit)}/${limit.scope ?? ""}/${limit.resetsAt ?? 0}/${why.code}`;
      const said = notSwitching(why, share, weekly);
      const back = s.switchedBack;
      if (back?.until != null && back.until <= s.now) s.switchedBack = null;
      const quiet = why.code !== "no_room" && s.switchedBack?.from === from!.label;
      stands(
        {
          came: "skipped",
          from: from!.label,
          used: shareOfLimit(limit),
          why: said,
          until: why.code === "settling" ? why.until : null,
        },
        quiet ? undefined : key,
        {
          id: `auto-${key}`,
          title: NOT_SWITCHED,
          subtitle: toolName("claude"),
          body: `${from!.label} has used ${shareOfLimit(limit)}. ${sentence(said)}`,
          button: null,
          opens: "limits",
        },
      );
      return;
    }
    case "switch": {
      const { to, limit } = decision;
      const was = from!.label;
      const because: Because = limit
        ? { used: shareOfLimit(limit) }
        : { to: resetsAtOf(decision.first.to), from: resetsAtOf(decision.first.from) };
      if (limit) s.ledger[limitKey("claude", from!.id, limit)] = limit.resetsAt ?? 0;
      else s.early[to.id] = { week: resetsAtOf(decision.first.to) ?? 0, at: s.now };
      switchTo(s, to, app ? "app" : "cli", "auto-switch");
      s.lastSwitch.claude!.because = because;
      s.autoTold = [];
      if (!app) return;
      s.switchedBack = null;
      // launch.rs `Because::resets_at`: a move before the share left no limit to reset
      s.lastSwitch.claude!.back = { resetsAt: limit ? limit.resetsAt : null };
      s.autoStanding = { came: "switched", from: was, to: to.label, because };
      // present/notices.rs `auto_switched_notice`
      const said =
        "used" in because
          ? `${was} has used ${because.used}.`
          : resetsFirstSaid(to.label, was, because.to !== null);
      post(s, {
        id: `auto/${was}/${to.label}/${s.now}`,
        title: eachFront(() => `Switched to ${to.label}`),
        subtitle: toolName("claude"),
        body: `${said} ${sentence(afterSwitch("claude", "Sessions"))}`,
        button: { title: SWITCH_BACK, tool: "claude", label: was },
        opens: "accounts",
      });
    }
  }
}

// warm/decide.rs `Standing`
export type WarmStanding =
  | { stands: "running"; until: number; next: number | null }
  | { stands: "resting"; at: number }
  | { stands: "due" }
  | {
      stands: "held";
      why: "no_reading" | "nothing_to_start" | "limit_used_up" | "parked_login_expired";
      until?: number;
      limit?: Limit;
    };

const localMinute = (at: number) => Math.floor(((((at + ZONE) % DAY) + DAY) % DAY) / MINUTE);
const holds = (s: State, at: number) => {
  const m = localMinute(at);
  const { from, to } = s.warm;
  return from < to ? m >= from && m < to : m >= from || m < to;
};
// warm/mod.rs `Range::opens_after`: today's opening, or tomorrow's once it has passed.
const opensAfter = (s: State, at: number) => {
  const midnight = at - ((((at + ZONE) % DAY) + DAY) % DAY);
  const today = midnight + s.warm.from * MINUTE;
  return today > at ? today : today + DAY;
};
const periodEnds = (started: number) => started - (started % START_STEP) + PERIOD;

// warm/decide.rs `decide`, for a warm-up Pitboard makes by itself, or one asked for now.
export function warmStanding(s: State, a: Account, asked: "now" | "by-itself"): WarmStanding {
  const now = s.now;
  if (s.inUse[a.tool] !== a.label && !switchable(a, now))
    return { stands: "held", why: "parked_login_expired" };
  if (a.unread) return { stands: "held", why: "no_reading" };
  const session = a.limits.find((l) => kind(l) === "session" && !l.scope);
  if (!session) return { stands: "held", why: "nothing_to_start" };
  const next = (until: number) => (holds(s, until) ? until : opensAfter(s, until));
  if (session.resetsAt !== null && session.resetsAt > now)
    return { stands: "running", until: session.resetsAt, next: next(session.resetsAt) };
  const started = s.warm.started[a.id];
  if (started !== undefined && started >= s.readAt && periodEnds(started) > now)
    return { stands: "running", until: periodEnds(started), next: next(periodEnds(started)) };
  const spent = a.limits
    .filter((l) => !l.scope && usedUp(l, now))
    .sort((x, y) => (y.resetsAt ?? 0) - (x.resetsAt ?? 0))[0];
  if (spent) return { stands: "held", why: "limit_used_up", limit: spent };
  if (asked === "now") return { stands: "due" };
  if (!holds(s, now)) return { stands: "resting", at: opensAfter(s, now) };
  return { stands: "due" };
}

// The note under an account turned on for warm-ups (present/accounts.rs `warm_note`).
export function warmNote(s: State, a: Account): string | null {
  if (a.tool !== "claude" || !s.warm.on.includes(a.id)) return null;
  return capitalised(appWarmSaid(warmStanding(s, a, "by-itself")));
}

// The line under Warm-Up (present/machine.rs `warm`, warm/mod.rs `Overview::next`).
export function nextWarmUp(s: State): string | null {
  let soonest: [string, number] | null = null;
  for (const a of s.accounts.filter((a) => a.tool === "claude" && s.warm.on.includes(a.id))) {
    const standing = warmStanding(s, a, "by-itself");
    const at =
      standing.stands === "due"
        ? s.now
        : standing.stands === "running"
          ? standing.next
          : standing.stands === "resting"
            ? standing.at
            : null;
    if (at !== null && (!soonest || at < soonest[1])) soonest = [a.label, at];
  }
  if (!soonest) return null;
  const [account, at] = soonest;
  return at <= s.now ? `Warming up ${account} now.` : `Next warm-up: ${account} at ${hm(at)}.`;
}

function warmUpNow(s: State, a: Account, caller: Caller) {
  s.warm.started[a.id] = s.now;
  record(s, caller, "warm-up", typed(a));
}

// state.rs `warm_tick`: whichever account turned on is due, while the app or the watch runs.
function warmTick(s: State) {
  if (!s.running && !s.watchService && s.watching === null) return;
  for (const a of s.accounts.filter((a) => a.tool === "claude" && s.warm.on.includes(a.id))) {
    if (warmStanding(s, a, "by-itself").stands === "due")
      warmUpNow(s, a, s.running ? "app" : "cli");
  }
}

// What a read finds: nothing on the page uses an account, so each limit stands as it was until
// it resets, and then starts again at nothing used. A Claude Code five hours starts only with a
// request, such as a warm-up's; any other limit runs on its own cycle.
function readUsage(s: State) {
  const now = s.now;
  s.readAt = now;
  for (const a of s.accounts.filter((a) => a.tool !== "desktop")) {
    if (a.unread) {
      const like = current(s, a.tool) ?? a;
      a.limits = like.limits.map((l) => ({
        lengthSeconds: l.lengthSeconds,
        scope: l.scope,
        percent: 0,
        resetsAt: a.tool === "claude" && kind(l) === "session" ? null : now + l.lengthSeconds,
      }));
      delete a.unread;
      continue;
    }
    for (const l of a.limits) {
      const opens = a.tool === "codex" && l.percent === 0;
      if (l.resetsAt !== null && l.resetsAt > now && !opens) continue;
      if (opens) {
        l.resetsAt = now + l.lengthSeconds;
      } else if (a.tool === "claude" && kind(l) === "session") {
        l.percent = 0;
        l.resetsAt = null;
      } else if (l.resetsAt !== null) {
        l.percent = 0;
        while (l.resetsAt <= now) l.resetsAt += l.lengthSeconds;
      }
    }
    const started = s.warm.started[a.id];
    const session = a.limits.find((l) => kind(l) === "session" && !l.scope);
    if (session && session.resetsAt === null && started !== undefined && periodEnds(started) > now)
      session.resetsAt = periodEnds(started);
  }
  advise(s);
  lookToSwitch(s);
  warmTick(s);
}

// A state of its own as a read leaves it, advised on and looked at as the app does after a read:
// the notification row's own moment, which the store never holds.
export function advised(state: State): State {
  const s = structuredClone(state);
  advise(s);
  lookToSwitch(s);
  return s;
}

function decidePass(s: State) {
  s.decidedAt = s.now;
  lookToSwitch(s);
  warmTick(s);
}

// A switch, as switch.rs makes it and service.rs records it.
function switchTo(s: State, to: Account, caller: Caller, verb = "use") {
  const from = current(s, to.tool);
  s.inUse[to.tool] = to.label;
  s.lastUsedAt[to.id] = s.now;
  if (to.tool === "desktop") to.readAt = s.now;
  s.lastSwitch[to.tool] = {
    from: from?.label ?? "",
    to: to.label,
    at: s.now,
    followsAt: to.tool === "claude" ? s.now + ADOPTION_CEILING_SECONDS : null,
  };
  delete s.newLogin[to.tool];
  s.advice = s.advice.filter((a) => a.tool !== to.tool);
  record(s, caller, verb, typed(to));
}

// Every action that changes what an account stands at is advised on, as a change the app made.
function afterChange(s: State) {
  advise(s);
  lookToSwitch(s);
}

export type Outcome =
  | "ok"
  | "already_active"
  | "account_unknown"
  | "label_taken"
  | "invalid_label"
  | "cannot_forget_active_account"
  | "parked_login_expired";

// state.rs `switch`: a row's Switch, a menu's account, a notification's Switch, `pitboard use`.
export function use(tool: Tool, label: string, caller: Caller = "app"): Outcome {
  return change((s) => {
    const to = find(s, tool, label);
    if (!to) {
      record(s, caller, "use", tool === "claude" ? label : `${tool}/${label}`, "account_unknown");
      return "account_unknown";
    }
    if (s.inUse[tool] === label) {
      record(s, caller, "use", typed(to), "already_active");
      return "already_active";
    }
    if (!switchable(to, s.now)) {
      record(s, caller, "use", typed(to), "parked_login_expired");
      return "parked_login_expired";
    }
    switchTo(s, to, caller);
    afterChange(s);
    return "ok";
  });
}

// label.rs `check`: one word, with no tool's separator in it.
export function nameProblem(name: string): "empty" | "not_one_word" | "separator" | null {
  if (name === "") return "empty";
  if (/[\s\p{Cc}]/u.test(name)) return "not_one_word";
  if (name.includes("/")) return "separator";
  return null;
}

export function rename(tool: Tool, from: string, to: string, caller: Caller = "app"): Outcome {
  return change((s) => {
    const a = find(s, tool, from);
    const subject = tool === "claude" ? from : `${tool}/${from}`;
    if (!a) {
      record(s, caller, "rename", subject, "account_unknown");
      return "account_unknown";
    }
    if (nameProblem(to)) {
      record(s, caller, "rename", subject, "invalid_label");
      return "invalid_label";
    }
    // service.rs logs a rename under both names
    const both = `${typed(a)} -> ${to}`;
    if (to !== from && find(s, tool, to)) {
      record(s, caller, "rename", both, "label_taken");
      return "label_taken";
    }
    if (to === from) {
      record(s, caller, "rename", both);
      return "ok";
    }
    a.label = to;
    if (s.inUse[tool] === from) s.inUse[tool] = to;
    const last = s.lastSwitch[tool];
    if (last?.to === from) last.to = to;
    if (last?.from === from) last.from = to;
    if (s.newLogin[tool]?.label === from) s.newLogin[tool]!.label = to;
    for (const advice of s.advice.filter((x) => x.tool === tool)) {
      if (advice.ran === from) advice.ran = to;
      if (advice.instead === from) advice.instead = to;
    }
    record(s, caller, "rename", both);
    return "ok";
  });
}

export function forget(tool: Tool, label: string, caller: Caller = "app"): Outcome {
  return change((s) => {
    const a = find(s, tool, label);
    const subject = tool === "claude" ? label : `${tool}/${label}`;
    if (!a) {
      record(s, caller, "forget", subject, "account_unknown");
      return "account_unknown";
    }
    if (s.inUse[tool] === label) {
      record(s, caller, "forget", subject, "cannot_forget_active_account");
      return "cannot_forget_active_account";
    }
    s.accounts = s.accounts.filter((x) => x !== a);
    s.warm.on = s.warm.on.filter((id) => id !== a.id);
    s.windowsOpen = s.windowsOpen.filter((id) => id !== a.id);
    s.advice = s.advice.filter((x) => !(x.tool === tool && x.instead === label));
    record(s, caller, "forget", subject);
    return "ok";
  });
}

let enrolled = 0;
// the people the page's sign-ins have signed in as, who each have an email
let people = 0;

// Hex that stands for what a simulated sign-in would bring, which nobody reads: FNV-1a's 64 bits.
function madeUp(seed: string): string {
  let h = 0xcbf29ce484222325n;
  for (const c of seed) h = BigInt.asUintN(64, (h ^ BigInt(c.codePointAt(0)!)) * 0x100000001b3n);
  return h.toString(16).padStart(16, "0");
}
const uuidOf = (seed: string) => {
  const hex = madeUp(`${seed}/a`) + madeUp(`${seed}/b`);
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-4${hex.slice(13, 16)}-a${hex.slice(17, 20)}-${hex.slice(20, 32)}`;
};

// label.rs `MAIL_SERVICES`: anyone can have an address at one of these, so the domain says
// nothing of whose it is.
const MAIL_SERVICES = new Set(
  "163 aol fastmail gmail gmx googlemail hey hotmail icloud live mac mail me msn naver outlook pm proton protonmail qq web yahoo yandex zoho".split(
    " ",
  ),
);

// label.rs `suggest`: a name no account of the tool has, from the email's company and then its
// mailbox, or the tool's code where it gives neither, and the first of them numbered after.
export function suggest(s: State, tool: Tool, email: string): string {
  const lower = email.toLowerCase();
  const at = lower.lastIndexOf("@");
  const [local, domain] = at < 0 ? ["", ""] : [lower.slice(0, at), lower.slice(at + 1)];
  const mailbox = local.split("+")[0];
  const company = domain.split(".")[0];
  const names = [MAIL_SERVICES.has(company) ? null : company, mailbox].filter(
    (n): n is string => !!n && nameProblem(n) === null,
  );
  if (names.length === 0) names.push(tool);
  const free = (name: string) => !find(s, tool, name);
  for (const name of names) if (free(name)) return name;
  for (let n = 2; ; n++) if (free(`${names[0]}-${n}`)) return `${names[0]}-${n}`;
}

// A simulated sign-in's end, for Add Account: the tool's own sign-in is never loaded, so it signs
// in as somebody new, dana@travel.example as the apps' fixture does and a number after dana for
// each one after, on the plan of the account in use, and its first read finds nothing used. The
// core adds the account under the name it picks, and Name This Account offers to change it.
// Claude Desktop's ends with Claude signed in to it, which has no email.
export function add(tool: Tool, caller: Caller = "app"): string {
  return change((s) => {
    const n = ++enrolled;
    const person = tool === "desktop" ? 0 : ++people;
    const email = person ? `dana${person > 1 ? person : ""}@travel.example` : "";
    const label = suggest(s, tool, email);
    const like = current(s, tool);
    const a: Account = {
      id: `new-${n}`,
      tool,
      label,
      email,
      plan: like?.plan ?? null,
      limits: [],
      uuid: uuidOf(`${tool}/${label}/${n}`),
      login: madeUp(`refresh/${tool}/${label}/${n}`),
    };
    if (tool === "claude") a.org = uuidOf(`org/${label}/${n}`);
    s.naming = { tool, label, email };
    if (tool === "desktop") {
      s.accounts.push(a);
      switchTo(s, a, caller, "enroll");
      s.newLogin.desktop = { label, at: s.now, added: true, opens: true };
      return label;
    }
    a.unread = true;
    // codex/engine.rs `expiry`: Codex states no refresh lifetime, so its parks keep none
    if (tool === "claude") a.refreshExpiresAt = s.now + SIGN_IN_LASTS;
    s.accounts.push(a);
    record(s, caller, "enroll", typed(a));
    return label;
  });
}

// Name This Account put away, by Done or Cancel: the account keeps the name it was added under,
// or Done renames it.
export function named() {
  change((s) => {
    s.naming = null;
  }, false);
}

// A simulated sign-in again: a fresh login, parked or in use. Claude Desktop's ends with Claude
// signed in to the account, which puts it in use.
export function signInAgain(tool: Tool, label: string, caller: Caller = "app"): Outcome {
  return change((s) => {
    const a = find(s, tool, label);
    if (!a) return "account_unknown";
    if (tool === "desktop") {
      const opens = s.inUse.desktop !== label;
      if (opens) switchTo(s, a, caller, "enroll");
      else {
        a.readAt = s.now;
        record(s, caller, "enroll", typed(a));
      }
      s.newLogin.desktop = { label, at: s.now, opens };
      return "ok";
    }
    if (tool === "claude") a.refreshExpiresAt = s.now + SIGN_IN_LASTS;
    a.login = madeUp(`refresh/${tool}/${label}/${s.now}`);
    if (s.inUse[tool] === label) s.newLogin[tool] = { label, at: s.now };
    record(s, caller, "enroll", typed(a));
    afterChange(s);
    return "ok";
  });
}

// When a limit reaches: kept at once, then advised on and looked at again (state.rs `at_limit_set`).
export function setChoice(atLimit: AtLimit) {
  change((s) => {
    s.preferences = { ...s.preferences, atLimit, chosen: true };
    syncWatch(s);
    if (atLimit !== "switch") s.autoStanding = null;
    afterChange(s);
  });
}

// Use first: kept at once, then advised on and looked at again (state.rs `order_set`).
export function setOrder(order: Order) {
  change((s) => {
    s.preferences = { ...s.preferences, order };
    syncWatch(s);
    afterChange(s);
  });
}

// autoswitch.rs `Threshold::clamped`
export const clamped = (share: number) => Math.min(HIGHEST, Math.max(LOWEST, Math.round(share)));

export function setShare(share: number) {
  change((s) => {
    s.preferences = {
      ...s.preferences,
      weeklyShare: weeklyShare(s.preferences),
      share: clamped(share),
      chosen: true,
    };
    syncWatch(s);
    afterChange(s);
  });
}

export function setWeeklyLimit(on: boolean, at: number) {
  change((s) => {
    s.preferences = { ...s.preferences, weeklyShare: clamped(at), weeklyOff: !on, chosen: true };
    syncWatch(s);
    afterChange(s);
  });
}

// An installed service follows changes made in the app, including either threshold.
function syncWatch(s: State) {
  if (s.watchService)
    s.watchService = {
      at: s.preferences.share,
      order: s.preferences.order,
      weekly: weeklyThreshold(s.preferences),
      switching: s.preferences.atLimit === "switch",
    };
}

// service.rs `warm_turn`, logged as the core logs it.
export function turnWarmUp(label: string, on: boolean, caller: Caller = "app") {
  change((s) => {
    const a = find(s, "claude", label);
    if (!a) return;
    s.warm.on = s.warm.on.filter((id) => id !== a.id);
    if (on) s.warm.on.push(a.id);
    record(s, caller, on ? "warm-on" : "warm-off", typed(a));
    warmTick(s);
  });
}

// warm/mod.rs `Range::new`: a span of the day in minutes, 00:00 at its end meaning 24:00, and
// never empty; service.rs `warm_range` logs it as `clock` says each end.
export function setWarmRange(from: number, to: number, caller: Caller = "app"): boolean {
  const end = to === 0 ? DAY / MINUTE : to;
  if (from >= DAY / MINUTE || end > DAY / MINUTE || from === end) return false;
  const clock = (m: number) =>
    `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
  return change((s) => {
    s.warm = { ...s.warm, from, to: end };
    record(s, caller, "warm-range", `${clock(from)}-${clock(end)}`);
    warmTick(s);
    return true;
  });
}

// Warm Up Now, offered while that account's warm-up is due (present/accounts.rs).
export function warmUp(label: string, caller: Caller = "app"): boolean {
  return change((s) => {
    const a = find(s, "claude", label);
    if (!a || warmStanding(s, a, "now").stands !== "due") return false;
    warmUpNow(s, a, caller);
    return true;
  });
}

export function refresh() {
  change((s) => {
    if (s.running) readUsage(s);
  });
}

export function installWatch(
  at: number,
  caller: Caller = "cli",
  order: Order = "most-left",
  weekly: number | null = at,
  switching = true,
) {
  change((s) => {
    s.watchService = {
      at: clamped(at),
      order,
      weekly: weekly === null ? null : clamped(weekly),
      ...(caller === "app"
        ? { switching: s.preferences.atLimit === "switch" }
        : switching
          ? {}
          : { switching: false }),
    };
    record(s, caller, "watch-service", "install");
  });
}

export function uninstallWatch(caller: Caller = "cli"): boolean {
  return change((s) => {
    if (!s.watchService) return false;
    s.watchService = null;
    record(s, caller, "watch-service", "uninstall");
    return true;
  });
}

export function openWindow(tool: Tool, label: string) {
  change((s) => {
    const a = find(s, tool, label);
    if (a && !s.windowsOpen.includes(a.id)) s.windowsOpen.push(a.id);
  });
}

// The link waiting opened as `id`, in its window, which opens where it is not open
// (windows.rs `open_link`), or put away with no account chosen (`dismiss_link`).
export function openLink(id: string) {
  const s = model.get();
  const a = s.accounts.find((x) => x.id === id);
  if (!s.linkWaiting || !a) return;
  change((s) => {
    s.linkWaiting = false;
    if (!s.windowsOpen.includes(a.id)) s.windowsOpen.push(a.id);
  });
}

export function dismissLink() {
  if (!model.get().linkWaiting) return;
  change((s) => {
    s.linkWaiting = false;
  });
}

// A change to Settings' General.
export function setGeneral(to: Partial<State["general"]>) {
  change((s) => {
    s.general = { ...s.general, ...to };
  });
}

// An account's window closed, by its close button: it opens again from a menu.
export function closeWindow(id: string) {
  if (!model.get().windowsOpen.includes(id)) return;
  change((s) => {
    s.windowsOpen = s.windowsOpen.filter((open) => open !== id);
  });
}

// The Linux app's window closed where the tray keeps Pitboard running, and shown again.
export function closeAppWindow() {
  if (!model.get().windowShown) return;
  change((s) => {
    s.windowShown = false;
  });
}

export function showAppWindow() {
  if (model.get().windowShown) return;
  change((s) => {
    s.windowShown = true;
  });
}

export function quit() {
  change((s) => {
    s.running = false;
    s.windowsOpen = [];
  });
}

// Moves the clock to `to`, making each read and look the app makes on the way at its time.
export function advance(to: number) {
  change((s) => {
    for (;;) {
      const read = s.running ? s.readAt + READ_EVERY_SECONDS : Infinity;
      const look =
        s.running || s.watchService || s.watching !== null
          ? s.decidedAt + DECIDE_EVERY_SECONDS
          : Infinity;
      const next = Math.min(read, look);
      if (next > to) break;
      s.now = next;
      if (next === read) readUsage(s);
      else decidePass(s);
    }
    s.now = Math.max(s.now, to);
  }, false);
}

// The page's clock: the world's moment, then real time, by performance.now() so a throttled
// tab catches up when it next runs.
let clockTimer: ReturnType<typeof setInterval> | undefined;
let began = { real: 0, model: 0 };
const rebase = () => (began = { real: performance.now(), model: model.get().now });

export function run() {
  if (clockTimer !== undefined) return;
  rebase();
  clockTimer = setInterval(
    () => advance(began.model + Math.floor((performance.now() - began.real) / 1000)),
    1000,
  );
}

// Where Reset returns to: the start, or the state the hero's opening demo left, once its switch
// is made (plan section 4).
let rest: State | null = null;
export function settle(at: State | null = model.get()) {
  rest = at && structuredClone({ ...at, changed: false });
  if (at) model.setKey("changed", false);
}

export function reset() {
  enrolled = 0;
  people = 0;
  model.set(rest ? structuredClone(rest) : start());
  rebase();
}

// How long each app took over a switch and a read (docs/facts/site.md): In Use 0.12 s after a click
// on macOS, In use 0.19 to 0.37 s after a row's button on windows-2025 and 0.25 s on
// windows-11-arm, in their UI tests on CI's runners; a tray entry's switch 1.06 s through KDE
// Plasma in a Linux session, which GNOME's window takes too, measured on neither. Refresh was
// disabled for 14 to 24 ms on Windows and never seen dimmed on macOS; Linux's read takes Windows'.
export const TAKES_MS: Record<"switch" | "read", Record<System, number>> = {
  switch: { macos: 120, windows: 250, gnome: 1060, kde: 1060 },
  read: { macos: 24, windows: 24, gnome: 24, kde: 24 },
};

// What each system's host gives the app beside its windows, as the app's model keeps it
// (front.rs `Surfaces`): a status item in macOS's menu bar, Windows' notification area and KDE
// Plasma's tray, and none on stock GNOME, where closing the window quits Pitboard.
export const surfaces = (system: System) => ({ statusItem: system !== "gnome" });
let token = 0;

// The app's switch from a menu, the window's Switch or a notification's: the account reads
// "Switching…" and the others are dimmed while it runs, then it is in use.
export function switchFromApp(tool: Tool, label: string, system: System) {
  const to = find(model.get(), tool, label);
  if (!to || model.get().switching) return;
  const mine = ++token;
  change((s) => {
    s.switching = { id: to.id, token: mine };
  });
  setTimeout(() => {
    if (model.get().switching?.token !== mine) return;
    change((s) => {
      s.switching = null;
    }, false);
    const now = model.get().accounts.find((a) => a.id === to.id);
    if (now) use(now.tool, now.label);
  }, TAKES_MS.switch[system]);
}

// The app's Refresh: "Reading…" while the read runs.
export function refreshFromApp(system: System) {
  if (model.get().reading !== null || !model.get().running) return;
  const mine = ++token;
  change((s) => {
    s.reading = mine;
  });
  setTimeout(() => {
    if (model.get().reading !== mine) return;
    change((s) => {
      s.reading = null;
      if (s.running) readUsage(s);
    });
  }, TAKES_MS.read[system]);
}

// Switch Back on the window's notice of a switch Pitboard made by itself: a switch to the account
// it left, while the one it switched to is still in use (state.rs `switch_back`).
export function switchBackFromApp(system: System) {
  const s = model.get();
  const last = s.lastSwitch.claude;
  if (!last?.back || s.inUse.claude !== last.to || s.switching) return;
  change((s) => {
    s.switchedBack = { from: last.from, until: last.back!.resetsAt };
  }, false);
  switchFromApp("claude", last.from, system);
}

// The window's notice of a tool's last switch or new login, put away by its close button.
export function dismissNotice(tool: Tool) {
  change((s) => {
    delete s.lastSwitch[tool];
    delete s.newLogin[tool];
  });
}

// Not Now on the step asking for a second account of `tool`.
export function declineSecond(tool: Tool) {
  change((s) => {
    if (!s.declinedSecond.includes(tool)) s.declinedSecond.push(tool);
  });
}

// A simulated sign-in ends this long after it starts (answer 16 a).
export const SIGN_IN_MS = 3000;

// The app's sign-in, from Add Account or Sign In Again to `again`: the tool's own sign-in is never
// loaded, so it ends by itself, Claude Desktop's as if Claude quit signed in. Claude Desktop saves
// the account in use first, for as long as a switch takes.
export function signInFromApp(tool: Tool, again: string | null) {
  if (model.get().signing) return;
  const mine = ++token;
  change((s) => {
    s.signing = {
      tool,
      label: again,
      again: again !== null,
      phase: tool === "desktop" ? "saving" : "waiting",
      token: mine,
    };
  });
  const still = () => model.get().signing?.token === mine;
  if (tool === "desktop")
    setTimeout(() => {
      if (still())
        change((s) => {
          s.signing!.phase = "waiting";
        }, false);
    }, TAKES_MS.switch.macos);
  // added before the sign-in ends, so what ends it finds the account to name
  setTimeout(() => {
    if (!still()) return;
    if (again !== null) signInAgain(tool, again);
    else add(tool);
    change((s) => {
      s.signing = null;
    }, false);
  }, SIGN_IN_MS);
}

export function cancelSignIn() {
  if (!model.get().signing) return;
  change((s) => {
    s.signing = null;
  }, false);
}

// A switch notice's countdown, as Banner.swift's timer and Countdown.Left draw it.
export function countdown(until: number, now: number): string {
  const left = Math.max(until - now, 0);
  const h = Math.floor(left / HOUR);
  const m = Math.floor((left % HOUR) / MINUTE);
  const sec = String(left % MINUTE).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

// What the command line changes that no figure of the app does: an entry of the log a refused
// or repeated command leaves, the renewal schedule, and a `pitboard watch` running in a terminal.
export function logEntry(caller: Caller, verb: string, subject: string, outcome = "ok") {
  change((s) => record(s, caller, verb, subject, outcome));
}

export function setSchedule(on: boolean, caller: Caller = "cli"): boolean {
  const was = model.get().schedule;
  if (!on && !was) return false;
  change((s) => {
    s.schedule = on;
    record(s, caller, "schedule", on ? "install" : "uninstall");
  });
  return was;
}

// Not a change of its own: the watch's switches and warm-ups are.
export function watchInTerminal(
  watching: { at: number; order: Order; weekly?: number | null; switching?: boolean } | null,
) {
  change((s) => {
    s.watching = watching;
  }, false);
}

// `pitboard watch --once`: one decision at `share` in `order`, switching as a watch does.
export function decideOnce(
  share: number,
  order: Order = "most-left",
  weekly: number | null = share,
): Decision {
  const decision = decide(model.get(), share, order, weekly);
  // switch/auto.rs `met_a_limit`
  if (decision.decided === "hold")
    change((s) => {
      delete s.early[current(s, "claude")!.id];
    }, false);
  if (decision.decided !== "switch") return decision;
  return change((s) => {
    const d = decide(s, share, order, weekly) as Extract<Decision, { decided: "switch" }>;
    const from = current(s, "claude")!;
    if (d.limit) {
      delete s.early[from.id];
      s.ledger[limitKey("claude", from.id, d.limit)] = d.limit.resetsAt ?? 0;
    } else s.early[d.to.id] = { week: resetsAtOf(d.first.to) ?? 0, at: s.now };
    switchTo(s, d.to, "cli", "auto-switch");
    s.lastSwitch.claude!.because = d.limit
      ? { used: shareOfLimit(d.limit) }
      : { to: resetsAtOf(d.first.to), from: resetsAtOf(d.first.from) };
    s.autoTold = [];
    afterChange(s);
    return d;
  });
}
