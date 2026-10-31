import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { points, regularM, sfSymbol } from "./sfsymbol.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const catalog = (name: string) =>
  repo(`apps/macos/App/Assets.xcassets/${name}.symbolset/${name}.svg`);
const template = readFileSync(new URL("../assets/logo/template.svg", import.meta.url), "utf8");

test("pitboard.symbolset's Regular-M is template.svg's mark, point for point, over its box", () => {
  const plain = catalog("pitboard");
  const drawn = sfSymbol(plain, plain, template);
  const [x0, y0, size] = drawn.viewBox.split(" ").map(Number);
  assert.equal(x0, 0);
  const unit = size / 16;
  const key = ([x, y]: [number, number]) => `${x.toFixed(2)},${y.toFixed(2)}`;
  const theirs = [...template.matchAll(/\sd="([^"]+)"/g)].map(
    ([, d]) => new Set(points(d).map(key)),
  );
  const ours = drawn.paths.map(
    ({ d }) =>
      new Set(
        points(d)
          .map(([x, y]): [number, number] => [(x - x0) / unit, (y - y0) / unit])
          .map(key),
      ),
  );
  // the board, then its dim row on its own layer
  assert.deepEqual(ours[1], theirs[1]);
  for (const point of theirs[0]) assert.ok(ours[0].has(point), point);
  assert.deepEqual(
    drawn.paths.map((p) => p.secondary),
    [false, true],
  );
});

test("the badged mark keeps the plain one's margins and its dim row", () => {
  const badged = catalog("pitboard.badge.exclamationmark");
  const plain = regularM(catalog("pitboard"));
  const drawn = regularM(badged);
  assert.equal(drawn.width, plain.width);
  assert.equal(drawn.paths.at(-1)!.d, plain.paths.at(-1)!.d);
  assert.ok(sfSymbol(badged, catalog("pitboard"), template).viewBox.startsWith("0 -101.77"));
});
