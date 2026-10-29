import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  ahead,
  fileRows,
  formatSize,
  linuxApp,
  linuxRelease,
  parseSums,
  readPublished,
  readRelease,
  readStars,
  windowsMsi,
  windowsRelease,
  windowsSigned,
  windowsZip,
  type Release,
} from "./github.ts";

const fixture = (name: string) =>
  readFileSync(new URL(`../data/fixtures/${name}`, import.meta.url), "utf8");
const recorded = JSON.parse(fixture("release-latest.json"));
const sums = fixture("SHA256SUMS");
const recordedBeta = JSON.parse(fixture("release-v1.0.0-beta.1.json"));
const betaSums = fixture("SHA256SUMS-v1.0.0-beta.1");

const API = "https://api.github.com/repos/datlechin/pitboard";
const WEB = "https://github.com/datlechin/pitboard";
const SUMS_URL = `${WEB}/releases/download/v0.15.1/SHA256SUMS`;
const BETA_SUMS_URL = `${WEB}/releases/download/v1.0.0-beta.1/SHA256SUMS`;

type Route = (init: RequestInit) => Response | Promise<Response>;

// A fetch that answers only the URLs a test names and records what was asked.
function server(routes: Record<string, Route>) {
  const asked: { url: string; method: string; auth: string | null }[] = [];
  const f = (async (input: string | URL | Request, init: RequestInit = {}) => {
    const url = String(input);
    const headers = new Headers(init.headers);
    asked.push({ url, method: init.method ?? "GET", auth: headers.get("authorization") });
    const route = routes[`${init.method ?? "GET"} ${url}`];
    if (!route) throw new TypeError(`fetch failed: ${url}`);
    return route(init);
  }) as typeof fetch;
  return { f, asked };
}

const json = (body: unknown, status = 200, headers: Record<string, string> = {}) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", ...headers },
  });

const latest = () => readRelease({ fetch: server({}).f, fixture: { release: recorded, sums } });
const beta = async () => {
  const published = await readPublished(
    {
      fetch: server({}).f,
      fixture: new Map([["1.0.0-beta.1", { release: recordedBeta, sums: betaSums }]]),
    },
    ["1.0.0-beta.1"],
  );
  return published.get("1.0.0-beta.1")!;
};

// A release with only the files a test names.
const made = (version: string, names: string[]): Release => ({
  version,
  tag: `v${version}`,
  prerelease: version.includes("-"),
  assets: names.map((name) => ({ name, size: 1, sha256: null, url: "" })),
  source: "fixture",
});
const installers = (version: string) =>
  made(version, [
    `Pitboard-v${version}-windows-x64.msi`,
    `Pitboard-v${version}-windows-arm64.msi`,
    "SHA256SUMS",
  ]);
const linuxPackages = (
  version: string,
  machines = ["amd64.deb", "arm64.deb", "x86_64.rpm", "aarch64.rpm"],
) =>
  made(version, [
    ...machines.flatMap((m) => [`pitboard-app-v${version}-${m}`, `pitboard-v${version}-${m}`]),
    "SHA256SUMS",
  ]);

test("reads the latest release from the API, with digests from SHA256SUMS", async () => {
  const { f, asked } = server({
    [`GET ${API}/releases/latest`]: () => json(recorded, 200, { "x-ratelimit-remaining": "4999" }),
    [`GET ${SUMS_URL}`]: () => new Response(sums),
  });
  const lines: string[] = [];
  const r = await readRelease({ fetch: f, token: "t0ken", log: (l) => lines.push(l) });
  assert.equal(r.version, "0.15.1");
  assert.equal(r.source, "api");
  assert.equal(r.assets.length, 13);
  const zip = r.assets.find((a) => a.name === "Pitboard-v0.15.1-macos.zip");
  assert.deepEqual(zip, {
    name: "Pitboard-v0.15.1-macos.zip",
    size: 18760928,
    sha256: "e3e3a396598200b57d56d5489ef33b390fcb490a330be290a26eedcf614c5a09",
    url: "https://github.com/datlechin/pitboard/releases/download/v0.15.1/Pitboard-v0.15.1-macos.zip",
  });
  assert.equal(asked[0].auth, "Bearer t0ken");
  // the token goes to the API only, never to a download
  assert.equal(asked[1].auth, null);
  assert.ok(lines.includes("release: token set: yes"));
  assert.ok(!lines.some((l) => l.includes("t0ken")));
});

