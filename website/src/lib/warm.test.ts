import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";
import { model, nextWarmUp, reset, setWarmRange, turnWarmUp, warmNote } from "./model.ts";
import { activity } from "./words.ts";
import { warmHours } from "../data/limits.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const service = repo("crates/pitboard-core/src/service.rs");
const warm = repo("crates/pitboard-core/src/duty/warm/mod.rs");
// present/words/, a file for each surface that says the words, read as one
const words = readdirSync(
  new URL("../../../crates/pitboard-model/src/present/words/", import.meta.url),
)
  .sort()
  .map((file) => repo(`crates/pitboard-model/src/present/words/${file}`))
  .join("\n");
const last = () => model.get().log.at(-1)!;

test("turning an account's warm-up on or off is logged as the core's warm_turn logs it", () => {
  assert.ok(service.includes('let verb = if on { "warm-on" } else { "warm-off" };'));
  reset();
  turnWarmUp("work", true);
  assert.deepEqual([last().verb, last().subject, last().caller], ["warm-on", "work", "app"]);
  const work = model.get().accounts.find((a) => a.label === "work")!;
  // its period ends at 23:50, past the hours, so the next comes as they open
  assert.equal(
    warmNote(model.get(), work),
    "Has its 5-hour period running until 23:50, and warms up again at 07:00",
  );
  turnWarmUp("personal", false);
  assert.deepEqual([last().verb, last().subject], ["warm-off", "personal"]);
  assert.equal(nextWarmUp(model.get()), "Next warm-up: work at 07:00.");
  turnWarmUp("work", false);
  assert.equal(nextWarmUp(model.get()), null);
  reset();
});

test("the hours are a span Range::new takes: never empty, 00:00 at the end meaning 24:00", () => {
  assert.ok(warm.includes("let to = if to == 0 { DAY } else { to };"));
  assert.ok(warm.includes("(from < DAY && to <= DAY && from != to).then_some(Range { from, to })"));
  assert.ok(service.includes('self.change(permit, "warm-range", &subject, settled, |settled| {'));
  assert.ok(warm.includes('format!("{:02}:{:02}", minutes / 60, minutes % 60)'));
  reset();
  assert.equal(setWarmRange(6 * 60 + 30, 23 * 60), true);
  assert.deepEqual([model.get().warm.from, model.get().warm.to], [390, 1380]);
  assert.deepEqual([last().verb, last().subject], ["warm-range", "06:30-23:00"]);
  const logged = model.get().log.length;
  assert.equal(setWarmRange(23 * 60, 23 * 60), false, "a span that starts as it ends is refused");
  assert.equal(model.get().log.length, logged);
  assert.equal(setWarmRange(0, 0), true);
  assert.deepEqual([model.get().warm.from, model.get().warm.to], [0, 1440]);
  assert.equal(last().subject, "00:00-24:00");
  reset();
});

test("the hours are said as present/words/machine.rs says them, and the log's changes as the app does", () => {
  assert.ok(words.includes('return Some("All day".into());'));
  assert.ok(words.includes('"midnight".to_owned()'));
  assert.deepEqual(
    [warmHours(420, 1380), warmHours(420, 1440), warmHours(0, 1440), warmHours(1320, 360)],
    ["07:00 to 23:00", "07:00 to midnight", "All day", "22:00 to 06:00"],
  );
  for (const [verb, said] of [
    ["warm-on", "Warm-up turned on"],
    ["warm-off", "Warm-up turned off"],
    ["warm-range", "Warm-up hours changed"],
  ]) {
    assert.ok(words.includes(`"${verb}" => "${said}".into(),`), verb);
    const [row] = activity(
      [{ at: 0, caller: "app", verb, subject: "work", outcome: "ok" }],
      "macos",
    );
    assert.equal(row.change, said);
  }
});
