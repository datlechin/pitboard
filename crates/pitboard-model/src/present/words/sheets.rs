//! What the sheets and the window's questions say: signing in, in a browser or inside an app,
//! naming and renaming an account, cleaning up a leftover sign-in file, and quitting an app to
//! go on.

use super::titled;
use crate::Front;
use crate::model::state::Recording;
use pitboard_core::label::Unusable;

// What the sheet says under the steps while Pitboard waits for `app` to quit signed in, as the
// wait last said.
pub(crate) fn in_app_note(app: &str, step: Option<pitboard_core::switch::WaitStep>) -> String {
    use pitboard_core::switch::WaitStep;
    match step {
        Some(WaitStep::QuitSignedOut) => {
            format!("{app} quit with nobody signed in. Open it, sign in, then quit it again.")
        }
        Some(WaitStep::QuitUnfinished) => format!(
            "{app} quit before it finished signing in. Open it, wait for your chats, then quit it \
             again."
        ),
        Some(WaitStep::Waiting | WaitStep::Open) | None => format!(
            "Don’t choose Log out in {app}: it ends the sign-in at claude.ai, and Pitboard can’t \
             bring it back."
        ),
    }
}

// What a sign-in inside `app` asks of a person, `again` for an account added already; `opened`
// where Pitboard opened `app` for it.
pub(crate) fn in_app_steps(app: &str, again: Option<&str>, opened: bool) -> Vec<String> {
    let (who, adds) = match again {
        Some(label) => (
            format!("{label} again"),
            format!("Pitboard saves it for {label}"),
        ),
        None => (
            "the account you’re adding".to_owned(),
            "Pitboard adds it".to_owned(),
        ),
    };
    let sign_in = if opened {
        format!("In {app}, sign in to {who}.")
    } else {
        format!("Sign in to {who}.")
    };
    (!opened)
        .then(|| format!("Open {app}."))
        .into_iter()
        .chain([
            sign_in,
            format!("Once it shows your chats, quit {app} with Command-Q, and {adds}."),
        ])
        .collect()
}

pub(crate) fn sign_in_message(tool: &str, again: Option<&str>) -> String {
    match again {
        None => format!(
            "Pitboard opens {tool}’s sign-in in your browser, where you sign in to the account \
             you’re adding."
        ),
        Some(label) => format!(
            "Pitboard opens {tool}’s sign-in in your browser, where you sign in to {label} again."
        ),
    }
}

pub(crate) fn add_account(front: Front, asks: bool) -> String {
    if asks {
        titled(front, "Add Account…", "Add account…")
    } else {
        titled(front, "Add Account", "Add account")
    }
}

pub(crate) fn sign_in_to_again(front: Front, label: &str, asks: bool) -> String {
    let more = if asks { "…" } else { "" };
    titled(
        front,
        &format!("Sign In to {label} Again{more}"),
        &format!("Sign in to {label} again{more}"),
    )
}

pub(crate) fn add_as(label: &str) -> String {
    format!("Add {label}")
}

pub(crate) fn add_shown(app: &str, label: &str) -> String {
    format!(
        "Open {app} and check that it shows {label}, then quit it. Pitboard saves {app}’s \
         sign-in for {label}."
    )
}

// What Pitboard does before a sign-in inside `app`, `work` signed in to it: it keeps the sign-in
// of `again` as it is.
pub(crate) fn signs_out_first(app: &str, work: &str, again: Option<&str>) -> String {
    let saves = if again == Some(work) {
        format!("keeps {work}’s sign-in as it is")
    } else {
        format!("saves {work}")
    };
    format!("Pitboard {saves} and signs {app} out first, without signing out at claude.ai.")
}

pub(crate) fn add_shown_first(app: &str) -> String {
    format!(
        "Pitboard adds the account {app} is signed in to. Check which account {app} shows \
         first: Pitboard can’t ask claude.ai whose it is."
    )
}

pub(crate) fn add_once_signed_in(app: &str) -> String {
    format!(
        "{app} is signed out so you can sign in to the account you’re adding. Sign in to it in \
         {app}, quit {app}, then add it here."
    )
}

