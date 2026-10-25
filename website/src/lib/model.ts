// The one state every live figure reads, as pitboard-ffi's model keeps it, and the core's rules
// that change it: crates/pitboard-core/src/duty/autoswitch.rs decides switches, ffi/model/advice.rs
// notifications, duty/warm warm-ups. Figures subscribe to `model`; the actions only write.
import { map } from "nanostores";
import {
  DAY,
  HOUR,
  MINUTE,
  ZONE,
  accounts as drawn,
  clock,
  inUse as drawnInUse,
  logged,
  share as defaultShare,
  tools,
  warmUp as drawnWarmUp,
  type Account,
  type Limit,
  type Logged,
  type Tool,
  type World,
} from "../data/world.ts";
import { capitalised, figure, hm, limitName, resets, scopedLimitName, whole } from "./words.ts";

// autoswitch.rs `Threshold`, `SETTLING_SECONDS`; switch/mod.rs `ADOPTION_CEILING_SECONDS`;
// advice.rs `FULL`; usage/mod.rs `SAME_RESET`; duty/cadence.rs's reads and looks.
export const LOWEST = 50;
export const HIGHEST = 99;
export const SETTLING_SECONDS = 300;
export const ADOPTION_CEILING_SECONDS = 33;
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
export type Caller = "app" | "cli";

export interface Preferences {
  atLimit: AtLimit;
  // whether anybody chose, which hides the setup step that asks (present/setup.rs)
  chosen: boolean;
  share: number;
}

export interface LastSwitch {
  from: string;
  to: string;
  at: number;
  // when sessions already open are sure to follow: Claude Code's alone
  followsAt: number | null;
}

// advice.rs `Advice`: an account in use has used a limit up, and another has room.
export interface Advice {
  tool: Tool;
  ran: string;
  limit: Limit;
  instead: string;
  left: number | null;
}

// present/notices.rs `RunOutNotice`, as the system's notification shows it.
export interface Posted {
  id: string;
  at: number;
  title: string;
  subtitle: string | null;
  body: string;
  switchTo: { tool: Tool; label: string } | null;
  opens: "accounts" | "limits";
}

// What switching Claude Code by itself last came to (ffi `AutoStanding`).
export type AutoStanding =
  | { came: "watching"; account: string; used: string | null; asOf: number | null }
  | { came: "switched"; from: string; to: string; used: string }
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
  // a sign-in that put a new login in use
  newLogin: Partial<Record<Tool, { label: string; at: number }>>;
  // limits switched away from, by account id, kind and scope, at their reset
  ledger: Record<string, number>;
  // reasons not to switch already told, until the next switch
  autoTold: string[];
  autoStanding: AutoStanding | null;
  // warm/mod.rs `Range` in minutes of the day, the accounts on by id, and when each last warmed
  warm: { from: number; to: number; on: string[]; started: Record<string, number> };
  watchService: { at: number } | null;
  log: Logged[];
  posted: Posted[];
  // accounts with a window of their site open, by id
  windowsOpen: string[];
  decidedAt: number;
  // a switch or a read the app has under way: "Switching…" and "Reading…" while it runs
  switching: { id: string; token: number } | null;
  reading: number | null;
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
    preferences: { atLimit: "notify", chosen: true, share: defaultShare },
    told: {},
    toldThisLaunch: {},
    advice: [],
    lastSwitch: {},
    newLogin: {},
    ledger: {},
    autoTold: [],
    autoStanding: null,
    warm: {
      from: minutes(drawnWarmUp.from),
      to: minutes(drawnWarmUp.to),
      on: drawnWarmUp.accounts.map(id),
      started: { [id("personal")]: clock.warmed },
    },
    watchService: null,
    log: logged.filter((e) => e.at !== clock.switched).map((e) => ({ ...e })),
    posted: [],
    windowsOpen: [id("work")],
    decidedAt: clock.menuBar,
    switching: null,
    reading: null,
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

// usage::LimitKind, as a reading names it by its length
const kind = (l: Limit) =>
  l.lengthSeconds === 5 * HOUR ? "session" : l.scope ? "weekly_scoped" : "weekly_all";
const sameLimit = (a: Limit, b: Limit) => kind(a) === kind(b) && a.scope === b.scope;
const limitKey = (tool: Tool, id: string, l: Limit) => `${tool}/${id}/${kind(l)}/${l.scope ?? ""}`;

// Anthropic's answer lists every limit an account has; OpenAI's may leave some out (codex/api.rs).
const listsEveryLimit = (tool: Tool) => tool === "claude";

// A parked login that still renews (ParkedLogin::restorable_at).
const switchable = (a: Account, now: number) =>
  a.refreshExpiresAt === undefined || a.refreshExpiresAt > now;

interface Room {
  used: number | null;
  fullest: number;
  weekly: boolean;
}

