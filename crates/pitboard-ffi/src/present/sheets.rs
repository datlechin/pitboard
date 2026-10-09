//! What the sheets and the window's questions say, from NameSheets.swift, SignInSheet.swift
//! and MainWindow.swift: the sheet over the window, a sign-in under way, the sheet for putting
//! away a login left in a file, the question about quitting an app to switch, and a failure's
//! alert.

use super::words;
use super::{Question, Seen, SheetText, SheetTool, SigningInText, StowText, alert_of};
use crate::Tool;
use crate::account_windows::AlertText;
use crate::model::state::{Leftover, trimmed};
use crate::model::{Failure, QuitQuestion, Sheet};
use pitboard_core::switch::Foreseen;
use pitboard_core::words as said;

/// The name a sheet would save or sign in as, from what was typed in it, or `None` while
/// there is nothing to save: what was typed without the white space around it, as the Swift
/// sheets trimmed it with Foundation's `whitespacesAndNewlines`, and not empty. A rename needs
/// a name it does not already have. Signing in again needs no name typed: it is the
/// account's own. Putting away a login left in a file saves no name at all.
///
/// The model saves by the same rule, so a sheet that offers Save as this says is never
/// refused for what is in its field. Answers at once from what it is given, so a view asks it
/// as it is typed in.
#[uniffi::export]
pub fn name_to_save(sheet: Sheet, typed: String) -> Option<String> {
    if let Sheet::SignInAgain { label, .. } = sheet {
        return Some(label);
    }
    let name = trimmed(&typed);
    match &sheet {
        Sheet::Stow => None,
        _ if name.is_empty() => None,
        Sheet::Rename { label, .. } if label == name => None,
        _ => Some(name.to_owned()),
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
/// where nothing said which, the first it offers.
pub(crate) fn starts_on(sheet: &Sheet, addable: &[Tool]) -> String {
    match sheet {
        Sheet::Add { provider } => provider
            .clone()
            .or_else(|| addable.first().map(|tool| tool.code.clone()))
            .unwrap_or_else(|| {
                pitboard_core::provider::ProviderId::Claude
                    .code()
                    .to_owned()
            }),
        Sheet::SignInAgain { provider, .. }
        | Sheet::Name { provider, .. }
        | Sheet::Rename { provider, .. } => provider.clone(),
        Sheet::Stow => pitboard_core::provider::ProviderId::Claude
            .code()
            .to_owned(),
    }
}

/// Why a tool is missing from the sheet for a new account, rather than leaving it out without
/// a word. `None` when every tool is offered.
pub(crate) fn not_offered(seen: &Seen, addable: &[Tool]) -> Option<String> {
    let missing: Vec<&Tool> = seen
        .tools
        .iter()
        .filter(|tool| !addable.contains(tool))
        .collect();
    if missing.is_empty() {
        return None;
    }
    let names: Vec<&str> = missing.iter().map(|tool| tool.name.as_str()).collect();
    let programs: Vec<&str> = missing.iter().map(|tool| tool.program.as_str()).collect();
    Some(words::not_offered(seen.os, &names, &programs))
}

/// What the sheet over the main window says, while one is up that names an account. The
/// sheet for putting away a login left in a file says `stow_text` instead.
pub(crate) fn sheet_text(seen: &Seen) -> Option<SheetText> {
    let state = seen.state;
    let sheet = state.sheet.as_ref()?;
    let addable = seen.addable();
    let tool = starts_on(sheet, &addable);
    let saving = state.saving.contains(sheet);
    let text = match sheet {
        Sheet::Add { .. } => SheetText {
            title: "Add Account".into(),
            message: sign_in_message(&seen.tool_name(&tool), None),
            tools: if addable.len() > 1 {
                addable
                    .iter()
                    .map(|offered| SheetTool {
                        code: offered.code.clone(),
                        name: offered.name.clone(),
                        message: sign_in_message(&offered.name, None),
                    })
                    .collect()
            } else {
                Vec::new()
            },
            tool,
            account: None,
            name: String::new(),
            prompt: "work".into(),
            not_offered: not_offered(seen, &addable),
            confirm: "Sign In".into(),
            saving,
        },
        Sheet::SignInAgain { label, .. } => SheetText {
            title: format!("Sign In to {label} Again"),
            message: sign_in_message(&seen.tool_name(&tool), Some(label)),
            tools: Vec::new(),
            tool,
            account: Some(label.clone()),
            name: label.clone(),
            prompt: "work".into(),
            not_offered: None,
            confirm: "Sign In".into(),
            saving,
        },
        Sheet::Name { email, .. } => SheetText {
            title: "Name This Account".into(),
            message: format!(
                "{email} is signed in to {}. Pitboard parks its login under this name whenever \
                 you switch to another account.",
                seen.tool_name(&tool)
            ),
            tools: Vec::new(),
            tool,
            account: None,
            name: String::new(),
            prompt: "work".into(),
            not_offered: None,
            confirm: "Save".into(),
            saving,
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
            not_offered: None,
            confirm: "Rename".into(),
            saving,
        },
        Sheet::Stow => return None,
    };
    Some(text)
}

/// What the sheet for putting away the login left in a file says while it is up: that it is
/// looking, then what the look found, in the words `pitboard stow` asks with. Put Away waits
/// for a login it can put away, which a login of an account nobody enrolled is not.
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
            said::left_lines(left),
            None,
            !saving && !matches!(left.login, Foreseen::NotEnrolled(_)),
        ),
    };
    Some(StowText {
        title: "Put Away the Login Left in a File".into(),
        message: "Claude Code left a login in a file behind the keychain, which keeps sessions \
                  already running from following a switch. Pitboard keeps that login for its \
                  account where it holds none it can switch to, then deletes the file."
            .into(),
        lines,
        looking,
        confirm: "Put Away".into(),
        can_confirm,
        saving,
    })
}

