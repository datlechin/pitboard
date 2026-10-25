// The Accounts windows: the list's selection and keys on each system, the shortcut and context
// menus by mouse, keyboard and touch, the alert, sheets and dialogs they ask in, and the Activity
// pane that reads the log (plan 3.3 to 3.5, 3.8, section 6).
import type { Locator, Page } from "@playwright/test";
import { axe, choose, expect, handedOver, test, type Platform } from "./fixtures.ts";

const at = { position: { x: 200, y: 12 } };

async function accounts(page: Page, platform: Platform) {
  await choose(page.context(), { platform });
  await page.goto("/");
  // the demo's switch to work is made, so the state stands still under each test
  await handedOver(page);
  const id = platform === "macos" ? "usage-mac" : "usage-win";
  const fig = page.locator(`#${id}`);
  await fig.scrollIntoViewIfNeeded();
  return {
    fig,
    row: (account: string) => fig.locator(`[role="row"][data-account="${account}"]`),
    menu: page.locator(`#${id}-menu`),
    offers: page.locator(`#${id}-menu [role="menuitem"]:not([hidden])`),
    grid: fig.locator('[role="grid"]'),
  };
}

// what ⌘C or Ctrl+C would put on the clipboard, read from the copy event they send
const copied = (on: Locator) =>
  on.evaluate((el) => {
    const copy = new ClipboardEvent("copy", {
      clipboardData: new DataTransfer(),
      bubbles: true,
      cancelable: true,
    });
    el.dispatchEvent(copy);
    return copy.defaultPrevented ? copy.clipboardData?.getData("text/plain") : null;
  });