// usage/mod.rs `room`
export function room(
  theirs: Limit[],
  everyLimit: boolean,
  mine: Limit[],
  limit: Limit,
  share: number,
  now: number,
): Room | null {
  if (theirs.length === 0 || theirs.some((l) => reached(used(l, now), share))) return null;
  const given = (had: Limit) => theirs.find((l) => sameLimit(l, had));
  const gives = (k: string) => theirs.some((l) => !l.scope && kind(l) === k);
  if (!everyLimit) {
    const overall = gives("session") && gives("weekly_all");
    const unknown = (had: Limit) =>
      !given(had) && !(overall && had.scope && !sameLimit(had, limit));
    if ([limit, ...mine].some(unknown)) return null;
  }
  const fullest = theirs
    .filter((l) => !sameLimit(l, limit))
    .reduce((most, l) => Math.max(most, used(l, now)), 0);
  const of = given(limit);
  return {
    used: of ? used(of, now) : null,
    fullest,
    weekly: !everyLimit || gives("weekly_all"),
  };
}

// usage/mod.rs `preferred`: without a weekly limit for all models first, then the least used of
// the limit, then the least full; the first of those alike.
export function preferred<T>(candidates: [T, Room][]): [T, Room] | null {
  let best: [T, Room] | null = null;
  for (const c of candidates) {
    if (!best) {
      best = c;
      continue;
    }
    const [, a] = c;
    const [, b] = best;
    const order =
      Number(a.weekly) - Number(b.weekly) || (a.used ?? 0) - (b.used ?? 0) || a.fullest - b.fullest;
    if (order < 0) best = c;
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
  | { decided: "switch"; to: Account; limit: Limit };

// The fuller of two limits first, then the one resetting sooner.
const fuller = (a: Limit, b: Limit, now: number) =>
  used(b, now) - used(a, now) || (a.resetsAt ?? Infinity) - (b.resetsAt ?? Infinity);

// autoswitch.rs `decide`, for the Claude Code account in use at `share`.
export function decide(s: State, share: number): Decision {
  const now = s.now;
  const from = current(s, "claude");
  if (!from || from.unread) return { decided: "no-reading" };
  const at = (l: Limit) => s.ledger[limitKey("claude", from.id, l)];
  const left = (l: Limit) => {
    const was = at(l);
    return was !== undefined && sameReset(was, l.resetsAt ?? 0);
  };
  const reachedLimits = from.limits
    .filter((l) => reached(used(l, now), share))
    .sort((a, b) => fuller(a, b, now));
  if (reachedLimits.length === 0) {
    const running = from.limits
      .filter((l) => l.resetsAt === null || l.resetsAt > now)
      .sort((a, b) => fuller(a, b, now));
    return { decided: "below", nearest: running[0] ?? null, asOf: s.readAt };
  }
  const limit = reachedLimits.find((l) => !left(l));
  if (!limit)
    return { decided: "hold", limit: reachedLimits[0], why: { code: "already_switched" } };
  const putInUse = s.lastUsedAt[from.id];
  if (putInUse !== undefined && now - putInUse >= 0 && now - putInUse < SETTLING_SECONDS)
    return {
      decided: "hold",
      limit,
      why: { code: "settling", until: putInUse + SETTLING_SECONDS },
    };
  const goes = s.accounts.filter((a) => a.tool === "claude" && a !== from && switchable(a, now));
  const best = preferred(
    goes.flatMap((a): [Account, Room][] => {
      if (a.unread) return [];
      const r = room(a.limits, listsEveryLimit("claude"), from.limits, limit, share, now);
      return r ? [[a, r]] : [];
    }),
  );
  if (best) return { decided: "switch", to: best[0], limit };
  return {
    decided: "hold",
    limit,
    why: { code: "no_room", unread: goes.filter((a) => a.unread).map(typed) },
  };
}

// The core's words for what switching by itself came to (words.rs `not_switching`,
// `not_watching`, `share_of_limit`).
export const shareOfLimit = (l: Limit) => `${figure(l.percent)} of its ${scopedLimitName(l)} limit`;
// words.rs `listed`: "a", "a and b", "a, b and c".
const listed = (names: string[]) =>
  names.length < 2 ? names.join("") : `${names.slice(0, -1).join(", ")} and ${names.at(-1)}`;

export function notSwitching(why: Skip, share: number): string {
  switch (why.code) {
    case "no_room": {
      const room = `no other Claude Code account has room below ${share}% in every limit`;
      return why.unread.length
        ? `${room}, and there is no reading of ${listed(why.unread)} from Anthropic yet`
        : room;
    }
    case "already_switched":
      return "this limit was switched away from once already, and is not again before it resets";
    case "settling":
      return `the account was put in use less than ${SETTLING_SECONDS / 60} minutes ago`;
  }
}

const notWatching = (account: string) => `there is no reading of ${account} from Anthropic yet`;

// present/words.rs `sentence`: a clause as a sentence.
const sentence = (text: string) => {
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
      return `Pitboard switches Claude Code once a limit of the account in use reaches ${p.share}%, never back by itself, and notifies you of Codex. Sessions already running follow within about ${ADOPTION_CEILING_SECONDS} seconds.`;
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

// The tools whose accounts were read, which a notification names where there is more than one.
const subtitle = (s: State, tool: Tool) =>
  new Set(s.accounts.map((a) => a.tool)).size > 1 ? toolName(tool) : null;

function post(s: State, notice: Omit<Posted, "at">) {
  if (!s.running) return;
  s.posted.push({ ...notice, at: s.now });
}

// advice.rs `spare`: the account of the same tool there is room on for a switch away from `ran`.
function spare(s: State, from: Account, ran: Limit, share: number) {
  const best = preferred(
    s.accounts.flatMap((a): [Account, Room][] => {
      if (a.tool !== from.tool || a === from || a.unread || !switchable(a, s.now)) return [];
      const r = room(a.limits, listsEveryLimit(a.tool), from.limits, ran, share, s.now);
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
  if (atLimit === "nothing" || !s.running) return;
  for (const a of readTools(s)) {
    const l = a.limits.find((l) => reached(used(l, now), share) && !told(s.told, a, l));
    if (!l || (atLimit === "switch" && a.tool === "claude")) continue;
    s.told[limitKey(a.tool, a.id, l)] = l.resetsAt ?? 0;
    const instead = spare(s, a, l, share);
    post(s, {
      id: `${limitKey(a.tool, a.label, l)}-${l.resetsAt ?? 0}`,
      title: usedUp(l, now)
        ? `${a.label} has no ${limitName(l)} limit left`
        : `${a.label} has used ${shareOfLimit(l)}`,
      subtitle: subtitle(s, a.tool),
      body: instead
        ? instead.left === null
          ? `${instead.account.label} has no such limit.`
          : `${instead.account.label} has ${instead.left}% of its own left.`
        : l.resetsAt !== null
          ? sentence(resets(l.resetsAt, now))
          : "",
      switchTo: instead && { tool: a.tool, label: instead.account.label },
      opens: "accounts",
    });
  }
}

// state.rs `look_to_switch` and its outcomes: what switching by itself came to, said under the
// setting, and each reason not to said once until the next switch.
function lookToSwitch(s: State) {
  const app = s.running && s.preferences.atLimit === "switch";
  const share = app ? s.preferences.share : s.watchService?.at;
  if (share === undefined) return;
  const decision = decide(s, share);
  const from = current(s, "claude");
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
        title: "Pitboard isn’t switching Claude Code",
        subtitle: null,
        body: sentence(why),
        switchTo: null,
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
      const said = notSwitching(why, share);
      stands(
        {
          came: "skipped",
          from: from!.label,
          used: shareOfLimit(limit),
          why: said,
          until: why.code === "settling" ? why.until : null,
        },
        key,
        {
          id: `auto-${key}`,
          title: "Claude Code wasn’t switched",
          subtitle: null,
          body: `${from!.label} has used ${shareOfLimit(limit)}. ${sentence(said)}`,
          switchTo: null,
          opens: "limits",
        },
      );
      return;
    }
    case "switch": {
      const { to, limit } = decision;
      const was = from!.label;
      s.ledger[limitKey("claude", from!.id, limit)] = limit.resetsAt ?? 0;
      switchTo(s, to, app ? "app" : "cli", "auto-switch");
      s.autoTold = [];
      if (!app) return;
      s.autoStanding = { came: "switched", from: was, to: to.label, used: shareOfLimit(limit) };
      post(s, {
        id: `auto/${was}/${to.label}/${s.now}`,
        title: `Switched Claude Code to ${to.label}`,
        subtitle: null,
        body: `${was} has used ${shareOfLimit(limit)}. ${sentence(afterSwitch("claude", "Sessions"))}`,
        switchTo: null,
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

// words.rs `warm_standing`, with the app's clock.
export function warmSaid(standing: WarmStanding): string {
  switch (standing.stands) {
    case "due":
      return "warms up now";
    case "running":
      if (standing.next === standing.until) return `warms up again at ${hm(standing.next)}`;
      return standing.next === null
        ? `has its 5-hour period running until ${hm(standing.until)}`
        : `has its 5-hour period running until ${hm(standing.until)}, and warms up again at ${hm(standing.next)}`;
    case "resting":
      return `warms up at ${hm(standing.at)}`;
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
            ? `has its ${name} limit used up until ${hm(l.resetsAt)}`
            : `has its ${name} limit used up`;
        }
      }
  }
}

// The note under an account turned on for warm-ups (present/accounts.rs `warm_note`).
export function warmNote(s: State, a: Account): string | null {
  if (a.tool !== "claude" || !s.warm.on.includes(a.id)) return null;
  return capitalised(warmSaid(warmStanding(s, a, "by-itself")));
}

// The line under Warm up limits (present/machine.rs `warm`, warm/mod.rs `Overview::next`).
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
  if (!s.running && !s.watchService) return;
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

// state.rs `switch`: Use, a menu's account, a notification's Switch, `pitboard use`.
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
    if (to !== from && find(s, tool, to)) {
      record(s, caller, "rename", subject, "label_taken");
      return "label_taken";
    }
    if (to === from) return "ok";
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
    record(s, caller, "rename", subject);
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

// A simulated sign-in's end, for Add Account and `pitboard enroll`: the tool's own sign-in is
// never loaded, so the new account is `<name>@example.com` on the plan of the one in use, and
// its first read finds nothing used. Claude Desktop's ends with Claude signed in to it.
export function enroll(tool: Tool, label: string, caller: Caller = "app"): Outcome {
  return change((s) => {
    const subject = tool === "claude" ? label : `${tool}/${label}`;
    if (nameProblem(label)) {
      record(s, caller, "enroll", subject, "invalid_label");
      return "invalid_label";
    }
    if (find(s, tool, label)) {
      record(s, caller, "enroll", subject, "label_taken");
      return "label_taken";
    }
    const like = current(s, tool);
    const a: Account = {
      id: `new-${++enrolled}`,
      tool,
      label,
      email: tool === "desktop" ? "" : `${label}@example.com`,
      plan: like?.plan ?? null,
      limits: [],
    };
    if (tool === "desktop") {
      s.accounts.push(a);
      switchTo(s, a, caller, "enroll");
      return "ok";
    }
    a.unread = true;
    // codex/engine.rs `expiry`: Codex states no refresh lifetime, so its parks keep none
    if (tool === "claude") a.refreshExpiresAt = s.now + SIGN_IN_LASTS;
    s.accounts.push(a);
    record(s, caller, "enroll", subject);
    return "ok";
  });
}

// A simulated sign-in again: a fresh login, parked or in use. Claude Desktop's ends with Claude
// signed in to the account, which puts it in use.
export function signInAgain(tool: Tool, label: string, caller: Caller = "app"): Outcome {
  return change((s) => {
    const a = find(s, tool, label);
    if (!a) return "account_unknown";
    if (tool === "desktop") {
      if (s.inUse.desktop !== label) switchTo(s, a, caller, "enroll");
      else {
        a.readAt = s.now;
        record(s, caller, "enroll", typed(a));
      }
      s.newLogin.desktop = { label, at: s.now };
      return "ok";
    }
    if (tool === "claude") a.refreshExpiresAt = s.now + SIGN_IN_LASTS;
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
    if (atLimit !== "switch") s.autoStanding = null;
    afterChange(s);
  });
}

// autoswitch.rs `Threshold::clamped`
export const clamped = (share: number) => Math.min(HIGHEST, Math.max(LOWEST, Math.round(share)));

export function setShare(share: number) {
  change((s) => {
    s.preferences = { ...s.preferences, share: clamped(share), chosen: true };
    afterChange(s);
  });
}

export function turnWarmUp(label: string, on: boolean) {
  change((s) => {
    const a = find(s, "claude", label);
    if (!a) return;
    s.warm.on = s.warm.on.filter((id) => id !== a.id);
    if (on) s.warm.on.push(a.id);
    warmTick(s);
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

export function installWatch(at: number, caller: Caller = "cli") {
  change((s) => {
    s.watchService = { at: clamped(at) };
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
      const look = s.running || s.watchService ? s.decidedAt + DECIDE_EVERY_SECONDS : Infinity;
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
  model.set(rest ? structuredClone(rest) : start());
  rebase();
}

// How long each app took over a switch and a read in its UI tests on CI's runners
// (docs/facts/site.md): In Use 0.12 s after a click on macOS, In use 0.19 to 0.37 s after Use on
// windows-2025 and 0.25 s on windows-11-arm, and Refresh disabled for 14 to 24 ms on Windows and
// never seen dimmed on macOS.
type System = "macos" | "windows";
export const TAKES_MS: Record<"switch" | "read", Record<System, number>> = {
  switch: { macos: 120, windows: 250 },
  read: { macos: 24, windows: 24 },
};
let token = 0;

// The app's switch from a menu, the window's Use or a notification's Switch: the account reads
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

// A switch notice's countdown, as NoticeRow.swift's timer and Countdown.Left draw it.
export function countdown(until: number, now: number): string {
  const left = Math.max(until - now, 0);
  const h = Math.floor(left / HOUR);
  const m = Math.floor((left % HOUR) / MINUTE);
  const sec = String(left % MINUTE).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}
