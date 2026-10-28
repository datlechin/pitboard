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
pub fn check_name(sheet: Sheet, tool: Tool, typed: String) -> NameCheck {
    let sends = |intent| NameCheck {
        problem: None,
        intent,
    };
    match sheet {
        Sheet::Add { .. } => sends(Some(Intent::SignIn { tool, again: None })),
        Sheet::SignInAgain { account } => sends(Some(Intent::SignIn {
            tool: account.tool,
            again: Some(account.label),
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
        Sheet::Rename { account } | Sheet::Name { account, .. } if account.label == name => {
            Ok(None)
        }
        _ => label::check(name).map(|()| Some(name.to_owned())),
    }
}

fn renames(sheet: Sheet, to: String) -> Option<Intent> {
    match sheet {
        Sheet::Rename { account } | Sheet::Name { account, .. } => {
            Some(Intent::Rename { account, to })
        }
        _ => None,
    }
}

/// The tool the sheet for `sheet` starts on: the one it is about, and for a new account
/// where nothing said which, the first Pitboard found.
pub(crate) fn starts_on(seen: &Seen, sheet: &Sheet) -> ProviderId {
    match sheet {
        Sheet::Add { tool } => tool
            .map(ProviderId::from)
            .or_else(|| {
                (seen.tools.iter())
                    .find(|&&tool| missing(seen, tool).is_none())
                    .or(seen.tools.first())
                    .copied()
            })
            .unwrap_or(ProviderId::Claude),
        Sheet::SignInAgain { account }
        | Sheet::Name { account, .. }
        | Sheet::Rename { account } => account.tool.into(),
        Sheet::Stow => ProviderId::Claude,
    }
}

// Unknown until the model has asked which tools are here, and then said only of one not found.
fn missing(seen: &Seen, tool: ProviderId) -> Option<Missing> {
    let found = seen.state.installed.as_ref()?;
    if found.contains(&tool) {
        return None;
    }
    Some(words::missing(seen.os, tool))
}

fn nothing_to_park(seen: &Seen, provider: ProviderId) -> bool {
    seen.in_use_of(provider).is_none()
        || seen
            .state
            .warnings
            .iter()
            .any(|warning| warning.code == "looks_signed_out" && warning.provider == Some(provider))
        || seen.state.refused_by(provider) == Some("live_credential_absent")
}

// The message, the default button and the steps of the form for a tool that signs in inside
// its app.
fn in_app_form(seen: &Seen, sheet: &Sheet, provider: ProviderId) -> (String, String, Vec<String>) {
    let app = provider.program();
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
    let signs_in = |again: Option<&str>| {
        (
            park.map(|work| words::signs_out_first(app, work, again))
                .unwrap_or_default(),
            words::CONTINUE.into(),
            words::in_app_steps(app, again, false),
        )
    };
    match sheet {
        Sheet::SignInAgain { account } if current => (
            words::add_shown(app, &account.label),
            words::add_as(&account.label),
            Vec::new(),
        ),
        Sheet::SignInAgain { account } => signs_in(Some(&account.label)),
        _ if current && first => (
            words::add_shown_first(app),
            words::add_account(seen.os, false),
            Vec::new(),
        ),
        _ if current => (
            match awaiting {
                Some(_) => words::add_once_signed_in(app),
                None => words::add_unidentified(app),
            },
            words::add_account_shown(seen.os, app, false),
            Vec::new(),
        ),
        _ => signs_in(None),
    }
}

// What a sign-in sheet says, its button and its steps, with `tool` picked.
fn form_words(seen: &Seen, sheet: &Sheet, tool: ProviderId) -> (String, String, Vec<String>) {
    if in_its_app(tool) {
        return in_app_form(seen, sheet, tool);
    }
    let again = match sheet {
        Sheet::SignInAgain { account } => Some(account.label.as_str()),
        _ => None,
    };
    (
        words::sign_in_message(tool.name(), again),
        words::sign_in_asking(seen.os),
        Vec::new(),
    )
}

fn sheet_tool(seen: &Seen, sheet: &Sheet, tool: ProviderId) -> SheetTool {
    let (message, confirm, steps) = form_words(seen, sheet, tool);
    SheetTool {
        tool: tool.into(),
        code: tool.code().into(),
        name: tool.name().into(),
        message,
        confirm,
        missing: missing(seen, tool),
        steps,
    }
}

// What the sheet says under its fields while it records the account an app shows.
fn progress(seen: &Seen) -> Option<String> {
    let (tool, step) = seen.state.recording()?;
    Some(words::recording(tool.program(), step))
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
            let (message, confirm, _) = form_words(seen, sheet, tool);
            SheetText {
                title: words::add_account(seen.os, false),
                message,
                tools: (seen.tools.iter().copied())
                    .filter(|&offered| offered == tool || missing(seen, offered).is_none())
                    .map(|offered| sheet_tool(seen, sheet, offered))
                    .collect(),
                tool: tool.into(),
                account: None,
                name: String::new(),
                prompt: words::NAME_PROMPT.into(),
                confirm,
                saving,
                progress,
            }
        }
        Sheet::SignInAgain { account } => {
            let (message, confirm, _) = form_words(seen, sheet, tool);
            SheetText {
                title: words::sign_in_to_again(seen.os, &account.label, false),
                message,
                tools: (seen.tools.iter().copied())
                    .filter(|&own| own == tool)
                    .map(|own| sheet_tool(seen, sheet, own))
                    .collect(),
                tool: tool.into(),
                account: Some(account.label.clone()),
                name: account.label.clone(),
                prompt: words::NAME_PROMPT.into(),
                confirm,
                saving,
                progress,
            }
        }
        Sheet::Name { account, email } => SheetText {
            title: words::name_this_account(seen.os),
            message: words::naming(email, tool.name()),
            tools: Vec::new(),
            account: None,
            name: account.label.clone(),
            tool: tool.into(),
            prompt: words::NAME_PROMPT.into(),
            confirm: words::DONE.into(),
            saving,
            progress: None,
        },
        Sheet::Rename { account } => SheetText {
            title: words::rename_title(&account.label),
            message: words::RENAMING.into(),
            tools: Vec::new(),
            tool: tool.into(),
            account: None,
            name: account.label.clone(),
            prompt: account.label.clone(),
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
    let unknown = || words::clean_up_message(seen.os);
    let (message, looking, can_confirm) = match state.left.as_ref() {
        None | Some(Leftover::Looking) => {
            (unknown(), Some(words::FINDING_OUT_WHOSE.to_owned()), false)
        }
        Some(Leftover::Unknown) => (unknown(), None, false),
        Some(Leftover::Found(None)) => (words::nothing_to_clean_up(seen.os), None, false),
        Some(Leftover::Found(Some(left))) => (
            words::clean_up_found(seen.os, left),
            None,
            !saving && !matches!(left.login, Foreseen::NotEnrolled(_)),
        ),
    };
    Some(StowText {
        title: words::clean_up_title(seen.os),
        message,
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
    let tool = signing.tool.name();
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
        steps: Vec::new(),
        title: words::signing_in_to(seen.os, tool),
        message: words::finish_in_browser(signing.again.as_deref()),
        code_note: words::code_note(tool),
        refused: signing.code_refused().then(|| words::code_refused(tool)),
    })
}

// It offers to put back the account the core parked for it, not the one last read in use.
fn in_app_signing_in_text(seen: &Seen, signing: &crate::model::state::SigningIn) -> SigningInText {
    let provider = signing.tool;
    let app = provider.program();
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
            words::in_app_note(app, signing.step),
            words::waiting_to_quit(app),
        )
    };
    let put_back = signing.put_back.as_ref().filter(|_| signing.started);
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
        undo: put_back.map(|account| Choice {
            title: words::switch_back_to(seen.os, &account.label),
            intent: Intent::SwitchTo {
                account: account.clone().into(),
            },
            enabled: !seen.state.switch_under_way(),
        }),
        announce,
        steps: words::in_app_steps(
            app,
            signing.again.as_deref(),
            signing.started && signing.opened_for_it,
        ),
    }
}

