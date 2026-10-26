// Every figure's drawing, light and dark, against baselines made in CI's Linux image, so a change
// to how a figure looks fails until its new look is approved. Each is drawn as Astro renders it,
// before any of the page's modules runs: head.js alone shows the platform and surface chosen.
import { choose, expect, platforms, roots, test, type Surface } from "./fixtures.ts";

const surfaces: Surface[] = ["app", "cli"];

// a drawing is taken once, on its platform and surface: macOS's drawings shown on another platform,
// and an app's drawing a figure with no terminal shows on both surfaces, are the same drawing
const own = (id: string, platform: string, surface: Surface) =>
  surface === "cli"
    ? id.includes("-cli-") && id.endsWith(`-${platform}`)
    : !id.includes("-cli-") &&
      ((platform === "macos" && id.endsWith("-mac")) ||
        (platform === "windows" && id.endsWith("-win")));

test.skip(process.platform !== "linux", "the baselines are made in CI's Linux image");
test.use({ reducedMotion: "reduce" });

for (const colorScheme of ["light", "dark"] as const) {
  for (const platform of platforms) {
    for (const surface of surfaces) {
      if (platform === "linux" && surface === "app") continue;
      test(`${platform} ${surface} ${colorScheme}`, async ({ page }) => {
        await page.emulateMedia({ colorScheme });
        await choose(page.context(), { platform, surface });
        await page.route("**/_astro/*.js", (route) =>
          route.fulfill({ contentType: "text/javascript", body: "" }),
        );
        await page.goto("/");
        await page.evaluate(() => document.fonts.ready);
        const shown = page.locator(`${roots}:visible`);
        const ids = (await shown.evaluateAll((all) => all.map((root) => root.id))).filter((id) =>
          own(id, platform, surface),
        );
        expect(ids.length).toBeGreaterThan(4);
        // every figure is compared, and every missing baseline written, before the test fails;
        // each is taken a whole pixel down the page, so a change above it moves it by whole pixels
        // the page's header stays at the top of the page, off the figure it would cover
        await page.evaluate(
          () => (document.querySelector<HTMLElement>("body > header")!.style.position = "static"),
        );
        for (const id of ids) {
          await page.evaluate((id) => {
            const root = document.documentElement;
            root.style.paddingTop = "0px";
            const top = document.getElementById(id)!.getBoundingClientRect().top + scrollY;
            root.style.paddingTop = `${Math.ceil(top) - top}px`;
          }, id);
          await expect.soft(page.locator(`#${id}`)).toHaveScreenshot(`${id}-${colorScheme}.png`);
        }
      });
    }
  }
}
