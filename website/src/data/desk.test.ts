import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";
import { menuCommands } from "./desk.ts";
import { READING, SWITCHING, openSiteAs, updated } from "../lib/words.ts";
import { clock } from "./world.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
// present/words/, a file for each surface that says the words, read as one
const catalogue = readdirSync(
  new URL("../../../crates/pitboard-model/src/present/words/", import.meta.url),
)
  .sort()
  .map((file) => repo(`crates/pitboard-model/src/present/words/${file}`))
  .join("\n");
const menu = repo("apps/macos/Sources/PitboardApp/MenuBar/MenuBarContent.swift");
// app_words.rs with its string literals' line continuations joined and its code on one line
const appWords = repo("crates/pitboard-model/src/present/app_words.rs")
  .replace(/\\\n\s*/g, "")
  .replace(/\s+/g, " ")
  .replace(/\( /g, "(");
// each command's words as the menu takes them, from app_words() or the macOS app's own
// catalogue, and as app_words.rs makes them
const said: Record<string, [string, string]> = {
  "Add Account…": ["words.menu.addAccount", "add_account: words::add_account(front, true),"],
  Refresh: ["words.menu.refresh", "refresh: words::REFRESH.into(),"],
  "Open Pitboard": [
    "words.menu.openPitboard",
    'open_pitboard: titled(front, "Open Pitboard", "Open Pitboard"),',
  ],
  "Settings…": ["words.menu.settings", "settings: words::settings(front, true),"],
  "Check for Updates…": [
    "macWords.checkForUpdates",
    'check_for_updates: "Check for Updates…".into(),',
  ],
  "About Pitboard": [
    "words.menu.about",
    'about: titled(front, "About Pitboard", "About Pitboard"),',
  ],
  "Pitboard Help": [
    "words.menu.help",
    'Front::MacMenuBar | Front::WindowsNotificationArea => { titled(front, "Pitboard Help", "Pitboard help") }',
  ],
  "Quit Pitboard": ["words.menu.quit", 'Front::MacMenuBar => "Quit Pitboard",'],
};

// From an item's words to the next item: the modifiers SwiftUI gives it.
function item(title: string): string {
  const [taken, made] = said[title];
  assert.ok(appWords.includes(made), made);
  const start = menu.indexOf(taken);
  assert.ok(start >= 0, `MenuBarContent.swift has ${title}`);
  const rest = menu.slice(start + taken.length);
  const end = rest.search(/Button[ ({]|Link\(|SiteMenuItems|\/\/ MARK/);
  return rest.slice(0, end < 0 ? undefined : end);
}

test("the macOS menu's commands have the keys MenuBarContent.swift gives them, and no others", () => {
  assert.ok(catalogue.includes('Front::MacMenuBar if menu_item => "Settings…",'));
  for (const c of menuCommands) {
    if (c === null || c.more) continue;
    const shortcut = /\.keyboardShortcut\("(.)"\)/.exec(item(c.title))?.[1];
    assert.equal(c.keys, shortcut && `⌘${shortcut.toUpperCase()}`, c.title);
  }
});

test("the questions' macOS shortcuts are AccountCommands.swift's: the Account menu, and Remove Account on Command-Delete", () => {
  const commands = repo("apps/macos/Sources/PitboardApp/Window/AccountCommands.swift");
  assert.ok(commands.includes("CommandMenu(words.menu.account)"));
  assert.ok(appWords.includes("account: words::ACCOUNT.into(),"));
  // .delete with SwiftUI's default modifier, Command, on the menu bar's Remove Account
  assert.match(
    commands,
    /if place == \.menuBar \{\s+Button\(remove\.title, role: \.destructive\) \{ command\.act\(remove\) \}\s+\.keyboardShortcut\(\.delete\)\n/,
  );
  assert.ok(
    repo("crates/pitboard-model/src/present/accounts.rs").includes(
      "title: words::remove_account(seen.front),",
    ),
  );
  assert.ok(catalogue.includes('titled(front, "Remove Account…", "Remove account…")'));
  const page = repo("website/src/pages/index.astro");
  assert.ok(
    page.includes(
      "On macOS, the <b>Account</b> menu lists what you can do to the account selected, with <b>Remove Account</b> on <a ",
    ),
  );
  assert.ok(page.includes('/reference/mac-app#keyboard-shortcuts">Command-Delete</a>'));
});

test("the menus say what the app says while a switch or a read runs, and of a site's one account", () => {
  const present = (file: string) => repo(`crates/pitboard-model/src/present/${file}`);
  assert.ok(present("accounts.rs").includes("return Some(words::SWITCHING_NOW.into());"));
  assert.ok(catalogue.includes(`pub(crate) const SWITCHING_NOW: &str = "${SWITCHING}";`));
  assert.ok(present("setup.rs").includes("return words::READING.into();"));
  assert.ok(catalogue.includes(`pub(crate) const READING: &str = "${READING}";`));
  assert.ok(catalogue.includes('format!("Updated {clock}")'));
  assert.equal(updated(clock.read), "Updated 20:50");
  assert.ok(
    repo("crates/pitboard-model/src/account_windows/accounts.rs").includes(
      "title: words::open_site_as(site.name(), &window.label),",
    ),
  );
  assert.ok(catalogue.includes('format!("Open {site} as {label}")'));
  assert.equal(openSiteAs("claude.ai", "work"), "Open claude.ai as work");
});
