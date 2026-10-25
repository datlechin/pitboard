// The hero's two desks: the demo that plays once and hands over, then Pitboard's macOS menu as
// NSMenu runs it and its Windows icon menu as WinUI's MenuFlyout runs it (plan 3.1, 3.2, section 4).
import type { Page } from "@playwright/test";
import { axe, choose, expect, handedOver, test, type Platform } from "./fixtures.ts";

const mac = {
  item: "#hero-mac-item",
  menu: "#hero-mac-menu",
  // an account's item in the moment the state draws
  account: (id: string) => `#hero-mac-menu [data-moment="after"] [data-account="${id}"]`,
};
const win = {
  icon: "#hero-win-icon",
  menu: "#hero-win-menu",
  account: (id: string) =>
    `#hero-win-menu [data-moment="after"] [data-tool] [data-account="${id}"]`,
};

async function hero(page: Page, platform: Platform) {
  await choose(page.context(), { platform });
  await page.goto("/");
  await handedOver(page);
}

// the hero's own Reset, of the one in each working figure's caption
const heroReset = "figure:has([data-demo-desk]) [data-reset]";

// time held, so each delay runs only as a test lets it: a second on, which leaves room for the
// time the page takes to answer
async function hold(page: Page) {
  await page.clock.install();
  await page.clock.pauseAt((await page.evaluate(() => Date.now())) + 1000);
}

// the frames the demo set on the root, with the menu bar item's name as each began
async function recordFrames(page: Page) {
  await page.addInitScript(() => {
    const seen: { demo: string | null; name: string | null }[] = [];
    (window as unknown as { frames_: typeof seen }).frames_ = seen;
    // the root is not made yet: head.js sets the first frame as it is parsed
    new MutationObserver(() =>
      seen.push({
        demo: document.documentElement.getAttribute("data-demo"),
        name: document.getElementById("hero-mac-item")?.getAttribute("aria-label") ?? null,
      }),
    ).observe(document, { subtree: true, attributeFilter: ["data-demo"] });
  });
}
const frames = (page: Page) =>
  page.evaluate(() => (window as unknown as { frames_: unknown[] }).frames_);

test.describe("the opening demo", () => {
  test.skip(({ isMobile }) => isMobile, "the hero is below the fold on a phone");

  test("plays once in view, switches to work at its pick, and hands over with the menu open", async ({
    page,
  }) => {
    await recordFrames(page);
    await choose(page.context(), { platform: "macos" });
    await page.clock.install();
    await page.goto("/");
    await expect(page.locator("html")).not.toHaveAttribute("data-demo", { timeout: 8000 });
    const before = "Pitboard, personal 73%, Claude Code";
    const after = "Pitboard, work 40%, Claude Code";
    expect(await frames(page)).toEqual([
      { demo: "start", name: null },
      { demo: "move", name: before },
      { demo: "hl", name: before },
      { demo: "pick", name: after },
      { demo: "shut", name: after },
      { demo: "reach", name: after },
      { demo: null, name: after },
    ]);
    await expect(page.locator(mac.item)).toHaveAttribute("aria-expanded", "true");
    await expect(page.locator(mac.menu)).toBeVisible();
    await expect(page.locator('#hero-mac [data-part="pointer"]')).toHaveCSS("opacity", "0");
    // the page rests here: the demo's switch is no change of the visitor's to undo
    await expect(page.locator(heroReset)).toBeHidden();
    // a platform change no longer plays it again
    await page.locator('[role="tab"][data-platform="windows"]').first().click();
    await page.clock.runFor(700);
    expect(await frames(page)).toHaveLength(7);
  });

  test("stops at a press inside a working figure, and the state stays as it stands", async ({
    page,
  }) => {
    await choose(page.context(), { platform: "macos" });
    await page.clock.install();
    await page.goto("/");
    await expect(page.locator("html")).toHaveAttribute("data-demo", "move");
    await page.locator("#hero-mac").click({ position: { x: 60, y: 420 } });
    await expect(page.locator("html")).not.toHaveAttribute("data-demo");
    // the rest of the demo's time, which would have reached its pick
    await page.clock.runFor(2500);
    await expect(page.locator(mac.item)).toHaveAttribute(
      "aria-label",
      "Pitboard, personal 73%, Claude Code",
    );
    await expect(page.locator(mac.account("personal"))).toHaveAttribute("aria-checked", "true");
  });

  test.describe("under reduced motion", () => {
    test.use({ reducedMotion: "reduce" });

    test("is not played, and the switch is made at once", async ({ page }) => {
      await recordFrames(page);
      await choose(page.context(), { platform: "macos" });
      await page.goto("/");
      await handedOver(page);
      expect(await frames(page)).toEqual([]);
      await expect(page.locator(mac.menu)).toBeVisible();
      await expect(page.locator(mac.account("work"))).toHaveAttribute("aria-checked", "true");
    });
  });

  test("on the Command line tab the switch is made at once, and the App tab shows the last frame", async ({
    page,
  }) => {
    await recordFrames(page);
    await choose(page.context(), { platform: "macos", surface: "cli" });
    await page.goto("/");
    await handedOver(page);
    expect(await frames(page)).toEqual([
      { demo: "start", name: null },
      { demo: null, name: "Pitboard, work 40%, Claude Code" },
    ]);
    await page.locator('figure:has([data-demo-desk]) button[data-surface="app"]').click();
    await expect(page.locator("#hero-mac")).toBeVisible();
    await expect(page.locator(mac.menu)).toBeVisible();
    await expect(page.locator('#hero-mac [data-part="pointer"]')).toHaveCSS("opacity", "0");
  });
});

