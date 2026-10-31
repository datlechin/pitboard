import assert from "node:assert/strict";
import { test } from "node:test";
import { month, todayButton, topBarClock } from "./gnome.ts";
import { clock } from "./world.ts";

const at = (local: string) => Date.parse(`${local}+07:00`) / 1000;

test("the top bar's clock is the date, then the 24-hour time", () => {
  assert.deepEqual(topBarClock(clock.menuBar), { date: "13 Oct", time: "20:51" });
  assert.deepEqual(topBarClock(clock.warned), { date: "14 Oct", time: "05:53" });
  assert.deepEqual(topBarClock(at("2026-11-09T09:05:00")), { date: "9 Nov", time: "09:05" });
});

test("the date menu's today button gives the weekday over the date", () => {
  assert.deepEqual(todayButton(clock.warned), { day: "Wednesday", date: "14 October 2026" });
});

test("the calendar starts its six weeks on the Monday before the first of the month", () => {
  const october = month(clock.warned);
  assert.equal(october.label, "October");
  assert.deepEqual(
    october.headings.map((h) => h.letter),
    ["M", "T", "W", "T", "F", "S", "S"],
  );
  assert.equal(october.weeks.length, 6);
  const days = october.weeks.flat();
  assert.deepEqual(
    days.slice(0, 4).map((d) => [d.label, d.otherMonth]),
    [
      ["28", true],
      ["29", true],
      ["30", true],
      ["01", false],
    ],
  );
  assert.deepEqual(days.at(-1), { label: "08", today: false, weekend: true, otherMonth: true });
  assert.deepEqual(
    days.filter((d) => d.today).map((d) => d.label),
    ["14"],
  );
  assert.deepEqual(
    october.weeks[2].map((d) => d.weekend),
    [false, false, false, false, false, true, true],
  );
});

test("a month that starts on a Monday gets a week before it", () => {
  const june = month(at("2026-06-10T12:00:00"));
  assert.equal(june.weeks[0][0].label, "25");
  assert.equal(june.weeks[1][0].label, "01");
});
