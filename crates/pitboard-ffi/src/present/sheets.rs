//! What the sheets and the window's questions say, from NameSheets.swift, SignInSheet.swift
//! and MainWindow.swift: the sheet over the window, a sign-in under way, the sheet for putting
//! away a login left in a file, the question about quitting an app to switch, and a failure's
//! alert.

use super::words;
use super::{
    Choice, FailureText, Missing, NameCheck, NextStep, Question, Seen, SheetText, SheetTool,
    SigningInText, StowText, WindowQuestion, alert_of,
};
use crate::Tool;
use crate::account_windows::AlertText;
use crate::model::state::{After, InAppAdd, Leftover, in_its_app, trimmed};
use crate::model::{Failure, Intent, QuitQuestion, Sheet};
use pitboard_core::host::Os;
use pitboard_core::label;
use pitboard_core::provider::ProviderId;
use pitboard_core::state::Key;
use pitboard_core::switch::{Foreseen, WaitStep};

/// What a sheet's default button sends with `typed` as the name, `tool` picked where the sheet
/// picks one, or why what is typed cannot be a name. The model saves by the same rule, and it
/// answers at once, so a view asks it as it is typed in. A sheet that signs in takes no name.
#[uniffi::export]
pub fn check_name(sheet: Sheet, tool: String, typed: String) -> NameCheck {
    let sends = |intent| NameCheck {
        problem: None,
        intent,
    };
    match sheet {
        Sheet::Add { .. } => sends(Some(Intent::SignIn {
            provider: tool,
            again: None,
        })),
        Sheet::SignInAgain { provider, label } => sends(Some(Intent::SignIn {
            provider,
            again: Some(label),
        })),
        Sheet::Stow => sends(None),
        Sheet::Name { .. } | Sheet::Rename { .. } => match saved_name(&sheet, &typed) {
            Ok(Some(to)) => sends(renames(sheet, to)),
            // The account is added under the name it shows already.
            Ok(None) => sends(matches!(sheet, Sheet::Name { .. }).then_some(Intent::CloseSheet)),
            Err(unusable) => NameCheck {
                problem: Some(words::unusable_name(unusable)),
                intent: None,
            },
        },
    }
}

/// The name `typed` gives the account `sheet` names, trimmed: none where it is the name the
/// account has.
pub(crate) fn saved_name(sheet: &Sheet, typed: &str) -> Result<Option<String>, label::Unusable> {
    let name = trimmed(typed);
    match sheet {
        Sheet::Rename { label, .. } | Sheet::Name { label, .. } if label == name => Ok(None),
        _ => label::check(name).map(|()| Some(name.to_owned())),
    }
}

fn renames(sheet: Sheet, to: String) -> Option<Intent> {
    match sheet {
        Sheet::Rename { provider, label }
        | Sheet::Name {
            provider, label, ..
        } => Some(Intent::Rename {
            provider,
            label,
            to,
        }),
        _ => None,
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
        Sheet::SignInAgain { label, .. } if current => {
            (words::add_shown(app, label), words::add_as(label))
        }
        Sheet::SignInAgain { label, .. } => match park {
            Some(work) => (
                words::sign_out_to_sign_in_again(app, label, work),
                words::CONTINUE.into(),
            ),
            None => (
                words::wait_to_sign_in_again(app, label),
                words::start_sign_in(seen.os),
            ),
        },
        _ if current && first => (
            words::add_shown_first(app),
            words::add_account(seen.os, false),
        ),
        _ if current => (
            match awaiting {
                Some(_) => words::add_once_signed_in(app),
                None => words::add_unidentified(app),
            },
            words::add_account_shown(seen.os, app, false),
        ),
        _ => match park {
            Some(work) => (words::sign_out_to_add(app, work), words::CONTINUE.into()),
            None => (words::wait_to_add(app), words::start_sign_in(seen.os)),
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
        words::sign_in_message(&seen.tool_name(tool), again),
        words::sign_in_asking(seen.os),
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
    Some(words::recording(seen.program_of(tool), step))
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
            SheetText {
                title: words::add_account(seen.os, false),
                message,
                tools: (seen.tools.iter())
                    .map(|offered| sheet_tool(seen, sheet, offered))
                    .collect(),
                tool,
                account: None,
                name: String::new(),
                prompt: words::NAME_PROMPT.into(),
                confirm,
                saving,
                progress,
            }
        }
        Sheet::SignInAgain { label, .. } => {
            let (message, confirm) = form_words(seen, sheet, &tool);
            SheetText {
                title: words::sign_in_to_again(seen.os, label, false),
                message,
                tools: (seen.tools.iter())
                    .filter(|own| own.code == tool)
                    .map(|own| sheet_tool(seen, sheet, own))
                    .collect(),
                tool,
                account: Some(label.clone()),
                name: label.clone(),
                prompt: words::NAME_PROMPT.into(),
                confirm,
                saving,
                progress,
            }
        }
        Sheet::Name { label, email, .. } => SheetText {
            title: words::name_this_account(seen.os),
            message: words::naming(email, &seen.tool_name(&tool)),
            tools: Vec::new(),
            account: None,
            name: label.clone(),
            tool,
            prompt: words::NAME_PROMPT.into(),
            confirm: words::DONE.into(),
            saving,
            progress: None,
        },
        Sheet::Rename { label, .. } => SheetText {
            title: words::rename_title(label),
            message: words::RENAMING.into(),
            tools: Vec::new(),
            tool,
            account: None,
            name: label.clone(),
            prompt: label.clone(),
            confirm: words::rename(false).into(),
            saving,
            progress: None,
        },
        Sheet::Stow => return None,
    };
    Some(text)
}

