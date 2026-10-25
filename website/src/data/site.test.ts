import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { TAGLINE } from "./site.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");

test("llms.txt, the docs and the README lead with the site's tagline", () => {
  assert.equal(/^> (.+)$/m.exec(repo("website/public/llms.txt"))?.[1], TAGLINE);
  assert.equal(JSON.parse(repo("docs/docs.json")).description, TAGLINE);
  assert.equal(/^description: "(.+)"$/m.exec(repo("docs/index.mdx"))?.[1], TAGLINE);
  assert.equal(/^# Pitboard\n\n(.+)$/m.exec(repo("README.md"))?.[1], TAGLINE);
});
