import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  COPY_COMMAND,
  PASTE_LINK,
  WATCH_IN_THE_BACKGROUND,
  app,
  header,
  mainMenu,
  preferences,
  shortcuts,
  trayAttention,
  trayMenu,
  trayNotices,
  trayTip,
} from "./linux.ts";
import { ADD_HELP, REFRESH_HELP } from "./window.ts";
import { moments } from "./world.ts";
import { CLOSING_QUITS } from "../lib/words.ts";
import { advised, model, reset, start, use } from "../lib/model.ts";
import { noticeMoment } from "../lib/moment.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const linux = (file: string) => repo(`apps/linux/${file}`);
const present = (file: string) => repo(`crates/pitboard-model/src/present/${file}`);
// A Rust string literal as it reads once its line continuations are joined.
const joined = (source: string) => source.replace(/\\\n\s*/g, "");
const appWords = joined(present("app_words.rs"));
// each of `parts` in `source`, in that order
const inOrder = (source: string, parts: string[]) => {
  const at = parts.map((p) => source.indexOf(p));
  for (const [i, n] of at.entries()) assert.ok(n >= 0, parts[i]);
  assert.deepEqual(
    at,
    [...at].sort((a, b) => a - b),
    `in the order ${parts.join(", ")}`,
  );
};

test("the header bar says what window/mod.rs gives it from the model's words for GNOME", () => {
  const window = linux("src/window/mod.rs");
  for (const set of [
    "self.set_title(Some(&words.app_name));",
    "imp.window_title.set_title(&words.window.accounts);",
    "imp.add_account\n            .set_tooltip_text(Some(&words.window.add_account_help));",
    "gtk::accessible::Property::Label(&words.window.add_account)",
    "imp.refresh\n            .set_tooltip_text(Some(&words.window.refresh_help));",
    "gtk::accessible::Property::Label(&words.window.refresh)",
    "imp.main_menu.set_tooltip_text(Some(&gnome.main_menu));",
  ])
    assert.ok(window.includes(set), set);
  assert.equal(app, "Pitboard");
  assert.ok(present("words/mod.rs").includes(`pub(crate) const PITBOARD: &str = "${app}";`));
  assert.ok(appWords.includes(`accounts: "${header.title}".into(),`));
  assert.ok(appWords.includes("add_account: words::add_account(front, add_account_asks),"));
  assert.ok(appWords.includes("Front::WindowsNotificationArea | Front::Gnome => true,"));
  assert.equal(header.add.name, "Add Account…");
  assert.equal(header.add.tip, ADD_HELP);
  assert.ok(appWords.includes(`add_account_help: "${ADD_HELP}".into(),`));
  assert.ok(appWords.includes(`refresh_help: "${REFRESH_HELP}".into(),`));
  assert.equal(header.refresh.tip, REFRESH_HELP);
  assert.ok(
    present("words/mod.rs").includes(`pub(crate) const REFRESH: &str = "${header.refresh.name}";`),
  );
  assert.ok(appWords.includes(`main_menu: "${header.menu.tip}".into(),`));
  // the window's buttons from start to end, as data/ui/window.ui packs them
  inOrder(linux("data/ui/window.ui"), ['id="add_account"', 'id="main_menu"', 'id="refresh"']);
});

test("the main menu holds the app's own items, then GNOME's, and no Quit", () => {
  inOrder(linux("src/window/mod.rs"), [
    "own.append(Some(&gnome.open_link)",
    "Some(&words.window.diagnostics)",
    "Some(&words.window.activity)",
    "standard.append(Some(&words.menu.settings)",
    "standard.append(Some(&gnome.keyboard_shortcuts)",
    "standard.append(Some(&words.menu.help)",
    "standard.append(Some(&words.menu.about)",
  ]);
  const [own, standard] = mainMenu;
  assert.ok(appWords.includes(`open_link: "${own[0]}".into(),`));
  assert.ok(appWords.includes(`diagnostics: "${own[1]}".into(),`));
  assert.ok(appWords.includes(`activity: "${own[2]}".into(),`));
  assert.ok(appWords.includes("settings: words::settings(front, true),"));
  assert.equal(standard[0], "Preferences");
  assert.ok(appWords.includes(`const KEYBOARD_SHORTCUTS: &str = "${standard[1]}";`));
  assert.ok(appWords.includes(`Front::Gnome => "${standard[2]}".into(),`));
  assert.ok(appWords.includes(`about: titled(front, "${standard[3]}", "${standard[3]}"),`));
});

