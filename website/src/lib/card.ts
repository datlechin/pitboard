// The link preview images: the hero's menu for the home page, a title card for the others.

import satori from "satori";
import sharp from "sharp";
import { bar, heavy } from "../data/glyphs.ts";
import { TAGLINE } from "../data/site.ts";
import { clock, moments, tools, type Tool } from "../data/world.ts";
import { hm, isInUse, listed, menuBarTitle, menuLine } from "./words.ts";

export interface Size {
  width: number;
  height: number;
}
export const OPEN_GRAPH: Size = { width: 1200, height: 630 };
// GitHub shows a repository's social preview at 2:1 and crops any other shape.
export const GITHUB: Size = { width: 1280, height: 640 };
// Laid out on OPEN_GRAPH. Words stay between these: Messages on a phone shows about the middle
// 900 pixels.
const LEFT = 176;
const RIGHT = 1040;
const LOGO_TOP = 96;
const LOGO_HEIGHT = 56;
const CONTENT_TOP = 246;
// the headline's last baseline on the menu's bottom edge
const HEADLINE_TOP = 272;

// A macOS point is U pixels, about 1.5 times the hero's largest, so a 13-point menu row is 24
// pixels tall: 8 when the image is shown 400 pixels wide.
const U = 1.85;
const pt = (n: number) => n * U;
const MENU: Tool[] = ["claude", "codex"];

export type Card = { kind: "home" } | { kind: "page"; label: string; title: string };

export const cards = {
  home: { kind: "home" },
  changelog: {
    kind: "page",
    label: "Changelog",
    title: "What each release added, changed and fixed",
  },
  download: { kind: "page", label: "Download", title: "Pitboard for macOS, Windows and Linux" },
} satisfies Record<string, Card>;

export interface Assets {
  css: string;
  lockup: string;
  template: string;
  fonts: { name: string; data: Buffer; weight: 400 | 500 | 700 }[];
}

type Colors = Record<string, string>;
type El = { type: string; props: Record<string, unknown> };
type Style = Record<string, string | number>;

const h = (type: string, style: Style, ...children: (El | string)[]): El => ({
  type,
  props: { style: { display: "flex", ...style }, children },
});
const img = (src: string, width: number, height: number, style: Style = {}): El => ({
  type: "img",
  props: { src, width, height, style },
});
const svg = (viewBox: string, body: string, color = "#000") =>
  `data:image/svg+xml;base64,${Buffer.from(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${viewBox}">${body.replaceAll("currentColor", color)}</svg>`,
  ).toString("base64")}`;
const nameOf = (tool: Tool) => tools.find(([t]) => t === tool)![1];

// the dark half of each light-dark() colour in global.css
export function darkColors(css: string): Colors {
  const out: Colors = {};
  for (const [, name, value] of css.matchAll(/--color-([\w-]+):\s*([^;]+);/g)) {
    const pair = /^light-dark\((.*)\)$/.exec(value.trim());
    if (!pair) {
      out[name] = value.trim();
      continue;
    }
    let depth = 0;
    const comma = [...pair[1]].findIndex((ch) => {
      depth += ch === "(" ? 1 : ch === ")" ? -1 : 0;
      return ch === "," && depth === 0;
    });
    out[name] = pair[1].slice(comma + 1).trim();
  }
  return out;
}

function paths(text: string, count: number, file: string) {
  const viewBox = /viewBox="([^"]+)"/.exec(text)?.[1];
  const ds = [...text.matchAll(/<path\b[^>]*?\sd="([^"]+)"/g)].map((m) => m[1]);
  if (!viewBox || ds.length !== count) throw new Error(`${file} is not the kit's ${count} paths`);
  return { viewBox, ds };
}

function lockup(assets: Assets, c: Colors): El {
  const { viewBox, ds } = paths(assets.lockup, 3, "lockup-color.svg");
  const [board, lit, word] = ds;
  const [, , w, vh] = viewBox.split(/\s+/).map(Number);
  const body = `<path fill="${c["logo-word"]}" d="${board}"/><path fill="${c["logo-lit"]}" d="${lit}"/><path fill="${c["logo-word"]}" d="${word}"/>`;
  return img(svg(viewBox, body), Math.round((LOGO_HEIGHT * w) / vh), LOGO_HEIGHT);
}

