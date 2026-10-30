// What the Accounts window says on each system, in the app's own words: its notices, its setup
// step, its rows' menus and its sheets and dialogs. window.test.ts finds each in the file that
// says it.
import { toolOn, tools, type Account, type Tool, type World } from "./world.ts";
import { countdown as countdownOf, type State } from "../lib/model.ts";
import { platformOf, type Front, type System } from "../lib/platform.ts";
import { thisMachine, titled } from "../lib/words.ts";

const toolName = (tool: Tool) => tools.find(([t]) => t === tool)![1];

// The tools the app lists accounts of on its system, of which a name says its own once there are
// two (present/mod.rs `name_of`, `tool_if_shown`).
const shownTools = (w: World, front: Front) =>
  new Set(w.accounts.filter((a) => toolOn[a.tool].includes(platformOf(front))).map((a) => a.tool));
export const toolIfShown = (w: World, front: Front, tool: Tool) =>
  shownTools(w, front).size > 1 ? toolName(tool) : null;
export const nameOf = (a: Account, w: World, front: Front) => {
  const tool = toolIfShown(w, front, a.tool);
  return tool ? `${a.label} (${tool})` : a.label;
};

// The program a tool signs in inside, for the one that does (ProviderId::program).
export const program = (tool: Tool) => (tool === "desktop" ? "Claude" : toolName(tool));
// The site an account's window opens (pitboard_sites).
export const siteOf = (tool: Tool) =>
  tool === "claude" ? "claude.ai" : tool === "codex" ? "chatgpt.com" : null;

// the Accounts window's and AccountsPage.xaml's toolbar help, and the macOS inspector's button
// (present/app_words.rs)
export const REFRESH_HELP = "Read every account’s usage again";
export const ADD_HELP = "Sign in to another account";
export const INSPECTOR = "Inspector";
export const INSPECTOR_HELP = "Show or hide the selected account’s details";

// What the macOS inspector says of the account selected (present/accounts.rs `item`'s detail and
// `limit_row`, present/app_words.rs), and in place of one.
export const NO_SELECTION = "No Account Selected";
export const EMAIL = "Email";
export const SIGN_IN_LABEL = "Sign-In";
export const WARM_UP_LABEL = "Warm-Up";
export const warmUpSettings = (front: Front) =>
  titled(front, "Warm-Up Settings…", "Warm-up settings");
export const WARM_UP_OFF = "Off";
export const IN_USE_SAID = "In use";
export const savedUntil = (until: string, lapses: boolean) =>
  lapses ? `Saved sign-in may expire after ${until}` : `Saved sign-in valid until ${until}`;
export const used = (figure: string) => `${figure} used`;
export const inUse = (front: Front) => titled(front, "In Use", "In use");
export const SWITCH = "Switch";
export const SWITCHING_SPOKEN = "Switching";

// The offers on an account, as the row's menus and the Account menu list them
// (present/accounts.rs `actions`, AccountCommands.swift, AccountMenu.cs).
export const switchOffer = (label: string) => `Switch to ${label}`;
export const signInAgain = (front: Front) => titled(front, "Sign In Again…", "Sign in again…");
export const RENAME_OFFER = "Rename…";
export const warmUpNow = (front: Front) => titled(front, "Warm Up Now", "Warm up now");
export const openSite = (site: string) => `Open ${site}`;
export const copyEmail = (front: Front) =>
  titled(front, "Copy Email Address", "Copy email address");
export const removeAccount = (front: Front) => titled(front, "Remove Account…", "Remove account…");

// What Remove Account… asks (present/words/accounts.rs `remove_title`, account_windows/accounts.rs
// `forget_message`).
export const removeTitle = (a: Account, w: World, front: Front) =>
  `Remove “${nameOf(a, w, front)}”?`;
export function removeMessage(a: Account, front: Front): string {
  if (a.tool === "desktop")
    return "Pitboard deletes its saved sign-in and the folders it kept for it, so using it again needs a sign-in in Claude.";
  // front.rs `offers_account_windows`: none on GNOME
  if (front === "gnome")
    return "Pitboard deletes its saved sign-in, so using it again needs a sign-in in your browser.";
  return `Pitboard deletes its saved sign-in and what its ${siteOf(a.tool)} window keeps on ${thisMachine(front)}, so using it again needs a sign-in in your browser.`;
}
export const REMOVE = "Remove";
export const CANCEL = "Cancel";

