// axe on the home page in light and dark, with JavaScript and without, and every figure as it is once
// it works: the roles, names and states its drawing carries, out from under its inert. A figure
// draws its system's own colours, which axe's contrast does not hold it to.
import AxeBuilder from "@axe-core/playwright";
import type { Page } from "@playwright/test";
import { choose, expect, platforms, roots, test, withoutScripts } from "./fixtures.ts";

const schemes = ["light", "dark"] as const;

// WCAG 2.2 A and AA, and an id that ARIA names twice, which the figures drawn for each platform and
// section would make first.
const axe = (page: Page) =>
  new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa", "best-practice"])
    .options({ rules: { "duplicate-id-aria": { enabled: true } } });

// one run at a time: axe refuses a second while the first runs
async function violations(...builders: AxeBuilder[]) {
  const found = [];
  for (const b of builders)
    for (const v of (await b.analyze()).violations)
      found.push({ rule: v.id, nodes: v.nodes.map((n) => n.target.join(" ")) });
  return found;
}

const figures = (page: Page) => axe(page).include(roots).disableRules(["color-contrast"]);
const everything = (page: Page) => violations(axe(page).exclude(roots), figures(page));

test.use({ reducedMotion: "reduce" });

for (const colorScheme of schemes) {
  test.describe(`${colorScheme}`, () => {
    test.use({ colorScheme });

    test("without JavaScript", async ({ page }) => {
      await withoutScripts(page);
      await page.goto("/");
      expect(await everything(page)).toEqual([]);
    });

    for (const platform of platforms) {
      test(`on ${platform}`, async ({ page }) => {
        await choose(page.context(), { platform });
        await page.goto("/");
        expect(await everything(page)).toEqual([]);
      });

      test(`on ${platform}, every figure as it works`, async ({ page }) => {
        await choose(page.context(), { platform });
        await page.goto("/");
        await page.locator(roots).evaluateAll((all) => {
          for (const root of all) (root as HTMLElement).inert = false;
        });
        expect(await violations(figures(page))).toEqual([]);
      });
    }
  });
}
