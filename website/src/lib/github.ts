import { REPO, SLUG } from "../data/site.ts";
import { compareVersions } from "./changelog.ts";
import type { Platform } from "./platform.ts";

const API = `https://api.github.com/repos/${SLUG}`;

export interface Asset {
  name: string;
  size: number;
  sha256: string | null;
  url: string;
}

export interface Release {
  version: string;
  tag: string;
  // a beta, which GitHub marks as a pre-release
  prerelease: boolean;
  assets: Asset[];
  source: "api" | "fallback" | "fixture";
}

export interface Fixture {
  release: unknown;
  sums: string;
}

interface Access {
  fetch: typeof fetch;
  token?: string;
  // a CI build that cannot read a release fails, so the last deploy stays live
  ci?: boolean;
  log?: (line: string) => void;
}

export interface ReadOptions extends Access {
  fixture?: Fixture;
}

export interface PublishedOptions extends Access {
  // the betas an offline build takes as published, by version
  fixture?: ReadonlyMap<string, Fixture>;
}

interface ApiAsset {
  name: string;
  size: number;
  digest?: string | null;
  browser_download_url: string;
}

interface ApiRelease {
  tag_name: string;
  draft: boolean;
  prerelease: boolean;
  assets: ApiAsset[];
}

export function parseSums(text: string): Map<string, string> {
  const sums = new Map<string, string>();
  for (const line of text.split("\n")) {
    if (!line.trim()) continue;
    const m = /^([0-9a-f]{64}) [ *](\S+)$/.exec(line.trim());
    if (!m) throw new Error(`SHA256SUMS has a line it cannot read: ${line}`);
    sums.set(m[2], m[1]);
  }
  return sums;
}

// A release candidate's tag never reaches the site.
function versionOf(tag: string): string {
  const m = /^v(\d+\.\d+\.\d+(?:-beta\.\d+)?)$/.exec(tag);
  if (!m) throw new Error(`${tag} is not the tag of a release or a beta`);
  return m[1];
}

function fromApi(json: unknown, sumsText: string): Omit<Release, "source"> {
  const r = json as ApiRelease;
  if (!r || typeof r.tag_name !== "string" || !Array.isArray(r.assets)) {
    throw new Error("the release answer has no tag or no files");
  }
  if (r.draft) throw new Error(`${r.tag_name} is a draft, not a published release`);
  const version = versionOf(r.tag_name);
  const prerelease = version.includes("-");
  if (r.prerelease && !prerelease)
    throw new Error(`${r.tag_name} is a pre-release, which only a beta is`);
  if (!r.prerelease && prerelease)
    throw new Error(`${r.tag_name} is a beta GitHub does not mark as a pre-release`);
  const sums = parseSums(sumsText);
  const assets = r.assets.map((a): Asset => {
    const digest = a.digest?.startsWith("sha256:") ? a.digest.slice(7) : null;
    const listed = sums.get(a.name) ?? null;
    if (listed && digest && listed !== digest) {
      throw new Error(`SHA256SUMS and GitHub disagree on ${a.name}`);
    }
    const sha256 = listed ?? digest;
    if (sha256 !== null && !/^[0-9a-f]{64}$/.test(sha256))
      throw new Error(`${a.name} has no SHA-256`);
    if (a.browser_download_url !== `${REPO}/releases/download/${r.tag_name}/${a.name}`) {
      throw new Error(`${a.name} downloads from somewhere else: ${a.browser_download_url}`);
    }
    return { name: a.name, size: a.size, sha256, url: a.browser_download_url };
  });
  for (const name of sums.keys()) {
    if (!assets.some((a) => a.name === name)) {
      throw new Error(`SHA256SUMS lists ${name}, which the release does not have`);
    }
  }
  return { version, tag: r.tag_name, prerelease, assets };
}

async function get(f: typeof fetch, url: string, init: RequestInit = {}): Promise<Response> {
  return f(url, { ...init, signal: AbortSignal.timeout(20_000) });
}

function apiHeaders(token?: string): Record<string, string> {
  const headers: Record<string, string> = {
    accept: "application/vnd.github+json",
    "x-github-api-version": "2022-11-28",
  };
  if (token) headers.authorization = `Bearer ${token}`;
  return headers;
}

// GitHub's answer for a release, or null when the API is limited, failing or out of reach and the
// release pages answer instead.
async function askApi(o: Access, path: string, what: string): Promise<Response | null> {
  let res: Response;
  try {
    res = await get(o.fetch, `${API}${path}`, { headers: apiHeaders(o.token) });
  } catch (e) {
    o.log?.(`${what}: api unreachable: ${(e as Error).message}`);
    return null;
  }
  const left = res.headers.get("x-ratelimit-remaining") ?? "unknown";
  o.log?.(`${what}: api ${res.status}, rate limit left ${left}`);
  if (res.ok || res.status === 404) return res;
  if (res.status === 401 && o.token) {
    throw new Error("GitHub refused the token (401): it has expired or was revoked");
  }
  if (res.status === 403 || res.status === 429 || res.status >= 500) return null;
  throw new Error(`GitHub answered ${res.status} for ${path}`);
}

