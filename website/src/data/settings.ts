// What Settings' General and Advanced show, in each app's words (present/app_words.rs for macOS,
// SettingsPage.xaml and SettingsViewModel.cs for Windows, and present/machine.rs and
// present/words.rs), for the page's one machine: a release from
// Homebrew's pitboard-app cask on macOS, whose command line is the one inside the app, and the
// MSI's install on Windows, whose command line is the one in the app's folder (reference/settings.mdx).
// settings.test.ts finds each in its source.

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

export const winGeneral = {
  section: "General",
  startAtSignIn: "Start when you sign in to Windows",
  checkFor: "Check for updates",
  checkNow: "Check now",
  // SparkleUpdates.cs `NoFeed`, which a beta says while no full release for Windows is out
  noFeed: "No full release of Pitboard for Windows is out yet, so there is no update to check for.",
  // SettingsViewModel.cs `BetaNote`
  betaNote: "A beta updates itself to 1.0.0 once that is out. Install a later beta by hand.",
};

// Versions.cs `Named`
export const named = (version: string) => (version ? `Pitboard ${version}` : "Pitboard");
export const isBeta = (version: string) => version.includes("-beta.");

// AdvancedSettings.swift's and SettingsPage.xaml's switch, in each system's case
export const renew = { macos: "Keep Saved Sign-Ins Fresh", windows: "Keep saved sign-ins fresh" };

// words.rs `renewal_about`, with Claude Desktop enrolled on macOS
export const renewalAbout = (system: "macos" | "windows") =>
  system === "macos"
    ? "Your Mac refreshes saved sign-ins, except Claude Desktop’s, once a day so they don’t expire."
    : "Your PC refreshes your saved sign-ins once a day, so one you leave alone doesn’t expire.";

// Windows' card for the command line a terminal finds, and macOS's heading and row for it
export const inTerminal = "In your terminal";
export const macCommandLine = { heading: "Command Line", path: "Path" };
// where each system's terminal finds the command line that came with the app
export const commandLine = {
  macos: "/opt/homebrew/bin/pitboard",
  windows: "C:\\Users\\me\\AppData\\Local\\Programs\\Pitboard\\bin\\pitboard.exe",
};
// words.rs `update_note` for the one inside the app
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