test("stops when SHA256SUMS and GitHub's digest disagree", async () => {
  const tampered = sums.replace(/^e3e3a3965982/m, "ffffffffffff");
  const { f } = server({
    [`GET ${API}/releases/latest`]: () => json(recorded),
    [`GET ${SUMS_URL}`]: () => new Response(tampered),
  });
  await assert.rejects(readRelease({ fetch: f }), /disagree on Pitboard-v0\.15\.1-macos\.zip/);
});

test("stops on a 401 when a token is set, rather than build without it", async () => {
  const { f } = server({
    [`GET ${API}/releases/latest`]: () => json({ message: "Bad credentials" }, 401),
  });
  await assert.rejects(readRelease({ fetch: f, token: "old" }), /refused the token/);
});

test("falls back to the release pages on a rate limit, and keeps no signed URL", async () => {
  const signed =
    "https://release-assets.githubusercontent.com/github-production-release-asset/1?sig=secret";
  const routes: Record<string, Route> = {
    [`GET ${API}/releases/latest`]: () => json({ message: "API rate limit exceeded" }, 403),
    [`HEAD ${WEB}/releases/latest`]: () =>
      new Response(null, { status: 302, headers: { location: `${WEB}/releases/tag/v0.15.1` } }),
    [`GET ${SUMS_URL}`]: () => new Response(sums),
  };
  for (const a of recorded.assets as { name: string; size: number }[]) {
    // fetch follows the redirect to the signed URL, so the answer is the file's own
    routes[`HEAD ${WEB}/releases/download/v0.15.1/${a.name}`] = () =>
      new Response(null, {
        status: 200,
        headers: { "content-length": String(a.size), "x-final": signed },
      });
  }
  const { f } = server(routes);
  const lines: string[] = [];
  const r = await readRelease({ fetch: f, log: (l) => lines.push(l) });
  assert.equal(r.source, "fallback");
  assert.equal(r.version, "0.15.1");
  assert.deepEqual(
    r.assets.map((a) => a.name).sort(),
    [...parseSums(sums).keys(), "SHA256SUMS"].sort(),
  );
  assert.equal(r.assets.find((a) => a.name === "install.sh")?.size, 7818);
  assert.ok(r.assets.every((a) => a.url.startsWith(`${WEB}/releases/download/v0.15.1/`)));
  assert.ok(!JSON.stringify(r).includes("sig="));
  assert.ok(!lines.some((l) => l.includes("sig=")));
});

test("fails when neither the API nor the release pages answer", async () => {
  const { f } = server({});
  await assert.rejects(
    readRelease({ fetch: f, ci: true }),
    /neither GitHub's API nor its release pages answered/,
  );
  await assert.rejects(readRelease({ fetch: f }), /PITBOARD_SITE_OFFLINE=1/);
});

test("refuses a draft, an rc, and a pre-release that is not a beta", async () => {
  const cases: [Record<string, unknown>, RegExp][] = [
    [{ draft: true }, /v0\.15\.1 is a draft/],
    [{ prerelease: true }, /v0\.15\.1 is a pre-release, which only a beta is/],
    [{ tag_name: "v1.0.0-rc1", prerelease: true }, /v1\.0\.0-rc1 is not the tag of a release/],
    [{ tag_name: "nightly" }, /nightly is not the tag of a release/],
  ];
  for (const [change, why] of cases) {
    const { f } = server({
      [`GET ${API}/releases/latest`]: () => json({ ...recorded, ...change }),
      [`GET ${SUMS_URL}`]: () => new Response(sums),
    });
    await assert.rejects(readRelease({ fetch: f }), why);
  }
});

