//! What the sheets and the window's questions say, from NameSheets.swift, SignInSheet.swift
//! and MainWindow.swift: the sheet over the window, a sign-in under way, the sheet for putting
//! away a login left in a file, the question about quitting an app to switch, and a failure's
//! alert.

use super::words;
use super::{
    Choice, Missing, NameCheck, Question, Seen, SheetText, SheetTool, SigningInText, StowText,
    alert_of,
};
use crate::Tool;
use crate::account_windows::AlertText;
use crate::model::state::{After, InAppAdd, Leftover, Recording, in_its_app, split, trimmed};
use crate::model::{Failure, Intent, QuitQuestion, ReplaceQuestion, Sheet};
use pitboard_core::label;
use pitboard_core::provider::ProviderId;
use pitboard_core::switch::{Foreseen, WaitStep};
use pitboard_core::words as said;

/// The name a sheet would save, trimmed, or why what is typed cannot be one. The model saves by
/// the same rule, and it answers at once, so a view asks it as it is typed in.
#[uniffi::export]
pub fn check_name(sheet: Sheet, typed: String) -> NameCheck {
    let saves = |name: Option<&str>| NameCheck {
        name: name.map(str::to_owned),
        problem: None,
    };
    if let Sheet::SignInAgain { label, .. } = &sheet {
        return saves(Some(label));
    }
    let name = trimmed(&typed);
    match &sheet {
        Sheet::Stow => saves(None),
        Sheet::Rename { label, .. } if label == name => saves(None),
        _ => match label::check(name) {
            Ok(()) => saves(Some(name)),
            Err(unusable) => NameCheck {
                name: None,
                problem: words::unusable_name(unusable, matches!(sheet, Sheet::Add { .. })),
            },
        },
    }
}

/// What the sheet for signing in through `tool`'s own sign-in says, for a new account or, as
/// `again`, for one signed in to again.
fn sign_in_message(tool: &str, again: Option<&str>) -> String {
    match again {
        None => format!(
            "Pitboard opens {tool}’s own sign-in in your browser. Sign in as the account \
             you’re adding, and Pitboard parks its login beside the one in use."
        ),
        Some(label) => format!(
            "Pitboard opens {tool}’s own sign-in in your browser. Sign in as {label} to give \
             Pitboard a new login for it."
        ),
    }
}

/// The tool the sheet for `sheet` starts on: the one it is about, and for a new account
/// where nothing said which, the first Pitboard found.
pub(crate) fn starts_on(seen: &Seen, sheet: &Sheet) -> String {
    match sheet {
        Sheet::Add { provider } => provider
            .clone()
            .or_else(|| {
                (seen.tools.iter())
                    .find(|tool| missing(seen, &tool.code).is_none())
                    .or(seen.tools.first())
                    .map(|tool| tool.code.clone())
            })
            .unwrap_or_else(|| ProviderId::Claude.code().to_owned()),
        Sheet::SignInAgain { provider, .. }
        | Sheet::Name { provider, .. }
        | Sheet::Rename { provider, .. } => provider.clone(),
        Sheet::Stow => ProviderId::Claude.code().to_owned(),
    }
}

// Unknown until the model has asked which tools are here, and then said only of one not found.
fn missing(seen: &Seen, tool: &str) -> Option<Missing> {
    let found = seen.state.installed.as_ref()?;
    if found.iter().any(|here| here.code == tool) {
        return None;
    }
    words::missing(seen.os, tool)
}

fn nothing_to_park(seen: &Seen, provider: &str) -> bool {
    seen.in_use_of(provider).is_none()
        || seen.state.warnings.iter().any(|warning| {
            warning.code == "looks_signed_out" && warning.provider.as_deref() == Some(provider)
        })
        || seen.state.refused_by(provider) == Some("live_credential_absent")
}

