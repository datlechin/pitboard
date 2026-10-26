// The menu bar of a Pitboard window in front on macOS (plan section 6, answer 5 a): its menus open
// from their titles by pointer and keyboard as a Mac's do, the Account menu follows the account
// selected, and ⌘⌫ forgets it.
import type { Locator, Page } from "@playwright/test";
import { axe, choose, expect, handedOver, test } from "./fixtures.ts";

const at = { position: { x: 200, y: 12 } };

async function window(page: Page, id = "switch-mac") {
  await choose(page.context(), { platform: "macos" });
  await page.goto("/");
  await handedOver(page);
  const fig = page.locator(`#${id}`);
  await fig.scrollIntoViewIfNeeded();
  const title = (menu: string) => page.locator(`#${id}-am-${menu}`);
  const menu = (menu: string) => page.locator(`#${id}-am-${menu}-menu`);
  return {
    fig,
    title,
    menu,
    items: (name: string) => menu(name).locator(':scope > [role="menuitem"]:not([hidden])'),
    row: (account: string) => fig.locator(`[role="row"][data-account="${account}"]`),
  };
}

// each item's words, its key equivalent after them
const said = (items: Locator) =>
  expect.poll(async () =>
    (await items.allTextContents()).map((t) => t.replace(/\s+/g, " ").trim()),
  );

test.use({ reducedMotion: "reduce" });
test.skip(({ isMobile }) => isMobile, "a pointer and a keyboard");

test("the bar lists the app's menus, and is the window's first Tab stop", async ({ page }) => {
  const { fig, title } = await window(page);
  await expect(fig.getByRole("menubar")).toHaveAccessibleName("Pitboard");
  await expect(fig.getByRole("menubar").getByRole("menuitem")).toHaveText([
    "Pitboard",
    "File",
    "Edit",
    "View",
    "Go",
    "Account",
    "Window",
    "Help",
  ]);
  // the bar, then the window's own controls
  await title("pitboard").focus();
  await page.keyboard.press("Tab");
  await expect(fig.locator('[data-part="window"] :focus')).toHaveCount(1);
  await page.keyboard.press("Shift+Tab");
  await expect(title("pitboard")).toBeFocused();
});

test("a click opens a menu, the pointer along the bar opens the next, and a click outside closes it", async ({
  page,
}) => {
  const { title, menu, items } = await window(page);
  await title("file").click();
  await expect(menu("file")).toBeVisible();
  await expect(title("file")).toHaveAttribute("aria-expanded", "true");
  await said(items("file")).toEqual([
    "Add Account… ⌘N",
    "Open claude.ai",
    "Open chatgpt.com",
    "Remove Website Data…",
    "Close ⌘W",
  ]);
  await title("view").hover();
  await expect(menu("file")).toBeHidden();
  await expect(menu("view")).toBeVisible();
  await page.locator("#switch-title").click();
  await expect(menu("view")).toBeHidden();
  await expect(title("view")).toHaveAttribute("aria-expanded", "false");
});

