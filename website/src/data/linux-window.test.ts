import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { test } from "node:test";
import {
  about,
  acceleratorLabel,
  accountMenu,
  accountsPage,
  activityPage,
  atLimitNote,
  date,
  diagnostics,
  group,
  iconOf,
  openLink,
  prefs,
  switchNotice,
  warmAccounts,
  type Icon,
} from "./linux-window.ts";
import { shortcuts } from "./linux.ts";
import { account, accounts, moments } from "./world.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const linux = (file: string) => repo(`apps/linux/${file}`);
const present = (file: string) => repo(`crates/pitboard-model/src/present/${file}`);
// a Rust string literal as it reads once its line continuations are joined
const joined = (source: string) => source.replace(/\\\n\s*/g, "");
const says = (source: string, words: string) => assert.ok(joined(source).includes(words), words);

test("dates read as local_time.rs formats them", () => {
  assert.ok(linux("src/local_time.rs").includes('formatted(&local(epoch)?, "%b %-e")'));
  assert.ok(linux("src/local_time.rs").includes('&format!("%b %-e, %Y, {}", self.time())'));
  assert.equal(date(account("work").refreshExpiresAt!), "Nov 9");
});

// What linux-facts.yml's KDE probe resolved each name to under Breeze on Fedora 44 (run
// 41107324579): Breeze's actions/16, GTK's resources, libadwaita's, or image-missing, where the
// page draws Adwaita's.
const underBreeze: Record<string, "breeze" | "gtk" | "libadwaita" | "missing"> = {
  "window-close": "breeze",
  "window-minimize": "breeze",
  "window-maximize": "breeze",
  "adw-external-link": "libadwaita",
  "battery-level-50": "missing",
  "dialog-error": "gtk",
  "dialog-information": "gtk",
  "dialog-warning": "gtk",
  "edit-copy": "breeze",
  "insert-link": "breeze",
  "list-add": "breeze",
  "object-select": "breeze",
  "open-menu": "breeze",
  "preferences-other": "breeze",
  "preferences-system": "breeze",
  "view-more": "breeze",
  "view-refresh": "breeze",
  "web-browser": "missing",
};

test("each desktop's icon for a name is the one its theme gives GTK", () => {
  for (const [name, from] of Object.entries(underBreeze))
    assert.equal(
      iconOf(name as Icon, "kde"),
      from === "missing" ? `adwaita-${name}` : `${from}-${name}`,
      name,
    );
  const icons = new URL("../assets/icons/", import.meta.url);
  const file = (id: string) => {
    const [theme, ...rest] = id.split("-");
    const name = rest.join("-");
    return new URL(`${theme}/${name}-symbolic.svg`, icons);
  };
  const named: Icon[] = [
    ...(Object.keys(underBreeze) as Icon[]),
    "go-previous",
    "go-next",
    "pan-down",
    "value-decrease",
    "value-increase",
    "adw-entry-edit",
  ];
  for (const name of named)
    for (const desktop of ["gnome", "kde"] as const)
      assert.ok(existsSync(file(iconOf(name, desktop))), `${name} on ${desktop}`);
  // each the window names is one the app or its toolkit asks for
  const app = [
    "data/ui/window.ui",
    "data/ui/account-group.ui",
    "data/ui/notice-row.ui",
    "data/ui/preferences.ui",
    "src/window/notices.rs",
    "src/pages/diagnostics.rs",
    "src/dialogs/open_link.rs",
    "src/preferences/advanced.rs",
  ]
    .map(linux)
    .join("\n");
  for (const name of Object.keys(underBreeze).filter((n) => !n.startsWith("window-")))
    assert.ok(app.includes(`${name}-symbolic`), name);
});

