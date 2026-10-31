// The hero's desks: the demo that plays once and hands over, then Pitboard's macOS menu as NSMenu
// runs it, its Windows icon menu as WinUI's MenuFlyout runs it, its window on stock GNOME, which
// shows no status item, and its status item's QMenu in KDE Plasma's tray (plan 3.1 to 3.3, section
// 4).
import type { Page } from "@playwright/test";
import { axe, choose, expect, handedOver, test, type System } from "./fixtures.ts";

const mac = {
  item: "#hero-mac-item",
  menu: "#hero-mac-menu",
  // an account's item in the moment the state draws
  account: (id: string) => `#hero-mac-menu [data-moment="after"] [data-account="${id}"]`,
};
const gnome = {
  window: "#hero-gnome-window",
  group: (id: string) => `#hero-gnome-window [data-account="${id}"]`,
};
const kde = {
  icon: "#hero-kde-icon",
  menu: "#hero-kde-menu",
  tip: "#hero-kde-tip",
  account: (id: string) => `#hero-kde-menu [data-moment="after"] [data-account="${id}"]`,
  does: (does: string) => `#hero-kde-menu [data-moment="after"] [data-does="${does}"]`,
};
const win = {
  icon: "#hero-win-icon",
  menu: "#hero-win-menu",
  open: '#hero-win-menu [data-moment="after"] [data-does="Open Pitboard"]',
  account: (id: string) =>
    `#hero-win-menu [data-moment="after"] [data-tool] [data-account="${id}"]`,
};

