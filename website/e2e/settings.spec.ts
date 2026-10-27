// Settings at When a limit reaches, and the notification row, on both systems: the pop-up and the
// ComboBox, the share's field, stepper and NumberBox, the words under the choice, and the banner
// the app posts at the share (plan 3.6 and 3.7).
import type { Locator, Page } from "@playwright/test";
import { axe, choose, expect, handedOver, test, type Platform } from "./fixtures.ts";

const notify =
  "Pitboard notifies you once a limit of the account in use reaches 95%, while the app is open.";
const watching = "Watching work: 40% of its weekly limit, as of 20:50.";
const heads = { macos: "Weekly Limit Almost Reached", windows: "Weekly limit almost reached" };
const room = "personal has used 95%. work has used 40%.";

async function open(page: Page, platform: Platform, id: string) {
  await choose(page.context(), { platform });
  await page.clock.install({ time: new Date("2026-10-13T13:51:00Z") });
  await page.goto("/");
  // the demo's switch to work is made, so the state stands still under each test
  await handedOver(page);
  await page.waitForFunction((name) => customElements.get(name) !== undefined, "pb-settings");
  const fig = page.locator(`#${id}`);
  await fig.scrollIntoViewIfNeeded();
  await expect(fig).not.toHaveAttribute("inert");
  return fig;
}

// the choice the box shows: every choice's title is in it, the others hidden
const chosen = (box: Locator) => box.locator(":scope > [data-value]:not([hidden])");
const height = (l: Locator) => l.evaluate((el) => el.getBoundingClientRect().height);

