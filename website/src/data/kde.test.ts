import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { konsoleTitle, panelClock } from "./kde.ts";
import { clock } from "./world.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");

test("the panel's clock says the time in 24 hours over en_GB's short date, as the page's locale gives Plasma", () => {
  assert.deepEqual(panelClock(clock.menuBar), { time: "20:51", date: "13/10/2026" });
  // docs/facts/linux-session.md: Plasma's Region writes LC_TIME, which the app follows too
  const session = repo("docs/facts/linux-session.md").replace(/\s+/g, " ");
  assert.ok(session.includes("so the app showed 24 hour times with Plasma's clock"));
});

test("Konsole's title is its tab's folder and program, with no dash after", () => {
  assert.equal(konsoleTitle("claude"), "~ : claude");
  assert.doesNotMatch(konsoleTitle("bash"), /[–—]/);
});
