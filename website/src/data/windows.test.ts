import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  accountsPage,
  activityPage,
  app,
  iconMenu,
  panes,
  settingsPage,
  toast,
} from "./windows.ts";
import { tools } from "./world.ts";
import { trayTip } from "../lib/words.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const views = (file: string) => repo(`apps/windows/Pitboard.Views/${file}`);
const shell = (file: string) => repo(`apps/windows/Pitboard.Windows/${file}`);
const present = (file: string) => repo(`crates/pitboard-ffi/src/present/${file}`);
const core = (file: string) => repo(`crates/pitboard-core/src/${file}`);
// A Rust string literal as it reads once its line continuations are joined.
const joined = (source: string) => source.replace(/\\\n\s*/g, "");

const headings = new Set(tools.map(([, name]) => name));
const menu = (updates: boolean) => iconMenu("after", updates).flat();

test("the icon's menu says what IconMenu.cs gives it, in its order", () => {
  const source = shell("IconMenu.cs");
  const commands = menu(true).flatMap((e) => ("command" in e && !e.submenu ? [e.command] : []));
  assert.deepEqual(commands, [
    "Add account…",
    "Refresh",
    "Open Pitboard",
    "Settings",
    "Check for updates",
    "Pitboard help",
    "Exit",
  ]);
  for (const c of commands) assert.ok(source.includes(`new IconMenuEntry.Command("${c}"`), c);
  assert.ok(
    menu(true).some((e) => "command" in e && e.command === "Open Pitboard" && e.bold),
    "the click's default is bold",
  );
  assert.ok(
    source.includes(
      '"Open Pitboard", new IconCommand.Send(new Intent.ShowWindow(Pane: null))) { Default = true }',
    ),
  );
});

test("a beta's menu has no Check for updates, as IconMenu.cs offers it only beside a release", () => {
  assert.ok(shell("IconMenu.cs").includes("updates.Available && !updates.Beta"));
  assert.ok(!menu(false).some((e) => "command" in e && e.command === "Check for updates"));
});

test("the menu heads each tool's accounts with its name, and names each account with its plan and the menu bar's figure", () => {
  const words = present("words.rs");
  assert.ok(words.includes("said.extend(plan);"));
  assert.ok(words.includes("if matches!(os, Os::Windows) {\n        said.extend(figure);"));
  assert.ok(present("accounts.rs").includes(".map(|limit| said::figure(limit.percent));"));
  for (const e of menu(false)) {
    if ("heading" in e) assert.ok(headings.has(e.heading), e.heading);
    if ("account" in e) assert.match(e.account, /^\w+ · (Max 20x|Pro|Plus) · \d+%$/);
  }
  assert.ok(!menu(false).some((e) => "heading" in e && e.heading === "Claude Desktop"));
  const checked = menu(false).filter((e) => "account" in e && e.checked);
  assert.deepEqual(
    checked.map((e) => ("account" in e ? e.account : "")),
    ["work · Max 20x · 40%", "main · Pro · 18%"],
  );
});

test("an account's tooltip in the icon's menu is the line the menu bar app shows under it", () => {
  assert.ok(shell("IconMenu.cs").includes("Lines(item.Summary, item.Help)"));
  assert.ok(views("IconMenuFlyout.cs").includes("ToolTipService.SetToolTip(item, detail);"));
  assert.ok(
    present("accounts.rs").includes(
      "in_place_of_limits(seen, account).unwrap_or_else(|| limits_said(seen, account))",
    ),
  );
  const [work] = iconMenu("before", false)
    .flat()
    .filter((e) => "account" in e && e.label === "work");
  assert.ok(work && "tip" in work);
  assert.equal(work.tip, "5-hour 12%, weekly 40%");
});

test("a site with several accounts' windows is a submenu of their labels", () => {
  const source = repo("crates/pitboard-ffi/src/account_windows/accounts.rs");
  assert.ok(source.includes('title: format!("Open {}", site.name()),'));
  const sites = menu(false).flatMap((e) => ("command" in e && e.submenu ? [e] : []));
  assert.deepEqual(
    sites.map((e) => ("command" in e ? [e.command, e.submenu] : [])),
    [
      ["Open claude.ai", ["personal", "work"]],
      ["Open chatgpt.com", ["main", "spare"]],
    ],
  );
  const named = repo("crates/pitboard-sites/src/site.rs");
  for (const host of ["claude.ai", "chatgpt.com"]) assert.ok(named.includes(`host: "${host}"`));
});