pub(crate) fn stow_text(seen: &Seen) -> Option<StowText> {
    let state = seen.state;
    if state.sheet != Some(Sheet::Stow) {
        return None;
    }
    let saving = state.saving.contains(&Sheet::Stow);
    let (lines, looking, can_confirm) = match state.left.as_ref() {
        None | Some(Leftover::Looking) => {
            (Vec::new(), Some(words::FINDING_OUT_WHOSE.to_owned()), false)
        }
        Some(Leftover::Unknown) => (Vec::new(), None, false),
        Some(Leftover::Found(None)) => (vec![words::nothing_to_clean_up(seen.os)], None, false),
        Some(Leftover::Found(Some(left))) => (
            words::clean_up_lines(seen.os, left)
                .iter()
                .map(|line| words::typeset(line))
                .collect(),
            None,
            !saving && !matches!(left.login, Foreseen::NotEnrolled(_)),
        ),
    };
    Some(StowText {
        title: words::clean_up_title(seen.os),
        message: words::clean_up_message(seen.os),
        lines,
        looking,
        confirm: words::clean_up(seen.os, false),
        can_confirm,
        saving,
    })
}

/// What a sign-in under way says, whichever sheet started it: the model runs one at a time.
pub(crate) fn signing_in_text(seen: &Seen) -> Option<SigningInText> {
    if let Some(signing) = seen.state.signing().filter(|signing| signing.in_its_app) {
        return Some(in_app_signing_in_text(seen, signing));
    }
    let signing = seen.state.signing()?;
    let tool = seen.tool_name(&signing.provider);
    Some(SigningInText {
        waiting: words::WAITING_FOR_BROWSER.into(),
        offers_link: true,
        cancel: Choice {
            title: words::CANCEL.into(),
            intent: Intent::CancelSignIn,
            enabled: true,
        },
        escape_cancels: true,
        undo: None,
        announce: None,
        title: words::signing_in_to(seen.os, &tool),
        message: words::finish_in_browser(signing.again.as_deref(), &tool),
        code_note: words::code_note(&tool),
        refused: signing.code_refused().then(|| words::code_refused(&tool)),
    })
}

