import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";
import * as w from "./window.ts";
import { account, moments } from "./world.ts";
import { changeOutcome, changeVerb } from "../lib/words.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const present = (file: string) => repo(`crates/pitboard-model/src/present/${file}`);
// A Rust string literal as it reads once its line continuations are joined.
const joined = (source: string) => source.replace(/\\\n\s*/g, "");
// present/words/, a file for each surface that says the words, read as one
const words = joined(
  readdirSync(new URL("../../../crates/pitboard-model/src/present/words/", import.meta.url))
    .sort()
    .map((file) => present(`words/${file}`))
    .join("\n"),
);
const appWords = joined(present("app_words.rs"));
// the words a test of present/words/ pins, as a string literal of it
const pinned = (said: string) => words.includes(`"${said}"`);
const after = moments.after;

test("a name says its tool once two tools have accounts, on each system's list", () => {
  assert.ok(words.includes('format!("{label} ({tool})")'));
  assert.equal(w.nameOf(account("work"), after, "macos"), "work (Claude Code)");
  assert.equal(w.nameOf(account("main", "codex"), after, "windows"), "main (Codex)");
  const one = { ...after, accounts: after.accounts.filter((a) => a.tool === "claude") };
  assert.equal(w.nameOf(account("work"), one, "macos"), "work");
});

test("the window's toolbar, rows and offers say what the apps say", () => {
  const pane = repo("apps/macos/Sources/PitboardApp/Window/AccountsWindow.swift");
  const page = repo("apps/windows/Pitboard.Views/AccountsPage.xaml");
  for (const [word, made, field, help] of [
    ["refreshHelp", "refresh_help", "RefreshHelp", w.REFRESH_HELP],
    ["addAccountHelp", "add_account_help", "AddAccountHelp", w.ADD_HELP],
  ]) {
    assert.ok(pane.includes(`.help(words.window.${word})`), help);
    assert.ok(appWords.includes(`${made}: "${help}".into(),`), help);
    // the Windows page binds the model's own words
    assert.ok(page.includes(`ToolTipService.ToolTip="{x:Bind shown:Words.Window.${field}}"`), help);
  }
  assert.ok(words.includes('titled(os, "In Use", IN_USE)'));
  assert.equal(w.inUse("windows"), "In use");
  assert.ok(words.includes(`pub(crate) const SWITCH: &str = "${w.SWITCH}";`));
  assert.ok(words.includes('format!("Switch to {label}")'));
  assert.equal(w.switchOffer("personal"), "Switch to personal");
  assert.ok(words.includes(`if asks { "${w.RENAME_OFFER}" } else { "Rename" }`));
  assert.ok(words.includes('titled(os, "Remove Account…", "Remove account…")'));
  assert.ok(words.includes('titled(os, "Sign In Again…", "Sign in again…")'));
  assert.ok(words.includes('titled(os, "Warm Up Now", "Warm up now")'));
  assert.ok(
    repo("apps/macos/Sources/PitboardApp/Window/AccountCommands.swift").includes(
      "Button(words.menu.copyEmail)",
    ),
  );
  assert.ok(appWords.includes(`copy_email: titled(os, "${w.copyEmail("macos")}", `));
  assert.ok(
    repo("apps/windows/Pitboard.Views/AccountMenu.cs").includes(
      'Text = Words.Menu.CopyEmail, AccessKey = "C", KeyboardAcceleratorTextOverride = "Ctrl+C"',
    ),
  );
  assert.ok(appWords.includes(`titled(os, "Copy Email Address", "${w.copyEmail("windows")}")`));
});

test("Remove Account asks with the account's name and says what goes, on this Mac or this PC", () => {
  const accounts = present("accounts.rs");
  assert.ok(accounts.includes("title: words::remove_title(spoken_name),"));
  assert.ok(
    accounts.includes(
      "confirm: words::REMOVE.into(),\n            destructive: true,\n            confirm_is_default: true,",
    ),
  );
  assert.ok(words.includes('format!("Remove “{name}”?")'));
  assert.ok(words.includes(`pub(crate) const REMOVE: &str = "${w.REMOVE}";`));
  assert.equal(w.removeTitle(account("work"), after, "macos"), "Remove “work (Claude Code)”?");
  const windows = joined(repo("crates/pitboard-model/src/account_windows/accounts.rs"));
  assert.ok(
    windows.includes(
      "Pitboard deletes its saved sign-in and what its {} window keeps on {}, so using it again needs a sign-in in your browser.",
    ),
  );
  assert.ok(
    windows.includes(
      "Pitboard deletes its saved sign-in and the folders it kept for it, so using it again needs a sign-in in {app}.",
    ),
  );
  const machines = repo("crates/pitboard-model/src/account_windows/mod.rs");
  assert.ok(
    machines.includes('Os::MacOs => "this Mac"') && machines.includes('Os::Windows => "this PC"'),
  );
  assert.match(
    w.removeMessage(account("main", "codex"), "windows"),
    /its chatgpt\.com window keeps on this PC, so/,
  );
  assert.match(w.removeMessage(account("office", "desktop"), "macos"), /sign-in in Claude\.$/);
});

