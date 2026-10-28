//! Doctor's checks as Diagnostics shows them, in the apps' words, and doctor's own report of
//! them, which Copy Report copies as `pitboard doctor --json` gives it.

use super::accounts::{item, offer_of};
use super::{ActionKind, ItemOffer, Seen, SetupLink, next, words};
use crate::model::{Intent, Place, SettingsSection, Sheet};
use crate::{Check, Held, Level};
use pitboard_core::provider::ProviderId;
use pitboard_core::state::Key;

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChecksShown {
    // Pitboard's own first, then each tool's in the order the apps list tools
    pub groups: Vec<CheckGroup>,
    // "2 Issues", once there are checks to head
    pub summary: Option<String>,
    // under it: "Everything else Pitboard checks is in order."
    pub verdict: Option<String>,
    // the worst of the checks, which the summary shows in its colour and symbol
    pub level: Level,
    // Check Again holds back while a check runs
    pub checking: bool,
    // "Checked at 14:05", once nothing is checking
    pub checked: Option<String>,
    // "Checking this Mac…", in place of the checks before there are any
    pub waiting: Option<String>,
    // what Copy Report copies: `pitboard doctor --json`'s data, redacted, once checked
    pub report: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CheckGroup {
    // "pitboard", or a tool's code
    pub id: String,
    pub heading: String,
    pub lines: Vec<CheckLine>,
}

// Not called `CheckRow`, which DiagnosticsWindow.swift has a view of.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CheckLine {
    // its place in doctor's order: one code names a check of each account
    pub id: u64,
    pub name: String,
    pub level: Level,
    // "Passed", "Warning", "Failed", where a symbol shows the level
    pub spoken_level: String,
    pub detail: String,
    // what to do, for a check that did not pass and says what
    pub advice: Option<String>,
    // what does something about it in the app, for a check that did not pass
    pub action: Option<ItemOffer>,
    // where nothing in the app does, the docs about the checks
    pub learn: Option<SetupLink>,
}

// A check that passed shows only where a person reads it; Copy Report keeps every one.
const SHOWN_WHEN_PASSED: [&str; 7] = [
    "security_tool",
    "home",
    "network",
    "in_use",
    "codex_login",
    "desktop_login",
    "schedule",
];

pub(crate) fn checks(seen: &Seen) -> ChecksShown {
    let machine = &seen.state.machine;
    let checking = machine.checking > 0;
    let any = !machine.checks.is_empty();
    // Doctor cannot know what the app's system refused, so the app says it after doctor's checks.
    let refused = machine
        .post_refused
        .as_ref()
        .filter(|_| any)
        .map(|refused| Check {
            code: "notification_not_posted".into(),
            name: words::NOTIFICATIONS.into(),
            level: Level::Warn,
            detail: words::not_posted(&refused.title, &seen.clock(refused.at), &refused.reason),
            advice: String::new(),
            account: None,
        });
    let checks: Vec<Check> = machine.checks.iter().cloned().chain(refused).collect();
    let mut groups: Vec<CheckGroup> = Vec::new();
    for (id, check) in (0..).zip(&checks) {
        if check.level == Level::Ok
            && check.account.is_none()
            && !SHOWN_WHEN_PASSED.contains(&check.code.as_str())
        {
            continue;
        }
        let tool = tool_of(check);
        let group = tool.map_or("pitboard", ProviderId::code);
        let line = line(seen, id, check);
        match groups.iter_mut().find(|shown| shown.id == group) {
            Some(shown) => shown.lines.push(line),
            None => groups.push(CheckGroup {
                id: group.into(),
                heading: tool.map_or_else(
                    || words::PITBOARD.into(),
                    |tool| seen.tool_name(tool.code()),
                ),
                lines: vec![line],
            }),
        }
    }
    let place = |group: &CheckGroup| {
        (group.id != "pitboard").then(|| {
            (seen.tools.iter())
                .position(|tool| tool.code == group.id)
                .unwrap_or(usize::MAX)
        })
    };
    groups.sort_by_key(place);
    let issues = checks
        .iter()
        .filter(|check| check.level != Level::Ok)
        .count();
    let failed = checks
        .iter()
        .filter(|check| check.level == Level::Fail)
        .count();
    ChecksShown {
        groups,
        summary: any.then(|| words::check_issues(seen.os, issues)),
        verdict: any.then(|| words::checks_verdict(issues, failed).into()),
        level: crate::doctor_level(&checks),
        checking,
        checked: machine
            .checked_at
            .filter(|_| any && !checking)
            .map(|at| words::checked_at(&seen.clock(at))),
        waiting: (!any).then(|| words::checking(seen.os)),
        report: machine.report.clone(),
    }
}

