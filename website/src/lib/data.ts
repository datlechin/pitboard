import changelogText from "../../../CHANGELOG.md?raw";
import fixtureRelease from "../data/fixtures/release-latest.json";
import fixtureSums from "../data/fixtures/SHA256SUMS?raw";
import { highlights } from "../data/highlights.ts";
import { WINDOWS_SIGNED_FROM } from "../data/site.ts";
import { parseChangelog, plainText, renderInline, type Version } from "./changelog.ts";
import {
  linuxRelease,
  readPublished,
  readRelease,
  readStars,
  windowsRelease,
  windowsSigned,
  type Fixture,
  type Release,
} from "./github.ts";
import type { Platform } from "./platform.ts";
import { ahead, shown } from "./shown.ts";

// An offline build takes each beta recorded in fixtures/ as published.
const recordedReleases = import.meta.glob<unknown>("../data/fixtures/release-v*.json", {
  eager: true,
  import: "default",
});
const recordedSums = import.meta.glob<string>("../data/fixtures/SHA256SUMS-v*", {
  eager: true,
  query: "?raw",
  import: "default",
});

function recordedBetas(): Map<string, Fixture> {
  return new Map(
    Object.entries(recordedReleases).map(([path, release]) => {
      const version = path.slice("../data/fixtures/release-v".length, -".json".length);
      const sums = recordedSums[`../data/fixtures/SHA256SUMS-v${version}`];
      if (sums === undefined) throw new Error(`src/data/fixtures has no SHA256SUMS-v${version}`);
      return [version, { release, sums }];
    }),
  );
}

export interface Download {
  release: Release;
  notes: Version;
}

export interface SiteData {
  versions: Version[];
  // macOS files come from the latest release, Windows and Linux files from windowsRelease and
  // linuxRelease
  downloads: Record<Platform, Download>;
  windowsSigned: boolean;
  stars: number | null;
}

let loading: Promise<SiteData> | null = null;

// once per build, so every page and the feed show the same releases
export function siteData(): Promise<SiteData> {
  loading ??= load();
  return loading;
}

async function load(): Promise<SiteData> {
  const env = process.env;
  const offline = env.PITBOARD_SITE_OFFLINE === "1";
  const token = env.GITHUB_TOKEN || undefined;
  const ci = Boolean(env.CI || env.WORKERS_CI);
  const log = (line: string) => console.log(`[site] ${line}`);
  const all = parseChangelog(changelogText);
  // the release commit fails here, not the pull requests after the release is out
  const [newest] = all;
  if (newest) highlight(newest.version);
  const latest = await readRelease({
    fetch,
    token,
    ci,
    fixture: offline ? { release: fixtureRelease, sums: fixtureSums } : undefined,
    log,
  });
  const download = (release: Release): Download => {
    const notes = all.find((v) => v.version === release.version);
    if (!notes) throw new Error(`CHANGELOG.md has no section for ${release.version}`);
    return { release, notes };
  };
  const macos = download(latest);
  // a version written up before its release is published stays off the site
  const betas = await readPublished(
    { fetch, token, ci, fixture: offline ? recordedBetas() : undefined, log },
    ahead(all, latest.version),
  );
  const versions = shown(all, latest.version, new Set(betas.keys()));
  const windows = download(windowsRelease(latest, betas.values()));
  const linux = download(linuxRelease(latest, betas.values()));
  const stars = await readStars({ fetch, token, offline, log });
  return {
    versions,
    downloads: { macos, windows, linux },
    windowsSigned: windowsSigned(windows.release, WINDOWS_SIGNED_FROM),
    stars,
  };
}

export function highlight(version: string): string {
  const text = highlights[version];
  if (!text) throw new Error(`website/src/data/highlights.ts has no line for ${version}`);
  const html = renderInline(text);
  const length = plainText(html).length;
  if (length > 60) throw new Error(`the highlight for ${version} is ${length} characters, over 60`);
  return html;
}