// The Rename sheet, and the one that names an account just added (present/sheets.rs,
// present/words/sheets.rs).
export const renameTitle = (label: string) => `Rename “${label}”`;
export const RENAMING = "Only its name changes, here and in the command line.";
export const RENAME = "Rename";
export const NAME = "Name";
export const NAME_PROMPT = "work";
export const couldNotRename = (label: string) => `Couldn’t rename ${label}`;
export const nameThisAccount = (front: Front) =>
  titled(front, "Name This Account", "Name this account");
export const naming = (email: string, tool: string) =>
  email ? `${email} is added.` : `The account ${tool} is signed in to is added.`;
export const DONE = "Done";

// present/words/sheets.rs `unusable_name`: why what is typed is no name.
export function unusableName(problem: "empty" | "not_one_word" | "separator"): string {
  switch (problem) {
    case "empty":
      return "Type a name for this account.";
    case "not_one_word":
      return "A name is one word, with no spaces.";
    case "separator":
      return "A name can’t contain “/”.";
  }
}

// present/words/refusals.rs `in_app_refusal` of core/error.rs `LabelTaken`: the name a tool's other
// account has, and whose it is (core/words.rs `login`).
export const nameTaken = (label: string, email: string) =>
  `${label} is already the name of ${email}. Type another name.`;

// The sign-in sheets (present/sheets.rs, present/words/sheets.rs).
export const TOOL = "Tool";
export const ACCOUNT = "Account";
export const addAccount = (front: Front) => titled(front, "Add Account", "Add account");
export const signInAsking = (front: Front) => titled(front, "Sign In…", "Sign in…");
export const signInToAgain = (front: Front, label: string) =>
  titled(front, `Sign In to ${label} Again`, `Sign in to ${label} again`);
export const signInMessage = (tool: string, again: string | null) =>
  again === null
    ? `Pitboard opens ${tool}’s sign-in in your browser, where you sign in to the account you’re adding.`
    : `Pitboard opens ${tool}’s sign-in in your browser, where you sign in to ${again} again.`;
export const signingInTo = (front: Front, tool: string) =>
  titled(front, `Signing In to ${tool}`, `Signing in to ${tool}`);
export const finishInBrowser = (again: string | null) =>
  again === null
    ? "Finish signing in in your browser."
    : `Finish signing in as ${again} in your browser.`;
export const signInPage = (front: Front) => titled(front, "Sign-In Page", "Sign-in page");
export const copyLink = (front: Front) => titled(front, "Copy Link", "Copy link");
export const OPEN = "Open";
export const WAITING_FOR_BROWSER = "Waiting for your browser";
export const LATER = "Later";

// A sign-in inside Claude, which saves the account in use and signs Claude out first
// (present/sheets.rs `in_app_form`, `in_app_signing_in_text`).
export const CONTINUE = "Continue";
// present/words/sheets.rs `signs_out_first`: what Pitboard does first, keeping `again`'s sign-in as
// it is
export const signsOutFirst = (app: string, work: string, again: string | null) =>
  `Pitboard ${again === work ? `keeps ${work}’s sign-in as it is` : `saves ${work}`} and signs ${app} out first, without signing out at claude.ai.`;
// present/words/sheets.rs `in_app_steps`, for a Claude Pitboard did not open for it
export function inAppSteps(app: string, again: string | null): string[] {
  const who = again === null ? "the account you’re adding" : `${again} again`;
  const adds = again === null ? "Pitboard adds it" : `Pitboard saves it for ${again}`;
  return [
    `Open ${app}.`,
    `Sign in to ${who}.`,
    `Once it shows your chats, quit ${app} with Command-Q, and ${adds}.`,
  ];
}
export const signingOut = (label: string) => [`Saving ${label}.`, `Saving ${label}`] as const;
// present/words/sheets.rs `in_app_note`, under the steps while Pitboard waits for the app to quit
export const inAppNote = (app: string) =>
  `Don’t choose Log out in ${app}: it ends the sign-in at claude.ai, and Pitboard can’t bring it back.`;
export const waitingToQuit = (app: string) => `Waiting for ${app} to quit`;
export const switchBackTo = (front: Front, label: string) =>
  titled(front, `Switch Back to ${label}`, `Switch back to ${label}`);

// The notice a tool's last switch leaves (present/notices.rs `of_last_switch`,
// present/words/notices.rs).
export const switchedTo = (tool: string | null, label: string) =>
  tool ? `Switched ${tool} to ${label}` : `Switched to ${label}`;
export const signedInTo = (tool: string | null, label: string) =>
  tool ? `Signed in to ${label} in ${tool}` : `Signed in to ${label}`;
export const sessionsFollowIn = (tool: string | null) =>
  tool ? `${tool} sessions already open follow in` : "Sessions already open follow in";