test("Preferences has the pages data/ui/preferences.ui lays out, in its order", () => {
  inOrder(
    linux("data/ui/preferences.ui"),
    preferences.pages.map((p) => `id="${p.toLowerCase()}"`),
  );
  assert.ok(appWords.includes("title: words::settings(front, false),"));
  assert.equal(preferences.title, "Preferences");
  for (const page of preferences.pages)
    assert.ok(appWords.includes(`${page.toLowerCase()}: "${page}".into(),`), page);
});

test("Keyboard Shortcuts lists the keys window/keys.rs binds, in its order and words", () => {
  const keys = linux("src/window/keys.rs");
  const names = ["AddAccount", "Refresh", "Preferences", "KeyboardShortcuts"];
  names.push("MainMenu", "Back", "CloseWindow", "Quit");
  inOrder(
    keys.slice(keys.indexOf("pub const ALL")),
    names.map((n) => `Shortcut::${n},`),
  );
  const bound: Record<string, string> = {
    "<Control>n": "(gdk::Key::n, control)",
    "<Control>r": "(gdk::Key::r, control)",
    "<Control>comma": "(gdk::Key::comma, control)",
    "<Control>question": "(gdk::Key::question, control)",
    F10: "(gdk::Key::F10, gdk::ModifierType::empty())",
    "<Alt>Left": "(gdk::Key::Left, gdk::ModifierType::ALT_MASK)",
    "<Control>w": "(gdk::Key::w, control)",
    "<Control>q": "(gdk::Key::q, control)",
  };
  const fields = ["add_account", "refresh", "preferences", "keyboard_shortcuts"];
  fields.push("main_menu", "back", "close_window", "quit");
  shortcuts.keys.forEach(([accelerator, title], i) => {
    assert.ok(keys.includes(`Shortcut::${names[i]} => ${bound[accelerator]},`), accelerator);
    assert.ok(keys.includes(`Shortcut::${names[i]} => &words.${fields[i]},`), title);
  });
  const said = appWords.slice(appWords.indexOf("shortcuts: GnomeShortcutWords {"));
  assert.ok(said.includes(`general: "${shortcuts.section}".into(),`));
  assert.ok(said.includes("add_account: words::add_account(front, false),"));
  assert.ok(said.includes("refresh: words::REFRESH.into(),"));
  assert.ok(said.includes("preferences: words::settings(front, false),"));
  for (const [field, title] of [
    ["main_menu", "Open Main Menu"],
    ["back", "Go Back"],
    ["close_window", "Close Window"],
    ["quit", "Quit"],
  ])
    assert.ok(said.includes(`${field}: "${title}".into(),`), title);
  assert.deepEqual(
    shortcuts.keys.map(([, title]) => title),
    [
      "Add Account",
      "Refresh",
      "Preferences",
      "Keyboard Shortcuts",
      "Open Main Menu",
      "Go Back",
      "Close Window",
      "Quit",
    ],
  );
});

test("the words only the Linux app says are app_words.rs's for it", () => {
  for (const [field, words] of [
    ["paste_link", PASTE_LINK],
    ["copy_command", COPY_COMMAND],
  ])
    assert.ok(appWords.includes(`${field}: "${words}".into(),`), words);
  assert.ok(appWords.includes("watch_service: words::WATCH_IN_THE_BACKGROUND.into(),"));
  assert.ok(
    present("words/machine.rs").includes(
      `pub(crate) const WATCH_IN_THE_BACKGROUND: &str = "${WATCH_IN_THE_BACKGROUND}";`,
    ),
  );
  assert.ok(appWords.includes("close_note: words::CLOSING_QUITS.into(),"));
  assert.ok(
    present("words/setup.rs").includes(
      `pub(crate) const CLOSING_QUITS: &str = "${CLOSING_QUITS}";`,
    ),
  );
});

test("the tray's menu is presence.rs's, in the entries a Linux session saw it draw", () => {
  inOrder(linux("src/presence.rs"), [
    "&words.open_pitboard,",
    "&section.heading",
    "&account.menu_title,",
    "&words.add_account,",
    "&words.refresh,",
    "&words.settings,",
    "&words.help,",
    "&words.quit,",
  ]);
  const menu = joined(present("app_words.rs"));
  assert.ok(menu.includes('open_pitboard: titled(front, "Open Pitboard", "Open Pitboard"),'));
  assert.ok(menu.includes("add_account: words::add_account(front, true),"));
  assert.ok(menu.includes('Front::Gnome => "Quit",'));
  assert.deepEqual(trayMenu(moments.after), [
    [{ command: "Open Pitboard", does: "open" }],
    [
      { heading: "Claude Code", tool: "claude" },
      { account: "work (40%)", label: "work", tool: "claude", checked: true },
      { account: "personal (73%)", label: "personal", tool: "claude", checked: false },
      { heading: "Codex", tool: "codex" },
      { account: "main (7%)", label: "main", tool: "codex", checked: true },
      { account: "spare (0%)", label: "spare", tool: "codex", checked: false },
    ],
    [
      { command: "Add Account…", does: "add-account" },
      { command: "Refresh", does: "refresh" },
    ],
    [
      { command: "Preferences", does: "preferences" },
      { command: "Help", does: "help" },
    ],
    [{ command: "Quit", does: "quit" }],
  ]);
  // each command under the id presence.rs gives its item, in the menu's order
  inOrder(
    linux("src/presence.rs"),
    ["open", "add-account", "refresh", "preferences", "help", "quit"].map((id) => `"${id}".into()`),
  );
  // docs/facts/linux-session.md: what GNOME's AppIndicator extension and Plasma drew of it
  const session = joined(repo("docs/facts/linux-session.md")).replace(/\s+/g, " ");
  assert.ok(
    session.includes(
      "Open Pitboard, each tool's name as a disabled header where there are two, its accounts as radio entries with the one in use checked, then Add Account…, Refresh, Preferences, Help and Quit.",
    ),
  );
});

