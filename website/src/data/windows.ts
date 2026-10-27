// What the Windows app shows in the site's figures, in its own words: windows.test.ts finds each
// in the file that says it.
import {
  clock,
  logged,
  moments,
  share,
  warmUp,
  type Account,
  type Limit,
  type Tool,
  type World,
} from "./world.ts";
import {
  SIGN_IN_EXPIRED,
  SWITCHING,
  accountSpoken,
  activity,
  dayAndMonth,
  figure,
  fullest,
  headsUp,
  hm,
  isInUse,
  limitRow,
  limitTitle,
  resetsIn,
  resetsSaid,
  listed,
  menuLine,
  rowNote,
  signInExpired,
  switchSpoken,
  warmsAgainNote,
  toolAccounts,
  toolsOn,
} from "../lib/words.ts";
import {
  SWITCH,
  inUse as inUseSaid,
  keysOf,
  openSite,
  removeAccount,
  signInAgain,
  signInAsking,
  siteOf,
} from "./window.ts";

export const app = "Pitboard";

// MainWindow.xaml's navigation: Accounts, and NavigationView's own Settings at the foot
export const panes = [["accounts", "Accounts", "fl-people"]] as const;
// the pages Settings > Troubleshoot opens under Settings, each from its card (SettingsPage.xaml)
export const SETTINGS = "Settings";
export const subpages = [
  [
    "machine",
    "Diagnostics",
    "fl-pulse",
    "Check how Pitboard, Claude Code and Codex are set up on this PC.",
  ],
  ["activity", "Activity", "fl-history", "Every switch and change Pitboard made."],
] as const;
export type Pane = (typeof panes)[number][0] | (typeof subpages)[number][0] | "settings";
// the page a page shows under, whose item stays selected while it shows
export const underOf = (pane: Pane) => (subpages.some(([p]) => p === pane) ? "settings" : null);
// TitleBar's back button, and Settings' sections
export const BACK = "Back";
export const TROUBLESHOOT = "Troubleshoot";

export type MenuEntry =
  | { heading: string }
  | { account: string; label: string; checked: boolean; tip: string }
  | { command: string; bold?: boolean; site?: string; submenu?: string[] };

// the site each tool's accounts have windows on, in pitboard_sites::ALL's order
export const sites: [site: string, tool: Tool][] = [
  ["claude.ai", "claude"],
  ["chatgpt.com", "codex"],
];

// An item has one line: the account's label, then the figure the menu bar would show of it, or why
// it can't be switched to (present/words.rs `named_in_menu`).
export function itemTitle(a: Account, w: World, switching = false): string {
  const aside = switching
    ? SWITCHING
    : signInExpired(a, w)
      ? SIGN_IN_EXPIRED
      : a.limits.length > 0
        ? `${fullest(a)}%`
        : null;
  return aside ? `${a.label} (${aside.charAt(0).toLowerCase()}${aside.slice(1)})` : a.label;
}

// IconMenu.cs `Entries` in Microsoft's order, Open Pitboard first and in bold and Exit last, a
// group between each two separators but each tool's accounts, which follow on under their heading.
// A beta's menu has no Check for updates. An account's tooltip is the line the menu bar app shows
// under it, in sentence case.
export function iconMenu(w: World, updates: boolean): MenuEntry[][] {
  return [
    [{ command: "Open Pitboard", bold: true }],
    ...toolsOn("windows").map(([tool, name]) => [
      { heading: name },
      ...listed(w, tool).map((a) => ({
        account: itemTitle(a, w),
        label: a.label,
        checked: isInUse(a, w),
        tip: menuLine(a, w, "windows"),
      })),
    ]),
    [{ command: "Add account…" }],
    sites.map(([site, tool]) => ({
      command: `Open ${site}`,
      site,
      submenu: listed(w, tool).map((a) => a.label),
    })),
    [
      { command: "Settings" },
      ...(updates ? [{ command: "Check for updates" }] : []),
      { command: "Pitboard help" },
    ],
    [{ command: "Exit" }],
  ];
}

// A tool's accounts follow the tool before them with no separator, as one group of IconMenu.cs.
export const joinsTheGroupBefore = (group: MenuEntry[], before: MenuEntry[] | undefined) =>
  !!before && "heading" in group[0] && "heading" in before[0];

// The tooltip of an account's row in the icon's menu.
export function accountTip(label: string, w: World): string {
  for (const e of iconMenu(w, false).flat()) {
    if ("account" in e && e.label === label) return e.tip;
  }
  throw new Error(`the icon's menu lists no ${label}`);
}

// NoticeBar.xaml's notice after the switch to work, counting down
export const switchNotice = {
  title: "Switched Claude Code to work",
  countdown: "Claude Code sessions already open follow in 0:21",
};

