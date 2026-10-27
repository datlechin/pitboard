import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  TROUBLESHOOT,
  accountsPage,
  activityPage,
  app,
  detail,
  diagnostics,
  iconMenu,
  joinsTheGroupBefore,
  panes,
  settingsPage,
  subpages,
  switchNotice,
  toast,
} from "./windows.ts";
import { moments, tools } from "./world.ts";
import { TIP_LENGTH, menuBarLevel, shellTip, trayTip } from "../lib/words.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const views = (file: string) => repo(`apps/windows/Pitboard.Views/${file}`);
const shell = (file: string) => repo(`apps/windows/Pitboard.Windows/${file}`);
const present = (file: string) => repo(`crates/pitboard-ffi/src/present/${file}`);
const core = (file: string) => repo(`crates/pitboard-core/src/${file}`);
// A Rust string literal as it reads once its line continuations are joined.
const joined = (source: string) => source.replace(/\\\n\s*/g, "");
// app_words.rs and words.rs, where every word the Windows views draw is the model's: each holds
// the words as a literal, and a view binds the field of app_words() that gives it.
const said = joined(present("app_words.rs") + present("words.rs"));
const owned = (words: string) => said.includes(`"${words}"`);

const headings = new Set(tools.map(([, name]) => name));
const menu = (updates: boolean) => iconMenu(moments.after, updates).flat();

test("the icon's menu says what IconMenu.cs gives it, in Microsoft's order, Open Pitboard first and Exit last", () => {
  const source = shell("IconMenu.cs");
  const commands = menu(true).flatMap((e) => ("command" in e && !e.submenu ? [e.command] : []));
  assert.deepEqual(commands, [
    "Open Pitboard",
    "Add account…",
    "Settings",
    "Check for updates",
    "Pitboard help",
    "Exit",
  ]);
  for (const c of commands) assert.ok(owned(c), c);
  // in the order IconMenu.cs's groups take app_words()' menu words, Check for updates among
  // Updates' and the accounts and sites between
  const at = (part: string) => source.indexOf(part);
  const order = [
    "words.OpenPitboard",
    "Accounts(model.Sections),",
    "words.AddAccount",
    "Sites(model.AccountWindows),",
    "words.Settings",
    ".. Updates(updates),",
    "words.Help",
    "words.Quit",
  ];
  for (const part of order) assert.ok(at(part) > 0, part);
  assert.deepEqual(
    order.map(at),
    order.map(at).toSorted((a, b) => a - b),
  );
  assert.ok(source.includes("Words.Menu.CheckForUpdates"));
  assert.ok(
    menu(true).some((e) => "command" in e && e.command === "Open Pitboard" && e.bold),
    "the click's default is bold",
  );
  assert.match(
    source,
    /words\.OpenPitboard, new IconCommand\.Send\(new Intent\.Show\(Place: null\)\)\)\s*\{\s*Default = true,/,
  );
});

test("a beta's menu has no Check for updates, as IconMenu.cs offers it only beside a release", () => {
  assert.ok(shell("IconMenu.cs").includes("return updates.Beta"));
  assert.ok(!menu(false).some((e) => "command" in e && e.command === "Check for updates"));
});

test("the menu heads each tool's accounts with its name, one group of them, and names each account with how much it has used", () => {
  const words = present("words.rs");
  assert.ok(
    words.includes('(Os::Windows, Some(aside)) => format!("{title} ({})", uncapitalised(aside)),'),
  );
  assert.ok(
    present("accounts.rs").includes(
      "let menu_title = words::named_in_menu(seen.os, &title, why_not.or(figure.as_deref()));",
    ),
  );
  assert.ok(present("accounts.rs").includes(".map(|limit| said::figure(limit.percent));"));
  for (const e of menu(false)) {
    if ("heading" in e) assert.ok(headings.has(e.heading), e.heading);
    if ("account" in e) assert.match(e.account, /^\w+ \(\d+%\)$/);
  }
  assert.ok(!menu(false).some((e) => "heading" in e && e.heading === "Claude Desktop"));
  const checked = menu(false).filter((e) => "account" in e && e.checked);
  assert.deepEqual(
    checked.map((e) => ("account" in e ? e.account : "")),
    ["work (40%)", "main (7%)"],
  );
  // each tool's accounts follow the one before with no separator, as IconMenu.cs makes them one
  // group
  const groups = iconMenu(moments.after, false);
  const tools = groups.filter((g) => "heading" in g[0]);
  assert.equal(tools.length, 2);
  assert.ok(joinsTheGroupBefore(tools[1], tools[0]));
  assert.ok(!joinsTheGroupBefore(tools[0], groups[0]));
});

