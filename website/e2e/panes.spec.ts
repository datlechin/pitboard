// The windows' panes, Settings' tabs and the windows the menus open (answers 3 a and 4 a): the
// Windows navigation shows each pane drawn from source, macOS draws the accounts and the log in
// windows of their own, Settings' toolbar shows General and Advanced, and About and an account's
// window open over the desk they are asked from.
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
  test("the Accounts window shows only the accounts, and the Activity window the log", async ({
    page,
  }) => {
    const { fig } = await open(page, "macos", "usage-mac");
    await expect(fig.getByRole("tablist")).toHaveCount(0);
    await expect(fig.getByRole("grid", { name: "Accounts" })).toBeVisible();
    const activity = page.locator("#security-mac");
    await activity.scrollIntoViewIfNeeded();
    await expect(activity.locator('[data-part="log"] > *').first()).toContainText("Switch");
    expect(await axe(page, ["#usage-mac", "#security-mac"])).toEqual([]);
  });

  test("the Activity window's File menu adds an account in the Accounts window", async ({
    page,
  }) => {
    const { fig } = await open(page, "macos", "security-mac");
    await fig.locator("#security-mac-am-file").click();
    await fig.locator('[id="security-mac-am-file-menu"] [data-does="add"]').click();
    await expect(page.locator("#mac-accounts-sign-in")).toBeVisible();
  });

  test("Settings' tabs show General and Advanced, whose settings act on the page", async ({
    page,
  }) => {
    const { fig } = await open(page, "macos", "auto-mac");
    await fig.getByRole("tab", { name: "General" }).click();
    await expect(fig.locator('[data-part="title"]')).toHaveText("General");
    await expect(fig.getByRole("switch", { name: "Open Pitboard at Login" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
    const installs = fig.getByRole("switch", { name: "Download and Install Automatically" });
    await fig.getByRole("switch", { name: "Check for Updates Automatically" }).click();
    await expect(installs).toHaveAttribute("aria-disabled", "true");
    await fig.locator("#auto-mac-shows").click();
    await page.locator("#auto-mac-shows-usage").click();
    await expect(page.locator('#hero-mac-item [data-part="title"]')).toHaveText("40%");
    await page.keyboard.press("ArrowRight");
    await fig.getByRole("tab", { name: "Advanced" }).click();
    await expect(fig.locator('[data-part="title"]')).toHaveText("Advanced");
    await expect(fig).toContainText("/opt/homebrew/bin/pitboard");
    await fig.getByRole("switch", { name: "Keep Saved Sign-Ins Fresh" }).click();
    // the other Settings figure is the same window
    const other = page.locator("#warm-mac");
    await other.scrollIntoViewIfNeeded();
    await other.getByRole("tab", { name: "Advanced" }).click();
    await expect(other.getByRole("switch", { name: "Keep Saved Sign-Ins Fresh" })).toHaveAttribute(
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
    // removed, from the window in #usage, its window goes too
    await page.locator('#usage-mac [role="row"][data-account="personal"]').click(at);
    await page.keyboard.press("Meta+Backspace");
    await page.locator("#mac-accounts-forget").getByRole("button", { name: "Remove" }).click();
    await expect(window).toHaveCount(0);
  });

  test("the Open Link window lists claude.ai's accounts as the state has them, and Open opens the one selected in its window", async ({
    page,
  }) => {
    const { fig } = await open(page, "macos", "windows-mac");
    const list = fig.getByRole("listbox", { name: "Open this claude.ai link as:" });
    const option = (account: string) => list.locator(`[role="option"][data-account="${account}"]`);
    await expect(list.getByRole("option")).toHaveText([
      /^\s*work\s+me@company\.com, window open\s*$/,
      /^\s*personal\s+me@example\.com\s*$/,
    ]);
    await expect(option("work")).toHaveAttribute("aria-selected", "true");
    expect(await axe(page, ["#windows-mac"])).toEqual([]);
    // a rename in the Accounts window shows in the list
    const row = page.locator('#usage-mac [role="row"][data-account="personal"]');
    await row.click({ button: "right", ...at });
    await page.locator('#usage-mac-menu [data-offer="rename"]').click();
    await page.locator('#mac-accounts-rename input[name="name"]').fill("side");
    await page.keyboard.press("Enter");
    await expect(option("personal").locator('[data-part="label"]')).toHaveText("side");
    await option("work").click();
    await expect(option("work")).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(option("personal")).toHaveAttribute("aria-selected", "true");
    // Return presses Open, the default button
    await page.keyboard.press("Enter");
    await expect(fig.locator('[data-part="window"]')).toBeHidden();
    const window = fig.locator('[data-part="account-window"][data-account="personal"]');
    await expect(window).toBeVisible();
    await expect(window.locator('[data-part="title"]')).toHaveText("side");
  });

  test("Escape, Cancel or the close button put the link away, and Reset brings it back", async ({
    page,
  }) => {
    const { fig } = await open(page, "macos", "windows-mac");
    const window = fig.locator('[data-part="window"]');
    const reset = page.locator("figure:has(#windows-mac) [data-reset]");
    await fig.getByRole("option", { name: /^personal/ }).click();
    await page.keyboard.press("Escape");
    await expect(window).toBeHidden();
    await expect(reset).toBeFocused();
    await reset.click();
    await expect(window).toBeVisible();
    // the selection is the account in use again
    await expect(fig.getByRole("option", { name: /^work/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await window.getByRole("button", { name: "Cancel" }).click();
    await expect(window).toBeHidden();
    await reset.click();
    await window.getByRole("button", { name: "Close" }).click();
    await expect(window).toBeHidden();
  });
});

test.describe("Windows", () => {
  test("the navigation moves focus with the arrows, wrapping, and Enter shows the page", async ({
    page,
  }) => {
    const { tab, panel } = await open(page, "windows", "usage-win");
    await tab("accounts").focus();
    await page.keyboard.press("ArrowDown");
    await expect(tab("settings")).toBeFocused();
    await expect(tab("accounts")).toHaveAttribute("aria-selected", "true");
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
    await panel("settings").getByRole("switch", { name: "Keep saved sign-ins fresh" }).click();
    await expect(
      panel("settings").getByRole("switch", { name: "Keep saved sign-ins fresh" }),
    ).toHaveAttribute("aria-checked", "true");
    expect(await axe(page, ["#usage-win"])).toEqual([]);
  });

  test("Troubleshoot's cards open Diagnostics and Activity under Settings, and back or the breadcrumb goes back", async ({
    page,
  }) => {
    const { fig, tab, panel } = await open(page, "windows", "usage-win");
    const back = fig.getByRole("button", { name: "Back" });
    await expect(back).toBeHidden();
    await tab("settings").click();
    await panel("settings").getByRole("button", { name: "Diagnostics" }).click();
    await expect(panel("machine")).toBeVisible();
    await expect(tab("settings")).toHaveAttribute("aria-selected", "true");
    await expect(panel("machine")).toContainText("Checking this PC…");
    await expect(panel("machine").getByRole("button", { name: "Check again" })).toBeDisabled();
    await expect(panel("machine").getByRole("button", { name: "Copy report" })).toBeDisabled();
    await back.click();
    await expect(panel("settings")).toBeVisible();
    await expect(back).toBeHidden();
    await panel("settings")
      .getByRole("button", { name: "Activity" })
      .and(page.getByRole("button", { description: "Every switch and change Pitboard made." }))
      .click();
    await expect(panel("activity")).toBeVisible();
    await expect(panel("activity")).toContainText("Settings");
    expect(await axe(page, ["#usage-win"])).toEqual([]);
    await panel("activity").getByRole("button", { name: "Settings" }).click();
    await expect(panel("settings")).toBeVisible();
  });

  test("the navigation's pane button opens it with its labels, and a page chosen closes it", async ({
    page,
  }) => {
    const { fig, tab, panel } = await open(page, "windows", "usage-win");
    await fig.getByRole("button", { name: "Open Navigation" }).click();
    await expect(fig.getByRole("button", { name: "Close Navigation" })).toBeVisible();
    await expect(tab("settings")).toContainText("Settings");
    await tab("settings").click();
    await expect(panel("settings")).toBeVisible();
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

  test("#account-windows' desk opens on Open claude.ai's submenu of the state's accounts, and one chosen opens its window", async ({
    page,
  }) => {
    const { fig } = await open(page, "windows", "windows-win");
    const submenu = page.locator('[id="windows-win-claude.ai-menu"]');
    // a press elsewhere on the page leaves it open until the visitor comes to it
    await page.locator("#hero-title").click();
    await expect(submenu).toBeVisible();
    await expect(
      page.locator('#windows-win-menu [data-does="claude.ai"]:not([data-one])'),
    ).toHaveAttribute("aria-expanded", "true");
    await expect(submenu.getByRole("menuitem")).toHaveText(["work", "personal"]);
    expect(await axe(page, ["#windows-win"])).toEqual([]);
    await submenu.locator('[data-account="personal"]').click();
    await expect(submenu).toBeHidden();
    const window = fig.locator('[data-part="account-window"][data-account="personal"]');
    await expect(window.locator('[data-part="title"]')).toHaveText("personal");
    // the visitor's pointer took over from the one drawn, and Reset opens the submenu again
    await expect(fig.locator('[data-part="pointer"]')).toHaveAttribute("data-state", "gone");
    await page.locator("figure:has(#windows-win) [data-reset]").click();
    await expect(window).toHaveCount(0);
    await expect(submenu).toBeVisible();
  });
});