test("reads the recorded fixture offline without the network", async () => {
  const { f, asked } = server({});
  const r = await readRelease({ fetch: f, fixture: { release: recorded, sums } });
  assert.equal(r.source, "fixture");
  assert.equal(r.version, "0.15.1");
  assert.equal(r.prerelease, false);
  assert.equal(asked.length, 0);
});

test("reads each beta GitHub has published, with its files, and a draft as not", async () => {
  const { f, asked } = server({
    [`GET ${API}/releases/tags/v1.0.0-beta.3`]: () => json({ message: "Not Found" }, 404),
    [`GET ${API}/releases/tags/v1.0.0-beta.2`]: () =>
      json({ ...recordedBeta, tag_name: "v1.0.0-beta.2", draft: true }),
    [`GET ${API}/releases/tags/v1.0.0-beta.1`]: () => json(recordedBeta),
    [`GET ${BETA_SUMS_URL}`]: () => new Response(betaSums),
  });
  const published = await readPublished({ fetch: f, token: "t0ken" }, [
    "1.0.0-beta.3",
    "1.0.0-beta.2",
    "1.0.0-beta.1",
  ]);
  assert.deepEqual([...published.keys()], ["1.0.0-beta.1"]);
  const r = published.get("1.0.0-beta.1")!;
  assert.equal(r.tag, "v1.0.0-beta.1");
  assert.equal(r.prerelease, true);
  assert.equal(r.source, "api");
  assert.equal(r.assets.length, 20);
  assert.equal(
    r.assets.find((a) => a.name === "Pitboard-v1.0.0-beta.1-windows-x64.msi")?.sha256,
    "494e0ea25832687ed788cf18fdcd725d6da18e99a01dc7a89f650511b9689cf4",
  );
  // a draft's files are not public, so they are never asked for
  assert.ok(!asked.some((a) => a.url.includes("/download/v1.0.0-beta.2/")));
  for (const a of asked) assert.equal(a.auth, a.url.startsWith(API) ? "Bearer t0ken" : null);
  assert.equal((await readPublished({ fetch: server({}).f }, [])).size, 0);
});

test("refuses a beta GitHub does not mark as a pre-release, or another tag's answer", async () => {
  const unmarked = server({
    [`GET ${API}/releases/tags/v1.0.0-beta.1`]: () => json({ ...recordedBeta, prerelease: false }),
    [`GET ${BETA_SUMS_URL}`]: () => new Response(betaSums),
  });
  await assert.rejects(
    readPublished({ fetch: unmarked.f }, ["1.0.0-beta.1"]),
    /v1\.0\.0-beta\.1 is a beta GitHub does not mark as a pre-release/,
  );
  const other = server({
    [`GET ${API}/releases/tags/v1.0.0-beta.1`]: () => json({ ...recorded, tag_name: "v1.0.0" }),
  });
  await assert.rejects(readPublished({ fetch: other.f }, ["1.0.0-beta.1"]), /v1\.0\.0/);
});