async function hero(page: Page, system: System) {
  await choose(page.context(), { system });
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

// the frames the demo set on the root, with what `probe` read of a desk as each began
async function recordStates(page: Page, probe: () => string) {
  await page.addInitScript((probe) => {
    const read = new Function(`return (${probe})()`) as () => string;
    const seen: { demo: string | null; state: string }[] = [];
    (window as unknown as { states_: typeof seen }).states_ = seen;
    new MutationObserver(() =>
      seen.push({ demo: document.documentElement.getAttribute("data-demo"), state: read() }),
    ).observe(document, { subtree: true, attributeFilter: ["data-demo"] });
  }, probe.toString());
}
const states = (page: Page) =>
  page.evaluate(() => (window as unknown as { states_: unknown[] }).states_);

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

  // the app's menu carries no tooltips, which the keyboard never showed
  test("open, with a submenu, the menu keeps WAI-ARIA's rules", async ({ page }) => {
    await page.locator(mac.item).click();
    await expect(page.locator(mac.account("home"))).not.toHaveAttribute("aria-describedby");
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

  test("a switch of Codex badges the item and opens the menu on 1 Issue, which shows the window", async ({
    page,
  }) => {
    const item = page.locator(mac.item);
    await expect(item.locator('[data-part="badged"]')).toBeHidden();
    await item.click();
    await page.locator(mac.account("spare")).click();
    await expect(item).toHaveAccessibleName(/, needs attention$/);
    await expect(item.locator('[data-part="badged"]')).toBeVisible();
    await expect(item.locator('[data-part="mark"]')).toBeHidden();
    await item.click();
    const issues = page.locator('#hero-mac-menu [data-does="issues"]');
    await expect(issues).toHaveText(/^\s*1 Issue\s+Switched Codex to spare\s*$/);
    await issues.click();
    await expect(page.locator("#switch-mac")).toBeInViewport();
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

  test("a right click opens the menu on Open Pitboard, and choosing another account switches", async ({
    page,
  }) => {
    await page.locator(win.icon).click({ button: "right" });
    await expect(page.locator(win.menu)).toBeVisible();
    await expect(page.locator(win.open)).toBeFocused();
    await expect(page.locator(win.open)).toHaveCSS("font-weight", "600");
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
      "Pitboardwork (Claude Code): 5-hour 12% used, weekly 40%main (Codex): weekly 7% used",
    );
    await expect(page.locator(win.icon)).toHaveAccessibleName(
      "Pitboard work (Claude Code): 5-hour 12% used, weekly 40% main (Codex): weekly 7% used",
    );
  });

  test("Shift+F10 opens it, arrows wrap past headings, Enter chooses as it comes up, Escape gives focus back", async ({
    page,
  }) => {
    await page.locator(win.icon).focus();
    await page.keyboard.press("Shift+F10");
    await expect(page.locator(win.open)).toBeFocused();
    await page.keyboard.press("ArrowUp");
    await expect(page.locator('#hero-win-menu [data-does="Exit"]')).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(page.locator(win.open)).toBeFocused();
    // each tool's heading is skipped
    await page.keyboard.press("ArrowDown");
    await expect(page.locator(win.account("work"))).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
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
    await expect(work).toHaveAccessibleDescription("5-hour 12% used, weekly 40%");
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

  test("a switch of Codex makes the icon say it needs attention", async ({ page }) => {
    await page.locator(win.icon).click({ button: "right" });
    await page.locator(win.account("spare")).click();
    await expect(page.locator(win.icon)).toHaveAccessibleName(/^Pitboard\s+Needs attention\s/);
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

test.describe("on GNOME", () => {
  test.skip(({ isMobile }) => isMobile, "a pointer and a keyboard");

  test("the demo presses Switch beside work, its spinner turns until the switch is made, and hands over", async ({
    page,
  }) => {
    await recordStates(page, () => {
      const group = (id: string) =>
        document.querySelector(`#hero-gnome-window [data-account="${id}"]`)!;
      const on = (id: string, part: string) =>
        (group(id)?.querySelector(`[data-part="${part}"]`) as HTMLElement | null)?.hidden === false;
      return on("work", "switching") ? "switching" : on("work", "in-use") ? "work" : "personal";
    });
    await choose(page.context(), { system: "gnome" });
    await page.clock.install();
    await page.goto("/");
    await expect(page.locator("html")).not.toHaveAttribute("data-demo", { timeout: 8000 });
    // the switch takes Plasma's 1.06 s, from the pick past the pointer's reach
    expect(await states(page)).toEqual([
      { demo: "start", state: "personal" },
      { demo: "move", state: "personal" },
      { demo: "hl", state: "personal" },
      { demo: "pick", state: "switching" },
      { demo: "shut", state: "switching" },
      { demo: "reach", state: "switching" },
      { demo: null, state: "work" },
    ]);
    await expect(page.locator(gnome.window).locator("[data-notice]:not([hidden])")).toContainText(
      "Switched Claude Code to work",
    );
    await expect(page.locator('#hero-gnome [data-part="status"]')).toContainText("work 12%");
    await expect(page.locator('#hero-gnome [data-part="pointer"]')).toHaveCSS("opacity", "0");
    await expect(page.locator(heroReset)).toBeHidden();
  });

  test.describe("under reduced motion", () => {
    test.use({ reducedMotion: "reduce" });

    test("is not played, and the switch is made at once", async ({ page }) => {
      await recordFrames(page);
      await hero(page, "gnome");
      expect(await frames(page)).toEqual([]);
      await expect(page.locator(gnome.group("work")).locator('[data-part="in-use"]')).toBeVisible();
      await expect(
        page.locator(gnome.group("work")).getByRole("img", { name: "Switching" }),
      ).toBeHidden();
    });
  });

  test.describe("after the demo", () => {
    test.use({ reducedMotion: "reduce" });

    test.beforeEach(async ({ page }) => {
      await hero(page, "gnome");
      await expect(page.locator(gnome.group("work")).locator('[data-part="in-use"]')).toBeVisible();
    });

    test("Switch beside personal shows the spinner, holds the other buttons back, then switches", async ({
      page,
    }) => {
      await hold(page);
      const personal = page.locator(gnome.group("personal"));
      const button = personal.getByRole("button", { name: "Switch to personal" });
      await expect(button).toHaveText("Switch");
      await button.click();
      await expect(personal.getByRole("img", { name: "Switching" })).toBeVisible();
      await page.clock.runFor(1000);
      await expect(personal.locator('[data-part="in-use"]')).toBeHidden();
      await page.clock.runFor(200);
      await expect(personal.locator('[data-part="in-use"]')).toBeVisible();
      await expect(page.locator(gnome.window).locator("[data-notice]:not([hidden])")).toContainText(
        "Switched Claude Code to personal",
      );
      await expect(page.locator('#hero-gnome [data-part="status"]')).toContainText("personal 59%");
      await expect(page.locator(heroReset)).toBeVisible();
    });

    test("Close quits Pitboard, which no status item keeps, until Reset", async ({ page }) => {
      const window = page.locator(gnome.window);
      await window.getByRole("button", { name: "Close" }).click();
      await expect(window).toBeHidden();
      await page.locator(heroReset).click();
      await expect(window).toBeVisible();
      await expect(window.getByRole("button", { name: "Add Account…" })).toBeFocused();
    });

    test("the window draws no Minimize or Maximize, as GNOME's layout has Close alone", async ({
      page,
    }) => {
      const window = page.locator(gnome.window);
      await expect(window.getByRole("button", { name: "Minimize" })).toBeHidden();
      await expect(window.getByRole("button", { name: "Maximize" })).toBeHidden();
      await expect(window.getByRole("button", { name: "Close" })).toBeVisible();
      expect(await axe(page, ["#hero-gnome"])).toEqual([]);
    });

    test("the clock opens the date menu, and Escape gives focus back to it", async ({ page }) => {
      const clock = page.locator('#hero-gnome [data-does="center"]');
      const menu = page.locator('#hero-gnome [data-part="center"]');
      await clock.click();
      await expect(menu).toBeVisible();
      await expect(clock).toHaveAttribute("aria-expanded", "true");
      await expect(menu).toContainText("No Notifications");
      await page.keyboard.press("Escape");
      await expect(menu).toBeHidden();
      await expect(clock).toBeFocused();
    });
  });
});

test.describe("on KDE Plasma", () => {
  test.skip(({ isMobile }) => isMobile, "a pointer and a keyboard");

  test("the demo opens the tray's menu, chooses work, shuts it, and the tooltip names work", async ({
    page,
  }) => {
    await recordStates(page, () => {
      const tip = document.getElementById("hero-kde-tip");
      const work = document.querySelector(
        '#hero-kde-menu [data-moment="after"] [data-account="work"]',
      );
      const chosen = work?.getAttribute("aria-checked") === "true" ? "work" : "personal";
      return tip?.checkVisibility() ? `tip ${chosen}` : chosen;
    });
    await choose(page.context(), { system: "kde" });
    await page.clock.install();
    await page.goto("/");
    await expect(page.locator("html")).not.toHaveAttribute("data-demo", { timeout: 8000 });
    expect(await states(page)).toEqual([
      { demo: "start", state: "personal" },
      { demo: "move", state: "personal" },
      { demo: "hl", state: "personal" },
      { demo: "pick", state: "work" },
      { demo: "shut", state: "work" },
      { demo: "reach", state: "tip work" },
      { demo: null, state: "work" },
    ]);
    // it hands over with the menu open on work, in use
    await expect(page.locator(kde.menu)).toBeVisible();
    await expect(page.locator(kde.account("work"))).toHaveAttribute("aria-checked", "true");
    await expect(page.locator(kde.icon)).toHaveAccessibleDescription(
      /^Pitboard\s*work \(Claude Code\): 5-hour 12% used, weekly 40%/,
    );
    await expect(page.locator('#hero-kde [data-part="status"]')).toContainText("work 12%");
    await expect(page.locator(heroReset)).toBeHidden();
  });

  test.describe("under reduced motion", () => {
    test.use({ reducedMotion: "reduce" });

    test("is not played, the switch is made at once, and a hover shows the tooltip", async ({
      page,
    }) => {
      await recordFrames(page);
      await hero(page, "kde");
      expect(await frames(page)).toEqual([]);
      await page.locator("#hero-title").click();
      await page.locator(kde.icon).hover();
      await expect(page.locator(kde.tip)).toBeVisible();
      await expect(page.locator(kde.tip)).toContainText("work (Claude Code)");
    });
  });

  test.describe("after the demo", () => {
    test.use({ reducedMotion: "reduce" });

    test.beforeEach(async ({ page }) => {
      await hero(page, "kde");
      await page.locator("#hero-title").click();
      await expect(page.locator(kde.menu)).toBeHidden();
    });

    test("a right click opens the menu over the panel, and an account's radio switches to it", async ({
      page,
    }) => {
      await hold(page);
      await page.locator(kde.icon).click({ button: "right" });
      await expect(page.locator(kde.menu)).toBeVisible();
      await expect(page.locator(kde.account("work"))).toHaveAttribute("aria-checked", "true");
      await expect(page.locator("#hero-kde-after-claude")).toHaveAttribute("aria-disabled", "true");
      const icon = (await page.locator(kde.icon).boundingBox())!;
      const menu = (await page.locator(kde.menu).boundingBox())!;
      expect(menu.y + menu.height).toBeLessThanOrEqual(icon.y + 1e-3);
      await page.locator(kde.account("personal")).click();
      await expect(page.locator(kde.menu)).toBeHidden();
      await page.clock.runFor(1100);
      await page.locator(kde.icon).click({ button: "right" });
      await expect(page.locator(kde.account("personal"))).toHaveAttribute("aria-checked", "true");
      await expect(page.locator(kde.account("work"))).toHaveAttribute("aria-checked", "false");
      await expect(page.locator('#hero-kde [data-part="status"]')).toContainText("personal 59%");
      // personal's 73% puts the icon at its low level
      await expect(page.locator(`${kde.icon} [data-level="y"]:not([hidden])`)).toHaveCount(1);
    });

    test("Shift+F10 opens it from the keyboard, arrows wrap, Tab moves as Down, Space does nothing, Escape gives focus back", async ({
      page,
    }) => {
      const icon = page.locator(kde.icon);
      await expect(icon).toHaveAttribute("aria-keyshortcuts", "Shift+F10");
      await expect(icon).toHaveAttribute("aria-expanded", "false");
      await icon.focus();
      await page.keyboard.press("Shift+F10");
      await expect(page.locator(kde.menu)).toBeVisible();
      await expect(icon).toHaveAttribute("aria-expanded", "true");
      // a QMenu opened from the keyboard has no item current until an arrow makes one
      await page.keyboard.press("ArrowDown");
      await expect(page.locator(kde.does("open"))).toBeFocused();
      await page.keyboard.press("ArrowUp");
      await expect(page.locator(kde.does("quit"))).toBeFocused();
      await page.keyboard.press("ArrowDown");
      await expect(page.locator(kde.does("open"))).toBeFocused();
      await page.keyboard.press("Tab");
      await expect(page.locator(kde.account("work"))).toBeFocused();
      await page.keyboard.press("End");
      await expect(page.locator(kde.does("quit"))).toBeFocused();
      await page.keyboard.press("Home");
      await expect(page.locator(kde.does("open"))).toBeFocused();
      await page.keyboard.press("ArrowDown");
      await page.keyboard.press("ArrowDown");
      await expect(page.locator(kde.account("personal"))).toBeFocused();
      await page.keyboard.press("Space");
      await expect(page.locator(kde.menu)).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(page.locator(kde.menu)).toBeHidden();
      await expect(icon).toBeFocused();
      await expect(icon).toHaveAttribute("aria-expanded", "false");
      await expect(page.locator(kde.account("work"))).toHaveAttribute("aria-checked", "true");
    });

    test("the tooltip comes after Plasma's delay and names the accounts in use", async ({
      page,
    }) => {
      await hold(page);
      await page.locator(kde.icon).hover();
      const tip = page.locator(kde.tip);
      await page.clock.runFor(600);
      await expect(tip).toBeHidden();
      await page.clock.runFor(200);
      await expect(tip).toBeVisible();
      await expect(tip.locator('[data-part="title"]')).toHaveText("Pitboard");
      await expect(tip.locator('[data-part="text"]')).toHaveText(
        /^\s*work \(Claude Code\): 5-hour 12% used, weekly 40%\s+main \(Codex\): weekly 7% used\s*$/,
      );
      await page.keyboard.press("Escape");
      await expect(tip).toBeHidden();
    });

    test("a left click is Activate: the window's figure comes into view", async ({ page }) => {
      await page.locator(kde.icon).click();
      await expect(page.locator("#switch-linux")).toBeInViewport();
    });

    test("a switch of Codex makes the icon ask for attention, its tooltip say so, and its menu offer 1 Issue, which shows the window", async ({
      page,
    }) => {
      const issues = page.locator(kde.does("issues"));
      await page.locator(kde.icon).click({ button: "right" });
      await expect(issues).toBeHidden();
      await page.locator(kde.account("spare")).click();
      await expect(page.locator(kde.icon)).toHaveAttribute("data-attention", "true", {
        timeout: 4000,
      });
      await expect(page.locator(kde.icon)).toHaveAccessibleDescription(/Needs attention/);
      await page.locator(kde.icon).click({ button: "right" });
      // presence.rs puts the notices between Open Pitboard and the accounts
      await expect(page.locator(`${kde.menu} [role="menuitem"]:visible`).nth(1)).toHaveText(
        "1 Issue",
      );
      await issues.click();
      await expect(page.locator(kde.menu)).toBeHidden();
      await expect(page.locator("#switch-linux")).toBeInViewport();
    });

    test("open, the menu keeps WAI-ARIA's rules", async ({ page }) => {
      await page.locator(kde.icon).click({ button: "right" });
      await expect(page.locator(kde.menu)).toBeVisible();
      expect(await axe(page, ["#hero-kde"])).toEqual([]);
    });

    test("Quit takes the icon out of the tray until Reset", async ({ page }) => {
      await page.locator(kde.icon).click({ button: "right" });
      await page.locator(kde.does("quit")).click();
      await expect(page.locator(kde.icon)).toBeHidden();
      await page.locator(heroReset).click();
      await expect(page.locator(kde.icon)).toBeVisible();
    });
  });
});
