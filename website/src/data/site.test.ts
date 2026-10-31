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

test("llms.txt, the install tabs, /download, the README and the install docs name the Linux app's systems in one wording", () => {
  const needs =
    "Debian 13, Ubuntu 26.04, Fedora 43 and later, or openSUSE Tumbleweed, with GNOME or KDE Plasma";
  for (const path of [
    "website/public/llms.txt",
    "website/src/components/InstallTabs.astro",
    "website/src/pages/download.astro",
    "README.md",
    "docs/install.mdx",
    "docs/install/linux-repository.mdx",
  ])
    assert.ok(repo(path).replace(/\s+/g, " ").includes(needs), path);
});
