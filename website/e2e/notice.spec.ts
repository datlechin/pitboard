// The notification the app posts once a limit reaches the share, on each system: macOS's banner,
// Windows' toast and Notification Center, GNOME Shell's banner and date menu, and KDE Plasma's popup
// over its tray (plan 3.3 "A warning first", 3.6).
import type { Page } from "@playwright/test";
import { axe, choose, expect, handedOver, test, type System } from "./fixtures.ts";

const at = new Date("2026-10-13T13:51:00Z");
const room = "personal has used 95%. work has used 40%.";
const heads: Record<System, string> = {
  macos: "Weekly Limit Almost Reached",
  windows: "Weekly limit almost reached",
  gnome: "Weekly Limit Almost Reached",
  kde: "Weekly Limit Almost Reached",
};
const switches: Record<System, string> = {
  macos: "Switch",
  windows: "Switch to work",
  gnome: "Switch to work",
  kde: "Switch to work",
};
const backs: Record<System, string> = {
  macos: "Switch Back",
  windows: "Switch back",
  gnome: "Switch Back",
  kde: "Switch Back",
};
const ids: Record<System, string> = {
  macos: "offer-mac",
  windows: "offer-win",
  gnome: "offer-gnome",
  kde: "offer-kde",
};
// each system's figure of When a limit reaches, and the system's own time a banner stays
const limits: Record<System, string> = {
  macos: "auto-mac",
  windows: "auto-win",
  gnome: "auto-linux",
  kde: "auto-linux",
};
const shownMs: Record<System, number> = { macos: 5000, windows: 5000, gnome: 4000, kde: 5000 };

async function offer(page: Page, system: System) {
  await choose(page.context(), { system });
  await page.clock.install({ time: at });
  await page.goto("/");
  await page.waitForFunction(() => customElements.get("pb-notice") !== undefined);
  return page.locator(`#${ids[system]}`);
}

// The share set to 90%, and then Switch Automatically chosen, in the system's own controls.
async function setShare(page: Page, system: System, share: string) {
  const id = limits[system];
  const fig = page.locator(`#${id}`);
  await fig.scrollIntoViewIfNeeded();
  await expect(fig).not.toHaveAttribute("inert");
  const field =
    system === "gnome" || system === "kde"
      ? page.locator(`#${id}-preferences [data-part="share"] input`)
      : page.locator(`#${id}-share`);
  await field.fill(share);
  await field.press("Enter");
}

async function chooseSwitch(page: Page, system: System) {
  const id = limits[system];
  await page.locator(`#${id}`).scrollIntoViewIfNeeded();
  if (system === "macos") {
    await page.locator(`#${id}-at`).focus();
    await page.keyboard.press("Space");
    await expect(page.locator(`#${id}-choices`)).toBeVisible();
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
  } else if (system === "windows") {
    await page.locator(`#${id}-at`).focus();
    await page.keyboard.press("ArrowDown");
  } else {
    await page.locator(`#${id}-preferences [data-part="choice"]`).click();
    await page.locator(`#${id}-at-limit-switch`).click();
  }
}

