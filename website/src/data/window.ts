// What the Accounts window says on each system, in the app's own words: its notices, its setup
// step, its rows' menus and its sheets and dialogs. window.test.ts finds each in the file that
// says it.
import { toolOn, tools, type Account, type Tool, type World } from "./world.ts";
import type { Platform } from "../lib/platform.ts";
import { countdown as countdownOf } from "../lib/model.ts";

export type System = Extract<Platform, "macos" | "windows">;

// present/words.rs `titled`: a control's words in title case on macOS, sentence case on Windows
export const titled = (system: System, mac: string, win: string) =>
  system === "windows" ? win : mac;

const toolName = (tool: Tool) => tools.find(([t]) => t === tool)![1];

// The tools the app lists accounts of on `system`, of which a name says its own once there are
// two (present/mod.rs `name_of`, `tool_if_shown`).
const shownTools = (w: World, system: System) =>
  new Set(w.accounts.filter((a) => toolOn[a.tool].includes(system)).map((a) => a.tool));
export const toolIfShown = (w: World, system: System, tool: Tool) =>
  shownTools(w, system).size > 1 ? toolName(tool) : null;
export const nameOf = (a: Account, w: World, system: System) => {
  const tool = toolIfShown(w, system, a.tool);
  return tool ? `${a.label} (${tool})` : a.label;
};

// The program a tool signs in inside, for the one that does (ProviderId::program).
export const program = (tool: Tool) => (tool === "desktop" ? "Claude" : toolName(tool));
// The site an account's window opens (pitboard_sites).
export const siteOf = (tool: Tool) =>
  tool === "claude" ? "claude.ai" : tool === "codex" ? "chatgpt.com" : null;

// AccountsPane.swift's and AccountsPage.xaml's toolbar help
export const REFRESH_HELP = "Read every account’s usage again";
export const ADD_HELP = "Sign in to another account and park its login";
export const inUse = (system: System) => titled(system, "In Use", "In use");
export const USE = "Use";
export const SWITCHING_SPOKEN = "Switching";

// The offers on an account, as the row's menus and the Account menu list them
// (present/accounts.rs `offers`, AccountCommands.swift, AccountMenu.cs).
export const useOffer = (label: string) => `Use ${label}`;
export const signInAgain = (system: System) => titled(system, "Sign In Again…", "Sign in again…");
export const RENAME_OFFER = "Rename…";
export const warmUpNow = (system: System) => titled(system, "Warm Up Now", "Warm up now");
export const openSite = (site: string) => `Open ${site}`;
export const copyEmail = (system: System) =>
  titled(system, "Copy Email Address", "Copy email address");
export const FORGET_OFFER = "Forget…";

// The Forget question (present/accounts.rs `forget`, account_windows/accounts.rs
// `forget_message_on`).
export const forgetTitle = (a: Account, w: World, system: System) =>
  `Forget “${nameOf(a, w, system)}”?`;
export function forgetMessage(a: Account, system: System): string {
  if (a.tool === "desktop")
    return "Pitboard deletes the login it parked for this account and the folders set aside for it. Using it again needs a sign-in in Claude.";
  const machine = system === "windows" ? "this PC" : "this Mac";
  return `Pitboard deletes the login it parked for this account, and everything its ${siteOf(a.tool)} window keeps on ${machine}, its sign-in included. Using it again needs a sign-in in your browser.`;
}
export const FORGET = "Forget";
export const CANCEL = "Cancel";

// The Rename sheet (present/sheets.rs, present/words.rs).
export const renameTitle = (label: string) => `Rename “${label}”`;
export const RENAMING =
  "The account keeps its parked login. Only the name you switch to it by changes, here and in the command line.";
export const RENAME = "Rename";
export const NAME = "Name";
export const NAME_PROMPT = "work";
export const couldNotRename = (label: string) => `Couldn’t rename ${label}`;

// present/words.rs `unusable_name`, and what a Windows field says while it is empty
// (SheetPage.cs), its confirm in lower case
export function unusableName(
  problem: "empty" | "not_one_word" | "separator",
  picksTool: boolean,
): string | null {
  switch (problem) {
    case "empty":
      return null;
    case "not_one_word":
      return "A name is one word, with no spaces.";
    case "separator":
      return picksTool
        ? "A name can’t contain “/”. Choose the tool above."
        : "A name can’t contain “/”.";
  }
}
export const typeAName = (confirm: string) =>
  `Type a name for this account to ${confirm[0].toLowerCase()}${confirm.slice(1)}.`;

