// The terminals (plan section 5): a prompt that runs pitboard against the one state, completes
// with Tab as each shell's shipped script does, recalls lines with Up and Down, answers Ctrl+C
// and the shell's own keys, and lets a keyboard leave.
import { choose, expect, test, type Platform } from "./fixtures.ts";
import type { Locator, Page } from "@playwright/test";

test.use({ reducedMotion: "reduce" });

// #switch's terminal on `platform`: macOS's zsh, Windows PowerShell, or, on Linux, #usage's bash
const ids: Record<Platform, string> = {
  macos: "switch-cli-macos",
  windows: "switch-cli-windows",
  linux: "usage-cli-linux",
};
const prompts: Record<Platform, string> = { macos: "%", windows: "PS C:\\Users\\me>", linux: "$" };

async function open(
  page: Page,
  platform: Platform,
): Promise<{ term: Locator; field: Locator; log: Locator }> {
  await choose(page.context(), { platform, surface: "cli" });
  await page.clock.install({ time: new Date("2026-10-13T13:51:00Z") });
  await page.goto("/");
  const term = page.locator(`#${ids[platform]}`);
  await expect(term).not.toHaveAttribute("inert");
  const field = term.locator("[data-line]");
  await field.focus();
  return { term, field, log: term.locator("[role=log]:visible") };
}

// what the window shows, its last `n` lines
const last = async (log: Locator, n: number) =>
  (await log.locator(":scope > div:not([data-below])").allInnerTexts())
    .map((l) => l.replace(/\u00a0/g, " ").trimEnd())
    .slice(-n);

for (const platform of ["macos", "windows", "linux"] as const) {
  test(`on ${platform}, pitboard runs against the one state and the prompt comes back`, async ({
    page,
  }) => {
    // the hero's switch has put work in use
    const { field, log } = await open(page, platform);
    await field.fill("pitboard use personal");
    await field.press("Enter");
    await expect
      .poll(() => last(log, 4))
      .toEqual([
        `${prompts[platform]} pitboard use personal`,
        "Switched to personal; work is parked.",
        "Claude Code sessions already running follow within 33 seconds.",
        prompts[platform],
      ]);
    await expect(field).toHaveValue("");
    await expect(field).toBeFocused();
    await field.fill("pitboard stat");
    await field.press("Enter");
    await expect
      .poll(() => last(log, 8))
      .toEqual([
        "error: unrecognized subcommand 'stat'",
        "",
        "  tip: some similar subcommands exist: 'statusline', 'status'",
        "",
        "Usage: pitboard [OPTIONS] [COMMAND]",
        "",
        "For more information, try '--help'.",
        prompts[platform],
      ]);
    // Up recalls the lines before, Down comes back to the line being typed
    await field.fill("draft");
    await field.press("ArrowUp");
    await expect(field).toHaveValue("pitboard stat");
    await field.press("ArrowUp");
    await expect(field).toHaveValue("pitboard use personal");
    await field.press("ArrowDown");
    await field.press("ArrowDown");
    await expect(field).toHaveValue("draft");
  });
}

test("zsh lists with the first Tab where nothing goes in, then puts each match in", async ({
  page,
}) => {
  const { field, log } = await open(page, "macos");
  await field.fill("pitboard u");
  await field.press("Tab");
  const below = log.locator("[data-below]");
  await expect(below).toContainText("uninstall  -- Delete every parked login");
  await expect(below).toContainText("use        -- Switch a tool to an enrolled account");
  await field.press("Tab");
  await expect(field).toHaveValue("pitboard uninstall ");
  await field.press("Tab");
  await expect(field).toHaveValue("pitboard use ");
  await expect(field).toBeFocused();
  await field.press("Enter");
  await expect(below).toHaveCount(0);
});

test("bash puts in what the matches share, then lists them on the Tab after the one that rang", async ({
  page,
}) => {
  const { field, log } = await open(page, "linux");
  await field.fill("pitboard use ");
  await field.press("Tab");
  await expect(field).toHaveValue("pitboard use -");
  await field.press("Tab");
  await field.press("Tab");
  await expect
    .poll(() => last(log, 3))
    .toEqual(["$ pitboard use -", "-y      -h      --yes   --json  --help", "$"]);
});

test("PowerShell puts in each offer in turn, and its Escape clears the line", async ({ page }) => {
  const { field } = await open(page, "windows");
  await field.fill("pitboard u");
  await field.press("Tab");
  await expect(field).toHaveValue("pitboard uninstall");
  await field.press("Tab");
  await expect(field).toHaveValue("pitboard use");
  await field.press("Escape");
  await expect(field).toHaveValue("");
  await expect(field).toBeFocused();
});

test("Ctrl+C leaves the line as each shell does, and PowerShell's ^C is bright red", async ({
  page,
}) => {
  for (const [platform, after] of [
    ["macos", "% pitboard u"],
    ["linux", "$ pitboard u^C"],
    ["windows", "PS C:\\Users\\me> pitboard u^C"],
  ] as const) {
    const { field, log, term } = await open(page, platform);
    await field.fill("pitboard u");
    await field.press("Control+c");
    await expect.poll(() => last(log, 2)).toEqual([after, prompts[platform]]);
    await expect(field).toHaveValue("");
    if (platform === "windows")
      await expect(term.locator(".br").last()).toHaveCSS("color", "rgb(231, 72, 86)");
  }
});