test("an account's tooltip in the icon's menu is the line the menu bar app shows under it", () => {
  assert.ok(shell("IconMenu.cs").includes("Lines(item.Summary, item.Help)"));
  assert.ok(views("IconMenuFlyout.cs").includes("ToolTipService.SetToolTip(radio, detail);"));
  assert.ok(
    present("accounts.rs").includes(
      "in_place_of_limits(seen, account).unwrap_or_else(|| limits_said(seen, account))",
    ),
  );
  const [work] = iconMenu(moments.before, false)
    .flat()
    .filter((e) => "account" in e && e.label === "work");
  assert.ok(work && "tip" in work);
  // in sentence case, as the Windows app says each limit after the first
  assert.ok(
    present("words.rs").includes(
      '(_, Os::Windows) => format!("{} {figure}", uncapitalised(name)),',
    ),
  );
  assert.equal(work.tip, "5-hour 12% used, weekly 40%");
});

test("a site with several accounts' windows is a submenu of their labels, the one in use first", () => {
  const source = repo("crates/pitboard-ffi/src/account_windows/accounts.rs");
  assert.ok(source.includes("title: words::open_site(site.name()),"));
  assert.ok(present("words.rs").includes('format!("Open {site}")'));
  // in the accounts' order, which the status gives with the signed-in account first
  assert.ok(source.includes("let windows = window_accounts(accounts);"));
  assert.ok(core("status.rs").includes("rows.sort_by_key(|r| (rank(r.provider), !r.signed_in));"));
  const sites = menu(false).flatMap((e) => ("command" in e && e.submenu ? [e] : []));
  assert.deepEqual(
    sites.map((e) => ("command" in e ? [e.command, e.submenu] : [])),
    [
      ["Open claude.ai", ["work", "personal"]],
      ["Open chatgpt.com", ["main", "spare"]],
    ],
  );
  const named = repo("crates/pitboard-sites/src/site.rs");
  for (const host of ["claude.ai", "chatgpt.com"]) assert.ok(named.includes(`host: "${host}"`));
});

test("the icon's tooltip names Pitboard, then each account in use with its tool and its menu line", () => {
  const words = present("words.rs");
  assert.ok(words.includes("let mut tip = vec![PITBOARD.to_owned()];"));
  assert.ok(words.includes('.map(|(name, line)| format!("{name}: {}", uncapitalised(&line))),'));
  assert.ok(
    present("accounts.rs").includes(
      ".map(|account| (seen.name_of(account), summary(seen, account))),",
    ),
  );
  assert.ok(words.includes('format!("{label} ({tool})")'));
  assert.equal(
    trayTip(moments.after),
    "Pitboard\nwork (Claude Code): 5-hour 12% used, weekly 40%\nmain (Codex): weekly 7% used",
  );
});

test("the Shell holds 127 UTF-16 units of the icon's tooltip, cut never inside a surrogate pair", () => {
  const icon = shell("NotifyIcon.cs");
  assert.ok(icon.includes(`public const int TipLength = ${TIP_LENGTH};`));
  assert.ok(icon.includes("char.IsHighSurrogate(tip[TipLength - 1]) ? TipLength - 1 : TipLength"));
  const tip = trayTip(moments.after);
  assert.equal(shellTip(tip), tip);
  assert.equal(shellTip("a".repeat(200)).length, 127);
  // a name of 126 letters and a face: the face's two units would end past the 127th
  assert.equal(shellTip(`${"a".repeat(126)}😀`), "a".repeat(126));
});