async function withSums(f: typeof fetch, json: ApiRelease): Promise<Release> {
  const sums = json.assets?.find((a) => a.name === "SHA256SUMS");
  if (!sums) throw new Error(`${json.tag_name} has no SHA256SUMS`);
  const text = await get(f, sums.browser_download_url);
  if (!text.ok) throw new Error(`SHA256SUMS answered ${text.status}`);
  return { ...fromApi(json, await text.text()), source: "api" };
}

async function fromPagesInstead<T>(o: Access, read: () => Promise<T>): Promise<T> {
  try {
    return await read();
  } catch (e) {
    const why = `neither GitHub's API nor its release pages answered: ${(e as Error).message}`;
    if (o.ci) throw new Error(why);
    throw new Error(`${why}. Build offline with PITBOARD_SITE_OFFLINE=1.`);
  }
}

export async function readRelease(o: ReadOptions): Promise<Release> {
  if (o.fixture) {
    o.log?.("release: the recorded fixture, PITBOARD_SITE_OFFLINE is set");
    return fromFixture(o.fixture);
  }
  o.log?.(`release: token set: ${o.token ? "yes" : "no"}`);
  const res = await askApi(o, "/releases/latest", "release");
  if (res?.ok) return withSums(o.fetch, (await res.json()) as ApiRelease);
  if (res) throw new Error(`GitHub answered ${res.status} for the latest release`);
  return fromPagesInstead(o, async () => {
    const latest = await get(o.fetch, `${REPO}/releases/latest`, {
      method: "HEAD",
      redirect: "manual",
    });
    const m = /\/releases\/tag\/([^/?#]+)$/.exec(latest.headers.get("location") ?? "");
    if (!m) throw new Error(`releases/latest answered ${latest.status} without a tag`);
    const tag = decodeURIComponent(m[1]);
    const release = await fromPages(o, tag);
    if (!release) throw new Error(`${tag} has no SHA256SUMS`);
    return release;
  });
}

// Null for a tag with no published release: a draft's files do not download. The signed URL a
// download redirects to is never logged or kept.
async function fromPages(o: Access, tag: string): Promise<Release | null> {
  const version = versionOf(tag);
  const base = `${REPO}/releases/download/${tag}`;
  const sumsRes = await get(o.fetch, `${base}/SHA256SUMS`);
  if (sumsRes.status === 404) return null;
  if (!sumsRes.ok) throw new Error(`SHA256SUMS answered ${sumsRes.status}`);
  const sums = parseSums(await sumsRes.text());
  const assets: Asset[] = [];
  for (const name of [...sums.keys(), "SHA256SUMS"]) {
    const url = `${base}/${name}`;
    const res = await get(o.fetch, url, { method: "HEAD" });
    const size = Number(res.headers.get("content-length"));
    if (!res.ok || !Number.isFinite(size) || size <= 0) {
      throw new Error(`${name} answered ${res.status} without a size`);
    }
    assets.push({ name, size, sha256: sums.get(name) ?? null, url });
  }
  o.log?.(`release ${tag}: from the release pages, ${assets.length} files`);
  return { version, tag, prerelease: version.includes("-"), assets, source: "fallback" };
}

export async function readPublished(
  o: PublishedOptions,
  versions: string[],
): Promise<Map<string, Release>> {
  const published = new Map<string, Release>();
  for (const version of versions) {
    const release = await readBeta(o, version);
    if (release) published.set(version, release);
  }
  return published;
}

function fromFixture(fixture: Fixture): Release {
  return { ...fromApi(fixture.release, fixture.sums), source: "fixture" };
}

// GitHub's API answers 404 for the tag of a release that is not published.
async function readBeta(o: PublishedOptions, version: string): Promise<Release | null> {
  if (o.fixture) {
    const recorded = o.fixture.get(version);
    return recorded ? fromFixture(recorded) : null;
  }
  const tag = `v${version}`;
  const res = await askApi(o, `/releases/tags/${tag}`, `release ${tag}`);
  if (!res) return fromPagesInstead(o, () => fromPages(o, tag));
  if (res.status === 404) return null;
  const json = (await res.json()) as ApiRelease;
  if (json.tag_name !== tag)
    throw new Error(`GitHub answered ${json.tag_name} for the release ${tag}`);
  return json.draft ? null : withSums(o.fetch, json);
}

export async function readStars(o: Access & { offline?: boolean }) {
  if (o.offline) return null;
  try {
    const headers: Record<string, string> = { accept: "application/vnd.github+json" };
    if (o.token) headers.authorization = `Bearer ${o.token}`;
    const res = await get(o.fetch, API, { headers });
    o.log?.(`stars: api ${res.status}`);
    if (!res.ok) return null;
    const n = ((await res.json()) as { stargazers_count?: unknown }).stargazers_count;
    return typeof n === "number" && Number.isInteger(n) && n >= 0 ? n : null;
  } catch (e) {
    o.log?.(`stars: ${(e as Error).message}`);
    return null;
  }
}

// 1 MB is 1,000,000 bytes, as GitHub and Finder count.
export function formatSize(bytes: number): string {
  if (bytes < 1000) return `${bytes} B`;
  if (bytes < 10_000) return `${(bytes / 1000).toFixed(1)} KB`;
  if (bytes < 1_000_000) return `${Math.round(bytes / 1000)} KB`;
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}

export interface FileRow {
  asset: Asset;
  forHtml: string;
  bom: Asset | null;
  platforms: Platform[];
}

// A file the release gains fails the build until it is described here.
export function fileRows(release: Release): FileRow[] {
  const v = release.version;
  const known: [string, string, Platform[]][] = [
    [`Pitboard-v${v}-macos.zip`, "The app, for Apple silicon and Intel", ["macos"]],
    [`Pitboard-v${v}-windows-x64.msi`, "The app, Windows on x64", ["windows"]],
    [`Pitboard-v${v}-windows-arm64.msi`, "The app, Windows on ARM64", ["windows"]],
    [
      `pitboard-v${v}-aarch64-apple-darwin.tar.gz`,
      "Command line, macOS on Apple silicon",
      ["macos"],
    ],
    [`pitboard-v${v}-x86_64-apple-darwin.tar.gz`, "Command line, macOS on Intel", ["macos"]],
    [`pitboard-v${v}-x86_64-unknown-linux-musl.tar.gz`, "Command line, Linux on x86_64", ["linux"]],
    [
      `pitboard-v${v}-aarch64-unknown-linux-musl.tar.gz`,
      "Command line, Linux on aarch64",
      ["linux"],
    ],
    [`pitboard-v${v}-x86_64-pc-windows-msvc.zip`, "Command line, Windows on x64", ["windows"]],
    [`pitboard-v${v}-aarch64-pc-windows-msvc.zip`, "Command line, Windows on ARM64", ["windows"]],
    ["install.sh", "The install script", ["macos", "linux"]],
    [
      "SHA256SUMS",
      "Checksums of the apps, the command line archives and <code>install.sh</code>",
      ["macos", "windows", "linux"],
    ],
    ["appcast.xml", "The feed the app updates from", ["macos"]],
    ["appcast-windows.xml", "The feed the Windows app updates from", ["windows"]],
  ];
  const byName = new Map(release.assets.map((a) => [a.name, a]));
  for (const a of release.assets) {
    const main = a.name.endsWith(".cdx.json") ? a.name.slice(0, -".cdx.json".length) : a.name;
    if (!known.some(([n]) => n === main))
      throw new Error(`the release has ${a.name}, which /download does not describe`);
  }
  return known
    .filter(([name]) => byName.has(name))
    .map(([name, forHtml, platforms]) => ({
      asset: byName.get(name)!,
      forHtml,
      bom: byName.get(`${name}.cdx.json`) ?? null,
      platforms,
    }));
}

export type Arch = "x64" | "arm64";

const TRIPLES: Record<Arch, string> = {
  x64: "x86_64-pc-windows-msvc",
  arm64: "aarch64-pc-windows-msvc",
};

function file(release: Release, name: string): Asset {
  const asset = release.assets.find((a) => a.name === name);
  if (!asset) throw new Error(`${release.tag} has no ${name}`);
  return asset;
}

export function appZip(release: Release): Asset {
  return file(release, `Pitboard-v${release.version}-macos.zip`);
}

const msiName = (release: Release, arch: Arch) =>
  `Pitboard-v${release.version}-windows-${arch}.msi`;

export function windowsMsi(release: Release, arch: Arch): Asset {
  return file(release, msiName(release, arch));
}

export function windowsZip(release: Release, arch: Arch): Asset {
  return file(release, `pitboard-v${release.version}-${TRIPLES[arch]}.zip`);
}

const hasInstallers = (release: Release) =>
  (["x64", "arm64"] as const).every((arch) =>
    release.assets.some((a) => a.name === msiName(release, arch)),
  );

// Windows files come from the latest release once it carries both installers, and from the newest
// published beta until then, so a later beta never takes over from a full release.
export function windowsRelease(latest: Release, betas: Iterable<Release>): Release {
  if (hasInstallers(latest)) return latest;
  const [newest] = [...betas]
    .filter(hasInstallers)
    .sort((a, b) => compareVersions(b.version, a.version));
  if (!newest) throw new Error("no published release has the Windows installers");
  return newest;
}

export function windowsAhead(windows: Release, latest: Release): string | null {
  return windows.version === latest.version
    ? null
    : `Until 1.0.0, Windows gets betas, and macOS and Linux get ${latest.version}.`;
}

// From 1.0.0 the release workflow refuses an unsigned rc or full release, and signs a beta once
// WINDOWS_SIGNER is set, from the version WINDOWS_SIGNED_FROM names.
export function windowsSigned(release: Release, signedFrom: string | null): boolean {
  if (!release.prerelease) return true;
  return signedFrom !== null && compareVersions(release.version, signedFrom) >= 0;
}
