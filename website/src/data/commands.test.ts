import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";
import { plainText } from "../lib/changelog.ts";
import { attestCommand, commands, linuxWays, windowsWays } from "./commands.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const pages = [
  "install",
  "install/script",
  "install/linux-repository",
  "install/other-ways",
  "install/verify",
]
  .map((page) => repo(`docs/${page}.mdx`))
  .join("\n");

test("each install command is one the install pages give", () => {
  for (const [name, command] of Object.entries(commands)) {
    assert.ok(pages.includes(command.copy), `${name}: ${command.copy}`);
  }
});

test("an attestation check is the one Verify a download gives", () => {
  const shown = [...pages.matchAll(/^(gh attestation verify (\S+) --repo \S+)/gm)];
  assert.ok(shown.length > 0, "Verify a download checks an attestation");
  for (const [, line, file] of shown) assert.equal(attestCommand(file).copy, line);
});

test("llms.txt installs with the install page's script", () => {
  const llms = repo("website/public/llms.txt");
  const shown = /```sh\n(.+)\n```/.exec(llms)?.[1];
  assert.equal(shown, commands.installScript.copy);
});

test("compares Windows checksums with Verify a download's PowerShell line", () => {
  const shown = /^Select-String .+$/m.exec(pages)?.[0];
  assert.ok(shown, "Verify a download compares checksums in PowerShell");
  assert.equal(commands.selectString?.copy, shown);
});

// GitHub's release of each kind, as the site reads it
const beta = { version: "1.0.0-beta.1", prerelease: true };
const full = { version: "1.0.0", prerelease: false };

test("names the version Windows downloads to Cargo, as the release checks it, until a full release", () => {
  const checked = /'cargo-binstall\.exe'\) "(pitboard@)\$env:VERSION"/.exec(
    repo(".github/workflows/release.yml"),
  );
  assert.ok(checked, "release.yml installs the release with cargo-binstall");
  const { binstall, cargoInstall } = windowsWays(beta);
  assert.equal(binstall.copy, `cargo binstall ${checked[1]}1.0.0-beta.1`);
  assert.equal(cargoInstall.copy, `cargo install --locked ${checked[1]}1.0.0-beta.1`);
  assert.equal(windowsWays(full).binstall.copy, commands.binstall.copy);
  assert.equal(windowsWays(full).cargoInstall.copy, commands.cargoInstall.copy);
});

test("offers Scoop with the bucket's own lines, from the first full release it holds", () => {
  const bucket = /```powershell\n([^`]+)```/.exec(repo("packaging/scoop/README.md"))?.[1];
  assert.ok(bucket, "the bucket's README gives its install lines");
  assert.equal(windowsWays(beta).scoop, null);
  assert.deepEqual(
    windowsWays(full).scoop?.map((c) => c.copy),
    bucket.trim().split("\n"),
  );
});

// A channel's setup is the repository page's, which sets up the beta channel and moves to stable.
const repository = repo("docs/install/linux-repository.mdx");

test("sets up the beta channel until a full release has the Linux app, and stable from then", () => {
  for (const [release, setup] of [
    [beta, "pitboard-beta"],
    [full, "pitboard"],
  ] as const) {
    const { apt, dnf, zypper } = linuxWays(release);
    for (const command of [...apt, ...dnf, ...zypper]) {
      assert.ok(repository.includes(command.copy), `${setup}: ${command.copy}`);
    }
    assert.ok(apt.some((c) => c.copy.includes(`/etc/apt/sources.list.d/${setup}.sources `)));
    for (const command of [dnf[0], zypper[0]]) assert.ok(command.copy.endsWith(`/${setup}.repo`));
  }
});

// The offline build draws no release with the Linux app, so CommandBox never checks these there.
test("shows each Linux command as it copies it", () => {
  for (const release of [beta, full]) {
    const { apt, dnf, zypper } = linuxWays(release);
    for (const command of [...apt, ...dnf, ...zypper])
      assert.equal(plainText(command.html), command.copy);
  }
});

test("names each setup file the repository serves", () => {
  const served = readdirSync(new URL("../../../packaging/linux/", import.meta.url)).filter((name) =>
    /^pitboard.*\.(sources|repo|pref)$/.test(name),
  );
  const named = [beta, full].flatMap((release) => {
    const { apt, dnf, zypper } = linuxWays(release);
    return [...apt, ...dnf, ...zypper].flatMap(
      (c) => c.copy.match(/[\w-]+\.(sources|repo|pref)\b/g) ?? [],
    );
  });
  assert.deepEqual([...new Set(named)].sort(), served.sort());
});