test.describe("on macOS", () => {
  test.use({ reducedMotion: "reduce" });
  test.skip(({ isMobile }) => isMobile, "a pointer and a keyboard");

  test.beforeEach(async ({ page }) => {
    await hero(page, "macos");
    // a click anywhere else closes the menu the demo left open
    await page.locator("#hero-title").click();
    await expect(page.locator(mac.menu)).toBeHidden();
    await expect(page.locator(mac.item)).toHaveAttribute("aria-expanded", "false");
  });

  test("a press opens the menu, and a release over an account switches to it", async ({ page }) => {
    await hold(page);
    const item = (await page.locator(mac.item).boundingBox())!;
    await page.mouse.move(item.x + item.width / 2, item.y + item.height / 2);
    await page.mouse.down();
    await expect(page.locator(mac.menu)).toBeVisible();
    const personal = page.locator(mac.account("personal"));
    const row = (await personal.boundingBox())!;
    await page.mouse.move(row.x + 30, row.y + row.height / 2, { steps: 4 });
    await expect(personal).toBeFocused();
    await page.mouse.up();
    // it blinks once, then the menu closes and the switch runs
    await expect(personal).toHaveAttribute("data-state", "blink");
    await page.clock.runFor(150);
    await expect(page.locator(mac.menu)).toBeHidden();
    await page.locator(mac.item).click();
    await expect(personal.locator('[data-part="line"]')).toHaveText("Switching…");
    await expect(page.locator(mac.account("spare"))).toHaveAttribute("aria-disabled", "true");
    await page.locator(mac.account("spare")).hover();
    await expect(page.locator(mac.account("spare"))).not.toBeFocused();
    await page.clock.runFor(500);
    await expect(personal).toHaveAttribute("aria-checked", "true");
    await expect(page.locator(mac.item)).toHaveAttribute(
      "aria-label",
      "Pitboard, personal 73%, Claude Code",
    );
    // the one in use is listed first, and the session's status line follows
    await expect(
      page
        .locator('#hero-mac-menu [data-moment="after"] [data-tool="claude"] [data-account]')
        .first(),
    ).toHaveAttribute("data-account", "personal");
    await expect(page.locator('#hero-mac [data-part="status"]')).toContainText("personal 59%");
    await expect(page.locator(heroReset)).toBeVisible();
  });

  test("a press let go off the menu chooses nothing and closes it", async ({ page }) => {
    const item = (await page.locator(mac.item).boundingBox())!;
    await page.mouse.move(item.x + item.width / 2, item.y + item.height / 2);
    await page.mouse.down();
    await page.mouse.move(item.x - 200, item.y + 300, { steps: 4 });
    await page.mouse.up();
    await expect(page.locator(mac.menu)).toBeHidden();
    await expect(page.locator(mac.account("work"))).toHaveAttribute("aria-checked", "true");
  });

  test("a click leaves the menu open, and a click on an item chooses it", async ({ page }) => {
    await page.locator(mac.item).click();
    await expect(page.locator(mac.menu)).toBeVisible();
    await page.locator(mac.account("spare")).click();
    await expect(page.locator(mac.menu)).toBeHidden();
    await expect(page.locator(mac.account("spare"))).toHaveAttribute("aria-checked", "true");
  });

  test("the keyboard opens it on the first item, moves, stops at the ends, types a name and closes", async ({
    page,
  }) => {
    await hold(page);
    await page.locator(mac.item).focus();
    await page.keyboard.press("Enter");
    await expect(page.locator(mac.account("work"))).toBeFocused();
    await page.keyboard.press("ArrowUp");
    await expect(page.locator(mac.account("work"))).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(page.locator(mac.account("personal"))).toBeFocused();
    await page.keyboard.press("o");
    await expect(page.locator(mac.account("office"))).toBeFocused();
    // a letter after a pause starts a name of its own
    await page.clock.runFor(1100);
    await page.keyboard.press("q");
    await expect(page.locator('#hero-mac-menu [data-does="quit"]')).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(page.locator('#hero-mac-menu [data-does="quit"]')).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(page.locator(mac.menu)).toBeHidden();
    await expect(page.locator(mac.item)).toBeFocused();
  });

  test("Home and End go to the first and last item, and typing finds a subtitle", async ({
    page,
  }) => {
    await page.locator(mac.item).focus();
    await page.keyboard.press("Enter");
    await page.keyboard.press("End");
    await expect(page.locator('#hero-mac-menu [data-does="quit"]')).toBeFocused();
    await page.keyboard.press("PageUp");
    await expect(page.locator(mac.account("work"))).toBeFocused();
    await page.keyboard.press("PageDown");
    await expect(page.locator('#hero-mac-menu [data-does="quit"]')).toBeFocused();
    await page.keyboard.press("Home");
    await expect(page.locator(mac.account("work"))).toBeFocused();
    // home's line, No usage read, is the first that starts so
    await page.keyboard.type("no");
    await expect(page.locator(mac.account("home"))).toBeFocused();
  });

  test("Right opens a site's submenu on its first account, and Left closes it", async ({
    page,
  }) => {
    await page.locator(mac.item).focus();
    await page.keyboard.press("ArrowUp");
    await page.keyboard.type("open c");
    const opener = page.locator('#hero-mac-menu [data-does="claude.ai"]:not([data-one])');
    await expect(opener).toBeFocused();
    await page.keyboard.press("ArrowRight");
    const submenu = page.locator('[id="hero-mac-claude.ai-menu"]');
    await expect(submenu).toBeVisible();
    await expect(opener).toHaveAttribute("aria-expanded", "true");
    await expect(submenu.locator("[data-account]").first()).toBeFocused();
    await expect(submenu.locator("[data-account]")).toHaveText(["work", "personal"]);
    await page.keyboard.press("ArrowLeft");
    await expect(submenu).toBeHidden();
    await expect(opener).toBeFocused();
  });

  test("a site's submenu opens as the pointer reaches its item", async ({ page }) => {
    await page.locator(mac.item).click();
    await hold(page);
    await page.locator('#hero-mac-menu [data-does="chatgpt.com"]:not([data-one])').hover();
    const submenu = page.locator('[id="hero-mac-chatgpt.com-menu"]');
    await expect(submenu).toBeVisible();
    // it stays inside the desk, on the side of its item with room
    const desk = (await page.locator("#hero-mac").boundingBox())!;
    const box = (await submenu.boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(desk.x);
    expect(box.x + box.width).toBeLessThanOrEqual(desk.x + desk.width);
  });

  test("home's item has its note as a tooltip, which Escape takes away", async ({ page }) => {
    await page.locator(mac.item).click();
    await hold(page);
    const home = page.locator(mac.account("home"));
    await expect(home).toHaveAccessibleDescription("Read 2h 06m ago, with Claude closed");
    await home.hover();
    const tip = page.locator("#hero-mac-tip-home");
    await page.clock.runFor(900);
    await expect(tip).toBeHidden();
    await page.clock.runFor(200);
    await expect(tip).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(tip).toBeHidden();
    await expect(page.locator(mac.menu)).toBeVisible();
  });

  test("open, with a submenu and a tooltip, the menu keeps WAI-ARIA's rules", async ({ page }) => {
    await page.locator(mac.item).click();
    await page.locator(mac.account("home")).hover();
    await expect(page.locator("#hero-mac-tip-home")).toBeVisible();
    await page.locator('#hero-mac-menu [data-does="claude.ai"]:not([data-one])').click();
    await expect(page.locator('[id="hero-mac-claude.ai-menu"]')).toBeVisible();
    expect(await axe(page, ["#hero-mac"])).toEqual([]);
  });

  test("Quit takes the item off the menu bar until Reset", async ({ page }) => {
    await page.locator(mac.item).click();
    await page.locator('#hero-mac-menu [data-does="quit"]').click();
    await expect(page.locator(mac.item)).toBeHidden();
    await page.locator(heroReset).click();
    await expect(page.locator(mac.item)).toBeVisible();
    await expect(page.locator(mac.item)).toBeFocused();
    await expect(page.locator(mac.account("work"))).toHaveAttribute("aria-checked", "true");
  });

  test("Refresh reads again, dimmed while it reads", async ({ page }) => {
    await page.locator(mac.item).click();
    await hold(page);
    const refresh = page.locator('#hero-mac-menu [data-does="refresh"]');
    await refresh.click();
    await page.clock.runFor(150);
    await page.locator(mac.item).click();
    await expect(refresh).toHaveAttribute("aria-disabled", "true");
    await expect(refresh.locator('[data-part="line"]')).toHaveText("Reading…");
    await page.clock.runFor(500);
    await expect(refresh).not.toHaveAttribute("aria-disabled");
    await expect(refresh.locator('[data-part="line"]')).toHaveText(/^Updated 20:5\d$/);
  });
});

test.describe("on Windows", () => {
  test.use({ reducedMotion: "reduce" });
  test.skip(({ isMobile }) => isMobile, "a pointer and a keyboard");

  test.beforeEach(async ({ page }) => {
    await hero(page, "windows");
    await page.locator("#hero-title").click();
    await expect(page.locator(win.menu)).toBeHidden();
  });

  test("a right click opens the menu on the account in use, and choosing another switches", async ({
    page,
  }) => {
    await page.locator(win.icon).click({ button: "right" });
    await expect(page.locator(win.menu)).toBeVisible();
    await expect(page.locator(win.account("work"))).toBeFocused();
    await page.locator(win.account("spare")).click();
    await expect(page.locator(win.menu)).toBeHidden();
    await expect(page.locator(win.icon)).toHaveAttribute(
      "aria-label",
      /spare \(Codex\): 5-hour 0%/,
    );
    await page.locator(win.icon).click({ button: "right" });
    await expect(page.locator(win.account("spare"))).toHaveAttribute("aria-checked", "true");
    // personal's 73% puts the icon's row at the menu bar's low level
    await page.locator(win.account("personal")).click();
    await expect(page.locator(`${win.icon} [data-level="y"]`)).toBeVisible();
    await expect(page.locator(`${win.icon} [data-level="g"]`)).toBeHidden();
  });

  test("the menu opens with its right edge where the icon was right-clicked, on the taskbar's top", async ({
    page,
  }) => {
    const icon = (await page.locator(win.icon).boundingBox())!;
    const taskbar = (await page
      .locator("#hero-win [data-time]")
      .locator("xpath=../..")
      .boundingBox())!;
    for (const dx of [8, 24]) {
      await page.mouse.click(icon.x + dx, icon.y + icon.height / 2, { button: "right" });
      const menu = (await page.locator(win.menu).boundingBox())!;
      expect(Math.abs(menu.x + menu.width - (icon.x + dx))).toBeLessThan(1);
      await page.keyboard.press("Escape");
    }
    await page.locator(win.icon).focus();
    await page.keyboard.press("Shift+F10");
    const menu = (await page.locator(win.menu).boundingBox())!;
    expect(Math.abs(menu.x + menu.width - (icon.x + icon.width / 2))).toBeLessThan(1);
    expect(Math.abs(menu.y + menu.height - taskbar.y)).toBeLessThan(1);
  });

  test("the icon's tooltip is the Shell's, after its delay, and names it", async ({ page }) => {
    await hold(page);
    await page.locator(win.icon).hover();
    const tip = page.locator('#hero-win [data-part="shell-tip"]');
    await page.clock.runFor(400);
    await expect(tip).toBeHidden();
    await page.clock.runFor(200);
    await expect(tip).toBeVisible();
    await expect(tip).toHaveText(
      "Pitboardwork (Claude Code): 5-hour 12%, weekly 40%main (Codex): Weekly 7%",
    );
    await expect(page.locator(win.icon)).toHaveAccessibleName(
      "Pitboard work (Claude Code): 5-hour 12%, weekly 40% main (Codex): Weekly 7%",
    );
  });

  test("Shift+F10 opens it, arrows wrap past headings, Enter chooses as it comes up, Escape gives focus back", async ({
    page,
  }) => {
    await page.locator(win.icon).focus();
    await page.keyboard.press("Shift+F10");
    await expect(page.locator(win.account("work"))).toBeFocused();
    await page.keyboard.press("ArrowUp");
    await expect(page.locator('#hero-win-menu [data-does="Exit"]')).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(page.locator(win.account("work"))).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    // the Codex heading is skipped
    await expect(page.locator(win.account("main"))).toBeFocused();
    for (const key of ["Tab", "Home", "End", "s"]) {
      await page.keyboard.press(key);
      await expect(page.locator(win.account("main"))).toBeFocused();
    }
    await page.keyboard.press("ArrowDown");
    await page.keyboard.down("Enter");
    await expect(page.locator(win.menu)).toBeVisible();
    await page.keyboard.up("Enter");
    await expect(page.locator(win.menu)).toBeHidden();
    await expect(page.locator(win.icon)).toBeFocused();
    await page.keyboard.press("Shift+F10");
    await page.keyboard.press("Escape");
    await expect(page.locator(win.menu)).toBeHidden();
    await expect(page.locator(win.icon)).toBeFocused();
  });

  test("an account's tooltip shows after WinUI's delay, and a submenu opens after its own", async ({
    page,
  }) => {
    await page.locator(win.icon).click({ button: "right" });
    await hold(page);
    const work = page.locator(win.account("work"));
    await expect(work).toHaveAccessibleDescription("5-hour 12%, weekly 40%");
    await work.hover();
    const tip = page.locator("#hero-win-tip-work");
    await page.clock.runFor(700);
    await expect(tip).toBeHidden();
    await page.clock.runFor(200);
    await expect(tip).toBeVisible();
    await page
      .locator('#hero-win-menu [data-moment="after"] [data-does="claude.ai"]:not([data-one])')
      .hover();
    const submenu = page.locator('[id="hero-win-claude.ai-menu"]');
    await page.clock.runFor(300);
    await expect(tip).toBeHidden();
    await expect(submenu).toBeHidden();
    await page.clock.runFor(200);
    await expect(submenu).toBeVisible();
  });

  test("open, with a submenu and a tooltip, the menu keeps WAI-ARIA's rules", async ({ page }) => {
    await page.locator(win.icon).click({ button: "right" });
    await page.locator(win.account("main")).hover();
    await expect(page.locator("#hero-win-tip-main")).toBeVisible();
    await page
      .locator('#hero-win-menu [data-moment="after"] [data-does="chatgpt.com"]:not([data-one])')
      .click();
    await expect(page.locator('[id="hero-win-chatgpt.com-menu"]')).toBeVisible();
    expect(await axe(page, ["#hero-win"])).toEqual([]);
  });

  test("a click on the icon opens the window: the page's Accounts window comes into view", async ({
    page,
  }) => {
    await page.locator(win.icon).click();
    await expect(page.locator("#switch-win")).toBeInViewport();
  });

  test("Exit takes the icon off the taskbar until Reset", async ({ page }) => {
    await page.locator(win.icon).click({ button: "right" });
    await page.locator('#hero-win-menu [data-does="Exit"]').click();
    await expect(page.locator(win.icon)).toBeHidden();
    await page.locator(heroReset).click();
    await expect(page.locator(win.icon)).toBeVisible();
  });
});

test.describe("on a phone", () => {
  test.use({ reducedMotion: "reduce" });
  test.skip(({ isMobile }) => !isMobile, "a touch screen");

  test("a tap opens the menu and leaves it open, and a tap on an account switches to it", async ({
    page,
  }) => {
    await hero(page, "macos");
    await page.locator("#hero-title").tap();
    await expect(page.locator(mac.menu)).toBeHidden();
    await page.locator(mac.item).tap();
    await expect(page.locator(mac.menu)).toBeVisible();
    await page.locator(mac.account("personal")).tap();
    await expect(page.locator(mac.menu)).toBeHidden();
    await expect(page.locator(mac.item)).toHaveAttribute(
      "aria-label",
      "Pitboard, personal 73%, Claude Code",
    );
  });
});