// An account's check is in its tool's group.
fn tool_of(check: &Check) -> Option<ProviderId> {
    match &check.account {
        Some(qualified) => Some(Key::parse(qualified).provider),
        None => pitboard_core::doctor::tool_of(&check.code),
    }
}

// Doctor's words, less any command, where they already are the apps'; the apps' own elsewhere.
fn line(seen: &Seen, id: u64, check: &Check) -> CheckLine {
    let (os, passed) = (seen.os, check.level == Level::Ok);
    let mut name = words::check_name(os, &check.code, &check.name);
    let mut detail = words::check_text(&check.detail);
    let mut advice =
        Some(words::check_text(&check.advice)).filter(|said| !passed && !said.is_empty());
    let mut action = None;
    let signed_in = |tool: ProviderId| {
        (seen.accounts().iter())
            .find(|account| account.provider == tool.code() && account.signed_in)
    };
    let in_use_of = |tool: ProviderId| match signed_in(tool) {
        Some(account) => words::signed_in_as(account.label.as_deref(), &account.email),
        None => words::sign_in_standing(pitboard_core::status::Standing::NothingParked).into(),
    };
    let settings = |section| ItemOffer {
        title: words::open_settings(os),
        intent: Intent::Show {
            place: Some(Place::Settings { section }),
        },
        enabled: true,
        confirm: None,
    };
    if let Some(qualified) = &check.account {
        let key = Key::parse(qualified);
        let found = (seen.accounts().iter())
            .find(|account| account.qualified.as_deref() == Some(qualified.as_str()))
            .map(|account| item(seen, account));
        let dormant = check.code.ends_with("dormant_account");
        name = key.label;
        detail = if dormant {
            words::NOT_SWITCHED_TO_IN_A_MONTH.into()
        } else {
            (found.as_ref())
                .and_then(|shown| shown.detail.sign_in.clone())
                .unwrap_or_else(|| words::saved_sign_in(passed).into())
        };
        advice = dormant.then(|| words::KEPT_FRESH_UNTIL_REMOVED.into());
        let kind = if dormant {
            ActionKind::Remove
        } else {
            ActionKind::SignInAgain
        };
        action = (found.as_ref())
            .filter(|_| !passed)
            .and_then(|shown| shown.actions.iter().find(|offered| offered.kind == kind))
            .and_then(offer_of);
    } else {
        match (check.code.as_str(), check.level) {
            ("security_tool", Level::Ok) => detail = words::KEYCHAIN_READY.into(),
            ("home", Level::Ok) => detail = words::folder_private(os).into(),
            ("home", _) => {
                detail = words::folder_shared(os);
                advice = Some(words::FOLDER_NAMES_ACCOUNTS.into());
            }
            ("private_on_disk", _) => {
                detail = words::sign_in_files_shared(os);
                advice = None;
            }
            ("vault", _) => {
                detail = words::NO_SAVED_SIGN_IN_OPENS.into();
                advice = Some(words::SIGN_IN_TO_EACH_AGAIN.into());
            }
            ("config_file", level) => {
                detail = words::claude_code_config(level == Level::Fail).into();
                advice = Some(words::OPEN_CLAUDE_CODE_ONCE.into());
            }
            ("identity", _) => {
                detail = words::NO_ACCOUNT_NAMED_YET.into();
                advice = Some(words::OPEN_CLAUDE_CODE_ONCE.into());
            }
            ("credential" | "credential_store", Level::Warn)
                if signed_in(ProviderId::Claude).is_none() =>
            {
                detail = words::nothing_signed_in_to(ProviderId::Claude.name());
                advice = None;
                action = Some(ItemOffer {
                    title: words::add_account(os, true),
                    intent: Intent::PresentSheet {
                        sheet: Sheet::Add {
                            provider: Some(ProviderId::Claude.code().into()),
                        },
                    },
                    enabled: !seen.state.sign_in_under_way(),
                    confirm: None,
                });
            }
            ("credential", Level::Warn) => {
                detail = words::CLAUDE_CODE_NEEDS_SIGNING_IN.into();
                action = (signed_in(ProviderId::Claude).map(|account| item(seen, account)))
                    .and_then(|shown| {
                        (shown.actions.iter())
                            .find(|offered| offered.kind == ActionKind::SignInAgain)
                            .and_then(offer_of)
                    });
            }
            ("credential", Level::Fail) => {
                detail = words::CLAUDE_CODE_SIGN_IN_UNREAD.into();
                advice = None;
            }
            ("in_use", Level::Ok) => detail = in_use_of(ProviderId::Claude),
            ("codex_login", Level::Ok) => detail = in_use_of(ProviderId::Codex),
            ("desktop_login", Level::Ok) => detail = in_use_of(ProviderId::Desktop),
            ("in_use", _) => {
                detail = words::not_sure_who(ProviderId::Claude.name());
                action = Some(ItemOffer {
                    title: words::REFRESH.into(),
                    intent: Intent::Refresh { asked: true },
                    enabled: seen.state.reads == 0,
                    confirm: None,
                });
            }
            ("desktop_login", _) => {
                detail = words::not_sure_who(seen.program_of(ProviderId::Desktop.code()));
                advice = Some(words::READ_AT_THE_NEXT_SWITCH.into());
            }
            ("codex_login", _) => {
                detail = words::CODEX_SIGN_IN_UNUSABLE.into();
                advice = Some(words::SIGN_IN_TO_CODEX_WITH_CHATGPT.into());
            }
            ("codex_backend", _) => {
                detail = words::CODEX_STORE_UNSWITCHABLE.into();
                advice = None;
            }
            ("codex_auth_file", _) => {
                detail = match signed_in(ProviderId::Codex) {
                    Some(_) => words::codex_file_shared(os),
                    None => words::nothing_signed_in_to(ProviderId::Codex.name()),
                };
                advice = None;
            }
            ("pending_parks", _) => advice = Some(words::READ_AGAIN_FIRST.into()),
            ("schedule", Level::Ok) => detail = words::RUNS_ONCE_A_DAY.into(),
            ("schedule", level) => {
                detail = words::daily_task(level == Level::Fail).into();
                advice = Some(words::TURN_IT_OFF_AND_ON.into());
                action = Some(settings(SettingsSection::Advanced));
            }
            ("fallback_login", _) => {
                let unread = (seen.state.warnings.iter()).any(|warning| {
                    warning.code == "fallback_login" && warning.held == Some(Held::Unreadable)
                });
                detail = words::LEFTOVER_FILE_SAYS.into();
                advice = unread.then(|| words::CANNOT_CLEAN_UP_UNREAD.into());
                action = (!unread).then(|| ItemOffer {
                    title: words::clean_up(os, true),
                    intent: Intent::PresentSheet { sheet: Sheet::Stow },
                    enabled: true,
                    confirm: None,
                });
            }
            ("interrupted_switch", _) => {
                advice = None;
                action = seen.state.stuck.then(|| ItemOffer {
                    title: words::cancel_switch(os, true),
                    intent: Intent::AbandonStuckSwitch,
                    enabled: true,
                    confirm: Some(super::notices::cancel_stuck_question(seen)),
                });
            }
            ("desktop_move", _) => {
                detail = words::SWITCH_IN_CLAUDE_WAITS.into();
                advice = Some(words::KEEP_CLAUDE_CLOSED.into());
            }
            ("desktop_awaiting", _) => {
                detail = words::CLAUDE_SIGNED_OUT_FOR_A_SIGN_IN.into();
                advice = None;
            }
            ("desktop_parks_folder", _) => {
                detail = words::CANNOT_SAVE_CLAUDE_SIGN_INS.into();
                advice = None;
            }
            ("desktop_set_aside", _) => {
                detail = words::CLAUDE_FOLDERS_MOVED.into();
                advice = None;
            }
            _ => {}
        }
    }
    CheckLine {
        id,
        name,
        level: check.level,
        spoken_level: words::spoken_level(check.level).to_owned(),
        detail,
        advice,
        learn: (!passed && action.is_none()).then(|| next::of_check(os)),
        action,
    }
}

