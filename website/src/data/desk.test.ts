import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { menuCommands } from "./desk.ts";

const menu = readFileSync(
  new URL("../../../apps/macos/Sources/PitboardApp/MenuBar/MenuBarContent.swift", import.meta.url),
  "utf8",
);

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
