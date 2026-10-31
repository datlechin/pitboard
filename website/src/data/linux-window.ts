// What the Linux app's window shows in the site's figures: its pages, Preferences and dialogs, in
// the model's words for GNOME's front, which it says on both desktops, and the icons each desktop's
// theme gives it. linux-window.test.ts finds each in the file that says it.
import {
  HOUR,
  clock,
  logged,
  share,
  warmUp,
  type Account,
  type Tool,
  type World,
} from "./world.ts";
import {
  section as BEFORE_RUNNING_OUT,
  choices,
  heading as AT_LIMIT,
  shareLabel,
  shareRange,
  terms,
} from "./limits.ts";
import { renewalAbout } from "./settings.ts";
import {
  CANCEL,
  EMAIL,
  RENAME_OFFER,
  SIGN_IN_LABEL,
  SWITCH,
  WARM_UP_LABEL,
  WARM_UP_OFF,
  copyEmail,
  inUse as inUseSaid,
  removeAccount,
  signInAgain,
  signInAsking,
  switchOffer,
  toolIfShown,
  warmUpNow,
} from "./window.ts";
import { header, preferences as pages } from "./linux.ts";
import {
  CLOSING_QUITS,
  SIGN_IN_EXPIRED,
  activity,
  atLimitNote as wordsAtLimitNote,
  hm,
  isInUse,
  limitRow,
  limitTitle,
  listed,
  settings,
  signInExpired,
  switchSpoken,
  thisMachine,
  titled,
  toolsOn,
  updated,
  warmsAgainNote,
  type LimitRow,
} from "../lib/words.ts";
import type { AtLimit, Preferences } from "../lib/model.ts";
import type { Desktop } from "../lib/platform.ts";

const FRONT = "gnome";

// local_time.rs: GNOME's own English form of a day, "Nov 9", in the world's zone.
const months = "Jan Feb Mar Apr May Jun Jul Aug Sep Oct Nov Dec".split(" ");
const local = (epoch: number) => new Date((epoch + 7 * HOUR) * 1000);
export const date = (epoch: number) => {
  const d = local(epoch);
  return `${months[d.getUTCMonth()]} ${d.getUTCDate()}`;
};

// The icon each desktop's theme gives a name the window asks for, as the KDE portal read found
// (research/kde-portal.md): GNOME's Adwaita, and on KDE Plasma Breeze's actions/16, GTK's own
// for the dialog icons and libadwaita's for its own. For battery-level-50 and web-browser, which
// Breeze lacks, the page draws Adwaita's, the copies the app is to carry in its resources, and
// not the image-missing Plasma draws in their place without them.
export type Icon =
  | "list-add"
  | "view-refresh"
  | "open-menu"
  | "window-close"
  | "window-minimize"
  | "window-maximize"
  | "go-previous"
  | "go-next"
  | "view-more"
  | "object-select"
  | "edit-copy"
  | "dialog-information"
  | "dialog-warning"
  | "dialog-error"
  | "preferences-system"
  | "battery-level-50"
  | "preferences-other"
  | "insert-link"
  | "web-browser"
  | "pan-down"
  | "value-decrease"
  | "value-increase"
  | "adw-external-link"
  | "adw-entry-edit";
const fromGtk: Icon[] = ["dialog-information", "dialog-warning", "dialog-error"];
export const notInBreeze: Icon[] = ["battery-level-50", "web-browser"];
export function iconOf(name: Icon, desktop: Desktop): string {
  if (name.startsWith("adw-")) return `libadwaita-${name}`;
  if (desktop === "gnome" || notInBreeze.includes(name)) return `adwaita-${name}`;
  return fromGtk.includes(name) ? `gtk-${name}` : `breeze-${name}`;
}

// A limit's bar fills with the accent, the warning or the error colour by its level
// (data/style.css), the model's words beside it.
export const levelOf = { g: "plenty", y: "low", r: "out" } as const;

