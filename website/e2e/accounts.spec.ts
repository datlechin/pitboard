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
  const rows = platform === "windows" ? "[data-row]" : '[role="row"]';
  return {
    fig,
    row: (account: string) => fig.locator(`${rows}[data-account="${account}"]`),
    // a Windows row's toggle, which keyboard focus takes
    toggle: (account: string) =>
      fig.locator(`${rows}[data-account="${account}"] [data-does="expand"]`),
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
      "Switch to personal",
      "Sign In Again…",
      "Rename…",
      "Open claude.ai",
      "Copy Email Address",
      "Remove Account…",
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
    // the account in use offers neither Switch nor Remove Account
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
    await field.fill("");
    await expect(sheet.locator('[data-part="problem"]')).toHaveText(
      "Type a name for this account.",
    );
    await field.fill("a/b");
    await expect(sheet.locator('[data-part="problem"]')).toHaveText("A name can’t contain “/”.");
    await field.fill("personal");
    await page.keyboard.press("Enter");
    await expect(sheet.locator('[data-part="failure"]')).toHaveText(
      "Couldn’t rename work personal is already the name of me@example.com. Type another name.",
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

  test("Delete asks before removing the selection; Escape and ⌘. cancel, Return presses nothing", async ({
    page,
  }) => {
    const { row, grid, fig } = await accounts(page, "macos");
    const alert = page.locator("#mac-accounts-forget");
    await row("personal").click(at);
    await page.keyboard.press("Delete");
    await expect(alert).toBeVisible();
    await expect(alert).toHaveAccessibleName("Remove “personal (Claude Code)”?");
    await expect(alert.getByRole("button")).toHaveText(["Cancel", "Remove"]);
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
    await alert.getByRole("button", { name: "Remove" }).focus();
    await page.keyboard.press("Enter");
    await expect(alert).toBeVisible();
    await expect(row("personal")).toBeVisible();
    await page.keyboard.press("Space");
    await expect(alert).toBeHidden();
    await expect(row("personal")).toBeHidden();
    // nothing is selected, the list keeps focus, and Down selects the first row
    await expect(grid).toBeFocused();
    await expect(fig.locator('[role="row"][aria-selected="true"]')).toHaveCount(0);
    await page.keyboard.press("ArrowDown");
    await expect(row("work")).toHaveAttribute("aria-selected", "true");
    // work is left alone in its tool, so the window asks for a second, until put off
    const step = fig.locator('[data-part="setup"]');
    await expect(step.locator('[data-part="title"]')).toHaveText(
      "Add a second Claude Code account",
    );
    await step.locator('[data-does="not-now"]').click();
    await expect(step).toBeHidden();
    // the account in use is never removed
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

  test("Add Account signs in to the tool picked in its sheet, then names the account it added", async ({
    page,
  }) => {
    await page.clock.install({ time: new Date("2026-10-13T13:51:00Z") });
    const { fig } = await accounts(page, "macos");
    await fig.locator('[data-does="add"]').first().click();
    const sheet = page.locator("#mac-accounts-sign-in");
    await expect(sheet).toHaveAccessibleName("Add Account");
    // the sign-in comes first: the sheet asks for no name
    await expect(sheet.locator('input[name="name"]')).toHaveCount(0);
    const confirm = sheet.locator('[data-part="confirm"]');
    await expect(confirm).toHaveText("Sign In…");
    await expect(confirm).toBeFocused();
    await sheet.locator('[role="combobox"]').click();
    await sheet.locator('[role="option"][data-value="codex"]').click();
    // NSMenu blinks the item chosen, then fades, before the button takes it
    await expect(sheet.locator('[role="combobox"] [data-value="codex"]')).toBeVisible();
    await expect(sheet.locator('[data-part="message"]')).toHaveText(
      "Pitboard opens Codex’s sign-in in your browser, where you sign in to the account you’re adding.",
    );
    await confirm.click();
    await expect(sheet).toHaveAccessibleName("Signing In to Codex");
    await expect(sheet.locator('[data-part="waiting"]')).toBeVisible();
    await page.clock.runFor(3500);
    await expect(sheet).toBeHidden();
    // added under the name the core picks from the email, which the next sheet offers to change
    const name = page.locator("#mac-accounts-rename");
    await expect(name).toHaveAccessibleName("Name This Account");
    await expect(name.locator('[data-part="message"]')).toHaveText("dana@travel.example is added.");
    const field = name.locator('input[name="name"]');
    await expect(field).toHaveValue("travel");
    await expect(fig.getByRole("row", { name: /^travel\b/ })).toBeVisible();
    await field.fill("main");
    await page.keyboard.press("Enter");
    await expect(name.locator('[data-part="failure"]')).toContainText(
      "main is already the name of me@example.com.",
    );
    // the page's clock ticking on redraws the window, and leaves what the name was told
    await page.clock.runFor(2000);
    await expect(name.locator('[data-part="failure"]')).toBeVisible();
    await field.fill("side");
    await page.keyboard.press("Enter");
    await expect(name).toBeHidden();
    await expect(fig.locator('[data-tool="codex"] [data-account]:not([hidden])')).toHaveCount(3);
    await expect(fig.getByRole("row", { name: /^side\b/ })).toBeVisible();
  });

  test("Claude Desktop's sign-in saves the login in use, and only Later or Switch Back put it away", async ({
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
      const confirm = sheet.locator('[data-part="confirm"]');
      await expect(confirm).toHaveText("Continue");
      await confirm.click();
      await expect(sheet.locator('[data-does="cancel"]')).toHaveText("Later");
      await expect(sheet.locator('[data-does="put-back"]')).toHaveText("Switch Back to home");
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
    await expect(fig.getByRole("row", { name: /^desktop\b/ })).toHaveCount(0);
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
    await activity.getByRole("button", { name: "Refresh" }).click();
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

  test("Tab goes from a row's toggle to its button, Space opens it on its cards, and Enter switches", async ({
    page,
    browserName,
  }) => {
    test.skip(
      process.platform === "darwin" && browserName === "webkit",
      "WebKit on a Mac tabs past buttons, as Safari does until Option-Tab",
    );
    const { row, toggle } = await accounts(page, "windows");
    await toggle("personal").focus();
    await page.keyboard.press("Tab");
    await expect(row("personal").locator('[data-does="use"]')).toBeFocused();
    await page.keyboard.press("Shift+Tab");
    await expect(toggle("personal")).toBeFocused();
    await expect(toggle("personal")).toHaveAttribute("aria-expanded", "false");
    await page.keyboard.press("Space");
    await expect(toggle("personal")).toHaveAttribute("aria-expanded", "true");
    const cards = row("personal").locator('[data-part="details"]');
    await expect(cards).toBeVisible();
    await expect(cards).toContainText("me@example.com");
    await expect(cards).toContainText("Saved sign-in valid until 9 Nov");
    await expect(cards).toContainText("Warm-up settings");
    // each limit with how long until it resets, and what the row's shortcut menu offers
    await expect(cards.locator('[data-part="limit-card"]').first()).toContainText(
      /5-hour limit.*in \d/s,
    );
    await expect(cards.getByRole("button", { name: "Sign in again…" })).toBeVisible();
    await expect(cards.getByRole("button", { name: "Remove account…" })).toBeEnabled();
    await page.keyboard.press("Enter");
    await expect(toggle("personal")).toHaveAccessibleName("personal, Max 20x, in use");
    await expect(cards.getByRole("button", { name: "Remove account…" })).toBeDisabled();
    // a press on its header closes it again
    await row("personal").locator('[data-part="label"]').click();
    await expect(cards).toBeHidden();
  });

  test("Shift+F10 opens the context menu above the row on its first item, its keys as labels", async ({
    page,
  }) => {
    const { row, toggle, menu, offers } = await accounts(page, "windows");
    await toggle("personal").focus();
    await page.keyboard.press("Shift+F10");
    await expect(menu).toBeVisible();
    await expect(offers.first()).toBeFocused();
    await expect(offers).toHaveText([
      "Switch to personal",
      "Sign in again…",
      "Rename…F2",
      "Open claude.ai",
      "Copy email addressCtrl+C",
      "Remove account…Del",
    ]);
    const above = (await row("personal").boundingBox())!;
    const box = (await menu.boundingBox())!;
    expect(box.y + box.height).toBeLessThanOrEqual(above.y + 1);
    await page.keyboard.press("Escape");
    await expect(menu).toBeHidden();
    await expect(toggle("personal")).toBeFocused();
    await row("spare").click({ button: "right", ...at });
    await expect(menu).toBeVisible();
    await expect(offers.first()).toHaveText("Switch to spare");
  });

  test("F2 renames in a ContentDialog that Escape cancels", async ({ page }) => {
    const { row, toggle } = await accounts(page, "windows");
    const dialog = page.locator("#win-accounts-rename");
    await toggle("personal").focus();
    await page.keyboard.press("F2");
    await expect(dialog).toBeVisible();
    const field = dialog.locator('input[name="name"]');
    await expect(field).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
    await expect(toggle("personal")).toBeFocused();
    await page.keyboard.press("F2");
    await field.fill("");
    await expect(dialog.locator('[data-part="problem"]')).toHaveText(
      "Type a name for this account.",
    );
    await field.fill("side");
    await page.keyboard.press("Enter");
    await expect(dialog).toBeHidden();
    await expect(row("personal").locator('[data-part="label"]')).toHaveText("side");
  });

  test("Delete asks with focus on Remove, Enter removes, and focus goes to the title bar", async ({
    page,
  }) => {
    const { row, toggle, fig } = await accounts(page, "windows");
    const dialog = page.locator("#win-accounts-forget");
    await toggle("personal").focus();
    await page.keyboard.press("Delete");
    await expect(dialog).toBeVisible();
    await expect(dialog.getByRole("button")).toHaveText(["Remove", "Cancel"]);
    await expect(dialog.getByRole("button", { name: "Remove" })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(dialog).toBeHidden();
    await expect(row("personal")).toBeHidden();
    const bar = fig.locator('[data-part="title-bar"]');
    await expect(bar).toBeFocused();
    await expect(bar).toHaveAccessibleName("Pitboard");
    // where WinUI draws no focus visual, the page draws one (WCAG 2.4.7)
    await expect(bar).not.toHaveCSS("outline-style", "none");
    // work is left alone in its tool, so a TeachingTip left of Add account asks for a second, once
    // the switch's notice, whose message it would cover, is closed
    const tip = fig.getByRole("dialog", { name: "Add a second Claude Code account" });
    await expect(tip).toBeHidden();
    await fig.locator('[data-notice]:not([hidden]) [data-does="dismiss"]').focus();
    await page.keyboard.press("Enter");
    await expect(tip).toBeVisible();
    await expect(tip.getByRole("button")).toHaveText(["Add account…", "Not now"]);
    const add = (await fig.locator('[data-does="add"]').first().boundingBox())!;
    const tail = (await tip.locator("svg").first().boundingBox())!;
    const body = (await tip.boundingBox())!;
    expect(tail.x).toBeGreaterThanOrEqual(body.x + body.width - 1);
    expect(tail.x + tail.width).toBeCloseTo(add.x, -1);
    expect(tail.y + tail.height / 2).toBeCloseTo(add.y + add.height / 2, -1);
    for (const action of await fig.locator('[data-part="action"]:visible').all())
      expect(body.x + body.width).toBeLessThanOrEqual((await action.boundingBox())!.x);
    await tip.getByRole("button", { name: "Not now" }).click();
    await expect(tip).toBeHidden();
  });

  test("Ctrl+C copies the focused account's email", async ({ page }) => {
    const { toggle } = await accounts(page, "windows");
    await toggle("spare").focus();
    expect(await copied(toggle("spare"))).toBe("me@company.com");
    await expect(toggle("spare")).toHaveAttribute("aria-keyshortcuts", "Delete F2 Control+C");
  });

  test("Add account picks its tool with radio buttons, signs in, then names the account", async ({
    page,
  }) => {
    const { fig } = await accounts(page, "windows");
    await fig.locator('[data-does="add"]').first().click();
    const dialog = page.locator("#win-accounts-sign-in");
    await expect(dialog).toHaveAccessibleName("Add account");
    await expect(dialog.getByRole("radio")).toHaveCount(2);
    await dialog.getByRole("radio", { name: "Codex" }).check();
    const confirm = dialog.locator('[data-part="confirm"]');
    await expect(confirm).toHaveText("Sign in…");
    await confirm.click();
    await expect(dialog).toHaveAccessibleName("Signing in to Codex");
    await expect(dialog).toBeHidden({ timeout: 6000 });
    const name = page.locator("#win-accounts-rename");
    await expect(name).toHaveAccessibleName("Name this account");
    await expect(name.locator('input[name="name"]')).toHaveValue("travel");
    await expect(name.getByRole("button", { name: "Done" })).toBeEnabled();
    await page.keyboard.press("Enter");
    await expect(name).toBeHidden();
    await expect(fig.getByRole("button", { name: /^travel\b/ })).toBeVisible();
  });

  test("with its menu open, a row open and each dialog up, the window keeps WAI-ARIA's rules", async ({
    page,
  }) => {
    const { toggle, menu } = await accounts(page, "windows");
    await toggle("work").click();
    await toggle("personal").focus();
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
