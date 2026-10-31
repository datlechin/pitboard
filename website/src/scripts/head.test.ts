import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { runInNewContext } from "node:vm";
import { onOf, type Platform } from "../lib/platform.ts";

const script = readFileSync(new URL("./head.js", import.meta.url), "utf8");

const MAC =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
const WINDOWS =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0";
const LINUX = "Mozilla/5.0 (X11; Linux x86_64; rv:143.0) Gecko/20100101 Firefox/143.0";
const CHROMEOS =
  "Mozilla/5.0 (X11; CrOS x86_64 14541.0.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
const IPHONE =
  "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.6 Mobile/15E148 Safari/604.1";
const ANDROID =
  "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Mobile Safari/537.36";

interface Visit {
  ua: string;
  touch?: number;
  hint?: string;
  // null when the browser refuses storage
  stored?: Record<string, string> | null;
  // this visit's, which keeps the platform chosen
  session?: Record<string, string> | null;
  hash?: string;
}

// runs head.js against a stand-in page and returns the attributes it set on the root
function visit({ ua, touch = 0, hint, stored = {}, session = {}, hash = "" }: Visit) {
  const attrs: Record<string, string> = {};
  runInNewContext(script, {
    document: {
      documentElement: {
        setAttribute: (k: string, v: string) => (attrs[k] = v),
        classList: { add: (c: string) => (attrs.class = c) },
      },
      querySelectorAll: () => [],
    },
    location: { hash },
    navigator: {
      userAgent: ua,
      maxTouchPoints: touch,
      userAgentData: hint ? { platform: hint } : undefined,
    },
    localStorage: {
      getItem: (k: string) => {
        if (stored === null) throw new Error("storage is blocked");
        return stored[k] ?? null;
      },
    },
    sessionStorage: {
      getItem: (k: string) => {
        if (session === null) throw new Error("storage is blocked");
        return session[k] ?? null;
      },
    },
    matchMedia: () => ({ matches: true }),
  });
  return attrs;
}

test("shows each system its own platform on a first visit, and macOS to one it cannot tell", () => {
  assert.equal(visit({ ua: MAC })["data-platform"], "macos");
  assert.equal(visit({ ua: WINDOWS })["data-platform"], "windows");
  assert.equal(visit({ ua: LINUX })["data-platform"], "linux");
  assert.equal(visit({ ua: CHROMEOS })["data-platform"], "linux");
  assert.equal(visit({ ua: "Mozilla/5.0", hint: "Windows" })["data-platform"], "windows");
  assert.equal(visit({ ua: "Mozilla/5.0" })["data-platform"], "macos");
});

test("shows a phone or a tablet macOS, and marks it as one", () => {
  for (const v of [{ ua: IPHONE }, { ua: ANDROID }, { ua: MAC, touch: 5 }]) {
    const attrs = visit(v);
    assert.equal(attrs["data-platform"], "macos");
    assert.equal(attrs["data-os"], "mobile");
  }
  assert.equal(visit({ ua: WINDOWS })["data-os"], undefined);
  assert.equal(visit({ ua: LINUX })["data-os"], undefined);
});

test("keeps the platform a reader chose for the visit, and detects one past a value it does not know", () => {
  assert.equal(visit({ ua: MAC, session: { platform: "windows" } })["data-platform"], "windows");
  assert.equal(visit({ ua: WINDOWS, session: { platform: "linux" } })["data-platform"], "linux");
  assert.equal(visit({ ua: WINDOWS, session: { platform: "beos" } })["data-platform"], "windows");
  assert.equal(visit({ ua: LINUX, session: null })["data-platform"], "linux");
});

// A Mac reader who once looked at the Linux tab still opens on macOS the next time they come.
test("shows the reader's own platform on a new visit, whatever an earlier one chose", () => {
  assert.equal(visit({ ua: MAC, stored: { platform: "linux" } })["data-platform"], "macos");
  assert.equal(visit({ ua: WINDOWS, stored: { platform: "macos" } })["data-platform"], "windows");
});

