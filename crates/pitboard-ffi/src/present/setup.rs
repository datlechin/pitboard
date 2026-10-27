//! How far setting Pitboard up this machine is, and what the window and the menu say in place
//! of a list, from AppModel.swift's footing, AccountsPane.swift and MenuBarContent.swift.

use super::accounts::in_order;
use super::notices::INSTALL_CLAUDE_CODE;
use super::words;
use super::{AccountsShown, Choice, Footing, Seen, SetupStep};
use crate::model::state::in_its_app;
use crate::model::{Intent, Sheet, Tip};
use pitboard_core::provider::ProviderId;

/// How far along setting Pitboard up this machine is, worked out across every tool.
pub(crate) fn footing(seen: &Seen) -> Footing {
    // Before the first read there is nothing to go on, and guessing at this point shows
    // somebody a setup step they may have finished years ago.
    let Some(status) = &seen.state.status else {
        return Footing::Ready;
    };
    let accounts = &status.accounts;
    // Said only where nothing else is: a login or an account means a tool is here however it
    // was installed, and a program the app could not find may still be. The core's read does
    // not fail for a missing tool, so this is the one place it shows.
    if accounts.is_empty() && seen.state.installed.as_ref().is_some_and(Vec::is_empty) {
        return Footing::NoClaudeCode;
    }
    let codex = codex_unused(seen);
    if !accounts.iter().any(|account| account.signed_in) {
        // Enrolled accounts with nobody signed in is a machine mid-switch or one whose login
        // was signed out from elsewhere, not a machine that needs setting up.
        return if !accounts.is_empty() {
            Footing::Ready
        } else if codex {
            Footing::CodexUnused
        } else {
            Footing::NoOneSignedIn
        };
    }
    // Not before the preferences are in, which say which tools somebody declined it for: a
    // read that lands first would show the step to them for as long as the file takes.
    if seen.state.reading_preferences() {
        return Footing::Ready;
    }
    // Per tool: an account can only be switched to another account of its own tool.
    let providers: Vec<&str> = accounts.iter().map(|a| a.provider.as_str()).collect();
    let declined = &seen.state.preferences.second_account_declined;
    for provider in in_order(&providers, &seen.tools) {
        if declined.contains(&provider) {
            continue;
        }
        let enrolled: Vec<_> = accounts
            .iter()
            .filter(|account| account.provider == provider && account.label.is_some())
            .collect();
        if let [only] = enrolled[..]
            && only.signed_in
            && let Some(label) = &only.label
        {
            return Footing::OnlyOne {
                provider,
                label: label.clone(),
            };
        }
    }
    if codex {
        return Footing::CodexUnused;
    }
    let preferences = &seen.state.preferences;
    let switching = accounts
        .iter()
        .filter(|account| account.provider == ProviderId::Claude.code() && account.label.is_some())
        .count();
    if switching > 1 && !preferences.at_limit_chosen() && !preferences.declined(Tip::AtLimit) {
        return Footing::AtLimitUnchosen;
    }
    Footing::Ready
}

fn found(seen: &Seen, tool: ProviderId) -> bool {
    (seen.state.installed.as_ref())
        .is_some_and(|found| found.iter().any(|here| here.code == tool.code()))
}

// Whose login Codex stores is read only once the tip is taken up.
fn codex_unused(seen: &Seen) -> bool {
    let codex = ProviderId::Codex.code();
    !seen.state.reading_preferences()
        && !seen.state.preferences.declined(Tip::UseCodex)
        && found(seen, ProviderId::Codex)
        && !(seen.accounts().iter())
            .any(|account| account.provider == codex && account.label.is_some())
}

/// The one next thing to do on a machine that is not set up yet. Somebody who opens the app
/// has usually never run a command and may never want to.
pub(crate) fn step(seen: &Seen, footing: &Footing) -> Option<SetupStep> {
    match footing {
        Footing::OnlyOne { provider, label } => {
            let in_its_app = in_its_app(provider);
            let named = if in_its_app {
                Some(seen.tool_name(provider))
            } else {
                seen.tool_if_shown(provider)
            };
            let app = in_its_app.then(|| seen.program_of(provider));
            Some(SetupStep {
                title: words::add_a_second(named.as_deref()),
                detail: words::only_one(label, named.as_deref(), app),
                actions: vec![
                    Choice {
                        title: words::add_account(seen.os, true),
                        intent: Intent::PresentSheet {
                            sheet: Sheet::Add {
                                provider: Some(provider.clone()),
                            },
                        },
                        enabled: true,
                    },
                    // Somebody may keep one account on purpose and watch its limits, so the
                    // nudge can be declined, for its tool alone.
                    Choice {
                        title: words::not_now(seen.os),
                        intent: Intent::DeclineSecondAccount {
                            provider: provider.clone(),
                        },
                        enabled: true,
                    },
                ],
            })
        }
        Footing::CodexUnused => Some(SetupStep {
            title: words::USE_WITH_CODEX.into(),
            detail: words::CODEX_UNUSED.into(),
            actions: vec![
                Choice {
                    title: words::add_codex_account(seen.os),
                    intent: Intent::Enrol {
                        provider: ProviderId::Codex.code().into(),
                    },
                    enabled: true,
                },
                not_now(seen, Tip::UseCodex),
            ],
        }),
        Footing::AtLimitUnchosen => Some(SetupStep {
            title: words::SWITCH_BEFORE_RUNNING_OUT.into(),
            detail: words::at_limit_tip(seen.state.preferences.threshold().percent()),
            // The settings say what the plans' terms may say of switching by itself.
            actions: vec![
                Choice {
                    title: words::open_settings(seen.os),
                    intent: Intent::ShowLimits,
                    enabled: true,
                },
                not_now(seen, Tip::AtLimit),
            ],
        }),
        Footing::NoClaudeCode | Footing::NoOneSignedIn | Footing::Ready => None,
    }
}

