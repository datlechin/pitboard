// /download's one action for the reader's system, with the rest under More options.
import { choose, expect, platforms, rules, test, type Platform } from "./fixtures.ts";

test.use({ reducedMotion: "reduce" });

const action: Record<Exclude<Platform, "linux">, RegExp> = {
  macos: /^Download for Mac/,
  windows: /^Download for Windows/,
};

test("gives an Arm PC the ARM64 installer", async ({ page, isMobile }) => {
  test.skip(isMobile, "a phone is offered macOS");
  await page.addInitScript(() => {
    Object.defineProperty(Navigator.prototype, "userAgentData", {
      configurable: true,
      get: () => ({
        platform: "Windows",
        getHighEntropyValues: () => Promise.resolve({ architecture: "arm", bitness: "64" }),
      }),
    });
  });
  await page.goto("/download");
  await expect(page.getByRole("link", { name: action.windows })).toHaveAttribute(
    "href",
    /-windows-arm64\.msi$/,
  );
});

for (const platform of platforms) {
  test(`/download on ${platform} shows that system's action alone, and More options closed`, async ({
    page,
  }) => {
    await choose(page.context(), { platform });
    await page.goto("/download");
    for (const other of platforms) {
      if (other === platform) await expect(page.locator(`#${other}`)).toBeVisible();
      else await expect(page.locator(`#${other}`)).toBeHidden();
    }
    const section = page.locator(`#${platform}`);
    if (platform === "linux")
      await expect(section.getByRole("tablist", { name: "Linux distribution" })).toBeVisible();
    else await expect(section.getByRole("link", { name: action[platform] })).toBeVisible();
    await expect(page.locator(`#${platform}-more`)).not.toHaveAttribute("open");
    await expect(section.getByRole("link", { name: "Release notes" })).toBeVisible();
    expect(
      await rules(page)
        .analyze()
        .then((r) => r.violations.map((v) => v.id)),
    ).toEqual([]);
  });
}

test("the platform tabs show another system, and a link to its options opens them", async ({
  page,
}) => {
  await choose(page.context(), { platform: "macos" });
  await page.goto("/download");
  await page
    .getByRole("tablist", { name: "Download Pitboard for" })
    .getByRole("tab", { name: "Linux" })
    .click();
  await expect(page.locator("#linux")).toBeVisible();
  await expect(page.locator("#macos")).toBeHidden();
  await page.goto("/download#windows-more");
  await expect(page.locator("#windows")).toBeVisible();
  await expect(page.locator("#windows-more")).toHaveAttribute("open");
  await expect(
    page.locator("#windows-more").getByRole("link", { name: "Arm PC" }).first(),
  ).toHaveAttribute("href", /-windows-arm64\.msi$/);
});

test("Linux's distributions are tabs the arrows, Home and End move along, each one block to copy", async ({
  page,
  isMobile,
}) => {
  test.skip(isMobile, "a keyboard");
  await choose(page.context(), { platform: "linux" });
  await page.goto("/download");
  const tabs = page.getByRole("tablist", { name: "Linux distribution" }).getByRole("tab");
  await expect(tabs).toHaveText(["Debian and Ubuntu", "Fedora", "openSUSE"]);
  await expect(tabs.first()).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#linux-apt")).toBeVisible();
  await expect(page.locator("#linux-dnf")).toBeHidden();
  const copied = await page.locator("#linux-apt [data-copy]").getAttribute("data-copy");
  expect(copied?.split("\n")).toHaveLength(5);
  expect(copied).toMatch(/\nsudo apt install pitboard-app$/);
  await tabs.first().focus();
  await page.keyboard.press("ArrowRight");
  await expect(tabs.nth(1)).toBeFocused();
  await expect(tabs.nth(1)).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#linux-dnf")).toBeVisible();
  await expect(page.locator("#linux-apt")).toBeHidden();
  await page.keyboard.press("End");
  await expect(tabs.nth(2)).toBeFocused();
  await expect(page.locator("#linux-zypper")).toBeVisible();
  await page.keyboard.press("ArrowRight");
  await expect(tabs.first()).toBeFocused();
  await page.keyboard.press("Home");
  await expect(page.locator("#linux-apt")).toBeVisible();
});

for (const platform of platforms) {
  test(`/download on ${platform} fits a 320 pixel phone with its options open`, async ({
    page,
    isMobile,
  }) => {
    test.skip(!isMobile, "a phone's width");
    await page.setViewportSize({ width: 320, height: 640 });
    await choose(page.context(), { platform });
    await page.goto(`/download#${platform}-more`);
    await expect(page.locator(`#${platform}-more`)).toHaveAttribute("open");
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
      320,
    );
  });
}