test("the icon's tooltip names Pitboard, then each account in use with its tool and its menu line", () => {
  const source = present("accounts.rs");
  assert.ok(source.includes('let mut tip = vec!["Pitboard".to_owned()];'));
  assert.ok(source.includes('format!("{}: {}", seen.name_of(account), summary(seen, account))'));
  assert.ok(present("words.rs").includes('format!("{label} ({tool})")'));
  assert.equal(
    trayTip("after"),
    "Pitboard\nwork (Claude Code): 5-hour 12%, weekly 40%\nmain (Codex): 5-hour 18%, weekly 7%",
  );
});

test("the window and its panes are named as MainWindow.xaml names them", () => {
  const source = views("MainWindow.xaml");
  assert.equal(app, "Pitboard");
  assert.ok(source.includes(`Title="${app}"`));
  for (const [, name] of panes) assert.ok(source.includes(`Content="${name}"`), name);
});

test("the Accounts page says what AccountsPage.xaml, AccountRow.xaml and the core say", () => {
  const page = accountsPage("after");
  const xaml = views("AccountsPage.xaml");
  assert.ok(xaml.includes(`Text="${page.title}"`));
  for (const c of page.commands) assert.ok(xaml.includes(`Label="${c}"`), c);
  assert.ok(present("words.rs").includes('format!("Updated {clock}")'));
  assert.match(page.updated, /^Updated \d\d:\d\d$/);
  assert.ok(present("words.rs").includes('titled(os, "In Use", "In use")'));
  assert.ok(present("accounts.rs").includes('title: "Use".into()'));
  assert.deepEqual(
    page.sections.map((s) => s.name),
    ["Claude Code", "Codex"],
  );
  const rows = page.sections.flatMap((s) => s.rows);
  assert.ok(rows.every((r) => r.inUse !== (r.action === "Use")));
});

test("the notice after a switch says what notices.rs and NoticeBar.xaml.cs put together", () => {
  const words = present("words.rs");
  assert.ok(words.includes('Some(tool) => format!("Switched {tool} to {label}")'));
  assert.ok(words.includes('Some(tool) => format!("{tool} sessions already open follow in")'));
  assert.ok(
    views("NoticeBar.xaml.cs").includes(
      'CountdownText.Text = $"{label} {Countdown.Left(until, now)}";',
    ),
  );
  const { notice } = accountsPage("after");
  assert.equal(notice?.title, "Switched Claude Code to work");
  assert.equal(notice?.countdown, "Claude Code sessions already open follow in 0:21");
  assert.equal(accountsPage("before").notice, null);
});

test("an account's notes are the core's, with when it warms up last", () => {
  const said = core("words.rs");
  assert.ok(
    said.includes('Some(left) => format!("{limit} limit runs out in {left} at this pace"),'),
  );
  assert.ok(said.includes('days => format!("Parked login good for {days} more days"),'));
  assert.ok(said.includes('format!("warms up again at {}", moment(*next))'));
  assert.ok(present("accounts.rs").includes(".chain(warm_note(seen, account))"));
  assert.ok(views("AccountRow.xaml.cs").includes("foreach (var note in item.Notes)"));
  const notes = (moment: "before" | "after") =>
    accountsPage(moment).sections.flatMap((s) => s.rows.flatMap((r) => r.notes));
  assert.deepEqual(notes("before"), [
    "Weekly limit runs out in 11h 05m at this pace",
    "Warms up again at 22:00",
    "Parked login good for 26 more days",
  ]);
  assert.deepEqual(notes("after"), [
    "Parked login good for 26 more days",
    "Warms up again at 22:00",
  ]);
});

test("Settings says what SettingsPage.xaml, SettingsViewModel.cs and machine.rs say", () => {
  const s = settingsPage;
  const xaml = views("SettingsPage.xaml");
  assert.ok(xaml.includes(`Text="${s.title}"`));
  assert.ok(xaml.includes(`Text="${s.section}"`));
  assert.ok(
    xaml.includes(
      '<toolkit:SettingsExpander Header="{x:Bind ViewModel.AtLimitHeading, Mode=OneWay}"',
    ),
  );
  const machine = joined(present("machine.rs"));
  assert.ok(machine.includes(`const AT_LIMIT: &str = "${s.atLimit.header}";`));
  assert.ok(machine.includes(`"${s.atLimit.choice}",`));
  assert.ok(machine.includes(`share_label: "${s.share.header}".into(),`));
  assert.ok(present("words.rs").includes('format!("From {lowest}% to {highest}%")'));
  assert.ok(shell("SettingsViewModel.cs").includes("ShareRange => AtLimit.ShareRange;"));
  const autoswitch = core("duty/autoswitch.rs");
  assert.ok(autoswitch.includes("pub const LOWEST: u8 = 50;"));
  assert.ok(autoswitch.includes("pub const HIGHEST: u8 = 99;"));
  assert.equal(s.share.range, "From 50% to 99%");
  assert.ok(core("words.rs").includes('let mut said = format!("Watching {account}");'));
  assert.match(
    s.atLimit.standing,
    /^Watching \w+: \d+% of its (5-hour|weekly) limit, as of \d\d:\d\d\.$/,
  );
  const note = `Pitboard switches Claude Code once a limit of the account in use reaches ${s.share.value}%, never back by itself, and notifies you of Codex. Sessions already running follow within about 33 seconds.`;
  assert.ok(machine.includes('"Sessions already running follow within about {} seconds."'));
  assert.ok(
    machine.includes(
      "Pitboard switches Claude Code once a limit of the account in use reaches {at}%, never back by itself, and notifies you of Codex. {follows}",
    ),
  );
  assert.ok(core("switch/mod.rs").includes("pub const ADOPTION_CEILING_SECONDS: u32 = 33;"));
  assert.equal(s.atLimit.note, note);
  // The apps curl the core's apostrophe, as words.rs's typeset does.
  assert.ok(core("words.rs").includes(`"${s.atLimit.terms.slice(0, -1).replace("’", "'")}";`));
  assert.ok(
    machine.includes("sentence: words::sentence(pitboard_core::words::TERMS_MAY_COVER_THIS),"),
  );
  assert.ok(machine.includes(`link_title: "${s.atLimit.link}".into(),`));
});

