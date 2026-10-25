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
  winGeneral,
} from "./settings.ts";
import { checksOf, checking, checkedAt, spokenLevel } from "./machine.ts";
import { rowsOf } from "../lib/doctor.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const mac = (file: string) => repo(`apps/macos/Sources/PitboardApp/${file}`);
const win = (file: string) => repo(`apps/windows/${file}`);
const words = repo("crates/pitboard-ffi/src/present/words.rs");

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

test("Windows' General says what SettingsPage.xaml and the updater give it", () => {
  const page = win("Pitboard.Views/SettingsPage.xaml");
  for (const t of [
    winGeneral.section,
    winGeneral.startAtSignIn,
    winGeneral.checkFor,
    winGeneral.checkNow,
  ])
    assert.ok(page.includes(`"${t}"`), t);
  assert.ok(win("Pitboard.Windows/SparkleUpdates.cs").includes(`NoFeed = "${winGeneral.noFeed}"`));
  assert.ok(
    win("Pitboard.Windows/SettingsViewModel.cs").includes(`BetaNote = "${winGeneral.betaNote}"`),
  );
  const vm = win("Pitboard.Windows/SettingsViewModel.cs");
  for (const [title, url] of winAbout.links)
    assert.ok(vm.includes(`new("${title}", "${url}")`) || title === "Pitboard help", title);
  assert.ok(win("Pitboard.Windows/IconMenu.cs").includes(`HelpLink = "${winAbout.links[1][1]}"`));
});

test("Advanced says what AdvancedSettings.swift, SettingsPage.xaml and words.rs give it", () => {
  assert.ok(mac("Settings/AdvancedSettings.swift").includes(`"${renew}"`));
  assert.ok(mac("Settings/AdvancedSettings.swift").includes(`LabeledContent("${inTerminal}")`));
  assert.ok(win("Pitboard.Views/SettingsPage.xaml").includes(`Header="${renew}"`));
  assert.ok(words.includes(`"${updateNote}"`));
  assert.ok(words.includes('Os::MacOs => "Your Mac’s own scheduler"'));
  assert.ok(
    words.includes(
      '"{scheduler} renews your parked logins once a day, so one you leave alone doesn’t expire."',
    ),
  );
  assert.ok(words.includes('"{about} A parked Claude Desktop login is never renewed."'));
  assert.ok(renewalAbout("macos").endsWith("A parked Claude Desktop login is never renewed."));
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
  assert.equal(lines.find((l) => l.name.startsWith("Claude Code’s new"))?.detail, "Inactive");
  // advice under a check that passed is not shown
  assert.equal(lines.find((l) => l.name === "Codex login store")?.advice, null);
  assert.ok(words.includes('"storage_v5" => "Claude Code’s new login storage".into(),'));
  assert.ok(words.includes(`Level::Ok => "${spokenLevel.ok}"`));
  assert.ok(words.includes('format!("Checked at {clock}")'));
  assert.equal(checkedAt("20:51"), "Checked at 20:51");
  assert.equal(checking("windows"), "Checking this PC…");
});