test("the icon in the notification area draws the menu bar's level, from Tray's drawings in the taskbar's ink", () => {
  assert.ok(
    shell("Shell.cs").includes(
      '"Tray", place.LightTaskbar ? "Light" : "Dark", $"{look.Level?.ToString() ?? "None"}.ico"',
    ),
  );
  const make = repo("apps/windows/Icon/make-ico.py");
  assert.ok(make.includes('for level in ("Plenty", "Low", "Out", "None"):'));
  assert.ok(make.includes('f"tray-{level.lower()}-{grid}.svg"'));
  // one ink, which the page's taskbar gives as its text colour, as make-ico.py's stylesheet does
  assert.ok(make.includes('".ink { color: #ffffff; }"'));
  for (const file of ["plenty", "low", "out"]) {
    const drawing = repo(`apps/windows/Icon/tray-${file}-16.svg`);
    assert.match(drawing, /^<svg /);
    assert.ok(drawing.includes('<g class="ink" fill="currentColor">'), file);
  }
  assert.ok(present("accounts.rs").includes(".map(|limit| crate::usage_level(limit.percent)),"));
  assert.equal(menuBarLevel(moments.before), "y");
  assert.equal(menuBarLevel(moments.after), "g");
});

test("the window and its places are named as MainWindow.xaml and SettingsPage.xaml name them", () => {
  const source = views("MainWindow.xaml");
  assert.equal(app, "Pitboard");
  assert.ok(source.includes('Title="{x:Bind shown:Words.AppName}"'));
  assert.ok(present("app_words.rs").includes("app_name: words::PITBOARD.into(),"));
  assert.ok(owned(app));
  // Accounts in the navigation, and NavigationView's own Settings at the foot
  assert.deepEqual(
    panes.map(([, name]) => name),
    ["Accounts"],
  );
  assert.ok(source.includes('Content="{x:Bind shown:Words.Window.Accounts}"'));
  assert.ok(source.includes('PaneDisplayMode="LeftCompact"'));
  assert.ok(owned("Accounts"));
  // Settings > Troubleshoot's cards open Diagnostics and Activity under Settings, with the title
  // bar's back button
  const settings = views("SettingsPage.xaml");
  assert.ok(settings.includes('Text="{x:Bind shown:Words.Settings.Troubleshoot}"'));
  assert.ok(owned(TROUBLESHOOT));
  for (const [pane, title, , about] of subpages) {
    const field = pane === "machine" ? "Diagnostics" : "Activity";
    assert.ok(settings.includes(`Header="{x:Bind shown:Words.Window.${field}}"`), field);
    assert.ok(
      settings.includes(`Description="{x:Bind shown:Words.Settings.${field}About}"`),
      field,
    );
    assert.ok(owned(title), title);
    assert.ok(
      owned(about) || said.includes(about.replace("Pitboard, Claude Code and Codex", "{}")),
      about,
    );
  }
  const window = views("MainWindow.xaml.cs");
  assert.ok(
    window.includes(
      "AppTitleBar.IsBackButtonVisible = args.SourcePageType == typeof(DiagnosticsPage) || args.SourcePageType == typeof(ActivityPage);",
    ),
  );
  assert.ok(views("SettingsBreadcrumb.xaml").includes("<BreadcrumbBar"));
  assert.ok(
    owned(diagnostics.title) && owned(diagnostics.checkAgain) && owned(diagnostics.copyReport),
  );
});

