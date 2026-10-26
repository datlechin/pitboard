import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  atLabel,
  choices,
  heading,
  section,
  shareLabel,
  shareRange,
  terms,
  timeOfDay,
  warm,
} from "./limits.ts";
import { HIGHEST, LOWEST } from "../lib/model.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const machine = repo("crates/pitboard-ffi/src/present/machine.rs").replace(/\\\n\s*/g, "");

test("the Limits settings say what machine.rs gives LimitsSettings.swift and SettingsPage.xaml", () => {
  assert.ok(machine.includes(`title: "${section}".into(),`));
  assert.ok(machine.includes(`const AT_LIMIT: &str = "${heading}";`));
  assert.ok(machine.includes('at_label: format!("{AT_LIMIT} {at}%"),'));
  assert.equal(atLabel(90), "When a limit reaches 90%");
  assert.ok(machine.includes(`share_label: "${shareLabel}".into(),`));
  assert.ok(
    repo("crates/pitboard-ffi/src/present/words.rs").includes(
      'format!("From {lowest}% to {highest}%")',
    ),
  );
  assert.equal(shareRange, `From ${LOWEST}% to ${HIGHEST}%`);
});

test("each choice is titled on macOS and in sentence case on Windows, in machine.rs's order", () => {
  const listed = [...machine.matchAll(/\(\s*AtLimit::(\w+),\s*"([^"]+)",\s*"([^"]+)",?\s*\)/g)].map(
    ([, value, mac, win]) => [value.toLowerCase(), mac, win],
  );
  assert.deepEqual(
    listed,
    choices.map((c) => [c.value, c.mac, c.win]),
  );
  assert.ok(
    repo("crates/pitboard-ffi/src/present/words.rs").includes(
      "Os::Windows => sentence_case,\n        Os::MacOs | Os::Linux => title_case,",
    ),
  );
});

test("the terms are the core's sentence, curled, and its link", () => {
  const core = repo("crates/pitboard-core/src/words.rs");
  assert.ok(core.includes(`"${terms.sentence.slice(0, -1).replace("’", "'")}";`));
  assert.ok(core.includes(`pub const IS_THIS_ALLOWED_URL: &str =\n    "${terms.href}";`));
  assert.ok(machine.includes(`link_title: "${terms.link}".into(),`));
});

test("Warm up limits is titled as machine.rs's `warm` titles it, its times as `clock` says them", () => {
  assert.ok(machine.includes(`title: "${warm.title}".into(),`));
  assert.ok(
    repo("crates/pitboard-core/src/duty/warm/mod.rs").includes(
      'format!("{:02}:{:02}", minutes / 60, minutes % 60)',
    ),
  );
  assert.deepEqual([timeOfDay(420), timeOfDay(1380), timeOfDay(1440)], ["07:00", "23:00", "00:00"]);
});