test("Warm up limits says what SettingsPage.xaml, WarmAccountRow.cs and machine.rs say", () => {
  const w = settingsPage.warm;
  const xaml = views("SettingsPage.xaml");
  assert.ok(xaml.includes('Header="{x:Bind ViewModel.WarmTitle, Mode=OneWay}"'));
  assert.ok(present("machine.rs").includes(`title: "${w.header}".into(),`));
  for (const [name] of w.hours) {
    assert.ok(xaml.includes(`<toolkit:SettingsCard Header="${name}"`), name);
  }
  assert.ok(present("words.rs").includes('Some(format!("{} to {to}", time_of_day(from)?))'));
  assert.equal(w.range, "07:00 to 23:00");
  assert.ok(
    present("machine.rs").includes('format!("Next warm-up: {account} at {}.", seen.clock(at))'),
  );
  assert.equal(w.standing, "Next warm-up: personal at 22:00.");
  assert.ok(present("machine.rs").includes("title: words::named_in_menu("));
  assert.ok(shell("WarmAccountRow.cs").includes("public string Title => _account.Title;"));
  assert.deepEqual(w.accounts, [
    { title: "personal · Max 20x", standing: "Warms up again at 22:00", on: true },
    { title: "work · Max 20x", standing: null, on: false },
  ]);
});

test("Activity lists Windows' changes in the app's words, dated as the regional format's g", () => {
  const page = activityPage;
  const xaml = views("ActivityPage.xaml");
  assert.ok(xaml.includes(`Text="${page.title}"`));
  assert.ok(xaml.includes(`Label="${page.command}"`));
  assert.ok(shell("WindowsLocalTime.cs").includes('At(epoch).ToString("g", Culture)'));
  const words = present("words.rs");
  assert.ok(words.includes('"use" | "switch" => "Switch".into(),'));
  assert.ok(words.includes('"enroll" => "Enrol".into(),'));
  assert.ok(words.includes('"warm-up" => "Warm-up".into(),'));
  assert.ok(words.includes('"ok" => "Done".into(),'));
  assert.ok(words.includes('"app" => "Pitboard app".into(),'));
  assert.ok(words.includes('"cli" => "Command line".into(),'));
  assert.deepEqual(
    page.rows.map((r) => [r.date, r.change, r.account, r.result, r.by]),
    [
      ["13/10/2026 20:49", "Switch", "work", "Done", "Pitboard app"],
      ["13/10/2026 17:00", "Warm-up", "personal", "Done", "Pitboard app"],
      ["10/10/2026 09:16", "Enrol", "work", "Done", "Command line"],
    ],
  );
});

test("the run-out notification says what WindowsNotifications.cs builds from notices.rs", () => {
  const source = shell("WindowsNotifications.cs");
  assert.ok(source.includes('public const string DisplayName = "Pitboard";'));
  assert.ok(source.includes("new AppNotificationButton(button.Title)"));
  assert.ok(present("words.rs").includes('pub(crate) const SWITCH: &str = "Switch";'));
  assert.ok(present("notices.rs").includes("button(words::SWITCH.into(),"));
  const words = present("words.rs");
  assert.ok(words.includes('format!("{account} has no {limit} limit left")'));
  assert.ok(words.includes('Some(left) => format!("{account} has {left}% of its own left."),'));
  assert.deepEqual(toast, {
    app: "Pitboard",
    title: "personal has no weekly limit left",
    tool: "Claude Code",
    body: "work has 60% of its own left.",
    button: "Switch",
  });
});