test("the Rename sheet and the one naming an account added say what present/sheets.rs gives them", () => {
  const sheets = present("sheets.rs");
  assert.ok(sheets.includes("title: words::rename_title(&account.label),"));
  assert.ok(sheets.includes("title: words::name_this_account(seen.os),"));
  assert.ok(sheets.includes("message: words::naming(email, tool.name()),"));
  assert.ok(sheets.includes("confirm: words::DONE.into(),"));
  assert.ok(pinned(w.renameTitle("work")));
  assert.ok(pinned(w.RENAMING));
  assert.ok(pinned(w.naming("a@example.com", "Codex")));
  assert.ok(pinned(w.naming("", "Claude Desktop")));
  assert.ok(words.includes(`pub(crate) const DONE: &str = "${w.DONE}";`));
  assert.ok(words.includes(`pub(crate) const NAME_PROMPT: &str = "${w.NAME_PROMPT}";`));
  assert.ok(words.includes('titled(os, "Name This Account", "Name this account")'));
  assert.equal(w.nameThisAccount("windows"), "Name this account");
  assert.ok(words.includes('format!("Couldn’t rename {label}")'));
  for (const problem of ["empty", "not_one_word", "separator"] as const)
    assert.ok(pinned(w.unusableName(problem)), problem);
  assert.ok(words.includes('"{} is already the name of {who}. Type another name.",'));
  assert.ok(pinned(w.nameTaken("work", "dana@work.example")));
});

test("the sign-in sheets and the wait say what the app says, for a browser and for Claude", () => {
  for (const said of [
    w.signInMessage("Codex", null),
    w.signInMessage("Codex", "work"),
    w.finishInBrowser("work"),
    w.finishInBrowser(null),
    w.signsOutFirst("Claude", "work", null),
    w.signsOutFirst("Claude", "work", "work"),
    w.inAppNote("Claude"),
    ...w.inAppSteps("Claude", null),
    w.waitingToQuit("Claude"),
    w.WAITING_FOR_BROWSER,
    w.LATER,
    w.CONTINUE,
  ])
    assert.ok(pinned(said), said);
  assert.deepEqual(w.signingOut("work"), ["Saving work.", "Saving work"]);
  assert.ok(words.includes('(format!("Saving {label}."), format!("Saving {label}"))'));
  for (const said of [
    'titled(os, "Add Account", "Add account")',
    'titled(os, "Sign In…", "Sign in…")',
    '&format!("Sign In to {label} Again{more}"),',
    '&format!("Signing In to {tool}"),',
    '&format!("Switch Back to {label}"),',
    '.then(|| format!("Open {app}."))',
    'format!("Sign in to {who}.")',
    'format!("Once it shows your chats, quit {app} with Command-Q, and {adds}."),',
    '"the account you’re adding".to_owned(),',
    '"Pitboard adds it".to_owned(),',
    'format!("Pitboard saves it for {label}"),',
    '"Don’t choose Log out in {app}: it ends the sign-in at claude.ai, and Pitboard can’t bring it back."',
    'format!("Pitboard {saves} and signs {app} out first, without signing out at claude.ai.")',
  ])
    assert.ok(words.includes(said), said);
  assert.deepEqual(w.inAppSteps("Claude", "home"), [
    "Open Claude.",
    "Sign in to home again.",
    "Once it shows your chats, quit Claude with Command-Q, and Pitboard saves it for home.",
  ]);
  const sheet = repo("apps/macos/Sources/PitboardApp/Sheets/SignInSheet.swift");
  assert.ok(sheet.includes("LabeledContent(words.sheet.signInPage)"));
  assert.ok(appWords.includes(`sign_in_page: titled(os, "${w.signInPage("macos")}", `));
  assert.ok(
    sheet.includes("Button(words.sheet.copyLink)") && sheet.includes("Button(words.sheet.open)"),
  );
  assert.ok(appWords.includes(`copy_link: titled(os, "${w.copyLink("macos")}", `));
  assert.ok(appWords.includes("open: words::OPEN.into(),"));
  assert.ok(words.includes(`pub(crate) const OPEN: &str = "${w.OPEN}";`));
  assert.ok(sheet.includes("Picker(words.sheet.tool, selection: $tool)"));
  assert.ok(appWords.includes(`tool: "${w.TOOL}".into(),`));
  assert.ok(sheet.includes("LabeledContent(words.sheet.account, value: account)"));
  // the sheet that adds an account asks for no name: one is picked once the sign-in adds it
  assert.ok(!sheet.includes("TextField(words.sheet.name"));
  assert.ok(present("sheets.rs").includes("Sheet::Add { .. } => sends(Some(Intent::SignIn {"));
});

