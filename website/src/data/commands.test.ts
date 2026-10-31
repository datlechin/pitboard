import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";
import { plainText } from "../lib/changelog.ts";
import {
  attestCommand,
  commands,
  LINUX_KEY,
  linuxWays,
  windowsWays,
  type Command,
} from "./commands.ts";

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
const lines = new Set(repository.split("\n").map((l) => l.trim()));
const everyWay = (release: { prerelease: boolean }) => {
  const { setup, keyCheck, app, cli } = linuxWays(release);
  return [setup, app, cli].flatMap((w) => [...w.apt, ...w.dnf, ...w.zypper]).concat(keyCheck);
};

test("sets up the beta channel until a full release has the Linux app, and stable from then", () => {
  for (const [release, setup] of [
    [beta, "pitboard-beta"],
    [full, "pitboard"],
  ] as const) {
    const ways = linuxWays(release);
    for (const command of [...Object.values(ways.setup).flat(), ways.keyCheck])
      assert.ok(lines.has(command.copy), `${setup}: ${command.copy}`);
    for (const command of Object.values(ways.app).flat())
      assert.ok(lines.has(command.copy), command.copy);
    assert.ok(
      ways.setup.apt.some((c) => c.copy.includes(`/etc/apt/sources.list.d/${setup}.sources `)),
    );
    for (const command of [ways.setup.dnf[0], ways.setup.zypper[0]])
      assert.ok(command.copy.endsWith(`/${setup}.repo`));
  }
});

test("checks the key apt is given against the fingerprint the repository page shows under pub", () => {
  assert.equal(/^\s*pub .+\n\s+([0-9A-F]{40})$/m.exec(repository)?.[1], LINUX_KEY);
  assert.ok(repository.includes(`whose fingerprint is \`${LINUX_KEY}\``));
  // the key the first download saves
  const { setup, keyCheck } = linuxWays(beta);
  const keyring = keyCheck.copy.replace(/^gpg --show-keys /, "");
  assert.ok(setup.apt[0].copy.startsWith(`sudo curl -fsSL -o ${keyring} `));
});

test("installs the command line alone as the pitboard package, as the app installs", () => {
  const { app, cli } = linuxWays(beta);
  const alone = (commands: Command[]) =>
    commands.map((c) => c.copy.replace(/ pitboard-app$/, " pitboard"));
  assert.deepEqual(
    Object.values(cli).map((commands) => commands.map((c) => c.copy)),
    Object.values(app).map(alone),
  );
  const said = "For the command line alone, install `pitboard`.";
  assert.equal(repository.split(said).length - 1, 3, "for apt, dnf and zypper");
  assert.ok(repo("docs/install.mdx").includes(said));
});

// The offline build draws the beta channel alone, so CommandBox never checks stable's.
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