for (const system of ["macos", "windows", "gnome", "kde"] as const) {
  test.describe(`the notification on ${system}`, () => {
    test("arrives once its row is in view, stays while held, and goes after the system's time", async ({
      page,
    }) => {
      const fig = await offer(page, system);
      const banner = fig.locator("[data-banner]");
      await expect(banner).toBeHidden();
      await fig.scrollIntoViewIfNeeded();
      await expect(banner).toBeVisible();
      await expect(banner).toHaveAccessibleName(heads[system]);
      await expect(page.locator("#status")).toHaveText(`${heads[system]}, Claude Code, ${room}`);
      await expect(banner.locator("[data-switch]")).toHaveText(switches[system]);
      // focus inside it holds it, and shows its close button and Switch
      await banner.locator("[data-close]").focus();
      await expect(banner).toHaveAttribute("data-state", "held");
      await expect(banner.locator("[data-close]")).toBeVisible();
      await page.clock.fastForward(8000);
      await expect(banner).toBeVisible();
      await banner.locator("[data-switch]").focus();
      await page.keyboard.press("Enter");
      await expect(banner).toBeHidden();
      // Switch acts in the row's moment
      if (system === "macos") await expect(fig.locator("[data-title]")).toHaveText("work 40%");
      else if (system !== "gnome")
        await expect(fig.locator("[data-level=g]:not([hidden])")).toBeVisible();
      // the same notification does not come again
      await page.evaluate(() => scrollTo(0, 0));
      await fig.scrollIntoViewIfNeeded();
      await page.clock.runFor(500);
      await expect(banner).toBeHidden();
    });

    test("goes by itself after the system's time, once nothing holds it", async ({ page }) => {
      const fig = await offer(page, system);
      const banner = fig.locator("[data-banner]");
      // the clock moves only as the test moves it, so a slow browser takes none of the banner's time
      await page.clock.pauseAt((await page.evaluate(() => Date.now())) + 1000);
      await fig.scrollIntoViewIfNeeded();
      await expect(banner).toBeVisible();
      await page.mouse.move(0, 0);
      await page.clock.runFor(shownMs[system] - 300);
      await expect(banner).toBeVisible();
      await page.clock.runFor(600);
      await expect(banner).toBeHidden();
    });

    test("follows the choice and the share", async ({ page }) => {
      await offer(page, system);
      await handedOver(page);
      await setShare(page, system, "90");
      const fig = page.locator(`#${ids[system]}`);
      const banner = fig.locator("[data-banner]");
      await fig.scrollIntoViewIfNeeded();
      await expect(banner).toHaveAccessibleName(heads[system]);
      await expect(banner.locator("[data-body]")).toHaveText(
        "personal has used 90%. work has used 40%.",
      );
      await page.mouse.move(0, 0);
      await page.clock.fastForward(5500);
      await expect(banner).toBeHidden();
      await chooseSwitch(page, system);
      await fig.scrollIntoViewIfNeeded();
      await expect(banner).toHaveAccessibleName("Switched to work");
      await expect(banner.locator("[data-switch]")).toHaveText(backs[system]);
      await expect(banner.locator("[data-subtitle]")).toHaveText("Claude Code");
    });
  });
}

for (const system of ["gnome", "kde"] as const)
  test(`on ${system}, a click on the notification opens the window: its figure comes into view`, async ({
    page,
  }) => {
    const fig = await offer(page, system);
    // the demo is over first: on the iPhone the press ended it, and the page moved under the pointer
    await handedOver(page);
    const banner = fig.locator("[data-banner]");
    await fig.scrollIntoViewIfNeeded();
    await banner.locator("[data-body]").click();
    await expect(banner).toBeHidden();
    await expect(page.locator("#switch-linux")).toBeInViewport();
  });