// The message and the default button of the form for a tool that signs in inside its app.
fn in_app_form(seen: &Seen, sheet: &Sheet, provider: &str) -> (String, String) {
    let app = seen.program_of(provider);
    let park = seen
        .in_use_of(provider)
        .filter(|_| !nothing_to_park(seen, provider));
    let awaiting = seen.state.status.as_ref().and_then(|status| {
        status
            .awaiting
            .iter()
            .find(|awaiting| awaiting.provider == provider)
    });
    let current = seen.state.in_app_add(provider) == InAppAdd::Current;
    let first = !seen
        .accounts()
        .iter()
        .any(|account| account.provider == provider);
    match sheet {
        Sheet::SignInAgain { label, .. } if current => (
            format!(
                "Open {app} and check that it shows {label}, then quit it. Pitboard records the \
                 account {app} is signed in to as {label}’s login."
            ),
            format!("Record as {label}"),
        ),
        Sheet::SignInAgain { label, .. } => match park {
            Some(work) if work == label => (
                format!(
                    "Pitboard parks {label}’s login as it is, which leaves {app} signed out and \
                     signs nothing out at claude.ai. Then you sign in to {label} again in {app}, \
                     and Pitboard records it once {app} quits."
                ),
                format!("Park {label}"),
            ),
            Some(work) => (
                format!(
                    "Pitboard parks {work}, which leaves {app} signed out and signs nothing out \
                     at claude.ai. Then you sign in to {label} again in {app}, and Pitboard \
                     records it once {app} quits."
                ),
                format!("Park {work}"),
            ),
            None => (
                format!(
                    "{app} is signed out. Pitboard waits while you sign in to {label} in {app}, \
                     and records it once {app} quits."
                ),
                words::titled(seen.os, "Start Sign-In", "Start sign-in"),
            ),
        },
        _ if current && first => (
            format!(
                "Pitboard records the account {app} is signed in to under this name. Check which \
                 account {app} shows first: Pitboard can’t ask claude.ai whose it is."
            ),
            words::titled(seen.os, "Add Account", "Add account"),
        ),
        _ if current => match awaiting {
            Some(awaiting) => (
                format!(
                    "{app} is signed out so you can sign in to the account you’re adding, and \
                     {} is parked. Sign in to it in {app} and quit {app}, then add it here.",
                    split(&awaiting.put_back).1
                ),
                words::titled(
                    seen.os,
                    &format!("Add the Account {app} Shows"),
                    &format!("Add the account {app} shows"),
                ),
            ),
            None => (
                format!(
                    "Pitboard can’t identify the login {app} holds. Open {app}, check which \
                     account it shows, and quit it. Pitboard records that account under this \
                     name: one you added, or a new one."
                ),
                words::titled(
                    seen.os,
                    &format!("Add the Account {app} Shows"),
                    &format!("Add the account {app} shows"),
                ),
            ),
        },
        _ => match park {
            Some(work) => (
                format!(
                    "Pitboard parks {work}, which leaves {app} signed out and signs nothing out \
                     at claude.ai. Then you sign in to the other account in {app}, and Pitboard \
                     adds it once {app} quits."
                ),
                format!("Park {work}"),
            ),
            None => (
                format!(
                    "{app} is signed out. Pitboard waits while you sign in to the account you’re \
                     adding in {app}, and adds it once {app} quits."
                ),
                words::titled(seen.os, "Start Sign-In", "Start sign-in"),
            ),
        },
    }
}

// What a sign-in sheet says, and its button, with `tool` picked.
fn form_words(seen: &Seen, sheet: &Sheet, tool: &str) -> (String, String) {
    if in_its_app(tool) {
        return in_app_form(seen, sheet, tool);
    }
    let again = match sheet {
        Sheet::SignInAgain { label, .. } => Some(label.as_str()),
        _ => None,
    };
    (
        sign_in_message(&seen.tool_name(tool), again),
        words::titled(seen.os, "Sign In", "Sign in"),
    )
}

fn sheet_tool(seen: &Seen, sheet: &Sheet, tool: &Tool) -> SheetTool {
    let (message, confirm) = form_words(seen, sheet, &tool.code);
    SheetTool {
        code: tool.code.clone(),
        name: tool.name.clone(),
        message,
        confirm,
        missing: missing(seen, &tool.code),
    }
}

// What the sheet says under its fields while it records the account an app shows.
fn progress(seen: &Seen) -> Option<String> {
    let (tool, step) = seen.state.recording()?;
    let app = seen.program_of(tool);
    Some(match step {
        Recording::Quitting => format!("Quitting {app}…"),
        Recording::Reading => format!("Reading {app}’s account…"),
    })
}

