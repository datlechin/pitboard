// What Settings' General and Advanced show, in each app's words (GeneralSettings.swift,
// AdvancedSettings.swift, MenuBarShows.swift, SettingsPage.xaml, SettingsViewModel.cs, and
// present/machine.rs and present/words.rs), for the page's one machine: a release from
// Homebrew's pitboard-app cask on macOS, whose command line is the one inside the app, and the
// MSI's install on Windows, whose command line is the one in the app's folder (reference/settings.mdx).
// settings.test.ts finds each in its source.

export type MenuBarShows = "nameAndUsage" | "usage" | "icon";

export const macGeneral = {
  openAtLogin: "Open Pitboard at login",
  menuBarShows: "Menu bar shows",
  shows: [
    ["nameAndUsage", "Account and usage"],
    ["usage", "Usage only"],
    ["icon", "Icon only"],
  ] as [MenuBarShows, string][],
  checks: "Check for updates automatically",
  installs: "Download and install updates automatically",
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

// Versions.cs `Named` and AppVersion.swift `named`
export const named = (version: string) => (version ? `Pitboard ${version}` : "Pitboard");
export const isBeta = (version: string) => version.includes("-beta.");

export const renew = "Renew parked logins daily";

// words.rs `renewal_about`, with Claude Desktop enrolled on macOS
export const renewalAbout = (system: "macos" | "windows") =>
  system === "macos"
    ? "Your Mac’s own scheduler renews your parked logins once a day, so one you leave alone doesn’t expire. A parked Claude Desktop login is never renewed."
    : "Your PC’s own scheduler renews your parked logins once a day, so one you leave alone doesn’t expire.";

export const inTerminal = "In your terminal";
// where each system's terminal finds the command line that came with the app
export const commandLine = {
  macos: "/opt/homebrew/bin/pitboard",
  windows: "C:\\Users\\me\\AppData\\Local\\Programs\\Pitboard\\bin\\pitboard.exe",
};
// words.rs `update_note` for the one inside the app
export const updateNote = "The one inside this app, so it updates with the app.";

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