// core/error.rs `LabelTaken`, as present/words.rs `typeset` shows it: the label a tool's other
// account has, and whose it is (core/words.rs `login`).
export const labelTaken = (typed: string, email: string) =>
  `${typed} already refers to ${email}. Choose a different label.`;

// The sign-in sheets (present/sheets.rs, present/words.rs).
export const TOOL = "Tool";
export const ACCOUNT = "Account";
export const addAccount = (system: System) => titled(system, "Add Account", "Add account");
export const signIn = (system: System) => titled(system, "Sign In", "Sign in");
export const signInToAgain = (system: System, label: string) =>
  titled(system, `Sign In to ${label} Again`, `Sign in to ${label} again`);
export const signInMessage = (tool: string, again: string | null) =>
  again === null
    ? `Pitboard opens ${tool}’s own sign-in in your browser. Sign in as the account you’re adding, and Pitboard parks its login beside the one in use.`
    : `Pitboard opens ${tool}’s own sign-in in your browser. Sign in as ${again} to give Pitboard a new login for it.`;
export const signingInTo = (system: System, tool: string) =>
  titled(system, `Signing In to ${tool}`, `Signing in to ${tool}`);
export const finishInBrowser = (name: string, tool: string) =>
  `Finish signing in as ${name} in your browser. This closes once ${tool} says you’re in.`;
export const signInPage = (system: System) => titled(system, "Sign-In Page", "Sign-in page");
export const copyLink = (system: System) => titled(system, "Copy Link", "Copy link");
export const OPEN = "Open";
export const WAITING_FOR_BROWSER = "Waiting for your browser";
export const LATER = "Later";

// A sign-in inside Claude, which parks the account in use first (present/sheets.rs
// `in_app_form`, `in_app_signing_in_text`).
export const park = (label: string) => `Park ${label}`;
export const parkToAdd = (app: string, work: string) =>
  `Pitboard parks ${work}, which leaves ${app} signed out and signs nothing out at claude.ai. Then you sign in to the other account in ${app}, and Pitboard adds it once ${app} quits.`;
export const parkToSignInAgain = (app: string, label: string, work: string) =>
  `Pitboard parks ${work === label ? `${label}’s login as it is` : work}, which leaves ${app} signed out and signs nothing out at claude.ai. Then you sign in to ${label} again in ${app}, and Pitboard records it once ${app} quits.`;
export const parking = (label: string) => [`Parking ${label}.`, `Parking ${label}`] as const;
export function inAppWaiting(app: string, name: string, again: boolean): string {
  const signIn = again ? `sign in to ${name} again` : "sign in to the account you’re adding";
  const adds = again ? `Pitboard records it as ${name}’s login` : `Pitboard adds it as ${name}`;
  return `Open ${app} and ${signIn}. When it shows your chats, quit ${app} with Command-Q, and ${adds}.\n\nDon’t choose Log out in ${app}: it ends the login ${app} holds at claude.ai, and Pitboard can’t bring it back.`;
}
export const waitingToQuit = (app: string) => `Waiting for ${app} to quit`;
export const putBack = (system: System, label: string) =>
  titled(system, `Put ${label} Back`, `Put ${label} back`);

// The notice a tool's last switch leaves (present/notices.rs `of_last_switch`, present/words.rs).
export const switchedTo = (tool: string | null, label: string) =>
  tool ? `Switched ${tool} to ${label}` : `Switched to ${label}`;
export const hasANewLogin = (tool: string | null, label: string) =>
  tool ? `${label} in ${tool} has a new login` : `${label} has a new login`;
export const sessionsFollowIn = (tool: string | null) =>
  tool ? `${tool} sessions already open follow in` : "Sessions already open follow in";
export function signedInNow(name: string, again: boolean, app: string | null): string {
  if (again)
    return app
      ? `Signed in to ${name} again. ${app} is signed in to it now.`
      : `Signed in to ${name} again. Its new login is the one in use now.`;
  return app
    ? `Added ${name}, the account ${app} is signed in to now.`
    : `Enrolled ${name}, the account signed in now. Its new login is the one in use.`;
}
export const restartNotice = (program: string, from: string) =>
  `Any ${program} session started before this switch keeps using ${from || "the account it started with"} until it is quit and started again.`;