/// What a sign-in under way says, whichever sheet started it: the model runs one at a time.
pub(crate) fn signing_in_text(seen: &Seen) -> Option<SigningInText> {
    let signing = seen.state.shown_sign_in()?;
    let tool = seen.tool_name(&signing.provider);
    Some(SigningInText {
        title: format!("Signing In to {tool}"),
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

/// The question about quitting an app that holds a tool's login before switching.
pub(crate) fn quit_confirmation(question: &QuitQuestion) -> Question {
    let name = &question.name;
    Question {
        title: format!("Quit {name} to switch?"),
        message: format!(
            "{name} keeps using the account it started with until it quits. Pitboard quits \
             it, switches, and opens it again."
        ),
        confirm: format!("Quit {name} and Switch"),
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

    /// A name is saved without the white space around it, and Save waits for there to be a
    /// name: in a rename, one the account does not already have.
    ///
    /// NameSheets.swift's Save and Rename buttons, and SignInSheet.swift's Sign In, which the
    /// Swift tested nowhere.
    #[test]
    fn a_name_is_saved_trimmed_and_only_once_there_is_one() {
        let name = Sheet::Name {
            provider: "claude".into(),
            email: "a@example.com".into(),
        };
        assert_eq!(
            name_to_save(name.clone(), "  work\n".into()).as_deref(),
            Some("work")
        );
        assert_eq!(name_to_save(name.clone(), " \t\u{200b}".into()), None);
        assert_eq!(name_to_save(name, String::new()), None);
        assert_eq!(
            name_to_save(rename("work"), " work ".into()),
            None,
            "unchanged"
        );
        assert_eq!(
            name_to_save(rename("work"), "home ".into()).as_deref(),
            Some("home")
        );
        assert_eq!(
            name_to_save(Sheet::Add { provider: None }, " travel".into()).as_deref(),
            Some("travel")
        );
        assert_eq!(
            name_to_save(
                Sheet::SignInAgain {
                    provider: "codex".into(),
                    label: "work".into()
                },
                String::new()
            )
            .as_deref(),
            Some("work"),
            "the account's own name"
        );
    }

    /// The question about quitting an app to switch names the app, and so does its button.
    #[test]
    fn the_question_about_quitting_names_the_app() {
        let asked = quit_confirmation(&QuitQuestion {
            qualified: "codex/work".into(),
            app_id: "com.openai.codex".into(),
            name: "ChatGPT".into(),
        });
        assert_eq!(asked.title, "Quit ChatGPT to switch?");
        assert_eq!(asked.confirm, "Quit ChatGPT and Switch");
        assert_eq!(
            asked.message,
            "ChatGPT keeps using the account it started with until it quits. Pitboard quits \
             it, switches, and opens it again."
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
                file_holds: None,
            }],
        });
        assert_eq!(alert.title, "Couldn’t switch to personal");
        assert_eq!(
            alert.message,
            "Nothing is parked.\n\nANTHROPIC_API_KEY is set"
        );
    }
}
