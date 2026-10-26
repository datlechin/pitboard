// The windows' panes, Settings' tabs and the windows the menus open (answers 3 a and 4 a): the
// macOS sidebar and the Windows navigation show each pane drawn from source, Settings' toolbar shows
// General and Advanced, and About and an account's window open over the desk they are asked from.
import type { Page } from "@playwright/test";
import { axe, choose, expect, handedOver, test, type Platform } from "./fixtures.ts";

const at = { position: { x: 200, y: 12 } };

async function open(page: Page, platform: Platform, id: string) {
  await choose(page.context(), { platform });
  await page.goto("/");
  await handedOver(page);
  const fig = page.locator(`#${id}`);
  await fig.scrollIntoViewIfNeeded();
  await expect(fig).not.toHaveAttribute("inert");
  return {
    fig,
    tab: (pane: string) => fig.locator(`[role="tab"][data-pane="${pane}"]`),
    panel: (pane: string) => fig.locator(`[data-pane-panel="${pane}"]`),
  };
}

test.use({ reducedMotion: "reduce" });
test.skip(({ isMobile }) => isMobile, "a pointer and a keyboard");

test.describe("macOS", () => {
  test("the sidebar shows each pane, the window keeping its size, and Up and Down select", async ({
    page,
  }) => {
    const { fig, tab, panel } = await open(page, "macos", "usage-mac");
    const window = fig.locator('[data-part="window"]');
    const tall = (await window.boundingBox())!.height;
    await tab("activity").click();
    await expect(panel("activity")).toBeVisible();
    await expect(panel("accounts")).toBeHidden();
    await expect(fig.locator('[data-head="activity"]')).toHaveText(/Activity/);
    expect(Math.round((await window.boundingBox())!.height)).toBe(Math.round(tall));
    await expect(panel("activity").locator('[data-part="log"] > *').first()).toContainText(
      "Switch",
    );
    await page.keyboard.press("ArrowDown");
    await expect(tab("machine")).toBeFocused();
    await expect(tab("machine")).toHaveAttribute("aria-selected", "true");
    await expect(panel("machine")).toContainText("Everything Pitboard checks is in order.");
    await expect(panel("machine")).toContainText(/Checked at 20:5\d/);
    await expect(panel("machine").getByRole("img", { name: "Passed" }).first()).toBeVisible();
    // the list stops at its ends
    await page.keyboard.press("ArrowDown");
    await expect(tab("machine")).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("ArrowUp");
    await expect(tab("accounts")).toHaveAttribute("aria-selected", "true");
    await expect(panel("accounts")).toBeVisible();
    expect(await axe(page, ["#usage-mac"])).toEqual([]);
  });

  test("the sidebar button hides the sidebar and shows it again", async ({ page }) => {
    const { fig } = await open(page, "macos", "usage-mac");
    const button = fig.getByRole("button", { name: "Hide Sidebar" });
    await button.click();
    await expect(fig.getByRole("tablist", { name: "Sidebar" })).toBeHidden();
    await fig.getByRole("button", { name: "Show Sidebar" }).click();
    await expect(fig.getByRole("tablist", { name: "Sidebar" })).toBeVisible();
  });

  test("the Activity window's Accounts pane lists the accounts, and Add Account comes back to it", async ({
    page,
  }) => {
    const { fig, tab, panel } = await open(page, "macos", "security-mac");
    await expect(panel("activity")).toBeVisible();
    await tab("accounts").click();
    await expect(panel("accounts").getByRole("row", { name: /^work\b/ })).toBeVisible();
    await tab("activity").click();
    await fig.locator("#security-mac-am-file").click();
    await fig.locator('[id="security-mac-am-file-menu"] [data-does="add"]').click();
    await expect(page.locator("#mac-accounts-sign-in")).toBeVisible();
    await expect(panel("accounts")).toBeVisible();
  });

  test("Settings' tabs show General and Advanced, whose settings act on the page", async ({
    page,
  }) => {
    const { fig } = await open(page, "macos", "auto-mac");
    await fig.getByRole("tab", { name: "General" }).click();
    await expect(fig.locator('[data-part="title"]')).toHaveText("General");
    await expect(fig.getByRole("switch", { name: "Open Pitboard at login" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
    const installs = fig.getByRole("switch", {
      name: "Download and install updates automatically",
    });
    await fig.getByRole("switch", { name: "Check for updates automatically" }).click();
    await expect(installs).toHaveAttribute("aria-disabled", "true");
    await fig.locator("#auto-mac-shows").click();
    await page.locator("#auto-mac-shows-usage").click();
    await expect(page.locator('#hero-mac-item [data-part="title"]')).toHaveText("40%");
    await page.keyboard.press("ArrowRight");
    await fig.getByRole("tab", { name: "Advanced" }).click();
    await expect(fig.locator('[data-part="title"]')).toHaveText("Advanced");
    await expect(fig).toContainText("/opt/homebrew/bin/pitboard");
    await fig.getByRole("switch", { name: "Renew parked logins daily" }).click();
    // the other Settings figure is the same window
    const other = page.locator("#warm-mac");
    await other.scrollIntoViewIfNeeded();
    await other.getByRole("tab", { name: "Advanced" }).click();
    await expect(other.getByRole("switch", { name: "Renew parked logins daily" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(await axe(page, ["#warm-mac"])).toEqual([]);
  });

  test("About Pitboard opens the About panel over the desk it is asked from", async ({ page }) => {
    await open(page, "macos", "hero-mac");
    await page.locator("#hero-title").click();
    await page.locator("#hero-mac-item").click();
    await page.locator('#hero-mac-menu [data-does="about"]').click();
    const panel = page.locator('#hero-mac > [data-part="about"]');
    await expect(panel).toBeVisible();
    await expect(panel).toHaveAccessibleName("Pitboard");
    await expect(panel).toContainText(/\d+\.\d+\.\d+.* \(\d+\)/);
    expect(await axe(page, ["#hero-mac"])).toEqual([]);
    await panel.getByRole("button", { name: "Close" }).click();
    await expect(panel).toHaveCount(0);
  });

  test("an account's window opens from File, titled by its account, and goes with it", async ({
    page,
  }) => {
    const { fig } = await open(page, "macos", "switch-mac");
    await fig.locator("#switch-mac-am-file").click();
    await fig
      .locator('[id="switch-mac-am-file-menu"] [data-does="claude.ai"]:not([data-one])')
      .hover();
    await fig.locator('[id="switch-mac-am-file-claude.ai"] [data-account="personal"]').click();
    const window = fig.locator('[data-part="account-window"][data-account="personal"]');
    await expect(window).toBeVisible();
    await expect(window.locator('[data-part="title"]')).toHaveText("personal");
    await expect(window.locator('[data-part="site"]')).toHaveText("claude.ai");
    expect(await axe(page, ["#switch-mac"])).toEqual([]);
    // forgotten, from the window in #usage, its window goes too
    await page.locator('#usage-mac [role="row"][data-account="personal"]').click(at);
    await page.keyboard.press("Meta+Backspace");
    await page.locator("#mac-accounts-forget").getByRole("button", { name: "Forget" }).click();
    await expect(window).toHaveCount(0);
  });
});

test.describe("Windows", () => {
  test("the navigation moves focus with the arrows, wrapping, and Enter shows the page", async ({
    page,
  }) => {
    const { tab, panel } = await open(page, "windows", "usage-win");
    await tab("accounts").focus();
    await page.keyboard.press("ArrowDown");
    await expect(tab("activity")).toBeFocused();
    await expect(tab("accounts")).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("Enter");
    await expect(panel("activity")).toBeVisible();
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Space");
    await expect(panel("machine")).toContainText("Checking this PC…");
    await expect(panel("machine").getByRole("button", { name: "Check again" })).toBeDisabled();
    await page.keyboard.press("ArrowDown");
    await expect(tab("settings")).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(tab("accounts")).toBeFocused();
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("Enter");
    // Settings from its top
    await expect(panel("settings")).toContainText("Start when you sign in to Windows");
    await expect(panel("settings").getByRole("button", { name: "Check now" })).toBeDisabled();
    await expect(panel("settings")).toContainText(
      "No full release of Pitboard for Windows is out yet",
    );
    await panel("settings").getByRole("switch", { name: "Renew parked logins daily" }).click();
    await expect(
      panel("settings").getByRole("switch", { name: "Renew parked logins daily" }),
    ).toHaveAttribute("aria-checked", "true");
    expect(await axe(page, ["#usage-win"])).toEqual([]);
  });

  test("the title bar's button opens the navigation with its labels, and a pane chosen closes it", async ({
    page,
  }) => {
    const { fig, tab, panel } = await open(page, "windows", "usage-win");
    await fig.getByRole("button", { name: "Open Navigation" }).click();
    await expect(fig.getByRole("button", { name: "Close Navigation" })).toBeVisible();
    await expect(tab("activity")).toContainText("Activity");
    await tab("activity").click();
    await expect(panel("activity")).toBeVisible();
    await expect(fig.getByRole("button", { name: "Open Navigation" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });

  test("the icon's Settings shows the Settings page, and an account's window opens titled Account, then by its account", async ({
    page,
  }) => {
    await open(page, "windows", "hero-win");
    await page.locator("#hero-title").click();
    await page.locator("#hero-win-icon").click({ button: "right" });
    await page.locator('#hero-win-menu [data-does="claude.ai"]:not([data-one])').click();
    await page.locator('[id="hero-win-claude.ai-menu"] [data-account="work"]').click();
    const window = page.locator('#hero-win [data-part="account-window"][data-account="work"]');
    await expect(window.locator('[data-part="title"]')).toHaveText("work");
    expect(await axe(page, ["#hero-win"])).toEqual([]);
    await window.getByRole("button", { name: "Close" }).click();
    await expect(window).toHaveCount(0);
    // closing gives focus back to the icon whose menu opened it
    await expect(page.locator("#hero-win-icon")).toBeFocused();
    await page.locator("#hero-win-icon").click({ button: "right" });
    await page.locator('#hero-win-menu [data-does="Settings"]').click();
    await expect(page.locator('#auto-win [role="tab"][data-pane="settings"]')).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });
});
