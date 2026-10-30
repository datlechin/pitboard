import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { headers, policy } from "./csp.mjs";
import {
  SITE_BANNED,
  bannedFromDocs,
  checkDocsToSite,
  checkImage,
  checkOldImage,
  checkPage,
  checkSigningPolicy,
  checkVersions,
  pngFacts,
  wordPattern,
} from "./guard.mjs";
import { elements, parse, textOf } from "./html.mjs";

const docs = readFileSync(new URL("../../docs/AGENTS.md", import.meta.url), "utf8");
const ascii = new Set(Array.from({ length: 0x7f - 0x20 }, (_, i) => i + 0x20));
const rules = {
  banned: wordPattern([...bannedFromDocs(docs), ...SITE_BANNED]),
  text: new Set([...ascii, 0x2019]),
  mono: new Set([...ascii, 0xb7, 0x2019, 0x2588]),
};

const image = (alt = "The logo.", twitterAlt = alt) =>
  `<meta property="og:image" content="https://usepitboard.com/og/home-0123abcd.png"><meta property="og:image:type" content="image/png">
<meta property="og:image:width" content="1200"><meta property="og:image:height" content="630"><meta property="og:image:alt" content="${alt}">
<meta name="twitter:card" content="summary_large_image"><meta name="twitter:image:alt" content="${twitterAlt}">`;
const page = (
  body,
  head = "",
  tags = image(),
) => `<!doctype html><html lang="en"><head><title>Pitboard</title>
<meta name="description" content="A page."><link rel="canonical" href="https://usepitboard.com/">
${tags}${head}</head><body><main id="main"><h1>Pitboard</h1>${body}</main></body></html>`;

const problems = (body, head) => checkPage("p.html", page(body, head), rules).problems;

test("reads the Banned words from docs/AGENTS.md", () => {
  const words = bannedFromDocs(docs);
  for (const w of [
    "simply",
    "seamless",
    "leverage",
    "e.g.",
    "click on",
    "Welcome to",
    "Note that",
  ]) {
    assert.ok(words.includes(w), w);
  }
  assert.ok(!words.includes("key"));
});

test("passes a page that keeps the rules", () => {
  assert.deepEqual(
    problems("<p>Pitboard switches accounts. Run <code>pitboard use work</code>.</p>"),
    [],
  );
});

