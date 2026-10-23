//! What Pitboard has to tell somebody that is not an account, from Notices.swift: notices in
//! the order they matter, the headings of warnings by code, what a tool's last switch still
//! has to say, and what the menu says of all of it, from MenuBarContent.swift.

use super::words;
use super::{
    Footing, MenuEntry, MenuNotices, NoticeAction, PanelNotice, Question, Seen, Severity,
    spoken_severity,
};
use crate::model::advice::HeadsUp;
use crate::model::state::{in_its_app, split};
use crate::model::{Intent, LastSwitch, Pane, Sheet};
use crate::model::{NotificationButton, RunOutNotice};
use crate::{Account, Adoption, Held, Warning};
use pitboard_core::host::{OS, Os};
use pitboard_core::provider::ProviderId;
use pitboard_core::words as said;

/// Where a person goes to install Claude Code, the tool a machine without one is told
/// about, as MenuBarContent.swift's `Links.installClaudeCode` named it.
pub(crate) const INSTALL_CLAUDE_CODE: &str = "https://docs.claude.com/en/docs/claude-code/setup";

fn notice(
    id: String,
    severity: Severity,
    title: String,
    lines: Vec<String>,
    actions: Vec<NoticeAction>,
) -> PanelNotice {
    PanelNotice {
        id,
        severity,
        spoken_severity: spoken_severity(severity).into(),
        title,
        lines,
        until: None,
        until_label: None,
        actions,
    }
}

/// A button that switches to `qualified`, named by its label, held back while a switch is
/// under way.
pub(crate) fn switch_action(seen: &Seen, qualified: &str, label: &str) -> NoticeAction {
    NoticeAction {
        title: words::switch_to(label),
        intent: Intent::SwitchTo {
            qualified: qualified.to_owned(),
        },
        dismisses: false,
        switches: true,
        enabled: seen.state.switch_under_way().is_none(),
        confirm: None,
    }
}

fn dismiss(intent: Intent) -> NoticeAction {
    NoticeAction {
        title: "Dismiss".into(),
        intent,
        dismisses: true,
        switches: false,
        enabled: true,
        confirm: None,
    }
}

fn other_warnings<'a>(seen: &'a Seen) -> impl Iterator<Item = &'a Warning> {
    let problem = seen.problem();
    let stuck = seen.state.stuck;
    seen.state.warnings.iter().filter(move |warning| {
        Some(warning.message.as_str()) != problem
            && warning.code != "sign_in_awaited"
            && !(stuck && warning.code == "move_waiting")
    })
}

// Not one whose own sheet this app shows, which offers the same.
fn awaiting(seen: &Seen) -> Vec<PanelNotice> {
    let Some(status) = &seen.state.status else {
        return Vec::new();
    };
    status
        .awaiting
        .iter()
        .filter(|awaiting| {
            !seen
                .state
                .signing()
                .is_some_and(|signing| signing.in_its_app && signing.provider == awaiting.provider)
        })
        .map(|awaiting| {
            let provider = &awaiting.provider;
            let app = seen.program_of(provider);
            let put_back = split(&awaiting.put_back).1;
            let (title, finish, sheet) = if awaiting.again {
                (
                    format!("Signing in to {} again isn’t finished", awaiting.label),
                    words::titled(seen.os, "Finish Signing In…", "Finish signing in…"),
                    Sheet::SignInAgain {
                        provider: provider.clone(),
                        label: awaiting.label.clone(),
                    },
                )
            } else {
                (
                    format!(
                        "Adding a {} account isn’t finished",
                        seen.tool_name(provider)
                    ),
                    words::titled(seen.os, "Finish Adding…", "Finish adding…"),
                    Sheet::Add {
                        provider: Some(provider.clone()),
                    },
                )
            };
            notice(
                format!("awaiting/{provider}"),
                Severity::Warning,
                title,
                words::awaiting_lines(app, &awaiting.label, put_back, awaiting.again),
                vec![
                    NoticeAction {
                        title: finish,
                        intent: Intent::PresentSheet { sheet },
                        dismisses: false,
                        switches: false,
                        enabled: true,
                        confirm: None,
                    },
                    NoticeAction {
                        title: words::titled(
                            seen.os,
                            &format!("Put {put_back} Back"),
                            &format!("Put {put_back} back"),
                        ),
                        ..switch_action(seen, &awaiting.put_back, put_back)
                    },
                ],
            )
        })
        .collect()
}