test.describe("macOS", () => {
  test("the pop-up opens on the press with the choice over it, and the release chooses", async ({
    page,
    isMobile,
  }) => {
    test.skip(isMobile, "a phone taps: the next test");
    const fig = await open(page, "macos", "auto-mac");
    const box = page.locator("#auto-mac-at");
    const list = page.locator("#auto-mac-choices");
    await expect(chosen(box)).toHaveText("Notify Me");
    await expect(box).toHaveAttribute("aria-label", "When a limit reaches 95%");
    await expect(fig.locator('[data-note="notify"]')).toHaveText(notify);
    await expect(fig.locator("[data-standing]")).toBeHidden();
    const tall = await height(fig);

    const at = (await box.boundingBox())!;
    await page.mouse.move(at.x + at.width / 3, at.y + at.height / 2);
    await page.mouse.down();
    await expect(list).toBeVisible();
    const choice = page.locator("#auto-mac-choice-notify");
    await expect(choice).toHaveAttribute("aria-checked", "true");
    // the choice opens over the button's title
    const over = (await choice.boundingBox())!;
    expect(Math.abs(over.y + over.height / 2 - (at.y + at.height / 2))).toBeLessThan(2);
    const to = (await page.locator("#auto-mac-choice-switch").boundingBox())!;
    await page.mouse.move(to.x + to.width / 2, to.y + to.height / 2, { steps: 4 });
    await expect(page.locator("#auto-mac-choice-switch")).toHaveAttribute("aria-selected", "true");
    await page.mouse.up();
    await expect(list).toBeHidden();
    await expect(chosen(box)).toHaveText("Switch Automatically");
    await expect(fig.locator("[data-standing]")).toHaveText(watching);
    await expect(fig.locator('[data-note="switch"]')).toContainText(
      "its running sessions follow within about 33 seconds",
    );
    const link = fig.getByRole("link", { name: "Is this allowed?" });
    await expect(link).toHaveAttribute(
      "href",
      "https://docs.usepitboard.com/concepts/limits#is-this-allowed",
    );
    await expect(link).toBeInViewport({ ratio: 1 });
    expect(await height(fig)).toBe(tall);

    // a click that comes straight back up leaves the menu open, and an exploratory press
    // released outside it chooses nothing
    await box.click();
    await expect(list).toBeVisible();
    await page.mouse.click(5, 5);
    await expect(list).toBeHidden();
    await expect(chosen(box)).toHaveText("Switch Automatically");
    await page.mouse.move(at.x + at.width / 3, at.y + at.height / 2);
    await page.mouse.down();
    await page.mouse.move(at.x - 200, at.y + 150, { steps: 4 });
    await page.mouse.up();
    await expect(list).toBeHidden();
    await expect(chosen(box)).toHaveText("Switch Automatically");
    expect(await height(fig)).toBe(tall);
  });

  test("a tap opens the pop-up and a tap on an item chooses it", async ({ page, isMobile }) => {
    test.skip(!isMobile, "taps on a phone");
    const fig = await open(page, "macos", "auto-mac");
    const box = page.locator("#auto-mac-at");
    await box.tap();
    await expect(page.locator("#auto-mac-choices")).toBeVisible();
    await page.locator("#auto-mac-choice-nothing").tap();
    await expect(chosen(box)).toHaveText("Do Nothing");
    await expect(fig.locator("[data-note]")).toHaveCount(2);
    for (const note of await fig.locator("[data-note]").all()) await expect(note).toBeHidden();
  });

  test("the keys open the pop-up on its choice, move, choose and close it", async ({ page }) => {
    const fig = await open(page, "macos", "auto-mac");
    const box = page.locator("#auto-mac-at");
    await box.focus();
    await page.keyboard.press("Space");
    await expect(box).toHaveAttribute("aria-expanded", "true");
    await expect(box).toHaveAttribute("aria-activedescendant", "auto-mac-choice-notify");
    await page.keyboard.press("ArrowUp");
    await expect(box).toHaveAttribute("aria-activedescendant", "auto-mac-choice-nothing");
    await page.keyboard.press("Escape");
    await expect(box).toHaveAttribute("aria-expanded", "false");
    await expect(chosen(box)).toHaveText("Notify Me");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("Enter");
    await expect(chosen(box)).toHaveText("Do Nothing");
    await expect(box).toBeFocused();
    // Do Nothing turns the share off
    await expect(page.locator("#auto-mac-share")).toBeDisabled();
    await expect(page.locator("#auto-mac-stepper")).toHaveAttribute("aria-disabled", "true");
    // type-select
    await page.keyboard.press("Space");
    await page.keyboard.type("sw");
    await expect(box).toHaveAttribute("aria-activedescendant", "auto-mac-choice-switch");
    await page.keyboard.press("Enter");
    await expect(chosen(box)).toHaveText("Switch Automatically");
    await expect(fig.locator("[data-standing]")).toBeVisible();
  });

  test("the field takes a share, clamped, gives back text that is no figure, and the stepper steps", async ({
    page,
  }) => {
    const fig = await open(page, "macos", "auto-mac");
    const field = page.locator("#auto-mac-share");
    const stepper = page.locator("#auto-mac-stepper");
    await field.fill("40");
    await field.press("Enter");
    await expect(field).toHaveValue("50%");
    await expect(stepper).toHaveAttribute("aria-valuenow", "50");
    await expect(page.locator("#auto-mac-at")).toHaveAttribute(
      "aria-label",
      "When a limit reaches 50%",
    );
    await expect(fig.locator('[data-note="notify"]')).toContainText("reaches 50%,");
    await field.fill("ninety");
    await field.press("Tab");
    await expect(field).toHaveValue("50%");
    await field.fill("90");
    await field.press("Enter");
    await expect(field).toHaveValue("90%");
    await stepper.focus();
    await page.keyboard.press("ArrowUp");
    await expect(field).toHaveValue("91%");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await expect(stepper).toHaveAttribute("aria-valuenow", "89");
    const s = (await stepper.boundingBox())!;
    await page.mouse.click(s.x + s.width / 2, s.y + s.height / 4);
    await expect(field).toHaveValue("90%");
    // held a second, it steps 16 times, as the app's did (docs/facts/site.md)
    await page.clock.pauseAt((await page.evaluate(() => Date.now())) + 1000);
    await page.mouse.move(s.x + s.width / 2, s.y + (s.height * 3) / 4);
    await page.mouse.down();
    await expect(field).toHaveValue("89%");
    await page.clock.runFor(1000);
    await page.mouse.up();
    await expect(field).toHaveValue("74%");
  });
});