test("finds banned words, dashes, dots and a lower-case name in the site's own words", () => {
  const cases = [
    ["<p>It is seamless.</p>", /"seamless"/],
    ["<p>Simply run it.</p>", /"Simply"/],
    ["<p>Switch, e.g. to work.</p>", /"e\.g\."/],
    ["<p>Never hit a limit again.</p>", /"Never hit"/],
    ["<p>Fast — and small.</p>", /en or em dash/],
    ["<p>Open source · free</p>", /dot or badge/],
    ["<p>Install pitboard first.</p>", /lower-case pitboard/],
    ["<p>Plain █ block</p>", /U\+2588/],
    ["<h1>Two</h1>", /has 2 h1/],
    ['<p style="color:red">Red</p>', /style attribute/],
    ['<button aria-controls="nope">Go</button>', /#nope/],
  ];
  for (const [body, why] of cases) assert.match(problems(body).join("\n"), why, body);
});

test("leaves program output and the changelog their own words", () => {
  assert.deepEqual(problems("<pre data-output>now █ · just</pre>"), []);
  assert.deepEqual(
    problems("<div data-output><span>Enrolled personal, the account signed in now.</span></div>"),
    [],
  );
  assert.deepEqual(
    problems('<div data-source="changelog"><p>It now renews the login.</p></div>'),
    [],
  );
  // a dash is wrong everywhere, the changelog included
  assert.match(problems('<div data-source="changelog"><p>a – b</p></div>').join(), /dash/);
});

test("needs the metadata every page carries", () => {
  const bare = "<!doctype html><html><head></head><body><h1>X</h1></body></html>";
  const found = checkPage("bare.html", bare, rules).problems.join("\n");
  for (const what of [
    "no title",
    "no description",
    "no canonical",
    "no og:image",
    "no og:image:type image/png",
    "no og:image:width 1200",
    "no og:image:height 630",
    "no og:image:alt",
    "no twitter:image:alt",
    "no twitter:card",
    "main#main",
  ]) {
    assert.match(found, new RegExp(what.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  }
});

test("needs an image description X takes, the same for X as for the others", () => {
  const tags = (alt, twitterAlt) =>
    checkPage("p.html", page("", "", image(alt, twitterAlt)), rules);
  assert.deepEqual(tags("The logo.").problems, []);
  assert.match(tags("x".repeat(421)).problems.join(), /over 420 characters/);
  assert.match(tags("The logo.", "Another.").problems.join(), /twitter:image:alt other than/);
  assert.match(tags("The logo \u2014 big.").problems.join(), /en or em dash/);
});

test("needs each platform tab and surface or desktop button to look chosen by the root's choice", () => {
  const tab = (look) =>
    `<button class="text-muted ${look}" role="tab" aria-selected="true" data-platform="macos">macOS</button>`;
  const pick = (look) =>
    `<button class="text-fg ${look}" type="button" aria-pressed="true" data-surface="app">App</button>`;
  const desktop = (look) =>
    `<button class="text-fg ${look}" type="button" aria-pressed="true" data-desktop="gnome">GNOME</button>`;
  assert.deepEqual(
    problems(
      tab("chosen:text-ink chosen:after:bg-ink") + pick("chosen:bg-ink") + desktop("chosen:bg-ink"),
    ),
    [],
  );
  assert.match(problems(desktop("")).join(), /the gnome button has no chosen: look/);
  assert.match(
    problems(desktop("aria-pressed:bg-ink")).join(),
    /the gnome button looks chosen by aria-pressed, which only site\.ts sets/,
  );
  assert.match(
    problems(tab("aria-selected:text-ink")).join(),
    /the macos tab looks chosen by aria-selected, which only site\.ts sets/,
  );
  assert.match(
    problems(pick("aria-pressed:bg-ink")).join(),
    /the app button looks chosen by aria-pressed, which only site\.ts sets/,
  );
  assert.match(problems(pick("")).join(), /the app button has no chosen: look/);
});

test("needs each installer link beside a step shown after a download to show that step", () => {
  const installer = (arch, after = ' data-download="windows"') =>
    `<a href="https://github.com/datlechin/pitboard/releases/download/v1.0.0-beta.2/Pitboard-v1.0.0-beta.2-windows-${arch}.msi"${after}>${arch}</a>`;
  const panel = (links) =>
    `<div role="tabpanel">${links}<p data-after="windows">Select <b>Run anyway</b>.</p></div>`;
  assert.deepEqual(problems(panel(installer("x64") + installer("arm64"))), []);
  assert.match(
    problems(panel(installer("x64") + installer("arm64", ""))).join(),
    /Pitboard-v1\.0\.0-beta\.2-windows-arm64\.msi does not show the step its panel gives after a download/,
  );
  // a list of the release's files sits outside every panel
  assert.deepEqual(problems(panel(installer("x64")) + installer("arm64", "")), []);
});

// the signature and the header of a PNG, then the chunks given, each with a dummy CRC
function png(width, height, colour, ...chunks) {
  const chunk = (type, data) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    return Buffer.concat([len, Buffer.from(type, "latin1"), data, Buffer.alloc(4)]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = colour;
  return Buffer.concat([
    Buffer.from("89504e470d0a1a0a", "hex"),
    chunk("IHDR", ihdr),
    ...chunks.map(([type, data]) => chunk(type, data)),
    chunk("IDAT", Buffer.alloc(16)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

test("reads a PNG's size and whether it can be transparent", () => {
  assert.deepEqual(pngFacts(png(1200, 630, 2)), { width: 1200, height: 630, alpha: false });
  assert.equal(pngFacts(png(1200, 630, 6)).alpha, true);
  assert.equal(pngFacts(png(1200, 630, 3, ["PLTE", Buffer.alloc(3)])).alpha, false);
  assert.equal(
    pngFacts(png(1200, 630, 3, ["PLTE", Buffer.alloc(3)], ["tRNS", Buffer.alloc(1)])).alpha,
    true,
  );
  assert.equal(pngFacts(Buffer.from("GIF89a")), null);
});

test("checks the preview image each page names", () => {
  const hashed = (bytes) => createHash("sha256").update(bytes).digest("hex").slice(0, 8);
  const check = (bytes, name = `home-${hashed(bytes)}`, origin = "https://usepitboard.com") => {
    const html = page("", "").replace(
      "https://usepitboard.com/og/home-0123abcd.png",
      `${origin}/og/${name}.png`,
    );
    const { all } = checkPage("p.html", html, rules);
    return checkImage("p.html", all, (path) => (path === `og/${name}.png` ? bytes : null)).join(
      "\n",
    );
  };
  assert.equal(check(png(1200, 630, 2)), "");
  assert.match(check(png(1200, 600, 2)), /is 1200x600, not 1200x630/);
  assert.match(check(png(1200, 630, 6)), /can be transparent/);
  assert.match(check(png(1200, 630, 2, ["zTXt", Buffer.alloc(300_000)])), /over 300000/);
  assert.match(check(png(1200, 630, 2), "home-0123abcd"), /not named by its hash/);
  assert.match(check(Buffer.from("not a png")), /is not a PNG/);
  assert.match(check(png(1200, 630, 2), "home"), /not a hashed image under \/og\//);
  assert.match(check(png(1200, 630, 2), undefined, "https://example.com"), /not a hashed image/);
  const { all } = checkPage("p.html", page(""), rules);
  assert.match(checkImage("p.html", all, () => null).join(), /is not in dist/);
});

test("serves the home page's card at /og.png", () => {
  const { all } = checkPage("index.html", page(""), rules);
  const card = png(1200, 630, 2);
  const read = (old) => (path) => ({ "og/home-0123abcd.png": card, "og.png": old })[path] ?? null;
  assert.deepEqual(checkOldImage(all, read(card)), []);
  assert.deepEqual(checkOldImage(all, read(png(1200, 630, 6))), [
    "og.png is not the home page's card",
  ]);
  assert.deepEqual(checkOldImage(all, read(null)), ["dist has no og.png"]);
});

const download = (v, file, text = file) =>
  `<a href="https://github.com/datlechin/pitboard/releases/download/v${v}/${file}">${text}</a>`;
const mac = (v, text = "Download for Mac") => download(v, `Pitboard-v${v}-macos.zip`, text);
const msi = (v, text) => download(v, `Pitboard-v${v}-windows-x64.msi`, text);
const site = (body, ...versions) =>
  new Map([
    ["download.html", checkPage("download.html", page(body), rules)],
    [
      "changelog.html",
      checkPage(
        "changelog.html",
        page(versions.map((v) => `<article id="v${v}"><h2>${v}</h2></article>`).join("")),
        rules,
      ),
    ],
  ]);

test("compares the macOS and Linux downloads with the newest full release on /changelog", () => {
  const linux = (v) => download(v, `pitboard-v${v}-x86_64-unknown-linux-musl.tar.gz`);
  assert.deepEqual(
    checkVersions(site(mac("0.15.0") + linux("0.15.0"), "1.0.0-beta.2", "1.0.0-beta.1", "0.15.0")),
    [],
  );
  assert.equal(checkVersions(site(mac("0.15.0"), "1.0.0-beta.1", "0.14.0")).length, 1);
  assert.equal(checkVersions(site(mac("0.15.0"), "0.16.0", "0.15.0")).length, 1);
  assert.match(
    checkVersions(site(mac("0.15.0") + linux("1.0.0-beta.1"), "1.0.0-beta.1", "0.15.0")).join(),
    /macOS and Linux downloads name 2 versions/,
  );
});

test("takes the Windows downloads from that release, or from a beta /changelog shows above it", () => {
  const beta = "1.0.0-beta.1";
  const files =
    msi(beta, "Download for Windows (beta)") +
    download(beta, `Pitboard-v${beta}-windows-arm64.msi`) +
    download(beta, `Pitboard-v${beta}-windows-arm64.msi.cdx.json`, ".cdx.json") +
    download(beta, `pitboard-v${beta}-aarch64-pc-windows-msvc.zip`) +
    download(beta, "SHA256SUMS") +
    download("0.15.0", "SHA256SUMS");
  assert.deepEqual(checkVersions(site(mac("0.15.0") + files, beta, "0.15.0")), []);
  const full =
    mac("1.0.0") + msi("1.0.0", "Download for Windows") + download("1.0.0", "appcast-windows.xml");
  assert.deepEqual(checkVersions(site(full, "1.1.0-beta.1", "1.0.0", beta)), []);
  const why = (body, ...versions) => checkVersions(site(body, ...versions)).join("\n");
  assert.match(
    why(mac("0.15.0") + files, "0.15.0"),
    /Windows downloads name 1\.0\.0-beta\.1, neither 0\.15\.0 nor a beta \/changelog shows above it/,
  );
  assert.match(
    why(mac("1.0.0") + msi(beta, "Download for Windows (beta)"), "1.0.0", beta),
    /Windows downloads name 1\.0\.0-beta\.1, neither 1\.0\.0/,
  );
  assert.match(
    why(mac("0.15.0") + msi(beta) + msi("1.0.0-beta.2"), "1.0.0-beta.2", beta, "0.15.0"),
    /Windows downloads name 2 versions: 1\.0\.0-beta\.1, 1\.0\.0-beta\.2/,
  );
  assert.match(
    why(mac("0.15.0") + msi(beta) + download("0.14.0", "SHA256SUMS"), beta, "0.15.0"),
    /SHA256SUMS links name 0\.14\.0, which no other download names/,
  );
});

test("needs a download button to say (beta) exactly when it downloads a beta", () => {
  const beta = "1.0.0-beta.1";
  const why = (body) => checkVersions(site(mac("0.15.0") + body, beta, "0.15.0")).join("\n");
  assert.equal(why(msi(beta, "Download for Windows (beta)")), "");
  assert.match(
    why(msi(beta, "Download for Windows")),
    /"Download for Windows" downloads 1\.0\.0-beta\.1, a beta, without saying \(beta\)/,
  );
  assert.match(
    checkVersions(site(mac("1.0.0", "Download for Mac (beta)"), "1.0.0")).join(),
    /"Download for Mac \(beta\)" says \(beta\) for 1\.0\.0/,
  );
});

test("finds two releases on one site", () => {
  const one = (v) =>
    checkPage(
      "x.html",
      page(
        `<a href="https://github.com/datlechin/pitboard/releases/download/v${v}/Pitboard-v${v}-macos.zip">Mac</a>`,
      ),
      rules,
    );
  assert.deepEqual(
    checkVersions(
      new Map([
        ["a.html", one("0.12.0")],
        ["b.html", one("0.12.0")],
      ]),
    ),
    [],
  );
  assert.equal(
    checkVersions(
      new Map([
        ["a.html", one("0.12.0")],
        ["b.html", one("0.11.0")],
      ]),
    ).length,
    1,
  );
});

test("needs the code signing policy on the home page and the download page", () => {
  const policy = `<a href="https://docs.usepitboard.com/install/code-signing-policy">Code signing policy</a>`;
  const pages = (home, download) =>
    new Map([
      ["index.html", checkPage("index.html", page(home), rules)],
      ["download.html", checkPage("download.html", page(download), rules)],
    ]);
  assert.deepEqual(checkSigningPolicy(pages(policy, policy)), []);
  assert.deepEqual(checkSigningPolicy(pages(policy, "")), [
    "download.html has no Code signing policy link",
  ]);
  assert.equal(
    checkSigningPolicy(pages(policy.replace("Code signing policy", "Signing"), policy)).length,
    1,
  );
});

test("needs each site address a docs page links with an id to name one the site has", () => {
  const pages = new Map([
    ["index.html", checkPage("index.html", page('<section id="faq"></section>'), rules)],
    [
      "download.html",
      checkPage("download.html", page('<section id="release-files"></section>'), rules),
    ],
  ]);
  const docs = (text) => checkDocsToSite(pages, [["install.mdx", text]]);
  assert.deepEqual(
    docs(
      "[Files](https://usepitboard.com/download#release-files), [FAQ](https://usepitboard.com/#faq) and `https://usepitboard.com/install.sh`.",
    ),
    [],
  );
  assert.deepEqual(docs("[Other ways](https://usepitboard.com/download#other-ways)."), [
    "install.mdx: https://usepitboard.com/download#other-ways names an id download.html does not have",
  ]);
  assert.deepEqual(docs("[Files](https://usepitboard.com/downloads#release-files)"), [
    "install.mdx: https://usepitboard.com/downloads#release-files goes to no page",
  ]);
  assert.deepEqual(docs("[Install](https://docs.usepitboard.com/install#what-you-need)"), []);
});

test("hashes inline scripts byte for byte and refuses foreign scripts", () => {
  const head = 'console.log("theme");\n';
  const html = page(
    "<p>Hi</p>",
    `<script>${head}</script><script type="application/ld+json">{"a":1}</script><script type="module" src="/_astro/site.js"></script>`,
  );
  const { csp, problems: none } = policy([["index.html", html]]);
  const hash = createHash("sha256").update(head).digest("base64");
  assert.deepEqual(none, []);
  assert.match(
    csp,
    new RegExp(`script-src 'self' 'sha256-${hash.replace(/[+/=]/g, "\\$&")}'; style-src 'self';`),
  );
  assert.doesNotMatch(csp, /cloudflareinsights/);
  const beacon = policy([
    [
      "i.html",
      page("", '<script defer src="https://static.cloudflareinsights.com/beacon.min.js"></script>'),
    ],
  ]);
  assert.match(beacon.csp, /script-src 'self' https:\/\/static\.cloudflareinsights\.com;/);
  assert.match(beacon.csp, /connect-src 'self' https:\/\/cloudflareinsights\.com;/);
  assert.match(
    policy([
      ["e.html", page("", '<script src="https://cdn.example/x.js"></script>')],
    ]).problems.join(),
    /a script from/,
  );
  assert.match(headers("default-src 'self'", false), /^\s+Content-Security-Policy-Report-Only: /m);
  assert.match(headers("default-src 'self'", true), /^\s+Content-Security-Policy: /m);
});

test("reads the HTML Astro writes", () => {
  const root = parse(
    '<p class="a">x &amp; <b>y</b><br>z<svg><use href="#i"/></svg></p><script>if (a < b) {}</script>',
  );
  const [p] = elements(root, "p");
  assert.equal(p.attrs.class, "a");
  assert.equal(textOf(p), "x & yz");
  assert.equal(elements(root, "use").length, 1);
  assert.equal(elements(root, "script")[0].children[0].raw, "if (a < b) {}");
});
