// Every figure's drawing, light and dark, against baselines made in CI's Linux image, so a change
// to how a figure looks fails until its new look is approved. Each is drawn as Astro renders it,
// before any of the page's modules runs: head.js alone shows the platform, desktop and surface
// chosen.
import {
  choose,
  desktopOf,
  expect,
  platformOf,
  roots,
  systems,
  test,
  type Surface,
  type System,
} from "./fixtures.ts";

const surfaces: Surface[] = ["app", "cli"];
const ends: Record<System, string[]> = {
  macos: ["-mac"],
  windows: ["-win"],
  gnome: ["-linux", "-gnome"],
  kde: ["-linux", "-kde"],
};

// a drawing is taken once, on its system and surface: macOS's drawings shown on another platform,
// and an app's drawing a figure with no terminal shows on both surfaces, are the same drawing
const own = (id: string, system: System, surface: Surface) =>
  surface === "cli"
    ? id.includes("-cli-") && id.endsWith(`-${platformOf(system)}`)
    : !id.includes("-cli-") && ends[system].some((end) => id.endsWith(end));

// a Linux drawing both desktops share is taken under each, named for it
const named = (id: string, system: System) => {
  const desktop = desktopOf(system);
  return desktop && !id.endsWith(`-${desktop}`) ? `${id}-${desktop}` : id;
};

test.skip(process.platform !== "linux", "the baselines are made in CI's Linux image");
test.use({ reducedMotion: "reduce" });

for (const colorScheme of ["light", "dark"] as const) {
  for (const system of systems) {
    for (const surface of surfaces) {
      test(`${system} ${surface} ${colorScheme}`, async ({ page }) => {
        await page.emulateMedia({ colorScheme });
        await choose(page.context(), { system, surface });
        await page.route("**/_astro/*.js", (route) =>
          route.fulfill({ contentType: "text/javascript", body: "" }),
        );
        await page.goto("/");
        await page.evaluate(() => document.fonts.ready);
        const shown = page.locator(`${roots}:visible`);
        const ids = (await shown.evaluateAll((all) => all.map((root) => root.id))).filter((id) =>
          own(id, system, surface),
        );
        expect(ids.length).toBeGreaterThan(4);
        // every figure is compared, and every missing baseline written, before the test fails;
        // each is taken a whole pixel down the page, so a change above it moves it by whole pixels
        // the page's header stays at the top of the page, off the figure it would cover, and the room
        // kept above a figure for it goes, or a figure nearly the window's height loses its foot
        await page.evaluate(() => {
          document.querySelector<HTMLElement>("body > header")!.style.position = "static";
          document.documentElement.style.scrollPaddingTop = "0px";
        });
        for (const id of ids) {
          await page.evaluate((id) => {
            const root = document.documentElement;
            root.style.paddingTop = "0px";
            const top = document.getElementById(id)!.getBoundingClientRect().top + scrollY;
            root.style.paddingTop = `${Math.ceil(top) - top}px`;
          }, id);
          await expect
            .soft(page.locator(`#${id}`))
            .toHaveScreenshot(`${named(id, system)}-${colorScheme}.png`);
        }
      });
    }
  }
}
