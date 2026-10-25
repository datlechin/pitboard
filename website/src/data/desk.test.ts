import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { menuCommands } from "./desk.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const menu = repo("apps/macos/Sources/PitboardApp/MenuBar/MenuBarContent.swift");

// From an item's title to the next item: the modifiers SwiftUI gives it.
function item(title: string): string {
  const start = menu.indexOf(`"${title}"`);
  assert.ok(start >= 0, `MenuBarContent.swift has ${title}`);
  const rest = menu.slice(start + title.length + 2);
  const end = rest.search(/Button[ ({]|Link\(|SiteMenuItems|\/\/ MARK/);
  return rest.slice(0, end < 0 ? undefined : end);
}

test("the macOS menu's commands have the keys MenuBarContent.swift gives them, and no others", () => {
  for (const c of menuCommands) {
    if (c === null || c.more) continue;
    const shortcut = /\.keyboardShortcut\("(.)"\)/.exec(item(c.title))?.[1];
    assert.equal(c.keys, shortcut && `⌘${shortcut.toUpperCase()}`, c.title);
  }
});

test("the questions' macOS shortcuts are AccountCommands.swift's: the Account menu, and Forget on Command-Delete", () => {
  const commands = repo("apps/macos/Sources/PitboardApp/Window/AccountCommands.swift");
  assert.ok(commands.includes('CommandMenu("Account")'));
  // .delete with SwiftUI's default modifier, Command, on the menu bar's Forget
  assert.match(
    commands,
    /if place == \.menuBar \{\s+Button\(forget\.title, role: \.destructive\) \{ command\.choose\(forget\) \}\s+\.keyboardShortcut\(\.delete\)\n/,
  );
  assert.ok(
    repo("crates/pitboard-ffi/src/present/accounts.rs").includes('const FORGET: &str = "Forget…";'),
  );
  const page = repo("website/src/pages/index.astro");
  assert.ok(
    page.includes(
      "On macOS, the <b>Account</b> menu lists what you can do to the account selected, with <b>Forget</b> on <a ",
    ),
  );
  assert.ok(page.includes('/reference/mac-app#keyboard-shortcuts">Command-Delete</a>'));
});
