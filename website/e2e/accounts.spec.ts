// The Accounts windows: the list's selection and keys on each system, the macOS inspector of the
// account selected, the shortcut and context menus by mouse, keyboard and touch, the alert, sheets
// and dialogs they ask in, and the Activity window and pane that read the log (plan 3.3 to 3.5,
// 3.8, section 6; app redo 4.3).
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

// whether a WinUI button is drawn in the accent, as the Rename dialog's Rename is
const accent = (button: Locator) =>
  button.evaluate((el) => {
    const rename = el.ownerDocument.querySelector('#win-accounts-rename [data-part="confirm"]')!;
    return getComputedStyle(el).backgroundColor === getComputedStyle(rename).backgroundColor;
  });

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
    await row("work").click({ modifiers: ["Control"], ...at });
    await expect(menu).toBeVisible();
    await expect(row("work")).toHaveAttribute("aria-selected", "false");
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
    await expect(sheet.locator('[data-part="wait-line"]')).toHaveText(
      "Finish signing in in your browser.",
    );
    await expect(sheet.getByRole("img", { name: "Waiting for your browser" })).toBeVisible();
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
      await expect(sheet.locator('[data-part="step"]:not([hidden]) [data-part="said"]')).toHaveText(
        [
          "Open Claude.",
          "Sign in to the account you’re adding.",
          "Once it shows your chats, quit Claude with Command-Q, and Pitboard adds it.",
        ],
      );
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
  });

  test("the inspector shows the account selected, and the account in use of the first tool at first", async ({
    page,
  }) => {
    const { fig, row } = await accounts(page, "macos");
    const inspector = fig.locator('[data-part="inspector"]');
    await expect(row("personal")).toHaveAttribute("aria-selected", "true");
    await expect(inspector.getByRole("heading")).toHaveText("personal");
    await expect(
      inspector.getByRole("img", { name: /^5-hour limit, 59 percent used/ }),
    ).toBeVisible();
    await expect(inspector.locator('[data-part="ins-sign-in"]')).toHaveText(
      "Saved sign-in valid until 9 Nov",
    );
    await expect(inspector.locator('[data-part="ins-warm"]')).toHaveText("Warms up again at 22:00");
    await row("main").click(at);
    await expect(inspector.getByRole("heading")).toHaveText("main");
    await expect(inspector.locator('[data-part="ins-sign-in"]')).toHaveText("In use");
    await expect(inspector.locator('[data-part="ins-warm-row"]')).toBeHidden();
    await expect(inspector.getByRole("button", { name: "Switch to main" })).toBeHidden();
    await row("spare").click(at);
    await inspector.getByRole("button", { name: "Switch to spare" }).click();
    await expect(row("spare")).toHaveAttribute("aria-label", "spare, Plus, in use");
    await expect(inspector.getByRole("button", { name: "Switch to spare" })).toBeHidden();
  });

  test("Warm-Up Settings… sits beside Off and under a note too long for it, as LabeledContent puts it", async ({
    page,
  }) => {
    const { fig, row } = await accounts(page, "macos");
    const inspector = fig.locator('[data-part="inspector"]');
    const note = inspector.locator('[data-part="ins-warm"]');
    const button = inspector.getByRole("button", { name: "Warm-Up Settings…" });
    await expect(note).toHaveText("Warms up again at 22:00");
    const long = (await note.boundingBox())!;
    expect((await button.boundingBox())!.y).toBeGreaterThan(long.y + long.height);
    await row("work").click(at);
    await expect(note).toHaveText("Off");
    expect((await button.boundingBox())!.y).toBeLessThan((await note.boundingBox())!.y);
    // it opens Settings on Limits, which the figure cut at Warm-Up draws
    await button.click();
    await expect(page.locator("#warm-mac")).toBeInViewport();
  });

  test("the toolbar's Inspector and ⌃⌘I hide the inspector and show it again", async ({ page }) => {
    const { fig, grid } = await accounts(page, "macos");
    const inspector = fig.locator('[data-part="inspector"]');
    const button = fig.getByRole("button", { name: "Inspector" });
    await expect(button).toHaveAttribute("aria-expanded", "true");
    await button.click();
    await expect(inspector).toBeHidden();
    await expect(button).toHaveAttribute("aria-expanded", "false");
    await grid.focus();
    await page.keyboard.press("Control+Meta+KeyI");
    await expect(inspector).toBeVisible();
    await expect(button).toHaveAttribute("aria-expanded", "true");
  });

  test("the Activity window reads the log as it comes into view and on Refresh, and not as it changes", async ({
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
    // Firefox's boxes are single precision (docs/facts/site.md)
    expect(box.y + box.height).toBeLessThanOrEqual(above.y + 1 + 1e-3);
    await page.keyboard.press("Escape");
    await expect(menu).toBeHidden();
    await expect(toggle("personal")).toBeFocused();
    await row("spare").click({ button: "right", ...at });
    await expect(menu).toBeVisible();
    await expect(offers.first()).toHaveText("Switch to spare");
  });

  test("an account's Warm-up settings opens the warm-up's expander at the page's top, with the focus on it", async ({
    page,
  }) => {
    const { fig, toggle } = await accounts(page, "windows");
    await toggle("personal").click();
    await fig.getByRole("button", { name: "Warm-up settings" }).first().click();
    const settings = fig.locator('[data-pane-panel="settings"]');
    await expect(settings).toBeVisible();
    const expand = settings.locator('[data-section="warm"] [data-expander]');
    await expect(expand).toHaveAttribute("aria-expanded", "true");
    await expect(expand).toBeFocused();
    // at the top, or as far up as the page scrolls
    const placed = await settings.locator('[data-part="scroller"]').evaluate((scroller) => {
      const section = scroller.querySelector('[data-section="warm"]')!;
      const gap = section.getBoundingClientRect().top - scroller.getBoundingClientRect().top;
      const end = scroller.scrollHeight - scroller.clientHeight;
      return scroller.scrollTop > 0 && (Math.abs(gap) < 1 || scroller.scrollTop >= end - 1);
    });
    expect(placed).toBe(true);
  });

  test("Uninstall Pitboard? opens on Cancel, its default, and Uninstall quits until Reset", async ({
    page,
  }) => {
    const { fig } = await accounts(page, "windows");
    await fig.locator('[role="tab"][data-pane="settings"]').click();
    await fig.locator('[data-setting="uninstall"]').click();
    const dialog = page.locator("#win-accounts-uninstall");
    await expect(dialog).toBeVisible();
    const cancel = dialog.getByRole("button", { name: "Cancel" });
    await expect(cancel).toBeFocused();
    expect(await accent(cancel)).toBe(true);
    expect(await accent(dialog.getByRole("button", { name: "Uninstall" }))).toBe(false);
    await page.keyboard.press("Enter");
    await expect(dialog).toBeHidden();
    await expect(fig.locator('[data-part="window"]')).toBeVisible();
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
    // Remove is the default button, which WinUI draws in the accent
    expect(await accent(dialog.getByRole("button", { name: "Remove" }))).toBe(true);
    expect(await accent(dialog.getByRole("button", { name: "Cancel" }))).toBe(false);
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
    // the account is saved under that name already, so Done is the step's one button
    await expect(name.getByRole("button")).toHaveText(["Done"]);
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

// The Linux window as libadwaita draws it on either desktop: an account group's button and its
// menu, the main menu, the pages pushed over the accounts, and the dialogs (plan 3.3 "What each
// account has left").
async function window(page: Page, system: "gnome" | "kde", id = "usage-linux") {
  await choose(page.context(), { system });
  await page.goto("/");
  await handedOver(page);
  const fig = page.locator(`#${id}`);
  await fig.scrollIntoViewIfNeeded();
  await expect(fig).not.toHaveAttribute("inert");
  return {
    fig,
    group: (account: string) => fig.locator(`[data-part="list"] > [data-account="${account}"]`),
    menu: page.locator(`#${id}-account-menu`),
    main: page.locator(`#${id}-main-menu`),
    offers: page.locator(`#${id}-account-menu [role="menuitem"]:not([hidden])`),
    dialog: (name: string) => page.locator(`#linux-accounts-${name}`),
  };
}

for (const system of ["gnome", "kde"] as const)
  test.describe(`on Linux under ${system}`, () => {
    test.use({ reducedMotion: "reduce" });
    test.skip(({ isMobile }) => isMobile, "a pointer and a keyboard");

    test("each group says its tool and plan, In Use or its Switch, and reads its limits as sentences", async ({
      page,
    }) => {
      // the demo has switched to work
      const { group } = await window(page, system);
      await expect(group("work")).toHaveAccessibleName("work");
      await expect(group("work")).toContainText("Claude Code, Max 20x");
      await expect(group("work").locator('[data-part="in-use"]')).toHaveText("In Use");
      await expect(group("work").getByRole("button", { name: /^Switch/ })).toBeHidden();
      // named for the account with its tool, as two tools are listed
      const personal = group("personal").getByRole("button", {
        name: "Switch to personal (Claude Code)",
        exact: true,
      });
      await expect(personal).toHaveText("Switch");
      await expect(
        group("work").locator('[data-part="limit"] [data-part="spoken"]').first(),
      ).toHaveText(/^5-hour limit, 12 percent used, resets 23:50/);
      await expect(group("spare")).toContainText("Codex, Plus");
      // Codex's accounts keep no warm-up
      await expect(group("spare").locator('[data-part="warm"]')).toBeHidden();
    });

    test("an account's menu offers what its header's button does not, and Remove last", async ({
      page,
    }) => {
      const { group, menu, offers } = await window(page, system);
      const text = { useInnerText: true };
      const button = group("personal").getByRole("button", { name: "Account" });
      await button.click();
      await expect(menu).toBeVisible();
      await expect(button).toHaveAttribute("aria-expanded", "true");
      await expect(offers).toHaveText(
        ["Sign In Again…", "Rename…", "Warm Up Now", "Copy Email Address", "Remove Account…"],
        text,
      );
      await expect(menu.getByRole("menuitem", { name: "Warm Up Now" })).toHaveAttribute(
        "aria-disabled",
        "true",
      );
      await page.keyboard.press("Escape");
      // the account in use keeps its Switch and Remove, dimmed, as the app's menu does
      await group("work").getByRole("button", { name: "Account" }).click();
      await expect(offers).toHaveText(
        [
          "Switch to work",
          "Sign In Again…",
          "Rename…",
          "Warm Up Now",
          "Copy Email Address",
          "Remove Account…",
        ],
        text,
      );
      for (const name of ["Switch to work", "Remove Account…"])
        await expect(menu.getByRole("menuitem", { name })).toHaveAttribute("aria-disabled", "true");
      // a press outside puts it away
      await page.locator("#usage-title").click();
      await expect(menu).toBeHidden();
      await expect(button).toHaveAttribute("aria-expanded", "false");
    });

    test("from the keyboard an account's menu opens on its first item it can, and Escape gives focus back", async ({
      page,
      browserName,
    }) => {
      test.skip(
        process.platform === "darwin" && browserName === "webkit",
        "WebKit on a Mac tabs past buttons, as Safari does until Option-Tab",
      );
      const { group, menu, offers } = await window(page, system);
      await group("main").getByRole("button", { name: "Account" }).focus();
      await page.keyboard.press("Shift+Tab");
      await page.keyboard.press("Tab");
      await page.keyboard.press("Enter");
      await expect(offers).toHaveText(
        ["Switch to main", "Sign In Again…", "Rename…", "Copy Email Address", "Remove Account…"],
        { useInnerText: true },
      );
      // focus takes the first item GTK lets it, and wraps past the dimmed ones
      await expect(menu.getByRole("menuitem", { name: "Sign In Again…" })).toBeFocused();
      await page.keyboard.press("ArrowUp");
      await expect(menu.getByRole("menuitem", { name: "Copy Email Address" })).toBeFocused();
      await page.keyboard.press("Escape");
      await expect(menu).toBeHidden();
      await expect(group("main").getByRole("button", { name: "Account" })).toBeFocused();
    });

    test("Switch shows the spinner named Switching, then In Use and the notice", async ({
      page,
    }) => {
      const { fig, group } = await window(page, system);
      await group("personal").getByRole("button", { name: "Switch to personal" }).click();
      await expect(group("personal").getByRole("img", { name: "Switching" })).toBeVisible();
      // while it runs, no other header offers its button
      await expect(group("spare").locator('[data-part="action"]')).toBeHidden();
      await expect(group("personal").locator('[data-part="in-use"]')).toBeVisible();
      await expect(fig.locator("[data-notice]:not([hidden])")).toContainText(
        "Switched Claude Code to personal",
      );
    });

    test("a switch of Codex warns with Learn More, and once one account is left the tip asks for a second", async ({
      page,
    }) => {
      const { fig, group, menu, dialog } = await window(page, system);
      await group("spare").getByRole("button", { name: "Switch to spare" }).click();
      await expect(group("spare").locator('[data-part="in-use"]')).toBeVisible();
      const notice = fig.locator('[data-notice="codex"]');
      await expect(notice.locator('[data-part="title"]').first()).toHaveText(
        "Switched Codex to spare",
      );
      await expect(notice.getByRole("link", { name: "Learn More" })).toHaveAttribute(
        "href",
        "https://docs.usepitboard.com/reference/errors#warning-codes",
      );
      const tip = fig.locator('[data-part="setup"]');
      await expect(tip).toBeHidden();
      await group("main").getByRole("button", { name: "Account" }).click();
      await menu.getByRole("menuitem", { name: "Remove Account…" }).click();
      await expect(dialog("remove").getByRole("button", { name: "Remove" })).toBeFocused();
      await page.keyboard.press("Enter");
      await expect(group("main")).toBeHidden();
      await expect(tip).toBeVisible();
      await expect(tip.locator('[data-part="title"]')).toHaveText("Add a second Codex account");
      await expect(tip.locator('[data-part="detail"]')).toHaveText(
        "spare is your only Codex account. Add another to switch between them.",
      );
      expect(await axe(page, ["#usage-linux"])).toEqual([]);
      await tip.getByRole("button", { name: "Not Now" }).click();
      await expect(tip).toBeHidden();
    });

    test("F10 opens the main menu, its items cycle, and each opens what it names", async ({
      page,
    }) => {
      const { fig, main } = await window(page, system);
      const button = fig.getByRole("button", { name: "Main Menu" });
      await button.focus();
      await page.keyboard.press("F10");
      await expect(main).toBeVisible();
      const items = main.getByRole("menuitem");
      await expect(items).toHaveText([
        "Open Link…",
        "Diagnostics",
        "Activity",
        "Preferences",
        "Keyboard Shortcuts",
        "Help",
        "About Pitboard",
      ]);
      await expect(items.first()).toBeFocused();
      await page.keyboard.press("ArrowUp");
      await expect(items.last()).toBeFocused();
      await page.keyboard.press("Tab");
      await expect(items.first()).toBeFocused();
      // GtkPopoverMenu binds no Home or End
      await page.keyboard.press("End");
      await expect(items.first()).toBeFocused();
      await page.keyboard.press("ArrowDown");
      await page.keyboard.press("Enter");
      // Diagnostics, pushed over the accounts, its back button focused
      const back = fig.locator('[data-page="diagnostics"]').getByRole("button", { name: "Back" });
      await expect(back).toBeFocused();
      await expect(fig.locator('[data-page="diagnostics"] [role="status"]')).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(fig.locator('[data-page="accounts"]')).toBeVisible();
      await expect(button).toBeFocused();
      await page.keyboard.press("F10");
      await items.nth(2).click();
      await expect(fig.locator('[data-page="activity"]')).toBeVisible();
      await page.keyboard.press("Alt+ArrowLeft");
      await expect(fig.locator('[data-page="accounts"]')).toBeVisible();
    });

    test("Add Account picks its tool, signs in, then names the account with Done alone", async ({
      page,
    }) => {
      const { fig, dialog } = await window(page, system);
      await fig.getByRole("button", { name: "Add Account…" }).click();
      const signIn = dialog("sign-in");
      await expect(signIn).toBeVisible();
      await expect(signIn).toHaveAccessibleName("Add Account");
      const tools = signIn.getByRole("group", { name: "Tool" });
      await expect(tools.getByRole("button")).toHaveText(["Claude Code", "Codex"]);
      await tools.getByRole("button", { name: "Codex" }).click();
      await expect(tools.getByRole("button", { name: "Codex" })).toHaveAttribute(
        "aria-pressed",
        "true",
      );
      await signIn.getByRole("button", { name: "Sign In…" }).click();
      await expect(signIn.getByRole("img", { name: "Waiting for your browser" })).toBeVisible();
      const name = dialog("name");
      await expect(name).toBeVisible({ timeout: 6000 });
      await expect(name).toHaveAccessibleName("Name This Account");
      await expect(name.getByRole("button")).toHaveText(["Done"]);
      await expect(name.locator('input[name="name"]')).toHaveValue("travel");
      await page.keyboard.press("Enter");
      await expect(name).toBeHidden();
      await expect(fig.locator('[data-part="list"] > [data-tool="codex"]')).toHaveCount(3);
    });

    test("Rename refuses an empty name, Escape cancels, and Remove asks with Remove focused", async ({
      page,
    }) => {
      const { group, menu, dialog } = await window(page, system);
      await group("personal").getByRole("button", { name: "Account" }).click();
      await menu.getByRole("menuitem", { name: "Rename…" }).click();
      const name = dialog("name");
      await expect(name).toHaveAccessibleName("Rename “personal”");
      await expect(name.getByRole("button")).toHaveText(["Cancel", "Rename"]);
      await name.locator('input[name="name"]').fill("");
      await expect(name.locator('[data-part="problem"]')).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(name).toBeHidden();
      await expect(group("personal").locator('[data-part="group-title"]')).toHaveText("personal");
      await group("personal").getByRole("button", { name: "Account" }).click();
      await menu.getByRole("menuitem", { name: "Remove Account…" }).click();
      const remove = dialog("remove");
      await expect(remove).toBeVisible();
      await expect(remove).toHaveAccessibleName("Remove “personal (Claude Code)”?");
      await expect(remove.getByRole("button", { name: "Remove" })).toBeFocused();
      expect(await axe(page, ["#linux-accounts-remove"])).toEqual([]);
      await page.keyboard.press("Enter");
      await expect(remove).toBeHidden();
      await expect(group("personal")).toBeHidden();
      await expect(
        page.locator("#usage-linux").getByRole("button", { name: "Add Account…" }),
      ).toBeFocused();
    });

    test("Open Link, Keyboard Shortcuts and About open from the main menu and close with Escape", async ({
      page,
    }) => {
      const { fig, main, dialog } = await window(page, system);
      const button = fig.getByRole("button", { name: "Main Menu" });
      await button.click();
      await main.getByRole("menuitem", { name: "Open Link…" }).click();
      await expect(dialog("open-link")).toBeVisible();
      await expect(dialog("open-link")).toContainText("No Link to Open");
      // the page reads no clipboard: Paste Link opens nothing
      await dialog("open-link").getByRole("button", { name: "Paste Link" }).click();
      await expect(dialog("open-link")).toContainText("No Link to Open");
      await page.keyboard.press("Escape");
      await expect(dialog("open-link")).toBeHidden();
      await button.click();
      await main.getByRole("menuitem", { name: "Keyboard Shortcuts" }).click();
      await expect(dialog("shortcuts")).toBeVisible();
      await expect(dialog("shortcuts")).toContainText("Ctrl+N");
      expect(await axe(page, ["#linux-accounts-shortcuts"])).toEqual([]);
      await dialog("shortcuts").getByRole("button", { name: "Close" }).click();
      await expect(dialog("shortcuts")).toBeHidden();
      await button.click();
      await main.getByRole("menuitem", { name: "About Pitboard" }).click();
      await expect(dialog("about")).toBeVisible();
      await expect(dialog("about")).toContainText("Pitboard");
      await page.keyboard.press("Escape");
      await expect(dialog("about")).toBeHidden();
    });

    test("with its menus open and a dialog up, the window keeps WAI-ARIA's rules", async ({
      page,
    }) => {
      const { fig, group, main, dialog } = await window(page, system);
      await group("work").getByRole("button", { name: "Account" }).click();
      expect(await axe(page, ["#usage-linux"])).toEqual([]);
      await page.keyboard.press("Escape");
      await fig.getByRole("button", { name: "Main Menu" }).click();
      await expect(main).toBeVisible();
      expect(await axe(page, ["#usage-linux"])).toEqual([]);
      await page.keyboard.press("Escape");
      await fig.getByRole("button", { name: "Add Account…" }).click();
      await expect(dialog("sign-in")).toBeVisible();
      expect(await axe(page, ["#linux-accounts-sign-in"])).toEqual([]);
    });
  });

test.describe("on Linux, the window's controls by desktop", () => {
  test.use({ reducedMotion: "reduce" });
  test.skip(({ isMobile }) => isMobile, "a pointer and a keyboard");

  test("KDE Plasma's layout adds Minimize and Maximize, and its tray keeps Pitboard past Close", async ({
    page,
  }) => {
    const { fig } = await window(page, "kde");
    await expect(fig.getByRole("button", { name: "Minimize" })).toBeVisible();
    const before = (await fig.boundingBox())!;
    await fig.getByRole("button", { name: "Maximize" }).click();
    expect((await fig.boundingBox())!.width).toBe(before.width);
    await fig.getByRole("button", { name: "Minimize" }).click();
    await expect(fig).toBeHidden();
    await page.locator("#hero-kde-icon").click({ button: "right" });
    await page.locator('#hero-kde-menu [data-moment="after"] [data-does="open"]').click();
    await expect(fig).toBeVisible();
    await fig.getByRole("button", { name: "Close" }).click();
    await expect(fig).toBeHidden();
    // Pitboard runs on in the tray
    await expect(page.locator("#hero-kde-icon")).toBeVisible();
    await page.locator("#hero-kde-icon").click();
    await expect(fig).toBeVisible();
  });

  test("on GNOME, Close quits Pitboard and every window goes until Reset", async ({ page }) => {
    const { fig } = await window(page, "gnome");
    await expect(fig.getByRole("button", { name: "Minimize" })).toBeHidden();
    await fig.getByRole("button", { name: "Close" }).click();
    await expect(fig).toBeHidden();
    await expect(page.locator("#switch-linux")).toBeHidden();
    await page.locator("figure:has(#usage-linux) [data-reset]").click();
    await expect(fig).toBeVisible();
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
    const box = (await row("work").boundingBox())!;
    const cdp = await page.context().newCDPSession(page);
    const point = { x: box.x + box.width / 2, y: box.y + 10 };
    await cdp.send("Input.synthesizeTapGesture", { ...point, duration: 800 });
    await expect(menu).toBeVisible();
    await expect(row("work")).toHaveAttribute("aria-selected", "false");
    await cdp.send("Input.synthesizeTapGesture", { x: 5, y: point.y });
    await expect(menu).toBeHidden();
    // Linux's headless shell makes no click of a tap CDP synthesizes
    await page.touchscreen.tap(point.x, point.y);
    await expect(row("work")).toHaveAttribute("aria-selected", "true");
  });

  test("a touch held still opens it where iOS sends no contextmenu, and the click after is swallowed", async ({
    page,
    browserName,
  }) => {
    test.skip(browserName !== "webkit", "iOS Safari's touch");
    const { row, menu } = await accounts(page, "macos");
    await row("work").evaluate(async (el) => {
      const r = el.getBoundingClientRect();
      const at = { clientX: r.left + 40, clientY: r.top + 10, bubbles: true, pointerType: "touch" };
      el.dispatchEvent(new PointerEvent("pointerdown", at));
      await new Promise((done) => setTimeout(done, 650));
      el.dispatchEvent(new PointerEvent("pointerup", at));
      el.dispatchEvent(new MouseEvent("click", { ...at, detail: 1 }));
    });
    await expect(menu).toBeVisible();
    await expect(row("work")).toHaveAttribute("aria-selected", "false");
  });
});
