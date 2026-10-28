// What to do about a failure, a notice or a note: an action in the app, or the docs about its code.

use super::{Choice, NextStep, SetupLink, words};
use crate::Front;
use crate::model::{Intent, Place, Sheet};
use pitboard_core::provider::ProviderId;
use pitboard_core::state::Key;

const ERRORS: &str = "https://docs.usepitboard.com/reference/errors";

// Its table says what each of doctor's checks checks, and where a failed one is put right.
const CHECKS: &str = "https://docs.usepitboard.com/reference/maintenance-commands#doctor";

// A failure's next step, by its code and the account it is about; with no code, it was lost.
pub(crate) fn of_failure(front: Front, code: Option<&str>, account: Option<&Key>) -> NextStep {
    let act = |title: String, intent: Intent| NextStep::Act {
        choice: Choice {
            title,
            intent,
            enabled: true,
        },
    };
    let Some(code) = code else {
        return act(words::REFRESH.into(), Intent::Refresh { asked: true });
    };
    match (code, account) {
        (
            "nothing_parked"
            | "parked_login_expired"
            | "parked_login_refused"
            | "parked_credential_missing"
            | "parked_credential_corrupt"
            | "parked_login_belongs_elsewhere"
            | "identity_unverifiable"
            | "sign_in_not_kept",
            Some(key),
        ) => act(
            words::sign_in_again(front, true),
            Intent::PresentSheet {
                sheet: Sheet::SignInAgain {
                    account: key.clone().into(),
                },
            },
        ),
        ("account_unknown", key) => act(
            words::add_account(front, true),
            Intent::PresentSheet {
                sheet: Sheet::Add {
                    tool: key.map(|key| key.provider.into()),
                },
            },
        ),
        ("live_account_not_enrolled", Some(key)) => act(
            words::add_account(front, true),
            Intent::Enrol {
                tool: key.provider.into(),
            },
        ),
        // Pitboard asked ChatGPT to quit before a switch, and it stayed open.
        ("app_open", Some(key)) if key.provider != ProviderId::Desktop => act(
            words::try_again(front),
            Intent::SwitchTo {
                account: key.clone().into(),
            },
        ),
        (
            "home_unwritable"
            | "state_unreadable"
            | "state_corrupt"
            | "state_write_failed"
            | "credential_store_unreadable"
            | "credential_store_locked"
            | "credential_sealed"
            | "credential_not_json"
            | "credential_write_failed"
            | "credential_not_durable"
            | "park_slot_exhausted"
            | "live_credential_shape_unexpected"
            | "recovery_record_corrupt"
            | "recovery_failed"
            | "claude_config_unreadable"
            | "claude_config_not_json",
            _,
        ) => act(
            words::show_diagnostics(front),
            Intent::Show {
                place: Some(Place::Checks),
            },
        ),
        (code, key) => {
            let desktop = key.is_some_and(|key| key.provider == ProviderId::Desktop);
            NextStep::Learn {
                link: learn_more(front, error_section(code, desktop)),
            }
        }
    }
}

// The error page's part about a warning's code.
pub(crate) fn of_warning(front: Front) -> SetupLink {
    learn_more(front, Some("warning-codes"))
}

// The error page's part about why a row's numbers are old or its account can't be used.
pub(crate) fn of_stale(front: Front) -> SetupLink {
    learn_more(front, Some("stale-codes"))
}

// A check nothing in the app puts right.
pub(crate) fn of_check(front: Front) -> SetupLink {
    SetupLink {
        title: words::learn_more(front),
        link: CHECKS.into(),
    }
}

fn learn_more(front: Front, section: Option<&str>) -> SetupLink {
    SetupLink {
        title: words::learn_more(front),
        link: section.map_or_else(|| ERRORS.into(), |section| format!("{ERRORS}#{section}")),
    }
}