/// What the sheet over the main window says, while one is up that names an account. The
/// sheet for putting away a login left in a file says `stow_text` instead.
pub(crate) fn sheet_text(seen: &Seen) -> Option<SheetText> {
    let state = seen.state;
    let sheet = state.sheet.as_ref()?;
    let tool = starts_on(seen, sheet);
    let saving = state.saving.contains(sheet);
    let progress = progress(seen);
    let text = match sheet {
        Sheet::Add { .. } => {
            let (message, confirm) = form_words(seen, sheet, &tool);
            // An add left halfway is finished under the name it was started with.
            let name = state
                .status
                .as_ref()
                .and_then(|status| {
                    status
                        .awaiting
                        .iter()
                        .find(|awaiting| awaiting.provider == tool && !awaiting.again)
                })
                .map(|awaiting| awaiting.label.clone())
                .unwrap_or_default();
            SheetText {
                title: words::titled(seen.os, "Add Account", "Add account"),
                message,
                tools: (seen.tools.iter())
                    .map(|offered| sheet_tool(seen, sheet, offered))
                    .collect(),
                tool,
                account: None,
                name,
                prompt: "work".into(),
                confirm,
                saving,
                progress,
            }
        }
        Sheet::SignInAgain { label, .. } => {
            let (message, confirm) = form_words(seen, sheet, &tool);
            SheetText {
                title: words::titled(
                    seen.os,
                    &format!("Sign In to {label} Again"),
                    &format!("Sign in to {label} again"),
                ),
                message,
                tools: (seen.tools.iter())
                    .filter(|own| own.code == tool)
                    .map(|own| sheet_tool(seen, sheet, own))
                    .collect(),
                tool,
                account: Some(label.clone()),
                name: label.clone(),
                prompt: "work".into(),
                confirm,
                saving,
                progress,
            }
        }
        Sheet::Name { email, .. } => SheetText {
            title: words::titled(seen.os, "Name This Account", "Name this account"),
            message: format!(
                "{email} is signed in to {}. Pitboard parks its login under this name whenever \
                 you switch to another account.",
                seen.tool_name(&tool)
            ),
            tools: Vec::new(),
            account: None,
            name: suggested_name(seen, &tool, email),
            tool,
            prompt: "work".into(),
            confirm: "Save".into(),
            saving,
            progress: None,
        },
        Sheet::Rename { label, .. } => SheetText {
            title: format!("Rename “{label}”"),
            message: "The account keeps its parked login. Only the name you switch to it by \
                      changes, here and in the command line."
                .into(),
            tools: Vec::new(),
            tool,
            account: None,
            name: label.clone(),
            prompt: label.clone(),
            confirm: "Rename".into(),
            saving,
            progress: None,
        },
        Sheet::Stow => return None,
    };
    Some(text)
}

// Anyone can have an address at one of these, so the domain says nothing of whose account it is.
const MAIL_SERVICES: &[&str] = &[
    "163.com",
    "aol.com",
    "fastmail.com",
    "gmail.com",
    "gmx.com",
    "gmx.de",
    "googlemail.com",
    "hey.com",
    "hotmail.com",
    "icloud.com",
    "live.com",
    "mac.com",
    "mail.ru",
    "me.com",
    "msn.com",
    "naver.com",
    "outlook.com",
    "pm.me",
    "proton.me",
    "protonmail.com",
    "qq.com",
    "yahoo.com",
    "yandex.ru",
    "zoho.com",
];

fn suggested_name(seen: &Seen, provider: &str, email: &str) -> String {
    let email = email.to_lowercase();
    let Some((local, domain)) = email.rsplit_once('@') else {
        return String::new();
    };
    let mailbox = local.split('+').next().unwrap_or_default();
    let company = domain.split('.').next().unwrap_or_default();
    let candidates = if MAIL_SERVICES.contains(&domain) {
        vec![mailbox]
    } else {
        vec![company, mailbox]
    };
    let taken = |name: &str| {
        seen.accounts()
            .iter()
            .any(|account| account.provider == provider && account.label.as_deref() == Some(name))
    };
    candidates
        .into_iter()
        .find(|name| label::check(name).is_ok() && !taken(name))
        .unwrap_or_default()
        .to_owned()
}

pub(crate) fn stow_text(seen: &Seen) -> Option<StowText> {
    let state = seen.state;
    if state.sheet != Some(Sheet::Stow) {
        return None;
    }
    let saving = state.saving.contains(&Sheet::Stow);
    let (lines, looking, can_confirm) = match state.left.as_ref() {
        None | Some(Leftover::Looking) => (
            Vec::new(),
            Some("Finding out whose login it is…".to_owned()),
            false,
        ),
        Some(Leftover::Unknown) => (Vec::new(), None, false),
        Some(Leftover::Found(None)) => (vec![said::nothing_left().to_owned()], None, false),
        Some(Leftover::Found(Some(left))) => (
            said::left_lines(seen.os, left),
            None,
            !saving && !matches!(left.login, Foreseen::NotEnrolled(_)),
        ),
    };
    Some(StowText {
        title: words::titled(
            seen.os,
            "Put Away the Login Left in a File",
            "Put away the login left in a file",
        ),
        message: format!(
            "Claude Code left a login in a file behind {}, which keeps sessions already running \
             from following a switch. Pitboard keeps that login for its account where it holds \
             none it can switch to, then deletes the file.",
            said::store_in_front(seen.os)
        ),
        lines,
        looking,
        confirm: words::titled(seen.os, "Put Away", "Put away"),
        can_confirm,
        saving,
    })
}

