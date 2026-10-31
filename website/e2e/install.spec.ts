// The hero's one download for the system its tabs choose, and the ARM64 installer for an Arm PC.
import type { Page } from "@playwright/test";
import { choose, expect, platforms, test, type Platform } from "./fixtures.ts";

test.use({ reducedMotion: "reduce" });

const hero = (page: Page) => page.locator('section[aria-labelledby="hero-title"]');
const action: Record<Platform, RegExp> = {
  macos: /^Download for Mac/,
  windows: /^Download for Windows/,
  linux: /^Install on Linux$/,
};

// a Windows browser that names its machine when asked, as Edge and Chrome do
async function onWindows(page: Page, architecture: string) {
  await page.addInitScript((architecture) => {
    Object.defineProperty(Navigator.prototype, "userAgentData", {
      configurable: true,
      get: () => ({
        platform: "Windows",
        getHighEntropyValues: () => Promise.resolve({ architecture, bitness: "64" }),
      }),
    });
  }, architecture);
}

test("the hero's tabs choose its one download, and the figures follow", async ({ page }) => {
  await choose(page.context(), { platform: "macos" });
  await page.goto("/");
  const tabs = hero(page).getByRole("tablist", { name: "Install Pitboard" });
  for (const platform of ["macos", "windows", "linux"] as const) {
    await tabs.locator(`[data-platform="${platform}"]`).click();
    await expect(page.locator("html")).toHaveAttribute("data-platform", platform);
    for (const other of platforms) {
      const link = hero(page).getByRole("link", { name: action[other] });
      if (other === platform) await expect(link).toBeVisible();
      else await expect(link).toBeHidden();
    }
  }
  await expect(hero(page).getByRole("link", { name: action.linux })).toHaveAttribute(
    "href",
    "/download#linux",
  );
  await expect(
    hero(page).getByRole("link", { name: "Other platforms and the command line" }),
  ).toHaveAttribute("href", "/download");
  await expect(page.locator("#hero-gnome")).toBeVisible();
});

test("gives an Arm PC the ARM64 installer", async ({ page, isMobile }) => {
  test.skip(isMobile, "a phone is offered macOS");
  await onWindows(page, "arm");
  await page.goto("/");
  await expect(page.locator("html")).toHaveAttribute("data-platform", "windows");
  for (const link of await page.getByRole("link", { name: action.windows }).all())
    await expect(link).toHaveAttribute("href", /-windows-arm64\.msi$/);
});

test("keeps the x64 installer for a PC that is not an Arm one", async ({ page, isMobile }) => {
  test.skip(isMobile, "a phone is offered macOS");
  await onWindows(page, "x86");
  await page.goto("/");
  await expect(hero(page).getByRole("link", { name: action.windows })).toHaveAttribute(
    "href",
    /-windows-x64\.msi$/,
  );
});