test.describe("Windows", () => {
  test("the ComboBox changes at once with the arrows while closed, and commits on Enter while open", async ({
    page,
  }) => {
    const fig = await open(page, "windows", "auto-win");
    const box = page.locator("#auto-win-at");
    await expect(chosen(box)).toHaveText("Notify me");
    await box.focus();
    await page.keyboard.press("ArrowDown");
    await expect(chosen(box)).toHaveText("Switch automatically");
    await expect(fig.locator("[data-standing]")).toHaveText(watching);
    await page.keyboard.press("Home");
    await expect(chosen(box)).toHaveText("Do nothing");
    // At shows only while something happens at the share
    await expect(fig.locator("[data-share-card]")).toBeHidden();
    await page.keyboard.press("Alt+ArrowDown");
    await expect(page.locator("#auto-win-choices")).toBeVisible();
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Escape");
    await expect(chosen(box)).toHaveText("Do nothing");
    await page.keyboard.press("F4");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    await expect(page.locator("#auto-win-choices")).toBeHidden();
    await expect(chosen(box)).toHaveText("Notify me");
    await box.click();
    await page.locator("#auto-win-choice-switch").click();
    await expect(chosen(box)).toHaveText("Switch automatically");
    await expect(fig.getByRole("link", { name: "Is this allowed?" })).toBeVisible();
  });

  test("the NumberBox steps by 1 and 5, clamps, takes typed text on Enter and puts it back on Escape", async ({
    page,
  }) => {
    await open(page, "windows", "auto-win");
    const box = page.locator("#auto-win-share");
    await box.focus();
    await page.keyboard.press("ArrowUp");
    await expect(box).toHaveValue("96");
    await page.keyboard.press("PageUp");
    await expect(box).toHaveValue("99");
    await expect(box).toHaveAttribute("aria-valuenow", "99");
    await expect(page.locator("[data-spin=up]").first()).toHaveAttribute("data-state", "off");
    await page.keyboard.press("PageDown");
    await expect(box).toHaveValue("94");
    await box.fill("12");
    await page.keyboard.press("Enter");
    await expect(box).toHaveValue("50");
    await expect(page.locator("#auto-win-at")).toHaveAttribute(
      "aria-label",
      "When a limit reaches 50%",
    );
    await box.fill("70");
    await page.keyboard.press("Escape");
    await expect(box).toHaveValue("50");
    await box.fill("");
    await page.keyboard.press("Enter");
    await expect(box).toHaveValue("50");
  });

  test("the expander shows and hides the share's field", async ({ page }) => {
    const fig = await open(page, "windows", "auto-win");
    const toggle = page.locator("#auto-win-expand");
    await expect(toggle).toHaveAttribute("aria-expanded", "true");
    const tall = await height(fig);
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    // the clock's next tick draws the figure again
    await page.clock.runFor(1000);
    await expect(page.locator("#auto-win-items")).toBeHidden();
    expect(await height(fig)).toBe(tall);
    await fig.getByText("When a limit reaches", { exact: true }).click();
    await expect(page.locator("#auto-win-items")).toBeVisible();
  });
});

test.describe("Notification Center", () => {
  test("on Windows the clock opens it, where the notification waits once its toast has gone", async ({
    page,
    isMobile,
  }) => {
    test.skip(isMobile, "a pointer and a keyboard");
    await choose(page.context(), { platform: "windows" });
    await page.clock.install({ time: new Date("2026-10-13T13:51:00Z") });
    await page.goto("/");
    await page.waitForFunction(() => customElements.get("pb-notice") !== undefined);
    const fig = page.locator("#offer-win");
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
    await choose(page.context(), { platform: "macos" });
    await page.goto("/");
    const fig = page.locator("#offer-mac");
    await fig.scrollIntoViewIfNeeded();
    await expect(fig.locator('[data-part="center"]')).toHaveCount(0);
    await expect(fig.locator("[data-clock]")).not.toHaveRole("button");
  });
});

