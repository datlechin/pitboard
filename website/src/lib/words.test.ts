import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { headsUp, level, toolsOn } from "./words.ts";
import { account, clock, share, windowsClock } from "../data/world.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const provider = repo("crates/pitboard-core/src/provider/mod.rs");

test("each system lists the tools Pitboard supports on it, Claude Desktop on macOS alone", () => {
  assert.ok(provider.includes("(ProviderId::Claude | ProviderId::Codex, _) => true,"));
  assert.ok(provider.includes("(ProviderId::Desktop, Os::MacOs) => true,"));
  assert.ok(provider.includes("(ProviderId::Desktop, Os::Linux | Os::Windows) => false,"));
  const listed = (p: Parameters<typeof toolsOn>[0]) => toolsOn(p).map(([tool]) => tool);
  assert.deepEqual(listed("macos"), ["claude", "codex", "desktop"]);
  assert.deepEqual(listed("windows"), ["claude", "codex"]);
  assert.deepEqual(listed("linux"), ["claude", "codex"]);
});

test("a figure's level steps where the core's usage_level does, at 70% and 90%", () => {
  const core = repo("crates/pitboard-core/src/words.rs");
  assert.ok(core.includes("        90.. => UsageLevel::Out,\n        70.. => UsageLevel::Low,"));
  assert.deepEqual([0, 69, 70, 89, 90, 100].map(level), ["g", "g", "y", "y", "r", "r"]);
});

test("the notification comes when personal's weekly limit, at its pace, reaches the share", () => {
  const week = account("personal").limits.find((l) => l.kind === "week")!;
  const minutes = (hm: string) => {
    const [h, m] = /(\d+)h (\d+)m/.exec(hm)!.slice(1).map(Number);
    return h * 60 + m;
  };
  const at = (clock: string) => {
    const [h, m] = clock.slice(-5).split(":").map(Number);
    return h * 60 + m;
  };
  const left = ((share - week.used) / (100 - week.used)) * minutes(week.runsOut!);
  const warned = (at(clock.menuBar) + Math.round(left)) % (24 * 60);
  assert.equal(at(clock.warned), warned);
  assert.equal(at(windowsClock.warned.time), warned);
  assert.equal(headsUp.title, `personal has used ${share}% of its weekly limit`);
});