fn not_now(seen: &Seen, tip: Tip) -> Choice {
    Choice {
        title: words::not_now(seen.os),
        intent: Intent::DeclineTip { tip },
        enabled: true,
    }
}

/// What the accounts pane shows: a list, or why there is none and what to do about it.
///
/// A failed read with nothing to list is the pane's whole content, with why and a way to try
/// again, and not a spinner that never stops or a list with nothing in it, whether nothing is
/// known at all or what is known is empty. A read with nothing in it says No Accounts, under
/// any tip, and a machine without Claude Code says that instead.
pub(crate) fn accounts_shown(seen: &Seen, footing: &Footing) -> AccountsShown {
    let names: Vec<&str> = seen.tools.iter().map(|tool| tool.name.as_str()).collect();
    let names = pitboard_core::words::listed(&names);
    match footing {
        Footing::NoClaudeCode => AccountsShown::NoTool {
            title: words::claude_code_not_installed(seen.os),
            detail: words::no_tool(&names),
            link_title: words::how_to_install_claude_code(seen.os),
            link: INSTALL_CLAUDE_CODE.into(),
        },
        _ if seen.problem().is_some() && seen.accounts().is_empty() => AccountsShown::ReadFailed {
            title: words::could_not_read_accounts(seen.os),
            detail: seen.problem().unwrap_or_default().to_owned(),
            // Held back while the accounts are read again, as the toolbar's Refresh is, so
            // pressing it is seen to have done something; AccountsPane.swift held it back.
            retry: Choice {
                title: words::try_again(seen.os),
                intent: Intent::Refresh { asked: true },
                enabled: seen.state.reads == 0,
            },
        },
        _ if (seen.state.status.as_ref()).is_some_and(|read| read.accounts.is_empty()) => {
            AccountsShown::NoAccounts {
                title: words::no_accounts(seen.os).0,
                detail: words::no_accounts(seen.os).1.into(),
                add: Choice {
                    title: words::add_account(seen.os, true),
                    intent: Intent::PresentSheet {
                        sheet: Sheet::Add { provider: None },
                    },
                    enabled: true,
                },
            }
        }
        _ if seen.state.status.is_none() => AccountsShown::Reading {
            title: words::READING_ACCOUNTS.into(),
        },
        _ => AccountsShown::List,
    }
}

/// What the menu says where it has no accounts to list.
pub(crate) fn menu_accounts_note(seen: &Seen, footing: &Footing) -> Option<String> {
    match &seen.state.status {
        None if seen.problem().is_none() => Some(words::READING_ACCOUNTS.into()),
        None => Some(words::no_accounts_to_show(seen.os)),
        Some(status) if status.accounts.is_empty() && *footing != Footing::NoClaudeCode => {
            Some(words::no_accounts_yet(seen.os))
        }
        Some(_) => None,
    }
}

/// When the numbers were read, as the menu's Refresh item says under it: as a time rather
/// than an age, since a menu can stay open, and "just now" would still say so ten minutes
/// later.
pub(crate) fn updated_menu(seen: &Seen) -> String {
    if seen.state.reads > 0 {
        return words::READING.into();
    }
    match seen.state.updated_ms {
        Some(ms) => words::updated(&seen.clock(ms.div_euclid(1000))),
        None if seen.problem().is_none() => words::NOT_READ_YET.into(),
        None => words::SHOWING_LAST_MEASURED.into(),
    }
}

/// The same, as the window's subtitle says it: nothing before the first read.
pub(crate) fn updated_window(seen: &Seen) -> String {
    if seen.state.reads > 0 {
        return words::READING.into();
    }
    seen.state
        .updated_ms
        .map(|ms| words::updated(&seen.clock(ms.div_euclid(1000))))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use crate::present::testing::{Utc, claude_desktop, desktop_account, read, status};
    use crate::present::{AccountsShown, present_on, words};
    use pitboard_core::host::Os;

    #[test]
    fn one_claude_desktop_account_is_offered_a_second_in_its_own_words() {
        let state = read(
            status(vec![desktop_account("work", true)], Vec::new()),
            vec![claude_desktop()],
        );
        let step = present_on(Os::MacOs, &state, 0, &Utc)
            .setup
            .expect("a second account offered");
        assert_eq!(step.title, words::add_a_second(Some("Claude Desktop")));
        assert_eq!(
            step.detail,
            words::only_one("work", Some("Claude Desktop"), Some("Claude"))
        );
    }

    #[test]
    fn a_mac_with_no_tool_lists_every_tool() {
        let state = read(status(Vec::new(), Vec::new()), Vec::new());
        let AccountsShown::NoTool { detail, .. } =
            present_on(Os::MacOs, &state, 0, &Utc).accounts_shown
        else {
            panic!("no tool here");
        };
        assert_eq!(
            detail,
            words::no_tool("Claude Code, Codex and Claude Desktop")
        );
    }
}