test("reads a beta from its release pages on a rate limit, and one with no files as unpublished", async () => {
  const routes: Record<string, Route> = {
    [`GET ${API}/releases/tags/v1.0.0-beta.2`]: () => json({ message: "rate limit" }, 403),
    [`GET ${WEB}/releases/download/v1.0.0-beta.2/SHA256SUMS`]: () =>
      new Response("Not Found", { status: 404 }),
    [`GET ${API}/releases/tags/v1.0.0-beta.1`]: () => json({ message: "rate limit" }, 403),
    [`GET ${BETA_SUMS_URL}`]: () => new Response(betaSums),
  };
  for (const a of recordedBeta.assets as { name: string; size: number }[]) {
    routes[`HEAD ${WEB}/releases/download/v1.0.0-beta.1/${a.name}`] = () =>
      new Response(null, { status: 200, headers: { "content-length": String(a.size) } });
  }
  const published = await readPublished({ fetch: server(routes).f }, [
    "1.0.0-beta.2",
    "1.0.0-beta.1",
  ]);
  assert.deepEqual([...published.keys()], ["1.0.0-beta.1"]);
  const r = published.get("1.0.0-beta.1")!;
  assert.equal(r.source, "fallback");
  assert.equal(r.prerelease, true);
  assert.deepEqual(
    r.assets.map((a) => a.name).sort(),
    [...parseSums(betaSums).keys(), "SHA256SUMS"].sort(),
  );
  assert.equal(
    r.assets.find((a) => a.name === "Pitboard-v1.0.0-beta.1-windows-arm64.msi")?.size,
    79483492,
  );
});

test("stops when neither the API nor the release pages say whether a beta is published", async () => {
  const { f } = server({
    [`GET ${API}/releases/tags/v1.0.0-beta.1`]: () => json({ message: "rate limit" }, 403),
  });
  await assert.rejects(
    readPublished({ fetch: f, ci: true }, ["1.0.0-beta.1"]),
    /neither GitHub's API nor its release pages answered/,
  );
  await assert.rejects(readPublished({ fetch: f }, ["1.0.0-beta.1"]), /PITBOARD_SITE_OFFLINE=1/);
});

test("takes the recorded betas offline without the network", async () => {
  const { f, asked } = server({});
  const published = await readPublished(
    { fetch: f, fixture: new Map([["1.0.0-beta.1", { release: recordedBeta, sums: betaSums }]]) },
    ["1.0.0-beta.2", "1.0.0-beta.1"],
  );
  assert.deepEqual([...published.keys()], ["1.0.0-beta.1"]);
  assert.equal(published.get("1.0.0-beta.1")?.source, "fixture");
  assert.equal(asked.length, 0);
});

test("gives no star count rather than a guessed one", async () => {
  const ok = server({ [`GET ${API}`]: () => json({ stargazers_count: 46 }) });
  assert.equal(await readStars({ fetch: ok.f }), 46);
  const limited = server({ [`GET ${API}`]: () => json({ message: "rate limit" }, 403) });
  assert.equal(await readStars({ fetch: limited.f }), null);
  const down = server({});
  assert.equal(await readStars({ fetch: down.f }), null);
  assert.equal(await readStars({ fetch: ok.f, offline: true }), null);
  const odd = server({ [`GET ${API}`]: () => json({ stargazers_count: "46" }) });
  assert.equal(await readStars({ fetch: odd.f }), null);
});

test("sizes read as /download prints them", () => {
  const cases: [number, string][] = [
    [624, "624 B"],
    [874, "874 B"],
    [7818, "7.8 KB"],
    [98355, "98 KB"],
    [98541, "99 KB"],
    [196025, "196 KB"],
    [1741831, "1.7 MB"],
    [2027528, "2.0 MB"],
    [13801164, "13.8 MB"],
  ];
  for (const [bytes, text] of cases) assert.equal(formatSize(bytes), text);
});

test("lists the release's files in order and fails on one it does not know", async () => {
  const r = await latest();
  const rows = fileRows(r);
  assert.deepEqual(
    rows.map((row) => row.asset.name),
    [
      "Pitboard-v0.15.1-macos.zip",
      "pitboard-v0.15.1-aarch64-apple-darwin.tar.gz",
      "pitboard-v0.15.1-x86_64-apple-darwin.tar.gz",
      "pitboard-v0.15.1-x86_64-unknown-linux-musl.tar.gz",
      "pitboard-v0.15.1-aarch64-unknown-linux-musl.tar.gz",
      "install.sh",
      "SHA256SUMS",
      "appcast.xml",
    ],
  );
  assert.equal(rows[0].bom?.name, "Pitboard-v0.15.1-macos.zip.cdx.json");
  assert.equal(rows[5].bom, null);
  const more: Release = {
    ...r,
    assets: [
      ...r.assets,
      { name: "pitboard-v0.15.1-riscv64.tar.gz", size: 1, sha256: null, url: "" },
    ],
  };
  assert.throws(() => fileRows(more), /does not describe/);
});