// An opened row's cards (AccountsPage.xaml, present/accounts.rs `detail`), in Windows' words
export const detail = {
  email: "Email",
  copy: "Copy",
  signIn: "Sign-in",
  warmUp: "Warm-up",
  warmUpSettings: "Warm-up settings",
  off: "Off",
  savedUntil: (date: string) => `Saved sign-in valid until ${date}`,
};

// An opened row's card for a limit: its name and use, its reset and how long until it
// (AccountsPage.xaml, present/accounts.rs `limit_row`).
export const limitCard = (l: Limit, w: World) => ({
  name: limitTitle(l),
  used: `${figure(l.percent)} used`,
  resets: resetsSaid(l.resetsAt, w.now),
  resetsIn: resetsIn(l.resetsAt, w.now),
});

// What an opened row's Sign-in says: in use, expired, or until when its saved sign-in is valid
// (present/accounts.rs `detail`).
export function signInSaid(a: Account, w: World): string | null {
  if (isInUse(a, w)) return inUseSaid("windows");
  if (signInExpired(a, w)) return SIGN_IN_EXPIRED;
  return a.refreshExpiresAt === undefined
    ? null
    : detail.savedUntil(dayAndMonth(a.refreshExpiresAt));
}

// A row's button: Switch, or Sign in… where its saved sign-in has expired (present/accounts.rs
// `action`).
export const rowAction = (a: Account, w: World) =>
  isInUse(a, w)
    ? null
    : signInExpired(a, w)
      ? { title: signInAsking("windows"), spoken: null }
      : { title: SWITCH, spoken: switchSpoken(a) };

// AccountsPage.xaml's rows, each a SettingsExpander: the account, its plan, a line a limit and at
// most one note, In use or its button, and opened, its email, sign-in, warm-up and commands.
export function accountsPage(w: World, notice: typeof switchNotice | null) {
  return {
    title: "Accounts",
    updated: `Updated ${hm(w.readAt)}`,
    commands: ["Refresh", "Add account…"],
    notice,
    sections: toolsOn("windows").map(([tool, name]) => ({
      name,
      rows: listed(w, tool).map((a) => {
        const inUse = isInUse(a, w);
        const site = siteOf(a.tool);
        return {
          id: a.id,
          label: a.label,
          plan: a.plan,
          inUse,
          spoken: accountSpoken(a, w),
          keys: keysOf(a, w, "windows"),
          action: rowAction(a, w),
          limits: a.limits.map((l) => limitRow(l, w)),
          cards: a.limits.map((l) => limitCard(l, w)),
          note: rowNote(a, w),
          email: a.email || null,
          signIn: signInSaid(a, w),
          warm: a.tool === "claude" ? (warmsAgainNote(a) ?? detail.off) : null,
          // its windows, what its shortcut menu offers past its own button, then Remove
          commands: {
            open: site ? openSite(site) : null,
            signInAgain: signInExpired(a, w) ? null : signInAgain("windows"),
            remove: { title: removeAccount("windows"), enabled: !inUse },
          },
        };
      }),
    })),
  };
}

// SettingsPage.xaml's Limits, with machine.rs's words under When a limit reaches, set to Switch
// automatically, with Is this allowed? alone after them, and Warm-up open on its hours and
// machine.rs's rows, what it does under its name and its hours beside it
export const settingsPage = {
  title: "Settings",
  section: "Limits",
  atLimit: {
    header: "When a limit reaches",
    // the ComboBox's name, machine.rs's `at_label`
    label: `When a limit reaches ${share}%`,
    choice: "Switch automatically",
    standing: `Watching work: 40% of its weekly limit, as of ${hm(clock.read)}.`,
    note: `Pitboard switches Claude Code at ${share}%, and its running sessions follow within about 33 seconds.`,
    link: "Is this allowed?",
  },
  share: { header: "At", range: "From 50% to 99%", value: share },
  warm: {
    header: "Warm-up",
    note: "Pitboard starts each account’s 5-hour limit again as soon as it resets, within these hours, while it’s open.",
    range: `${warmUp.from} to ${warmUp.to}`,
    standing: `Next warm-up: ${warmUp.accounts[0]} at ${hm(clock.warmsAgain)}.`,
    hours: [
      ["From", warmUp.from],
      ["To", warmUp.to],
    ],
    accounts: toolAccounts(moments.after, "claude").map((a) => {
      const on = warmUp.accounts.includes(a.label);
      return {
        title: a.label,
        standing: on ? `Warms up again at ${hm(clock.warmsAgain)}` : null,
        on,
      };
    }),
  },
};

// DiagnosticsPage.xaml's header
export const diagnostics = {
  title: "Diagnostics",
  checkAgain: "Check again",
  copyReport: "Copy report",
};

export const activityPage = {
  title: "Activity",
  command: "Refresh",
  rows: activity(logged, "windows"),
};

// The notification at the share: title, tool and body, and its one button (WindowsNotifications.cs)
export const toast = {
  app,
  title: headsUp.title.windows,
  tool: headsUp.tool,
  body: headsUp.body,
  button: headsUp.button.windows,
};