test("the Accounts page says what AccountsPage.xaml binds and the core says", () => {
  const page = accountsPage(moments.after, switchNotice);
  const xaml = views("AccountsPage.xaml");
  assert.ok(xaml.includes('Text="{x:Bind shown:Words.Window.Accounts}"'));
  for (const field of ["Refresh", "AddAccount"])
    assert.ok(xaml.includes(`Label="{x:Bind shown:Words.Window.${field}}"`), field);
  for (const said of [page.title, ...page.commands]) assert.ok(owned(said), said);
  assert.ok(present("words.rs").includes('format!("Updated {clock}")'));
  assert.match(page.updated, /^Updated \d\d:\d\d$/);
  assert.ok(present("words.rs").includes('titled(os, "In Use", IN_USE)'));
  assert.deepEqual(
    page.sections.map((s) => s.name),
    ["Claude Code", "Codex"],
  );
  const rows = page.sections.flatMap((s) => s.rows);
  assert.ok(rows.every((r) => r.inUse !== (r.action?.title === "Switch")));
  // Narrator hears a row's toggle, its button and each limit as present/accounts.rs says them
  assert.ok(xaml.includes('AutomationProperties.Name="{x:Bind Item.Spoken, Mode=OneWay}"'));
  assert.ok(
    xaml.includes(
      'AutomationProperties.Name="{x:Bind local:AccountsPage.ActionSpoken(Item), Mode=OneWay}"',
    ),
  );
  assert.ok(views("LimitLines.cs").includes("AutomationProperties.SetName(name, limit.Spoken);"));
  const personal = rows.find((r) => r.label === "personal")!;
  assert.equal(personal.spoken, "personal, Max 20x");
  assert.equal(personal.action?.spoken, "Switch to personal");
  assert.equal(personal.limits[0].spoken, "5-hour limit, 59 percent used, resets 22:00");
  assert.equal(personal.keys, "Delete F2 Control+C");
});

test("an opened row's cards say what AccountsPage.xaml binds from app_words() and the core, and its commands follow", () => {
  const xaml = views("AccountsPage.xaml");
  for (const [field, words] of [
    ["Email", detail.email],
    ["SignIn", detail.signIn],
    ["WarmUp", detail.warmUp],
  ]) {
    assert.ok(xaml.includes(`Header="{x:Bind shown:Words.Window.${field}}"`), field);
    assert.ok(
      xaml.includes(`AutomationProperties.Name="{x:Bind shown:Words.Window.${field}}"`),
      field,
    );
    assert.ok(owned(words), words);
  }
  assert.ok(xaml.includes('Content="{x:Bind shown:Words.Window.Copy}"'));
  assert.ok(xaml.includes('AutomationProperties.Name="{x:Bind shown:Words.Menu.CopyEmail}"'));
  assert.ok(owned(detail.copy) && owned("Copy email address"));
  assert.ok(
    present("words.rs").includes(`titled(os, "Warm-Up Settings…", "${detail.warmUpSettings}")`),
  );
  assert.ok(present("words.rs").includes('format!("Saved sign-in valid until {until}")'));
  assert.ok(present("words.rs").includes(`pub(crate) const WARM_UP_OFF: &str = "${detail.off}";`));
  // the day and month in the regional format's order, its month abbreviated
  assert.ok(
    shell("WindowsLocalTime.cs").includes(
      'MonthDayPattern.Replace("MMMM", "MMM", StringComparison.Ordinal)',
    ),
  );
  const rows = accountsPage(moments.after, null).sections.flatMap((s) => s.rows);
  const row = (id: string) => rows.find((r) => r.id === id)!;
  assert.equal(row("work").signIn, "In use");
  assert.equal(row("personal").signIn, "Saved sign-in valid until 9 Nov");
  assert.equal(row("spare").signIn, null, "no saved sign-in's date to say");
  assert.deepEqual(
    [row("personal").warm, row("work").warm, row("main").warm],
    ["Warms up again at 22:00", "Off", null],
  );
  // its windows, then what its shortcut menu offers past its own button, and Remove, which the
  // account in use holds back
  const shown = shell("ShownAccount.cs");
  assert.ok(
    shown.includes(
      "action is { Kind: ActionKind.SignInAgain or ActionKind.Rename or ActionKind.WarmUpNow, Enabled: true }",
    ),
  );
  assert.ok(shown.includes("&& !Equals(action.Intent, item.Action?.Intent))"));
  assert.ok(shown.includes(".. item.Actions.Where(action => action.Kind is ActionKind.Remove)"));
  assert.ok(owned("Sign in again…") && owned("Warm up now"));
  assert.deepEqual(
    rows.map((r) => r.commands.signInAgain),
    rows.map((r) => (r.action?.title === "Sign in…" ? null : "Sign in again…")),
  );
  // each limit's card, its reset then how long until it
  for (const bound of [
    'Header="{x:Bind Name}"',
    'Description="{x:Bind Used}"',
    'Text="{x:Bind ResetsIn}"',
  ])
    assert.ok(xaml.includes(bound), bound);
  assert.ok(present("words.rs").includes('format!("in {}", span(seconds))'));
  const [card, line] = [row("work").cards[0], row("work").limits[0]];
  assert.deepEqual(
    [card.name, card.used, card.resets],
    ["5-hour limit", `${line.figure} used`, line.resets],
  );
  assert.match(card.resetsIn, /^in \d/);
  assert.equal(row("work").commands.open, "Open claude.ai");
  assert.equal(row("main").commands.open, "Open chatgpt.com");
  assert.deepEqual(
    [row("work").commands.remove.enabled, row("personal").commands.remove.enabled],
    [false, true],
  );
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
  assert.equal(switchNotice.title, "Switched Claude Code to work");
  assert.equal(switchNotice.countdown, "Claude Code sessions already open follow in 0:21");
});