test("lists a beta's files in order, each with the platforms it serves", async () => {
  const rows = fileRows(await beta());
  assert.deepEqual(
    rows.map((row) => [row.asset.name, row.platforms]),
    [
      ["Pitboard-v1.0.0-beta.1-macos.zip", ["macos"]],
      ["Pitboard-v1.0.0-beta.1-windows-x64.msi", ["windows"]],
      ["Pitboard-v1.0.0-beta.1-windows-arm64.msi", ["windows"]],
      ["pitboard-v1.0.0-beta.1-aarch64-apple-darwin.tar.gz", ["macos"]],
      ["pitboard-v1.0.0-beta.1-x86_64-apple-darwin.tar.gz", ["macos"]],
      ["pitboard-v1.0.0-beta.1-x86_64-unknown-linux-musl.tar.gz", ["linux"]],
      ["pitboard-v1.0.0-beta.1-aarch64-unknown-linux-musl.tar.gz", ["linux"]],
      ["pitboard-v1.0.0-beta.1-x86_64-pc-windows-msvc.zip", ["windows"]],
      ["pitboard-v1.0.0-beta.1-aarch64-pc-windows-msvc.zip", ["windows"]],
      ["install.sh", ["macos", "linux"]],
      ["SHA256SUMS", ["macos", "windows", "linux"]],
    ],
  );
  assert.deepEqual(
    rows
      .filter((row) => row.platforms.includes("windows") && row.bom)
      .map((row) => [row.forHtml, row.bom?.name]),
    [
      ["The app, Windows on x64", "Pitboard-v1.0.0-beta.1-windows-x64.msi.cdx.json"],
      ["The app, Windows on ARM64", "Pitboard-v1.0.0-beta.1-windows-arm64.msi.cdx.json"],
      [
        "Command line, Windows on x64",
        "pitboard-v1.0.0-beta.1-x86_64-pc-windows-msvc.zip.cdx.json",
      ],
      [
        "Command line, Windows on ARM64",
        "pitboard-v1.0.0-beta.1-aarch64-pc-windows-msvc.zip.cdx.json",
      ],
    ],
  );
});