/// What `last` warned about that the read after it did not already show, so nothing is said
/// twice.
pub(crate) fn warned_after<'a>(seen: &'a Seen, last: &'a LastSwitch) -> Vec<&'a Warning> {
    last.warnings
        .iter()
        .filter(|warning| !seen.state.warnings.contains(warning))
        .collect()
}

/// What a switch means for a tool's running sessions, said only when the core did not count
/// them. When it did, its own warning says the same with the count and with what not to do
/// in them, and the same fact twice is once too many.
pub(crate) fn restart_line(last: &LastSwitch) -> Option<String> {
    let restart = last.restart.as_ref()?;
    if last
        .warnings
        .iter()
        .any(|warning| warning.code == "sessions_still_running")
    {
        return None;
    }
    Some(words::restart_notice(&restart.program, &restart.from))
}

/// What a warning's notice is known by from one snapshot to the next: the account it stands
/// for, where its words change while it stands, and otherwise its words, so two messages of
/// one code are two notices.
fn warning_id(warning: &Warning) -> String {
    let mark = warning
        .account
        .clone()
        .unwrap_or_else(|| mark(&warning.message));
    format!("warning/{}/{mark}", warning.code)
}

/// A stable mark for a warning's message: FNV-1a, which needs no seed.
fn mark(message: &str) -> String {
    let hash = message
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
    format!("{hash:016x}")
}

/// Everything to tell somebody, the most pressing first: what stops Pitboard working, then
/// accounts that ran out, then what the last switch of each tool said, then any warning the
/// last read or change carried, then what is only worth knowing.
pub(crate) fn notices(seen: &Seen, footing: &Footing) -> Vec<PanelNotice> {
    let state = seen.state;
    let mut said = Vec::new();
    if state.stuck {
        let reason = seen
            .problem()
            .map_or_else(|| words::switch_waits_for(&seen.services()), str::to_owned);
        // Their own notices are left out: every change they offer waits for this switch.
        let mut moving: Vec<(String, &str)> = Vec::new();
        let waiting = state
            .warnings
            .iter()
            .filter(|warning| warning.code == "move_waiting");
        for tool in waiting.filter_map(|warning| warning.provider.as_deref()) {
            let name = seen.tool_name(tool);
            if !moving.iter().any(|(named, _)| *named == name) {
                moving.push((name, seen.program_of(tool)));
            }
        }
        let mut lines = vec![
            reason,
            "Giving up on it keeps every login. Nothing is deleted.".into(),
        ];
        lines.extend(
            moving
                .iter()
                .map(|(tool, program)| words::move_waits_too(tool, program)),
        );
        said.push(notice(
            "stuck".into(),
            Severity::Error,
            "An interrupted switch is waiting".into(),
            lines,
            vec![NoticeAction {
                title: words::titled(seen.os, "Give Up…", "Give up…"),
                intent: Intent::AbandonStuckSwitch,
                dismisses: false,
                switches: false,
                enabled: true,
                confirm: Some(Question {
                    title: "Give up on the interrupted switch?".into(),
                    message: words::give_up_stuck(&moving),
                    confirm: words::titled(seen.os, "Give Up", "Give up"),
                }),
            }],
        ));
    } else if let Some(problem) = seen.problem()
        && *footing != Footing::NoClaudeCode
    {
        let mut lines = vec![problem.to_owned()];
        if seen
            .accounts()
            .iter()
            .any(|account| account.usage.is_some())
        {
            lines.push("The numbers shown are the last ones measured.".into());
        }
        said.push(notice(
            "read".into(),
            Severity::Error,
            "Couldn’t read usage".into(),
            lines,
            Vec::new(),
        ));
    }
    for advice in &state.advice {
        let limit = said::limit_name(&advice.window.kind, advice.window.length_seconds);
        said.push(notice(
            format!("advice/{}", advice.key_of()),
            Severity::Warning,
            words::of_tool(advice.tool.as_deref(), &words::ran_out(&advice.ran, &limit)),
            vec![words::room_left(&advice.instead, advice.left)],
            vec![switch_action(seen, &advice.switch_to, &advice.instead)],
        ));
    }
    for last in &state.last_switches {
        said.push(of_last_switch(seen, last));
    }
    said.extend(awaiting(seen));
    // What a switch's notice says already, which is only what the read does not repeat: a
    // warning both carry is the read's to say, and leaving it out of both said it nowhere.
    let shown: Vec<&Warning> = state
        .last_switches
        .iter()
        .flat_map(|last| warned_after(seen, last))
        .collect();
    let mut ids: Vec<String> = Vec::new();
    for warning in other_warnings(seen) {
        if shown.contains(&warning) || (state.stuck && warning.code == "recovery_undetermined") {
            continue;
        }
        let mut id = warning_id(warning);
        let copy = format!("{id}/");
        let twice = ids
            .iter()
            .filter(|seen| **seen == id || seen.starts_with(&copy))
            .count();
        if twice > 0 {
            id = format!("{id}/{twice}");
        }
        ids.push(id.clone());
        said.push(notice(
            id,
            Severity::Warning,
            words::warning_heading(seen.os, warning).into(),
            vec![warning.message.clone()],
            warning_actions(seen, warning),
        ));
    }
    if let Some(abandoned) = &state.abandoned {
        said.push(notice(
            "abandoned".into(),
            Severity::Info,
            "Gave up on the interrupted switch".into(),
            vec![words::gave_up(
                &abandoned.from,
                &abandoned.to,
                abandoned.logins_kept,
            )],
            vec![dismiss(Intent::DismissAbandoned)],
        ));
    }
    if !state.moves_given_up().is_empty() {
        said.push(notice(
            "moves_given_up".into(),
            Severity::Info,
            "Gave up on the interrupted move".into(),
            state.moves_given_up().to_vec(),
            vec![dismiss(Intent::DismissAbandoned)],
        ));
    }
    if let Some(stowed) = &state.stowed {
        said.push(notice(
            "stowed".into(),
            Severity::Info,
            "Put away the login left in a file".into(),
            said::stowed_lines(seen.os, stowed),
            vec![dismiss(Intent::DismissStowed)],
        ));
    }
    said
}