test("each shell's own keys edit the line: zsh's and bash's Ctrl+U and Ctrl+W differ", async ({
  page,
}) => {
  const { field } = await open(page, "macos");
  await field.fill("pitboard use codex/main");
  await field.press("Control+w");
  await expect(field).toHaveValue("pitboard use ");
  await field.fill("pitboard use work");
  await field.evaluate((f: HTMLInputElement) => f.setSelectionRange(9, 9));
  await field.press("Control+u");
  await expect(field).toHaveValue("");
  const bash = await open(page, "linux");
  await bash.field.fill("pitboard use work");
  await bash.field.evaluate((f: HTMLInputElement) => f.setSelectionRange(9, 9));
  await bash.field.press("Control+u");
  await expect(bash.field).toHaveValue("use work");
  await bash.field.press("Control+y");
  await expect(bash.field).toHaveValue("pitboard use work");
});

test("forget asks on the prompt line and reads y, Y or yes", async ({ page }) => {
  const { field, log } = await open(page, "macos");
  await field.fill("pitboard forget personal");
  await field.press("Enter");
  const line = log.locator("[data-prompt]");
  await expect(line).toContainText("[y/N]");
  await field.fill("n");
  await field.press("Enter");
  await expect.poll(() => last(log, 1)).toEqual(["%"]);
  await field.fill("pitboard forget personal");
  await field.press("Enter");
  await field.fill("y");
  await field.press("Enter");
  await expect.poll(() => last(log, 2)).toEqual(["Forgot personal (me@example.com).", "%"]);
});

test("pitboard watch says what it decides as time passes, until Ctrl+C", async ({ page }) => {
  const { field, log } = await open(page, "linux");
  await field.fill("pitboard use personal");
  await field.press("Enter");
  await field.fill("pitboard watch --at 70");
  await field.press("Enter");
  await expect(log).toContainText("Stop with Ctrl-C.");
  await page.clock.runFor(31_000);
  await expect(log).toContainText("the account was put in use less than 5 minutes ago");
  await page.clock.runFor(300_000);
  await expect(log).toContainText(
    "Switched Claude Code from personal to work: personal has used 73% of its weekly limit.",
  );
  await field.press("Control+c");
  await expect.poll(() => last(log, 2)).toEqual(["^C", "$"]);
});

test("Shift+Tab, and Escape then Tab, leave the prompt; a keyboard moves past every terminal", async ({
  page,
}) => {
  const { field } = await open(page, "macos");
  await field.press("Shift+Tab");
  await expect(field).not.toBeFocused();
  await field.focus();
  await field.press("Escape");
  await field.press("Tab");
  await expect(field).not.toBeFocused();
  const terms = page.locator("[data-line]:visible");
  for (const each of await terms.all()) {
    await each.focus();
    await page.keyboard.press("Escape");
    await page.keyboard.press("Tab");
    await expect(each).not.toBeFocused();
  }
  // the way out is said to a screen reader (question 18)
  await expect(field).toHaveAccessibleDescription(
    "Tab completes a command. Escape, then Tab, moves on.",
  );
  await expect(field).toHaveAccessibleName("pitboard use work");
});

test("Reset puts every terminal back to the session it draws", async ({ page }) => {
  const { field, log, term } = await open(page, "macos");
  const drawn = await last(log, 10);
  await field.fill("pitboard use personal");
  await field.press("Enter");
  const reset = term.locator("xpath=ancestor::figure").locator("[data-reset]");
  await expect(reset).toBeVisible();
  await reset.click();
  await expect.poll(() => last(log, 10)).toEqual(drawn);
  await field.press("ArrowUp");
  await expect(field).toHaveValue("pitboard use work");
});

test("on a phone, a tap on a command runs it, and focus stays on the button", async ({
  page,
  isMobile,
}) => {
  test.skip(!isMobile, "the row shows where the pointer is coarse");
  await choose(page.context(), { platform: "macos", surface: "cli" });
  await page.goto("/");
  const term = page.locator("#switch-cli-macos");
  await expect(term).not.toHaveAttribute("inert");
  const row = term.getByRole("toolbar", { name: "Commands" });
  await expect(row.getByRole("button")).toHaveText([
    "pitboard",
    "pitboard use personal",
    "pitboard log -n 4",
    "pitboard warm",
    "pitboard watch install",
    "pitboard doctor",
    "pitboard --help",
    "clear",
  ]);
  // the switch it makes turns its words round, so it is found by its place
  const use = row.getByRole("button").nth(1);
  await use.tap();
  await expect(use).toBeFocused();
  await expect(term.locator("[role=log]:visible")).toContainText(
    "Switched to personal; work is parked.",
  );
  await expect(use).toHaveText("pitboard use work");
  // the field takes no tap, which iOS would zoom the page for
  await expect(term.locator("[data-line]")).toHaveCSS("pointer-events", "none");
  // the toolbar keeps one Tab stop, and its arrows move along it
  await use.press("ArrowRight");
  await expect(row.getByRole("button", { name: "pitboard log -n 4" })).toBeFocused();
  await page.keyboard.press("End");
  await expect(row.getByRole("button", { name: "clear" })).toBeFocused();
});

test("where the pointer is fine, the terminal draws no row of commands", async ({
  page,
  isMobile,
}) => {
  test.skip(isMobile, "a phone shows it");
  await choose(page.context(), { platform: "linux", surface: "cli" });
  await page.goto("/");
  await expect(page.locator("#usage-cli-linux [role=toolbar]")).toBeHidden();
});
