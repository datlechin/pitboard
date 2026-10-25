// The notification row's own moment (plan 3.6, question 2): when personal, left in use, reaches the
// share at the pace world.ts reads it at, 05:53 the next morning for 95%. A limit read at or past
// the share reaches it at once, the 5-hour first. The moment takes the page's choice, share and
// accounts, and what the app posts there is what the row's notification shows.
import { accounts as read, clock, type Limit } from "../data/world.ts";
import { advised, type Posted, type State } from "./model.ts";
import { paceOf, whole } from "./words.ts";

export interface Moment {
  at: number;
  // the moment once the app has read, advised and looked to switch
  state: State;
  posted: Posted | null;
}

// From the menu bar's clock, the part of what is left of a limit over its pace that takes it to
// the share, as words.test.ts derives the static figure's 05:53.
function reachesAt(l: Limit, share: number): number | null {
  if (whole(l.percent) >= share) return clock.menuBar;
  const p = paceOf(l.percent, l.resetsAt, l.lengthSeconds, clock.read, clock.read);
  if (p?.runsOutIn === undefined) return null;
  return clock.menuBar + Math.floor(((share - l.percent) / (100 - l.percent)) * p.runsOutIn);
}

export function noticeMoment(s: State): Moment | null {
  const subject = s.accounts.find((a) => a.id === "personal");
  const reading = read.find((a) => a.id === "personal")!;
  if (!subject) return null;
  const share = s.preferences.share;
  let at = clock.warned;
  let reaching: Limit | null = null;
  for (const l of reading.limits) {
    const when = reachesAt(l, share);
    if (when !== null && (reaching === null || when < at)) {
      at = when;
      reaching = l;
    }
  }
  const m = structuredClone(s);
  Object.assign(m, {
    now: at,
    readAt: at,
    inUse: { ...s.inUse, claude: subject.label },
    lastUsedAt: {},
    told: {},
    toldThisLaunch: {},
    advice: [],
    lastSwitch: {},
    newLogin: {},
    ledger: {},
    autoTold: [],
    autoStanding: null,
    posted: [],
  });
  m.accounts.find((a) => a.id === "personal")!.limits = reading.limits.map((l) =>
    l === reaching ? { ...l, percent: Math.max(l.percent, share) } : { ...l },
  );
  const state = advised(m);
  return { at, state, posted: state.posted[0] ?? null };
}
