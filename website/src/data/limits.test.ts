import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";
import {
  atLabel,
  choices,
  heading,
  orderLabel,
  orders,
  section,
  shareLabel,
  shareRange,
  terms,
  timeOfDay,
  warm,
  weeklyLabel,
  weeklyToggle,
  weeklyNote,
} from "./limits.ts";
import { HIGHEST, LOWEST } from "../lib/model.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const machine = repo("crates/pitboard-model/src/present/machine.rs").replace(/\\\n\s*/g, "");
// present/words/, a file for each surface that says the words, read as one
const words = readdirSync(
  new URL("../../../crates/pitboard-model/src/present/words/", import.meta.url),
)
  .sort()
  .map((file) => repo(`crates/pitboard-model/src/present/words/${file}`))
  .join("\n")
  .replace(/\\\n\s*/g, "");

test("the Limits settings say what machine.rs gives LimitsSettings.swift and SettingsPage.xaml", () => {
  assert.ok(machine.includes("title: words::BEFORE_RUNNING_OUT.into(),"));
  assert.ok(words.includes(`pub(crate) const BEFORE_RUNNING_OUT: &str = "${section}";`));
  assert.ok(machine.includes("heading: words::AT_LIMIT.into(),"));
  assert.ok(words.includes(`pub(crate) const AT_LIMIT: &str = "${heading}";`));
  assert.ok(machine.includes("at_label: words::at_limit_label(at),"));
  assert.ok(words.includes('format!("{AT} {at}%")'));
  assert.equal(atLabel(90), "5-hour limit 90%");
  assert.ok(machine.includes("share_label: words::AT.into(),"));
  assert.ok(words.includes(`pub(crate) const AT: &str = "${shareLabel}";`));
  assert.ok(words.includes('format!("From {lowest}% to {highest}%")'));
  assert.equal(shareRange, `From ${LOWEST}% to ${HIGHEST}%`);
});

test("each choice is titled on macOS and in sentence case on Windows, in at_limit_choice's order", () => {
  const listed = [
    ...words.matchAll(/AtLimit::(\w+) => titled\(front, "([^"]+)", "([^"]+)"\),/g),
  ].map(([, value, mac, win]) => [value.toLowerCase(), mac, win]);
  assert.deepEqual(
    listed,
    choices.map((c) => [c.value, c.mac, c.win]),
  );
  assert.ok(machine.includes("choices: [AtLimit::Nothing, AtLimit::Notify, AtLimit::Switch]"));
  assert.ok(
    words.includes(
      "Front::WindowsNotificationArea => sentence_case,\n        Front::MacMenuBar | Front::Gnome => title_case,",
    ),
  );
});

test("Use first offers each order titled on macOS and in sentence case on Windows, under At", () => {
  assert.ok(machine.includes("order_label: words::USE_FIRST.into(),"));
  assert.ok(words.includes(`pub(crate) const USE_FIRST: &str = "${orderLabel}";`));
  const listed = [
    ...words.matchAll(/SwitchOrder::(\w+) => titled\(front, "([^"]+)", "([^"]+)"\),/g),
  ].map(([, , mac, win]) => [mac, win]);
  assert.deepEqual(
    listed,
    orders.map((o) => [o.mac, o.win]),
  );
  assert.ok(machine.includes("orders: [SwitchOrder::MostLeft, SwitchOrder::ResetsSoonest]"));
  // each by the core's code, which the command line's --order takes
  const order = repo("crates/pitboard-core/src/usage/order.rs");
  for (const o of orders) {
    const name = o.mac.replace(" ", "");
    assert.ok(order.includes(`Order::${name} => "${o.value}",`), o.value);
  }
  // and Most Left the default
  assert.ok(order.includes("#[default]\n    MostLeft,"));
  // shown, and enabled, while something happens at the share
  assert.ok(
    machine.includes(
      "share_enabled: state.preferences_have_been_read() && choice != AtLimit::Nothing,",
    ),
  );
});

test("the terms are the core's sentence, curled, and its link", () => {
  const core = repo("crates/pitboard-core/src/words.rs");
  assert.ok(core.includes(`"${terms.sentence.slice(0, -1).replace("’", "'")}";`));
  assert.ok(core.includes(`pub const IS_THIS_ALLOWED_URL: &str =\n    "${terms.href}";`));
  assert.ok(machine.includes("link_title: words::IS_THIS_ALLOWED.into(),"));
  assert.ok(words.includes(`pub(crate) const IS_THIS_ALLOWED: &str = "${terms.link}";`));
});

test("Warm-Up is titled as machine.rs's `warm` titles it, its times as `clock` says them", () => {
  assert.ok(machine.includes("title: words::warm_up(seen.front),"));
  assert.ok(words.includes(`titled(front, "${warm.title.macos}", "${warm.title.windows}")`));
  assert.ok(
    repo("crates/pitboard-core/src/duty/warm/mod.rs").includes(
      'format!("{:02}:{:02}", minutes / 60, minutes % 60)',
    ),
  );
  assert.deepEqual([timeOfDay(420), timeOfDay(1380), timeOfDay(1440)], ["07:00", "23:00", "00:00"]);
});

test("weekly controls share the native model's labels and explain off without hiding exhaustion", () => {
  assert.ok(words.includes(`pub(crate) const WEEKLY_LIMITS: &str = "${weeklyLabel}";`));
  assert.ok(words.includes(`titled(front, "${weeklyToggle.mac}", "${weeklyToggle.win}")`));
  for (const on of [false, true]) assert.ok(words.includes(weeklyNote(on, "most-left")));
  assert.ok(
    weeklyNote(false, "resets-soonest").includes("Resets Soonest can still choose an account"),
  );
});