/// What a sign-in under way says, whichever sheet started it: the model runs one at a time.
pub(crate) fn signing_in_text(seen: &Seen) -> Option<SigningInText> {
    if let Some(signing) = seen.state.signing().filter(|signing| signing.in_its_app) {
        return Some(in_app_signing_in_text(seen, signing));
    }
    let signing = seen.state.shown_sign_in()?;
    let tool = seen.tool_name(&signing.provider);
    Some(SigningInText {
        waiting: "Waiting for your browser".into(),
        offers_link: true,
        cancel: Choice {
            title: "Cancel".into(),
            intent: Intent::CancelSignIn,
            enabled: true,
        },
        escape_cancels: true,
        undo: None,
        announce: None,
        title: words::titled(
            seen.os,
            &format!("Signing In to {tool}"),
            &format!("Signing in to {tool}"),
        ),
        message: format!(
            "Finish signing in as {} in your browser. This closes once {tool} says you’re in.",
            signing.name
        ),
        code_note: format!(
            "{tool} takes the code shown after you sign in, whether or not your browser came \
             back to it."
        ),
        // The owner approved this sentence as written on 6 October 2026. The owner decided
        // that a code may be pasted again, and the Swift app, which never asked again, had
        // nothing to say here.
        refused: signing.code_refused.then(|| {
            format!(
                "{tool} didn’t take that code. Copy the whole code your browser shows, and \
                 paste it again."
            )
        }),
    })
}

// It offers to put back the account the core parked for it, not the one last read in use.
fn in_app_signing_in_text(seen: &Seen, signing: &crate::model::state::SigningIn) -> SigningInText {
    let provider = &signing.provider;
    let app = seen.program_of(provider);
    let (message, waiting) = if signing.quitting {
        (
            format!("Pitboard is quitting {app}. {app} may ask about work in progress first."),
            format!("Quitting {app}"),
        )
    } else if !signing.started {
        match seen
            .in_use_of(provider)
            .filter(|_| !nothing_to_park(seen, provider))
        {
            Some(work) => (format!("Parking {work}."), format!("Parking {work}")),
            None => ("Getting ready.".into(), "Starting".into()),
        }
    } else {
        (
            words::in_app_waiting(
                app,
                &signing.name,
                signing.again,
                signing.step,
                signing.opened_for_it,
            ),
            format!("Waiting for {app} to quit"),
        )
    };
    let put_back = signing.put_back.as_deref().filter(|_| signing.started);
    let asks_again = matches!(
        signing.step,
        Some(WaitStep::QuitSignedOut | WaitStep::QuitUnfinished)
    );
    let announce = (signing.started && asks_again).then(|| message.clone());
    SigningInText {
        title: if signing.again {
            words::titled(
                seen.os,
                &format!("Sign In to {} Again", signing.name),
                &format!("Sign in to {} again", signing.name),
            )
        } else {
            words::titled(seen.os, "Add Account", "Add account")
        },
        message,
        code_note: String::new(),
        refused: None,
        waiting,
        offers_link: false,
        cancel: Choice {
            title: if put_back.is_some() {
                "Later"
            } else {
                "Cancel"
            }
            .into(),
            intent: Intent::CancelSignIn,
            enabled: !signing.quitting,
        },
        escape_cancels: signing.started && put_back.is_none(),
        undo: put_back.map(|qualified| Choice {
            title: words::titled(
                seen.os,
                &format!("Put {} Back", split(qualified).1),
                &format!("Put {} back", split(qualified).1),
            ),
            intent: Intent::SwitchTo {
                qualified: qualified.to_owned(),
            },
            enabled: seen.state.switch_under_way().is_none(),
        }),
        announce,
    }
}

/// What a switch to `qualified` asks before it puts that account's login in place of a login
/// its tool has in use, which its service refuses for good, for `why`.
pub(crate) fn replace_question(seen: &Seen, qualified: &str, why: &str) -> ReplaceQuestion {
    let tool = seen.tool_name(split(qualified).0);
    ReplaceQuestion {
        qualified: qualified.to_owned(),
        question: Question {
            title: words::titled(
                seen.os,
                &format!("Replace {tool}’s Login?"),
                &format!("Replace {tool}’s login?"),
            ),
            message: why.to_owned(),
            confirm: "Replace".into(),
        },
    }
}