test("an account's note is its most pressing, a saved sign-in's only once it expires within days", () => {
  const accounts = present("accounts.rs");
  assert.ok(accounts.includes("let notes: Vec<AccountNote> = ["));
  assert.ok(accounts.includes(".filter(|_| account.standing == status::Standing::Expiring)"));
  assert.ok(
    views("AccountsPage.xaml.cs").includes(
      'public static string NoteOf(AccountItem item) => item.Notes.FirstOrDefault()?.Text ?? "";',
    ),
  );
  const notes = (moment: "before" | "after") =>
    accountsPage(moments[moment], null).sections.flatMap((s) => s.rows.map((r) => r.note));
  assert.deepEqual(notes("before"), [null, null, null, null]);
  assert.deepEqual(notes("after"), [null, null, null, null]);
  // two days before work's saved sign-in expires, and once it has
  const work = moments.after.accounts.find((a) => a.id === "work")!;
  const at = (now: number) =>
    accountsPage({ ...moments.before, now }, null).sections[0].rows.find((r) => r.id === "work")!;
  assert.equal(at(work.refreshExpiresAt! - 2 * 86_400 - 60).note, "Sign-in expires in 2 days");
  const expired = at(work.refreshExpiresAt!);
  assert.equal(expired.note, "Sign-in expired");
  // and its button signs in, as a row's does (present/accounts.rs `action`)
  assert.equal(expired.action?.title, "Sign in…");
  assert.ok(owned("Sign in…"));
});