pub(crate) fn add_unidentified(app: &str) -> String {
    format!(
        "Pitboard can’t tell which account {app} is signed in to. Open {app}, check which \
         account it shows, and quit it. Pitboard then adds it, or saves it for the account it is."
    )
}

// What the sheet says under its fields while it records the account `app` shows.
pub(crate) fn recording(app: &str, step: Recording) -> String {
    match step {
        Recording::Quitting => format!("Quitting {app}…"),
        Recording::Reading => format!("Reading {app}’s account…"),
    }
}

pub(crate) const WAITING_FOR_BROWSER: &str = "Waiting for your browser";

// Said in place of Cancel where the sign-in goes on, an account parked for it.
pub(crate) const LATER: &str = "Later";

pub(crate) fn signing_in_to(front: Front, tool: &str) -> String {
    titled(
        front,
        &format!("Signing In to {tool}"),
        &format!("Signing in to {tool}"),
    )
}

pub(crate) fn finish_in_browser(again: Option<&str>) -> String {
    match again {
        Some(label) => format!("Finish signing in as {label} in your browser."),
        None => "Finish signing in in your browser.".to_owned(),
    }
}

pub(crate) fn code_note(tool: &str) -> String {
    format!(
        "{tool} takes the code shown after you sign in, whether or not your browser came back \
         to it."
    )
}

// The owner approved this sentence as written on 6 October 2026. The owner decided that a
// code may be pasted again, and the Swift app, which never asked again, had nothing to say here.
pub(crate) fn code_refused(tool: &str) -> String {
    format!(
        "{tool} didn’t take that code. Copy the whole code your browser shows, and paste it \
         again."
    )
}

// A sign-in inside `app` under way, before it waits: its message, then what its spinner says.

pub(crate) fn quitting(app: &str) -> (String, String) {
    (
        format!("Pitboard is quitting {app}. {app} may ask about work in progress first."),
        format!("Quitting {app}"),
    )
}

pub(crate) fn signing_out(label: &str) -> (String, String) {
    (format!("Saving {label}."), format!("Saving {label}"))
}

pub(crate) const GETTING_READY: (&str, &str) = ("Getting ready.", "Starting");

pub(crate) fn waiting_to_quit(app: &str) -> String {
    format!("Waiting for {app} to quit")
}

// A question, in sentence case on both systems.
pub(crate) fn replace_title(tool: &str) -> String {
    format!("Replace {tool}’s sign-in?")
}

pub(crate) const REPLACE: &str = "Replace";

// What the name field shows while it is empty.
pub(crate) const NAME_PROMPT: &str = "work";

pub(crate) fn name_this_account(front: Front) -> String {
    titled(front, "Name This Account", "Name this account")
}

// The account the sign-in or the one signed in now added, by its email where it has one.
pub(crate) fn naming(email: &str, tool: &str) -> String {
    if email.is_empty() {
        format!("The account {tool} is signed in to is added.")
    } else {
        format!("{email} is added.")
    }
}

// Why what was typed cannot be a new name, where something was.
pub(crate) fn unusable_name(unusable: Unusable) -> String {
    match unusable {
        Unusable::Empty => "Type a name for this account.",
        Unusable::NotOneWord => "A name is one word, with no spaces.",
        Unusable::Separator => "A name can’t contain “/”.",
    }
    .into()
}

pub(crate) const DONE: &str = "Done";

pub(crate) fn rename_title(label: &str) -> String {
    format!("Rename “{label}”")
}

pub(crate) const RENAMING: &str = "Only its name changes, here and in the command line.";

pub(crate) fn clean_up(front: Front, asks: bool) -> String {
    if asks {
        titled(front, "Clean Up…", "Clean up…")
    } else {
        titled(front, "Clean Up", "Clean up")
    }
}

pub(crate) const FINDING_OUT_WHOSE: &str = "Finding out whose sign-in it is…";

pub(crate) fn clean_up_title(front: Front) -> String {
    titled(
        front,
        "Clean Up the Leftover Sign-In File",
        "Clean up the leftover sign-in file",
    )
}