test("a switch's notice and the setup step say what present/notices.rs and setup.rs give them", () => {
  for (const said of [
    w.switchedTo(null, "work"),
    w.switchedTo("Claude Code", "work"),
    w.sessionsFollowIn(null),
    w.sessionsFollowIn("Claude Code"),
    w.restartNotice("codex", "personal"),
    w.signedInTo(null, "work"),
    w.signedInTo("Codex", "work"),
    w.signedInNow("work", true, null),
    w.signedInNow("work", false, null),
    w.onlyOneDetail("work", "claude", null),
    w.onlyOneDetail("work", "desktop", "Claude Desktop"),
  ])
    assert.ok(pinned(said), said);
  for (const said of [
    '(true, Some(app)) => format!("Signed in to {name} again. {app} is signed in to it now."),',
    '(false, Some(app)) => format!("Added {name}, the account {app} is signed in to now."),',
    'Some(tool) => format!("Add a second {tool} account"),',
    'Adoption::NextOpen { app } => format!("{app} opens signed in to it."),',
    'titled(os, "Switch Back", "Switch back")',
  ])
    assert.ok(words.includes(said), said);
  assert.equal(w.CLAUDE_OPENS, "Claude opens signed in to it.");
  const notices = present("notices.rs");
  assert.ok(notices.includes("title: words::switch_back(seen.os),"));
  assert.ok(notices.includes("words::signed_in_to(tool, label)"));
  const setup = present("setup.rs");
  assert.ok(setup.includes("detail: words::only_one(&account.label, named, app),"));
  assert.ok(setup.includes("title: words::add_account(seen.os, true),"));
  assert.ok(setup.includes("title: words::not_now(seen.os),"));
  assert.ok(words.includes('titled(os, "Not Now", "Not now")'));
  assert.ok(notices.includes("spoken_severity: words::spoken_severity(severity).into(),"));
  assert.ok(words.includes('Severity::Info => "Note",'));
});

test("a switch Pitboard made by itself offers Switch Back on its notice, while the account it left is the one to go back to", () => {
  const last = { from: "personal", to: "work", at: 0, followsAt: null };
  const s = { ...after, now: 0, lastSwitch: { claude: last }, newLogin: {} };
  assert.equal(w.noticeOf(s, "claude", "macos")?.back ?? null, null);
  const back = { ...s, lastSwitch: { claude: { ...last, back: { resetsAt: null } } } };
  assert.equal(w.noticeOf(back, "claude", "macos")?.back, "Switch Back");
  assert.equal(w.noticeOf(back, "claude", "windows")?.back, "Switch back");
  const notices = present("notices.rs");
  assert.ok(notices.includes("let back = last.back.as_ref().map(|back| NoticeAction {"));
  // a bare one stays while it offers Switch Back
  assert.ok(notices.includes("|| notice.actions.iter().any(|action| action.switches)"));
});

test("Activity names each change and its end as present/words/machine.rs does", () => {
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
  // an end the app says as it says the account's standing
  const standing = words.slice(words.indexOf("fn sign_in_standing("));
  for (const [code, of] of [
    ["parked_login_expired", "LoginExpired"],
    ["nothing_parked", "NothingParked"],
  ] as const) {
    assert.ok(words.includes(`"${code}" => sign_in_standing(Standing::${of}).into(),`), code);
    assert.ok(standing.includes(`Standing::${of} => "${changeOutcome(code)}",`), code);
  }
  assert.ok(
    words.includes('"parked_login_refused" => sign_in_standing(Standing::LoginRefused).into(),'),
  );
  assert.ok(standing.includes(`_ => "${changeOutcome("parked_login_refused")}",`));
});