for (const [platform, id] of [
  ["macos", "offer-mac"],
  ["windows", "offer-win"],
] as const) {
  test.describe(`the notification on ${platform}`, () => {
    test("arrives once its row is in view, stays while held, and goes after the system's time", async ({
      page,
    }) => {
      await choose(page.context(), { platform });
      await page.clock.install({ time: new Date("2026-10-13T13:51:00Z") });
      await page.goto("/");
      await page.waitForFunction(() => customElements.get("pb-notice") !== undefined);
      const fig = page.locator(`#${id}`);
      const banner = fig.locator("[data-banner]");
      await expect(banner).toBeHidden();
      await fig.scrollIntoViewIfNeeded();
      await expect(banner).toBeVisible();
      await expect(banner).toHaveAccessibleName(heads[platform]);
      await expect(page.locator("#status")).toHaveText(`${heads[platform]}, Claude Code, ${room}`);
      await expect(banner.locator("[data-switch]")).toHaveText(
        platform === "macos" ? "Switch" : "Switch to work",
      );
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
      if (platform === "macos") await expect(fig.locator("[data-title]")).toHaveText("work 40%");
      else await expect(fig.locator("[data-level=g]")).toBeVisible();
      // the same notification does not come again
      await page.evaluate(() => scrollTo(0, 0));
      await fig.scrollIntoViewIfNeeded();
      await page.clock.runFor(500);
      await expect(banner).toBeHidden();
    });

    test("follows the choice and the share, and goes by itself", async ({ page }) => {
      const pre = platform === "macos" ? "auto-mac" : "auto-win";
      const settings = await open(page, platform, pre);
      const share = page.locator(`#${pre}-share`);
      await share.fill("90");
      await share.press("Enter");
      const fig = page.locator(`#${id}`);
      const banner = fig.locator("[data-banner]");
      await fig.scrollIntoViewIfNeeded();
      await expect(banner).toHaveAccessibleName(heads[platform]);
      await expect(banner.locator("[data-body]")).toHaveText(
        "personal has used 90%. work has used 40%.",
      );
      await page.mouse.move(0, 0);
      await page.clock.fastForward(5500);
      await expect(banner).toBeHidden();
      await settings.scrollIntoViewIfNeeded();
      const box = page.locator(`#${pre}-at`);
      await box.focus();
      if (platform === "macos") {
        await page.keyboard.press("Space");
        await expect(page.locator(`#${pre}-choices`)).toBeVisible();
        await page.keyboard.press("ArrowDown");
        await page.keyboard.press("Enter");
      } else await page.keyboard.press("ArrowDown");
      await expect(chosen(box)).toHaveText(
        platform === "macos" ? "Switch Automatically" : "Switch automatically",
      );
      await fig.scrollIntoViewIfNeeded();
      await expect(banner).toHaveAccessibleName("Switched to work");
      await expect(banner.locator("[data-switch]")).toHaveText(
        platform === "macos" ? "Switch Back" : "Switch back",
      );
      await expect(banner.locator("[data-subtitle]")).toHaveText("Claude Code");
    });
  });
}

test("axe finds nothing in the open pop-up and the open ComboBox", async ({ page }) => {
  for (const [platform, id] of [
    ["macos", "auto-mac"],
    ["windows", "auto-win"],
  ] as const) {
    await open(page, platform, id);
    await page.locator(`#${id}-at`).focus();
    await page.keyboard.press(platform === "macos" ? "Space" : "Enter");
    await expect(page.locator(`#${id}-choices`)).toBeVisible();
    expect(await axe(page, [`#${id}`])).toEqual([]);
  }
});