test("describes every Linux package and its bill of materials, and SHA256SUMS names them", async () => {
  const v = "1.0.0-beta.8";
  const packages = [
    `pitboard-app-v${v}-amd64.deb`,
    `pitboard-app-v${v}-arm64.deb`,
    `pitboard-app-v${v}-x86_64.rpm`,
    `pitboard-app-v${v}-aarch64.rpm`,
    `pitboard-v${v}-amd64.deb`,
    `pitboard-v${v}-arm64.deb`,
    `pitboard-v${v}-x86_64.rpm`,
    `pitboard-v${v}-aarch64.rpm`,
  ];
  const keyring = `pitboard-archive-keyring-v${v}-all.deb`;
  const release = made(v, [
    `pitboard-v${v}-x86_64-unknown-linux-musl.tar.gz`,
    ...packages,
    ...packages.map((name) => `${name}.cdx.json`),
    keyring,
    "install.sh",
    "SHA256SUMS",
  ]);
  const rows = fileRows(release);
  assert.deepEqual(
    rows.map((row) => [row.asset.name, row.forHtml, row.bom?.name ?? null, row.platforms]),
    [
      [
        `pitboard-app-v${v}-amd64.deb`,
        "The app, a Debian package for x86_64",
        `pitboard-app-v${v}-amd64.deb.cdx.json`,
        ["linux"],
      ],
      [
        `pitboard-app-v${v}-arm64.deb`,
        "The app, a Debian package for aarch64",
        `pitboard-app-v${v}-arm64.deb.cdx.json`,
        ["linux"],
      ],
      [
        `pitboard-app-v${v}-x86_64.rpm`,
        "The app, an RPM package for x86_64",
        `pitboard-app-v${v}-x86_64.rpm.cdx.json`,
        ["linux"],
      ],
      [
        `pitboard-app-v${v}-aarch64.rpm`,
        "The app, an RPM package for aarch64",
        `pitboard-app-v${v}-aarch64.rpm.cdx.json`,
        ["linux"],
      ],
      [
        `pitboard-v${v}-x86_64-unknown-linux-musl.tar.gz`,
        "Command line, Linux on x86_64",
        null,
        ["linux"],
      ],
      [
        `pitboard-v${v}-amd64.deb`,
        "Command line, a Debian package for x86_64",
        `pitboard-v${v}-amd64.deb.cdx.json`,
        ["linux"],
      ],
      [
        `pitboard-v${v}-arm64.deb`,
        "Command line, a Debian package for aarch64",
        `pitboard-v${v}-arm64.deb.cdx.json`,
        ["linux"],
      ],
      [
        `pitboard-v${v}-x86_64.rpm`,
        "Command line, an RPM package for x86_64",
        `pitboard-v${v}-x86_64.rpm.cdx.json`,
        ["linux"],
      ],
      [
        `pitboard-v${v}-aarch64.rpm`,
        "Command line, an RPM package for aarch64",
        `pitboard-v${v}-aarch64.rpm.cdx.json`,
        ["linux"],
      ],
      [keyring, "The repository's key, a Debian package", null, ["linux"]],
      ["install.sh", "The install script", null, ["macos", "linux"]],
      [
        "SHA256SUMS",
        "Checksums of the apps, the command line archives, the Linux packages and <code>install.sh</code>",
        null,
        ["macos", "windows", "linux"],
      ],
    ],
  );
  const without = fileRows(made(v, ["install.sh", "SHA256SUMS"]));
  assert.equal(
    without.at(-1)?.forHtml,
    "Checksums of the apps, the command line archives and <code>install.sh</code>",
  );
  for (const name of [`pitboard-v${v}-riscv64.deb`, `pitboard-plasma-widget-v${v}-all.deb`]) {
    assert.throws(() => fileRows(made(v, [name, "SHA256SUMS"])), /does not describe/);
  }
});

test("gives each update feed to its own platform", async () => {
  const r = await latest();
  const feed = { name: "appcast-windows.xml", size: 1, sha256: null, url: "" };
  const rows = fileRows({ ...r, assets: [...r.assets, feed] });
  assert.deepEqual(
    rows.slice(-2).map((row) => [row.asset.name, row.forHtml, row.platforms, row.bom]),
    [
      ["appcast.xml", "The feed the app updates from", ["macos"], null],
      ["appcast-windows.xml", "The feed the Windows app updates from", ["windows"], null],
    ],
  );
});

test("takes Windows files from the latest release once it has both installers, and until then from the newest published beta", async () => {
  const r = await latest();
  const b = await beta();
  assert.equal(windowsRelease(r, [b]), b);
  const later = installers("1.0.0-beta.2");
  assert.equal(windowsRelease(r, [b, later]), later);
  assert.equal(windowsRelease(r, [later, b]), later);
  const half = made("1.0.0-beta.3", ["Pitboard-v1.0.0-beta.3-windows-x64.msi", "SHA256SUMS"]);
  assert.equal(windowsRelease(r, [b, half]), b);
  const full = installers("1.0.0");
  assert.equal(windowsRelease(full, [installers("1.1.0-beta.1")]), full);
  assert.throws(() => windowsRelease(r, []), /no published release has the Windows installers/);
});

