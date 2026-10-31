// Settings at When a limit reaches on macOS and Windows: the pop-up and the ComboBox, the share's
// field, stepper and NumberBox, and the words under the choice (plan 3.6 and 3.7). notice.spec.ts
// has the banner the app posts at the share.
import type { Locator, Page } from "@playwright/test";
import { axe, choose, expect, handedOver, test, type Platform } from "./fixtures.ts";

const notify =
  "Pitboard notifies you once a limit of the account in use reaches 95%, while the app is open.";
const watching = "Watching work: 40% of its weekly limit, as of 20:50.";

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
    await expect(box).toHaveAccessibleName("When a limit reaches");
    await expect(fig.locator('[data-note="notify"]')).toHaveText(notify);
    await expect(fig.locator("[data-standing]")).toBeHidden();
    await expect(fig.getByRole("link", { name: "Is this allowed?" })).toBeVisible();
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
    // Do Nothing leaves the share out
    await expect(page.locator("#auto-mac-share")).toBeHidden();
    await expect(page.locator("#auto-mac-stepper")).toBeHidden();
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
    // At stays, dimmed, while nothing happens at the share
    await expect(fig.locator("[data-share-card]")).toBeVisible();
    await expect(page.locator("#auto-win-share")).toBeDisabled();
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

// Preferences on Linux, an AdwPreferencesDialog over its window: the view switcher as a tablist,
// When a limit reaches as a combobox and listbox, At as a spin button, and the words a desktop with
// a tray and one without each say (plan 3.3 "Automatic switch, and Warm-up").
async function preferences(page: Page, system: "gnome" | "kde") {
  await choose(page.context(), { system });
  await page.clock.install({ time: new Date("2026-10-13T13:51:00Z") });
  await page.goto("/");
  await handedOver(page);
  await page.waitForFunction(() => customElements.get("pb-linux-preferences") !== undefined);
  const fig = page.locator("#auto-linux-preferences");
  await fig.scrollIntoViewIfNeeded();
  await expect(fig).not.toHaveAttribute("inert");
  return fig;
}

