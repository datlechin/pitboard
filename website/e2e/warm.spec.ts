// Settings at Warm-Up on each system: the hours as each system picks a time, each Claude Code
// account's switch, and the words that follow them (plan question 14, answer c; plan 3.3).
import type { Page } from "@playwright/test";
import { axe, choose, expect, handedOver, test, type Platform } from "./fixtures.ts";

async function open(page: Page, platform: Platform) {
  const id = platform === "windows" ? "warm-win" : "warm-mac";
  await choose(page.context(), { platform });
  await page.clock.install({ time: new Date("2026-10-13T13:51:00Z") });
  await page.goto("/");
  // the demo's switch to work is made, so the state stands still under each test
  await handedOver(page);
  await page.waitForFunction(() => customElements.get("pb-warm") !== undefined);
  const fig = page.locator(`#${id}`);
  await fig.scrollIntoViewIfNeeded();
  await expect(fig).not.toHaveAttribute("inert");
  return fig;
}

const workNote = "Has its 5-hour period running until 23:50, and warms up again at 07:00";

test.describe("macOS", () => {
  test("the hour and the minute step with the arrows, take typed digits, and the stepper steps", async ({
    page,
  }) => {
    const fig = await open(page, "macos");
    const from = fig.locator("#warm-mac-from");
    const hour = from.locator("[data-part=hour]");
    const minute = from.locator("[data-part=minute]");
    await hour.focus();
    await page.keyboard.press("ArrowDown");
    await expect(hour).toHaveText("06");
    await expect(hour).toHaveAttribute("aria-valuenow", "6");
    await page.keyboard.press("ArrowRight");
    await expect(minute).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(minute).toHaveText("59");
    await hour.focus();
    await page.keyboard.type("0915");
    await expect(hour).toHaveText("09");
    await expect(minute).toHaveText("15");
    const stepper = from.locator("[data-stepper]");
    const box = (await stepper.boundingBox())!;
    await page.mouse.click(box.x + box.width / 2, box.y + box.height / 4);
    await expect(minute).toHaveText("16");
    // a span that starts as it ends is refused, and the picker keeps what it had
    const to = fig.locator("#warm-mac-to [data-part=hour]");
    await to.focus();
    await page.keyboard.type("09");
    await fig.locator("#warm-mac-to [data-part=minute]").focus();
    await page.keyboard.type("16");
    await expect(fig.locator("#warm-mac-to [data-part]")).toHaveText(["09", "01"]);
  });

  test("an account's switch turns its warm-up on and off, and its words follow", async ({
    page,
  }) => {
    const fig = await open(page, "macos");
    const work = fig.locator('[data-account="work"]');
    const toggle = work.getByRole("switch");
    await expect(toggle).toHaveAttribute("aria-checked", "false");
    await expect(work.locator("[data-note]")).toBeHidden();
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", "true");
    await expect(work.locator("[data-note]")).toHaveText(workNote);
    const personal = fig.locator('[data-account="personal"]').getByRole("switch");
    await personal.focus();
    await page.keyboard.press("Space");
    await expect(personal).toHaveAttribute("aria-checked", "false");
    await expect(fig.locator("[data-next]")).toHaveText("Next warm-up: work at 07:00.");
    await work.getByRole("switch").click();
    await expect(fig.locator("[data-next-row]")).toBeHidden();
    // Reset puts every account back
    await page.locator("figure:has(#warm-mac)").getByRole("button", { name: "Reset" }).click();
    await expect(personal).toHaveAttribute("aria-checked", "true");
  });
});

test.describe("Windows", () => {
  test("the TimePicker's flyout changes the time only once Accept takes it", async ({ page }) => {
    const fig = await open(page, "windows");
    const from = fig.locator("#warm-win-from");
    const flyout = fig.locator("#warm-win-from-flyout");
    await expect(from).toHaveAccessibleName("Warm up from 07:00");
    await from.click();
    await expect(flyout).toBeVisible();
    const hour = flyout.getByRole("spinbutton", { name: "Hour" });
    await expect(hour).toBeFocused();
    await page.keyboard.press("ArrowUp");
    await expect(hour).toHaveAttribute("aria-valuetext", "06");
    await page.keyboard.press("Escape");
    await expect(flyout).toBeHidden();
    await expect(from).toBeFocused();
    await expect(from).toHaveAccessibleName("Warm up from 07:00");
    await page.keyboard.press("Enter");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("Tab");
    await expect(flyout.getByRole("spinbutton", { name: "Minute" })).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    await expect(flyout).toBeHidden();
    await expect(from).toHaveAccessibleName("Warm up from 06:01");
    await expect(fig.locator("[data-range]")).toHaveText("06:01 to 23:00");
    // a click on a row takes its value, and Accept takes the draft
    await from.click();
    await flyout.locator('[data-part="hour"] [data-at="2"]').click();
    await flyout.getByRole("button", { name: "Accept" }).click();
    await expect(from).toHaveAccessibleName("Warm up from 08:01");
    // a press elsewhere dismisses it
    await from.click();
    await page.mouse.click(5, 5);
    await expect(flyout).toBeHidden();
  });

  test("a ToggleSwitch turns an account's warm-up on with a click or Space, and says On", async ({
    page,
  }) => {
    const fig = await open(page, "windows");
    const work = fig.locator('[data-account="work"]');
    const toggle = work.getByRole("switch", { name: "work" });
    await toggle.focus();
    await page.keyboard.press("Enter");
    await expect(toggle).toHaveAttribute("aria-checked", "false");
    await page.keyboard.press("Space");
    await expect(toggle).toHaveAttribute("aria-checked", "true");
    await expect(work.locator("[data-said]")).toHaveText("On");
    await expect(work.locator("[data-note]")).toHaveText(workNote);
    await fig.locator("#warm-win-warm-expand").click();
    await expect(fig.locator("#warm-win-warm-items")).toBeHidden();
  });

  test("axe finds nothing with a flyout open", async ({ page }) => {
    const fig = await open(page, "windows");
    await fig.locator("#warm-win-to").click();
    await expect(fig.locator("#warm-win-to-flyout")).toBeVisible();
    // the open flyout covers the pickers under it, as on Windows, which target-size counts
    expect(await axe(page, ["#warm-win"], ["target-size"])).toEqual([]);
  });
});