// What the Clean Up sheet says until it has looked.
pub(crate) fn clean_up_message(front: Front) -> String {
    format!(
        "{} deletes the sign-in file Claude Code left behind {}, and keeps any sign-in an \
         account needs.",
        clean_up(front, false),
        pitboard_core::words::store_in_front(front.os())
    )
}

// The Clean Up sheet's one sentence once it has looked: what the file holds, and what Clean Up does.
pub(crate) fn clean_up_found(front: Front, left: &pitboard_core::switch::Left) -> String {
    use pitboard_core::switch::{Foreseen, Kept};
    let button = clean_up(front, false);
    let only = format!("so {button} only deletes");
    match &left.login {
        Foreseen::Kept(Kept::NoLogin) => format!("The file holds no sign-in, {only} it."),
        Foreseen::Kept(Kept::Refused) => {
            format!("The file holds a sign-in that no longer works, {only} it.")
        }
        Foreseen::Kept(Kept::Stored { label: Some(label) }) => format!(
            "The file holds {label}’s sign-in, which Claude Code keeps too, {only} the file."
        ),
        Foreseen::Kept(Kept::Stored { label: None }) => {
            format!("The file holds the sign-in Claude Code keeps too, {only} the file.")
        }
        Foreseen::Kept(Kept::AlreadyParked { label }) => {
            format!("The file holds {label}’s saved sign-in, {only} the file.")
        }
        Foreseen::Kept(Kept::SecondSignIn { label }) => format!(
            "The file holds a second sign-in of {label}, the account in use, which {button} \
             deletes with the file."
        ),
        Foreseen::Kept(Kept::ParkedNow { label }) => format!(
            "The file holds {label}’s sign-in, which {button} saves for {label} before it deletes \
             the file."
        ),
        Foreseen::Kept(Kept::ParkKept { label }) => format!(
            "The file holds another sign-in of {label}, which {button} deletes with the file."
        ),
        Foreseen::NotEnrolled(owner) => format!(
            "The file holds {}’s sign-in, an account you haven’t added, so add it before you \
             clean up.",
            owner.email
        ),
        Foreseen::Untold => format!(
            "The file’s sign-in has expired, so {button} renews it first and goes on only for an \
             account you’ve added."
        ),
        _ => clean_up_message(front),
    }
}

pub(crate) fn nothing_to_clean_up(front: Front) -> String {
    format!(
        "No sign-in file is left behind {}, so there’s nothing to clean up.",
        pitboard_core::words::store_in_front(front.os())
    )
}

pub(crate) fn quit_to_switch(name: &str) -> String {
    format!("Quit {name} to switch?")
}

// `again`: the account signed in to again, where it is not a new one.
pub(crate) fn quit_to_sign_in(name: &str, again: Option<&str>) -> String {
    match again {
        Some(label) => format!("Quit {name} to sign in to {label} again?"),
        None => format!("Quit {name} to add an account?"),
    }
}

pub(crate) fn quit_to_uninstall(name: &str) -> String {
    format!("Quit {name} to uninstall Pitboard?")
}

pub(crate) fn quit_to_delete(name: &str) -> String {
    format!(
        "{} It quits {name}, uninstalls, and opens {name} again.",
        only_while_closed(name, &format!("deletes the sign-ins it saved for {name}"))
    )
}

pub(crate) fn quit_and_uninstall(front: Front, name: &str) -> String {
    quit_and(front, name, "Uninstall", "uninstall")
}

// `label` was added already, and gets a new sign-in.
pub(crate) fn quit_to_add(name: &str, label: &str) -> String {
    format!("Quit {name} to add {label}?")
}

// For an app that keeps no login Pitboard moves, which takes a switch once it starts again.
pub(crate) fn quit_and_reopen(name: &str) -> String {
    format!(
        "{name} keeps using the account it started with until it quits. Pitboard quits it, \
         switches, and opens it again."
    )
}

fn only_while_closed(name: &str, does: &str) -> String {
    format!("Pitboard {does} only while {name} is closed.")
}

pub(crate) fn quit_to_move(name: &str, label: &str) -> String {
    format!(
        "{} It quits {name}, switches to {label}, and opens it again.",
        only_while_closed(name, &format!("switches {name}"))
    )
}

