import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";
import { appMenus, NO_ACCOUNT } from "./appmenu.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const mac = (file: string) => repo(`apps/macos/Sources/PitboardApp/${file}`);
const present = (file: string) => repo(`crates/pitboard-model/src/present/${file}`);
// present/words/, a file for each surface that says the words, read as one
const catalogue = readdirSync(
  new URL("../../../crates/pitboard-model/src/present/words/", import.meta.url),
)
  .sort()
  .map((file) => present(`words/${file}`))
  .join("\n");
// app_words.rs with its string literals' line continuations joined and its code on one line
const appWords = present("app_words.rs")
  .replace(/\\\n\s*/g, "")
  .replace(/\s+/g, " ")
  .replace(/\( /g, "(");
const facts = repo("docs/facts/site.md");
// the facts' section on the menu bar, its lines joined
const bar = facts
  .split("### macOS: the menu bar with the Accounts window in front")[1]
  .split("### ")[0]
  .replace(/\n\s*/g, " ");

// the items a menu's bullet says "is dimmed" or "are dimmed" of, in a sentence of their own
function dimmedIn(read: string): string[] {
  return [...read.matchAll(/(?<=\. )([^.]+?) (?:is|are) dimmed\./g)].flatMap((m) =>
    m[1].split(/, | and /),
  );
}

test("the menu bar lists the menus the app's UI tests read, in their order", () => {
  const titles = appMenus(true).map((m) => m.title);
  assert.ok(
    bar.includes(`The bar: the Apple menu, ${titles.join(", ").replace(/, Help$/, ", Help.")}`),
  );
});

test("each item is one the UI tests read, with its key, and dimmed where they read it so", () => {
  for (const menu of appMenus(false)) {
    if (menu.id === "account") continue;
    // the menu's bullet, up to the next
    const read = bar.split(`- ${menu.title}`)[1].split(" - ")[0];
    for (const item of menu.items) {
      // a site's item is each site with accounts, as the next test ties
      if (!item || item.does === "claude.ai" || item.does === "chatgpt.com") continue;
      assert.ok(read.includes(item.title), `${menu.title}: ${item.title}`);
      if (item.keys) {
        const [mods, key] = item.keys;
        assert.ok(
          read.includes(`${item.title} ${mods}${mods.endsWith("fn") ? " " : ""}${key}`),
          item.title,
        );
      }
    }
    // the state dims Edit's Copy and Delete; the others stay as read, each way
    const drawn = menu.items.filter((i) => i?.dimmed && !i.does).map((i) => i!.title);
    assert.deepEqual(drawn, dimmedIn(read), menu.title);
  }
});

test("the app's own items say what PitboardScenes.swift and its commands give them", () => {
  const scenes = mac("App/PitboardScenes.swift");
  for (const [word, said] of [
    ["words.menu.about", 'about: titled(front, "About Pitboard", "About Pitboard")'],
    // Sparkle's, from the macOS app's own catalogue
    ["macWords.checkForUpdates", 'check_for_updates: "Check for Updates…".into()'],
    ["words.menu.addAccount", "add_account: words::add_account(front, true)"],
    [
      "words.menu.help",
      'help: match front { Front::MacMenuBar | Front::WindowsNotificationArea => { titled(front, "Pitboard Help", "Pitboard help")',
    ],
  ]) {
    assert.ok(scenes.includes(`(${word}`), word);
    assert.ok(appWords.includes(said), said);
  }
  assert.ok(catalogue.includes('titled(front, "Add Account…", "Add account…")'));
  assert.ok(scenes.includes('.keyboardShortcut("n")'));
  const windows = mac("AccountWindows/AccountWindowCommands.swift");
  for (const [word, title] of [
    ["removeWebsiteData", "Remove Website Data…"],
    ["actualSize", "Actual Size"],
    ["zoomIn", "Zoom In"],
    ["zoomOut", "Zoom Out"],
  ]) {
    assert.ok(windows.includes(`(words.accountWindow.${word})`), word);
    assert.ok(appWords.includes(`titled(front, "${title}", `), title);
  }
  for (const [word, title] of [
    ["back", "Back"],
    ["forward", "Forward"],
  ]) {
    assert.ok(windows.includes(`(words.accountWindow.${word})`), word);
    assert.ok(appWords.includes(`${word}: "${title}".into(),`), title);
  }
  assert.ok(mac("App/RefreshCommand.swift").includes("?? words.menu.refresh"));
  assert.ok(appWords.includes("refresh: words::REFRESH.into(),"));
  assert.ok(catalogue.includes('pub(crate) const REFRESH: &str = "Refresh";'));
  // File lists a site's item for each site, the fixture's being claude.ai alone
  assert.ok(
    windows.includes(
      "CommandGroup(after: .newItem) {\n            Divider()\n            SiteMenuItems(windows: windows)",
    ),
  );
  assert.ok(bar.includes("File: Add Account… ⌘N; Open claude.ai ›;"));
  assert.ok(!appMenus(false).some((m) => m.items.some((i) => i?.title === "Check for Updates…")));
});

test("the Account menu with nothing selected offers what the model says, each dimmed", () => {
  const accounts = repo("crates/pitboard-model/src/present/accounts.rs");
  assert.ok(
    accounts.includes(
      "offers: vec![\n            words::switch_account(None),\n            words::sign_in_again(front, true),\n            words::rename(true).into(),\n        ],\n        forget: words::remove_account(front),",
    ),
  );
  assert.ok(catalogue.includes(`None => SWITCH.into(),`));
  assert.ok(catalogue.includes(`pub(crate) const SWITCH: &str = "${NO_ACCOUNT[0]}";`));
  assert.ok(catalogue.includes(`if asks { "${NO_ACCOUNT[2]}" } else { "Rename" }`));
  assert.ok(
    bar.includes(
      "Account, with no account selected: Switch, Sign In Again…, Rename…, Remove Account… ⌘⌫, all dimmed.",
    ),
  );
  assert.ok(mac("Window/AccountCommands.swift").includes(".keyboardShortcut(.delete)"));
});
