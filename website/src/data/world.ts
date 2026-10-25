// Max 20x shows two of its three limits (the register's `usage_answer_lists_every_limit`), and Codex
// Pro its weekly limit alone, as docs/concepts/usage.mdx says.
import { platforms, type Platform } from "../lib/platform.ts";

export type Tool = "claude" | "codex" | "desktop";

// In the order every listing gives the tools (ProviderId::ALL).
export const tools: [Tool, string][] = [
  ["claude", "Claude Code"],
  ["codex", "Codex"],
  ["desktop", "Claude Desktop"],
];

// The systems Pitboard lists each tool on (ProviderId::supported_on).
export const toolOn: Record<Tool, Platform[]> = {
  claude: platforms,
  codex: platforms,
  desktop: ["macos"],
};

// Seconds, as time.rs counts them.
export const MINUTE = 60;
export const HOUR = 60 * MINUTE;
export const DAY = 24 * HOUR;
export const WEEK = 7 * DAY;

// The world's clocks and log are set to +07:00.
export const ZONE = 7 * HOUR;
const at = (local: string) => Date.parse(`${local}+07:00`) / 1000;

// A reading of one limit (core::usage::Window): the share used, when it resets, where a period
// runs, how long it runs, and the model it is limited to, where it is one model's.
export interface Limit {
  percent: number;
  resetsAt: number | null;
  lengthSeconds: number;
  scope?: string;
}

export interface Account {
  // Account::id, which stays through a rename
  id: string;
  tool: Tool;
  label: string;
  // none for Claude Desktop: Pitboard cannot ask claude.ai whose its login is
  email: string;
  plan: string | null;
  limits: Limit[];
  // ParkedLogin::refresh_expires_at, said while the account is parked
  refreshExpiresAt?: number;
  // Claude Desktop's: when Pitboard last read it in use (Row::read_at), and when claude.ai's
  // session ends while it is parked (session_expires_at)
  readAt?: number;
  sessionExpiresAt?: number;
  // enrolled since the last read, so no usage is known yet
  unread?: boolean;
  // what `pitboard doctor` names an account by: its uuid at Anthropic or OpenAI, a Claude Code
  // account's organization, and the fingerprint of its login's refresh token (store.rs
  // `fingerprint`), as crates/pitboard-core/src/doctor/example.rs gives them
  uuid?: string;
  org?: string;
  login?: string;
}

// A five-hour period starts and resets on a ten-minute mark (concepts/usage#when-a-period-starts):
// personal's started with its warm-up at 17:00.
const fiveHours = 5 * HOUR;
const personalPeriodEnds = at("2026-10-13T22:00:00");