// Writing the config moves no login; a file Pitboard cannot read it cannot put away either.
fn warning_actions(seen: &Seen, warning: &Warning) -> Vec<NoticeAction> {
    if warning.code == "login_replaced" {
        return sign_in_again_action(seen, warning);
    }
    if let Some(provider) = warning.provider.as_deref().filter(|tool| in_its_app(tool)) {
        return in_app_actions(seen, provider, &warning.code);
    }
    match warning.code.as_str() {
        "config_names_another" => update_config_action(seen),
        "fallback_login" if warning.held != Some(Held::Unreadable) => {
            vec![NoticeAction {
                title: words::titled(seen.os, "Put Away…", "Put away…"),
                intent: Intent::PresentSheet { sheet: Sheet::Stow },
                dismisses: false,
                switches: false,
                enabled: true,
                confirm: None,
            }]
        }
        _ => Vec::new(),
    }
}

// The sign-in its row offers, of the account whose login was replaced.
fn sign_in_again_action(seen: &Seen, warning: &Warning) -> Vec<NoticeAction> {
    seen.accounts()
        .iter()
        .find(|account| Some(account.id.as_str()) == warning.account.as_deref())
        .and_then(|account| Some((account.provider.clone(), account.label.clone()?)))
        .map(|(provider, label)| NoticeAction {
            title: words::titled(seen.os, "Sign In Again…", "Sign in again…"),
            intent: Intent::PresentSheet {
                sheet: Sheet::SignInAgain { provider, label },
            },
            dismisses: false,
            switches: false,
            enabled: !seen.state.sign_in_under_way(),
            confirm: None,
        })
        .into_iter()
        .collect()
}

// A move waiting is given up only once a change from this app could not settle it.
fn in_app_actions(seen: &Seen, provider: &str, code: &str) -> Vec<NoticeAction> {
    if !matches!(
        code,
        "in_use_unconfirmed" | "config_names_another" | "move_waiting"
    ) {
        return Vec::new();
    }
    let app = seen.program_of(provider);
    let mut actions = vec![NoticeAction {
        title: words::titled(
            seen.os,
            &format!("Add the Account {app} Shows…"),
            &format!("Add the account {app} shows…"),
        ),
        intent: Intent::PresentSheet {
            sheet: Sheet::Add {
                provider: Some(provider.to_owned()),
            },
        },
        dismisses: false,
        switches: false,
        enabled: true,
        confirm: None,
    }];
    let stopped = matches!(
        seen.state.refused_by(provider),
        Some("recovery_undetermined" | "recovery_elsewhere" | "move_mixed")
    );
    if code == "move_waiting" && stopped {
        actions.push(NoticeAction {
            title: words::titled(seen.os, "Give Up…", "Give up…"),
            intent: Intent::AbandonStuckSwitch,
            dismisses: false,
            switches: false,
            enabled: true,
            confirm: Some(Question {
                title: "Give up on the interrupted move?".into(),
                message: format!(
                    "Every folder is kept and nothing is deleted. Whatever is in {app}’s folder \
                     stays there."
                ),
                confirm: words::titled(seen.os, "Give Up", "Give up"),
            }),
        });
    }
    actions
}