test("Settings says what SettingsPage.xaml, SettingsViewModel.cs and machine.rs say", () => {
  const s = settingsPage;
  const xaml = views("SettingsPage.xaml");
  assert.ok(xaml.includes('Text="{x:Bind shown:Words.Settings.Title}"'));
  assert.ok(xaml.includes('Text="{x:Bind shown:Words.Settings.Limits}"'));
  assert.ok(owned(s.title) && owned(s.section));
  // When a limit reaches with its icon, SpeedHigh, the site's Fluent top speed
  assert.ok(xaml.includes('<FontIcon Glyph="&#xEC4A;" />'));
  assert.ok(
    xaml.includes(
      '<toolkit:SettingsExpander Header="{x:Bind ViewModel.AtLimitHeading, Mode=OneWay}"',
    ),
  );
  const machine = joined(present("machine.rs"));
  const words = joined(present("words.rs"));
  assert.ok(words.includes(`pub(crate) const AT_LIMIT: &str = "${s.atLimit.header}";`));
  assert.ok(words.includes(`"${s.atLimit.choice}"),`));
  assert.ok(machine.includes("share_label: words::AT.into(),"));
  assert.ok(words.includes(`pub(crate) const AT: &str = "${s.share.header}";`));
  // the ComboBox and the NumberBox are named as the XAML binds them
  assert.ok(words.includes('format!("{AT_LIMIT} {at}%")'));
  assert.ok(shell("SettingsViewModel.cs").includes("AtLimitLabel => AtLimit.AtLabel;"));
  assert.ok(
    xaml.includes('AutomationProperties.Name="{x:Bind ViewModel.AtLimitLabel, Mode=OneWay}"'),
  );
  assert.ok(
    xaml.includes('AutomationProperties.Name="{x:Bind ViewModel.ShareTitle, Mode=OneWay}"'),
  );
  assert.equal(s.atLimit.label, "When a limit reaches 95%");
  assert.ok(words.includes('format!("From {lowest}% to {highest}%")'));
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
  const note = `Pitboard switches Claude Code at ${s.share.value}%, and its running sessions follow within about 33 seconds.`;
  assert.ok(
    words.includes(
      '"Pitboard switches Claude Code at {at}%, and its running sessions follow within about {seconds} seconds."',
    ),
  );
  assert.ok(machine.includes("pitboard_core::switch::ADOPTION_CEILING_SECONDS,"));
  assert.ok(core("switch/mod.rs").includes("pub const ADOPTION_CEILING_SECONDS: u32 = 33;"));
  assert.equal(s.atLimit.note, note);
  // Is this allowed? alone after the note, the terms' own sentence left to the page it opens
  assert.ok(machine.includes("link_title: words::IS_THIS_ALLOWED.into(),"));
  assert.ok(words.includes(`pub(crate) const IS_THIS_ALLOWED: &str = "${s.atLimit.link}";`));
  assert.ok(xaml.includes('Content="{x:Bind ViewModel.TermsLinkTitle, Mode=OneWay}"'));
  assert.ok(
    !xaml.includes("Terms.Sentence") && !shell("SettingsViewModel.cs").includes("Terms.Sentence"),
  );
  // At, only where something happens at the share
  assert.ok(xaml.includes('Visibility="{x:Bind ViewModel.ShowsShare, Mode=OneWay}"'));
  assert.ok(
    shell("SettingsViewModel.cs").includes(
      "public bool ShowsShare => AtLimit.Choice != Core.AtLimit.Nothing;",
    ),
  );
});

test("Warm-up says what SettingsPage.xaml, WarmAccountRow.cs and machine.rs say", () => {
  const w = settingsPage.warm;
  const xaml = views("SettingsPage.xaml");
  assert.ok(xaml.includes('Header="{x:Bind ViewModel.WarmTitle, Mode=OneWay}"'));
  assert.ok(present("machine.rs").includes("title: words::warm_up(seen.os),"));
  assert.ok(present("words.rs").includes(`titled(os, "Warm-Up", "${w.header}")`));
  // what a warm-up does under its name, its hours beside it, and its icon, Brightness
  assert.ok(xaml.includes('<TextBlock Text="{x:Bind ViewModel.WarmNote, Mode=OneWay}" />'));
  assert.ok(present("machine.rs").includes("note: words::WARM_UP_NOTE.into(),"));
  assert.ok(owned(w.note));
  assert.ok(
    xaml.includes(
      '<TextBlock Text="{x:Bind ViewModel.WarmRange, Mode=OneWay}" AutomationProperties.AutomationId="settings.warmRange" />',
    ),
  );
  assert.ok(xaml.includes('<FontIcon Glyph="&#xE706;" />'));
  for (const [name] of w.hours) {
    const field = name === "From" ? "From" : "To";
    assert.ok(xaml.includes(`Header="{x:Bind shown:Words.Settings.${field}}"`), name);
    assert.ok(owned(name), name);
  }
  assert.ok(present("words.rs").includes('Some(format!("{} to {to}", time_of_day(from)?))'));
  assert.equal(w.range, "07:00 to 23:00");
  assert.ok(
    present("words.rs").includes('Some(at) => format!("Next warm-up: {account} at {at}."),'),
  );
  assert.equal(w.standing, "Next warm-up: personal at 22:00.");
  assert.ok(present("machine.rs").includes("title: choice.account.clone(),"));
  assert.ok(shell("WarmAccountRow.cs").includes("public string Title => _account.Title;"));
  assert.deepEqual(w.accounts, [
    { title: "personal", standing: "Warms up again at 22:00", on: true },
    { title: "work", standing: null, on: false },
  ]);
});

