import type { Platform } from "./platform.ts";

// A version only when every system downloads one release, so none while Windows takes a beta.
export function application(
  site: URL,
  downloads: Record<Platform, { release: { version: string } }>,
) {
  const [version, ...others] = new Set(Object.values(downloads).map((d) => d.release.version));
  return {
    "@context": "https://schema.org",
    "@type": "SoftwareApplication",
    name: "Pitboard",
    applicationCategory: "DeveloperApplication",
    operatingSystem: "macOS 14 or later, Windows 10 version 2004 or later, Linux",
    ...(others.length === 0 ? { softwareVersion: version } : {}),
    downloadUrl: new URL("/download", site).href,
    license: "https://www.apache.org/licenses/LICENSE-2.0",
    url: new URL("/", site).href,
  };
}