// The part of docs/reference/errors.mdx that lists `code`, Claude Desktop's for its accounts.
fn error_section(code: &str, desktop: bool) -> Option<&'static str> {
    const DESKTOP: [&str; 6] = [
        "live_store_unsupported",
        "live_credential_absent",
        "parked_credential_corrupt",
        "credential_store_unreadable",
        "recovery_undetermined",
        "recovery_elsewhere",
    ];
    if desktop && DESKTOP.contains(&code) {
        return Some("claude-desktop");
    }
    Some(match code {
        "account_unknown"
        | "label_taken"
        | "already_enrolled"
        | "label_ambiguous"
        | "provider_unknown"
        | "live_account_not_enrolled"
        | "cannot_forget_active_account"
        | "account_not_in_use"
        | "usage" => "accounts-and-labels",
        "nothing_parked"
        | "parked_login_expired"
        | "parked_login_refused"
        | "parked_login_belongs_elsewhere"
        | "parked_credential_missing"
        | "parked_credential_corrupt"
        | "park_slot_exhausted"
        | "renewal_failed"
        | "credential_too_large"
        | "vault_form_unsupported" => "parked-logins",
        "switch_rolled_back"
        | "switch_did_not_hold"
        | "switch_unverified"
        | "switch_corrupted"
        | "signed_in_account_changed"
        | "identity_unverifiable"
        | "live_login_refused"
        | "account_on_hold"
        | "replace_failed"
        | "install_failed"
        | "session_expired" => "switching",
        // The app's own: the sign-in it started ended under it.
        "sign_in_gone"
        | "claude_program_missing"
        | "codex_program_missing"
        | "claude_not_found"
        | "codex_not_found"
        | "sign_in_incomplete"
        | "sign_in_not_isolated"
        | "sign_in_in_progress"
        | "sign_in_pinned_to_console"
        | "sign_in_not_installed"
        | "sign_in_not_kept" => "signing-in",
        "recovery_undetermined"
        | "recovery_elsewhere"
        | "recovery_failed"
        | "recovery_record_corrupt" => "interrupted-switches",
        "home_not_absolute"
        | "state_on_synced_drive"
        | "state_unreadable"
        | "state_corrupt"
        | "state_from_newer_version"
        | "state_version_unknown"
        | "state_names_unknown_tool"
        | "state_wrong_machine"
        | "state_write_failed"
        | "home_unwritable"
        | "pitboard_open" => "the-pitboard-directory",
        "claude_config_missing"
        | "claude_config_unreadable"
        | "claude_config_not_json"
        | "live_credential_absent"
        | "live_credential_elsewhere"
        | "live_credential_shape_unexpected"
        | "live_store_unsupported"
        | "login_restricted"
        | "codex_home_missing"
        | "custom_oauth_endpoint"
        | "left_login_changed"
        | "left_login_not_enrolled"
        | "left_login_unidentified"
        | "stored_login_changed" => "claude-code-and-codex-files",
        "credential_store_unreadable"
        | "credential_store_locked"
        | "credential_sealed"
        | "credential_not_json"
        | "credential_write_failed"
        | "credential_not_durable"
        | "switch_in_progress"
        | "lock_unavailable" => "credential-stores-and-locks",
        "app_open"
        | "app_state_unknown"
        | "login_folder_busy"
        | "login_folder_unknown"
        | "login_folder_unreadable"
        | "sign_in_unfinished"
        | "login_in_doubt"
        | "different_volume"
        | "parks_not_private"
        | "machine_unknown"
        | "app_opened_midway"
        | "move_undone"
        | "move_stopped"
        | "switch_left_signed_out"
        | "move_mixed"
        | "uninstall_would_strand"
        | "sign_in_settled_elsewhere" => "claude-desktop",
        "schedule_unsupported"
        | "schedule_not_default_home"
        | "schedule_refused"
        | "schedule_manager_elsewhere"
        | "schedule_program_missing"
        | "schedule_program_temporary"
        | "schedule_program_unnamed" => "renewal-schedule",
        "warm_up_unsupported"
        | "warm_up_not_started"
        | "warm_up_limit_reached"
        | "warm_up_login_refused"
        | "warm_up_unreachable"
        | "warm_up_timed_out"
        | "warm_up_failed"
        | "warm_up_auth_overridden"
        | "warm_up_in_progress"
        | "warm_setting_unreadable"
        | "warm_setting_corrupt" => "warm-up",
        "elevated" => "root-sudo-and-administrator",
        "system_too_old" | "windows_not_released" => "windows",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AccountKey;
    use crate::Tool;

    // As Mintlify anchors a heading.
    fn anchor(heading: &str) -> String {
        heading
            .to_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-'))
            .map(|c| if c == ' ' { '-' } else { c })
            .collect()
    }

    // Each error code the page lists links to its part, Claude Desktop's for its accounts.
    #[test]
    fn each_code_on_the_error_page_links_to_the_part_that_lists_it() {
        let page = include_str!("../../../../docs/reference/errors.mdx");
        let mut part = String::new();
        let mut first: Vec<(String, String)> = Vec::new();
        for line in page.lines() {
            if let Some(heading) = line.strip_prefix("## ") {
                part = anchor(heading);
                continue;
            }
            let Some(cell) = line
                .strip_prefix("| ")
                .and_then(|rest| rest.split_once(" |"))
                .map(|(cell, _)| cell)
                .filter(|cell| cell.starts_with('`'))
            else {
                continue;
            };
            if matches!(
                part.as_str(),
                "stale-codes" | "cause-codes" | "warning-codes" | "command-output"
            ) {
                continue;
            }
            let codes = cell.split(", ").map(|code| code.trim_matches('`'));
            for code in
                codes.filter(|code| code.chars().all(|c| c == '_' || c.is_ascii_lowercase()))
            {
                if part == "claude-desktop" {
                    assert_eq!(error_section(code, true), Some("claude-desktop"), "{code}");
                }
                if !first.iter().any(|(seen, _)| seen == code) {
                    first.push((code.to_owned(), part.clone()));
                }
            }
        }
        assert!(first.len() > 100, "{}", first.len());
        for (code, part) in first {
            assert_eq!(error_section(&code, false), Some(part.as_str()), "{code}");
        }
        assert!(page.contains("\n## Warning codes\n"));
        assert!(page.contains("\n## Stale codes\n"));
    }

    #[test]
    fn a_failure_the_app_can_do_something_about_offers_it() {
        let act = |code, account: Option<&Key>| match of_failure(
            Front::MacMenuBar,
            Some(code),
            account,
        ) {
            NextStep::Act { choice } => Some(choice.intent),
            NextStep::Learn { .. } => None,
        };
        assert_eq!(
            act(
                "parked_login_expired",
                Some(&Key::new(ProviderId::Codex, "work"))
            ),
            Some(Intent::PresentSheet {
                sheet: Sheet::SignInAgain {
                    account: AccountKey::new(Tool::Codex, "work")
                },
            })
        );
        assert_eq!(
            act("state_unreadable", None),
            Some(Intent::Show {
                place: Some(Place::Checks),
            })
        );
        assert_eq!(
            act(
                "live_account_not_enrolled",
                Some(&Key::new(ProviderId::Claude, ""))
            ),
            Some(Intent::Enrol { tool: Tool::Claude })
        );
        assert_eq!(
            of_failure(Front::WindowsNotificationArea, None, None),
            NextStep::Act {
                choice: Choice {
                    title: words::REFRESH.into(),
                    intent: Intent::Refresh { asked: true },
                    enabled: true,
                },
            }
        );
        assert_eq!(
            of_failure(
                Front::MacMenuBar,
                Some("app_open"),
                Some(&Key::new(ProviderId::Desktop, "home"))
            ),
            NextStep::Learn {
                link: SetupLink {
                    title: words::learn_more(Front::MacMenuBar),
                    link: format!("{ERRORS}#claude-desktop"),
                },
            }
        );
        assert_eq!(
            of_failure(
                Front::WindowsNotificationArea,
                Some("app_open"),
                Some(&Key::new(ProviderId::Codex, "work"))
            ),
            NextStep::Act {
                choice: Choice {
                    title: words::try_again(Front::WindowsNotificationArea),
                    intent: Intent::SwitchTo {
                        account: AccountKey::new(Tool::Codex, "work")
                    },
                    enabled: true,
                },
            }
        );
        assert_eq!(
            of_failure(Front::WindowsNotificationArea, Some("not_a_code_yet"), None),
            NextStep::Learn {
                link: SetupLink {
                    title: words::learn_more(Front::WindowsNotificationArea),
                    link: ERRORS.into(),
                },
            }
        );
    }
}