pub(crate) fn quit_confirmation(seen: &Seen, question: &QuitQuestion, then: &After) -> Question {
    let name = &question.name;
    let (tool, label) = split(&question.qualified);
    if !in_its_app(tool) {
        return Question {
            title: format!("Quit {name} to switch?"),
            message: format!(
                "{name} keeps using the account it started with until it quits. Pitboard quits \
                 it, switches, and opens it again."
            ),
            confirm: words::titled(
                seen.os,
                &format!("Quit {name} and Switch"),
                &format!("Quit {name} and switch"),
            ),
        };
    }
    let only_while_closed = |does: &str| format!("Pitboard {does} only while {name} is closed.");
    match then {
        After::Switch => Question {
            title: format!("Quit {name} to switch?"),
            message: format!(
                "{} It quits {name}, switches to {label}, and opens it again.",
                only_while_closed(&format!("moves {name}’s login"))
            ),
            confirm: words::titled(
                seen.os,
                &format!("Quit {name} and Switch"),
                &format!("Quit {name} and switch"),
            ),
        },
        After::SignIn { from, .. } => {
            let title = match from {
                Some(Sheet::SignInAgain { .. }) => {
                    format!("Quit {name} to sign in to {label} again?")
                }
                _ => format!("Quit {name} to add an account?"),
            };
            match seen
                .in_use_of(tool)
                .filter(|_| !nothing_to_park(seen, tool))
            {
                Some(work) => Question {
                    title,
                    message: format!(
                        "{} It quits {name}, parks {work}, and opens {name} again for you to \
                         sign in.",
                        only_while_closed(&format!("parks {work}"))
                    ),
                    confirm: words::titled(
                        seen.os,
                        &format!("Quit {name} and Park {work}"),
                        &format!("Quit {name} and park {work}"),
                    ),
                },
                None => Question {
                    title,
                    message: format!(
                        "{} It quits {name} and opens it again for you to sign in.",
                        only_while_closed("starts waiting for a sign-in")
                    ),
                    confirm: words::titled(
                        seen.os,
                        &format!("Quit {name} and Start Sign-In"),
                        &format!("Quit {name} and start sign-in"),
                    ),
                },
            }
        }
        After::Enrol { from, .. } => {
            let reads = only_while_closed(&format!("reads which account {name} is signed in to"));
            match from {
                Some(Sheet::SignInAgain { .. }) => Question {
                    title: format!("Quit {name} to record {label}?"),
                    message: format!(
                        "{reads} It quits {name}, records that account as {label}’s login, and \
                         opens it again."
                    ),
                    confirm: words::titled(
                        seen.os,
                        &format!("Quit {name} and Record"),
                        &format!("Quit {name} and record"),
                    ),
                },
                _ => Question {
                    title: format!("Quit {name} to add {label}?"),
                    message: format!(
                        "{reads} It quits {name}, records that account as {label}, and opens it \
                         again."
                    ),
                    confirm: words::titled(
                        seen.os,
                        &format!("Quit {name} and Add"),
                        &format!("Quit {name} and add"),
                    ),
                },
            }
        }
    }
}