test("Activity lists Windows' changes in the app's words, dated as the regional format's g", () => {
  const page = activityPage;
  const xaml = views("ActivityPage.xaml");
  assert.ok(
    xaml.includes(
      '<local:SettingsBreadcrumb VerticalAlignment="Top" Title="{x:Bind shown:Words.Window.Activity}"',
    ),
  );
  assert.ok(xaml.includes('Label="{x:Bind shown:Words.Window.Refresh}"'));
  assert.ok(owned(page.title) && owned(page.command));
  assert.ok(shell("WindowsLocalTime.cs").includes('At(epoch).ToString("g", Culture)'));
  const words = present("words.rs");
  assert.ok(words.includes('"use" | "switch" => "Switch".into(),'));
  assert.ok(words.includes('"enroll" => "Add".into(),'));
  assert.ok(words.includes('"warm-up" => "Warm-up".into(),'));
  assert.ok(words.includes('"ok" => "Done".into(),'));
  assert.ok(words.includes('"app" => "Pitboard app".into(),'));
  assert.ok(words.includes('"cli" => "Command line".into(),'));
  assert.deepEqual(
    page.rows.map((r) => [r.date, r.change, r.account, r.result, r.by]),
    [
      ["13/10/2026 20:49", "Switch", "work", "Done", "Pitboard app"],
      ["13/10/2026 17:00", "Warm-up", "personal", "Done", "Pitboard app"],
      ["10/10/2026 09:16", "Add", "work", "Done", "Command line"],
    ],
  );
});

test("the notification at the share says what WindowsNotifications.cs builds from notices.rs", () => {
  const source = shell("WindowsNotifications.cs");
  assert.ok(source.includes('public const string DisplayName = "Pitboard";'));
  assert.ok(source.includes("new AppNotificationButton(button.Title)"));
  const words = joined(present("words.rs"));
  // a toast takes any button, so it names the account it switches to
  assert.ok(words.includes("Os::Windows => switch_to(label),"));
  const notices = present("notices.rs");
  assert.ok(notices.includes("words::switch_button(OS, instead),"));
  // short of 100%, the title names the limit, and the body how much of it the account has used
  assert.ok(notices.includes("let title = words::limit_notification("));
  assert.ok(
    notices.includes(
      "let used = words::has_used_share(&heads_up.ran, &said::figure(heads_up.window.percent));",
    ),
  );
  assert.ok(
    notices.includes("(Some((instead, left, _)), _) => words::other_used(instead, *left),"),
  );
  assert.ok(
    words.includes('Some(left) => format!("{account} has used {}%.", (100 - left).max(0)),'),
  );
  assert.ok(core("duty/autoswitch.rs").includes("pub const DEFAULT: Threshold = Threshold(95);"));
  assert.deepEqual(toast, {
    app: "Pitboard",
    title: "Weekly limit almost reached",
    tool: "Claude Code",
    body: "personal has used 95%. work has used 40%.",
    button: "Switch to work",
  });
});

test("the questions' Windows shortcuts are AccountKeys.cs's: F2, Delete and Ctrl+C", () => {
  const keys = shell("AccountKeys.cs");
  assert.ok(
    keys.includes(
      "VirtualKey.F2 when Offered(item, ActionKind.Rename) is { Intent: { } intent } rename",
    ),
  );
  assert.ok(
    keys.includes(
      "VirtualKey.Delete when Offered(item, ActionKind.Remove) is { Intent: { } intent } remove",
    ),
  );
  assert.ok(keys.includes("key == VirtualKey.C && control && item.Email.Length > 0 ? item.Email"));
  const page = views("AccountsPage.xaml.cs");
  assert.ok(page.includes("AccountKeys.Copied(args.Key, control, shown.Item)"));
  assert.ok(page.includes("AccountKeys.Pressed(args.Key, shown.Item)"));
  assert.ok(
    repo("website/src/pages/index.astro").includes(
      "include F2 to rename the account selected, Delete to remove it and Ctrl+C to copy its email address.",
    ),
  );
});
