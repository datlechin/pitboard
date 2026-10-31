import { strict as assert } from "node:assert";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { breeze } from "./breeze.ts";

// the icons Plasma's surfaces draw, kept as breeze-icons files them by context and size
const root = new URL("../assets/icons/breeze/", import.meta.url).pathname;
const files = (readdirSync(root, { recursive: true }) as string[]).filter((f) =>
  /^[a-z]+\/\d+\/[\w.-]+\.svg$/.test(f),
);

test("every Breeze icon Plasma draws comes inline with no style, class or editor attribute", () => {
  assert.ok(files.length > 0);
  for (const name of files) {
    const { viewBox, body } = breeze(readFileSync(join(root, name), "utf8"), "k");
    assert.match(viewBox, /^0 0 \d+(\.\d+)? \d+(\.\d+)?$/, name);
    assert.doesNotMatch(body, /\sstyle=|<style|\sclass=/, name);
    assert.doesNotMatch(body, /\s[\w-]+:[\w-]+=|<\/?[\w-]+:/, name);
    assert.doesNotMatch(body, /ColorScheme/, name);
  }
});

test("an icon's ids are its drawing's own, and every reference finds one", () => {
  for (const name of files) {
    const { body } = breeze(readFileSync(join(root, name), "utf8"), "hero-task");
    const ids = [...body.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]);
    for (const id of ids) assert.ok(id.startsWith("hero-task-"), `${name}: ${id}`);
    for (const m of body.matchAll(/url\(#([^)]+)\)|href="#([^"]+)"/g))
      assert.ok(ids.includes(m[1] ?? m[2]), `${name}: ${m[0]}`);
  }
});

test("ColorScheme-Text draws in currentColor, as KIconLoader gives it the text's colour, and another class its own", () => {
  const svg = `<svg viewBox="0 0 16 16"><style>.ColorScheme-Text{color:#232629}.ColorScheme-NegativeText{color:#da4453}</style><path class="ColorScheme-Text" style="fill:currentColor;fill-opacity:1;-inkscape-stroke:none" d="M0 0h1"/><path class="ColorScheme-NegativeText" fill="currentColor" d="M1 1h1"/></svg>`;
  assert.equal(
    breeze(svg, "k").body,
    '<path d="M0 0h1" fill="currentColor" fill-opacity="1"/><path fill="currentColor" d="M1 1h1" color="#da4453"/>',
  );
});

test("Pitboard's status icons draw as Plasma draws a Breeze icon", () => {
  const status = new URL("../../../apps/linux/data/icons/symbolic/status/", import.meta.url);
  for (const name of readdirSync(status)) {
    const { viewBox, body } = breeze(readFileSync(new URL(name, status), "utf8"), "k");
    assert.equal(viewBox, "0 0 16 16", name);
    assert.match(body, /fill="currentColor"/, name);
    assert.doesNotMatch(body, /\scolor=|\sclass=/, name);
  }
});