for (const system of ["gnome", "kde"] as const)
  test.describe(`Linux under ${system}`, () => {
    test.skip(({ isMobile }) => isMobile, "a pointer and a keyboard");
    const tray = system === "kde";

    test("the view switcher is a tablist the arrows, Home and End move along", async ({ page }) => {
      const fig = await preferences(page, system);
      await expect(fig).toHaveAccessibleName("Preferences");
      const tabs = fig.getByRole("tablist", { name: "Preferences" }).getByRole("tab");
      await expect(tabs).toHaveText(["General", "Limits", "Advanced"], { useInnerText: true });
      const limits = fig.getByRole("tab", { name: "Limits" });
      await expect(limits).toHaveAttribute("aria-selected", "true");
      await limits.focus();
      await page.keyboard.press("ArrowRight");
      await expect(fig.getByRole("tab", { name: "Advanced" })).toBeFocused();
      await expect(fig.getByRole("tabpanel", { name: "Advanced" })).toBeVisible();
      await page.keyboard.press("ArrowRight");
      await expect(fig.getByRole("tab", { name: "General" })).toBeFocused();
      await page.keyboard.press("End");
      await expect(fig.getByRole("tab", { name: "Advanced" })).toHaveAttribute(
        "aria-selected",
        "true",
      );
      await page.keyboard.press("Home");
      const general = fig.getByRole("tabpanel", { name: "General" });
      await expect(general).toBeVisible();
      // with no status item, closing the window quits Pitboard, and General says so
      await expect(general.getByText("Pitboard quits once its window is closed.")).toBeVisible({
        visible: !tray,
      });
      await expect(general.getByRole("switch", { name: "Start Pitboard at Login" })).toBeVisible();
    });

    test("When a limit reaches opens its list, and Switch Automatically says what it watches", async ({
      page,
    }) => {
      const fig = await preferences(page, system);
      const box = fig.getByRole("combobox", { name: "When a limit reaches" });
      const list = page.locator("#auto-linux-at-limit-choices");
      const note = fig.locator('[data-part="note"]:not([hidden])');
      await expect(chosen(box)).toHaveText("Notify Me");
      await expect(note).toHaveText(notify);
      await box.click();
      await expect(list).toBeVisible();
      await expect(box).toHaveAttribute("aria-expanded", "true");
      await expect(list.getByRole("option")).toHaveText(
        ["Do Nothing", "Notify Me", "Switch Automatically"],
        { useInnerText: true },
      );
      expect(await axe(page, ["#auto-linux"])).toEqual([]);
      await list.getByRole("option", { name: "Switch Automatically" }).click();
      await expect(list).toBeHidden();
      await expect(chosen(box)).toHaveText("Switch Automatically");
      await expect(note).toContainText("Pitboard switches Claude Code at 95%");
      await expect(fig).toContainText(watching);
      await expect(fig.locator('[data-part="once-closed"]')).toBeVisible({ visible: !tray });
      // the keyboard opens it on the choice, and Escape closes it on the box
      await box.focus();
      await page.keyboard.press("Enter");
      await expect(list).toBeVisible();
      await page.keyboard.press("ArrowUp");
      await page.keyboard.press("Escape");
      await expect(list).toBeHidden();
      await expect(box).toBeFocused();
      await expect(chosen(box)).toHaveText("Switch Automatically");
    });

    test("At steps by a percent and a page, goes to its ends, and clamps what is typed", async ({
      page,
    }) => {
      const fig = await preferences(page, system);
      const at = fig.getByRole("spinbutton", { name: "At" });
      await at.focus();
      await page.keyboard.press("ArrowUp");
      await expect(at).toHaveAttribute("aria-valuenow", "96");
      await page.keyboard.press("PageDown");
      await expect(at).toHaveAttribute("aria-valuenow", "91");
      await page.keyboard.press("Control+End");
      await expect(at).toHaveAttribute("aria-valuenow", "99");
      // with no wrap, a step past an end stays on it
      await page.keyboard.press("ArrowUp");
      await expect(at).toHaveAttribute("aria-valuenow", "99");
      await page.keyboard.press("Control+Home");
      await expect(at).toHaveAttribute("aria-valuenow", "50");
      await at.fill("120");
      await at.press("Enter");
      await expect(at).toHaveAttribute("aria-valuenow", "99");
      await expect(at).toHaveValue("99%");
      await at.fill("lots");
      await at.press("Enter");
      await expect(at).toHaveValue("99%");
      await at.fill("80%");
      await at.press("Enter");
      await expect(at).toHaveAttribute("aria-valuenow", "80");
      await expect(fig.locator('[data-part="note"]:not([hidden])')).toContainText("reaches 80%");
    });

    test("Advanced offers Watch in the Background only where no status item keeps Pitboard", async ({
      page,
    }) => {
      const fig = await preferences(page, system);
      await fig.getByRole("tab", { name: "Advanced" }).click();
      const panel = fig.getByRole("tabpanel", { name: "Advanced" });
      await expect(panel.getByRole("switch", { name: "Keep Saved Sign-Ins Fresh" })).toBeVisible();
      await expect(panel).toContainText("/usr/bin/pitboard");
      const watch = panel.getByRole("switch", { name: "Watch in the Background" });
      if (tray) return await expect(watch).toBeHidden();
      await watch.click();
      await expect(watch).toHaveAttribute("aria-checked", "true");
      // Limits then says what goes on once the window is closed
      await fig.getByRole("tab", { name: "Limits" }).click();
      await expect(fig.locator('[data-part="once-closed"]')).toHaveText(
        "Once its window is closed, nothing notifies you, and the watch service only warms up.",
      );
    });

    test("Uninstall asks with Cancel its default, then names dnf's command, and Quit quits until Reset", async ({
      page,
    }) => {
      const fig = await preferences(page, system);
      await fig.getByRole("tab", { name: "Advanced" }).click();
      await fig.getByRole("button", { name: "Uninstall Pitboard…" }).click();
      const question = page.locator("#auto-linux-uninstall-question");
      await expect(question).toBeVisible();
      await expect(question).toHaveAccessibleName("Uninstall Pitboard?");
      await expect(question.getByRole("button", { name: "Cancel" })).toBeFocused();
      await question.getByRole("button", { name: "Uninstall" }).click();
      const removal = page.locator("#auto-linux-remove-the-app");
      await expect(removal).toBeVisible();
      await expect(removal).toContainText("sudo dnf remove pitboard-app pitboard");
      expect(await axe(page, ["#auto-linux-remove-the-app"])).toEqual([]);
      await removal.getByRole("button", { name: "Quit" }).click();
      await expect(removal).toBeHidden();
      await expect(page.locator("#auto-linux")).toBeHidden();
      await page.locator("figure:has(#auto-linux) [data-reset]").click();
      await expect(page.locator("#auto-linux")).toBeVisible();
      // focus goes past the window Preferences keeps out of reach, to its first control
      await expect(fig.getByRole("tab", { name: "Limits" })).toBeFocused();
    });

    test("Close and Escape put Preferences away, and focus goes back to the window", async ({
      page,
    }) => {
      const fig = await preferences(page, system);
      // a modal dialog: the window and its menus behind it are out of reach
      await expect(fig).toHaveAttribute("aria-modal", "true");
      const behind = page.locator("#auto-linux > div:not([data-part])");
      await expect(behind.first()).toHaveAttribute("inert", "");
      await expect(page.locator("#auto-linux-main-menu")).toHaveAttribute("inert", "");
      await fig.getByRole("spinbutton", { name: "At" }).focus();
      await page.keyboard.press("Escape");
      await expect(fig).toBeHidden();
      await expect(behind.first()).not.toHaveAttribute("inert");
      await expect(
        page.locator("#auto-linux").getByRole("button", { name: "Main Menu" }),
      ).toBeFocused();
      await page.keyboard.press("Control+Comma");
      await expect(fig).toBeVisible();
      await expect(fig.getByRole("tab", { name: "General" })).toHaveAttribute(
        "aria-selected",
        "true",
      );
      await fig.getByRole("button", { name: "Close" }).click();
      await expect(fig).toBeHidden();
    });
  });

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