export const accounts: Account[] = [
  {
    id: "personal",
    tool: "claude",
    label: "personal",
    email: "me@example.com",
    uuid: "5f0c9a52-7d1e-4b8a-9c3f-2e6a1d4b8c07",
    org: "8b2d4e61-3c5a-4f7e-a1d9-6c0e2b7f5a34",
    login: "fcda3e5dfe80953d",
    plan: "Max 20x",
    limits: [
      { percent: 59, resetsAt: personalPeriodEnds, lengthSeconds: fiveHours },
      { percent: 73, resetsAt: at("2026-10-19T14:50:00"), lengthSeconds: WEEK },
    ],
    refreshExpiresAt: at("2026-11-09T01:00:00"),
  },
  {
    id: "work",
    tool: "claude",
    label: "work",
    email: "me@company.com",
    uuid: "c3a7e1f9-5b2d-4e8c-b6a0-9d4f1e2c7b58",
    org: "e4f8a2c6-1d7b-4a3e-9f5c-0b8d6e2a4c91",
    login: "7f344b8dffee70ba",
    plan: "Max 20x",
    limits: [
      { percent: 12, resetsAt: at("2026-10-13T23:50:00"), lengthSeconds: fiveHours },
      { percent: 40, resetsAt: at("2026-10-16T01:00:00"), lengthSeconds: WEEK },
    ],
    refreshExpiresAt: at("2026-11-09T01:00:00"),
  },
  {
    id: "main",
    tool: "codex",
    label: "main",
    email: "me@example.com",
    uuid: "a1e5c9d3-6f2b-4c8a-8e7d-3b0f5a9c1e26",
    login: "a551afc0bc23fb53",
    plan: "Pro",
    limits: [{ percent: 7, resetsAt: at("2026-10-19T15:00:00"), lengthSeconds: WEEK }],
  },
  {
    id: "spare",
    tool: "codex",
    label: "spare",
    email: "me@company.com",
    uuid: "d7b3f1a5-9c4e-4e2a-a8f6-1c5e9b3d7f40",
    login: "726500ce1cf2dd6b",
    plan: "Plus",
    // nothing used, so no period runs: it resets five hours from the reading
    limits: [{ percent: 0, resetsAt: at("2026-10-14T01:50:00"), lengthSeconds: fiveHours }],
  },
  {
    id: "home",
    tool: "desktop",
    label: "home",
    email: "",
    plan: null,
    limits: [],
    readAt: at("2026-10-13T18:45:00"),
  },
  {
    id: "office",
    tool: "desktop",
    label: "office",
    email: "",
    plan: null,
    limits: [],
    sessionExpiresAt: at("2026-11-08T00:00:00"),
  },
];

export const clock = {
  // when the app last read usage: the figures say what it showed then
  read: at("2026-10-13T20:50:00"),
  // the menu bar's and the taskbar's clock, a minute on
  menuBar: at("2026-10-13T20:51:00"),
  switched: at("2026-10-13T20:49:31"),
  desktopSwitched: at("2026-10-13T18:46:12"),
  warmed: at("2026-10-13T17:00:04"),
  warmsAgain: personalPeriodEnds,
  // when personal, left in use, reaches the share of its weekly limit at the pace it runs out at
  warned: at("2026-10-14T05:53:00"),
};

export type Moment = "before" | "after";
export const inUse: Record<Moment, Record<Tool, string>> = {
  before: { claude: "personal", codex: "main", desktop: "home" },
  after: { claude: "work", codex: "main", desktop: "home" },
};

// What every figure reads: the accounts, the one in use for each tool, when usage was read, and
// the moment its words are said as of.
export interface World {
  now: number;
  readAt: number;
  accounts: Account[];
  inUse: Record<Tool, string>;
}

export const moments: Record<Moment, World> = {
  before: { now: clock.read, readAt: clock.read, accounts, inUse: inUse.before },
  after: { now: clock.read, readAt: clock.read, accounts, inUse: inUse.after },
};

// The share of a limit the app notifies or switches at until someone sets another
// (autoswitch.rs `Threshold::DEFAULT`).
export const share = 95;

// The accounts turned on for warm-up, alike on every system.
export const warmUp = { from: "07:00", to: "23:00", accounts: ["personal"] };

// An entry of the log (audit.rs `Entry`).
export interface Logged {
  at: number;
  caller: "app" | "cli";
  verb: string;
  subject: string;
  outcome: string;
  // where not every system makes the change
  on?: Platform[];
}

// What the log holds, oldest first.
export const logged: Logged[] = [
  {
    at: at("2026-10-10T09:16:52"),
    caller: "cli",
    verb: "enroll",
    subject: "work",
    outcome: "ok",
  },
  { at: clock.warmed, caller: "app", verb: "warm-up", subject: "personal", outcome: "ok" },
  {
    at: clock.desktopSwitched,
    caller: "app",
    verb: "use",
    subject: "desktop/home",
    outcome: "ok",
    on: toolOn.desktop,
  },
  { at: clock.switched, caller: "app", verb: "use", subject: "work", outcome: "ok" },
];

export const account = (label: string, tool: Tool = "claude"): Account => {
  const found = accounts.find((a) => a.label === label && a.tool === tool);
  if (!found) throw new Error(`no example account ${tool}/${label}`);
  return found;
};