// An account's menu (window/account_group.rs `offer`): what it does to the account but the
// header's own button, each dimmed while the model holds it back, then copying its address, then
// removing it (present/accounts.rs `actions`): Switch while the header offers it, Sign In Again
// while no sign-in runs, Warm Up Now while a warm-up is due, and Remove for an account not in use.
// `now` says what holds them back; none of the page's drawn moments is switching, signing in or
// due a warm-up.
export interface Offer {
  title: string;
  offer: "use" | "sign-in-again" | "rename" | "warm" | "copy" | "forget";
  enabled: boolean;
}
export interface Holding {
  switching: string | null;
  signing: boolean;
  warmDue: boolean;
}
export const headerOffer = (a: Account, w: World, switching: string | null = null) =>
  isInUse(a, w) || switching === a.id ? null : signInExpired(a, w) ? "sign-in-again" : "use";
export function accountMenu(
  a: Account,
  w: World,
  now: Holding = { switching: null, signing: false, warmDue: false },
): Offer[][] {
  const inUse = isInUse(a, w);
  const header = headerOffer(a, w, now.switching);
  const switches = header === "use" && now.switching === null && !now.signing;
  const does: Offer[] = [
    { title: switchOffer(a.label), offer: "use" as const, enabled: switches },
    { title: signInAgain(FRONT), offer: "sign-in-again" as const, enabled: !now.signing },
    { title: RENAME_OFFER, offer: "rename" as const, enabled: true },
    ...(a.tool === "claude"
      ? [{ title: warmUpNow(FRONT), offer: "warm" as const, enabled: now.warmDue }]
      : []),
  ].filter((o) => o.offer !== header);
  return [
    does,
    a.email ? [{ title: copyEmail(FRONT), offer: "copy", enabled: true }] : [],
    [{ title: removeAccount(FRONT), offer: "forget", enabled: !inUse }],
  ];
}

export interface Group {
  id: string;
  tool: Tool;
  label: string;
  // present/words/accounts.rs `tool_and_plan`: "Claude Code, Max 20x"
  description: string | null;
  inUse: boolean;
  button: { title: string; spoken: string } | null;
  menu: Offer[][];
  limits: (LimitRow & { title: string; fill: (typeof levelOf)[keyof typeof levelOf] })[];
  email: string | null;
  signIn: string | null;
  warm: string | null;
}

// The Sign-In row: in use, expired, or until when its saved sign-in is valid
// (present/accounts.rs `detail`).
export function signInOf(a: Account, w: World): string | null {
  if (isInUse(a, w)) return "In use";
  if (signInExpired(a, w)) return SIGN_IN_EXPIRED;
  return a.refreshExpiresAt === undefined
    ? null
    : `Saved sign-in valid until ${date(a.refreshExpiresAt)}`;
}

// The accounts page: a group per account, each tool's in the model's order (present/accounts.rs
// `sections`), with the notice of the switch to work above them after it.
export function accountsPage(w: World) {
  return {
    title: header.title,
    updated: updated(w.readAt),
    groups: toolsOn("linux").flatMap(([tool]) =>
      listed(w, tool).map((a): Group => {
        const inUse = isInUse(a, w);
        const expired = signInExpired(a, w);
        const said = [toolIfShown(w, FRONT, a.tool), a.plan].filter(Boolean);
        return {
          id: a.id,
          tool,
          label: a.label,
          description: said.length ? said.join(", ") : null,
          inUse,
          button: inUse
            ? null
            : expired
              ? { title: signInAsking(FRONT), spoken: signInAgain(FRONT) }
              : { title: SWITCH, spoken: switchSpoken(a) },
          menu: accountMenu(a, w),
          limits: a.limits.map((l) => {
            const row = limitRow(l, w);
            return { ...row, title: limitTitle(l), fill: levelOf[row.level] };
          }),
          email: a.email || null,
          signIn: signInOf(a, w),
          warm: a.tool === "claude" ? (warmsAgainNote(a) ?? WARM_UP_OFF) : null,
        };
      }),
    ),
  };
}

