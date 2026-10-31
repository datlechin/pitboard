// One markup, static and live (plan 2.4): without JavaScript every figure is its drawing and nothing
// in it can be used; with it, each figure is the markup Astro rendered, apart from the attributes
// its live code changes.
import {
  astroDrew,
  choose,
  defined,
  drawn,
  expect,
  handedOver,
  platforms,
  roots,
  test,
} from "./fixtures.ts";

test.use({ reducedMotion: "reduce" });

test.describe("without JavaScript", () => {
  test.use({ javaScriptEnabled: false });

  test("every figure is inert, so nothing in it takes focus, a click or a key", async ({
    page,
  }) => {
    await page.goto("/");
    const figures = await page.locator(roots).evaluateAll((all) =>
      all.map((root) => ({
        name: root.localName,
        inert: (root as HTMLElement).inert,
        hidden: root.getAttribute("aria-hidden"),
      })),
    );
    expect(figures.length).toBeGreaterThan(20);
    for (const f of figures) {
      expect(f.name).toMatch(/^pb-[a-z]+(-[a-z]+)*$/);
      expect(f.inert).toBe(true);
      expect(f.hidden).toBeNull();
    }
    const focusable = await page
      .locator(`${roots} :is(a[href], button, input, select, textarea, [tabindex])`)
      .evaluateAll((all) =>
        all.filter((el) => {
          (el as HTMLElement).focus();
          return document.activeElement === el;
        }),
      );
    expect(focusable).toEqual([]);

    // the menu bar item, a click or Enter on which would open Pitboard's menu
    const item = page.locator("#hero-mac-item");
    await item.click({ force: true });
    await page.keyboard.press("Enter");
    await expect(item).not.toBeFocused();
    await expect(item).toHaveAttribute("aria-expanded", "true");
    await expect(page).toHaveURL("/");
  });
});

// the figures index.astro draws at the moment before the demo's switch
const before = ["usage-mac", "usage-win"];

test.describe("with JavaScript", () => {
  for (const platform of platforms) {
    test(`on ${platform}, every figure is the markup Astro drew`, async ({
      page,
      browser,
      baseURL,
      problems,
    }) => {
      const astro = await astroDrew(browser, baseURL!, problems);
      await choose(page.context(), { platform });
      await page.clock.install();
      await page.goto("/");
      const live = new Set(await defined(page));
      await handedOver(page);
      const now = await drawn(page);
      expect(Object.keys(now)).toEqual(Object.keys(astro));
      for (const [id, html] of Object.entries(now))
        if (!before.includes(id)) expect(html, id).toBe(astro[id]);
      // the window #usage draws before the switch: back on personal, it is that markup again, in
      // the window the platform shows, a hidden one drawing nothing until it shows
      await page
        .locator('#switch-mac [data-account="personal"] [data-does="use"]')
        .dispatchEvent("click");
      const inUse = {
        macos: ['#usage-mac [data-account="personal"]', "Meta+C"],
        windows: ['#usage-win [data-account="personal"] [data-does="expand"]', "F2 Control+C"],
      }[platform as string];
      if (inUse)
        await expect(page.locator(inUse[0])).toHaveAttribute("aria-keyshortcuts", inUse[1]);
      const back = await drawn(page);
      for (const id of before) expect(back[id], id).toBe(astro[id]);
      // a figure whose element is not defined stays the drawing, inert
      const inert = await page
        .locator(roots)
        .evaluateAll((all) =>
          all.map((root) => [root.localName, (root as HTMLElement).inert] as const),
        );
      for (const [name, isInert] of inert) expect(isInert, name).toBe(!live.has(name));
    });
  }
});

test.describe("on a phone", () => {
  test.skip(({ isMobile }) => !isMobile, "a phone's width");

  for (const platform of platforms)
    for (const surface of ["app", "cli"] as const)
      test(`on ${platform}, every ${surface} figure fits the page and scrolls nothing sideways`, async ({
        page,
      }) => {
        await choose(page.context(), { platform, surface });
        await page.goto("/");
        const over = await page.locator(roots).evaluateAll((all) =>
          all
            .filter((root) => root.checkVisibility())
            .filter((root) => {
              const box = root.getBoundingClientRect();
              const scrolls = [root, root.closest("[data-drawing]")!].some((el) =>
                /auto|scroll/.test(getComputedStyle(el).overflowX),
              );
              return scrolls || box.left < 0 || box.right > innerWidth;
            })
            .map((root) => root.id),
        );
        expect(over).toEqual([]);
        expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
          await page.evaluate(() => innerWidth),
        );
      });
});