pub(crate) fn quit_to_sign_out(name: &str, work: &str) -> String {
    format!(
        "{} It quits {name}, saves {work}, and opens {name} again for you to sign in.",
        only_while_closed(name, &format!("signs {name} out"))
    )
}

pub(crate) fn quit_to_wait(name: &str) -> String {
    format!(
        "{} It quits {name} and opens it again for you to sign in.",
        only_while_closed(name, "starts waiting for a sign-in")
    )
}

// `again`: the account its sign-in is saved for, or none for a new one.
pub(crate) fn quit_to_read(name: &str, again: Option<&str>) -> String {
    let adds = match again {
        Some(label) => format!("saves that sign-in for {label}"),
        None => "adds that account".to_owned(),
    };
    format!(
        "{} It quits {name}, {adds}, and opens it again.",
        only_while_closed(name, &format!("reads which account {name} is signed in to"))
    )
}

// What a quit question's button says: "Quit Claude and Switch".
fn quit_and(front: Front, name: &str, then: &str, then_sentence: &str) -> String {
    titled(
        front,
        &format!("Quit {name} and {then}"),
        &format!("Quit {name} and {then_sentence}"),
    )
}

pub(crate) fn quit_and_switch(front: Front, name: &str) -> String {
    quit_and(front, name, "Switch", "switch")
}

pub(crate) fn quit_and_continue(front: Front, name: &str) -> String {
    quit_and(front, name, "Continue", "continue")
}

pub(crate) fn quit_and_start_sign_in(front: Front, name: &str) -> String {
    quit_and(front, name, "Start Sign-In", "start sign-in")
}

