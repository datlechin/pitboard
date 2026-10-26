import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";

const site = new URL("../", import.meta.url);
const facts = readFileSync(new URL("../../docs/facts/site.md", site), "utf8");
const departures = facts
  .split("## Where the site departs from the apps")[1]
  .split("\n## ")[0]
  .replace(/\s+/g, " ");

// every comment that says the code departs from the apps, and the line it names
function named(): [string, string][] {
  const found: [string, string][] = [];
  for (const file of readdirSync(site, { recursive: true, encoding: "utf8" })) {
    if (!/\.(ts|astro|css)$/.test(file) || file.endsWith(".test.ts")) continue;
    const text = readFileSync(new URL(file, site), "utf8");
    for (const m of text.matchAll(/departs from the apps: ([^\n*]+?)\s*(?:\*\/|\}|$)/gm))
      found.push([file, m[1].trim()]);
  }
  return found;
}

test("each departure the code names is one docs/facts/site.md lists", () => {
  const all = named();
  assert.ok(all.length > 0);
  for (const [file, line] of all) assert.ok(departures.includes(line), `${file}: ${line}`);
});