// the desk's wall: a gradient under rings 40 points apart, centred off the lower left
function wall(c: Colors, { width, height }: Size): El {
  const cx = -0.1 * width;
  const cy = 1.35 * height;
  const rings = [];
  for (let r = pt(39.5); r < Math.hypot(width - cx, cy); r += pt(40)) {
    rings.push(`<circle cx="${cx}" cy="${cy}" r="${r}"/>`);
  }
  const body = `<defs><linearGradient id="w" x2="0" y2="1"><stop stop-color="${c["wall-a"]}"/><stop offset="1" stop-color="${c["wall-b"]}"/></linearGradient></defs><rect width="${width}" height="${height}" fill="url(#w)"/><g fill="none" stroke="${c["wall-line"]}" stroke-width="${pt(1)}">${rings.join("")}</g>`;
  return img(svg(`0 0 ${width} ${height}`, body), width, height, {
    position: "absolute",
    left: 0,
    top: 0,
  });
}

// The menu bar holds the top edge, so a wider card centres the words and a taller one grows down.
const frame = (assets: Assets, c: Colors, size: Size, content: El[], over: El[] = []) =>
  h(
    "div",
    { position: "relative", width: size.width, height: size.height, fontFamily: "Inter" },
    wall(c, size),
    h(
      "div",
      {
        position: "absolute",
        left: LEFT + (size.width - OPEN_GRAPH.width) / 2,
        top: LOGO_TOP,
        width: RIGHT - LEFT,
        height: size.height - LOGO_TOP,
        flexDirection: "column",
      },
      lockup(assets, c),
      ...content,
    ),
    ...over,
    // the desk's 1-pixel edge, which keeps the card apart from a dark page
    h("div", {
      position: "absolute",
      left: 0,
      top: 0,
      width: size.width,
      height: size.height,
      border: `1px solid ${c["desk-edge"]}`,
    }),
  );

const mono = (weight: number, size: number, color: string, more: Style = {}): Style => ({
  fontFamily: "JetBrains Mono",
  fontWeight: weight,
  fontSize: size,
  color,
  ...more,
});

// Desk.astro's menu, its Claude Code and Codex sections after the switch to work
function menu(c: Colors): El {
  const check = img(
    svg(
      "0 0 20 20",
      `<path d="${heavy["sf-check-heavy"].d}" fill="none" stroke="currentColor" stroke-width="${heavy["sf-check-heavy"].width}" stroke-linecap="round" stroke-linejoin="round"/>`,
      c["mac-menu-text"],
    ),
    pt(14),
    pt(14),
    { position: "absolute", left: pt(7), top: "50%", marginTop: pt(-6) },
  );
  const sep = h("div", {
    height: 1,
    margin: `${pt(5.5)}px ${pt(16)}px ${pt(4.5)}px ${pt(30)}px`,
    backgroundColor: c["mac-menu-sep"],
  });
  const sections = MENU.flatMap((tool, i) => [
    ...(i > 0 ? [sep] : []),
    h(
      "div",
      {
        padding: `${pt(5.5)}px ${pt(18)}px ${pt(3)}px ${pt(30)}px`,
        fontSize: pt(11),
        fontWeight: 700,
        color: c["mac-menu-dim"],
      },
      nameOf(tool),
    ),
    ...listed(moments.after, tool).map((a) =>
      h(
        "div",
        {
          position: "relative",
          flexDirection: "column",
          margin: `0 ${pt(5)}px`,
          padding: `${pt(3)}px ${pt(13)}px ${pt(4)}px ${pt(25)}px`,
        },
        ...(isInUse(a, moments.after) ? [check] : []),
        h("div", {}, a.label),
        h(
          "div",
          { fontSize: pt(11), lineHeight: `${pt(15)}px`, color: c["mac-menu-sub"] },
          menuLine(a, moments.after),
        ),
      ),
    ),
  ]);
  return h(
    "div",
    {
      position: "absolute",
      top: pt(26),
      left: 0,
      flexDirection: "column",
      padding: `${pt(4)}px 0 ${pt(6)}px`,
      borderRadius: pt(11),
      backgroundColor: c["mac-menu-bg"],
      boxShadow: [
        `0 0 0 ${pt(0.5)}px ${c["mac-menu-edge"]}`,
        `inset 0 0 0 ${pt(0.5)}px ${c["mac-menu-rim"]}`,
        `0 ${pt(12)}px ${pt(36)}px ${pt(-4)}px ${c["mac-menu-shadow"]}`,
        `0 ${pt(2)}px ${pt(8)}px ${pt(-2)}px ${c["mac-menu-shadow"]}`,
      ].join(", "),
      color: c["mac-menu-text"],
    },
    ...sections,
  );
}