/// Writing the account in use into Claude Code's config, held back while a switch is under
/// way, where that account is enrolled.
fn update_config_action(seen: &Seen) -> Vec<NoticeAction> {
    seen.accounts()
        .iter()
        .find(|account| account.provider == ProviderId::Claude.code() && account.signed_in)
        .and_then(|account| account.qualified.clone())
        .map(|qualified| NoticeAction {
            title: words::titled(
                seen.os,
                "Update Claude Code’s Config",
                "Update Claude Code’s config",
            ),
            intent: Intent::UpdateConfig { qualified },
            dismisses: false,
            switches: false,
            enabled: seen.state.switch_under_way().is_none(),
            confirm: None,
        })
        .into_iter()
        .collect()
}

/// What one tool's last switch still has to say. Sessions of a tool that follows a switch by
/// itself pick it up within a moment, and the notice counts down to it; once that moment has
/// passed a countdown would count to something already over, so there is none.
fn of_last_switch(seen: &Seen, last: &LastSwitch) -> PanelNotice {
    let label = split(&last.to).1;
    let tool = seen.tool_if_shown(&last.provider);
    let warned = warned_after(seen, last);
    let restart = restart_line(last);
    let severity = if warned.is_empty() && restart.is_none() {
        Severity::Info
    } else {
        Severity::Warning
    };
    let mut lines: Vec<String> = last
        .opens
        .iter()
        .chain(&last.said)
        .cloned()
        .chain(restart)
        .collect();
    lines.extend(warned.iter().map(|warning| warning.message.clone()));
    let title = if last.said.is_some() {
        words::has_a_new_login(tool.as_deref(), label)
    } else {
        words::switched_to(tool.as_deref(), label)
    };
    let until = last.follows_at.filter(|at| *at > seen.now);
    PanelNotice {
        until_label: until.map(|_| words::sessions_follow_in(tool.as_deref())),
        until,
        ..notice(
            format!("switch/{}", last.provider),
            severity,
            title,
            lines,
            vec![dismiss(Intent::DismissSwitch {
                provider: last.provider.clone(),
            })],
        )
    }
}

// What a click on a notification, or on its button, asks for, kept as text with the
// notification until then, across launches too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Clicked {
    Accounts,
    Limits,
    Switch(String),
    SignInAgain(String),
}

const SWITCH: &str = "switch/";
const SIGN_IN_AGAIN: &str = "sign-in-again/";
const LIMITS: &str = "limits";
pub(crate) const UNUSABLE: &str = "unusable/";

impl Clicked {
    pub(crate) fn argument(&self) -> String {
        match self {
            Clicked::Accounts => "accounts".into(),
            Clicked::Limits => LIMITS.into(),
            Clicked::Switch(qualified) => format!("{SWITCH}{qualified}"),
            Clicked::SignInAgain(qualified) => format!("{SIGN_IN_AGAIN}{qualified}"),
        }
    }

    // Anything else, such as what an older Pitboard's notification hands back, opens the accounts.
    pub(crate) fn read(argument: &str) -> Clicked {
        let named = |prefix: &str| {
            argument
                .strip_prefix(prefix)
                .filter(|qualified| !qualified.is_empty())
                .map(str::to_owned)
        };
        if let Some(qualified) = named(SWITCH) {
            Clicked::Switch(qualified)
        } else if let Some(qualified) = named(SIGN_IN_AGAIN) {
            Clicked::SignInAgain(qualified)
        } else if argument == LIMITS {
            Clicked::Limits
        } else {
            Clicked::Accounts
        }
    }
}

fn posted(id: String, title: String, body: String, clicked: &Clicked) -> RunOutNotice {
    RunOutNotice {
        id,
        title,
        subtitle: None,
        body,
        clicked: clicked.argument(),
        button: None,
    }
}

fn button(title: String, clicked: &Clicked) -> Option<NotificationButton> {
    Some(NotificationButton {
        title,
        clicked: clicked.argument(),
    })
}

