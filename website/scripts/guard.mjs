import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { gzipSync } from "node:zlib";
import { closest, elements, parse, textOf, walk } from "./html.mjs";

const DIST = new URL("../dist/", import.meta.url).pathname;
const FONTS = new URL("../src/assets/fonts/", import.meta.url).pathname;
const DOCS = new URL("../../docs/", import.meta.url).pathname;
const DOCS_RULES = join(DOCS, "AGENTS.md");

const BUDGETS = {
  html: {
    // its figures drawn for each platform: the size measured with them, and a fifth more
    "index.html": 114_800,
    "download.html": 30_000,
    "404.html": 30_000,
    "changelog.html": 80_000,
  },
  // the figures' live states: the size measured with them, and a fifth more
  css: 24_100,
  fonts: { files: 2, bytes: 90_000, preloads: 2 },
  // WhatsApp drops a larger preview image
  image: 300_000,
};
const IMAGE = { width: 1200, height: 630 };
// X's limit for twitter:image:alt
const ALT_LENGTH = 420;

// `key` is banned as an adjective only, which a word match cannot tell.
export function bannedFromDocs(markdown) {
  const section = markdown.split(/^## Banned\s*$/m)[1]?.split(/^## /m)[0];
  if (!section) throw new Error("docs/AGENTS.md has no Banned section");
  const out = [];
  for (const line of section.split(/\n(?=- )/)) {
    if (!/^- (Words|Phrases):/.test(line)) continue;
    for (const m of line.matchAll(/`([^`]+)`/g)) if (m[1] !== "key") out.push(m[1]);
  }
  if (out.length < 40) throw new Error("docs/AGENTS.md's Banned section reads short");
  return out;
}

export const SITE_BANNED = [
  "instant",
  "blazing",
  "magical",
  "supercharge",
  "elevate",
  "game-changing",
  "the only",
  "the first",
  "the best",
  "workflow",
  "experience",
  "so you can",
  "so you never",
  "making it",
  "not just",
  "not only",
  "introducing",
  "trusted by",
  "never hit",
  "never lose",
  "official app",
  "unlimited",
  "pool your",
  "one click away",
  "signed in forever",
  "works offline",
  "bank-grade",
];

const escape = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
export function wordPattern(words) {
  const parts = words.map((w) => {
    const head = /^\w/.test(w) ? "\\b" : "";
    const tail = /\w$/.test(w) ? "\\b" : "";
    return `${head}${escape(w).replace(/ /g, "\\s+")}${tail}`;
  });
  return new RegExp(parts.join("|"), "gi");
}

// Program output, the app's own strings and the changelog keep their own words.
const quoted = (el) =>
  ["code", "pre", "kbd", "script", "style"].includes(el.tag) ||
  "data-output" in el.attrs ||
  el.attrs["data-source"] === "changelog";

// CHANGELOG.md quotes a full-width c on purpose, in a look-alike claude.ai address.
const FALLBACK = new Set([0xff43]);

const MONO = (el) => ["code", "pre", "kbd"].includes(el.tag) || "data-output" in el.attrs;

export function checkPage(name, html, rules) {
  const problems = [];
  const fail = (m) => problems.push(`${name}: ${m}`);
  const root = parse(html);
  const all = elements(root);
  const meta = (key, value) => all.find((e) => e.tag === "meta" && e.attrs[key] === value);

  if (html.includes("astro-island")) fail("holds an astro-island");
  const h1 = all.filter((e) => e.tag === "h1");
  if (h1.length !== 1) fail(`has ${h1.length} h1`);
  const title = all.find((e) => e.tag === "title");
  if (!title?.children[0]?.value.trim()) fail("has no title");
  if (!meta("name", "description")?.attrs.content) fail("has no description");
  const noindex = meta("name", "robots")?.attrs.content?.includes("noindex");
  if (!noindex && !all.some((e) => e.tag === "link" && e.attrs.rel === "canonical"))
    fail("has no canonical");
  if (!meta("property", "og:image")?.attrs.content) fail("has no og:image");
  if (meta("property", "og:image:type")?.attrs.content !== "image/png")
    fail("has no og:image:type image/png");
  for (const side of ["width", "height"]) {
    if (meta("property", `og:image:${side}`)?.attrs.content !== String(IMAGE[side]))
      fail(`has no og:image:${side} ${IMAGE[side]}`);
  }
  const alt = meta("property", "og:image:alt")?.attrs.content;
  if (!alt) fail("has no og:image:alt");
  else if (alt.length > ALT_LENGTH) fail(`has an og:image:alt over ${ALT_LENGTH} characters`);
  const twitterAlt = meta("name", "twitter:image:alt")?.attrs.content;
  if (!twitterAlt) fail("has no twitter:image:alt");
  else if (twitterAlt !== alt) fail("has a twitter:image:alt other than its og:image:alt");
  if (meta("name", "twitter:card")?.attrs.content !== "summary_large_image")
    fail("has no twitter:card");
  if (!all.some((e) => e.tag === "main" && e.attrs.id === "main"))
    fail("has no main#main for the skip link");
  if (all.some((e) => "style" in e.attrs)) fail("has a style attribute");
  const preloads = all.filter(
    (e) => e.tag === "link" && e.attrs.rel === "preload" && e.attrs.as === "font",
  );
  if (preloads.length > BUDGETS.fonts.preloads) fail(`preloads ${preloads.length} fonts`);

  // head.js sets the root's choice before the first paint; site.ts sets aria state once it loads
  for (const e of all.filter((e) => e.tag === "button")) {
    const value = e.attrs["data-platform"] ?? e.attrs["data-surface"] ?? e.attrs["data-desktop"];
    if (!value) continue;
    const what = `the ${value} ${e.attrs.role === "tab" ? "tab" : "button"}`;
    const look = e.attrs.class ?? "";
    const aria = /(?:^|[\s:])aria-(selected|pressed):/.exec(look);
    if (aria) fail(`${what} looks chosen by aria-${aria[1]}, which only site.ts sets`);
    else if (!/(?:^|[\s:])chosen:/.test(look)) fail(`${what} has no chosen: look`);
  }

  // site.ts shows a [data-after] step once a link with its platform's data-download is clicked
  const panelOf = (e) => closest(e.parent, (p) => p.attrs.role === "tabpanel");
  for (const note of all.filter((e) => e.attrs["data-after"])) {
    const platform = note.attrs["data-after"];
    for (const a of all.filter((e) => e.tag === "a" && panelOf(e) === panelOf(note))) {
      const file = RELEASE_FILE.exec(a.attrs.href ?? "")?.[2];
      if (file && INSTALLER[platform]?.test(file) && a.attrs["data-download"] !== platform)
        fail(`${file} does not show the step its panel gives after a download`);
    }
  }

  const ids = new Set(all.map((e) => e.attrs.id).filter(Boolean));
  for (const e of all) {
    for (const key of ["aria-labelledby", "aria-controls", "aria-describedby"]) {
      for (const id of (e.attrs[key] ?? "").split(/\s+/).filter(Boolean)) {
        if (!ids.has(id)) fail(`${key} names #${id}, which is not on the page`);
      }
    }
  }

  const texts = [...walk(root)].filter(
    (n) => n.type === "text" && !closest(n.parent, (e) => ["script", "style"].includes(e.tag)),
  );
  const attrText = all.flatMap((e) =>
    ["alt", "aria-label", "title"].filter((a) => a in e.attrs).map((a) => e.attrs[a]),
  );
  for (const t of [
    ...texts.map((n) => n.value),
    ...attrText,
    meta("name", "description")?.attrs.content ?? "",
    alt ?? "",
  ]) {
    if (/[–—]/.test(t)) fail(`an en or em dash in "${t.trim().slice(0, 60)}"`);
  }

  for (const n of texts) {
    const inside = closest(n.parent, quoted);
    if (inside) continue;
    const t = n.value;
    for (const m of t.matchAll(rules.banned)) fail(`"${m[0]}" in "${t.trim().slice(0, 70)}"`);
    if (/[·●■◆▪•]/.test(t)) fail(`a dot or badge character in "${t.trim().slice(0, 70)}"`);
    if (/\bpitboard\b/.test(t))
      fail(`lower-case pitboard outside code in "${t.trim().slice(0, 70)}"`);
  }

  // outside mono contexts a character needs both faces: headings and labels are mono too
  for (const n of texts) {
    const mono = closest(n.parent, MONO);
    for (const ch of n.value) {
      const cp = ch.codePointAt(0);
      if (cp < 0x20 || ch === " ") continue;
      const ok =
        FALLBACK.has(cp) || (mono ? rules.mono.has(cp) : rules.mono.has(cp) && rules.text.has(cp));
      if (!ok)
        fail(`U+${cp.toString(16).toUpperCase().padStart(4, "0")} "${ch}" is in no committed face`);
    }
  }
  return { problems, root, all };
}

