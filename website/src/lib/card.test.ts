import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import sharp from "sharp";
import { TAGLINE } from "../data/site.ts";
import {
  GITHUB,
  OPEN_GRAPH,
  cards,
  darkColors,
  describe,
  layout,
  render,
  type Assets,
  type Card,
} from "./card.ts";

const file = (path: string) => readFileSync(new URL(`../${path}`, import.meta.url));
const font = (name: string) => file(`assets/fonts/${name}.ttf`);

const assets: Assets = {
  css: file("styles/global.css").toString(),
  lockup: file("assets/logo/lockup-color.svg").toString(),
  template: file("assets/logo/template.svg").toString(),
  fonts: [
    { name: "Inter", data: font("Inter-4.1-400"), weight: 400 },
    { name: "Inter", data: font("Inter-4.1-700"), weight: 700 },
    { name: "JetBrains Mono", data: font("JetBrainsMono-2.304-500"), weight: 500 },
    { name: "JetBrains Mono", data: font("JetBrainsMono-2.304-700"), weight: 700 },
  ],
};

test("takes the site's dark colours, whatever commas their light half holds", () => {
  const c = darkColors(assets.css);
  assert.equal(c["page"], "#090b0b");
  assert.equal(c["mac-menu-bg"], "rgb(36 36 36 / 0.78)");
  assert.equal(c["white"], "#ffffff");
  assert.deepEqual(darkColors("--color-x: light-dark(rgb(0 0 0 / 0.1), #fff);"), { x: "#fff" });
});

test("draws each card as an opaque 1200 by 630 PNG under 300 KB", async () => {
  for (const card of Object.values(cards) as Card[]) {
    const png = await render(card, assets);
    const meta = await sharp(png).metadata();
    assert.deepEqual(
      [meta.format, meta.width, meta.height],
      ["png", OPEN_GRAPH.width, OPEN_GRAPH.height],
    );
    assert.equal(meta.hasAlpha, false);
    assert.ok(png.length < 300_000, `${card.kind} is ${png.length} bytes`);
  }
});

test("draws the home card at 2:1 for GitHub's social preview", async () => {
  const meta = await sharp(await render(cards.home, assets, GITHUB)).metadata();
  assert.deepEqual([meta.width, meta.height], [1280, 640]);
  assert.equal(meta.hasAlpha, false);
});

test("describes the accounts the home card's menu shows", () => {
  const alt = describe({ kind: "home" });
  assert.match(alt, /menu bar item, work 40%\./);
  assert.match(
    alt,
    /Claude Code has work, in use, and personal; Codex has main, in use, and spare;/,
  );
  assert.ok(alt.includes(`Pitboard's logo above the line: ${TAGLINE} On the right,`), alt);
  assert.ok(alt.length <= 420, `${alt.length} characters`);
  assert.equal(
    describe(cards.download),
    "Pitboard's logo above the label Download and the line: Pitboard for macOS, Windows and Linux.",
  );
});

test("sets the tagline on the home card in three lines, the last on the menu's bottom edge", async () => {
  assert.equal(TAGLINE, "Switch AI coding tools between your accounts.");
  for (const size of [OPEN_GRAPH, GITHUB]) {
    const svg = await layout(cards.home, assets, { embedFont: false, size });
    const lines = new Set(
      [...svg.matchAll(/<text [^>]*y="([\d.]+)"[^>]*font-size="56"/g)].map((m) => m[1]),
    );
    assert.equal(lines.size, 3, `${size.width} by ${size.height}`);
  }
});
