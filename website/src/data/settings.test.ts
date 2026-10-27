import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  buildNumber,
  inTerminal,
  macGeneral,
  renew,
  renewalAbout,
  updateNote,
  winAbout,
  winAdvanced,
  winGeneral,
} from "./settings.ts";
import { appNames, checksOf, checking, checkedAt, spokenLevel } from "./machine.ts";
import { rowsOf } from "../lib/doctor.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const mac = (file: string) => repo(`apps/macos/Sources/PitboardApp/${file}`);
const win = (file: string) => repo(`apps/windows/${file}`);
const words = repo("crates/pitboard-ffi/src/present/words.rs");
// app_words.rs and words.rs with their line continuations joined, where every word the Windows
// views draw is the model's
const appWords = (repo("crates/pitboard-ffi/src/present/app_words.rs") + words).replace(
  /\\\n\s*/g,
  "",
);
const owned = (said: string) => appWords.includes(`"${said}"`);

test("macOS's General says what GeneralSettings.swift and MenuBarShows.swift give it", () => {
  const general = mac("Settings/GeneralSettings.swift");
  for (const t of [
    macGeneral.openAtLogin,
    macGeneral.menuBarShows,
    macGeneral.checks,
    macGeneral.installs,
    macGeneral.checkNow,
  ])
    assert.ok(general.includes(`"${t}"`), t);
  const shows = mac("MenuBar/MenuBarShows.swift");
  for (const [value, title] of macGeneral.shows)
    assert.ok(shows.includes(`case .${value}: "${title}"`), title);
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
  assert.ok(mac("Settings/AdvancedSettings.swift").includes(`"${renew.macos}"`));
  assert.ok(mac("Settings/AdvancedSettings.swift").includes(`LabeledContent("${inTerminal}")`));
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

test("This Mac words doctor's checks as present/machine.rs does", () => {
  const printed = repo(
    "crates/pitboard/src/render/snapshots/pitboard__render__doctor__tests__doctor_on_a_healthy_mac.snap",
  );
  const { lines, summary } = checksOf(rowsOf(printed));
  assert.equal(summary, "Everything Pitboard checks is in order.");
  assert.equal(lines.length, 34);
  assert.deepEqual(lines[0], {
    name: "Security tool",
    level: "ok",
    detail: "/usr/bin/security",
    advice: null,
  });
  assert.equal(lines.find((l) => l.name === "In use")?.detail, "personal, as Anthropic said 20:50");
  assert.equal(lines.find((l) => l.name === "New Sign-In Storage")?.detail, "Inactive");
  // advice under a check that passed is not shown
  assert.deepEqual(
    lines.filter((l) => l.name === "Sign-In Store").map((l) => l.advice),
    [null, null],
  );
  // the app's own names, by doctor's codes, in macOS's title case, and the name doctor prints
  // for each
  const checkName = words
    .slice(words.indexOf("fn check_name("), words.indexOf("fn check_text("))
    .replace(/\s+/g, " ");
  const titles = new Map(
    [
      ...checkName.matchAll(/((?:"\w+" \| )*"\w+") => (?:\{ )?\( ?"([^"]+)", "[^"]+",? ?\)/g),
    ].flatMap(([, codes, title]) =>
      [...codes.matchAll(/"(\w+)"/g)].map(([, code]) => [code, title] as const),
    ),
  );
  for (const [code, , app] of appNames) assert.equal(titles.get(code), app, code);
  const doctor = ["claude.rs", "codex.rs", "desktop.rs", "machine.rs", "parks.rs"]
    .map((f) => repo(`crates/pitboard-core/src/doctor/${f}`))
    .join("\n")
    .replace(/\s+/g, " ");
  for (const [code, name] of appNames)
    assert.ok(
      doctor.includes(`"${code}", "${name}"`) || doctor.includes(`let name = "${name}";`),
      code,
    );
  assert.ok(words.includes(`Level::Ok => "${spokenLevel.ok}"`));
  assert.ok(words.includes(`Level::Warn => "${spokenLevel.warn}"`));
  assert.ok(words.includes('format!("Checked at {clock}")'));
  assert.equal(checkedAt("20:51"), "Checked at 20:51");
  assert.equal(checking("windows"), "Checking this PC…");
});