#[cfg(test)]
impl ChecksShown {
    // every line, in the order the groups show them
    pub(crate) fn lines(&self) -> Vec<CheckLine> {
        (self.groups.iter())
            .flat_map(|group| group.lines.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{ChecksShown, words};
    use crate::launch::Check;
    use crate::model::state::State;
    use crate::model::{Intent, Place, SettingsSection, Sheet};
    use crate::present::testing::{
        Utc, account, both_tools, claude_code, mechanism_words, read, status, strings_of,
    };
    use crate::present::{ItemOffer, present_on};
    use crate::{Account, Level};
    use pitboard_core::host::Os;
    use pitboard_core::status::Standing;

    const OS: Os = Os::MacOs;

    fn check(code: &str, level: Level, detail: &str, advice: &str) -> Check {
        Check {
            code: code.into(),
            name: code.replace('_', " "),
            level,
            detail: detail.into(),
            advice: advice.into(),
            account: None,
        }
    }

    fn of(account: &str, check: Check) -> Check {
        Check {
            account: Some(account.into()),
            ..check
        }
    }

    fn shown_in(state: &mut State, checks: Vec<Check>) -> ChecksShown {
        state.machine.checks = checks;
        present_on(OS, state, 0, &Utc).machine.checks
    }

    fn shown(accounts: Vec<Account>, checks: Vec<Check>) -> ChecksShown {
        shown_in(
            &mut read(status(accounts, Vec::new()), both_tools()),
            checks,
        )
    }

    // The mockup's Diagnostics: what a person reads, and every check that did not pass.
    #[test]
    fn a_check_that_passed_shows_only_where_a_person_reads_it() {
        let lines = shown(
            vec![account(Some("work")).signed_in().build()],
            vec![
                check("security_tool", Level::Ok, "/usr/bin/security", ""),
                check("slot", Level::Ok, "default  ·  Claude Code-credentials", ""),
                check("credential", Level::Warn, "refresh 5010  ·  expired", "x."),
                check("state", Level::Ok, "5 enrolled", ""),
                check("home", Level::Ok, "/Users/dana/.pitboard", ""),
            ],
        )
        .lines();
        let said: Vec<(u64, String, String)> = (lines.iter())
            .map(|line| (line.id, line.name.clone(), line.detail.clone()))
            .collect();
        assert_eq!(
            said,
            [
                (0, "Keychain".into(), words::KEYCHAIN_READY.into()),
                (
                    4,
                    "Pitboard Folder".into(),
                    words::folder_private(OS).into()
                ),
                (
                    2,
                    "Sign-In".into(),
                    words::CLAUDE_CODE_NEEDS_SIGNING_IN.into()
                ),
            ]
        );
    }

    #[test]
    fn each_check_is_under_its_tools_heading_pitboards_own_first() {
        let shown = shown(
            vec![
                account(Some("work")).signed_in().build(),
                account(Some("main")).of("codex").signed_in().build(),
            ],
            vec![
                check("in_use", Level::Ok, "work, as Anthropic said 15:24", ""),
                check(
                    "codex_login",
                    Level::Ok,
                    "main@example.com  ·  refresh f",
                    "",
                ),
                of(
                    "claude/work",
                    check("parked_login", Level::Ok, "parked", ""),
                ),
                check(
                    "network",
                    Level::Ok,
                    "direct: no variable names a proxy",
                    "",
                ),
            ],
        );
        let groups: Vec<(&str, &str, Vec<String>)> = (shown.groups.iter())
            .map(|group| {
                let names = group.lines.iter().map(|line| line.name.clone()).collect();
                (group.id.as_str(), group.heading.as_str(), names)
            })
            .collect();
        assert_eq!(
            groups,
            [
                ("pitboard", "Pitboard", vec!["Network".to_owned()]),
                (
                    "claude",
                    "Claude Code",
                    vec!["Signed-In Account".into(), "work".into()]
                ),
                ("codex", "Codex", vec!["Signed-In Account".into()]),
            ]
        );
        let details: Vec<String> = (shown.lines().into_iter())
            .map(|line| line.detail)
            .collect();
        assert_eq!(
            details,
            [
                words::check_text("direct: no variable names a proxy"),
                words::signed_in_as(Some("work"), "work@example.com"),
                words::IN_USE.into(),
                words::signed_in_as(Some("main"), "main@example.com"),
            ]
        );
    }

    // Plan 4.6: an account's saved sign-in in the words of its row, and its Sign In Again….
    #[test]
    fn an_accounts_check_is_named_by_it_and_said_as_its_detail_says_it() {
        let accounts = vec![
            account(Some("work")).signed_in().build(),
            account(Some("old"))
                .standing(Standing::LoginExpired)
                .parked_until(-86_400)
                .build(),
            account(Some("spare")).parked_until(37 * 86_400).build(),
        ];
        let lines = shown(
            accounts,
            vec![
                of(
                    "claude/work",
                    check("parked_login", Level::Ok, "signed in", ""),
                ),
                of(
                    "claude/old",
                    check("parked_login", Level::Warn, "expired", "Run `x`."),
                ),
                of(
                    "claude/spare",
                    check("parked_login", Level::Ok, "parked", ""),
                ),
                of(
                    "claude/spare",
                    check("dormant_account", Level::Warn, "not switched to", "forget"),
                ),
            ],
        )
        .lines();
        let said: Vec<(&str, &str, Option<&str>, Option<&str>)> = (lines.iter())
            .map(|line| {
                (
                    line.name.as_str(),
                    line.detail.as_str(),
                    line.advice.as_deref(),
                    line.action.as_ref().map(|action| action.title.as_str()),
                )
            })
            .collect();
        let again = words::sign_in_again(OS, true);
        let remove = words::remove_account(OS);
        assert_eq!(
            said,
            [
                ("work", words::IN_USE, None, None),
                ("old", "Sign-in expired", None, Some(again.as_str())),
                ("spare", "Saved sign-in valid until Feb 7", None, None),
                (
                    "spare",
                    words::NOT_SWITCHED_TO_IN_A_MONTH,
                    Some(words::KEPT_FRESH_UNTIL_REMOVED),
                    Some(remove.as_str())
                ),
            ]
        );
        assert!(
            (lines[3].action.as_ref()).is_some_and(|remove| remove.confirm.is_some()),
            "Remove asks first"
        );
    }

    // Plan section 3: every problem ends with something to do in the app, or a Learn More link.
    #[test]
    fn a_check_that_did_not_pass_ends_with_an_action_or_learn_more() {
        let mut state = read(status(Vec::new(), Vec::new()), vec![claude_code()]);
        state.stuck = true;
        let lines = shown_in(
            &mut state,
            vec![
                check("schedule", Level::Fail, "runs /gone/pitboard", "Run `y`."),
                check(
                    "interrupted_switch",
                    Level::Warn,
                    "a switch did not finish",
                    "",
                ),
                check("state", Level::Fail, "unreadable", ""),
                check("fallback_login", Level::Warn, "/x/.credentials.json", ""),
                check("in_use", Level::Warn, "not asked", "Run `pitboard status`."),
            ],
        )
        .lines();
        let offered: Vec<Option<(String, Intent)>> = (lines.iter())
            .map(|line| {
                (line.action.clone()).map(|ItemOffer { title, intent, .. }| (title, intent))
            })
            .collect();
        assert_eq!(
            offered,
            [
                Some((
                    words::open_settings(OS),
                    Intent::Show {
                        place: Some(Place::Settings {
                            section: SettingsSection::Advanced
                        })
                    }
                )),
                Some((words::cancel_switch(OS, true), Intent::AbandonStuckSwitch)),
                None,
                Some((
                    words::clean_up(OS, true),
                    Intent::PresentSheet { sheet: Sheet::Stow }
                )),
                Some((words::REFRESH.into(), Intent::Refresh { asked: true })),
            ]
        );
        assert!(
            lines[1]
                .action
                .as_ref()
                .is_some_and(|cancel| cancel.confirm.is_some())
        );
        for line in &lines {
            assert_eq!(
                line.learn,
                line.action
                    .is_none()
                    .then(|| crate::present::next::of_check(OS)),
                "{}",
                line.name
            );
        }
    }

    // A fresh machine's checks of Claude Code, and one whose sign-in in use runs out.
    #[test]
    fn claude_codes_sign_in_offers_add_account_or_sign_in_again() {
        let nobody = "Nothing is signed in for this slot.";
        let lines = shown(
            Vec::new(),
            vec![
                check(
                    "credential_store",
                    Level::Warn,
                    "no credential in any backend",
                    nobody,
                ),
                check("credential", Level::Warn, "nothing stored", nobody),
            ],
        )
        .lines();
        for line in lines {
            assert_eq!(line.detail, words::nothing_signed_in_to("Claude Code"));
            assert_eq!(line.advice, None);
            assert_eq!(
                line.action.map(|add| add.intent),
                Some(Intent::PresentSheet {
                    sheet: Sheet::Add {
                        provider: Some("claude".into())
                    }
                })
            );
        }
        let soon = "This login expires soon and will need signing in again.";
        let lines = shown(
            vec![account(Some("work")).signed_in().build()],
            vec![check(
                "credential",
                Level::Warn,
                "refresh f  ·  1d 0h",
                soon,
            )],
        )
        .lines();
        assert_eq!(
            (
                lines[0].detail.as_str(),
                lines[0].advice.clone(),
                lines[0].action.as_ref().map(|again| again.title.clone())
            ),
            (
                words::CLAUDE_CODE_NEEDS_SIGNING_IN,
                Some(words::check_text(soon)),
                Some(words::sign_in_again(OS, true))
            )
        );
    }

    // The brief's "Diagnostics on screen in app words": doctor's report alone says park or enrol.
    #[test]
    fn no_check_says_park_enrol_or_a_command() {
        let accounts = vec![
            account(Some("work")).signed_in().build(),
            account(Some("main")).of("codex").signed_in().build(),
        ];
        let shown = shown(
            accounts,
            vec![
                check(
                    "home",
                    Level::Warn,
                    "/x is mode 755",
                    "Park names contain account identifiers, so only you should read it: \
                     `chmod 700 /x`.",
                ),
                check(
                    "private_on_disk",
                    Level::Fail,
                    "/x is mode 644",
                    "These hold a login: a sealed park only this account opens.",
                ),
                check(
                    "vault",
                    Level::Fail,
                    "none of the 2 parked logins opens for this Windows account",
                    "Sign in to each again: `pitboard enroll work --sign-in`.",
                ),
                check(
                    "identity",
                    Level::Warn,
                    "Claude Code has not recorded a signed-in account",
                    "Run `claude` once.",
                ),
                check(
                    "codex_backend",
                    Level::Fail,
                    "keyring (Codex's default)",
                    "Pitboard parks and switches no Codex login while it cannot tell.",
                ),
                check(
                    "codex_login",
                    Level::Warn,
                    "an API key",
                    "Pitboard will not park or switch a Codex login it cannot read.",
                ),
                check(
                    "desktop_move",
                    Level::Warn,
                    "a move of `desktop/work`'s login was interrupted",
                    "Keep Claude closed: the next change settles it first.",
                ),
                check(
                    "desktop_awaiting",
                    Level::Warn,
                    "Claude is signed out for a sign-in, and `desktop/home` is parked",
                    "",
                ),
                check(
                    "desktop_parks_folder",
                    Level::Fail,
                    "/Users/dana/.pitboard/parks is a link",
                    "Pitboard parks no Claude Desktop login through a link.",
                ),
                check(
                    "desktop_set_aside",
                    Level::Warn,
                    "1 in /Users/dana/.pitboard/parks, 1 holding a login",
                    "",
                ),
                check(
                    "desktop_login",
                    Level::Warn,
                    "`desktop/work` is recorded in use and also holds a parked login",
                    "",
                ),
                check(
                    "config_file",
                    Level::Warn,
                    "Claude Code hasn't run here",
                    "Run `claude` once.",
                ),
                check(
                    "credential",
                    Level::Fail,
                    "the keychain is locked",
                    "Unlock it with `security unlock-keychain`.",
                ),
            ],
        );
        for said in strings_of(&shown.groups) {
            assert!(mechanism_words(&said).is_empty(), "{said}");
            assert!(!said.contains('`'), "{said}");
        }
    }

    #[test]
    fn the_line_over_the_checks_counts_the_issues_and_takes_the_worst_level() {
        let over = |levels: &[Level]| {
            let checks = (levels.iter())
                .map(|&level| check("state", level, "", ""))
                .collect();
            let shown = shown(Vec::new(), checks);
            (shown.summary, shown.verdict, shown.level)
        };
        let said = |issues, failed| {
            (
                Some(words::check_issues(OS, issues)),
                Some(words::checks_verdict(issues, failed).to_owned()),
            )
        };
        let (none, one, two) = (said(0, 0), said(1, 0), said(2, 1));
        assert_eq!(over(&[Level::Ok, Level::Ok]), (none.0, none.1, Level::Ok));
        assert_eq!(over(&[Level::Warn, Level::Ok]), (one.0, one.1, Level::Warn));
        assert_eq!(
            over(&[Level::Ok, Level::Fail, Level::Warn]),
            (two.0, two.1, Level::Fail)
        );
        assert_eq!(over(&[]), (None, None, Level::Ok));
        assert_eq!(words::check_issues(OS, 0), "No Issues");
        assert_eq!(words::check_issues(Os::Windows, 2), "2 issues");
    }
}