test("opens the app on every system, and Linux's command line on ChromeOS until a surface is chosen", () => {
  assert.equal(visit({ ua: LINUX })["data-surface"], "app");
  assert.equal(visit({ ua: MAC, session: { platform: "linux" } })["data-surface"], "app");
  assert.equal(visit({ ua: WINDOWS })["data-surface"], "app");
  assert.equal(visit({ ua: MAC, stored: { tab: "cli" } })["data-surface"], "cli");
  assert.equal(visit({ ua: CHROMEOS })["data-surface"], "cli");
  assert.equal(visit({ ua: "Mozilla/5.0", hint: "Chrome OS" })["data-surface"], "cli");
  assert.equal(visit({ ua: CHROMEOS, stored: { tab: "app" } })["data-surface"], "app");
  assert.equal(visit({ ua: CHROMEOS, session: { platform: "macos" } })["data-surface"], "app");
});

test("shows GNOME until the reader chooses KDE Plasma, and where storage is blocked", () => {
  assert.equal(visit({ ua: LINUX })["data-desktop"], "gnome");
  assert.equal(visit({ ua: LINUX, stored: { desktop: "kde" } })["data-desktop"], "kde");
  assert.equal(visit({ ua: MAC, stored: { desktop: "kde" } })["data-desktop"], "kde");
  assert.equal(visit({ ua: LINUX, stored: { desktop: "xfce" } })["data-desktop"], "gnome");
  assert.equal(visit({ ua: LINUX, stored: null })["data-desktop"], "gnome");
});

// The home page's sections for some systems alone, with the systems index.astro shows each to.
const page = readFileSync(new URL("../pages/index.astro", import.meta.url), "utf8");
const sections = [...page.matchAll(/<Feature\b([^>]*)>/g)].flatMap(([, attrs]) => {
  const on = /\bon=\{\[([^\]]*)\]\}/.exec(attrs)?.[1];
  return on ? [{ id: /\bid="([^"]+)"/.exec(attrs)![1], on: on.match(/[a-z]+/g)! }] : [];
});
const readers = [
  { system: "macos", ua: MAC },
  { system: "windows", ua: WINDOWS },
  { system: "gnome", ua: LINUX },
  { system: "kde", ua: LINUX, stored: { desktop: "kde" } },
];

test("shows macOS for the visit where a link names a section the reader's system does not show", () => {
  // head.js's map is index.astro's sections, each with the data-on its Feature gives it
  const only = JSON.parse(/var only = (\{[^}]*\})/.exec(script)![1]);
  assert.deepEqual(
    only,
    Object.fromEntries(sections.map(({ id, on }) => [`#${id}`, onOf(on as Platform[])])),
  );
  for (const { id, on } of sections) {
    assert.ok(on.includes("macos"), `#${id} shows to macOS`);
    for (const { system, ua, stored } of readers) {
      const shown = visit({ ua, stored, hash: `#${id}` })["data-platform"];
      const own = system === "gnome" || system === "kde" ? "linux" : system;
      assert.equal(
        shown,
        on.includes(system) || on.includes(own) ? own : "macos",
        `${system} #${id}`,
      );
    }
  }
  assert.equal(visit({ ua: LINUX, hash: "#security" })["data-platform"], "linux");
  assert.equal(visit({ ua: LINUX, hash: "#constructor" })["data-platform"], "linux");
  assert.equal(visit({ ua: LINUX, stored: null, hash: "#desktop" })["data-platform"], "macos");
});

test("shows the platform a link names for the visit, over the stored one and the detected one", () => {
  assert.equal(visit({ ua: MAC, hash: "#windows" })["data-platform"], "windows");
  assert.equal(visit({ ua: WINDOWS, hash: "#linux" })["data-platform"], "linux");
  assert.equal(
    visit({ ua: LINUX, stored: { platform: "windows" }, hash: "#macos" })["data-platform"],
    "macos",
  );
  assert.equal(visit({ ua: WINDOWS, hash: "#release-files" })["data-platform"], "windows");
  assert.equal(visit({ ua: MAC, stored: null, hash: "#linux" })["data-platform"], "linux");
});