// MenuBar.astro, with the clock's time alone so the menu clears the headline
function menuBar(assets: Assets, c: Colors, width: number): El {
  const color = c["mac-bar-text"];
  const item = (children: El[], style: Style = {}) =>
    h(
      "div",
      {
        alignItems: "center",
        height: pt(22),
        padding: `0 ${pt(7)}px`,
        borderRadius: pt(6),
        ...style,
      },
      ...children,
    );
  const glyph = (id: keyof typeof bar, w: number, ht: number) =>
    item([img(svg(bar[id].viewBox, bar[id].body, color), pt(w), pt(ht))]);
  const { viewBox, ds } = paths(assets.template, 2, "template.svg");
  const dim = /fill-opacity="([^"]+)"/.exec(assets.template)?.[1] ?? "0.35";
  const icon = svg(
    viewBox,
    `<path fill="${color}" d="${ds[0]}"/><path fill="${color}" fill-opacity="${dim}" d="${ds[1]}"/>`,
  );
  return h(
    "div",
    {
      position: "absolute",
      left: 0,
      top: 0,
      width,
      height: pt(24),
      alignItems: "center",
      justifyContent: "flex-end",
      paddingRight: pt(6),
      backgroundColor: c["mac-bar-bg"],
      boxShadow: `inset 0 ${pt(-0.5)}px 0 ${c["mac-bar-edge"]}`,
      color,
      fontSize: pt(13),
      lineHeight: `${pt(16)}px`,
      whiteSpace: "nowrap",
    },
    item(
      [
        img(icon, pt(16), pt(16), { marginRight: pt(2.25) }),
        h("div", {}, menuBarTitle(moments.after)),
        menu(c),
      ],
      { position: "relative", backgroundColor: c["mac-bar-hl"] },
    ),
    glyph("ic-batt", 25, 12),
    glyph("ic-wifi", 16, 12),
    glyph("ic-cc", 16, 14),
    item([h("div", {}, hm(clock.menuBar))], { paddingLeft: pt(9) }),
  );
}

function tree(card: Card, assets: Assets, c: Colors, size: Size): El {
  if (card.kind === "page") {
    return frame(assets, c, size, [
      h(
        "div",
        mono(500, 24, c["muted"], {
          marginTop: CONTENT_TOP - LOGO_TOP - LOGO_HEIGHT,
          letterSpacing: 1.2,
        }),
        card.label.toUpperCase(),
      ),
      h(
        "div",
        mono(700, 60, c["strong"], {
          marginTop: 24,
          lineHeight: "68px",
          letterSpacing: -1.2,
          textWrap: "balance",
        }),
        card.title,
      ),
    ]);
  }
  return frame(
    assets,
    c,
    size,
    [
      h(
        "div",
        mono(700, 56, c["strong"], {
          marginTop: HEADLINE_TOP - LOGO_TOP - LOGO_HEIGHT,
          // 16 characters a line, clear of the menu
          width: 540,
          lineHeight: "64px",
          letterSpacing: -1.12,
        }),
        TAGLINE,
      ),
    ],
    [menuBar(assets, c, size.width)],
  );
}

// og:image:alt, from what the image draws
export function describe(card: Card): string {
  if (card.kind === "page") {
    return `Pitboard's logo above the label ${card.label} and the line: ${card.title}.`;
  }
  const sections = MENU.map((tool) => {
    const rows = listed(moments.after, tool).map((a) =>
      isInUse(a, moments.after) ? `${a.label}, in use,` : a.label,
    );
    return `${nameOf(tool)} has ${rows.join(" and ")}`;
  });
  return `Pitboard's logo above the line: ${TAGLINE} On the right, Pitboard's menu open below its menu bar item, ${menuBarTitle(moments.after)}. ${sections.join("; ")}; each account shows its plan and how much of each limit it has used.`;
}

export function layout(card: Card, assets: Assets, o: { embedFont?: boolean; size?: Size } = {}) {
  const size = o.size ?? OPEN_GRAPH;
  // satori types its input as a React element, which this tree has the shape of
  return satori(tree(card, assets, darkColors(assets.css), size) as never, {
    ...size,
    fonts: assets.fonts,
    embedFont: o.embedFont ?? true,
  });
}

export async function render(card: Card, assets: Assets, size = OPEN_GRAPH): Promise<Buffer> {
  return sharp(Buffer.from(await layout(card, assets, { size })))
    .flatten({ background: darkColors(assets.css)["page"] })
    .removeAlpha()
    .png({ compressionLevel: 9, adaptiveFiltering: true })
    .toBuffer();
}