// The group's rows and buttons (window/account_group.rs, present/app_words.rs `window`).
export const group = {
  inUse: inUseSaid(FRONT),
  switching: "Switching",
  account: "Account",
  email: EMAIL,
  signIn: SIGN_IN_LABEL,
  warmUp: WARM_UP_LABEL,
  // present/words/machine.rs `warm_up_settings`, which GNOME's front words as its own
  warmUpPreferences: `${WARM_UP_LABEL} ${settings(FRONT, true)}`,
  dismiss: "Dismiss",
};

// The notice of the last switch, its countdown ticking (present/notices.rs `of_last_switch`). A
// switch made by hand offers no Switch Back: the model keeps a way back only where switching by
// itself made it.
export const switchNotice = {
  title: "Switched Claude Code to work",
  countdown: "Claude Code sessions already open follow in 0:21",
  switchBack: "Switch Back",
};

// Diagnostics, which the page leaves checking (present/app_words.rs `window`, words/checks.rs).
export const diagnostics = {
  title: "Diagnostics",
  checking: `Checking ${thisMachine(FRONT)}…`,
  checkAgain: "Check Again",
  checkAgainHelp: "Make every check again",
  copyReport: "Copy Report",
  copyReportHelp:
    "Copy the checks for a bug report, with email addresses and account identifiers hidden",
};

// Activity's columns, the date numeric and By always quieter, Result where the change was done
// (pages/activity.rs `columns`), newest first.
export const activityPage = {
  title: "Activity",
  refreshHelp: "Read the log again",
  columns: ["Date", "Change", "Account", "Result", "By"],
  rows: activity(logged, "linux"),
};

// What a desktop with no status item says, and one with one does not (present/words/setup.rs,
// present/words/machine.rs): on stock GNOME, closing the window quits Pitboard, and the watch
// service goes on switching at the share or never.
const watchAbout = (switching: boolean, at = share) =>
  switching
    ? `${CLOSING_QUITS} Turned on, ${thisMachine(FRONT)} goes on switching Claude Code at ${at}% and warming up the accounts ${WARM_UP_LABEL} turns on, from each login.`
    : `${CLOSING_QUITS} Turned on, ${thisMachine(FRONT)} goes on warming up the accounts ${WARM_UP_LABEL} turns on, from each login, and never switches Claude Code.`;

// The at-limit note under Before an account runs out, then, where nothing keeps Pitboard running
// once its window is closed, what goes on then: switching, or with the watch service on its
// warm-ups alone (present/machine.rs `at_limit`).
export function atLimitNote(
  p: Pick<Preferences, "atLimit" | "share">,
  statusItem: boolean,
  watched = false,
): string | null {
  const closed = statusItem
    ? null
    : p.atLimit === "switch"
      ? watched
        ? "Once its window is closed, the watch service goes on switching it."
        : "Once its window is closed, nothing switches it unless Watch in the Background is on."
      : !watched
        ? null
        : p.atLimit === "notify"
          ? "Once its window is closed, nothing notifies you, and the watch service only warms up."
          : "The watch service only warms up, and never switches.";
  const said = [wordsAtLimitNote({ ...p, chosen: true }), closed].filter(Boolean);
  return said.length ? said.join(" ") : null;
}