test("takes Linux files from the latest release once it has the app's packages, and until then from the newest published beta that has them", async () => {
  const r = await latest();
  assert.equal(linuxRelease(r, [await beta()]), r);
  assert.equal(linuxRelease(r, []), r);
  const packaged = linuxPackages("1.0.0-beta.8");
  const later = linuxPackages("1.0.0-beta.9");
  assert.equal(linuxRelease(r, [await beta(), packaged]), packaged);
  assert.equal(linuxRelease(r, [later, packaged]), later);
  const half = linuxPackages("1.0.0-beta.10", ["amd64.deb", "x86_64.rpm"]);
  assert.equal(linuxRelease(r, [packaged, half]), packaged);
  const full = linuxPackages("1.0.0");
  assert.equal(linuxRelease(full, [linuxPackages("1.1.0-beta.1")]), full);
});

test("offers the Linux app only from a release with its packages for both machines", async () => {
  assert.equal(linuxApp(await latest()), false);
  assert.equal(linuxApp(linuxPackages("1.0.0-beta.8")), true);
  assert.equal(linuxApp(linuxPackages("1.0.0-beta.8", ["amd64.deb", "x86_64.rpm"])), false);
});

test("says which systems take betas while macOS stays behind, and nothing once none does", async () => {
  const r = await latest();
  const b = await beta();
  assert.equal(
    ahead(r, { macos: r, windows: b, linux: r }),
    "Until 1.0.0, Windows gets betas, and macOS and Linux get 0.15.1.",
  );
  const packaged = linuxPackages("1.0.0-beta.8");
  assert.equal(
    ahead(r, { macos: r, windows: b, linux: packaged }),
    "Until 1.0.0, Windows and Linux get betas, and macOS gets 0.15.1.",
  );
  const full = installers("1.0.0");
  assert.equal(ahead(full, { macos: full, windows: full, linux: full }), null);
});

test("finds the Windows installer and zip of each architecture", async () => {
  const b = await beta();
  assert.deepEqual(
    (["x64", "arm64"] as const).map((arch) => [
      windowsMsi(b, arch).name,
      windowsMsi(b, arch).size,
      windowsZip(b, arch).name,
    ]),
    [
      [
        "Pitboard-v1.0.0-beta.1-windows-x64.msi",
        83587732,
        "pitboard-v1.0.0-beta.1-x86_64-pc-windows-msvc.zip",
      ],
      [
        "Pitboard-v1.0.0-beta.1-windows-arm64.msi",
        79483492,
        "pitboard-v1.0.0-beta.1-aarch64-pc-windows-msvc.zip",
      ],
    ],
  );
  const r = await latest();
  assert.throws(() => windowsMsi(r, "x64"), /v0\.15\.1 has no Pitboard-v0\.15\.1-windows-x64\.msi/);
  assert.throws(
    () => windowsZip(r, "arm64"),
    /v0\.15\.1 has no pitboard-v0\.15\.1-aarch64-pc-windows-msvc\.zip/,
  );
});

test("reads Windows files as signed in a full release, and in a beta from the version a signer was set at", async () => {
  const b = await beta();
  assert.equal(windowsSigned(b, null), false);
  assert.equal(windowsSigned(b, "1.0.0-beta.2"), false);
  assert.equal(windowsSigned(installers("1.0.0-beta.2"), "1.0.0-beta.2"), true);
  assert.equal(windowsSigned(installers("1.1.0-beta.1"), "1.0.0-beta.2"), true);
  assert.equal(windowsSigned(installers("1.0.0"), null), true);
});

test("reads SHA256SUMS in text and binary mode, and refuses anything else", () => {
  const a = "a".repeat(64);
  assert.equal(parseSums(`${a}  x.tar.gz\n${a} *y.zip\n`).size, 2);
  assert.throws(() => parseSums("not a sum\n"), /cannot read/);
});