/// A failure's alert, gone once it is read: its title, and its message with everything else
/// it warned about.
pub(crate) fn failure_alert(failure: &Failure) -> AlertText {
    alert_of(&failure.title, &failure.message, &failure.warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rename(label: &str) -> Sheet {
        Sheet::Rename {
            provider: "claude".into(),
            label: label.into(),
        }
    }

    fn saved(sheet: Sheet, typed: &str) -> Option<String> {
        check_name(sheet, typed.into()).name
    }

    fn problem(sheet: Sheet, typed: &str) -> Option<String> {
        check_name(sheet, typed.into()).problem
    }

    // Save waits for a name, trimmed: in a rename, one the account does not already have.
    #[test]
    fn a_name_is_saved_trimmed_and_only_once_there_is_one() {
        let name = Sheet::Name {
            provider: "claude".into(),
            email: "a@example.com".into(),
        };
        assert_eq!(saved(name.clone(), "  work\n").as_deref(), Some("work"));
        assert_eq!(saved(name.clone(), " \t\u{200b}"), None);
        assert_eq!(saved(name.clone(), ""), None);
        assert_eq!(problem(name, ""), None, "nothing typed is nothing wrong");
        assert_eq!(saved(rename("work"), " work "), None, "unchanged");
        assert_eq!(problem(rename("work"), " work "), None);
        assert_eq!(saved(rename("work"), "home ").as_deref(), Some("home"));
        assert_eq!(
            saved(Sheet::Add { provider: None }, " travel").as_deref(),
            Some("travel")
        );
        let again = Sheet::SignInAgain {
            provider: "codex".into(),
            label: "my work".into(),
        };
        assert_eq!(
            saved(again, "").as_deref(),
            Some("my work"),
            "the account's own name, whatever a new one may be"
        );
    }

    // The tool is picked apart from the name, so a name naming one again is refused as typed,
    // and not where the core would read it as `codex/codex/phu`.
    #[test]
    fn a_name_that_cannot_be_one_says_why_as_it_is_typed() {
        let add = || Sheet::Add {
            provider: Some("codex".into()),
        };
        assert_eq!(saved(add(), "codex/phu"), None);
        assert_eq!(
            problem(add(), "codex/phu").as_deref(),
            Some("A name can’t contain “/”. Choose the tool above.")
        );
        assert_eq!(
            problem(rename("work"), "a/b").as_deref(),
            Some("A name can’t contain “/”.")
        );
        assert_eq!(saved(add(), "my work"), None);
        assert_eq!(
            problem(add(), "my work").as_deref(),
            Some("A name is one word, with no spaces.")
        );
        assert_eq!(problem(add(), "phu"), None);
    }

    use crate::Account;
    use crate::model::state::{Cadence, State};
    use crate::present::testing::{
        Utc, awaiting, claude_code, claude_desktop, desktop_account, desktop_warning, read, status,
    };
    use pitboard_core::host::Os;
    use pitboard_core::switch::WaitStep;

    fn asked(qualified: &str, app_id: &str, name: &str) -> QuitQuestion {
        QuitQuestion {
            qualified: qualified.into(),
            app_id: app_id.into(),
            name: name.into(),
        }
    }

    const CLAUDE: &str = "com.anthropic.claudefordesktop";

    fn add_desktop() -> Sheet {
        Sheet::Add {
            provider: Some("desktop".into()),
        }
    }

    // What the form for `sheet` says on a Mac that read `accounts`, warned of `warnings`.
    fn form(sheet: Sheet, accounts: Vec<Account>, warnings: Vec<crate::Warning>) -> SheetText {
        form_of(
            &mut read(
                status(accounts, warnings),
                vec![claude_code(), claude_desktop()],
            ),
            sheet,
        )
    }

    fn form_of(state: &mut State, sheet: Sheet) -> SheetText {
        state.sheet = Some(sheet);
        sheet_text(&Seen::new(state, 0, &Utc, Os::MacOs)).expect("a form")
    }

    fn work_and_home() -> Vec<Account> {
        vec![
            desktop_account("work", true),
            desktop_account("home", false),
        ]
    }

    #[test]
    fn chatgpt_is_asked_to_quit_in_its_own_words() {
        let state = State::new(Cadence::APP);
        let seen = Seen::new(&state, 0, &Utc, Os::MacOs);
        let asked = quit_confirmation(
            &seen,
            &asked("codex/work", "com.openai.codex", "ChatGPT"),
            &After::Switch,
        );
        assert_eq!(asked.title, "Quit ChatGPT to switch?");
        assert_eq!(asked.confirm, "Quit ChatGPT and Switch");
        assert_eq!(
            asked.message,
            "ChatGPT keeps using the account it started with until it quits. Pitboard quits \
             it, switches, and opens it again."
        );
    }

    #[test]
    fn each_tool_in_the_add_sheet_names_its_own_button() {
        let mut accounts = work_and_home();
        accounts.push(
            crate::present::testing::account(Some("work"))
                .signed_in()
                .build(),
        );
        let text = form(Sheet::Add { provider: None }, accounts, Vec::new());
        assert_eq!(
            text.tools
                .iter()
                .map(|tool| (tool.code.as_str(), tool.confirm.as_str()))
                .collect::<Vec<_>>(),
            [
                ("claude", "Sign In"),
                ("codex", "Sign In"),
                ("desktop", "Park work")
            ]
        );
        assert_eq!(text.confirm, "Sign In", "it starts on Claude Code");
    }

    #[test]
    fn the_add_sheet_for_claude_desktop_says_what_it_does_in_each_case() {
        let case = |accounts: Vec<Account>, warnings: Vec<crate::Warning>| {
            let text = form(add_desktop(), accounts, warnings);
            (text.message, text.confirm)
        };
        assert_eq!(
            case(Vec::new(), Vec::new()),
            (
                "Pitboard records the account Claude is signed in to under this name. Check \
                 which account Claude shows first: Pitboard can’t ask claude.ai whose it is."
                    .into(),
                "Add Account".into()
            )
        );
        assert_eq!(
            case(work_and_home(), Vec::new()),
            (
                "Pitboard parks work, which leaves Claude signed out and signs nothing out at \
                 claude.ai. Then you sign in to the other account in Claude, and Pitboard adds \
                 it once Claude quits."
                    .into(),
                "Park work".into()
            )
        );
        let signed_out = (
            "Claude is signed out. Pitboard waits while you sign in to the account you’re \
             adding in Claude, and adds it once Claude quits."
                .to_owned(),
            "Start Sign-In".to_owned(),
        );
        assert_eq!(
            case(
                vec![
                    desktop_account("work", false),
                    desktop_account("home", false)
                ],
                Vec::new()
            ),
            signed_out
        );
        assert_eq!(
            case(
                work_and_home(),
                vec![desktop_warning(
                    "looks_signed_out",
                    "Claude looks signed out"
                )]
            ),
            signed_out
        );
        let doubt = (
            "Pitboard can’t identify the login Claude holds. Open Claude, check which account \
             it shows, and quit it. Pitboard records that account under this name: one you \
             added, or a new one."
                .to_owned(),
            "Add the Account Claude Shows".to_owned(),
        );
        for code in ["in_use_unconfirmed", "move_waiting"] {
            assert_eq!(
                case(work_and_home(), vec![desktop_warning(code, code)]),
                doubt,
                "{code}"
            );
        }
        let mut waiting = status(
            vec![
                desktop_account("work", false),
                desktop_account("home", false),
            ],
            Vec::new(),
        );
        waiting.awaiting = vec![awaiting("travel", "work", false)];
        let mut state = read(waiting, vec![claude_code(), claude_desktop()]);
        let text = form_of(&mut state, add_desktop());
        assert_eq!(
            (
                text.message.as_str(),
                text.confirm.as_str(),
                text.name.as_str()
            ),
            (
                "Claude is signed out so you can sign in to the account you’re adding, and \
                 work is parked. Sign in to it in Claude and quit Claude, then add it here.",
                "Add the Account Claude Shows",
                "travel"
            )
        );
    }

    #[test]
    fn a_claude_that_looks_signed_out_is_offered_a_sign_in_with_nothing_to_park() {
        let warned = vec![desktop_warning(
            "looks_signed_out",
            "Claude looks signed out",
        )];
        let mut state = read(
            status(work_and_home(), warned),
            vec![claude_code(), claude_desktop()],
        );
        assert_eq!(form_of(&mut state, add_desktop()).confirm, "Start Sign-In");
        let seen = Seen::new(&state, 0, &Utc, Os::MacOs);
        let question = quit_confirmation(
            &seen,
            &asked("desktop/travel", CLAUDE, "Claude"),
            &After::SignIn {
                provider: "desktop".into(),
                name: "travel".into(),
                from: Some(add_desktop()),
            },
        );
        assert_eq!(question.confirm, "Quit Claude and Start Sign-In");
        assert_eq!(
            question.message,
            "Pitboard starts waiting for a sign-in only while Claude is closed. It quits Claude \
             and opens it again for you to sign in."
        );
    }

    #[test]
    fn recording_from_sign_in_again_asks_to_record_the_account() {
        let again = Sheet::SignInAgain {
            provider: "desktop".into(),
            label: "home".into(),
        };
        let text = form(
            again.clone(),
            work_and_home(),
            vec![desktop_warning("in_use_unconfirmed", "in doubt")],
        );
        assert_eq!(text.confirm, "Record as home");
        let state = State::new(Cadence::APP);
        let seen = Seen::new(&state, 0, &Utc, Os::MacOs);
        let question = quit_confirmation(
            &seen,
            &asked("desktop/home", CLAUDE, "Claude"),
            &After::Enrol {
                provider: "desktop".into(),
                name: "home".into(),
                from: Some(again),
            },
        );
        assert_eq!(question.title, "Quit Claude to record home?");
        assert_eq!(
            question.message,
            "Pitboard reads which account Claude is signed in to only while Claude is closed. It \
             quits Claude, records that account as home’s login, and opens it again."
        );
        assert_eq!(question.confirm, "Quit Claude and Record");
    }

    #[test]
    fn the_quit_question_says_what_the_quit_is_for() {
        let state = read(
            status(work_and_home(), Vec::new()),
            vec![claude_code(), claude_desktop()],
        );
        let seen = Seen::new(&state, 0, &Utc, Os::MacOs);
        let said = |qualified: &str, then: After| {
            let question = quit_confirmation(&seen, &asked(qualified, CLAUDE, "Claude"), &then);
            (question.title, question.message, question.confirm)
        };
        assert_eq!(
            said("desktop/home", After::Switch),
            (
                "Quit Claude to switch?".into(),
                "Pitboard moves Claude’s login only while Claude is closed. It quits Claude, \
                 switches to home, and opens it again."
                    .into(),
                "Quit Claude and Switch".into()
            )
        );
        let parks = "Pitboard parks work only while Claude is closed. It quits Claude, parks \
                     work, and opens Claude again for you to sign in.";
        assert_eq!(
            said(
                "desktop/travel",
                After::SignIn {
                    provider: "desktop".into(),
                    name: "travel".into(),
                    from: Some(add_desktop()),
                }
            ),
            (
                "Quit Claude to add an account?".into(),
                parks.into(),
                "Quit Claude and Park work".into()
            )
        );
        assert_eq!(
            said(
                "desktop/home",
                After::SignIn {
                    provider: "desktop".into(),
                    name: "home".into(),
                    from: Some(Sheet::SignInAgain {
                        provider: "desktop".into(),
                        label: "home".into(),
                    }),
                }
            ),
            (
                "Quit Claude to sign in to home again?".into(),
                parks.into(),
                "Quit Claude and Park work".into()
            )
        );
        assert_eq!(
            said(
                "desktop/travel",
                After::Enrol {
                    provider: "desktop".into(),
                    name: "travel".into(),
                    from: Some(add_desktop()),
                }
            ),
            (
                "Quit Claude to add travel?".into(),
                "Pitboard reads which account Claude is signed in to only while Claude is \
                 closed. It quits Claude, records that account as travel, and opens it again."
                    .into(),
                "Quit Claude and Add".into()
            )
        );
    }

    #[test]
    fn the_wait_is_said_step_by_step() {
        let both = |message: &str| {
            format!(
                "{message}\n\nDon’t choose Log out in Claude: it ends the login Claude holds at \
                 claude.ai, and Pitboard can’t bring it back."
            )
        };
        let said =
            |again, step, opened| words::in_app_waiting("Claude", "travel", again, step, opened);
        assert_eq!(
            said(false, Some(WaitStep::Waiting), true),
            both(
                "In Claude, sign in to the account you’re adding. When it shows your chats, quit \
                 Claude with Command-Q, and Pitboard adds it as travel."
            )
        );
        assert_eq!(
            said(false, Some(WaitStep::Open), false),
            both(
                "Open Claude and sign in to the account you’re adding. When it shows your chats, \
                 quit Claude with Command-Q, and Pitboard adds it as travel."
            )
        );
        assert_eq!(
            said(false, Some(WaitStep::QuitSignedOut), true),
            both(
                "Claude quit with nobody signed in. Open Claude, sign in to the account you’re \
                 adding, then quit it with Command-Q."
            )
        );
        assert_eq!(
            said(false, Some(WaitStep::QuitUnfinished), true),
            both(
                "Claude quit before it finished signing in. Open Claude, wait until it shows \
                 your chats, then quit it with Command-Q."
            )
        );
        assert_eq!(
            said(true, None, true),
            both(
                "In Claude, sign in to travel again. When it shows your chats, quit Claude with \
                 Command-Q, and Pitboard records it as travel’s login."
            )
        );
    }

    /// A failure's alert says what went wrong and everything else it warned about, a
    /// paragraph each.
    #[test]
    fn a_failures_alert_says_its_warnings_too() {
        let alert = failure_alert(&Failure {
            id: 1,
            title: "Couldn’t switch to personal".into(),
            message: "Nothing is parked.".into(),
            code: Some("nothing_parked".into()),
            warnings: vec![crate::Warning {
                code: "auth_overridden".into(),
                message: "ANTHROPIC_API_KEY is set".into(),
                account: None,
                held: None,
                provider: None,
            }],
        });
        assert_eq!(alert.title, "Couldn’t switch to personal");
        assert_eq!(
            alert.message,
            "Nothing is parked.\n\nANTHROPIC_API_KEY is set"
        );
    }

    #[test]
    fn the_replace_question_is_titled_as_each_system_titles_one() {
        let state = read(status(Vec::new(), Vec::new()), vec![claude_code()]);
        let titled = |os| {
            replace_question(&Seen::new(&state, 0, &Utc, os), "work", "why")
                .question
                .title
        };
        assert_eq!(titled(Os::MacOs), "Replace Claude Code’s Login?");
        assert_eq!(titled(Os::Windows), "Replace Claude Code’s login?");
    }
}