test("the tray's notices follow Open Pitboard, the pressing ones counted in one entry that opens the window", () => {
  const presence = linux("src/presence.rs");
  inOrder(presence, [
    "&words.open_pitboard,",
    "let notices = &snapshot.menu_notices;",
    "&entry.title,",
    "&section.heading",
  ]);
  const notices = present("notices.rs");
  for (const part of [
    "title: words::issues(seen.front, others.len()),",
    "place: Some(Place::Accounts),",
  ])
    assert.ok(notices.includes(part), part);
  reset();
  assert.deepEqual(trayNotices(model.get()), []);
  // a switch of Codex warns that its running sessions keep the old account
  use("codex", "spare", "app");
  assert.deepEqual(trayNotices(model.get()), [{ command: "1 Issue", does: "issues" }]);
  assert.deepEqual(trayMenu(moments.after, trayNotices(model.get())).slice(0, 2), [
    [{ command: "Open Pitboard", does: "open" }],
    [{ command: "1 Issue", does: "issues" }],
  ]);
});

test("the tray's menu heads a tool's accounts with its name only while two tools have accounts", () => {
  const sections = present("accounts.rs").split("pub(crate) fn sections(")[1];
  inOrder(sections, [
    "if providers.len() <= 1 {",
    "heading: None,",
    "heading: Some(tool.name().into()),",
  ]);
  assert.ok(linux("src/presence.rs").includes("if let Some(heading) = &section.heading {"));
  const codexGone = {
    ...moments.after,
    accounts: moments.after.accounts.filter((a) => a.tool !== "codex"),
  };
  assert.deepEqual(trayMenu(codexGone)[1], [
    { account: "work (40%)", label: "work", tool: "claude", checked: true },
    { account: "personal (73%)", label: "personal", tool: "claude", checked: false },
  ]);
});

test("the tray's menu leaves out a group with nothing in it, as presence.rs does", () => {
  assert.ok(
    linux("src/presence.rs").includes("groups.into_iter().filter(|group| !group.is_empty())"),
  );
  const none = { ...moments.after, accounts: [] };
  assert.deepEqual(
    trayMenu(none).map((group) => group.map((e) => ("does" in e ? e.does : null))),
    [["open"], ["add-account", "refresh"], ["preferences", "help"], ["quit"]],
  );
});

test("the tray's tooltip is Pitboard over a line for each account in use", () => {
  const presence = linux("src/presence.rs");
  assert.ok(presence.includes("let (title, text) = shown.tip.split_once('\\n')"));
  assert.deepEqual(trayTip(moments.after), {
    title: "Pitboard",
    text: "work (Claude Code): 5-hour 12% used, weekly 40%\nmain (Codex): weekly 7% used",
  });
  assert.ok(
    repo("docs/facts/linux-session.md").includes(
      'tooltip "Pitboard" over a line per\n  account in use.',
    ),
  );
});

test("the tray's icon needs attention while its menu offers a switch, as status_item says", () => {
  const accounts = present("accounts.rs");
  for (const part of [
    "let attention = notices.install.is_some()",
    "|| !notices.switches.is_empty()",
    "|| !notices.sign_ins.is_empty()",
    "|| notices.others.is_some();",
  ])
    assert.ok(accounts.includes(part), part);
  assert.equal(trayAttention(start()), false);
  // the notification at the share offers its switch, and the window's advice comes at 100%
  assert.equal(trayAttention(noticeMoment(start())!.state), false);
  const out = start();
  for (const l of out.accounts.find((a) => a.id === "personal")!.limits) l.percent = 100;
  assert.equal(trayAttention(advised(out)), true);
});