for (const system of ["gnome", "kde"] as const)
  test.describe(`Linux under ${system}`, () => {
    test.skip(({ isMobile }) => isMobile, "a pointer and a keyboard");

    async function preferences(page: Page) {
      await choose(page.context(), { system });
      await page.clock.install({ time: new Date("2026-10-13T13:51:00Z") });
      await page.goto("/");
      await handedOver(page);
      await page.waitForFunction(() => customElements.get("pb-linux-preferences") !== undefined);
      const fig = page.locator("#warm-linux-preferences");
      await fig.scrollIntoViewIfNeeded();
      await expect(fig).not.toHaveAttribute("inert");
      return fig;
    }

    test("From and To step a quarter hour and an hour a page, wrap past midnight, and take no typing", async ({
      page,
    }) => {
      const fig = await preferences(page);
      const from = fig.getByRole("spinbutton", { name: "From" });
      await expect(from).toHaveAttribute("aria-valuetext", "07:00");
      await from.focus();
      await page.keyboard.press("ArrowUp");
      await expect(from).toHaveAttribute("aria-valuetext", "07:15");
      await page.keyboard.press("PageDown");
      await expect(from).toHaveAttribute("aria-valuetext", "06:15");
      await page.keyboard.press("Control+Home");
      await expect(from).toHaveAttribute("aria-valuetext", "00:00");
      // wrapping, a step from an end goes to the other, 23:59 as the app's adjustment has it
      await page.keyboard.press("ArrowDown");
      await expect(from).toHaveAttribute("aria-valuetext", "23:59");
      await expect(from).not.toBeEditable();
      const to = fig.getByRole("spinbutton", { name: "To" });
      await to.focus();
      await page.keyboard.press("ArrowDown");
      await expect(to).toHaveAttribute("aria-valuetext", "22:45");
    });

    test("each Claude Code account's switch turns its warm-up, and the next warm-up follows", async ({
      page,
    }) => {
      const fig = await preferences(page);
      const personal = fig.getByRole("switch", { name: "personal" });
      const work = fig.getByRole("switch", { name: "work" });
      await expect(personal).toHaveAttribute("aria-checked", "true");
      await expect(work).toHaveAttribute("aria-checked", "false");
      await expect(fig.locator('[data-part="warm-standing"]')).toHaveText(
        "Next warm-up: personal at 22:00.",
      );
      await personal.click();
      await expect(personal).toHaveAttribute("aria-checked", "false");
      await expect(fig.locator('[data-account="personal"] [data-part="subtitle"]')).toBeHidden();
      await work.focus();
      await page.keyboard.press("Space");
      await expect(work).toHaveAttribute("aria-checked", "true");
      await expect(fig.locator('[data-account="work"] [data-part="subtitle"]')).toBeVisible();
      // the window's Warm-Up row says the same
      await expect(
        page.locator(
          '#usage-linux [data-account="work"] [data-part="warm"] [data-part="subtitle"]',
        ),
      ).not.toHaveText("Off");
      expect(await axe(page, ["#warm-linux"])).toEqual([]);
    });

    test("an account's Warm-Up Preferences opens Preferences at Warm-Up with From focused", async ({
      page,
    }) => {
      await preferences(page);
      const usage = page.locator("#usage-linux");
      await usage.scrollIntoViewIfNeeded();
      await usage
        .locator('[data-account="personal"]')
        .getByRole("button", { name: "Warm-Up Preferences" })
        .click();
      const fig = page.locator("#warm-linux-preferences");
      await expect(fig).toBeInViewport();
      await expect(fig.getByRole("tab", { name: "Limits" })).toHaveAttribute(
        "aria-selected",
        "true",
      );
      await expect(fig.getByRole("spinbutton", { name: "From" })).toBeFocused();
    });
  });
