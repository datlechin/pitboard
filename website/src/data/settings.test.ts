import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  buildNumber,
  macCommandLine,
  macGeneral,
  macUninstall,
  renew,
  renewalAbout,
  updateNote,
  winAbout,
  winAdvanced,
  winGeneral,
} from "./settings.ts";
import { checking } from "./machine.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const mac = (file: string) => repo(`apps/macos/Sources/PitboardApp/${file}`);
const win = (file: string) => repo(`apps/windows/${file}`);
const words = repo("crates/pitboard-model/src/present/words.rs");
// app_words.rs with its string literals' line continuations joined and its code on one line
const appWords = repo("crates/pitboard-model/src/present/app_words.rs")
  .replace(/\\\n\s*/g, "")
  .replace(/\s+/g, " ")
  .replace(/\( /g, "(");
// app_words.rs and words.rs with their line continuations joined, where every word the Windows
// views draw is the model's
const modelWords = (repo("crates/pitboard-model/src/present/app_words.rs") + words).replace(
  /\\\n\s*/g,
  "",
);
const owned = (said: string) => modelWords.includes(`"${said}"`);

test("macOS's General says what app_words.rs gives GeneralSettings.swift and MenuBarShows.swift", () => {
  const general = mac("Settings/GeneralSettings.swift");
  for (const [word, made, said] of [
    ["openAtLogin", "open_at_login: words::open_at_login(os).0.into(),", macGeneral.openAtLogin],
    ["menuBarShows", 'menu_bar_shows: mac("', macGeneral.menuBarShows],
    ["updates", 'updates: "', macGeneral.updates],
    [
      "checkForUpdatesAutomatically",
      'check_for_updates_automatically: titled(os, "',
      macGeneral.checks,
    ],
    [
      "downloadAndInstallAutomatically",
      'download_and_install_automatically: mac("',
      macGeneral.installs,
    ],
    ["version", 'version: "', macGeneral.version],
    ["checkNow", 'check_now: titled(os, "', macGeneral.checkNow],
  ]) {
    assert.ok(general.includes(`said.${word}`), word);
    assert.ok(appWords.includes(made.endsWith('"') ? `${made}${said}"` : made), said);
  }
  assert.ok(words.includes(`"${macGeneral.openAtLogin}",`));
  const shows = mac("MenuBar/MenuBarShows.swift");
  for (const [value, title] of macGeneral.shows) {
    const word = { nameAndUsage: "accountAndUsage", usage: "usageOnly", icon: "iconOnly" }[value];
    assert.ok(shows.includes(`case .${value}: words.settings.${word}`), title);
    assert.ok(appWords.includes(`: mac("${title}"),`), title);
  }
});

test("Windows' General says what SettingsPage.xaml binds from app_words(), and the updater", () => {
  const page = win("Pitboard.Views/SettingsPage.xaml");
  for (const [field, said] of [
    ["General", winGeneral.section],
    ["OpenAtLogin", winGeneral.startAtSignIn],
    ["CheckForUpdatesAutomatically", winGeneral.checksAutomatically],
    ["Version", winGeneral.version],
    ["CheckNow", winGeneral.checkNow],
  ]) {
    assert.ok(page.includes(`{x:Bind shown:Words.Settings.${field}}`), field);
    assert.ok(owned(said), said);
  }
  // under Start when you sign in to Windows, what the welcome says under its own
  const vm = win("Pitboard.Windows/SettingsViewModel.cs");
  assert.ok(vm.includes("_start.Note(Words.Settings.OpenAtLoginNote)"));
  assert.ok(owned(winGeneral.startNote));
  assert.ok(
    win("Pitboard.Windows/SparkleUpdates.cs").includes("Words.Settings.NoReleaseToUpdateTo"),
  );
  assert.ok(vm.includes("Words.Settings.BetaUpdates"));
  assert.ok(owned(winGeneral.noFeed) && owned(winGeneral.betaNote));
  // About's links, the site's titled by its host
  const [site, ...rest] = winAbout.links;
  assert.ok(vm.includes(`private const string Site = "${site[1]}";`));
  assert.equal(site[0], new URL(site[1]).host);
  const fields = ["Help", "SourceCode", "ReportAProblem", "License"];
  rest.forEach(([title, url], i) => {
    assert.ok(owned(title), title);
    assert.ok(
      vm.includes(
        `new(Words.Settings.${fields[i]}, ${i === 0 ? "IconMenu.HelpLink" : `"${url}"`})`,
      ),
      title,
    );
  });
  assert.ok(win("Pitboard.Windows/IconMenu.cs").includes(`HelpLink = "${winAbout.links[1][1]}"`));
  // About's name and copyright, as the expander binds them
  assert.ok(page.includes('Header="{x:Bind shown:Words.AppName}"'));
  assert.ok(page.includes('Description="{x:Bind shown:Words.Settings.Copyright}"'));
  assert.ok(owned(winAbout.header) && owned(winAbout.copyright));
});

test("Advanced says what AdvancedSettings.swift, SettingsPage.xaml and words.rs give it", () => {
  const advanced = mac("Settings/AdvancedSettings.swift");
  assert.ok(advanced.includes("said.keepSavedSignInsFresh"));
  assert.ok(appWords.includes("keep_saved_sign_ins_fresh: words::keep_saved_sign_ins_fresh(os),"));
  assert.ok(words.includes(`titled(os, "${renew.macos}", "${renew.windows}")`));
  assert.ok(advanced.includes("Text(said.commandLine)"));
  assert.ok(appWords.includes(`command_line: titled(os, "${macCommandLine.heading}", `));
  assert.ok(advanced.includes("LabeledContent(said.path)"));
  assert.ok(appWords.includes(`path: "${macCommandLine.path}".into(),`));
  assert.ok(advanced.includes("LabeledContent(words.settings.uninstall)"));
  assert.ok(words.includes(`pub(crate) const UNINSTALL: &str = "${macUninstall.title}";`));
  assert.ok(advanced.includes("Text(words.settings.uninstallNote)"));
  assert.ok(appWords.includes(`uninstall_note: "${macUninstall.note}"`));
  assert.ok(words.includes(`Os::MacOs | Os::Linux => "${macUninstall.button}",`));
  // each of Windows' Advanced cards binds its header from app_words()
  const page = win("Pitboard.Views/SettingsPage.xaml");
  for (const [field, said] of [
    ["KeepSavedSignInsFresh", renew.windows],
    ["CommandLine", winAdvanced.commandLine],
    ["Uninstall", winAdvanced.uninstall.title],
  ]) {
    assert.ok(page.includes(`Header="{x:Bind shown:Words.Settings.${field}}"`), field);
    assert.ok(owned(said), said);
  }
  assert.ok(page.includes('Text="{x:Bind shown:Words.Settings.Advanced}"'));
  assert.ok(owned(winAdvanced.section));
  // Uninstall Pitboard's note, button and question, the model's
  const u = winAdvanced.uninstall;
  assert.ok(
    win("Pitboard.Windows/SettingsViewModel.cs").includes("_ => Words.Settings.UninstallNote,"),
  );
  for (const said of [u.note, u.button, u.question, u.says, u.confirm])
    assert.ok(owned(said), said);
  assert.ok(words.includes(`"${updateNote}"`));
  const joined = words.replace(/\\\n\s*/g, "");
  assert.ok(
    joined.includes('Os::MacOs => "Your Mac",') && joined.includes('Os::Windows => "Your PC",'),
  );
  assert.ok(
    joined.includes(
      '"{machine} refreshes saved sign-ins, except Claude Desktop’s, once a day so they don’t expire."',
    ),
  );
  assert.ok(
    joined.includes(
      '"{machine} refreshes your saved sign-ins once a day, so one you leave alone doesn’t expire."',
    ),
  );
  // the page's Mac has Claude Desktop accounts, and its PC none
  assert.ok(renewalAbout("macos").includes("except Claude Desktop’s"));
  assert.ok(renewalAbout("windows").startsWith("Your PC refreshes your saved sign-ins"));
});

test("the About panel numbers a build as build-number.sh does", () => {
  assert.equal(buildNumber("0.15.0"), "150099");
  assert.equal(buildNumber("1.0.0-beta.1"), "1000001");
  assert.ok(
    repo("apps/macos/Tests/PitboardAppTests/AppVersionTests.swift").includes(
      '"CFBundleVersion": "150099"',
    ),
  );
});

test("This PC says what present/words.rs `checking` says while it waits", () => {
  assert.ok(words.includes('format!("Checking {}…", this_machine(os))'));
  assert.equal(checking("windows"), "Checking this PC…");
});