// It offers to put back the account the core parked for it, not the one last read in use.
fn in_app_signing_in_text(seen: &Seen, signing: &crate::model::state::SigningIn) -> SigningInText {
    let provider = &signing.provider;
    let app = seen.program_of(provider);
    let (message, waiting) = if signing.quitting {
        words::quitting(app)
    } else if !signing.started {
        match seen
            .in_use_of(provider)
            .filter(|_| !nothing_to_park(seen, provider))
        {
            Some(work) => words::signing_out(work),
            None => (words::GETTING_READY.0.into(), words::GETTING_READY.1.into()),
        }
    } else {
        (
            words::in_app_waiting(
                app,
                signing.again.as_deref(),
                signing.step,
                signing.opened_for_it,
            ),
            words::waiting_to_quit(app),
        )
    };
    let put_back = signing.put_back.as_deref().filter(|_| signing.started);
    let asks_again = matches!(
        signing.step,
        Some(WaitStep::QuitSignedOut | WaitStep::QuitUnfinished)
    );
    let announce = (signing.started && asks_again).then(|| message.clone());
    SigningInText {
        title: match &signing.again {
            Some(label) => words::sign_in_to_again(seen.os, label, false),
            None => words::add_account(seen.os, false),
        },
        message,
        code_note: String::new(),
        refused: None,
        waiting,
        offers_link: false,
        cancel: Choice {
            title: if put_back.is_some() {
                words::LATER
            } else {
                words::CANCEL
            }
            .into(),
            intent: Intent::CancelSignIn,
            enabled: !signing.quitting,
        },
        escape_cancels: signing.started && put_back.is_none(),
        undo: put_back.map(|qualified| Choice {
            title: words::switch_back_to(seen.os, &Key::parse(qualified).label),
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
pub(crate) fn replace_question(seen: &Seen, qualified: &str, why: &str) -> WindowQuestion {
    let tool = seen.tool_name(Key::parse(qualified).provider.code());
    WindowQuestion {
        question: Question::asking(
            words::replace_title(&tool),
            why.to_owned(),
            words::REPLACE.into(),
        ),
        confirmed: Intent::ReplaceAndSwitch {
            qualified: qualified.to_owned(),
        },
        declined: Intent::KeepLogin,
    }
}

/// What the quit question asks, while it is asked, with its answers.
pub(crate) fn quit_question(seen: &Seen, question: &QuitQuestion, then: &After) -> WindowQuestion {
    WindowQuestion {
        question: quit_asks(seen, question, then),
        confirmed: Intent::QuitAndSwitch {
            qualified: question.qualified.clone(),
        },
        declined: Intent::KeepAppOpen,
    }
}

fn quit_asks(seen: &Seen, question: &QuitQuestion, then: &After) -> Question {
    let name = &question.name;
    let Key { provider, label } = Key::parse(&question.qualified);
    let tool = provider.code();
    if !in_its_app(tool) {
        return Question::asking(
            words::quit_to_switch(name),
            words::quit_and_reopen(name),
            words::quit_and_switch(seen.os, name),
        );
    }
    match then {
        After::Switch => Question::asking(
            words::quit_to_switch(name),
            words::quit_to_move(name, &label),
            words::quit_and_switch(seen.os, name),
        ),
        After::SignIn { again, .. } => {
            let title = words::quit_to_sign_in(name, again.as_deref());
            match seen
                .in_use_of(tool)
                .filter(|_| !nothing_to_park(seen, tool))
            {
                Some(work) => Question::asking(
                    title,
                    words::quit_to_sign_out(name, work),
                    words::quit_and_continue(seen.os, name),
                ),
                None => Question::asking(
                    title,
                    words::quit_to_wait(name),
                    words::quit_and_start_sign_in(seen.os, name),
                ),
            }
        }
        After::Enrol { again, .. } => Question::asking(
            match again {
                Some(label) => words::quit_to_add(name, label),
                None => words::quit_to_sign_in(name, None),
            },
            words::quit_to_read(name, again.as_deref()),
            words::quit_and_add(seen.os, name),
        ),
        After::Uninstall => Question::asking(
            words::quit_to_uninstall(name),
            words::quit_to_delete(name),
            words::quit_and_uninstall(seen.os, name),
        ),
    }
}

/// A failure's alert, gone once it is read: its title, its message with everything else it
/// warned about, and what to do.
pub(crate) fn failure_alert(os: Os, failure: &Failure) -> AlertText {
    AlertText {
        next: Some(next_of(os, failure)),
        ..alert_of(&failure.title, &failure.message, &failure.warnings)
    }
}

pub(crate) fn failure_text(os: Os, failure: &Failure) -> FailureText {
    FailureText {
        id: failure.id,
        title: failure.title.clone(),
        message: failure.message.clone(),
        warnings: failure
            .warnings
            .iter()
            .map(|warning| warning.message.clone())
            .collect(),
        next: next_of(os, failure),
    }
}

fn next_of(os: Os, failure: &Failure) -> NextStep {
    super::next::of_failure(os, failure.code.as_deref(), failure.account.as_deref())
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
        saved_name(&sheet, typed).ok().flatten()
    }

    fn problem(sheet: Sheet, typed: &str) -> Option<String> {
        check_name(sheet, "claude".into(), typed.into()).problem
    }

    fn named(label: &str) -> Sheet {
        Sheet::Name {
            provider: "claude".into(),
            label: label.into(),
            email: "a@example.com".into(),
        }
    }

    // Save waits for a name, trimmed, the account does not already have.
    #[test]
    fn a_name_is_saved_trimmed_and_only_once_there_is_one() {
        for (sheet, kind) in [(named("acme"), "named"), (rename("acme"), "renamed")] {
            assert_eq!(
                saved(sheet.clone(), "  work\n").as_deref(),
                Some("work"),
                "{kind}"
            );
            assert_eq!(saved(sheet.clone(), " \t\u{200b}"), None, "{kind}");
            assert_eq!(
                problem(sheet.clone(), " "),
                Some(words::unusable_name(label::Unusable::Empty)),
                "{kind}: the field says why its button waits"
            );
            assert_eq!(saved(sheet.clone(), " acme "), None, "{kind}: unchanged");
            assert_eq!(problem(sheet, " acme "), None, "{kind}");
        }
    }

    // Neither app maps a sheet to an intent; naming an account just added renames it on a change.
    #[test]
    fn a_name_comes_with_what_its_sheet_sends() {
        let sends = |sheet: Sheet, tool: &str, typed: &str| {
            check_name(sheet, tool.into(), typed.into()).intent
        };
        assert_eq!(
            sends(Sheet::Add { provider: None }, "codex", " travel"),
            Some(Intent::SignIn {
                provider: "codex".into(),
                again: None,
            }),
            "the tool picked"
        );
        assert_eq!(
            sends(
                Sheet::SignInAgain {
                    provider: "codex".into(),
                    label: "my work".into()
                },
                "codex",
                ""
            ),
            Some(Intent::SignIn {
                provider: "codex".into(),
                again: Some("my work".into()),
            }),
            "the account's own name, whatever a new one may be"
        );
        let to_home = Some(Intent::Rename {
            provider: "claude".into(),
            label: "acme".into(),
            to: "home".into(),
        });
        assert_eq!(sends(named("acme"), "claude", "home"), to_home);
        assert_eq!(
            sends(named("acme"), "claude", " acme"),
            Some(Intent::CloseSheet)
        );
        assert_eq!(sends(rename("acme"), "claude", "home"), to_home);
        assert_eq!(sends(rename("acme"), "claude", "acme"), None, "unchanged");
        assert_eq!(sends(named("acme"), "claude", "a/b"), None);
        assert_eq!(sends(Sheet::Stow, "claude", "work"), None);
    }

    #[test]
    fn a_name_that_cannot_be_one_says_why_as_it_is_typed() {
        use pitboard_core::label::Unusable;
        for sheet in [named("acme"), rename("acme")] {
            assert_eq!(saved(sheet.clone(), "codex/phu"), None);
            assert_eq!(
                problem(sheet.clone(), "codex/phu"),
                Some(words::unusable_name(Unusable::Separator))
            );
            assert_eq!(
                problem(sheet.clone(), "my work"),
                Some(words::unusable_name(Unusable::NotOneWord))
            );
            assert_eq!(problem(sheet, "phu"), None);
        }
    }

    use crate::Account;
    use crate::model::state::{Cadence, State};
    use crate::present::testing::{
        Utc, awaiting, claude_code, claude_desktop, desktop_account, desktop_warning, read, status,
    };
    use pitboard_core::host::Os;

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
        let asked = quit_asks(
            &seen,
            &asked("codex/work", "com.openai.codex", "ChatGPT"),
            &After::Switch,
        );
        assert_eq!(
            asked,
            Question::asking(
                words::quit_to_switch("ChatGPT"),
                words::quit_and_reopen("ChatGPT"),
                words::quit_and_switch(Os::MacOs, "ChatGPT")
            )
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
        let sign_in = words::sign_in_asking(Os::MacOs);
        assert_eq!(
            text.tools
                .iter()
                .map(|tool| (tool.code.as_str(), tool.confirm.clone()))
                .collect::<Vec<_>>(),
            [
                ("claude", sign_in.clone()),
                ("codex", sign_in.clone()),
                ("desktop", words::CONTINUE.into())
            ]
        );
        assert_eq!(text.confirm, sign_in, "it starts on Claude Code");
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
                words::add_shown_first("Claude"),
                words::add_account(Os::MacOs, false)
            )
        );
        assert_eq!(
            case(work_and_home(), Vec::new()),
            (
                words::sign_out_to_add("Claude", "work"),
                words::CONTINUE.into()
            )
        );
        let signed_out = (
            words::wait_to_add("Claude"),
            words::start_sign_in(Os::MacOs),
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
            words::add_unidentified("Claude"),
            words::add_account_shown(Os::MacOs, "Claude", false),
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
            (text.message, text.confirm),
            (
                words::add_once_signed_in("Claude"),
                words::add_account_shown(Os::MacOs, "Claude", false),
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
        assert_eq!(
            form_of(&mut state, add_desktop()).confirm,
            words::start_sign_in(Os::MacOs)
        );
        let seen = Seen::new(&state, 0, &Utc, Os::MacOs);
        let question = quit_asks(
            &seen,
            &asked("desktop/", CLAUDE, "Claude"),
            &After::SignIn {
                provider: "desktop".into(),
                again: None,
                from: Some(add_desktop()),
            },
        );
        assert_eq!(
            question.confirm,
            words::quit_and_start_sign_in(Os::MacOs, "Claude")
        );
        assert_eq!(question.message, words::quit_to_wait("Claude"));
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
        assert_eq!(text.confirm, words::add_as("home"));
        let state = State::new(Cadence::APP);
        let seen = Seen::new(&state, 0, &Utc, Os::MacOs);
        let question = quit_asks(
            &seen,
            &asked("desktop/home", CLAUDE, "Claude"),
            &After::Enrol {
                provider: "desktop".into(),
                again: Some("home".into()),
                from: Some(again),
            },
        );
        assert_eq!(
            question,
            Question::asking(
                words::quit_to_add("Claude", "home"),
                words::quit_to_read("Claude", Some("home")),
                words::quit_and_add(Os::MacOs, "Claude")
            )
        );
    }

    #[test]
    fn the_quit_question_says_what_the_quit_is_for() {
        let state = read(
            status(work_and_home(), Vec::new()),
            vec![claude_code(), claude_desktop()],
        );
        let seen = Seen::new(&state, 0, &Utc, Os::MacOs);
        let said = |qualified: &str, then: After| {
            quit_asks(&seen, &asked(qualified, CLAUDE, "Claude"), &then)
        };
        assert_eq!(
            said("desktop/home", After::Switch),
            Question::asking(
                words::quit_to_switch("Claude"),
                words::quit_to_move("Claude", "home"),
                words::quit_and_switch(Os::MacOs, "Claude")
            )
        );
        let parks = |again| {
            Question::asking(
                words::quit_to_sign_in("Claude", again),
                words::quit_to_sign_out("Claude", "work"),
                words::quit_and_continue(Os::MacOs, "Claude"),
            )
        };
        assert_eq!(
            said(
                "desktop/",
                After::SignIn {
                    provider: "desktop".into(),
                    again: None,
                    from: Some(add_desktop()),
                }
            ),
            parks(None)
        );
        assert_eq!(
            said(
                "desktop/home",
                After::SignIn {
                    provider: "desktop".into(),
                    again: Some("home".into()),
                    from: Some(Sheet::SignInAgain {
                        provider: "desktop".into(),
                        label: "home".into(),
                    }),
                }
            ),
            parks(Some("home"))
        );
        assert_eq!(
            said(
                "desktop/",
                After::Enrol {
                    provider: "desktop".into(),
                    again: None,
                    from: Some(add_desktop()),
                }
            ),
            Question::asking(
                words::quit_to_sign_in("Claude", None),
                words::quit_to_read("Claude", None),
                words::quit_and_add(Os::MacOs, "Claude")
            )
        );
    }

    /// A failure's alert says what went wrong and everything else it warned about, a
    /// paragraph each.
    #[test]
    fn a_failures_alert_says_its_warnings_too() {
        let alert = failure_alert(
            Os::MacOs,
            &Failure {
                id: 1,
                title: "Couldn’t switch to personal".into(),
                message: "Nothing is parked.".into(),
                code: Some("nothing_parked".into()),
                account: None,
                warnings: vec![crate::Warning {
                    code: "auth_overridden".into(),
                    message: "ANTHROPIC_API_KEY is set".into(),
                    account: None,
                    held: None,
                    provider: None,
                }],
            },
        );
        assert_eq!(alert.title, "Couldn’t switch to personal");
        assert_eq!(
            alert.message,
            "Nothing is parked.\n\nANTHROPIC_API_KEY is set"
        );
    }

    // Each answer the window's questions take back is said with the question, so neither app
    // maps a button to an intent.
    #[test]
    fn the_windows_questions_come_with_their_answers() {
        let state = State::new(Cadence::APP);
        let seen = Seen::new(&state, 0, &Utc, Os::MacOs);
        let quit = quit_question(
            &seen,
            &asked("codex/work", "com.openai.codex", "ChatGPT"),
            &After::Switch,
        );
        assert_eq!(
            (quit.confirmed, quit.declined),
            (
                Intent::QuitAndSwitch {
                    qualified: "codex/work".into()
                },
                Intent::KeepAppOpen
            )
        );
        let replace = replace_question(&seen, "claude/work", "why");
        assert_eq!(
            (replace.confirmed, replace.declined),
            (
                Intent::ReplaceAndSwitch {
                    qualified: "claude/work".into()
                },
                Intent::KeepLogin
            )
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
        for os in [Os::MacOs, Os::Windows] {
            assert_eq!(titled(os), words::replace_title("Claude Code"));
        }
    }
}