test("each group says what account_group.rs sets from the model", () => {
  const page = accountsPage(moments.after);
  assert.deepEqual(
    page.groups.map((g) => [g.label, g.description, g.inUse, g.button?.title ?? null]),
    [
      ["work", "Claude Code, Max 20x", true, null],
      ["personal", "Claude Code, Max 20x", false, "Switch"],
      ["main", "Codex, Pro", true, null],
      ["spare", "Codex, Plus", false, "Switch"],
    ],
  );
  says(present("words/accounts.rs"), 'said.join(", ")');
  const personal = accountsPage(moments.before).groups[0];
  assert.deepEqual(
    personal.limits.map((l) => [l.title, l.resets, l.figure, l.fill]),
    [
      ["5-hour limit", "Resets 22:00", "59%", "plenty"],
      ["Weekly limit", "Resets Mon 14:50", "73%", "low"],
    ],
  );
  assert.equal(personal.signIn, "In use");
  assert.equal(personal.warm, "Warms up again at 22:00");
  const work = accountsPage(moments.before).groups[1];
  assert.equal(work.signIn, "Saved sign-in valid until Nov 9");
  assert.equal(work.warm, "Off");
  assert.equal(page.groups[2].warm, null);
  for (const [name, value] of Object.entries(group)) assert.ok(value, name);
  says(
    present("words/machine.rs"),
    'Front::Gnome => format!("{} {}", warm_up(front), settings(front, true)),',
  );
  assert.equal(group.warmUpPreferences, "Warm-Up Preferences");
  says(linux("src/window/account_group.rs"), "RowEnd::Switching");
  assert.ok(linux("data/style.css").includes("levelbar block.plenty"));
});

test("an account's menu is its actions less the header's, then Copy Email Address and Remove", () => {
  const titles = (label: string, tool: "claude" | "codex" = "claude") =>
    accountMenu(account(label, tool), moments.before).map((s) =>
      s.map((o) => (o.enabled ? o.title : `(${o.title})`)),
    );
  assert.deepEqual(titles("personal"), [
    ["(Switch to personal)", "Sign In Again…", "Rename…", "(Warm Up Now)"],
    ["Copy Email Address"],
    ["(Remove Account…)"],
  ]);
  assert.deepEqual(titles("work"), [
    ["Sign In Again…", "Rename…", "(Warm Up Now)"],
    ["Copy Email Address"],
    ["Remove Account…"],
  ]);
  assert.deepEqual(titles("spare", "codex"), [
    ["Sign In Again…", "Rename…"],
    ["Copy Email Address"],
    ["Remove Account…"],
  ]);
  const group = linux("src/window/account_group.rs");
  says(group, "if action.intent.is_some() && action.intent == offered {");
  says(group, "let section = if kind == ActionKind::Remove { 2 } else { 0 };");
  says(group, "sections[1].push((words.copy_email, COPY_EMAIL));");
});

test("the switch notice offers Switch Back only after an automatic switch", () => {
  says(present("notices.rs"), "let back = last.back.as_ref().map(|back| NoticeAction {");
  says(
    repo("crates/pitboard-model/src/model/mod.rs"),
    "// where switching by itself made it\n    pub back: Option<Back>,",
  );
  assert.equal(switchNotice.title, "Switched Claude Code to work");
  assert.match(switchNotice.countdown, /^Claude Code sessions already open follow in \d:\d\d$/);
});

test("Diagnostics and Activity say the model's words", () => {
  const words = present("app_words.rs");
  for (const said of [
    `check_again_help: "${diagnostics.checkAgainHelp}"`,
    `refresh_help: "${activityPage.refreshHelp}"`,
    `diagnostics: "${diagnostics.title}"`,
  ])
    says(words, said);
  says(words, diagnostics.copyReportHelp);
  for (const column of activityPage.columns.filter((c) => c !== "Account"))
    says(words, `${column.toLowerCase()}: "${column}".into(),`);
  assert.deepEqual(activityPage.rows[0], {
    date: "Oct 13, 2026, 20:49",
    change: "Switch",
    account: "work",
    result: "Done",
    by: "Pitboard app",
  });
  assert.ok(!activityPage.rows.some((r) => r.account.startsWith("desktop/")));
  assert.equal(diagnostics.checking, "Checking this computer…");
});

