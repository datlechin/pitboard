import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";
import { plainText } from "../lib/changelog.ts";
import { commands, linuxWays, windowsScoop } from "./commands.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const pages = ["install", "install/script"].map((page) => repo(`docs/${page}.mdx`)).join("\n");

test("each install command is one the install pages give", () => {
  for (const [name, command] of Object.entries(commands)) {
    assert.ok(pages.includes(command.copy), `${name}: ${command.copy}`);
  }
});

test("llms.txt installs with the install page's script", () => {
  const llms = repo("website/public/llms.txt");
  const shown = /```sh\n(.+)\n```/.exec(llms)?.[1];
  assert.equal(shown, commands.installScript.copy);
});

// GitHub's release of each kind, as the site reads it
const beta = { version: "1.0.0-beta.1", prerelease: true };
const full = { version: "1.0.0", prerelease: false };

test("offers Scoop with the bucket's own lines, from the first full release it holds", () => {
  const bucket = /```powershell\n([^`]+)```/.exec(repo("packaging/scoop/README.md"))?.[1];
  assert.ok(bucket, "the bucket's README gives its install lines");
  assert.equal(windowsScoop(beta), null);
  assert.deepEqual(
    windowsScoop(full)?.map((c) => c.copy),
    bucket.trim().split("\n"),
  );
});

// A channel's setup is the repository page's, which sets up the beta channel and moves to stable.
const repository = repo("docs/install/linux-repository.mdx");
const lines = new Set(repository.split("\n").map((l) => l.trim()));
const everyWay = (release: { prerelease: boolean }) => Object.values(linuxWays(release)).flat();

test("sets up the beta channel until a full release has the Linux app, and stable from then", () => {
  for (const [release, setup] of [
    [beta, "pitboard-beta"],
    [full, "pitboard"],
  ] as const) {
    const ways = linuxWays(release);
    for (const command of everyWay(release))
      assert.ok(lines.has(command.copy), `${setup}: ${command.copy}`);
    assert.ok(ways.apt.some((c) => c.copy.includes(`/etc/apt/sources.list.d/${setup}.sources `)));
    for (const command of [ways.dnf[0], ways.zypper[0]])
      assert.ok(command.copy.endsWith(`/${setup}.repo`));
  }
});

test("ends each distribution's setup by installing the app", () => {
  for (const commands of Object.values(linuxWays(beta)))
    assert.match(commands.at(-1)!.copy, / install pitboard-app$/);
  const said = "For the command line alone, install `pitboard`.";
  assert.equal(repository.split(said).length - 1, 3, "for apt, dnf and zypper");
  assert.ok(repo("docs/install.mdx").includes(said));
});

// The offline build draws the beta channel alone, so CommandBlock never checks stable's.
test("shows each Linux command as it copies it", () => {
  for (const release of [beta, full]) {
    for (const command of everyWay(release)) assert.equal(plainText(command.html), command.copy);
  }
});

test("names each setup file the repository serves", () => {
  const served = readdirSync(new URL("../../../packaging/linux/", import.meta.url)).filter((name) =>
    /^pitboard.*\.(sources|repo|pref)$/.test(name),
  );
  const named = [beta, full].flatMap((release) =>
    everyWay(release).flatMap((c) => c.copy.match(/[\w-]+\.(sources|repo|pref)\b/g) ?? []),
  );
  assert.deepEqual([...new Set(named)].sort(), served.sort());
});