test("the keyboard moves along the bar, opens a menu on its first item and goes on to the next", async ({
  page,
}) => {
  const { title, menu } = await window(page);
  await title("pitboard").focus();
  await page.keyboard.press("ArrowRight");
  await expect(title("file")).toBeFocused();
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  await expect(title("help")).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(menu("help").getByRole("menuitem", { name: "Pitboard Help" })).toBeFocused();
  await page.keyboard.press("ArrowRight");
  await expect(menu("help")).toBeHidden();
  await expect(menu("pitboard").getByRole("menuitem", { name: "About Pitboard" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(menu("pitboard")).toBeHidden();
  await expect(title("pitboard")).toBeFocused();
});

test("the Account menu offers the account selected, dimmed where it does not apply, and Remove Account on ⌘⌫", async ({
  page,
}) => {
  const { title, menu, items, row } = await window(page);
  // the account in use is selected as the window opens, and asks nothing on ⌘⌫
  await title("account").click();
  await said(items("account")).toEqual([
    "Switch to work",
    "Sign In Again…",
    "Rename…",
    "Warm Up Now",
    "Open claude.ai",
    "Remove Account… ⌘⌫",
  ]);
  for (const name of ["Switch to work", "Warm Up Now", "Remove Account…"])
    await expect(menu("account").getByRole("menuitem", { name })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
  await page.keyboard.press("Escape");
  const alert = page.locator("#mac-accounts-forget");
  await row("work").click(at);
  await page.keyboard.press("Meta+Backspace");
  await expect(alert).toBeHidden();

  await row("personal").click(at);
  await title("account").click();
  await said(items("account")).toEqual([
    "Switch to personal",
    "Sign In Again…",
    "Rename…",
    "Warm Up Now",
    "Open claude.ai",
    "Remove Account… ⌘⌫",
  ]);
  const forget = menu("account").getByRole("menuitem", { name: "Remove Account…" });
  await expect(forget).not.toHaveAttribute("aria-disabled", "true");
  await expect(forget).toHaveAttribute("aria-keyshortcuts", "Meta+Backspace");
  await forget.click();
  await expect(alert).toHaveAccessibleName("Remove “personal (Claude Code)”?");
  await page.keyboard.press("Escape");

  // ⌘⌫ wherever focus is in the window
  await page.locator('#switch-mac [data-part="window"] [data-does="refresh"]').focus();
  await page.keyboard.press("Meta+Backspace");
  await expect(alert).toBeVisible();
  // once it is removed, nothing is selected, and the menu offers what it would, each dimmed
  await alert.locator('[data-does="forget"]').click();
  await title("account").click();
  await said(items("account")).toEqual([
    "Switch",
    "Sign In Again…",
    "Rename…",
    "Remove Account… ⌘⌫",
  ]);
  for (const item of await items("account").all())
    await expect(item).toHaveAttribute("aria-disabled", "true");
});

test("Edit's Copy and Delete act on the selection, and View's Refresh reads", async ({ page }) => {
  const { fig, title, menu, row } = await window(page);
  await title("edit").click();
  const copy = menu("edit").getByRole("menuitem", { name: "Copy" });
  const remove = menu("edit").getByRole("menuitem", { name: "Delete" });
  // the account in use, selected as the window opens
  await expect(copy).not.toHaveAttribute("aria-disabled", "true");
  await expect(remove).not.toHaveAttribute("aria-disabled", "true");
  await page.keyboard.press("Escape");
  await row("personal").click(at);
  await title("edit").click();
  await remove.click();
  await expect(page.locator("#mac-accounts-forget")).toBeVisible();
  await page.keyboard.press("Escape");
  // the page's clock starts a minute after the last read
  await expect(fig.locator('[data-part="updated"]')).toHaveText("Updated 20:50");
  await title("view").click();
  await menu("view").getByRole("menuitem", { name: "Refresh" }).click();
  await expect(fig.locator('[data-part="updated"]')).toHaveText(/^Updated 20:5[1-4]$/);
});

test("File's sites list their accounts, whose windows the app opens", async ({ page }) => {
  const { title, menu } = await window(page);
  await title("file").click();
  await menu("file").getByRole("menuitem", { name: "Open claude.ai" }).hover();
  const sites = page.locator('[id="switch-mac-am-file-claude.ai"]');
  await expect(sites).toBeVisible();
  await expect(sites.getByRole("menuitem")).toHaveText(["work", "personal"]);
});

test("in Settings, nothing is selected, nothing refreshes, and the window neither minimizes nor zooms", async ({
  page,
}) => {
  const { title, menu, items } = await window(page, "auto-mac");
  await title("account").click();
  for (const item of await items("account").all())
    await expect(item).toHaveAttribute("aria-disabled", "true");
  await title("view").hover();
  await expect(menu("view").getByRole("menuitem", { name: "Refresh" })).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  await title("window").hover();
  for (const name of ["Minimize", "Zoom"])
    await expect(menu("window").getByRole("menuitem", { name, exact: true })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
});

test("Quit takes the app's menus off the bar until Reset", async ({ page }) => {
  const { fig, title, menu } = await window(page);
  await title("pitboard").click();
  await menu("pitboard").getByRole("menuitem", { name: "Quit Pitboard" }).click();
  await expect(fig.getByRole("menubar")).toBeHidden();
  await page.locator("figure:has(#switch-mac) [data-reset]").click();
  await expect(fig.getByRole("menubar")).toBeVisible();
});

test("with a menu and its submenu open, the window keeps WAI-ARIA's rules", async ({ page }) => {
  const { title, menu } = await window(page);
  await title("file").click();
  await menu("file").getByRole("menuitem", { name: "Open claude.ai" }).hover();
  expect(await axe(page, ["#switch-mac"])).toEqual([]);
});