pub(crate) fn quit_and_add(front: Front, name: &str) -> String {
    quit_and(front, name, "Add", "add")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::present::words::{CANCEL, CONTINUE, cleaned_up, sign_in_asking};

    // A sheet's words, by what it does, pinned once: the sheets' and the model's tests compare
    // with these functions.
    #[test]
    fn a_sheet_says_what_it_does_in_these_words() {
        assert_eq!(
            sign_in_message("Codex", None),
            "Pitboard opens Codex’s sign-in in your browser, where you sign in to the account \
             you’re adding."
        );
        assert_eq!(
            sign_in_message("Claude Code", None),
            "Pitboard opens Claude Code’s sign-in in your browser, where you sign in to the \
             account you’re adding."
        );
        assert_eq!(
            sign_in_message("Codex", Some("work")),
            "Pitboard opens Codex’s sign-in in your browser, where you sign in to work again."
        );
        assert_eq!(
            add_shown("Claude", "home"),
            "Open Claude and check that it shows home, then quit it. Pitboard saves Claude’s \
             sign-in for home."
        );
        assert_eq!(
            signs_out_first("Claude", "work", Some("home")),
            "Pitboard saves work and signs Claude out first, without signing out at claude.ai."
        );
        assert_eq!(
            signs_out_first("Claude", "work", Some("work")),
            "Pitboard keeps work’s sign-in as it is and signs Claude out first, without signing \
             out at claude.ai."
        );
        assert_eq!(
            signs_out_first("Claude", "work", None),
            signs_out_first("Claude", "work", Some("home"))
        );
        assert_eq!(
            in_app_steps("Claude", None, false),
            [
                "Open Claude.",
                "Sign in to the account you’re adding.",
                "Once it shows your chats, quit Claude with Command-Q, and Pitboard adds it.",
            ]
        );
        assert_eq!(
            in_app_steps("Claude", Some("home"), true),
            [
                "In Claude, sign in to home again.",
                "Once it shows your chats, quit Claude with Command-Q, and Pitboard saves it for \
                 home.",
            ],
            "Pitboard opened Claude for it"
        );
        assert_eq!(
            add_shown_first("Claude"),
            "Pitboard adds the account Claude is signed in to. Check which account Claude shows \
             first: Pitboard can’t ask claude.ai whose it is."
        );
        assert_eq!(
            add_once_signed_in("Claude"),
            "Claude is signed out so you can sign in to the account you’re adding. Sign in to it \
             in Claude, quit Claude, then add it here."
        );
        assert_eq!(
            add_unidentified("Claude"),
            "Pitboard can’t tell which account Claude is signed in to. Open Claude, check which \
             account it shows, and quit it. Pitboard then adds it, or saves it for the account \
             it is."
        );
        assert_eq!(CONTINUE, "Continue");
        assert_eq!(add_as("home"), "Add home");
        assert_eq!(recording("Claude", Recording::Quitting), "Quitting Claude…");
        assert_eq!(
            recording("Claude", Recording::Reading),
            "Reading Claude’s account…"
        );
        assert_eq!(naming("a@example.com", "Codex"), "a@example.com is added.");
        assert_eq!(
            naming("", "Claude Desktop"),
            "The account Claude Desktop is signed in to is added."
        );
        assert_eq!(rename_title("work"), "Rename “work”");
        assert_eq!(
            RENAMING,
            "Only its name changes, here and in the command line."
        );
        assert_eq!(
            clean_up_message(Front::WindowsNotificationArea),
            "Clean up deletes the sign-in file Claude Code left behind Credential Manager, and \
             keeps any sign-in an account needs."
        );
        assert_eq!(
            finish_in_browser(Some("work")),
            "Finish signing in as work in your browser."
        );
        assert_eq!(
            finish_in_browser(None),
            "Finish signing in in your browser."
        );
        assert_eq!(
            code_note("Claude Code"),
            "Claude Code takes the code shown after you sign in, whether or not your browser \
             came back to it."
        );
        assert_eq!(
            code_note("Codex"),
            "Codex takes the code shown after you sign in, whether or not your browser came back \
             to it."
        );
        assert_eq!(
            code_refused("Claude Code"),
            "Claude Code didn’t take that code. Copy the whole code your browser shows, and \
             paste it again."
        );
        assert_eq!(
            quitting("Claude"),
            (
                "Pitboard is quitting Claude. Claude may ask about work in progress first.".into(),
                "Quitting Claude".into()
            )
        );
        assert_eq!(
            signing_out("work"),
            ("Saving work.".into(), "Saving work".into())
        );
        assert_eq!(waiting_to_quit("Claude"), "Waiting for Claude to quit");
        assert_eq!(NAME_PROMPT, "work");
        assert_eq!(DONE, "Done");
        assert_eq!(FINDING_OUT_WHOSE, "Finding out whose sign-in it is…");
        assert_eq!(WAITING_FOR_BROWSER, "Waiting for your browser");
        assert_eq!(CANCEL, "Cancel");
        assert_eq!(LATER, "Later");
        assert_eq!(GETTING_READY, ("Getting ready.", "Starting"));
        assert_eq!(REPLACE, "Replace");
        assert_eq!(
            replace_title("Claude Code"),
            "Replace Claude Code’s sign-in?"
        );
    }

    #[test]
    fn a_name_that_cannot_be_one_says_why_in_these_words() {
        assert_eq!(
            unusable_name(Unusable::Empty),
            "Type a name for this account."
        );
        assert_eq!(
            unusable_name(Unusable::NotOneWord),
            "A name is one word, with no spaces."
        );
        assert_eq!(
            unusable_name(Unusable::Separator),
            "A name can’t contain “/”."
        );
    }

    // A sheet's titles and buttons, as each system titles them.
    #[test]
    fn a_sheet_is_titled_as_each_system_titles_one() {
        let both = |said: &dyn Fn(Front) -> String| {
            (
                said(Front::MacMenuBar),
                said(Front::WindowsNotificationArea),
            )
        };
        let pair = |mac: &str, windows: &str| (mac.to_owned(), windows.to_owned());
        assert_eq!(both(&sign_in_asking), pair("Sign In…", "Sign in…"));
        assert_eq!(
            both(&|front| add_account(front, false)),
            pair("Add Account", "Add account")
        );
        assert_eq!(
            both(&|front| sign_in_to_again(front, "work", false)),
            pair("Sign In to work Again", "Sign in to work again")
        );
        assert_eq!(
            both(&|front| sign_in_to_again(front, "work", true)),
            pair("Sign In to work Again…", "Sign in to work again…")
        );
        assert_eq!(
            both(&name_this_account),
            pair("Name This Account", "Name this account")
        );
        assert_eq!(
            both(&clean_up_title),
            pair(
                "Clean Up the Leftover Sign-In File",
                "Clean up the leftover sign-in file"
            )
        );
        assert_eq!(
            both(&|front| signing_in_to(front, "Codex")),
            pair("Signing In to Codex", "Signing in to Codex")
        );
    }

    #[test]
    fn a_quit_question_says_what_the_quit_is_for() {
        assert_eq!(quit_to_switch("Claude"), "Quit Claude to switch?");
        assert_eq!(
            quit_to_sign_in("Claude", None),
            "Quit Claude to add an account?"
        );
        assert_eq!(
            quit_to_sign_in("Claude", Some("home")),
            "Quit Claude to sign in to home again?"
        );
        assert_eq!(quit_to_add("Claude", "home"), "Quit Claude to add home?");
        assert_eq!(
            quit_and_reopen("ChatGPT"),
            "ChatGPT keeps using the account it started with until it quits. Pitboard quits it, \
             switches, and opens it again."
        );
        assert_eq!(
            quit_to_move("Claude", "home"),
            "Pitboard switches Claude only while Claude is closed. It quits Claude, switches to \
             home, and opens it again."
        );
        assert_eq!(
            quit_to_sign_out("Claude", "work"),
            "Pitboard signs Claude out only while Claude is closed. It quits Claude, saves work, \
             and opens Claude again for you to sign in."
        );
        assert_eq!(
            quit_to_wait("Claude"),
            "Pitboard starts waiting for a sign-in only while Claude is closed. It quits Claude \
             and opens it again for you to sign in."
        );
        assert_eq!(
            quit_to_read("Claude", Some("home")),
            "Pitboard reads which account Claude is signed in to only while Claude is closed. It \
             quits Claude, saves that sign-in for home, and opens it again."
        );
        assert_eq!(
            quit_to_read("Claude", None),
            "Pitboard reads which account Claude is signed in to only while Claude is closed. It \
             quits Claude, adds that account, and opens it again."
        );
        let both = |said: &dyn Fn(Front) -> String| {
            (
                said(Front::MacMenuBar),
                said(Front::WindowsNotificationArea),
            )
        };
        let pair = |mac: &str, windows: &str| (mac.to_owned(), windows.to_owned());
        assert_eq!(
            both(&|front| quit_and_switch(front, "Claude")),
            pair("Quit Claude and Switch", "Quit Claude and switch")
        );
        assert_eq!(
            both(&|front| quit_and_continue(front, "Claude")),
            pair("Quit Claude and Continue", "Quit Claude and continue")
        );
        assert_eq!(
            both(&|front| quit_and_start_sign_in(front, "Claude")),
            pair(
                "Quit Claude and Start Sign-In",
                "Quit Claude and start sign-in"
            )
        );
        assert_eq!(
            both(&|front| quit_and_add(front, "Claude")),
            pair("Quit Claude and Add", "Quit Claude and add")
        );
        assert_eq!(
            quit_to_uninstall("Claude"),
            "Quit Claude to uninstall Pitboard?"
        );
        assert_eq!(
            quit_to_delete("Claude"),
            "Pitboard deletes the sign-ins it saved for Claude only while Claude is closed. It \
             quits Claude, uninstalls, and opens Claude again."
        );
        assert_eq!(
            both(&|front| quit_and_uninstall(front, "Claude")),
            pair("Quit Claude and Uninstall", "Quit Claude and uninstall")
        );
    }

    #[test]
    fn the_wait_is_said_step_by_step() {
        use pitboard_core::switch::WaitStep;
        let said = |step| in_app_note("Claude", step);
        let log_out = "Don’t choose Log out in Claude: it ends the sign-in at claude.ai, and \
                       Pitboard can’t bring it back.";
        for step in [None, Some(WaitStep::Waiting), Some(WaitStep::Open)] {
            assert_eq!(said(step), log_out, "{step:?}");
        }
        assert_eq!(
            said(Some(WaitStep::QuitSignedOut)),
            "Claude quit with nobody signed in. Open it, sign in, then quit it again."
        );
        assert_eq!(
            said(Some(WaitStep::QuitUnfinished)),
            "Claude quit before it finished signing in. Open it, wait for your chats, then quit \
             it again."
        );
    }

    // Plan 4.4: the sheet and the notice after it say one sentence each.
    #[test]
    fn clean_up_says_in_one_sentence_what_it_found_and_what_it_did() {
        use pitboard_core::provider::Owner;
        use pitboard_core::switch::{Foreseen, Kept, Left, Stowed};
        let label = || "work".to_owned();
        let left = |login| Left {
            path: "/h/.claude/.credentials.json".into(),
            seen: String::new(),
            login,
            dropped: vec!["mcpOAuth".into()],
        };
        let found = [
            (
                Kept::NoLogin,
                "The file holds no sign-in, so Clean Up only deletes it.",
            ),
            (
                Kept::Refused,
                "The file holds a sign-in that no longer works, so Clean Up only deletes it.",
            ),
            (
                Kept::Stored {
                    label: Some(label()),
                },
                "The file holds work’s sign-in, which Claude Code keeps too, so Clean Up only \
                 deletes the file.",
            ),
            (
                Kept::Stored { label: None },
                "The file holds the sign-in Claude Code keeps too, so Clean Up only deletes the \
                 file.",
            ),
            (
                Kept::AlreadyParked { label: label() },
                "The file holds work’s saved sign-in, so Clean Up only deletes the file.",
            ),
            (
                Kept::SecondSignIn { label: label() },
                "The file holds a second sign-in of work, the account in use, which Clean Up \
                 deletes with the file.",
            ),
            (
                Kept::ParkedNow { label: label() },
                "The file holds work’s sign-in, which Clean Up saves for work before it deletes \
                 the file.",
            ),
            (
                Kept::ParkKept { label: label() },
                "The file holds another sign-in of work, which Clean Up deletes with the file.",
            ),
        ];
        for (kept, said) in found {
            let windows = said.replace("Clean Up", "Clean up");
            assert_eq!(
                clean_up_found(Front::MacMenuBar, &left(Foreseen::Kept(kept.clone()))),
                said
            );
            assert_eq!(
                clean_up_found(Front::WindowsNotificationArea, &left(Foreseen::Kept(kept))),
                windows
            );
        }
        assert_eq!(
            clean_up_found(
                Front::MacMenuBar,
                &left(Foreseen::NotEnrolled(Owner {
                    account_uuid: "u".into(),
                    email: "dana@home.example".into(),
                    organization_uuid: "o".into(),
                }))
            ),
            "The file holds dana@home.example’s sign-in, an account you haven’t added, so add \
             it before you clean up."
        );
        assert_eq!(
            clean_up_found(Front::MacMenuBar, &left(Foreseen::Untold)),
            "The file’s sign-in has expired, so Clean Up renews it first and goes on only for \
             an account you’ve added."
        );
        assert_eq!(
            clean_up_found(Front::WindowsNotificationArea, &left(Foreseen::Untold)),
            "The file’s sign-in has expired, so Clean up renews it first and goes on only for \
             an account you’ve added."
        );
        let did = [
            (Kept::NoLogin, "Deleted the file, which held no sign-in."),
            (
                Kept::Refused,
                "Deleted the file and the sign-in in it that no longer worked.",
            ),
            (
                Kept::Stored {
                    label: Some(label()),
                },
                "Deleted the file, and work’s sign-in stays where Claude Code keeps it.",
            ),
            (
                Kept::Stored { label: None },
                "Deleted the file, and the sign-in Claude Code keeps stays.",
            ),
            (
                Kept::AlreadyParked { label: label() },
                "Deleted the file, and work’s saved sign-in stays.",
            ),
            (
                Kept::SecondSignIn { label: label() },
                "Deleted the file and the second sign-in of work in it.",
            ),
            (
                Kept::ParkedNow { label: label() },
                "Saved work’s sign-in from the file, then deleted the file.",
            ),
            (
                Kept::ParkKept { label: label() },
                "Deleted the file, and work’s saved sign-in stays.",
            ),
        ];
        for (kept, said) in did {
            let stowed = Stowed {
                path: "/h/.claude/.credentials.json".into(),
                kept,
                dropped: Vec::new(),
            };
            assert_eq!(cleaned_up(&stowed), said);
        }
    }
}
