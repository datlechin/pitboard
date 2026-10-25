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

export interface Limit {
  kind: "5h" | "week";
  used: number;
  even: number | null;
  resets: string;
  runsOut?: string;
}

export interface Account {
  tool: Tool;
  label: string;
  // none for Claude Desktop: Pitboard cannot ask claude.ai whose its login is
  email: string;
  plan: string | null;
  limits: Limit[];
  parked?: { cli: string; app: string };
  // Claude Desktop's notes, as reference/account-status gives them
  read?: string;
  lapses?: { cli: string; app: string };
}

export const accounts: Account[] = [
  {
    tool: "claude",
    label: "personal",
    email: "me@example.com",
    plan: "Max 20x",
    limits: [
      { kind: "5h", used: 59, even: 77, resets: "1h 10m" },
      { kind: "week", used: 73, even: 18, resets: "5d 18h", runsOut: "11h 05m" },
    ],
    parked: { cli: "26d 4h", app: "26 more days" },
  },
  {
    tool: "claude",
    label: "work",
    email: "me@company.com",
    plan: "Max 20x",
    limits: [
      { kind: "5h", used: 12, even: 40, resets: "3h 00m" },
      { kind: "week", used: 40, even: 69, resets: "2d 4h" },
    ],
    parked: { cli: "26d 4h", app: "26 more days" },
  },
  {
    tool: "codex",
    label: "main",
    email: "me@example.com",
    plan: "Pro",
    limits: [{ kind: "week", used: 7, even: 18, resets: "5d 18h" }],
  },
  {
    tool: "codex",
    label: "spare",
    email: "me@company.com",
    plan: "Plus",
    limits: [{ kind: "5h", used: 0, even: null, resets: "5h 00m" }],
  },
  {
    tool: "desktop",
    label: "home",
    email: "",
    plan: null,
    limits: [],
    read: "2h 05m",
  },
  {
    tool: "desktop",
    label: "office",
    email: "",
    plan: null,
    limits: [],
    lapses: { cli: "25d 3h", app: "25 days" },
  },
];

export type Moment = "before" | "after";
export const inUse: Record<Moment, Record<Tool, string>> = {
  before: { claude: "personal", codex: "main", desktop: "home" },
  after: { claude: "work", codex: "main", desktop: "home" },
};

// A five-hour period starts and resets on a ten-minute mark (concepts/usage#when-a-period-starts).
export const clock = {
  menuBar: "Tue 13 Oct 20:51",
  read: "20:50",
  switched: "20:49",
  // Claude Desktop to home, read then: 2h 05m before the menu bar's clock
  desktopSwitched: "18:46",
  warmed: "17:00",
  warmsAgain: "22:00",
  // when personal, left in use, reaches the share of its weekly limit at the pace it runs out at
  warned: "Wed 14 Oct 05:53",
};

// The same moments as Windows says them in the en-GB regional format: its taskbar's time over its
// date, and the app's "t" and "g" (WindowsLocalTime.cs).
export const windowsClock = {
  taskbar: { time: "20:51", date: "13/10/2026" },
  warned: { time: "05:53", date: "14/10/2026" },
};

// The share of a limit the app notifies or switches at until someone sets another
// (autoswitch.rs `Threshold::DEFAULT`).
export const share = 95;

// The accounts turned on for warm-up, alike on every system.
export const warmUp = { from: "07:00", to: "23:00", accounts: ["personal"] };

export interface Logged {
  day: string;
  time: string;
  verb: string;
  account: string;
  caller: "app" | "cli";
  // where not every system makes the change
  on?: Platform[];
}

// What the log holds, oldest first, as audit.rs writes it.
export const logged: Logged[] = [
  { day: "2026-10-10", time: "09:16:52", verb: "enroll", account: "work", caller: "cli" },
  {
    day: "2026-10-13",
    time: `${clock.warmed}:04`,
    verb: "warm-up",
    account: "personal",
    caller: "app",
  },
  {
    day: "2026-10-13",
    time: `${clock.desktopSwitched}:12`,
    verb: "use",
    account: "desktop/home",
    caller: "app",
    on: toolOn.desktop,
  },
  { day: "2026-10-13", time: `${clock.switched}:31`, verb: "use", account: "work", caller: "app" },
];

export const account = (label: string, tool: Tool = "claude"): Account => {
  const found = accounts.find((a) => a.label === label && a.tool === tool);
  if (!found) throw new Error(`no example account ${tool}/${label}`);
  return found;
};
