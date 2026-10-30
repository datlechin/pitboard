import assert from "node:assert/strict";
import { test } from "node:test";
import {
  drawingsOf,
  frontOf,
  labelOf,
  onOf,
  platformOf,
  systems,
  systemsOf,
  wordsOf,
  type Desktop,
  type Platform,
  type System,
} from "./platform.ts";

test("Linux stands for both its desktops, in the order the site names systems", () => {
  assert.deepEqual(systemsOf(["linux"]), ["gnome", "kde"]);
  assert.deepEqual(systemsOf(["kde", "macos"]), ["macos", "kde"]);
  assert.deepEqual(systemsOf(["linux", "gnome", "windows"]), ["windows", "gnome", "kde"]);
  assert.equal(onOf(["linux", "macos"]), "macos gnome kde");
  for (const s of systems) assert.ok(systemsOf([platformOf(s)]).includes(s), s);
});

test("both desktops say the words of one front, GNOME's, as front.rs maps them", () => {
  assert.deepEqual(
    systems.map((s) => frontOf(s)),
    ["mac", "windows", "gnome", "gnome"],
  );
});

test("a page without JavaScript names the systems of each part above it", () => {
  const cases: [(Platform | Desktop)[], string][] = [
    [["macos"], "On macOS:"],
    [["windows"], "On Windows:"],
    [["linux"], "On Linux:"],
    [["gnome", "kde"], "On Linux:"],
    [["gnome"], "On GNOME:"],
    [["kde"], "On KDE Plasma:"],
    [["windows", "macos"], "On macOS and Windows:"],
    [["macos", "linux"], "On macOS and Linux:"],
    [["macos", "windows", "kde"], "On macOS, Windows and KDE Plasma:"],
  ];
  for (const [on, label] of cases) assert.equal(labelOf(on), label);
  assert.throws(() => labelOf([]));
});

// Every set of drawings a figure has, by its slots, with the data-on of each.
const sets: [(Platform | Desktop)[], System[], string[]][] = [
  [["macos"], ["macos"], ["macos"]],
  [
    ["macos", "windows"],
    ["macos", "windows"],
    ["macos", "windows"],
  ],
  [["macos", "windows", "linux"], systems, ["macos", "windows", "gnome kde"]],
  [["macos", "windows", "gnome", "kde"], systems, ["macos", "windows", "gnome", "kde"]],
];

test("each drawing shows to its own systems, a Linux one to both desktops", () => {
  for (const [drawn, on, shown] of sets) assert.deepEqual(drawingsOf(drawn, on).map(onOf), shown);
});

test("a figure draws each system its section shows to, every one unless it names some", () => {
  assert.throws(() => drawingsOf(["macos", "windows"]), /shown on GNOME has no drawing/);
  assert.throws(() => drawingsOf(["macos", "windows", "gnome"]), /shown on KDE Plasma/);
  assert.throws(() => drawingsOf(["macos"], ["macos", "windows"]), /shown on Windows/);
  assert.deepEqual(drawingsOf(["macos", "windows", "linux"]).map(onOf), sets[2][2]);
});

test("a figure draws no system its section does not show to, and none twice", () => {
  assert.throws(() => drawingsOf(["macos", "windows"], ["macos"]), /draws Windows, which/);
  assert.throws(() => drawingsOf(["macos", "linux", "gnome"], ["macos", "gnome", "kde"]), /twice/);
});

test("a figure has a macOS drawing, which a page without JavaScript shows", () => {
  assert.throws(() => drawingsOf(["windows"], ["windows"]), /no macOS drawing/);
});

const side = (s: string) => ({ caption: s, describe: s });
const described = (said: [System[], { caption: string }][]) =>
  said.map(([on, w]) => [onOf(on), w.caption]);

test("each drawing has words of its own, Linux's for both desktops or each desktop's", () => {
  assert.deepEqual(
    described(wordsOf({ macos: side("m"), windows: side("w") }, ["macos", "windows"])),
    [
      ["macos", "m"],
      ["windows", "w"],
    ],
  );
  assert.deepEqual(
    described(wordsOf({ macos: side("m"), windows: side("w"), linux: side("l") }, systems)),
    [
      ["macos", "m"],
      ["windows", "w"],
      ["gnome kde", "l"],
    ],
  );
  assert.deepEqual(
    described(
      wordsOf({ macos: side("m"), gnome: side("g"), kde: side("k") }, ["macos", "gnome", "kde"]),
    ),
    [
      ["macos", "m"],
      ["gnome", "g"],
      ["kde", "k"],
    ],
  );
});

test("a figure refuses words for a drawing it lacks, and a drawing without words", () => {
  assert.throws(
    () => wordsOf({ macos: side("m") }, ["macos", "windows"]),
    /Windows drawing has no/,
  );
  assert.throws(() => wordsOf({ macos: side("m"), linux: side("l") }, ["macos", "gnome"]), /KDE/);
  assert.throws(() => wordsOf({ macos: side("m"), windows: side("w") }, ["macos"]), /Windows/);
  assert.throws(() => wordsOf({ windows: side("w") }, ["windows"]), /no words for macOS/);
  assert.throws(
    () => wordsOf({ macos: side("m"), linux: side("l"), kde: side("k") }, systems),
    /for Linux and for one of its desktops/,
  );
});

test("a terminal's words are macOS's wherever a system has none of its own", () => {
  assert.deepEqual(described(wordsOf({ macos: side("m") })), [["macos windows gnome kde", "m"]]);
  assert.deepEqual(described(wordsOf({ macos: side("m"), linux: side("l") })), [
    ["macos windows", "m"],
    ["gnome kde", "l"],
  ]);
});