// core/words.rs: a Claude Desktop switch, with Claude closed
export const CLAUDE_OPENS = "Claude opens signed in to it.";
export const DISMISS = "Dismiss";
export const CLOSE = "Close";
// present/mod.rs `spoken_severity`
export const severitySaid = { info: "Note", warning: "Warning" } as const;

// The setup step a tool with one account shows (present/setup.rs `OnlyOne`).
export const addASecond = (tool: string | null) =>
  tool ? `Add a second ${tool} account` : "Add a second account";
export function onlyOneDetail(label: string, tool: Tool, shown: string | null): string {
  const adding =
    tool === "desktop"
      ? `Adding another parks this one while you sign in to the other in ${program(tool)}.`
      : "Adding another signs in to it and parks its login beside this one.";
  return `${label} is the only ${shown ? `${shown} ` : ""}account Pitboard knows, so there’s nothing to switch to. ${adding}`;
}
export const addAccountAsks = (system: System) => titled(system, "Add Account…", "Add account…");
export const notNow = (system: System) => titled(system, "Not Now", "Not now");

// What the window's notice of a tool's last switch or new login says, at `now`, or null where it
// says nothing: a Claude Code switch's notice is its countdown, and goes with it (present/notices.rs
// `of_last_switch`; NoticeRow.swift). Codex's warns that running sessions keep the old account.
export interface Notice {
  title: string;
  lines: string[];
  countdown: string | null;
  severity: "info" | "warning";
}
interface Switches {
  now: number;
  accounts: Account[];
  inUse: Record<Tool, string>;
  lastSwitch: Partial<Record<Tool, { from: string; to: string; followsAt: number | null }>>;
  newLogin: Partial<Record<Tool, { label: string; added?: boolean }>>;
}
export function noticeOf(s: Switches, tool: Tool, system: System): Notice | null {
  const last = s.lastSwitch[tool];
  const login = s.newLogin[tool];
  if (!last && !login) return null;
  const shown = toolIfShown({ ...s, readAt: s.now }, system, tool);
  const app = tool === "desktop" ? program(tool) : null;
  const lines: string[] = [];
  if (login) lines.push(signedInNow(login.label, !login.added, app));
  else if (tool === "desktop") lines.push(CLAUDE_OPENS);
  const restart = tool === "codex" && last && !login;
  if (restart) lines.push(restartNotice("codex", last.from));
  const running = last?.followsAt != null && last.followsAt > s.now;
  const countdown = running
    ? `${sessionsFollowIn(shown)} ${countdownOf(last!.followsAt!, s.now)}`
    : null;
  if (lines.length === 0 && !countdown) return null;
  return {
    title: login ? hasANewLogin(shown, login.label) : switchedTo(shown, last!.to),
    lines,
    countdown,
    severity: restart ? "warning" : "info",
  };
}

// The step asking for a second account of the first tool with one, unless put off for it
// (present/setup.rs `footing`, `OnlyOne`).
export function setupStepOf(
  w: World,
  declined: Tool[],
  system: System,
): { tool: Tool; title: string; detail: string } | null {
  for (const [tool] of tools) {
    if (!toolOn[tool].includes(system) || declined.includes(tool)) continue;
    const own = w.accounts.filter((a) => a.tool === tool);
    if (own.length !== 1 || w.inUse[tool] !== own[0].label) continue;
    // Claude Desktop is named always, as the account's app is
    const shown = tool === "desktop" ? toolName(tool) : toolIfShown(w, system, tool);
    return { tool, title: addASecond(shown), detail: onlyOneDetail(own[0].label, tool, shown) };
  }
  return null;
}

// The keys a row answers, as aria-keyshortcuts names them: Delete forgets an account not in use,
// on macOS Forward Delete too and ⌘C, on Windows F2 and Ctrl+C (AccountsPane.swift
// `onDeleteCommand` and `copyable`, AccountKeys.cs). None is a key a browser keeps from a page
// (docs/facts/site.md).
export function keysOf(a: Account, w: World, system: System): string {
  const forgets = w.inUse[a.tool] !== a.label;
  const copies = a.email !== "";
  const keys =
    system === "windows"
      ? [forgets && "Delete", "F2", copies && "Control+C"]
      : [forgets && "Backspace", forgets && "Delete", copies && "Meta+C"];
  return keys.filter(Boolean).join(" ");
}
