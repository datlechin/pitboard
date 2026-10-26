// What the Windows app shows in the site's figures, in its own words: windows.test.ts finds each
// in the file that says it.
import {
  clock,
  logged,
  moments,
  share,
  warmUp,
  type Account,
  type Tool,
  type World,
} from "./world.ts";
import {
  accountSpoken,
  activity,
  fullest,
  headsUp,
  hm,
  isInUse,
  limitRow,
  listed,
  menuLine,
  menuTitle,
  rowLines,
  switchSpoken,
  warmsAgainNote,
  toolAccounts,
  toolsOn,
} from "../lib/words.ts";

export const app = "Pitboard";

// MainWindow.xaml's panes, and NavigationView's own Settings at the foot
export const panes = [
  ["accounts", "Accounts", "fl-people"],
  ["activity", "Activity", "fl-history"],
  ["machine", "This PC", "fl-desktop"],
] as const;
export type Pane = (typeof panes)[number][0] | "settings";

export type MenuEntry =
  | { heading: string }
  | { account: string; label: string; checked: boolean; tip: string }
  | { command: string; bold?: boolean; site?: string; submenu?: string[] };

// the site each tool's accounts have windows on, in pitboard_sites::ALL's order
export const sites: [site: string, tool: Tool][] = [
  ["claude.ai", "claude"],
  ["chatgpt.com", "codex"],
];

// An item has one line, so the figure the menu bar would show of the account ends it (words.rs
// `named_in_menu`).
export const itemTitle = (a: Account) =>
  a.limits.length > 0 ? `${menuTitle(a)} · ${fullest(a)}%` : menuTitle(a);

// IconMenu.cs `Entries`, a group between each two separators. A beta's menu has no Check for updates.
// An account's tooltip is the line the menu bar app shows under it, in sentence case.
export function iconMenu(w: World, updates: boolean): MenuEntry[][] {
  return [
    ...toolsOn("windows").map(([tool, name]) => [
      { heading: name },
      ...listed(w, tool).map((a) => ({
        account: itemTitle(a),
        label: a.label,
        checked: isInUse(a, w),
        tip: menuLine(a, w, "windows"),
      })),
    ]),
    [{ command: "Add account…" }, { command: "Refresh" }],
    sites.map(([site, tool]) => ({
      command: `Open ${site}`,
      site,
      submenu: listed(w, tool).map((a) => a.label),
    })),
    [
      { command: "Open Pitboard", bold: true },
      { command: "Settings" },
      ...(updates ? [{ command: "Check for updates" }] : []),
    ],
    [{ command: "Pitboard help" }, { command: "Exit" }],
  ];
}

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

// AccountRow.xaml: its lines are when a login in use was read, a sign-in that has expired or soon
// does, and the warm-up.
export function accountsPage(w: World, notice: typeof switchNotice | null) {
  return {
    title: "Accounts",
    updated: `Updated ${hm(w.readAt)}`,
    commands: ["Refresh", "Add account"],
    notice,
    sections: toolsOn("windows").map(([tool, name]) => ({
      name,
      rows: listed(w, tool).map((a) => {
        const inUse = isInUse(a, w);
        return {
          id: a.id,
          label: a.label,
          plan: a.plan,
          email: a.email,
          inUse,
          spoken: accountSpoken(a, w),
          action: inUse ? null : { title: "Switch", spoken: switchSpoken(a) },
          limits: a.limits.map((l) => limitRow(l, w)),
          lines: rowLines(a, w, warmsAgainNote(a)),
        };
      }),
    })),
  };
}

// SettingsPage.xaml's Limits, with machine.rs's words under When a limit reaches, set to Switch
// automatically, and Warm-up open on its hours and machine.rs's rows
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
    terms: "Anthropic’s terms may cover automatic switching and warm-ups.",
    link: "Is this allowed?",
  },
  share: { header: "At", range: "From 50% to 99%", value: share },
  warm: {
    header: "Warm-up",
    range: `${warmUp.from} to ${warmUp.to}`,
    standing: `Next warm-up: ${warmUp.accounts[0]} at ${hm(clock.warmsAgain)}.`,
    hours: [
      ["From", warmUp.from],
      ["To", warmUp.to],
    ],
    accounts: toolAccounts(moments.after, "claude").map((a) => {
      const on = warmUp.accounts.includes(a.label);
      return {
        title: menuTitle(a),
        standing: on ? `Warms up again at ${hm(clock.warmsAgain)}` : null,
        on,
      };
    }),
  },
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