export function signedInNow(name: string, again: boolean, app: string | null): string {
  if (again)
    return app
      ? `Signed in to ${name} again. ${app} is signed in to it now.`
      : `Signed in to ${name} again. It’s the account in use now.`;
  return app
    ? `Added ${name}, the account ${app} is signed in to now.`
    : `Added ${name}, the account signed in now.`;
}
export const restartNotice = (program: string, from: string) =>
  `Any ${program} session started before this switch keeps using ${from || "the account it started with"} until it is quit and started again.`;
// present/words/notices.rs `follows`: a Claude Desktop switch, with Claude closed
export const CLAUDE_OPENS = "Claude opens signed in to it.";
export const DISMISS = "Dismiss";
export const switchBack = (front: Front) => titled(front, "Switch Back", "Switch back");
// present/mod.rs `spoken_severity`
export const severitySaid = { info: "Note", warning: "Warning" } as const;

// The setup step a tool with one account shows (present/setup.rs `OnlyOne`).
export const addASecond = (tool: string | null) =>
  tool ? `Add a second ${tool} account` : "Add a second account";
export function onlyOneDetail(label: string, tool: Tool, shown: string | null): string {
  const adding =
    tool === "desktop"
      ? `To add another, ${program(tool)} signs out while you sign in to it.`
      : "Add another to switch between them.";
  return `${label} is your only ${shown ? `${shown} ` : ""}account. ${adding}`;
}
export const addAccountAsks = (front: Front) => titled(front, "Add Account…", "Add account…");
export const notNow = (front: Front) => titled(front, "Not Now", "Not now");

// What the window's notice of a tool's last switch or new login says, at `now`, or null where it
// says nothing: a Claude Code switch's notice is its countdown, and goes with it (present/notices.rs
// `of_last_switch`; Banner.swift), unless it offers Switch Back, as a switch Pitboard made by
// itself does. Codex's warns that running sessions keep the old account.
export interface Notice {
  title: string;
  lines: string[];
  countdown: string | null;
  severity: "info" | "warning";
  back: string | null;
}
export function noticeOf(
  s: Pick<State, "now" | "accounts" | "inUse" | "lastSwitch" | "newLogin">,
  tool: Tool,
  front: Front,
): Notice | null {
  const last = s.lastSwitch[tool];
  const login = s.newLogin[tool];
  if (!last && !login) return null;
  const shown = toolIfShown({ ...s, readAt: s.now }, front, tool);
  const app = tool === "desktop" ? program(tool) : null;
  const said = login ? signedInNow(login.label, !login.added, app) : null;
  // a sign-in inside Claude to an account not in use before is said as a switch to it, in place
  // of what Claude's opening says
  const asSwitch = !said || !!login?.opens;
  const opens =
    tool === "desktop" && last && last.to === (login?.label ?? last.to) ? CLAUDE_OPENS : null;
  const lines = (asSwitch ? [said ?? opens] : [opens, said]).filter((l): l is string => !!l);
  const restart = tool === "codex" && last && !login;
  if (restart) lines.push(restartNotice("codex", last.from));
  const running = last?.followsAt != null && last.followsAt > s.now;
  const countdown = running
    ? `${sessionsFollowIn(shown)} ${countdownOf(last!.followsAt!, s.now)}`
    : null;
  const back = last?.back ? switchBack(front) : null;
  if (lines.length === 0 && !countdown && !back) return null;
  return {
    title: asSwitch ? switchedTo(shown, last!.to) : signedInTo(shown, login!.label),
    lines,
    countdown,
    severity: restart ? "warning" : "info",
    back,
  };
}

// The step asking for a second account of the first tool with one, unless put off for it
// (present/setup.rs `footing`, `OnlyOne`).
export function setupStepOf(
  w: World,
  declined: Tool[],
  front: Front,
): { tool: Tool; title: string; detail: string } | null {
  for (const [tool] of tools) {
    if (!toolOn[tool].includes(platformOf(front)) || declined.includes(tool)) continue;
    const own = w.accounts.filter((a) => a.tool === tool);
    if (own.length !== 1 || w.inUse[tool] !== own[0].label) continue;
    // Claude Desktop is named always, as the account's app is
    const shown = tool === "desktop" ? toolName(tool) : toolIfShown(w, front, tool);
    return { tool, title: addASecond(shown), detail: onlyOneDetail(own[0].label, tool, shown) };
  }
  return null;
}

// The keys a row answers, as aria-keyshortcuts names them: Delete forgets an account not in use,
// on macOS Forward Delete too and ⌘C, on Windows F2 and Ctrl+C (AccountsWindow.swift
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