test("Preferences says machine.rs's words, with a tray and without", () => {
  const machine = present("words/machine.rs");
  says(
    present("words/setup.rs"),
    `"${prefs.general.startAtLogin}",\n            "${prefs.general.startNote}",`,
  );
  says(
    machine,
    "Pitboard notifies you once a limit of the account in use reaches {at}%, while the app is open.",
  );
  says(
    machine,
    "Pitboard switches Claude Code at {at}%, and its running sessions follow within about {seconds} seconds.",
  );
  says(
    machine,
    "Once its window is closed, nothing switches it unless {WATCH_IN_THE_BACKGROUND} is on.",
  );
  assert.equal(atLimitNote("nothing", true), null);
  assert.equal(atLimitNote("switch", true)?.endsWith("33 seconds."), true);
  assert.equal(
    atLimitNote("switch", false),
    "Pitboard switches Claude Code at 95%, and its running sessions follow within about 33 seconds. Once its window is closed, nothing switches it unless Watch in the Background is on.",
  );
  says(machine, `None => "while it’s open",\n        Some(false) => "while its window is open",`);
  assert.equal(
    prefs.limits.warm.note(false),
    "Pitboard starts each account’s 5-hour limit again as soon as it resets, within these hours, while its window is open.",
  );
  says(
    machine,
    "{CLOSING_QUITS} Turned on, {} goes on switching Claude Code at {}% and warming up the accounts {} turns on, from each login.",
  );
  assert.equal(
    prefs.advanced.watchAbout(true),
    "Pitboard quits once its window is closed. Turned on, this computer goes on switching Claude Code at 95% and warming up the accounts Warm-Up turns on, from each login.",
  );
  says(machine, `"${prefs.advanced.updateNote}"`);
  says(machine, `pub(crate) const UNINSTALL_QUESTION: &str = "${prefs.uninstallQuestion.title}";`);
  says(machine, `pub(crate) const REMOVE_THE_APP: &str = "${prefs.removeTheApp.title}";`);
  says(
    present("removal.rs"),
    "Remove the app and its command line with this command in a terminal.",
  );
  says(present("removal.rs"), 'const PACKAGES: &str = "pitboard-app pitboard";');
  assert.equal(prefs.removeTheApp.command, "sudo dnf remove pitboard-app pitboard");
  // the package's command line sits beside the app, which the core takes for the app's own
  says(repo("crates/pitboard-core/src/host/mod.rs"), '.then(|| app.with_file_name("pitboard")),');
  assert.deepEqual(
    warmAccounts(moments.after).map((a) => [a.label, a.on, a.standing]),
    [
      ["work", false, null],
      ["personal", true, "Warms up again at 22:00"],
    ],
  );
  assert.ok(linux("data/ui/preferences.ui").includes("battery-level-50-symbolic"));
});

test("Open Link, Keyboard Shortcuts and About say what the app gives them", () => {
  const windows = present("words/windows.rs");
  says(windows, `titled(front, "${openLink.none.title}", "No link to open")`);
  says(windows, "Paste a {sites} link, or drop one here, to open it in your browser.");
  says(windows, `titled(front, "${openLink.link.title}", "Open in your browser")`);
  says(windows, `titled(front, "${openLink.link.open}", "Open in browser")`);
  assert.deepEqual(
    shortcuts.keys.map(([key]) => acceleratorLabel(key)),
    ["Ctrl+N", "Ctrl+R", "Ctrl+,", "Ctrl+?", "F10", "Alt+Left", "Ctrl+W", "Ctrl+Q"],
  );
  assert.equal(about.name, "Pitboard");
  assert.ok(
    linux("data/com.usepitboard.Pitboard.metainfo.xml.in").includes("<name>Pitboard</name>"),
  );
  assert.ok(accounts.length > 0);
});
