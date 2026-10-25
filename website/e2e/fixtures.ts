// What every browser test shares: the page as a visitor's choices leave it, the figures it draws,
// and the rules every test keeps.
import AxeBuilder from "@axe-core/playwright";
import {
  test as base,
  expect,
  type Browser,
  type BrowserContext,
  type Page,
} from "@playwright/test";

export { expect };

export type Platform = "macos" | "windows" | "linux";
export type Surface = "app" | "cli";
export const platforms: Platform[] = ["macos", "windows", "linux"];

// AGENTS.md: no test loads claude.ai or chatgpt.com.
const theirs = /^https?:\/\/([^/?#]*\.)?(claude\.ai|chatgpt\.com)([:/?#]|$)/;

async function refuseTheirs(context: BrowserContext, problems: string[]) {
  await context.route(theirs, (route) => {
    problems.push(`a request to ${route.request().url()}`);
    return route.abort("blockedbyclient");
  });
}

// Every test fails on a request to claude.ai or chatgpt.com, a console error, an uncaught error or
// anything the CSP refuses.
export const test = base.extend<{ problems: string[] }>({
  problems: async ({}, use) => {
    await use([]);
  },
  context: async ({ context, problems }, use) => {
    await refuseTheirs(context, problems);
    await context.addInitScript(() =>
      document.addEventListener("securitypolicyviolation", (e) =>
        console.error(`the CSP refused ${e.blockedURI} by ${e.violatedDirective}`),
      ),
    );
    await use(context);
  },
  page: async ({ page, problems }, use) => {
    page.on("console", (m) => {
      if (m.type() === "error") problems.push(`console: ${m.text()}`);
    });
    page.on("pageerror", (e) => problems.push(`uncaught: ${e.message}`));
    await use(page);
    expect(problems).toEqual([]);
  },
});

// axe by WCAG 2.2 A and AA, and an id that ARIA names twice, which the figures drawn for each
// platform and section would make first.
export const rules = (page: Page) =>
  new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa", "best-practice"])
    .options({ rules: { "duplicate-id-aria": { enabled: true } } });

// What axe finds within each part, by its rules; a figure draws its system's own colours, which
// contrast does not hold it to, and `off` names any other rule a part is let off.
export async function axe(page: Page, parts: string[], off: string[] = []) {
  let builder = rules(page).disableRules(["color-contrast", ...off]);
  for (const part of parts) builder = builder.include(part);
  const found = await builder.analyze();
  return found.violations.map((v) => ({ rule: v.id, nodes: v.nodes.map((n) => n.target) }));
}

// The platform, surface and theme a visitor chose, as head.js reads them before the first paint.
export async function choose(
  context: BrowserContext,
  chosen: { platform?: Platform; surface?: Surface; theme?: "light" | "dark" },
) {
  await context.addInitScript((c) => {
    const keys = { platform: "platform", surface: "tab", theme: "theme" } as const;
    for (const [name, value] of Object.entries(c))
      if (value) localStorage.setItem(keys[name as keyof typeof keys], value);
  }, chosen);
}

// The page as a browser without JavaScript builds it, for a check that runs in the page: the same
// HTML with its scripts taken out, which the site's lack of <noscript> makes the same document.
export async function withoutScripts(page: Page) {
  await page.route("/", async (route) => {
    const response = await route.fetch();
    const body = (await response.text()).replace(/<script\b[^>]*>[\s\S]*?<\/script>/g, "");
    await route.fulfill({ response, body });
  });
}

// Every figure's root: the light-DOM custom element around its drawing, `pb-` and its name, with
// the drawing's prefix for an id.
export const roots = "[data-output]";

// The elements the page has defined once its scripts ran: a figure whose element is defined works.
export async function defined(page: Page): Promise<string[]> {
  await page.waitForLoadState("load");
  return page.evaluate(() => {
    const names = new Set(
      [...document.querySelectorAll("[data-output]")].map((root) => root.localName),
    );
    return [...names].filter((name) => customElements.get(name) !== undefined);
  });
}

// The hero's opening demo has handed over, and so made its switch to work: under reduced motion
// at once, once the hero is mostly in view (src/scripts/live/demo.ts).
export async function handedOver(page: Page) {
  await page
    .locator("figure:has([data-demo-desk])")
    .evaluate((hero) => hero.scrollIntoView({ block: "center" }));
  // the demo runs 3.8 s once it starts
  await expect(page.locator("#hero-mac-item")).toHaveAttribute("aria-label", /\bwork\b/, {
    timeout: 10_000,
  });
}

// The attributes a live figure changes (plan 2.4, rule 2), and a style that only sets custom
// properties, which is how it places what it draws.
const changing = [
  "aria-checked",
  "aria-selected",
  "aria-expanded",
  "aria-disabled",
  "inert",
  "hidden",
  "open",
  "tabindex",
  "data-state",
];

// Each figure's markup as Astro rendered it, read with JavaScript off.
export async function astroDrew(browser: Browser, baseURL: string, problems: string[]) {
  const still = await browser.newContext({
    baseURL,
    ignoreHTTPSErrors: true,
    javaScriptEnabled: false,
  });
  await refuseTheirs(still, problems);
  const page = await still.newPage();
  await page.goto("/");
  const html = await drawn(page);
  await still.close();
  return html;
}

// Each figure's markup by its root's id, without the attributes its live code changes.
export function drawn(page: Page): Promise<Record<string, string>> {
  return page.evaluate((changing) => {
    const custom = /^(\s*--[\w-]+\s*:[^;]*;?)*\s*$/;
    const out: Record<string, string> = {};
    for (const root of document.querySelectorAll("[data-output]")) {
      const copy = root.cloneNode(true) as Element;
      for (const el of [copy, ...copy.querySelectorAll("*")]) {
        for (const name of changing) el.removeAttribute(name);
        if (custom.test(el.getAttribute("style") ?? "")) el.removeAttribute("style");
      }
      out[root.id] = copy.outerHTML;
    }
    return out;
  }, changing);
}
