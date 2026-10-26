import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import * as w from "./window.ts";
import { account, moments } from "./world.ts";
import { changeOutcome, changeVerb } from "../lib/words.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const present = (file: string) => repo(`crates/pitboard-ffi/src/present/${file}`);
// A Rust string literal as it reads once its line continuations are joined.
const joined = (source: string) => source.replace(/\\\n\s*/g, "");
const words = joined(present("words.rs"));
const after = moments.after;

test("a name says its tool once two tools have accounts, on each system's list", () => {
  assert.ok(words.includes('format!("{label} ({tool})")'));
  assert.equal(w.nameOf(account("work"), after, "macos"), "work (Claude Code)");
  assert.equal(w.nameOf(account("main", "codex"), after, "windows"), "main (Codex)");
  const one = { ...after, accounts: after.accounts.filter((a) => a.tool === "claude") };
  assert.equal(w.nameOf(account("work"), one, "macos"), "work");
});

test("the window's toolbar, rows and offers say what the apps say", () => {
  const pane = repo("apps/macos/Sources/PitboardApp/Window/AccountsPane.swift");
  const page = repo("apps/windows/Pitboard.Views/AccountsPage.xaml");
  for (const help of [w.REFRESH_HELP, w.ADD_HELP]) {
    assert.ok(pane.includes(`.help("${help}")`), help);
    assert.ok(page.includes(`ToolTipService.ToolTip="${help}"`), help);
  }
  assert.ok(words.includes('titled(os, "In Use", "In use")'));
  assert.equal(w.inUse("windows"), "In use");
  const accounts = present("accounts.rs");
  assert.ok(accounts.includes('format!("Use {label}")'));
  assert.ok(accounts.includes('"Rename…".into()'));
  assert.ok(accounts.includes(`const FORGET: &str = "${w.FORGET_OFFER}";`));
  assert.ok(words.includes('titled(os, "Sign In Again…", "Sign in again…")'));
  assert.ok(words.includes('titled(os, "Warm Up Now", "Warm up now")'));
  assert.ok(
    repo("apps/macos/Sources/PitboardApp/Window/AccountCommands.swift").includes(
      `Button("${w.copyEmail("macos")}")`,
    ),
  );
  assert.ok(
    repo("apps/windows/Pitboard.Views/AccountMenu.cs").includes(
      `Text = "${w.copyEmail("windows")}", KeyboardAcceleratorTextOverride = "Ctrl+C"`,
    ),
  );
});

test("Forget asks with the account's name and says what goes, on this Mac or this PC", () => {
  assert.ok(present("accounts.rs").includes('title: format!("Forget “{spoken_name}”?"),'));
  assert.ok(present("accounts.rs").includes('confirm: "Forget".into(),'));
  assert.equal(w.forgetTitle(account("work"), after, "macos"), "Forget “work (Claude Code)”?");
  const windows = joined(repo("crates/pitboard-ffi/src/account_windows/accounts.rs"));
  assert.ok(
    windows.includes(
      "Pitboard deletes the login it parked for this account, and everything its {} window keeps on {}, its sign-in included. Using it again needs a sign-in in your browser.",
    ),
  );
  assert.ok(
    windows.includes(
      "Pitboard deletes the login it parked for this account and the folders set aside for it. Using it again needs a sign-in in {app}.",
    ),
  );
  const machines = repo("crates/pitboard-ffi/src/account_windows/mod.rs");
  assert.ok(
    machines.includes('Os::MacOs => "this Mac"') && machines.includes('Os::Windows => "this PC"'),
  );
  assert.match(
    w.forgetMessage(account("main", "codex"), "windows"),
    /its chatgpt\.com window keeps on this PC,/,
  );
  assert.match(w.forgetMessage(account("office", "desktop"), "macos"), /sign-in in Claude\.$/);
});

test("the Rename sheet says what present/sheets.rs gives it, and its problems as typed", () => {
  assert.ok(words.includes('format!("Rename “{label}”")'));
  assert.ok(words.includes(`pub(crate) const RENAMING: &str = "${w.RENAMING}";`));
  assert.ok(words.includes(`pub(crate) const RENAME: &str = "${w.RENAME}";`));
  assert.ok(words.includes(`pub(crate) const NAME_PROMPT: &str = "${w.NAME_PROMPT}";`));
  assert.ok(words.includes('format!("Couldn’t rename {label}")'));
  for (const problem of ["not_one_word", "separator"] as const)
    for (const picks of [false, true])
      assert.ok(words.includes(`"${w.unusableName(problem, picks)}"`), problem);
  assert.ok(
    repo("apps/windows/Pitboard.Windows/SheetPage.cs").includes(
      '$"Type a name for this account to {char.ToLowerInvariant(confirm[0])}{confirm[1..]}."',
    ),
  );
  assert.equal(w.typeAName(w.signIn("windows")), "Type a name for this account to sign in.");
  assert.ok(
    repo("crates/pitboard-core/src/error.rs").includes(
      '#[error("`{label}` already refers to {who}. Choose a different label.")]',
    ),
  );
});

