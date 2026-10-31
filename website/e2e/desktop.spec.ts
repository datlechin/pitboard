// The GNOME / KDE Plasma control beside the hero's platform tabs (plan 3.1): two pressed buttons in
// a group, shown while Linux is chosen, the choice kept, one state under both desktops.
import type { Page } from "@playwright/test";
import { choose, expect, handedOver, test } from "./fixtures.ts";

const control = (page: Page) => page.locator('[role="group"][aria-label="Linux desktop"]');
const button = (page: Page, name: "GNOME" | "KDE Plasma") =>
  control(page).getByRole("button", { name });

test.describe("the desktop control", () => {
  test.use({ reducedMotion: "reduce" });

  test("shows with Linux alone, in the hero's install tabs, GNOME pressed", async ({ page }) => {
    await choose(page.context(), { platform: "macos" });
    await page.goto("/");
    await expect(control(page)).toHaveCount(1);
    await expect(control(page)).toBeHidden();
    await page.locator('[role="tab"][data-platform="linux"]').first().click();
    await expect(control(page)).toBeVisible();
    await expect(button(page, "GNOME")).toHaveAttribute("aria-pressed", "true");
    await expect(button(page, "KDE Plasma")).toHaveAttribute("aria-pressed", "false");
    await expect(page.locator("html")).toHaveAttribute("data-desktop", "gnome");
    // at the end of the hero's platform tabs' row, and nowhere else
    const row = control(page).locator("xpath=../..");
    await expect(row.getByRole("tablist", { name: "Install Pitboard" })).toBeVisible();
    expect(await control(page).evaluate((el) => el.closest("section")?.id ?? null)).toBe(
      await page
        .locator("figure:has([data-demo-desk])")
        .evaluate((el) => el.closest("section")?.id ?? null),
    );
  });

  test("Tab reaches each button, Enter and Space press it, and the press is kept", async ({
    page,
    isMobile,
    browserName,
  }) => {
    test.skip(isMobile, "a keyboard");
    test.skip(
      process.platform === "darwin" && browserName === "webkit",
      "WebKit on a Mac tabs past buttons, as Safari does until Option-Tab",
    );
    await choose(page.context(), { platform: "linux" });
    await page.goto("/");
    await button(page, "GNOME").focus();
    await page.keyboard.press("Tab");
    await expect(button(page, "KDE Plasma")).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(button(page, "KDE Plasma")).toHaveAttribute("aria-pressed", "true");
    await expect(button(page, "GNOME")).toHaveAttribute("aria-pressed", "false");
    await expect(page.locator("html")).toHaveAttribute("data-desktop", "kde");
    await expect(page.locator("#hero-kde")).toBeVisible();
    await expect(page.locator("#hero-gnome")).toBeHidden();
    await page.reload();
    await expect(page.locator("html")).toHaveAttribute("data-desktop", "kde");
    await expect(button(page, "KDE Plasma")).toHaveAttribute("aria-pressed", "true");
    await button(page, "GNOME").focus();
    await page.keyboard.press("Space");
    await expect(page.locator("html")).toHaveAttribute("data-desktop", "gnome");
    await expect(page.locator("#hero-gnome")).toBeVisible();
  });

  test("a store that refuses leaves GNOME, and the press still redraws", async ({ page }) => {
    await page.context().addInitScript(() => {
      sessionStorage.setItem("platform", "linux");
      const refuse = () => {
        throw new DOMException("refused", "SecurityError");
      };
      Storage.prototype.getItem = function (key: string) {
        return key === "platform" ? "linux" : refuse();
      };
      Storage.prototype.setItem = refuse;
    });
    await page.goto("/");
    await expect(page.locator("html")).toHaveAttribute("data-desktop", "gnome");
    await button(page, "KDE Plasma").click();
    await expect(page.locator("html")).toHaveAttribute("data-desktop", "kde");
  });

  test("a switch made under GNOME shows in KDE Plasma's tray, and the demo plays no more", async ({
    page,
    isMobile,
  }) => {
    test.skip(isMobile, "the hero's window takes a pointer");
    await choose(page.context(), { system: "gnome" });
    await page.goto("/");
    await handedOver(page);
    const window = page.locator("#hero-gnome-window");
    await window.locator('[data-account="personal"] [data-does="use"]').click();
    await expect(window.locator('[data-account="personal"] [data-part="in-use"]')).toBeVisible({
      timeout: 4000,
    });
    await button(page, "KDE Plasma").click();
    await expect(page.locator("html")).not.toHaveAttribute("data-demo");
    const icon = page.locator("#hero-kde-icon");
    await expect(icon).toHaveAccessibleDescription(/^Pitboard\s*personal \(Claude Code\)/);
    await icon.click({ button: "right" });
    await expect(
      page.locator('#hero-kde-menu [data-moment="after"] [data-account="personal"]'),
    ).toHaveAttribute("aria-checked", "true");
    await expect(page.locator('#hero-kde [data-part="status"]')).toContainText("personal 59%");
  });

  test("on a 320 pixel phone the control fits its box", async ({ page, isMobile }) => {
    test.skip(!isMobile, "a phone's width");
    await page.setViewportSize({ width: 320, height: 640 });
    await choose(page.context(), { platform: "linux" });
    await page.goto("/");
    const box = (await control(page).boundingBox())!;
    expect(box.width).toBeLessThanOrEqual(288);
    expect(box.x).toBeGreaterThanOrEqual(16 - 1e-3);
    expect(box.x + box.width).toBeLessThanOrEqual(320 - 16 + 1e-3);
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
      320,
    );
  });
});

test.describe("with motion", () => {
  test.skip(({ isMobile }) => isMobile, "the hero is below the fold on a phone");

  test("the demo plays once under GNOME, and KDE Plasma's desk shows where it ended", async ({
    page,
  }) => {
    await choose(page.context(), { system: "gnome" });
    await page.clock.install();
    await page.goto("/");
    await expect(page.locator("html")).toHaveAttribute("data-demo", "move");
    await page.clock.runFor(4000);
    await expect(page.locator("html")).not.toHaveAttribute("data-demo");
    await expect(
      page.locator('#hero-gnome-window [data-account="work"] [data-part="in-use"]'),
    ).toBeVisible();
    await button(page, "KDE Plasma").click();
    await page.clock.runFor(4000);
    await expect(page.locator("html")).not.toHaveAttribute("data-demo");
    await expect(page.locator("#hero-kde-icon")).toHaveAccessibleDescription(
      /^Pitboard\s*work \(Claude Code\)/,
    );
  });
});