// Preferences, as preferences/ shows the model's words for GNOME's front, with Switch Automatically
// chosen and the command line from the repository's package, /usr/bin/pitboard beside
// /usr/bin/pitboard-app, which the core calls the app's own (core/app.rs `find_command_line`).
export const prefs = {
  title: pages.title,
  pages: [
    ["general", pages.pages[0], "preferences-system"],
    ["limits", pages.pages[1], "battery-level-50"],
    ["advanced", pages.pages[2], "preferences-other"],
  ] as [id: "general" | "limits" | "advanced", title: string, icon: Icon][],
  general: {
    closingQuits: CLOSING_QUITS,
    startAtLogin: "Start Pitboard at Login",
    startNote: "Pitboard watches your limits only while it’s running.",
    version: "Version",
  },
  limits: {
    title: BEFORE_RUNNING_OUT,
    heading: AT_LIMIT,
    choices: choices.map((c) => [c.value, titled(FRONT, c.mac, c.win)] as [AtLimit, string]),
    standing: `Watching work: 40% of its weekly limit, as of ${hm(clock.read)}.`,
    share: { label: shareLabel, range: shareRange, at: share },
    terms,
    warm: {
      title: WARM_UP_LABEL,
      note: (statusItem: boolean, watched = false) =>
        `Pitboard starts each account’s 5-hour limit again as soon as it resets, within these hours, ${statusItem ? "while it’s open" : watched ? "and the watch service goes on once its window is closed" : "while its window is open"}.`,
      from: warmUp.from,
      to: warmUp.to,
      standing: `Next warm-up: ${warmUp.accounts[0]} at ${hm(clock.warmsAgain)}.`,
    },
  },
  advanced: {
    renew: "Keep Saved Sign-Ins Fresh",
    renewAbout: renewalAbout(FRONT),
    watch: "Watch in the Background",
    watchAbout,
    commandLine: "Command Line",
    path: "Path",
    inTerminal: "/usr/bin/pitboard",
    updateNote: "The one inside this app, so it updates with the app.",
    uninstall: "Uninstall",
    uninstallNote:
      "Deletes Pitboard’s saved sign-ins and settings, and leaves each tool signed in to the account in use.",
    uninstallButton: "Uninstall Pitboard…",
  },
  // the question before it, and what the app says once Pitboard's data is gone, with Fedora's
  // command (present/removal.rs `package_removal`)
  uninstallQuestion: {
    title: "Uninstall Pitboard?",
    says: "Pitboard deletes its saved sign-ins and settings. Each tool stays signed in to the account in use, and you sign in to the others again to use them.",
    cancel: CANCEL,
    uninstall: "Uninstall",
  },
  removeTheApp: {
    title: "Remove the App",
    message:
      "Pitboard’s saved sign-ins and settings are deleted. Remove the app and its command line with this command in a terminal.",
    command: "sudo dnf remove pitboard-app pitboard",
    copy: "Copy",
    quit: "Quit",
  },
};

// The warm-up's switches, a Claude Code account each, the one turned on saying when it warms up.
// Each Claude Code account in the core's order, which warm/mod.rs `overview` keeps, not the one in
// use first.
export const warmAccounts = (w: World) =>
  w.accounts
    .filter((a) => a.tool === "claude")
    .map((a) => ({
      id: a.id,
      label: a.label,
      on: warmUp.accounts.includes(a.label),
      standing: warmUp.accounts.includes(a.label) ? warmsAgainNote(a) : null,
    }));

// Open Link with no link waiting, and with one (present/words/windows.rs `no_link`, `no_windows`;
// app_words.rs `gnome_words`).
export const openLink = {
  title: "Open Link",
  none: {
    title: "No Link to Open",
    detail: "Paste a claude.ai or chatgpt.com link, or drop one here, to open it in your browser.",
    paste: "Paste Link",
  },
  link: {
    title: "Open in Your Browser",
    detail: (site: string) =>
      `Pitboard has no ${site} window of its own here, so your browser opens this link as the account it’s signed in to.`,
    open: "Open in Browser",
  },
};

// Keyboard Shortcuts' rows, the keys as GTK labels an accelerator (gtk_accelerator_get_label).
const keyNames: Record<string, string> = { comma: ",", question: "?", Left: "Left" };
export const acceleratorLabel = (accelerator: string) =>
  accelerator
    .replace(/<Control>/g, "Ctrl+")
    .replace(/<Alt>/g, "Alt+")
    .replace(/(?<=\+|^)(\w+)$/, (k) => keyNames[k] ?? k.toUpperCase());

// About Pitboard, libadwaita's dialog from the metainfo: its rows are libadwaita's own words
// (adw-about-dialog.ui).
export const about = {
  name: "Pitboard",
  developer: "Pitboard",
  rows: [["Details"], ["Support Questions", "Report an Issue"], ["Legal"]],
};
