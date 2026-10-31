// What Settings' General and Advanced show, in each app's words (present/app_words.rs for macOS,
// SettingsPage.xaml and SettingsViewModel.cs for Windows, and present/machine.rs and
// present/words/), for the page's one machine: a release from
// Homebrew's pitboard-app cask on macOS, whose command line is the one inside the app, and the
// MSI's install on Windows, whose command line is the one in the app's folder (reference/settings.mdx).
// settings.test.ts finds each in its source.

import type { Front } from "../lib/platform.ts";
import { eachFront, titled } from "../lib/words.ts";

export type MenuBarShows = "nameAndUsage" | "usage" | "icon";

export const macGeneral = {
  openAtLogin: "Open Pitboard at Login",
  menuBarShows: "Menu Bar Shows",
  shows: [
    ["nameAndUsage", "Account and Usage"],
    ["usage", "Usage Only"],
    ["icon", "Icon Only"],
  ] as [MenuBarShows, string][],
  updates: "Updates",
  checks: "Check for Updates Automatically",
  installs: "Download and Install Automatically",
  version: "Version",
  checkNow: "Check Now",
};

// SettingsPage.xaml's General, each card with its icon: what starting at sign-in is for, as the
// welcome says it, automatic checks where the updater runs, and the version beside Check now
export const winGeneral = {
  section: "General",
  startAtSignIn: "Start when you sign in to Windows",
  startNote: "Pitboard watches your limits only while it’s running.",
  checksAutomatically: "Check for updates automatically",
  version: "Version",
  checkNow: "Check now",
  // app_words.rs `no_release_to_update_to`, which a beta says while no full release for Windows is
  // out
  noFeed: "No full release of Pitboard for Windows is out yet, so there is no update to check for.",
  // app_words.rs `beta_updates`
  betaNote: "A beta updates itself to 1.0.0 once that is out. Install a later beta by hand.",
};

// SettingsPage.xaml's Advanced: Keep saved sign-ins fresh, where the command line is, and Uninstall
// Pitboard, which asks first (present/words/machine.rs `uninstall_pitboard`, `UNINSTALL_QUESTION`)
export const winAdvanced = {
  section: "Advanced",
  commandLine: "Command line",
  uninstall: {
    title: "Uninstall Pitboard",
    note: "Deletes Pitboard’s saved sign-ins and settings, and leaves each tool signed in to the account in use.",
    button: "Uninstall…",
    question: "Uninstall Pitboard?",
    says: "Pitboard deletes its saved sign-ins, settings and account windows. Each tool stays signed in to the account in use, and you sign in to the others again to use them.",
    confirm: "Uninstall",
  },
};

// app_words.rs `named_version`
export const named = (version: string) => (version ? `Pitboard ${version}` : "Pitboard");
export const isBeta = (version: string) => version.includes("-beta.");

// AdvancedSettings.swift's, SettingsPage.xaml's and Advanced's switch, in each front's case
export const renew = eachFront((front) =>
  titled(front, "Keep Saved Sign-Ins Fresh", "Keep saved sign-ins fresh"),
);

// present/words/machine.rs `renewal_about`, its machine as each front names it, with Claude
// Desktop enrolled on macOS, the one system it runs on
const refreshes: Record<Front, string> = {
  macos: "Your Mac",
  windows: "Your PC",
  gnome: "This computer",
};
export const renewalAbout = (front: Front) =>
  front === "macos"
    ? `${refreshes.macos} refreshes saved sign-ins, except Claude Desktop’s, once a day so they don’t expire.`
    : `${refreshes[front]} refreshes your saved sign-ins once a day, so one you leave alone doesn’t expire.`;

// Windows' card for the command line a terminal finds, and macOS's heading and row for it
export const inTerminal = "In your terminal";
export const macCommandLine = { heading: "Command Line", path: "Path" };
// where each system's terminal finds the command line that came with the app
export const commandLine = {
  macos: "/opt/homebrew/bin/pitboard",
  windows: "C:\\Users\\me\\AppData\\Local\\Programs\\Pitboard\\bin\\pitboard.exe",
};
// present/words/machine.rs `update_note` for the one inside the app
export const updateNote = "The one inside this app, so it updates with the app.";

// macOS's Uninstall, its button the model's offer, and the note under it
export const macUninstall = {
  title: "Uninstall",
  button: "Uninstall Pitboard…",
  note: "Deletes Pitboard’s saved sign-ins and settings, and leaves each tool signed in to the account in use.",
};

// Windows' About: SettingsPage.xaml's expander, and SettingsViewModel.cs's links under it
export const winAbout = {
  section: "About",
  header: "Pitboard",
  copyright: "Copyright 2026 the Pitboard authors",
  links: [
    ["usepitboard.com", "https://usepitboard.com"],
    ["Pitboard help", "https://docs.usepitboard.com"],
    ["Source code on GitHub", "https://github.com/datlechin/pitboard"],
    ["Report a problem", "https://github.com/datlechin/pitboard/issues"],
    ["Apache License 2.0", "https://github.com/datlechin/pitboard/blob/main/LICENSE"],
  ] as [string, string][],
};

// macOS's build number, as build-number.sh makes it for a version
export function buildNumber(version: string): string {
  const [base, beta] = version.split("-beta.");
  const [major, minor, patch] = base.split(".").map(Number);
  return String(major * 1_000_000 + minor * 10_000 + patch * 100 + (beta ? Number(beta) : 99));
}