test.describe("on macOS", () => {
  test.use({ reducedMotion: "reduce" });
  test.skip(({ isMobile }) => isMobile, "a pointer and a keyboard");

  test("a click selects, Up and Down move the selection, a name typed selects, and Return switches", async ({
    page,
  }) => {
    const { row } = await accounts(page, "macos");
    await row("personal").click(at);
    await expect(row("personal")).toHaveAttribute("aria-selected", "true");
    await expect(row("personal")).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(row("main")).toHaveAttribute("aria-selected", "true");
    await expect(row("personal")).toHaveAttribute("aria-selected", "false");
    // a List has no Home or End
    await page.keyboard.press("End");
    await expect(row("main")).toBeFocused();
    await page.keyboard.press("o");
    await expect(row("office")).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("ArrowUp");
    await expect(row("spare")).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(row("spare")).toHaveAttribute("aria-label", "spare, Plus, in use");
    // the one in use has no action
    await page.keyboard.press("Enter");
    await expect(row("main")).toHaveAttribute("aria-label", "main, Pro");
  });

  test("a right click opens the row's shortcut menu where it is, and leaves the selection", async ({
    page,
  }) => {
    const { row, menu, offers, fig } = await accounts(page, "macos");
    await row("work").click(at);
    await row("personal").click({ button: "right", ...at });
    await expect(menu).toBeVisible();
    await expect(offers).toHaveText([
      "Use personal",
      "Sign In Again…",
      "Rename…",
      "Open claude.ai",
      "Copy Email Address",
      "Forget…",
    ]);
    await expect(row("work")).toHaveAttribute("aria-selected", "true");
    await expect(row("personal")).toHaveAttribute("aria-selected", "false");
    // the row it acts on is outlined while it is open
    await expect(row("personal")).toHaveAttribute("data-state", "menu");
    await expect(row("work")).not.toHaveAttribute("data-state", "menu");
    const desk = (await fig.boundingBox())!;
    const box = (await menu.boundingBox())!;
    expect(box.x + box.width).toBeLessThanOrEqual(desk.x + desk.width);
    await page.keyboard.press("Escape");
    await expect(menu).toBeHidden();
    await expect(row("personal")).not.toHaveAttribute("data-state", "menu");
    // the account in use offers neither Use nor Forget
    await row("work").click({ button: "right", ...at });
    await expect(offers).toHaveText([
      "Sign In Again…",
      "Rename…",
      "Open claude.ai",
      "Copy Email Address",
    ]);
    // a press anywhere else closes it
    await page.locator("#usage-title").click();
    await expect(menu).toBeHidden();
  });

  test("a Control-click is the Mac's right click, and no key opens the menu", async ({
    page,
    browserName,
  }) => {
    test.skip(
      process.platform !== "darwin" || browserName === "firefox",
      "a browser makes a Control-click a right click on a Mac; Playwright's Firefox sends none",
    );
    const { row, menu } = await accounts(page, "macos");
    await row("personal").click({ modifiers: ["Control"], ...at });
    await expect(menu).toBeVisible();
    await expect(row("personal")).toHaveAttribute("aria-selected", "false");
    await page.keyboard.press("Escape");
    await row("personal").click(at);
    await page.keyboard.press("Shift+F10");
    await expect(menu).toBeHidden();
  });

  test("Rename… asks in a sheet, keeps a name taken there, and a name shows as text everywhere", async ({
    page,
  }) => {
    const { row, menu } = await accounts(page, "macos");
    await row("work").click({ button: "right", ...at });
    await menu.locator('[data-offer="rename"]').click();
    const sheet = page.locator("#mac-accounts-rename");
    await expect(sheet).toBeVisible();
    await expect(sheet).toHaveAccessibleName("Rename “work”");
    const field = sheet.locator('input[name="name"]');
    await expect(field).toBeFocused();
    await expect(field).toHaveValue("work");
    await expect(sheet.locator('button[value="rename"]')).toBeDisabled();
    await field.fill("a/b");
    await expect(sheet.locator('[data-part="problem"]')).toHaveText("A name can’t contain “/”.");
    await field.fill("personal");
    await page.keyboard.press("Enter");
    await expect(sheet.locator('[data-part="failure"]')).toHaveText(
      "Couldn’t rename work personal already refers to me@example.com. Choose a different label.",
    );
    await expect(sheet).toBeVisible();
    await field.fill("<b>x&y");
    await page.keyboard.press("Enter");
    await expect(sheet).toBeHidden();
    await expect(row("work").locator('[data-part="label"]')).toHaveText("<b>x&y");
    await expect(page.locator("#hero-mac-item")).toHaveAttribute(
      "aria-label",
      "Pitboard, <b>x&y 40%, Claude Code",
    );
    await expect(page.locator('[data-output] b:text-is("x&y")')).toHaveCount(0);
  });

  test("Delete asks before forgetting the selection; Escape and ⌘. cancel, Return presses nothing", async ({
    page,
  }) => {
    const { row, grid, fig } = await accounts(page, "macos");
    const alert = page.locator("#mac-accounts-forget");
    await row("personal").click(at);
    await page.keyboard.press("Delete");
    await expect(alert).toBeVisible();
    await expect(alert).toHaveAccessibleName("Forget “personal (Claude Code)”?");
    await expect(alert.getByRole("button")).toHaveText(["Cancel", "Forget"]);
    await page.keyboard.press("Escape");
    await expect(alert).toBeHidden();
    await expect(row("personal")).toBeFocused();
    await page.keyboard.press("Backspace");
    await expect(alert).toBeVisible();
    await page.keyboard.press("Meta+.");
    await expect(alert).toBeHidden();
    await page.keyboard.press("Delete");
    // neither button is the default, so Return presses neither, wherever focus is
    await page.keyboard.press("Enter");
    await expect(alert).toBeVisible();
    await alert.getByRole("button", { name: "Forget" }).focus();
    await page.keyboard.press("Enter");
    await expect(alert).toBeVisible();
    await expect(row("personal")).toBeVisible();
    await page.keyboard.press("Space");
    await expect(alert).toBeHidden();
    await expect(row("personal")).toBeHidden();
    // nothing is selected, the list keeps focus, and Down selects the first row
    await expect(grid).toBeFocused();
    await expect(fig.locator('[aria-selected="true"]')).toHaveCount(0);
    await page.keyboard.press("ArrowDown");
    await expect(row("work")).toHaveAttribute("aria-selected", "true");
    // work is left alone in its tool, so the window asks for a second, until put off
    const step = fig.locator('[data-part="setup"]');
    await expect(step.locator('[data-part="title"]')).toHaveText(
      "Add a second Claude Code account",
    );
    await step.locator('[data-does="not-now"]').click();
    await expect(step).toBeHidden();
    // the account in use is never forgotten
    await row("work").click(at);
    await page.keyboard.press("Delete");
    await expect(alert).toBeHidden();
  });

  test("⌘C copies the selected account's email", async ({ page }) => {
    const { row } = await accounts(page, "macos");
    await row("personal").click(at);
    expect(await copied(row("personal"))).toBe("me@example.com");
    await expect(row("personal")).toHaveAttribute("aria-keyshortcuts", "Backspace Delete Meta+C");
  });

  test("Add Account signs in to the tool picked in its sheet, and lists the account", async ({
    page,
  }) => {
    await page.clock.install({ time: new Date("2026-10-13T13:51:00Z") });
    const { fig } = await accounts(page, "macos");
    await fig.locator('[data-does="add"]').first().click();
    const sheet = page.locator("#mac-accounts-sign-in");
    await expect(sheet).toHaveAccessibleName("Add Account");
    await expect(sheet.locator('input[name="name"]')).toBeFocused();
    const confirm = sheet.locator('[data-part="confirm"]');
    await expect(confirm).toBeDisabled();
    await sheet.locator('[role="combobox"]').click();
    await sheet.locator('[role="option"][data-value="codex"]').click();
    // NSMenu blinks the item chosen, then fades, before the button takes it
    await expect(sheet.locator('[role="combobox"] [data-value="codex"]')).toBeVisible();
    await sheet.locator('input[name="name"]').fill("main");
    await expect(confirm).toBeEnabled();
    await page.keyboard.press("Enter");
    await expect(sheet.locator('[data-part="problem"]')).toContainText("main already refers to");
    // the page's clock ticking on redraws the sheet, and leaves what the name was told
    await page.clock.runFor(2000);
    await expect(sheet.locator('[data-part="problem"]')).toContainText("main already refers to");
    await sheet.locator('input[name="name"]').fill("side");
    await page.keyboard.press("Enter");
    await expect(sheet).toHaveAccessibleName("Signing In to Codex");
    await expect(sheet.locator('[data-part="waiting"]')).toBeVisible();
    await expect(sheet).toBeHidden({ timeout: 6000 });
    await expect(fig.locator('[data-tool="codex"] [data-account]:not([hidden])')).toHaveCount(3);
    await expect(fig.getByRole("row", { name: /^side\b/ })).toBeVisible();
  });

  test("Claude Desktop's sign-in parks the login in use, and only Later or Put Back put it away", async ({
    page,
  }) => {
    await page.clock.install({ time: new Date("2026-10-13T13:51:00Z") });
    const { fig, row } = await accounts(page, "macos");
    const sheet = page.locator("#mac-accounts-sign-in");
    const window = fig.locator('[data-part="window"]');
    const start = async () => {
      await fig.locator('[data-does="add"]').first().click();
      await sheet.locator('[role="combobox"]').click();
      await sheet.locator('[role="option"][data-value="desktop"]').click();
      await expect(sheet.locator('[role="combobox"] [data-value="desktop"]')).toBeVisible();
      await sheet.locator('input[name="name"]').fill("side");
      const confirm = sheet.locator('[data-part="confirm"]');
      await expect(confirm).toHaveText("Park home");
      await confirm.click();
      await expect(sheet.locator('[data-does="cancel"]')).toHaveText("Later");
      await expect(sheet.locator('[data-does="put-back"]')).toHaveText("Put home Back");
    };
    await start();
    await expect(window).toHaveAttribute("data-modal", "");
    await expect(window.locator(':scope > [data-part="smoke"]')).toBeVisible();
    // a browser closes a modal dialog itself on a second Escape with no press between
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    await expect(sheet).toBeVisible();
    await sheet.locator('[data-does="cancel"]').click();
    await expect(sheet).toBeHidden();
    await expect(window).not.toHaveAttribute("data-modal");
    await page.clock.runFor(3500);
    await expect(fig.getByRole("row", { name: /^side\b/ })).toHaveCount(0);
    await start();
    await sheet.locator('[data-does="put-back"]').click();
    await expect(sheet).toBeHidden();
    await expect(row("home")).toHaveAttribute("aria-label", /in use/);
  });

  test("a switch leaves a notice that its close button dismisses", async ({ page }) => {
    const { fig } = await accounts(page, "macos");
    const notice = fig.locator('[data-notice="claude"]');
    await expect(notice.locator('[data-part="title"]')).toHaveText("Switched Claude Code to work");
    await notice.getByRole("button", { name: "Dismiss" }).click();
    await expect(notice).toBeHidden();
    await expect(fig.locator('[data-part="first"]')).toHaveAttribute("data-state", "top");
  });

  test("Activity reads the log as it comes into view and on Refresh, and not as it changes", async ({
    page,
  }) => {
    const { row } = await accounts(page, "macos");
    const activity = page.locator("#security-mac");
    const first = activity.locator('[data-part="log"] > *').first().locator(":scope > *");
    await activity.scrollIntoViewIfNeeded();
    const read = (account: string) => [/ at 20:5\d$/, "Switch", account, "Done", "Pitboard app"];
    await expect(first).toHaveText(read("work"));
    await row("personal").locator('[data-does="use"]').dispatchEvent("click");
    await expect(row("personal")).toHaveAttribute("aria-label", /in use/);
    await expect(first).toHaveText(read("work"));
    await activity.locator('[data-does="refresh"]').click();
    await expect(first).toHaveText(read("personal"));
  });

  test("with its menu open and each sheet up, the window keeps WAI-ARIA's rules", async ({
    page,
  }) => {
    const { row, menu } = await accounts(page, "macos");
    await row("personal").click({ button: "right", ...at });
    await expect(menu).toBeVisible();
    expect(await axe(page, ["#usage-mac"])).toEqual([]);
    await menu.locator('[data-offer="forget"]').click();
    await expect(page.locator("#mac-accounts-forget")).toBeVisible();
    expect(await axe(page, ["#mac-accounts-forget"])).toEqual([]);
    await page.keyboard.press("Escape");
    await page.locator("#usage-mac").locator('[data-does="add"]').first().click();
    await expect(page.locator("#mac-accounts-sign-in")).toBeVisible();
    expect(await axe(page, ["#mac-accounts-sign-in"])).toEqual([]);
    await page.keyboard.press("Escape");
    await row("personal").click({ button: "right", ...at });
    await menu.locator('[data-offer="rename"]').click();
    await expect(page.locator("#mac-accounts-rename")).toBeVisible();
    expect(await axe(page, ["#mac-accounts-rename"])).toEqual([]);
  });
});

