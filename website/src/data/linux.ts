// What the Linux app shows in the site's figures, in its own words: the model's for GNOME's front,
// which it says on both desktops, and its own. linux.test.ts finds each in the file that says it.
import { listed, isInUse, menuTitle, settings, toolsOn, barTip } from "../lib/words.ts";
import type { State } from "../lib/model.ts";
import { ADD_HELP, REFRESH_HELP, addAccountAsks, needsAttention } from "./window.ts";
import type { Tool, World } from "./world.ts";

export const app = "Pitboard";

// The window's header bar (data/ui/window.ui, window/mod.rs `words`): Add Account… at its start,
// the page's title, then Refresh and the main menu at its end, each named for a screen reader and
// with its tooltip.
export const header = {
  title: "Accounts",
  add: { name: addAccountAsks("gnome"), tip: ADD_HELP },
  refresh: { name: "Refresh", tip: REFRESH_HELP },
  menu: { name: "Main Menu", tip: "Main Menu" },
};

// The main menu (window/mod.rs `words`): the app's own items, then GNOME's in one section at its
// end, and no Quit.
export const mainMenu = [
  ["Open Link…", "Diagnostics", "Activity"],
  [settings("gnome", true), "Keyboard Shortcuts", "Help", "About Pitboard"],
];

// Preferences and its pages (data/ui/preferences.ui, app_words.rs `settings`).
export const preferences = {
  title: settings("gnome", false),
  pages: ["General", "Limits", "Advanced"],
};

// The keys the window answers as GTK names them, in the order Keyboard Shortcuts lists them, under
// its one section (window/keys.rs, app_words.rs `GnomeShortcutWords`).
export const shortcuts = {
  section: "General",
  keys: [
    ["<Control>n", "Add Account"],
    ["<Control>r", "Refresh"],
    ["<Control>comma", "Preferences"],
    ["<Control>question", "Keyboard Shortcuts"],
    ["F10", "Open Main Menu"],
    ["<Alt>Left", "Go Back"],
    ["<Control>w", "Close Window"],
    ["<Control>q", "Quit"],
  ] as [accelerator: string, title: string][],
};

// The words only the Linux app says that the page draws (app_words.rs `gnome_words`): Open Link's
// button, Preferences' switch for watching once the window is closed, and the button beside each
// command Remove the App gives.
export const PASTE_LINK = "Paste Link";
export const WATCH_IN_THE_BACKGROUND = "Watch in the Background";
export const COPY_COMMAND = "Copy";

// An entry of the tray's menu: a command under the id presence.rs gives its item, a tool's name
// over its accounts, or an account as a radio entry, the one in use chosen.
export type TrayCommand = "open" | "add-account" | "refresh" | "preferences" | "help" | "quit";
export type TrayEntry =
  | { command: string; does: TrayCommand }
  | { heading: string; tool: Tool }
  | { account: string; label: string; tool: Tool; checked: boolean };

// The tray's menu, groups between separators (presence.rs `tray`): Open Pitboard, each tool's
// accounts, under its name while two tools have accounts (accounts.rs `sections`), Add Account…
// and Refresh, Preferences and Help, then Quit. A group with nothing in it goes.
export function trayMenu(w: World): TrayEntry[][] {
  const shown = toolsOn("linux").filter(([tool]) => listed(w, tool).length > 0);
  const groups: TrayEntry[][] = [
    [{ command: "Open Pitboard", does: "open" }],
    shown.flatMap(([tool, name]): TrayEntry[] => [
      ...(shown.length > 1 ? [{ heading: name, tool }] : []),
      ...listed(w, tool).map((a) => ({
        account: menuTitle(a, w, "gnome"),
        label: a.label,
        tool,
        checked: isInUse(a, w),
      })),
    ]),
    [
      { command: addAccountAsks("gnome"), does: "add-account" },
      { command: "Refresh", does: "refresh" },
    ],
    [
      { command: settings("gnome", true), does: "preferences" },
      { command: "Help", does: "help" },
    ],
    [{ command: "Quit", does: "quit" }],
  ];
  return groups.filter((group) => group.length > 0);
}

// The tray's tooltip, its first line its title and the rest its text (presence.rs `face`).
export function trayTip(w: World, attention = false) {
  const [title, ...text] = barTip(w, "gnome", attention).split("\n");
  return { title, text: text.join("\n") };
}

// The item needs attention, and Plasma draws its attention icon, as window.ts says it does.
export const trayAttention = (s: State): boolean => needsAttention(s, "gnome");