const idsOf = (page) => new Set(page.all.map((e) => e.attrs.id).filter(Boolean));

function checkLinks(pages) {
  const problems = [];
  const ids = new Map([...pages].map(([name, p]) => [name, idsOf(p)]));
  const resolve = (path) => {
    if (path === "/") return "index.html";
    const clean = path.replace(/^\//, "");
    if (existsSync(join(DIST, clean)) && statSync(join(DIST, clean)).isFile()) return clean;
    if (existsSync(join(DIST, `${clean}.html`))) return `${clean}.html`;
    return null;
  };
  for (const [name, p] of pages) {
    for (const e of p.all) {
      const href = e.tag === "a" || e.tag === "link" ? e.attrs.href : null;
      if (!href || /^(https?:|mailto:)/.test(href) || href.startsWith("//")) continue;
      const [path, frag] = href.split("#");
      const target = path ? resolve(path) : name;
      if (!target) {
        problems.push(`${name}: ${href} goes to no file`);
        continue;
      }
      if (frag && target.endsWith(".html") && !ids.get(target)?.has(decodeURIComponent(frag))) {
        problems.push(`${name}: ${href} names an anchor ${target} does not have`);
      }
    }
  }
  return problems;
}

// The docs link the site by its full address, which checkLinks passes over.
export function checkDocsToSite(pages, docs) {
  const problems = [];
  for (const [file, text] of docs) {
    for (const [url, path = "", id] of text.matchAll(
      /https:\/\/usepitboard\.com(\/[\w./-]*)?#([\w.-]*\w)/g,
    )) {
      const clean = path.replace(/^\/|\/$/g, "");
      const name = clean
        ? [`${clean}.html`, `${clean}/index.html`].find((n) => pages.has(n))
        : "index.html";
      if (!pages.has(name)) problems.push(`${file}: ${url} goes to no page`);
      else if (!idsOf(pages.get(name)).has(id))
        problems.push(`${file}: ${url} names an id ${name} does not have`);
    }
  }
  return problems;
}

// SignPath's terms for its open source certificate name both pages.
const SIGNING_POLICY = "https://docs.usepitboard.com/install/code-signing-policy";
export function checkSigningPolicy(pages) {
  const problems = [];
  for (const name of ["index.html", "download.html"]) {
    const linked = pages
      .get(name)
      ?.all.some(
        (e) =>
          e.tag === "a" &&
          e.attrs.href === SIGNING_POLICY &&
          textOf(e).trim() === "Code signing policy",
      );
    if (!linked) problems.push(`${name} has no Code signing policy link`);
  }
  return problems;
}

const RELEASE_FILE = /\/releases\/download\/v(\d+\.\d+\.\d+(?:-beta\.\d+)?)\/([^/?#]+)$/;
const INSTALLER = { macos: /-macos\.zip$/, windows: /-windows-(?:x64|arm64)\.msi$/ };
const WINDOWS_FILE =
  /-windows-(?:x64|arm64)\.msi(?:\.cdx\.json)?$|-pc-windows-msvc\.zip(?:\.cdx\.json)?$|^appcast-windows\.xml$/;
const LINUX_FILE = /-unknown-linux-musl\.tar\.gz(?:\.cdx\.json)?$|\.(?:deb|rpm)(?:\.cdx\.json)?$/;
// listed under more than one system
const SHARED_FILES = ["SHA256SUMS", "install.sh"];

// macOS files come from the newest full release on /changelog, Windows and Linux files from it or
// from a beta /changelog shows above it, and each shared file from a version one of them names.
export function checkVersions(pages) {
  const problems = [];
  const links = [...pages.values()].flatMap((p) =>
    p.all.flatMap((e) => {
      const m = RELEASE_FILE.exec(e.attrs.href ?? "");
      return m ? [{ version: m[1], file: m[2], text: textOf(e).trim() }] : [];
    }),
  );
  const versionsOf = (want) => [...new Set(links.filter(want).map((l) => l.version))];
  const early = [
    ["Windows", versionsOf((l) => WINDOWS_FILE.test(l.file))],
    ["Linux", versionsOf((l) => LINUX_FILE.test(l.file))],
  ];
  const macos = versionsOf(
    (l) => !SHARED_FILES.includes(l.file) && !WINDOWS_FILE.test(l.file) && !LINUX_FILE.test(l.file),
  );

  const articles = (pages.get("changelog.html")?.all ?? [])
    .filter((e) => e.tag === "article" && /^v\d+\.\d+\.\d+(?:-beta\.\d+)?$/.test(e.attrs.id ?? ""))
    .map((e) => e.attrs.id.slice(1));
  const at = articles.findIndex((v) => !v.includes("-"));
  const newest = articles[at] ?? macos[0];
  const above = articles.slice(0, Math.max(at, 0));

  if (macos.length !== 1)
    problems.push(`macOS downloads name ${macos.length} versions: ${macos.join(", ")}`);
  else if (macos[0] !== newest)
    problems.push(`/changelog starts at ${newest}, the macOS downloads at ${macos[0]}`);
  for (const [system, versions] of early) {
    if (versions.length > 1)
      problems.push(`${system} downloads name ${versions.length} versions: ${versions.join(", ")}`);
    else if (versions.length === 1 && versions[0] !== newest && !above.includes(versions[0]))
      problems.push(
        `${system} downloads name ${versions[0]}, neither ${newest} nor a beta /changelog shows above it`,
      );
  }
  const named = [macos, ...early.map(([, versions]) => versions)].flat();
  for (const file of SHARED_FILES) {
    for (const v of versionsOf((l) => l.file === file)) {
      if (!named.includes(v))
        problems.push(`${file} links name ${v}, which no other download names`);
    }
  }
  for (const { version, text } of links.filter((l) => l.text.startsWith("Download for"))) {
    const beta = version.includes("-");
    if (beta && !text.includes("(beta)"))
      problems.push(`"${text}" downloads ${version}, a beta, without saying (beta)`);
    if (!beta && text.includes("(beta)")) problems.push(`"${text}" says (beta) for ${version}`);
  }
  return problems;
}

// width, height and whether any pixel can be transparent, from the header and the chunks
// before the image data
export function pngFacts(bytes) {
  if (bytes.length < 33 || bytes.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a") return null;
  const colour = bytes[25];
  let alpha = colour === 4 || colour === 6;
  for (let at = 8; at + 8 <= bytes.length; at += 12 + bytes.readUInt32BE(at)) {
    const type = bytes.toString("latin1", at + 4, at + 8);
    if (type === "tRNS") alpha = true;
    if (type === "IDAT") break;
  }
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20), alpha };
}

// og:image is a file in dist named by its hash, so a changed image gets a new address
export function checkImage(name, all, read) {
  const content = all.find((e) => e.tag === "meta" && e.attrs.property === "og:image")?.attrs
    .content;
  if (!content) return [];
  const url = URL.parse(content);
  const m = /^\/og\/[a-z]+-([0-9a-f]{8})\.png$/.exec(url?.pathname ?? "");
  if (url?.origin !== "https://usepitboard.com" || !m)
    return [`${name}: og:image ${content} is not a hashed image under /og/`];
  const bytes = read(url.pathname.slice(1));
  if (!bytes) return [`${name}: og:image ${url.pathname} is not in dist`];
  const problems = [];
  const fail = (msg) => problems.push(`${name}: ${url.pathname} ${msg}`);
  const png = pngFacts(bytes);
  if (!png) fail("is not a PNG");
  else {
    if (png.width !== IMAGE.width || png.height !== IMAGE.height)
      fail(`is ${png.width}x${png.height}, not ${IMAGE.width}x${IMAGE.height}`);
    if (png.alpha) fail("can be transparent");
  }
  if (bytes.length > BUDGETS.image) fail(`is ${bytes.length} bytes, over ${BUDGETS.image}`);
  if (createHash("sha256").update(bytes).digest("hex").slice(0, 8) !== m[1])
    fail("is not named by its hash");
  return problems;
}

// /og.png, the one image before each card was named by its hash, serves the home page's card.
export function checkOldImage(home, read) {
  const content = home.find((e) => e.tag === "meta" && e.attrs.property === "og:image")?.attrs
    .content;
  const card = content ? read(URL.parse(content)?.pathname.slice(1) ?? "") : null;
  const old = read("og.png");
  if (!old) return ["dist has no og.png"];
  return card && old.equals(card) ? [] : ["og.png is not the home page's card"];
}

function gz(file) {
  return gzipSync(readFileSync(file)).length;
}

function readDist(path) {
  const file = join(DIST, path);
  return existsSync(file) && statSync(file).isFile() ? readFileSync(file) : null;
}

function main() {
  const problems = [];
  const files = readdirSync(DIST, { recursive: true }).map(String);

  if (files.some((f) => f.split("/").pop() === "install.sh"))
    problems.push("dist holds an install.sh");
  for (const f of ["_redirects", "_headers"]) {
    const p = join(DIST, f);
    if (!existsSync(p)) continue;
    const text = readFileSync(p, "utf8");
    if (text.includes("install.sh"))
      problems.push(`${f} names install.sh, which a zone rule serves`);
    if (f === "_redirects" && /^\/\*\s/m.test(text))
      problems.push("_redirects sends every path somewhere");
  }
  if (!existsSync(join(DIST, "_headers"))) problems.push("dist has no _headers: run csp.mjs first");
  const wrangler = readFileSync(new URL("../wrangler.jsonc", import.meta.url), "utf8");
  for (const key of ["single-page-application", '"routes"', '"route"', '"custom_domain"']) {
    if (wrangler.includes(key)) problems.push(`wrangler.jsonc names ${key}`);
  }
  if (!wrangler.includes('"previews"')) problems.push("wrangler.jsonc has no previews block");
  if (!existsSync(join(DIST, "404.html"))) problems.push("dist has no 404.html");
  const sitemaps = files.filter((f) => /^sitemap.*\.xml$/.test(f));
  if (!sitemaps.length) problems.push("dist has no sitemap");
  for (const s of sitemaps) {
    if (/404/.test(readFileSync(join(DIST, s), "utf8"))) problems.push(`${s} lists the 404 page`);
  }

  const astro = files.filter((f) => f.startsWith("_astro/"));
  const sum = (ext) =>
    astro.filter((f) => f.endsWith(ext)).reduce((n, f) => n + gz(join(DIST, f)), 0);
  const css = sum(".css");
  const js = sum(".js");
  const fonts = astro.filter((f) => f.endsWith(".woff2"));
  const fontBytes = fonts.reduce((n, f) => n + statSync(join(DIST, f)).size, 0);
  if (css > BUDGETS.css) problems.push(`CSS is ${css} bytes gzipped, over ${BUDGETS.css}`);
  if (fonts.length > BUDGETS.fonts.files) problems.push(`${fonts.length} font files`);
  if (fontBytes > BUDGETS.fonts.bytes)
    problems.push(`fonts are ${fontBytes} bytes, over ${BUDGETS.fonts.bytes}`);

  const face = (name) =>
    new Set(JSON.parse(readFileSync(join(FONTS, `${name}.codepoints.json`), "utf8")));
  const rules = {
    banned: wordPattern([...bannedFromDocs(readFileSync(DOCS_RULES, "utf8")), ...SITE_BANNED]),
    text: face("Inter-4.1-opsz-wght"),
    mono: face("JetBrainsMono-2.304-wght"),
  };

  const pages = new Map();
  const report = [];
  for (const f of files.filter((f) => f.endsWith(".html"))) {
    const html = readFileSync(join(DIST, f), "utf8");
    const size = gzipSync(html).length;
    const budget = BUDGETS.html[f];
    if (budget === undefined) problems.push(`${f} has no HTML budget`);
    else if (size > budget) problems.push(`${f} is ${size} bytes gzipped, over ${budget}`);
    const page = checkPage(f, html, rules);
    problems.push(...page.problems, ...checkImage(f, page.all, readDist));
    pages.set(f, page);
    report.push(`${f} ${size}`);
  }
  const docs = readdirSync(DOCS, { recursive: true })
    .map(String)
    .filter((f) => f.endsWith(".mdx") || f === "docs.json")
    .map((f) => [`docs/${f}`, readFileSync(join(DOCS, f), "utf8")]);
  problems.push(
    ...checkLinks(pages),
    ...checkVersions(pages),
    ...checkSigningPolicy(pages),
    ...checkDocsToSite(pages, docs),
    ...checkOldImage(pages.get("index.html")?.all ?? [], readDist),
  );

  console.log(
    `guard: html gzipped ${report.join(", ")}; css ${css}; js ${js}; fonts ${fontBytes} in ${fonts.length}`,
  );
  if (problems.length) {
    console.error(problems.map((p) => `guard: ${p}`).join("\n"));
    process.exit(1);
  }
}

if (import.meta.url === `file://${process.argv[1]}`) main();