test.describe("on Windows", () => {
  test.use({ reducedMotion: "reduce" });
  test.skip(({ isMobile }) => isMobile, "a pointer and a keyboard");

  test("the selection follows focus, Ctrl with an arrow moves focus alone, and Enter switches", async ({
    page,
  }) => {
    const { row } = await accounts(page, "windows");
    await row("work").click(at);
    await page.keyboard.press("ArrowDown");
    await expect(row("personal")).toBeFocused();
    await expect(row("personal")).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("Control+ArrowDown");
    await expect(row("main")).toBeFocused();
    await expect(row("personal")).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("Control+Space");
    await expect(row("main")).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("End");
    await expect(row("spare")).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("Home");
    await expect(row("work")).toHaveAttribute("aria-selected", "true");
    // Tab goes from the row to its button and out of the list
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Tab");
    await expect(row("personal").locator('[data-does="use"]')).toBeFocused();
    await page.keyboard.press("Shift+Tab");
    await page.keyboard.press("Enter");
    await expect(row("personal")).toHaveAttribute("aria-label", "personal, Max 20x, in use");
  });

  test("Shift+F10 opens the context menu above the row on its first item, its keys as labels", async ({
    page,
  }) => {
    const { row, menu, offers } = await accounts(page, "windows");
    await row("personal").click(at);
    await page.keyboard.press("Shift+F10");
    await expect(menu).toBeVisible();
    await expect(offers.first()).toBeFocused();
    await expect(offers).toHaveText([
      "Use personal",
      "Sign in again…",
      "Rename…F2",
      "Open claude.ai",
      "Copy email addressCtrl+C",
      "Forget…Del",
    ]);
    const above = (await row("personal").boundingBox())!;
    const box = (await menu.boundingBox())!;
    expect(box.y + box.height).toBeLessThanOrEqual(above.y + 1);
    await page.keyboard.press("Escape");
    await expect(menu).toBeHidden();
    await expect(row("personal")).toBeFocused();
    await row("spare").click({ button: "right", ...at });
    await expect(menu).toBeVisible();
    await expect(row("personal")).toHaveAttribute("aria-selected", "true");
  });

  test("F2 renames in a ContentDialog that Escape cancels", async ({ page }) => {
    const { row } = await accounts(page, "windows");
    const dialog = page.locator("#win-accounts-rename");
    await row("personal").click(at);
    await page.keyboard.press("F2");
    await expect(dialog).toBeVisible();
    const field = dialog.locator('input[name="name"]');
    await expect(field).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
    await expect(row("personal")).toBeFocused();
    await page.keyboard.press("F2");
    await field.fill("");
    await expect(dialog.locator('[data-part="problem"]')).toHaveText(
      "Type a name for this account to rename.",
    );
    await field.fill("side");
    await page.keyboard.press("Enter");
    await expect(dialog).toBeHidden();
    await expect(row("personal").locator('[data-part="label"]')).toHaveText("side");
  });

  test("Delete asks with focus on Forget, Enter forgets, and focus goes to the title bar", async ({
    page,
  }) => {
    const { row, fig } = await accounts(page, "windows");
    const dialog = page.locator("#win-accounts-forget");
    await row("personal").click(at);
    await page.keyboard.press("Delete");
    await expect(dialog).toBeVisible();
    await expect(dialog.getByRole("button")).toHaveText(["Forget", "Cancel"]);
    await expect(dialog.getByRole("button", { name: "Forget" })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(dialog).toBeHidden();
    await expect(row("personal")).toBeHidden();
    // nothing is selected, and Down moves nothing
    const bar = fig.locator('[data-part="title-bar"]');
    await expect(bar).toBeFocused();
    await expect(bar).toHaveAccessibleName("Pitboard");
    // where WinUI draws no focus visual, the page draws one (WCAG 2.4.7)
    await expect(bar).not.toHaveCSS("outline-style", "none");
    await expect(fig.locator('[aria-selected="true"]')).toHaveCount(0);
    await page.keyboard.press("ArrowDown");
    await expect(bar).toBeFocused();
    await expect(fig.locator('[aria-selected="true"]')).toHaveCount(0);
  });

  test("Ctrl+C copies the focused account's email", async ({ page }) => {
    const { row } = await accounts(page, "windows");
    await row("spare").click(at);
    expect(await copied(row("spare"))).toBe("me@company.com");
    await expect(row("spare")).toHaveAttribute("aria-keyshortcuts", "Delete F2 Control+C");
  });

  test("Add account picks its tool with radio buttons and lists the account", async ({ page }) => {
    const { fig } = await accounts(page, "windows");
    await fig.locator('[data-does="add"]').first().click();
    const dialog = page.locator("#win-accounts-sign-in");
    await expect(dialog).toHaveAccessibleName("Add account");
    await expect(dialog.getByRole("radio")).toHaveCount(2);
    await dialog.getByRole("radio", { name: "Codex" }).check();
    await dialog.locator('input[name="name"]').fill("side");
    await page.keyboard.press("Enter");
    await expect(dialog).toHaveAccessibleName("Signing in to Codex");
    await expect(dialog).toBeHidden({ timeout: 6000 });
    await expect(fig.getByRole("row", { name: /^side\b/ })).toBeVisible();
  });

  test("with its menu open and each dialog up, the window keeps WAI-ARIA's rules", async ({
    page,
  }) => {
    const { row, menu } = await accounts(page, "windows");
    await row("personal").click(at);
    await page.keyboard.press("Shift+F10");
    await expect(menu).toBeVisible();
    expect(await axe(page, ["#usage-win"])).toEqual([]);
    await page.keyboard.press("Escape");
    await page.keyboard.press("F2");
    expect(await axe(page, ["#win-accounts-rename"])).toEqual([]);
    await page.keyboard.press("Escape");
    await page.keyboard.press("Delete");
    expect(await axe(page, ["#win-accounts-forget"])).toEqual([]);
    await page.keyboard.press("Escape");
    await page.locator("#usage-win").locator('[data-does="add"]').first().click();
    await expect(page.locator("#win-accounts-sign-in")).toBeVisible();
    expect(await axe(page, ["#win-accounts-sign-in"])).toEqual([]);
  });
});

test.describe("on a phone", () => {
  test.use({ reducedMotion: "reduce" });
  test.skip(({ isMobile }) => !isMobile, "a touch");

  test("a long press opens a row's shortcut menu, and a tap selects", async ({
    page,
    browserName,
  }) => {
    test.skip(browserName !== "chromium", "Chromium's own long press, sent through CDP");
    const { row, menu } = await accounts(page, "macos");
    const box = (await row("personal").boundingBox())!;
    const cdp = await page.context().newCDPSession(page);
    const point = { x: box.x + box.width / 2, y: box.y + 10 };
    await cdp.send("Input.synthesizeTapGesture", { ...point, duration: 800 });
    await expect(menu).toBeVisible();
    await expect(row("personal")).toHaveAttribute("aria-selected", "false");
    await cdp.send("Input.synthesizeTapGesture", { x: 5, y: point.y });
    await expect(menu).toBeHidden();
    // Linux's headless shell makes no click of a tap CDP synthesizes
    await page.touchscreen.tap(point.x, point.y);
    await expect(row("personal")).toHaveAttribute("aria-selected", "true");
  });

  test("a touch held still opens it where iOS sends no contextmenu, and the click after is swallowed", async ({
    page,
    browserName,
  }) => {
    test.skip(browserName !== "webkit", "iOS Safari's touch");
    const { row, menu } = await accounts(page, "macos");
    await row("personal").evaluate(async (el) => {
      const r = el.getBoundingClientRect();
      const at = { clientX: r.left + 40, clientY: r.top + 10, bubbles: true, pointerType: "touch" };
      el.dispatchEvent(new PointerEvent("pointerdown", at));
      await new Promise((done) => setTimeout(done, 650));
      el.dispatchEvent(new PointerEvent("pointerup", at));
      el.dispatchEvent(new MouseEvent("click", { ...at, detail: 1 }));
    });
    await expect(menu).toBeVisible();
    await expect(row("personal")).toHaveAttribute("aria-selected", "false");
  });
});
