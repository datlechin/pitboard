// The menu bar a Pitboard window in front puts up on macOS, as the app's UI tests read it with the
// Accounts window in front (docs/facts/site.md; PitboardScenes.swift's commands): each menu's
// items in order, a null between two sections, each with its key equivalent and the symbol macOS
// 26 draws before the system's own. `does` names what the page does with an item; an item of the
// system's, such as Hide, Edit's text commands or the Window menu's, closes the menu and does
// nothing more, as a window's own buttons do nothing on the page (plan 3.0). The Apple menu is the
// system's, and the site draws no Apple logo.

export type AppDoes =
  | "about"
  | "updates"
  | "settings"
  | "quit"
  | "add"
  | "claude.ai"
  | "chatgpt.com"
  | "copy"
  | "delete"
  | "refresh"
  | "help";

export interface AppItem {
  title: string;
  // the modifiers, then the key
  keys?: [string, string];
  icon?: string;
  // a submenu of the system's, which the page does not draw
  more?: boolean;
  dimmed?: boolean;
  does?: AppDoes;
}

export interface AppMenu {
  id: "pitboard" | "file" | "edit" | "view" | "go" | "account" | "window" | "help";
  title: string;
  items: (AppItem | null)[];
}

// A beta has no Check for Updates…, as its status menu has none.
export const appMenus = (updates: boolean): AppMenu[] => [
  {
    id: "pitboard",
    title: "Pitboard",
    items: [
      { title: "About Pitboard", does: "about" },
      ...(updates ? [{ title: "Check for Updates…", does: "updates" as const }] : []),
      null,
      { title: "Settings…", keys: ["⌘", ","], icon: "sf-gear", does: "settings" },
      null,
      { title: "Services", icon: "sf-gear2", more: true },
      null,
      { title: "Hide Pitboard", keys: ["⌘", "H"], icon: "mi-hide" },
      { title: "Hide Others", keys: ["⌥⌘", "H"], icon: "mi-windows" },
      { title: "Show All", icon: "mi-windows", dimmed: true },
      null,
      { title: "Quit Pitboard", keys: ["⌘", "Q"], icon: "mi-quit", does: "quit" },
    ],
  },
  {
    id: "file",
    title: "File",
    items: [
      { title: "Add Account…", keys: ["⌘", "N"], does: "add" },
      null,
      { title: "Open claude.ai", does: "claude.ai" },
      { title: "Open chatgpt.com", does: "chatgpt.com" },
      null,
      { title: "Remove Website Data…", dimmed: true },
      null,
      { title: "Close", keys: ["⌘", "W"], icon: "mi-close" },
    ],
  },
  {
    id: "edit",
    title: "Edit",
    items: [
      { title: "Undo", keys: ["⌘", "Z"], icon: "mi-undo", dimmed: true },
      { title: "Redo", keys: ["⇧⌘", "Z"], icon: "mi-redo", dimmed: true },
      null,
      { title: "Cut", keys: ["⌘", "X"], icon: "mi-cut", dimmed: true },
      { title: "Copy", keys: ["⌘", "C"], icon: "mi-copy", does: "copy" },
      { title: "Paste", keys: ["⌘", "V"], icon: "mi-paste", dimmed: true },
      { title: "Delete", icon: "mi-trash", does: "delete" },
      { title: "Select All", keys: ["⌘", "A"], icon: "mi-select-all", dimmed: true },
      { title: "Copy Report", dimmed: true },
      null,
      { title: "Find", more: true },
      { title: "Spelling and Grammar", more: true },
      { title: "Substitutions", more: true },
      { title: "Transformations", more: true },
      { title: "Speech", more: true },
      null,
      { title: "AutoFill", icon: "mi-autofill", more: true },
      { title: "Start Dictation…", keys: ["fn", "D"], icon: "mi-mic" },
      { title: "Emoji & Symbols", keys: ["fn", "E"], icon: "mi-emoji" },
    ],
  },
  {
    id: "view",
    title: "View",
    items: [
      { title: "Refresh", keys: ["⌘", "R"], does: "refresh" },
      null,
      { title: "Actual Size", keys: ["⌘", "0"], dimmed: true },
      { title: "Zoom In", keys: ["⌘", "+"], dimmed: true },
      { title: "Zoom Out", keys: ["⌘", "−"], dimmed: true },
      null,
      { title: "Hide Inspector", keys: ["⌃⌘", "I"], icon: "mi-inspector" },
      { title: "Enter Full Screen", keys: ["fn", "F"], icon: "mi-full-screen", dimmed: true },
    ],
  },
  {
    id: "go",
    title: "Go",
    items: [
      { title: "Back", keys: ["⌘", "["], dimmed: true },
      { title: "Forward", keys: ["⌘", "]"], dimmed: true },
    ],
  },
  // drawn from the state by AppMenuBar.astro, as AccountCommands.swift builds it
  { id: "account", title: "Account", items: [] },
  {
    id: "window",
    title: "Window",
    items: [
      { title: "Minimize", keys: ["⌘", "M"], icon: "mi-minimize" },
      { title: "Zoom", icon: "mi-zoom" },
      { title: "Fill", keys: ["⌃fn", "F"], icon: "mi-fill" },
      { title: "Center", keys: ["⌃fn", "C"], icon: "mi-center" },
      null,
      { title: "Move & Resize", icon: "mi-move", more: true },
      { title: "Full Screen Tile", icon: "mi-tile", dimmed: true },
      null,
      { title: "Remove Window from Set", icon: "mi-remove", dimmed: true },
      null,
      { title: "Accounts" },
      { title: "Diagnostics" },
      { title: "Activity" },
      null,
      { title: "Bring All to Front", icon: "mi-front" },
    ],
  },
  { id: "help", title: "Help", items: [{ title: "Pitboard Help", does: "help" }] },
];

// The Account menu with no account selected: what it would offer, each dimmed, and Remove Account…
// on ⌘⌫ (AccountCommands.swift; the model's `accountMenu`).
export const NO_ACCOUNT = ["Switch", "Sign In Again…", "Rename…"];
