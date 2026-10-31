// The Linux window as the site draws it against CI's pictures of the app (plan section 7, "Against
// CI's pictures of the Linux app"): each scene drawn at device scale 2 with a desk unit a pixel, its
// elements matched by kind to GTK's in the picture's boxes file, their metrics held within a pixel
// and their colours within a byte, and a sheet of the two side by side for each scheme. The worlds
// differ, so words, counts and what text sets are not compared. No CI job runs it: name a copy of
// a run's linux-app-pictures in PITBOARD_LINUX_PICTURES, and PITBOARD_LINUX_SHEETS for the sheets.
import { mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import type { Page } from "@playwright/test";
import sharp from "sharp";
import { choose, expect, handedOver, test } from "./fixtures.ts";

const pictures = process.env.PITBOARD_LINUX_PICTURES;
const sheets = process.env.PITBOARD_LINUX_SHEETS;

interface Box {
  parent: number | null;
  type: string;
  css_name: string;
  css_classes: string[];
  bounds: { x: number; y: number; width: number; height: number };
  font?: { family: string; size: number; weight: number };
}
interface Measured {
  x: number;
  y: number;
  width: number;
  height: number;
  size?: number;
  weight?: number;
}
type Prop = "x" | "y" | "width" | "height" | "size" | "weight";
// a point of an element whose colour is its ground, away from its corners and its text
type Point = "left" | "right" | "top";

interface Kind {
  name: string;
  // GTK's node and the classes it has and has not, the type where a node is not enough, and the
  // node, node.class or type it is inside
  gtk: { node: string; classes?: string[]; not?: string[]; type?: string; within?: string };
  // the site's selector, inside the scene's root unless it starts with #
  site: string;
  compare: Prop[];
  // what the text sets, which Inter, standing in for Adwaita Sans, may set a little apart
  typeSets?: Prop[];
  colour?: Point;
}

interface Scene {
  name: string;
  picture: string;
  system: "gnome" | "kde";
  root: string;
  // what draws the scene in the site, once the page has handed over
  open?: (page: Page) => Promise<void>;
  kinds: Kind[];
}

const header: Kind[] = [
  {
    name: "header bar",
    gtk: { node: "headerbar" },
    site: '[data-page]:not([hidden]) > [data-part="header-bar"]',
    compare: ["y", "height"],
    colour: "top",
  },
  {
    name: "header bar's title",
    gtk: { node: "label", classes: ["title"], within: "headerbar" },
    site: '[data-page]:not([hidden]) > [data-part="header-bar"] [data-part="title"]',
    compare: ["size", "weight"],
  },
  {
    name: "header bar's first button",
    gtk: { node: "button", classes: ["image-button"], within: "headerbar" },
    site: '[data-page]:not([hidden]) > [data-part="header-bar"] button',
    compare: ["x", "y", "width", "height"],
  },
  {
    name: "Close",
    gtk: { node: "button", classes: ["image-button", "close"], within: "windowcontrols" },
    site: '[data-page]:not([hidden]) > [data-part="header-bar"] [data-does="close"]',
    compare: ["x", "y", "width", "height"],
  },
];

const accounts: Kind[] = [
  ...header,
  {
    name: "window",
    gtk: { node: "window" },
    site: "",
    compare: ["width", "height"],
  },
  {
    name: "notice row",
    gtk: { node: "row", type: "PitboardNoticeRow" },
    site: "[data-notice]:not([hidden])",
    compare: ["x", "width"],
    colour: "left",
  },
  {
    name: "notice's icon",
    gtk: { node: "image", within: "PitboardNoticeRow" },
    site: '[data-notice]:not([hidden]) [data-part="severity"] svg',
    compare: ["x", "width", "height"],
  },
  {
    name: "notice's title",
    gtk: { node: "label", classes: ["title"], within: "PitboardNoticeRow" },
    site: '[data-notice]:not([hidden]) [data-part="title"]',
    compare: ["x", "size", "weight"],
  },
  {
    name: "notice's subtitle",
    gtk: { node: "label", classes: ["subtitle"], within: "PitboardNoticeRow" },
    site: '[data-notice]:not([hidden]) [data-part="subtitle"]',
    compare: ["x", "size", "weight"],
  },
  {
    name: "account's group heading",
    gtk: { node: "label", classes: ["heading", "h4"], type: "GtkLabel" },
    site: '[data-part="list"] > [data-account] [data-part="group-title"]',
    compare: ["x", "size", "weight"],
  },
  {
    name: "account's boxed list",
    gtk: { node: "list", classes: ["boxed-list"], within: "preferencesgroup" },
    site: '[data-part="list"] > [data-account] [data-part="rows"]',
    compare: ["x", "width"],
    colour: "left",
  },
  {
    name: "limit row",
    gtk: { node: "row", type: "PitboardLimitRow" },
    site: '[data-part="list"] [data-part="limit"]',
    compare: ["x", "width", "height"],
  },
  {
    name: "limit row's title",
    gtk: { node: "label", classes: ["title"], within: "PitboardLimitRow" },
    site: '[data-part="list"] [data-part="limit"] [data-part="title"]',
    compare: ["x", "size", "weight"],
  },
  {
    name: "limit row's subtitle",
    gtk: { node: "label", classes: ["subtitle"], within: "PitboardLimitRow" },
    site: '[data-part="list"] [data-part="limit"] [data-part="subtitle"]',
    compare: ["size", "weight"],
  },
  {
    name: "level bar",
    gtk: { node: "levelbar" },
    site: '[data-part="list"] [data-part="limit"] svg',
    compare: ["width", "height"],
    colour: "right",
  },
  {
    name: "level bar's fill, plenty",
    gtk: { node: "block", classes: ["plenty", "filled"] },
    site: '[data-part="list"] [data-part="limit"] [data-fill="plenty"]',
    compare: ["height"],
    colour: "left",
  },
  {
    name: "level bar's figure",
    gtk: { node: "label", classes: ["numeric"] },
    site: '[data-part="list"] [data-part="limit"] [data-part="figure"]',
    compare: ["size", "weight"],
  },
  {
    name: "property row",
    gtk: { node: "row", classes: ["property"] },
    site: '[data-part="list"] [data-part="email"]',
    compare: ["x", "width", "height"],
  },
  {
    name: "property row's title",
    gtk: { node: "label", classes: ["title"], within: "row.property" },
    site: '[data-part="list"] [data-part="email"] [data-part="title"]',
    compare: ["x", "size", "weight"],
  },
  {
    name: "property row's subtitle",
    gtk: { node: "label", classes: ["subtitle"], within: "row.property" },
    site: '[data-part="list"] [data-part="email"] [data-part="subtitle"]',
    compare: ["size", "weight"],
  },
  {
    name: "account's menu button",
    gtk: { node: "menubutton", classes: ["flat", "popup"] },
    site: '[data-part="list"] [data-does="account-menu"]',
    compare: ["x", "width", "height"],
  },
  {
    name: "account's text button",
    gtk: { node: "button", classes: ["text-button"], within: "PitboardAccountGroup" },
    site: '[data-part="list"] [data-does="use"]:visible',
    compare: ["height"],
    colour: "left",
  },
  {
    name: "Warm-Up Preferences",
    gtk: { node: "button", classes: ["flat"], type: "GtkButton", within: "row.property" },
    site: '[data-part="list"] [data-does="warm-preferences"]',
    compare: ["height"],
  },
];

const sheet = (id: string, extra: Kind[]): Kind[] => [
  {
    name: "sheet",
    gtk: { node: "sheet", classes: ["background"] },
    site: id,
    compare: ["x", "y", "width", "height"],
    colour: "left",
  },
  {
    name: "dimming over the window",
    gtk: { node: "dimming" },
    site: '[data-part="dimming"]',
    compare: [],
    colour: "left",
  },
  {
    name: "sheet's header bar",
    gtk: { node: "headerbar", within: "sheet" },
    site: `${id} [data-part="header-bar"]`,
    compare: ["height"],
  },
  ...extra,
];

const preferences: Kind[] = [
  {
    name: "sheet",
    gtk: { node: "sheet", classes: ["background"] },
    site: "pb-linux-preferences",
    compare: ["x", "y", "width", "height"],
  },
  {
    name: "view switcher's button",
    gtk: { node: "button", classes: ["flat", "horizontal", "toggle"] },
    site: 'pb-linux-preferences [role="tab"]',
    compare: ["width", "height"],
    // homogeneous, each as wide as the widest title
    typeSets: ["width"],
  },
  {
    name: "group heading",
    gtk: { node: "label", classes: ["heading", "h4"], within: "sheet" },
    site: 'pb-linux-preferences [role="tabpanel"]:not([hidden]) [data-part="group-title"]',
    compare: ["x", "size", "weight"],
  },
  {
    name: "group's boxed list",
    gtk: { node: "list", classes: ["boxed-list"], within: "sheet" },
    site: 'pb-linux-preferences [role="tabpanel"]:not([hidden]) .bg-adw-card',
    compare: ["x", "width"],
    colour: "left",
  },
  {
    name: "row's title",
    gtk: { node: "label", classes: ["title"], within: "sheet" },
    site: 'pb-linux-preferences [role="tabpanel"]:not([hidden]) [data-part="title"]',
    compare: ["x", "size", "weight"],
  },
  {
    name: "switch",
    gtk: { node: "switch", within: "sheet" },
    site: 'pb-linux-preferences [role="tabpanel"]:not([hidden]) [role="switch"]',
    compare: ["width", "height"],
  },
  {
    name: "dimming over the window",
    gtk: { node: "dimming" },
    site: '[data-part="dimming"]',
    compare: [],
    colour: "left",
  },
];

const limits: Kind[] = [
  ...preferences,
  {
    name: "combo row",
    gtk: { node: "row", classes: ["combo"] },
    site: 'pb-linux-preferences [data-part="at-limit-row"]',
    compare: ["x", "width", "height"],
  },
  {
    name: "spin row",
    gtk: { node: "row", classes: ["spin"] },
    site: 'pb-linux-preferences [data-part="share"]',
    compare: ["x", "width", "height"],
  },
  {
    name: "spin button's step",
    gtk: { node: "button", classes: ["image-button", "up"] },
    site: 'pb-linux-preferences [data-part="share"] [data-spin="up"]',
    compare: ["width", "height"],
  },
];

const scenes: Scene[] = [
  {
    name: "accounts",
    picture: "linux-accounts-twoTools",
    system: "gnome",
    root: "#usage-linux",
    kinds: accounts,
  },
  ...(["general", "limits", "advanced"] as const).flatMap((tab) =>
    (["gnome", "kde"] as const).map((system): Scene => ({
      name: `preferences-${tab}-${system}`,
      picture: `linux-preferences-${tab}${system === "kde" ? "-tray" : ""}`,
      system,
      root: "#auto-linux",
      open: async (page) => {
        await page.locator(`#auto-linux-${tab}-tab`).click();
      },
      // KDE Plasma's window takes Noto Sans 10 through its portal, where the pictures are drawn
      // with GNOME's settings: its type is not compared
      kinds: (tab === "limits" ? limits : preferences).map((k) =>
        system === "gnome"
          ? k
          : { ...k, compare: k.compare.filter((p) => p !== "size" && p !== "weight") },
      ),
    })),
  ),
  {
    name: "activity",
    picture: "linux-activity",
    system: "gnome",
    root: "#security-linux",
    kinds: [
      ...header,
      {
        name: "column header",
        gtk: { node: "header", classes: ["activatable"] },
        site: '[data-page="activity"] thead tr',
        compare: ["y", "height"],
      },
      {
        name: "column header's caption",
        gtk: { node: "label", within: "header" },
        site: '[data-page="activity"] th',
        compare: ["size", "weight"],
      },
      {
        name: "log row",
        gtk: { node: "row", classes: ["activatable"], within: "listview" },
        site: '[data-page="activity"] tbody tr',
        compare: ["y", "height"],
      },
      {
        name: "date",
        gtk: { node: "label", classes: ["numeric"] },
        site: '[data-page="activity"] tbody td',
        compare: ["size", "weight"],
      },
    ],
  },
  {
    name: "remove",
    picture: "linux-question-remove",
    system: "gnome",
    root: "#usage-linux",
    open: async (page) => {
      await page
        .locator('#usage-linux [data-account="personal"] [data-does="account-menu"]')
        .click();
      await page.locator('#usage-linux-account-menu [data-offer="forget"]').click();
    },
    kinds: [
      {
        name: "alert",
        gtk: { node: "sheet", classes: ["background"] },
        site: "#linux-accounts-remove",
        // its words, which the worlds do not share, set its height, and so where it floats
        compare: ["x", "width"],
        colour: "top",
      },
      {
        name: "alert's heading",
        gtk: { node: "label", classes: ["title-2"] },
        site: '#linux-accounts-remove [data-part="title"]',
        compare: ["size", "weight"],
      },
      {
        name: "alert's message",
        gtk: { node: "label", classes: ["body"], within: "sheet" },
        site: '#linux-accounts-remove [data-part="message"]',
        compare: ["size", "weight"],
      },
      {
        name: "alert's response",
        gtk: { node: "button", classes: ["text-button"], within: "sheet" },
        site: "#linux-accounts-remove button",
        compare: ["width", "height"],
        colour: "left",
      },
    ],
  },
  {
    name: "name",
    picture: "linux-name",
    system: "gnome",
    root: "#usage-linux",
    // Name This Account, after a sign-in, as the picture draws it
    open: async (page) => {
      await page.locator('#usage-linux [data-does="add"]').click();
      await page.locator('#linux-accounts-sign-in [data-does="sign-in"]').click();
      await expect(page.locator("#linux-accounts-name")).toBeVisible({ timeout: 8000 });
    },
    kinds: sheet("#linux-accounts-name", [
      {
        name: "entry row",
        gtk: { node: "row", classes: ["entry"] },
        site: "#linux-accounts-name label",
        compare: ["x", "width", "height"],
      },
      {
        name: "suggested button",
        gtk: { node: "button", classes: ["suggested-action"], within: "sheet" },
        site: '#linux-accounts-name [data-part="confirm"]',
        compare: ["y", "height"],
        colour: "left",
      },
    ]),
  },
  {
    name: "sign-in",
    picture: "linux-signIn-tool",
    system: "gnome",
    root: "#usage-linux",
    open: async (page) => {
      await page.locator('#usage-linux [data-does="add"]').click();
    },
    kinds: sheet("#linux-accounts-sign-in", [
      {
        name: "toggle group",
        gtk: { node: "toggle-group" },
        site: '#linux-accounts-sign-in [data-part="tool"]',
        compare: ["x", "width", "height"],
        colour: "left",
      },
      {
        name: "toggle",
        gtk: { node: "toggle", classes: ["text-button", "toggle"] },
        site: '#linux-accounts-sign-in [data-part="tool"] button',
        compare: ["height"],
      },
    ]),
  },
  {
    name: "open-link",
    picture: "linux-openLink-empty",
    system: "gnome",
    root: "#usage-linux",
    open: async (page) => {
      await page.locator('#usage-linux [data-does="main-menu"]').click();
      await page.locator('#usage-linux-main-menu [data-does="open-link"]').click();
    },
    kinds: sheet("#linux-accounts-open-link", [
      {
        name: "status page's title",
        gtk: { node: "label", classes: ["title"], within: "statuspage" },
        site: "#linux-accounts-open-link .adw-title-2",
        compare: ["size", "weight"],
      },
      {
        name: "pill",
        gtk: { node: "button", classes: ["pill"] },
        site: '#linux-accounts-open-link [data-does="paste"]',
        compare: ["height"],
        colour: "left",
      },
    ]),
  },
  {
    name: "shortcuts",
    picture: "linux-shortcuts",
    system: "gnome",
    root: "#usage-linux",
    open: async (page) => {
      await page.locator('#usage-linux [data-does="main-menu"]').click();
      await page.locator('#usage-linux-main-menu [data-does="shortcuts"]').click();
    },
    kinds: sheet("#linux-accounts-shortcuts", [
      {
        name: "row",
        gtk: { node: "row", within: "sheet" },
        site: "#linux-accounts-shortcuts .bg-adw-card > *",
        compare: ["x", "width", "height"],
      },
    ]),
  },
  {
    name: "about",
    picture: "linux-about",
    system: "gnome",
    root: "#usage-linux",
    open: async (page) => {
      await page.locator('#usage-linux [data-does="main-menu"]').click();
      await page.locator('#usage-linux-main-menu [data-does="about"]').click();
    },
    kinds: sheet("#linux-accounts-about", [
      {
        name: "app's name",
        gtk: { node: "label", classes: ["title-1"] },
        site: "#linux-accounts-about .adw-title-1",
        compare: ["size", "weight"],
      },
    ]),
  },
];

// GTK's widgets of a kind, parents before children, in the order they are drawn
function gtkOf(boxes: Box[], kind: Kind): Box[] {
  const { node, classes = [], not = [], type, within } = kind.gtk;
  const [inNode, inClass] = within?.split(".") ?? [];
  const byType = /^[A-Z]/.test(inNode ?? "");
  const inside = (i: number | null): boolean => {
    for (let p = i; p !== null; p = boxes[p].parent)
      if (
        byType
          ? boxes[p].type === inNode
          : boxes[p].css_name === inNode && (!inClass || boxes[p].css_classes.includes(inClass))
      )
        return true;
    return false;
  };
  return boxes.filter(
    (b) =>
      b.css_name === node &&
      classes.every((c) => b.css_classes.includes(c)) &&
      !not.some((c) => b.css_classes.includes(c)) &&
      (!type || b.type === type) &&
      (!within || inside(b.parent)) &&
      b.bounds.width > 0 &&
      b.bounds.height > 0,
  );
}

// the site's elements of a kind, measured from the scene's window at scale 1
function siteOf(page: Page, root: string, kind: Kind): Promise<Measured[]> {
  return page.evaluate(
    ([root, selector]) => {
      const window =
        document.querySelector(root)!.closest("pb-linux-window") ?? document.querySelector(root)!;
      const at = window.getBoundingClientRect();
      const visible = selector.endsWith(":visible");
      const own = visible ? selector.slice(0, -":visible".length) : selector;
      const els =
        own === ""
          ? [window]
          : own.startsWith("#")
            ? [...document.querySelectorAll(own)]
            : [...window.querySelectorAll(own)];
      return els
        .filter((el) => (el as HTMLElement).checkVisibility())
        .map((el) => {
          const b = el.getBoundingClientRect();
          const cs = getComputedStyle(el);
          return {
            x: b.x - at.x,
            y: b.y - at.y,
            width: b.width,
            height: b.height,
            size: parseFloat(cs.fontSize),
            weight: Number(cs.fontWeight),
          };
        })
        .sort((a, b) => a.y - b.y || a.x - b.x);
    },
    [root, kind.site] as const,
  );
}

const pointOf = (b: Measured, at: Point) =>
  at === "left"
    ? { x: b.x + 4, y: b.y + b.height / 2 }
    : at === "right"
      ? { x: b.x + b.width - 3, y: b.y + b.height / 2 }
      : { x: b.x + b.width / 2, y: b.y + 2 };

async function raw(png: Buffer) {
  const { data, info } = await sharp(png).raw().toBuffer({ resolveWithObject: true });
  return { data, width: info.width, channels: info.channels };
}
type Raw = Awaited<ReturnType<typeof raw>>;
const rgb = (img: Raw, x: number, y: number) => {
  const i = (Math.floor(y * 2) * img.width + Math.floor(x * 2)) * img.channels;
  return [img.data[i], img.data[i + 1], img.data[i + 2]];
};

test.skip(!pictures, "PITBOARD_LINUX_PICTURES names no copy of linux-app-pictures");
test.skip(({ browserName, isMobile }) => browserName !== "chromium" || isMobile, "Chromium alone");
test.use({ reducedMotion: "reduce", deviceScaleFactor: 2, viewport: { width: 1280, height: 900 } });

for (const scene of scenes)
  for (const scheme of ["light", "dark"] as const)
    test(`${scene.name} in ${scheme} is drawn as the app draws ${scene.picture}`, async ({
      page,
    }) => {
      const file = join(pictures!, `${scene.picture}${scheme === "dark" ? "-dark" : ""}.png`);
      const boxes = JSON.parse(
        readFileSync(join(pictures!, `${scene.picture}.boxes.json`), "utf8"),
      ) as Box[];
      await page.emulateMedia({ colorScheme: scheme });
      await choose(page.context(), { system: scene.system });
      await page.goto("/");
      await handedOver(page);
      const root = page.locator(scene.root);
      await root.scrollIntoViewIfNeeded();
      await expect(root).not.toHaveAttribute("inert");
      // a desk unit a pixel, however wide a column of the page is, the dialogs' desk too
      await page.evaluate(() => {
        for (const desk of document.querySelectorAll<HTMLElement>(".desk"))
          desk.style.setProperty("--u", "1px");
      });
      await scene.open?.(page);
      await page.mouse.move(0, 0);
      await page.evaluate(() => document.fonts.ready);
      // the window, from the whole pixel nearest its corner
      const at = (await root.boundingBox())!;
      const shot = await page.screenshot({
        clip: { x: Math.round(at.x), y: Math.round(at.y), width: 600, height: 640 },
        animations: "disabled",
        caret: "hide",
      });
      const picture = await raw(readFileSync(file));
      const drawn = await raw(shot);
      const misses: string[] = [];
      for (const kind of scene.kinds) {
        const theirs = gtkOf(boxes, kind)[0];
        const [ours] = await siteOf(page, scene.root, kind);
        if (!theirs || !ours) {
          // a kind neither draws is not this scene's
          if (theirs || ours)
            misses.push(`${kind.name}: ${theirs ? "not drawn by the site" : "not in the picture"}`);
          continue;
        }
        const gtk: Measured = { ...theirs.bounds, ...theirs.font };
        for (const prop of kind.compare.filter((p) => !kind.typeSets?.includes(p))) {
          const [a, b] = [gtk[prop], ours[prop]];
          if (a === undefined || b === undefined) continue;
          const within = prop === "size" ? 0.1 : prop === "weight" ? 0 : 1;
          if (Math.abs(a - b) > within)
            misses.push(`${kind.name}: ${prop} ${b.toFixed(2)} where GTK's is ${a.toFixed(2)}`);
        }
        if (kind.colour) {
          const p = pointOf(gtk, kind.colour);
          const q = pointOf(ours, kind.colour);
          const [c, d] = [rgb(picture, p.x, p.y), rgb(drawn, q.x, q.y)];
          if (c.some((v, i) => Math.abs(v - d[i]) > 1))
            misses.push(`${kind.name}: colour rgb(${d}) where GTK's is rgb(${c})`);
        }
      }
      if (sheets) {
        mkdirSync(sheets, { recursive: true });
        const height = 1280;
        const gap = 40;
        await sharp({
          create: { width: 2400 + gap, height, channels: 4, background: "#808080" },
        })
          .composite([
            { input: file, left: 0, top: 0 },
            { input: shot, left: 1200 + gap, top: 0 },
          ])
          .png()
          .toFile(join(sheets, `${scene.name}-${scheme}.png`));
      }
      expect(misses).toEqual([]);
    });