pub(crate) fn notification_buttons() -> Vec<String> {
    vec![switch_button(), sign_in_again_button(OS)]
}

fn switch_button() -> String {
    "Switch".into()
}

fn sign_in_again_button(os: Os) -> String {
    words::titled(os, "Sign In Again", "Sign in again")
}

// Worded as the window's notice of a run-out is, past 100%. Its id names the account, the limit
// and the reset, so one limit is one notification however far it goes.
pub(crate) fn at_limit_notice(heads_up: &HeadsUp, now: i64) -> RunOutNotice {
    let limit = &heads_up.window;
    let title = if limit.percent >= 100.0 {
        words::ran_out(
            &heads_up.ran,
            &said::limit_name(&limit.kind, limit.length_seconds),
        )
    } else {
        format!(
            "{} has used {}",
            heads_up.ran,
            said::share_of_limit(&crate::model::advice::window(limit))
        )
    };
    let body = match (&heads_up.instead, limit.resets_at) {
        (Some((instead, left, _)), _) => words::room_left(instead, *left),
        (None, Some(at)) => format!("{}.", capitalised(&said::resets(at, now))),
        (None, None) => String::new(),
    };
    RunOutNotice {
        subtitle: heads_up.tool.clone(),
        button: heads_up.instead.as_ref().and_then(|(_, _, switch_to)| {
            button(switch_button(), &Clicked::Switch(switch_to.clone()))
        }),
        ..posted(
            format!("{}-{}", heads_up.key_of(), limit.resets_at.unwrap_or(0)),
            title,
            body,
            &Clicked::Accounts,
        )
    }
}

/// What Pitboard switched by itself, posted since nobody was there to ask for it: "Switched
/// Claude Code to home", and "work had used 96% of its 5-hour limit. Sessions already running
/// follow within 33 seconds.", or, while a file sits behind the keychain, when they do
/// instead. It has nothing to switch to: the account left has reached the share it was
/// switched away at.
pub(crate) fn auto_switched_notice(
    from: &str,
    to: &str,
    used: &str,
    adoption: &Adoption,
    at: i64,
) -> RunOutNotice {
    let follows = adoption.follows();
    posted(
        format!("auto/{from}/{to}/{at}"),
        format!("Switched Claude Code to {to}"),
        format!("{from} had used {used}. {follows}"),
        &Clicked::Accounts,
    )
}

/// Where Pitboard would have switched Claude Code by itself and did not: how much of which
/// limit, and why, in the core's words. Identified by what tells the reason apart, `key`.
pub(crate) fn auto_skipped_notice(key: &str, from: &str, used: &str, why: &str) -> RunOutNotice {
    posted(
        format!("auto-{key}"),
        "Claude Code was not switched".into(),
        format!("{from} has used {used}. {}.", capitalised(why)),
        &Clicked::Limits,
    )
}

/// Where Pitboard cannot judge whether to switch Claude Code by itself: why, in the core's
/// words. Identified by what tells the reason apart, `key`.
pub(crate) fn auto_not_watching_notice(key: &str, why: &str) -> RunOutNotice {
    posted(
        format!("auto-{key}"),
        "Pitboard is not switching Claude Code".into(),
        format!("{}.", capitalised(why)),
        &Clicked::Limits,
    )
}

/// A warning that stands until somebody acts, posted once while it does, since nobody may
/// have the window open: a login replaced outside Pitboard, with a button that signs in to
/// `again`, the account it names, where that is enrolled. Titled and identified as its notice
/// in the window is, and said in the core's words.
pub(crate) fn standing_notice(warning: &Warning, again: Option<&str>) -> RunOutNotice {
    RunOutNotice {
        button: again.and_then(|qualified| {
            button(
                sign_in_again_button(OS),
                &Clicked::SignInAgain(qualified.to_owned()),
            )
        }),
        ..posted(
            warning_id(warning),
            words::warning_heading(OS, warning).into(),
            warning.message.clone(),
            &Clicked::Accounts,
        )
    }
}

// Titled as its row is marked.
pub(crate) fn unusable_notice(account: &Account, tool: Option<String>) -> RunOutNotice {
    let label = account.label.as_deref().unwrap_or_default();
    RunOutNotice {
        subtitle: tool,
        button: account.qualified.as_ref().and_then(|qualified| {
            button(
                sign_in_again_button(OS),
                &Clicked::SignInAgain(qualified.clone()),
            )
        }),
        ..posted(
            format!("{UNUSABLE}{}", account.id),
            format!("{label} needs signing in again"),
            "Its parked login can no longer be used, so Pitboard can’t switch to it.".into(),
            &Clicked::Accounts,
        )
    }
}

