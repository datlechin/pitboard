//! What Pitboard has to tell somebody that is not an account, from Notices.swift: notices in
//! the order they matter, the headings of warnings by code, what a tool's last switch still
//! has to say, and what the menu says of all of it, from MenuBarContent.swift.

use super::words;
use super::{
    Footing, MenuEntry, MenuNotices, NoticeAction, PanelNotice, Question, Seen, Severity,
    spoken_severity,
};
use crate::model::RunOutNotice;
use crate::model::advice::Advice;
use crate::model::state::split;
use crate::model::{Intent, LastSwitch, Pane};
use crate::{Adoption, Warning};
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

/// What the read warns about, but for the one its failure already says. A failure's message
/// is not one of its warnings, and dropping the first of them hid one that was.
fn other_warnings<'a>(seen: &'a Seen) -> impl Iterator<Item = &'a Warning> {
    let problem = seen.problem();
    seen.state
        .warnings
        .iter()
        .filter(move |warning| Some(warning.message.as_str()) != problem)
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
        said.push(notice(
            "stuck".into(),
            Severity::Error,
            "An interrupted switch is waiting".into(),
            vec![
                reason,
                "Giving up on it keeps every login. Nothing is deleted.".into(),
            ],
            vec![NoticeAction {
                title: "Give Up…".into(),
                intent: Intent::AbandonStuckSwitch,
                dismisses: false,
                switches: false,
                enabled: true,
                confirm: Some(Question {
                    title: "Give up on the interrupted switch?".into(),
                    message: "Every login is kept, and nothing is deleted. Pitboard stops \
                              trying to finish it."
                        .into(),
                    confirm: "Give Up".into(),
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
            words::warning_heading(warning).into(),
            vec![warning.message.clone()],
            Vec::new(),
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
    said
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
    let mut lines: Vec<String> = last.said.iter().cloned().chain(restart).collect();
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

/// The notification that tells somebody an account in use has run out, worded as the window's
/// notice is: its title the account and the limit, the tool beneath it beside another tool's
/// accounts, and the account offered with what it has left. Its id names the account, the
/// limit and the reset, so the same run-out is one notification.
pub(crate) fn run_out_notice(advice: &Advice) -> RunOutNotice {
    let limit = said::limit_name(&advice.window.kind, advice.window.length_seconds);
    RunOutNotice {
        id: format!(
            "{}-{}",
            advice.key_of(),
            advice.window.resets_at.unwrap_or(0)
        ),
        title: words::ran_out(&advice.ran, &limit),
        subtitle: advice.tool.clone(),
        body: words::room_left(&advice.instead, advice.left),
        switch_to: Some(advice.switch_to.clone()),
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
    let follows = match adoption {
        Adoption::Follows { within_seconds } => {
            format!("Sessions already running follow within {within_seconds} seconds.")
        }
        Adoption::Renewal { path } => format!(
            "Sessions already running {}: {path} is there.",
            said::kept_until_renewed()
        ),
        Adoption::Restart { program } => {
            format!("Restart any running `{program}` for this to take effect.")
        }
    };
    RunOutNotice {
        id: format!("auto/{from}/{to}/{at}"),
        title: format!("Switched Claude Code to {to}"),
        subtitle: None,
        body: format!("{from} had used {used}. {follows}"),
        switch_to: None,
    }
}

/// Where Pitboard would have switched Claude Code by itself and did not: how much of which
/// limit, and why, in the core's words.
pub(crate) fn auto_skipped_notice(from: &str, used: &str, code: &str, why: &str) -> RunOutNotice {
    RunOutNotice {
        id: format!("auto-skipped/{code}"),
        title: "Claude Code was not switched".into(),
        subtitle: None,
        body: format!("{from} has used {used}. {}.", capitalised(why)),
        switch_to: None,
    }
}

/// A warning that stands until somebody acts, posted once while it does, since nobody may
/// have the window open: a login replaced outside Pitboard. Titled and identified as its
/// notice in the window is, and said in the core's words.
pub(crate) fn standing_notice(warning: &Warning) -> RunOutNotice {
    RunOutNotice {
        id: warning_id(warning),
        title: words::warning_heading(warning).into(),
        subtitle: None,
        body: warning.message.clone(),
        switch_to: None,
    }
}

/// A switch Pitboard tried by itself and could not make, in the core's words.
pub(crate) fn auto_refused_notice(code: &str, message: &str) -> RunOutNotice {
    RunOutNotice {
        id: format!("auto-refused/{code}"),
        title: "Pitboard could not switch Claude Code".into(),
        subtitle: None,
        body: message.into(),
        switch_to: None,
    }
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// What the menu says of the notices: advice as items that switch; anything else to know
/// about, past a note, as one item that opens the window where it is said in full; and the
/// one thing to do on a machine that has no tool.
pub(crate) fn menu_notices(seen: &Seen, footing: &Footing, notices: &[PanelNotice]) -> MenuNotices {
    let pressing: Vec<&PanelNotice> = notices
        .iter()
        .filter(|notice| notice.severity != Severity::Info)
        .collect();
    let switches = pressing
        .iter()
        .filter_map(|notice| {
            let action = notice.actions.iter().find(|action| action.switches)?;
            Some(MenuEntry {
                title: action.title.clone(),
                subtitle: Some(notice.title.clone()),
                help: None,
                severity: None,
                intent: Some(action.intent.clone()),
                link: None,
                enabled: seen.state.switch_under_way().is_none(),
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
}