test("the sign-in sheets and the wait say what the app says, for a browser and for Claude", () => {
  for (const said of [
    'titled(os, "Add Account", "Add account")',
    'titled(os, "Sign In", "Sign in")',
    '&format!("Sign In to {label} Again"),',
    '&format!("Signing In to {tool}"),',
    "Pitboard opens {tool}’s own sign-in in your browser. Sign in as the account you’re adding, and Pitboard parks its login beside the one in use.",
    "Pitboard opens {tool}’s own sign-in in your browser. Sign in as {label} to give Pitboard a new login for it.",
    'format!("Finish signing in as {name} in your browser. This closes once {tool} says you’re in.")',
    `pub(crate) const WAITING_FOR_BROWSER: &str = "${w.WAITING_FOR_BROWSER}";`,
    `pub(crate) const LATER: &str = "${w.LATER}";`,
    'format!("Park {label}")',
    "Pitboard parks {work}, which leaves {app} signed out and signs nothing out at claude.ai. Then you sign in to the other account in {app}, and Pitboard adds it once {app} quits.",
    "Pitboard parks {parks}, which leaves {app} signed out and signs nothing out at claude.ai. Then you sign in to {label} again in {app}, and Pitboard records it once {app} quits.",
    'format!("{label}’s login as it is")',
    '(format!("Parking {label}."), format!("Parking {label}"))',
    '"Open {app} and {sign_in}. When it shows your chats, quit {app} with Command-Q, and {adds}."',
    "{first}\\n\\nDon’t choose Log out in {app}: it ends the login {app} holds at claude.ai, and Pitboard can’t bring it back.",
    'format!("Waiting for {app} to quit")',
    '&format!("Put {label} Back"),',
  ])
    assert.ok(words.includes(said), said);
  const sheet = repo("apps/macos/Sources/PitboardApp/Sheets/SignInSheet.swift");
  assert.ok(sheet.includes('LabeledContent("Sign-In Page")'));
  assert.ok(sheet.includes('Button("Copy Link")') && sheet.includes('Button("Open")'));
  assert.ok(sheet.includes('Picker("Tool", selection: $provider)'));
  assert.ok(sheet.includes('LabeledContent("Account", value: account)'));
});

test("a switch's notice and the setup step say what present/notices.rs and setup.rs give them", () => {
  for (const said of [
    'Some(tool) => format!("Switched {tool} to {label}"),',
    'Some(tool) => format!("{label} in {tool} has a new login"),',
    'Some(tool) => format!("{tool} sessions already open follow in"),',
    '(true, None) => format!("Signed in to {name} again. Its new login is the one in use now."),',
    '(true, Some(app)) => format!("Signed in to {name} again. {app} is signed in to it now."),',
    '(false, Some(app)) => format!("Added {name}, the account {app} is signed in to now."),',
    "Any {program} session started before this switch keeps using {old} until it is quit and started again.",
    'Some(tool) => format!("Add a second {tool} account"),',
  ])
    assert.ok(words.includes(said), said);
  assert.ok(present("notices.rs").includes(`opens: Some("${w.CLAUDE_OPENS}".into())`));
  const setup = joined(present("setup.rs"));
  assert.ok(
    setup.includes(
      '"{label} is the only {tool}account Pitboard knows, so there’s nothing to switch to. {adding}"',
    ),
  );
  assert.ok(setup.includes('"Adding another signs in to it and parks its login beside this one."'));
  assert.ok(
    setup.includes('"Adding another parks this one while you sign in to the other in {}."'),
  );
  assert.ok(setup.includes('words::titled(seen.os, "Add Account…", "Add account…")'));
  assert.ok(setup.includes('words::titled(seen.os, "Not Now", "Not now")'));
  assert.equal(
    w.onlyOneDetail("work", "claude", "Claude Code"),
    "work is the only Claude Code account Pitboard knows, so there’s nothing to switch to. Adding another signs in to it and parks its login beside this one.",
  );
  assert.ok(present("mod.rs").includes('Severity::Info => "Note",'));
});

test("Activity names each change and its end as present/words.rs does", () => {
  const arms = (fn: string) => {
    const body = words.slice(words.indexOf(`fn ${fn}(`)).split("\n}\n")[0];
    return [...body.matchAll(/((?:"[^"]+"\s*\|\s*)*"[^"]+")\s*=>\s*"([^"]+)"\.into\(\)/g)].flatMap(
      ([, codes, said]) => [...codes.matchAll(/"([^"]+)"/g)].map(([, code]) => [code, said]),
    );
  };
  const verbs = arms("change_verb");
  const outcomes = arms("change_outcome");
  assert.ok(verbs.length > 15 && outcomes.length > 5);
  for (const [code, said] of verbs) assert.equal(changeVerb(code), said, code);
  for (const [code, said] of outcomes) assert.equal(changeOutcome(code), said, code);
});