/// A switch Pitboard tried by itself and could not make, in the core's words.
pub(crate) fn auto_refused_notice(code: &str, message: &str) -> RunOutNotice {
    posted(
        format!("auto-refused/{code}"),
        "Pitboard could not switch Claude Code".into(),
        message.into(),
        &Clicked::Limits,
    )
}

/// A warm-up Pitboard tried by itself that started no period, in the core's words, which name
/// the account, so one account's does not replace another's.
pub(crate) fn warm_failed_notice(code: &str, message: &str) -> RunOutNotice {
    posted(
        format!("warm-failed/{code}/{message}"),
        "Pitboard could not warm up an account".into(),
        message.into(),
        &Clicked::Limits,
    )
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// What the menu says of the notices: advice and an add left halfway as items that act on
/// them; anything else to know about, past a note, as one item that opens the window where it
/// is said in full; and the one thing to do on a machine that has no tool.
pub(crate) fn menu_notices(seen: &Seen, footing: &Footing, notices: &[PanelNotice]) -> MenuNotices {
    let pressing: Vec<&PanelNotice> = notices
        .iter()
        .filter(|notice| notice.severity != Severity::Info)
        .collect();
    // A sign-in left halfway offers its way on before its way back.
    let switches = pressing
        .iter()
        .flat_map(|notice| {
            let switching = notice.actions.iter().find(|action| action.switches);
            let on = notice
                .actions
                .iter()
                .find(|action| !action.switches && !action.dismisses)
                .filter(|_| notice.id.starts_with("awaiting/"));
            on.into_iter().chain(switching).map(|action| MenuEntry {
                title: action.title.clone(),
                subtitle: Some(notice.title.clone()),
                help: None,
                // The menu draws an item that switches with the switch's symbol.
                severity: (!action.switches).then_some(notice.severity),
                intent: Some(action.intent.clone()),
                link: None,
                enabled: action.enabled && seen.state.switch_under_way().is_none(),
            })
        })
        .collect();
    let others: Vec<&&PanelNotice> = pressing
        .iter()
        .filter(|notice| !notice.actions.iter().any(|action| action.switches))
        .collect();
    let others = others.first().map(|first| {
        let one = others.len() == 1;
        MenuEntry {
            title: if one {
                first.title.clone()
            } else {
                words::things_to_look_at(others.len())
            },
            subtitle: Some(if one {
                "Show in Pitboard".into()
            } else {
                first.title.clone()
            }),
            help: one.then(|| first.lines.join(" ")),
            severity: Some(first.severity),
            intent: Some(Intent::ShowWindow {
                pane: Some(Pane::Accounts),
            }),
            link: None,
            enabled: true,
        }
    });
    MenuNotices {
        install: (*footing == Footing::NoClaudeCode).then(|| MenuEntry {
            title: "Claude Code isn’t installed".into(),
            subtitle: Some("Learn how to install it".into()),
            help: None,
            severity: None,
            intent: None,
            link: Some(INSTALL_CLAUDE_CODE.into()),
            enabled: true,
        }),
        switches,
        others,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A warning's mark is the same each time it is made, and two messages differ.
    #[test]
    fn a_warnings_mark_is_stable_and_its_own() {
        assert_eq!(
            mark("ANTHROPIC_API_KEY is set"),
            mark("ANTHROPIC_API_KEY is set")
        );
        assert_ne!(
            mark("ANTHROPIC_API_KEY is set"),
            mark("OPENAI_API_KEY is set")
        );
        assert_eq!(mark(""), "cbf29ce484222325");
    }

    /// A notice is said as a word as well as shown as a symbol. If two severities ever
    /// sounded the same an error would pass for a note, and they order by how pressing they
    /// are.
    ///
    /// PresentationTests.swift's everySeverityLooksAndSoundsLikeItselfAndOrdersByHowPressingItIs,
    /// whose symbols stay the app's.
    #[test]
    fn every_severity_sounds_like_itself_and_orders_by_how_pressing_it_is() {
        let severities = [Severity::Info, Severity::Warning, Severity::Error];
        let spoken: std::collections::BTreeSet<_> =
            severities.iter().map(|s| spoken_severity(*s)).collect();
        assert_eq!(spoken.len(), severities.len());
        let mut sorted = vec![Severity::Error, Severity::Info, Severity::Warning];
        sorted.sort();
        assert_eq!(sorted, severities);
    }

    use crate::LastSwitch;
    use crate::present::present_on;
    use crate::present::testing::{
        Utc, account, awaiting, claude_code, claude_desktop, desktop_account, desktop_warning,
        read, status,
    };
    use pitboard_core::host::Os;

    fn parked_both() -> crate::Status {
        status(
            vec![
                desktop_account("work", false),
                desktop_account("home", false),
            ],
            Vec::new(),
        )
    }

    fn shown(status: crate::Status) -> crate::Snapshot {
        let state = read(status, vec![claude_code(), claude_desktop()]);
        present_on(Os::MacOs, &state, 0, &Utc)
    }

    fn awaiting_notice(shown: &crate::Snapshot) -> PanelNotice {
        shown
            .notices
            .iter()
            .find(|notice| notice.id == "awaiting/desktop")
            .expect("the notice of a sign-in left halfway")
            .clone()
    }

    fn actions(notice: &PanelNotice) -> Vec<(&str, &Intent)> {
        notice
            .actions
            .iter()
            .map(|action| (action.title.as_str(), &action.intent))
            .collect()
    }

    #[test]
    fn an_add_left_halfway_is_a_notice_with_its_way_on_and_its_way_back() {
        let mut read = parked_both();
        read.awaiting = vec![awaiting("travel", "work", false)];
        let notice = awaiting_notice(&shown(read));
        assert_eq!(notice.severity, Severity::Warning);
        assert_eq!(
            notice.title,
            "Adding a Claude Desktop account isn’t finished"
        );
        assert_eq!(
            notice.lines,
            [
                "Claude is signed out for a sign-in, and work is parked.",
                "Sign in to the account you’re adding in Claude, quit Claude, then choose \
                 Finish Adding. Put work Back undoes it."
            ]
        );
        assert_eq!(
            actions(&notice),
            [
                (
                    "Finish Adding…",
                    &Intent::PresentSheet {
                        sheet: Sheet::Add {
                            provider: Some("desktop".into())
                        }
                    }
                ),
                (
                    "Put work Back",
                    &Intent::SwitchTo {
                        qualified: "desktop/work".into()
                    }
                )
            ]
        );
    }

    #[test]
    fn a_sign_in_again_left_halfway_is_said_as_one() {
        let mut read = parked_both();
        read.awaiting = vec![awaiting("home", "work", true)];
        let notice = awaiting_notice(&shown(read));
        assert_eq!(notice.title, "Signing in to home again isn’t finished");
        assert_eq!(
            notice.lines[1],
            "Sign in to home in Claude, quit Claude, then choose Finish Signing In. Put work \
             Back undoes it."
        );
        assert_eq!(
            actions(&notice)[0],
            (
                "Finish Signing In…",
                &Intent::PresentSheet {
                    sheet: Sheet::SignInAgain {
                        provider: "desktop".into(),
                        label: "home".into()
                    }
                }
            )
        );
    }

    #[test]
    fn the_awaited_sign_in_is_said_once() {
        let mut read = parked_both();
        read.awaiting = vec![awaiting("travel", "work", false)];
        read.warnings = vec![desktop_warning(
            "sign_in_awaited",
            "Claude is signed out for a sign-in, and work is parked.",
        )];
        let shown = shown(read);
        assert_eq!(
            shown
                .notices
                .iter()
                .map(|notice| notice.id.as_str())
                .collect::<Vec<_>>(),
            ["awaiting/desktop"]
        );
    }

    #[test]
    fn a_claude_desktop_login_in_doubt_offers_the_account_claude_shows() {
        for code in ["in_use_unconfirmed", "config_names_another"] {
            let read = status(
                vec![
                    desktop_account("work", true),
                    desktop_account("home", false),
                ],
                vec![desktop_warning(code, "Claude’s settings have named home.")],
            );
            let shown = shown(read);
            let notice = shown
                .notices
                .iter()
                .find(|notice| notice.id.starts_with(&format!("warning/{code}/")))
                .expect("the warning's notice");
            assert_eq!(
                notice.title, "The account in use is not confirmed",
                "{code}"
            );
            assert_eq!(
                actions(notice),
                [(
                    "Add the Account Claude Shows…",
                    &Intent::PresentSheet {
                        sheet: Sheet::Add {
                            provider: Some("desktop".into())
                        }
                    }
                )],
                "{code}"
            );
        }
        let claude_code_config = status(
            vec![account(Some("work")).signed_in().build()],
            vec![Warning {
                provider: Some("claude".into()),
                ..desktop_warning("config_names_another", "Claude Code’s config names home.")
            }],
        );
        let shown = shown(claude_code_config);
        assert_eq!(
            shown.notices[0].title,
            "Claude Code’s config names another account"
        );
        assert_eq!(
            shown.notices[0].actions[0].title, "Update Claude Code’s Config",
            "Claude Code's own stays"
        );
    }

    fn stuck_beside(warnings: Vec<Warning>) -> PanelNotice {
        let read = status(
            vec![
                account(Some("work")).signed_in().build(),
                desktop_account("work", false),
                desktop_account("home", false),
            ],
            warnings,
        );
        let mut state = super::super::testing::read(read, vec![claude_code(), claude_desktop()]);
        state.stuck = true;
        let shown = present_on(Os::MacOs, &state, 0, &Utc);
        assert!(
            !shown
                .notices
                .iter()
                .any(|notice| notice.id.starts_with("warning/move_waiting/")),
            "its own notice offers what a stuck switch refuses"
        );
        shown
            .notices
            .into_iter()
            .find(|notice| notice.id == "stuck")
            .expect("the stuck notice")
    }

    #[test]
    fn a_stuck_switch_says_the_claude_desktop_move_its_give_up_settles_too() {
        let stuck = stuck_beside(vec![desktop_warning(
            "move_waiting",
            "A move of work’s login was interrupted. Quit Claude, then use any Claude Desktop \
             account: Pitboard settles the move first.",
        )]);
        assert_eq!(
            stuck.lines,
            [
                "An interrupted switch can’t be finished until Anthropic answers.",
                "Giving up on it keeps every login. Nothing is deleted.",
                "A Claude Desktop move was interrupted too. With Claude quit, Give Up… settles it \
                 or gives it up as well, and keeps every folder.",
            ]
        );
        assert_eq!(
            stuck.actions[0]
                .confirm
                .as_ref()
                .map(|asked| asked.message.as_str()),
            Some(
                "Every login is kept, and nothing is deleted. Pitboard stops trying to finish it \
                 and, with Claude quit, settles or gives up the interrupted Claude Desktop move \
                 too."
            )
        );
        let alone = stuck_beside(Vec::new());
        assert_eq!(alone.lines.len(), 2);
        assert_eq!(
            alone.actions[0]
                .confirm
                .as_ref()
                .map(|asked| asked.message.as_str()),
            Some(
                "Every login is kept, and nothing is deleted. Pitboard stops trying to finish it."
            )
        );
    }

    #[test]
    fn an_add_left_halfway_offers_finish_and_put_back_in_the_menu() {
        let mut read = parked_both();
        read.awaiting = vec![awaiting("travel", "work", false)];
        let menu = shown(read).menu_notices;
        assert_eq!(
            menu.switches
                .iter()
                .map(|entry| entry.title.as_str())
                .collect::<Vec<_>>(),
            ["Finish Adding…", "Put work Back"]
        );
        assert!(menu.switches.iter().all(|entry| entry.subtitle.as_deref()
            == Some("Adding a Claude Desktop account isn’t finished")));
        assert_eq!(
            menu.switches
                .iter()
                .map(|entry| entry.severity)
                .collect::<Vec<_>>(),
            [Some(Severity::Warning), None],
            "only Put work Back switches"
        );
    }

    #[test]
    fn a_switch_of_claude_desktop_says_claude_opens_signed_in() {
        let read = status(
            vec![
                desktop_account("home", true),
                desktop_account("work", false),
            ],
            Vec::new(),
        );
        let mut state = super::super::testing::read(read, vec![claude_code(), claude_desktop()]);
        state.last_switches = vec![LastSwitch {
            provider: "desktop".into(),
            to: "desktop/home".into(),
            follows_at: None,
            restart: None,
            said: None,
            warnings: Vec::new(),
            opens: Some("Claude opens signed in to it.".into()),
        }];
        let shown = present_on(Os::MacOs, &state, 0, &Utc);
        let notice = shown
            .notices
            .iter()
            .find(|notice| notice.id == "switch/desktop")
            .expect("what the switch said");
        assert_eq!(notice.title, "Switched to home");
        assert_eq!(notice.severity, Severity::Info);
        assert_eq!(notice.lines, ["Claude opens signed in to it."]);
    }
}