test.describe("Notification Center", () => {
  test.skip(({ isMobile }) => isMobile, "a pointer and a keyboard");

  test("on Windows the clock opens it, where the notification waits once its toast has gone", async ({
    page,
  }) => {
    const fig = await offer(page, "windows");
    const center = fig.locator('[data-part="center"]');
    const clock = fig.getByRole("button", { name: "05:53 14/10/2026" });
    await fig.scrollIntoViewIfNeeded();
    await expect(fig.locator("[data-banner]")).toBeVisible();
    await page.mouse.move(0, 0);
    await page.clock.fastForward(5500);
    await expect(fig.locator("[data-banner]")).toBeHidden();
    await clock.click();
    await expect(center).toBeVisible();
    await expect(clock).toHaveAttribute("aria-expanded", "true");
    await expect(center).toBeFocused();
    const note = center.locator('[data-part="note"]');
    await expect(note).toHaveAccessibleName(heads.windows);
    await expect(note).toContainText(room);
    await expect(note.getByRole("button", { name: "Switch to work" })).toBeVisible();
    await expect(center).toContainText("Wednesday 14 October");
    await page.keyboard.press("Escape");
    await expect(center).toBeHidden();
    await expect(clock).toBeFocused();
    // dismissed, it is gone
    await clock.click();
    await note.getByRole("button", { name: "Dismiss" }).click();
    await expect(center).toContainText("No new notifications");
    await expect(center.getByRole("button", { name: "Clear all" })).toBeHidden();
    expect(await axe(page, ["#offer-win"])).toEqual([]);
  });

  test("on macOS, which no reading of it drew, the clock opens nothing", async ({ page }) => {
    const fig = await offer(page, "macos");
    await fig.scrollIntoViewIfNeeded();
    await expect(fig.locator('[data-part="center"]')).toHaveCount(0);
    await expect(fig.locator("[data-clock]")).not.toHaveRole("button");
  });

  test("on GNOME the clock opens the date menu, the notification under Pitboard, and Escape gives focus back", async ({
    page,
  }) => {
    const fig = await offer(page, "gnome");
    const menu = fig.locator('[data-part="center"]');
    const clock = fig.locator('[data-does="center"]');
    await expect(clock).toHaveAccessibleName("14 Oct 05:53");
    await fig.scrollIntoViewIfNeeded();
    await expect(fig.locator("[data-banner]")).toBeVisible();
    await clock.click();
    // the date menu's opening puts the banner away
    await expect(fig.locator("[data-banner]")).toBeHidden();
    await expect(menu).toBeVisible();
    await expect(clock).toHaveAttribute("aria-expanded", "true");
    await expect(menu).toBeFocused();
    const note = menu.locator('[data-part="note"]');
    await expect(note).toHaveAccessibleName(heads.gnome);
    await expect(note).toContainText("Pitboard");
    await expect(note).toContainText(room);
    await expect(menu).toContainText("Wednesday");
    await expect(menu).toContainText("14 October 2026");
    // Expand shows its button
    const expand = note.getByRole("button", { name: "Expand" });
    await expect(note.getByRole("button", { name: "Switch to work" })).toBeHidden();
    await expand.click();
    await expect(expand).toHaveAttribute("aria-expanded", "true");
    await expect(note.getByRole("button", { name: "Switch to work" })).toBeVisible();
    expect(await axe(page, ["#offer-gnome"])).toEqual([]);
    await page.keyboard.press("Escape");
    await expect(menu).toBeHidden();
    await expect(clock).toHaveAttribute("aria-expanded", "false");
    await expect(clock).toBeFocused();
    // closed, it is gone, and the list says so
    await clock.click();
    await note.getByRole("button", { name: "Close" }).click();
    await expect(menu).toContainText("No Notifications");
    await expect(menu.getByRole("button", { name: "Clear all notifications" })).toBeHidden();
    // a press elsewhere closes the menu
    await page.locator("#offer-title").click();
    await expect(menu).toBeHidden();
  });

  test("on KDE Plasma the bell shows in the tray while the notification is unread", async ({
    page,
  }) => {
    const fig = await offer(page, "kde");
    const bell = fig.locator('[data-part="bell"]');
    await expect(bell).toBeHidden();
    await fig.scrollIntoViewIfNeeded();
    await expect(fig.locator("[data-banner]")).toBeVisible();
    await expect(bell).toBeVisible();
    await fig.locator("[data-banner] [data-close]").click();
    await expect(fig.locator("[data-banner]")).toBeHidden();
    await expect(bell).toBeVisible();
    // Plasma's history is not drawn: the bell opens nothing
    await expect(bell).not.toHaveRole("button");
    expect(await axe(page, ["#offer-kde"])).toEqual([]);
  });
});

test.describe("under reduced motion", () => {
  test.use({ reducedMotion: "reduce" });

  for (const system of ["gnome", "kde"] as const)
    test(`on ${system}, the notification comes with no movement`, async ({ page }) => {
      const fig = await offer(page, system);
      const banner = fig.locator("[data-banner]");
      await fig.scrollIntoViewIfNeeded();
      await expect(banner).toBeVisible();
      const durations = await banner.evaluate((el) =>
        getComputedStyle(el).transitionDuration.split(", "),
      );
      expect(durations.every((d) => d === "0s")).toBe(true);
    });
});