/// What a switch to `account` asks before it puts that account's login in place of a login
/// its tool has in use, which its service refuses for good, for `why`.
pub(crate) fn replace_question(account: &Key, why: &str) -> WindowQuestion {
    WindowQuestion {
        question: Question::asking(
            words::replace_title(account.provider.name()),
            why.to_owned(),
            words::REPLACE.into(),
        ),
        confirmed: Intent::ReplaceAndSwitch {
            account: account.clone().into(),
        },
        declined: Intent::KeepLogin,
    }
}

/// What the quit question asks, while it is asked, with its answers.
pub(crate) fn quit_question(seen: &Seen, question: &QuitQuestion, then: &After) -> WindowQuestion {
    WindowQuestion {
        question: quit_asks(seen, question, then),
        confirmed: Intent::QuitAndSwitch {
            account: question.account.clone().into(),
        },
        declined: Intent::KeepAppOpen,
    }
}

fn quit_asks(seen: &Seen, question: &QuitQuestion, then: &After) -> Question {
    let name = &question.name;
    let Key {
        provider: tool,
        label,
    } = &question.account;
    let tool = *tool;
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
            words::quit_to_move(name, label),
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
    super::next::of_failure(os, failure.code.as_deref(), failure.account.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AccountKey;

    fn rename(label: &str) -> Sheet {
        Sheet::Rename {
            account: AccountKey::new(Tool::Claude, label),
        }
    }

    fn saved(sheet: Sheet, typed: &str) -> Option<String> {
        saved_name(&sheet, typed).ok().flatten()
    }

    fn problem(sheet: Sheet, typed: &str) -> Option<String> {
        check_name(sheet, Tool::Claude, typed.into()).problem
    }

    fn named(label: &str) -> Sheet {
        Sheet::Name {
            account: AccountKey::new(Tool::Claude, label),
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
        let sends =
            |sheet: Sheet, tool: Tool, typed: &str| check_name(sheet, tool, typed.into()).intent;
        assert_eq!(
            sends(Sheet::Add { tool: None }, Tool::Codex, " travel"),
            Some(Intent::SignIn {
                tool: Tool::Codex,
                again: None,
            }),
            "the tool picked"
        );
        assert_eq!(
            sends(
                Sheet::SignInAgain {
                    account: AccountKey::new(Tool::Codex, "my work")
                },
                Tool::Codex,
                ""
            ),
            Some(Intent::SignIn {
                tool: Tool::Codex,
                again: Some("my work".into()),
            }),
            "the account's own name, whatever a new one may be"
        );
        let to_home = Some(Intent::Rename {
            account: AccountKey::new(Tool::Claude, "acme"),
            to: "home".into(),
        });
        assert_eq!(sends(named("acme"), Tool::Claude, "home"), to_home);
        assert_eq!(
            sends(named("acme"), Tool::Claude, " acme"),
            Some(Intent::CloseSheet)
        );
        assert_eq!(sends(rename("acme"), Tool::Claude, "home"), to_home);
        assert_eq!(
            sends(rename("acme"), Tool::Claude, "acme"),
            None,
            "unchanged"
        );
        assert_eq!(sends(named("acme"), Tool::Claude, "a/b"), None);
        assert_eq!(sends(Sheet::Stow, Tool::Claude, "work"), None);
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

    fn asked(account: Key, app_id: &str, name: &str) -> QuitQuestion {
        QuitQuestion {
            account,
            app_id: app_id.into(),
            name: name.into(),
        }
    }

    const CLAUDE: &str = "com.anthropic.claudefordesktop";

    fn add_desktop() -> Sheet {
        Sheet::Add {
            tool: Some(Tool::Desktop),
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
            &asked(
                Key::new(ProviderId::Codex, "work"),
                "com.openai.codex",
                "ChatGPT",
            ),
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
        let text = form(Sheet::Add { tool: None }, accounts, Vec::new());
        let sign_in = words::sign_in_asking(Os::MacOs);
        assert_eq!(
            text.tools
                .iter()
                .map(|tool| (tool.code.as_str(), tool.confirm.clone()))
                .collect::<Vec<_>>(),
            [
                ("claude", sign_in.clone()),
                ("desktop", words::CONTINUE.into())
            ]
        );
        assert_eq!(text.confirm, sign_in, "it starts on Claude Code");
    }

    // Plan 4.4: the picker shows only with two tools found, and a tool not found says how to
    // install it where the sheet starts on it.
    #[test]
    fn the_add_sheet_offers_the_tools_found() {
        let offered = |found: Vec<ProviderId>, sheet: Sheet| {
            let mut state = read(status(Vec::new(), Vec::new()), found);
            let text = form_of(&mut state, sheet);
            (text.tools.into_iter())
                .map(|tool| (tool.code, tool.missing.is_some()))
                .collect::<Vec<_>>()
        };
        let add = Sheet::Add { tool: None };
        assert_eq!(
            offered(vec![claude_code()], add.clone()),
            [("claude".into(), false)]
        );
        assert_eq!(
            offered(vec![claude_code(), claude_desktop()], add.clone()),
            [("claude".into(), false), ("desktop".into(), false)]
        );
        assert_eq!(
            offered(Vec::new(), add),
            [("claude".into(), true)],
            "nothing found: Claude Code, and how to install it"
        );
        assert_eq!(
            offered(
                vec![claude_code()],
                Sheet::Add {
                    tool: Some(Tool::Codex)
                }
            ),
            [("claude".into(), false), ("codex".into(), true)],
            "asked for by name"
        );
    }

    #[test]
    fn the_add_sheet_for_claude_desktop_says_what_it_does_in_each_case() {
        let case = |accounts: Vec<Account>, warnings: Vec<crate::Warning>| {
            let text = form(add_desktop(), accounts, warnings);
            let steps = (text.tools.iter())
                .find(|tool| tool.code == "desktop")
                .map(|tool| tool.steps.clone())
                .unwrap_or_default();
            (text.message, text.confirm, steps)
        };
        assert_eq!(
            case(Vec::new(), Vec::new()),
            (
                words::add_shown_first("Claude"),
                words::add_account(Os::MacOs, false),
                Vec::new()
            )
        );
        // Plan 4.4: three short steps with Continue, in place of a paragraph.
        let steps = words::in_app_steps("Claude", None, false);
        assert_eq!(steps.len(), 3);
        assert_eq!(
            case(work_and_home(), Vec::new()),
            (
                words::signs_out_first("Claude", "work", None),
                words::CONTINUE.into(),
                steps.clone()
            )
        );
        let signed_out = (String::new(), words::CONTINUE.into(), steps);
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
            Vec::new(),
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
        assert!(text.tools.iter().all(|tool| tool.steps.is_empty()));
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
        assert_eq!(form_of(&mut state, add_desktop()).confirm, words::CONTINUE);
        let seen = Seen::new(&state, 0, &Utc, Os::MacOs);
        let question = quit_asks(
            &seen,
            &asked(Key::new(ProviderId::Desktop, ""), CLAUDE, "Claude"),
            &After::SignIn {
                tool: ProviderId::Desktop,
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
            account: AccountKey::new(Tool::Desktop, "home"),
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
            &asked(Key::new(ProviderId::Desktop, "home"), CLAUDE, "Claude"),
            &After::Enrol {
                tool: ProviderId::Desktop,
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
        let said = |label: &str, then: After| {
            let account = Key::new(ProviderId::Desktop, label);
            quit_asks(&seen, &asked(account, CLAUDE, "Claude"), &then)
        };
        assert_eq!(
            said("home", After::Switch),
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
                "",
                After::SignIn {
                    tool: ProviderId::Desktop,
                    again: None,
                    from: Some(add_desktop()),
                }
            ),
            parks(None)
        );
        assert_eq!(
            said(
                "home",
                After::SignIn {
                    tool: ProviderId::Desktop,
                    again: Some("home".into()),
                    from: Some(Sheet::SignInAgain {
                        account: AccountKey::new(Tool::Desktop, "home")
                    }),
                }
            ),
            parks(Some("home"))
        );
        assert_eq!(
            said(
                "",
                After::Enrol {
                    tool: ProviderId::Desktop,
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
            &asked(
                Key::new(ProviderId::Codex, "work"),
                "com.openai.codex",
                "ChatGPT",
            ),
            &After::Switch,
        );
        assert_eq!(
            (quit.confirmed, quit.declined),
            (
                Intent::QuitAndSwitch {
                    account: AccountKey::new(Tool::Codex, "work")
                },
                Intent::KeepAppOpen
            )
        );
        let replace = replace_question(&Key::new(ProviderId::Claude, "work"), "why");
        assert_eq!(
            (replace.confirmed, replace.declined),
            (
                Intent::ReplaceAndSwitch {
                    account: AccountKey::new(Tool::Claude, "work")
                },
                Intent::KeepLogin
            )
        );
    }

    #[test]
    fn the_replace_question_is_titled_as_each_system_titles_one() {
        let replace = replace_question(&Key::new(ProviderId::Claude, "work"), "why");
        assert_eq!(replace.question.title, words::replace_title("Claude Code"));
    }
}
