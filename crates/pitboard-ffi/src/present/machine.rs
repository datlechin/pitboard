//! What the settings and the window's other panes show of this machine rather than its
//! accounts, as SettingsView.swift, MachinePane.swift and ActivityPane.swift said it from
//! MachineModel.swift: daily renewal and why it cannot be turned on, Renew Now and what it
//! did, doctor's checks with the line over them, every change Pitboard made, and the
//! `pitboard` a terminal runs and how it is kept up to date.
//!
//! What only labels a control, such as "Renew parked logins daily", "Check Again" or "In your
//! terminal", and what is about the app's own system, such as opening at login or linking the
//! command line with an administrator's password, is each app's own.

use super::{Seen, words};
use crate::model::machine::ScheduleFailure;
use crate::model::state::AutoStanding;
use crate::{AutoSwitched, FoundCommandLine, Level, Schedule};
use pitboard_core::autoswitch::Threshold;

/// What the app shows of this machine rather than its accounts. Not called `Machine`, the
/// name the model's tests give the core they script.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MachineShown {
    /// Daily renewal, the settings' switch and what is said around it.
    pub schedule: ScheduleShown,
    /// Renew Now, and what the last renewal did.
    pub renewal: RenewalShown,
    /// Doctor's checks, as the window's machine pane shows them.
    pub checks: ChecksShown,
    /// Every change Pitboard made, as the window's activity pane lists them.
    pub activity: ActivityShown,
    /// The `pitboard` a terminal runs, as the settings show it.
    pub command_line: CommandLineShown,
    /// Switching Claude Code by itself, as the settings show it.
    pub auto_switch: AutoSwitchShown,
}

/// Switching Claude Code by itself before the account in use runs out, as the settings show
/// it: their switch, the share it switches at, what it came to last, and what is said under
/// them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AutoSwitchShown {
    /// Whether the switch shows it as on. Off unless somebody turned it on.
    pub on: bool,
    /// The share of a limit it switches at, a whole percentage, which `Intent::SetAutoSwitch`
    /// takes back.
    pub at: u8,
    /// The lowest and highest shares there can be, for the control that picks one.
    pub lowest: u8,
    pub highest: u8,
    /// Whether the switch and the share can be changed: not until the app's preferences
    /// have been read, where a change would be lost under what they say, nor for good where
    /// they could not be.
    pub enabled: bool,
    /// The share as the control names it: "Switch when a limit reaches 95%".
    pub at_label: String,
    /// What is said under them: what it does and what it never does.
    pub note: String,
    /// While it is on, what it came to last, once there is anything: which account it
    /// watches, how much of its fullest limit is used and when that was read, or why it is
    /// not switching and when it acts again.
    pub standing: Option<String>,
}

/// Daily renewal, as the settings show it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ScheduleShown {
    /// The schedule as the core last read it, `None` before it has been read.
    pub schedule: Option<Schedule>,
    /// Whether the switch shows daily renewal as on: while a change is under way, what was
    /// asked for, rather than snapping back until the scheduler answers; and otherwise
    /// whether the scheduler has it. A machine with no scheduler Pitboard writes to, or one
    /// whose schedule has not been read yet, has it off.
    pub on: bool,
    /// A change is under way, from the moment it is asked for until the schedule has been
    /// read back after it.
    pub changing: bool,
    /// Whether the switch can be pressed: not while a change is under way, when the model
    /// drops a second press, and not to turn renewal on where this copy of the app cannot.
    /// Turning it off always can, which is how a schedule that cannot work goes.
    pub enabled: bool,
    /// How often it runs, while the scheduler has it: "Every day".
    pub runs: Option<String>,
    /// Where the scheduler keeps it, while it has it.
    pub scheduled_in: Option<String>,
    /// Said under the switch while the scheduler does not have it: that this machine has no
    /// scheduler Pitboard writes to, or why this copy cannot schedule renewal.
    pub note: Option<String>,
    /// Why the last change could not be made, until the next is asked for, where `note` does
    /// not say it already.
    pub failed: Option<String>,
}

/// Renew Now, as the settings show it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RenewalShown {
    /// A renewal is under way, from the moment it is asked for until the accounts have been
    /// read again after it: the button holds back.
    pub renewing: bool,
    /// What the button is beside: what it does, until it has been pressed, and then what the
    /// last renewal did, in the words `pitboard renew` says it in.
    pub note: String,
}

/// Doctor's checks, as the window shows them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChecksShown {
    /// Every check, in the order doctor makes them.
    pub lines: Vec<CheckLine>,
    /// Over them, the last line of `pitboard doctor`, so the two never disagree, once there
    /// are checks to head.
    pub summary: Option<String>,
    /// A check is under way: Check Again holds back, and the line over the checks shows it.
    pub checking: bool,
    /// When the checks shown were made, beside the line over them, once nothing is checking:
    /// "Checked at 14:05", or with its weekday once that is not today.
    pub checked: Option<String>,
    /// What stands in for the checks before there are any: "Checking this Mac…".
    pub waiting: Option<String>,
}

/// One of doctor's checks. Not called `CheckRow`, which MachinePane.swift has a view of.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CheckLine {
    /// Its place in the list, in the order doctor makes the checks, from 0. A check's code is
    /// not its own: doctor makes one check of each enrolled account's parked login, and every
    /// one of a tool's is `parked_login`, or `codex_parked_login`, so a list whose rows
    /// claimed the code would have two or more rows of one identity.
    pub id: u64,
    /// What kind of check it is, as `pitboard doctor --json` gives a check's `code`. Several
    /// checks can share one.
    pub code: String,
    pub name: String,
    pub level: Level,
    /// The level in words, where a symbol shows it: "Passed", "Worth looking at", "Failed".
    pub spoken_level: String,
    /// What it looked at.
    pub detail: String,
    /// What to do about it, for a check that did not pass and says what.
    pub advice: Option<String>,
}

/// Every change Pitboard made, as the window lists them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ActivityShown {
    /// The changes, newest first.
    pub lines: Vec<ActivityLine>,
    /// What stands in for the list while it has nothing in it.
    pub empty: Option<EmptyList>,
}

/// One change Pitboard made, as the activity list says it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ActivityLine {
    /// Its place in the list, newest first from 0. The log has no id of its own: two changes
    /// can share a time, a verb and an account, and a list whose rows claim one identity
    /// drops all but one of them.
    pub id: u64,
    /// When, as the person's own calendar and clock say it, or the log's own text where that
    /// does not read as a time.
    pub date: String,
    /// What was done, in a word: "Switch", "Enrol".
    pub change: String,
    /// What it was done to, as the log keeps it.
    pub account: String,
    /// How it ended: "Done", or what stopped it.
    pub result: String,
    /// It ended as asked, which a list shows quieter than one that did not.
    pub done: bool,
    /// Who asked for it: "Pitboard app", "Command line".
    pub asked_by: String,
}

/// What stands in for a list with nothing in it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EmptyList {
    pub title: String,
    /// Why there is nothing, or what will be there.
    pub detail: String,
}

/// The `pitboard` a terminal runs, as the settings show it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CommandLineShown {
    /// The one found, once it has been looked for.
    pub found: Option<FoundCommandLine>,
    /// Where it is, or "Not installed" where none was found. `None` until it has first been
    /// looked for.
    pub in_terminal: Option<String>,
    /// How it is kept up to date, once one was found.
    pub update_note: Option<String>,
    /// Whether the app offers to put the command line inside it on the `PATH`: none was
    /// found, and this copy has one inside it that a link would keep reaching. How it links
    /// it is the app's own, with its system's password prompt, after which it sends
    /// `Intent::LookForCommandLine`.
    pub offers_link: bool,
    /// Why it is not offered, where none was found and this copy runs from a temporary place.
    pub cannot_link: Option<String>,
}

/// What the app shows of this machine.
pub(crate) fn machine(seen: &Seen) -> MachineShown {
    MachineShown {
        schedule: schedule(seen),
        renewal: renewal(seen),
        checks: checks(seen),
        activity: activity(seen),
        command_line: command_line(seen),
        auto_switch: auto_switch(seen),
    }
}

/// Switching Claude Code by itself, from the app's preferences. How soon running sessions
/// follow is promised only while the last read found no file behind the keychain.
fn auto_switch(seen: &Seen) -> AutoSwitchShown {
    let state = seen.state;
    let at = state.preferences.threshold().percent();
    let follows = if state
        .warnings
        .iter()
        .any(|warning| warning.code == "fallback_login")
    {
        format!(
            "While a login file is left behind the keychain, Claude Code sessions already \
             running {}.",
            pitboard_core::words::kept_until_renewed()
        )
    } else {
        format!(
            "Sessions already running follow within about {} seconds.",
            pitboard_core::switch::ADOPTION_CEILING_SECONDS
        )
    };
    AutoSwitchShown {
        on: state.preferences.auto_switch,
        at,
        lowest: Threshold::LOWEST,
        highest: Threshold::HIGHEST,
        enabled: state.preferences_have_been_read(),
        at_label: format!("Switch when a limit reaches {at}%"),
        note: format!(
            "Pitboard switches Claude Code to another of your accounts with room below \
             {at}% in every limit, once a limit of the one in use reaches {at}%, while the app \
             is open. {follows} It never switches back by itself, and never switches Codex: a \
             running codex keeps its account until restarted."
        ),
        standing: state
            .auto_standing
            .as_ref()
            .filter(|_| state.preferences.auto_switch)
            .map(|standing| auto_standing(seen, standing)),
    }
}

/// What switching Claude Code by itself came to last, in the words `pitboard watch` says it
/// in, with its clock times as the person's own clock says them.
fn auto_standing(seen: &Seen, standing: &AutoStanding) -> String {
    match standing {
        AutoStanding::Came(AutoSwitched::Watching {
            account,
            used,
            as_of,
            held_until,
        }) => {
            let mut said = format!("Watching {account}");
            if let Some(used) = used {
                said.push_str(&format!(": {used}"));
            }
            if let Some(at) = as_of {
                said.push_str(&format!(", as of {}", seen.clock(*at)));
            }
            said.push('.');
            if let Some(until) = held_until {
                said.push_str(&format!(
                    " Anthropic holds Pitboard off asking again until {}.",
                    seen.clock(*until)
                ));
            }
            said
        }
        AutoStanding::Came(AutoSwitched::Waiting { from, used, until }) => format!(
            "{from} has used {used}. Pitboard tries again at {}.",
            seen.clock(*until)
        ),
        AutoStanding::Came(AutoSwitched::Switched { from, to, used, .. }) => {
            format!("Switched Claude Code from {from} to {to}: {from} had used {used}.")
        }
        AutoStanding::Came(AutoSwitched::Skipped {
            from,
            used,
            why,
            until,
            ..
        }) => {
            let said = format!("{from} has used {used}. Pitboard is not switching: {why}.");
            match until {
                Some(until) => format!("{said} It decides again at {}.", seen.clock(*until)),
                None => said,
            }
        }
        AutoStanding::Came(AutoSwitched::NotWatching { why, until, .. }) => {
            let said = format!("Pitboard is not switching Claude Code: {why}.");
            match until {
                Some(until) => format!("{said} It asks again at {}.", seen.clock(*until)),
                None => said,
            }
        }
        AutoStanding::Stopped {
            message,
            until: Some(until),
        } => format!(
            "Pitboard could not switch Claude Code, and tries again at {}: {message}",
            seen.clock(*until)
        ),
        AutoStanding::Stopped {
            message,
            until: None,
        } => format!("Pitboard cannot tell whether to switch Claude Code: {message}"),
    }
}

/// Daily renewal, as SettingsView.swift showed it from MachineModel.swift's `schedule`,
/// `scheduling`, `cannotSchedule` and `scheduleFailed`.
fn schedule(seen: &Seen) -> ScheduleShown {
    let machine = &seen.state.machine;
    let renews_daily = matches!(machine.schedule, Some(Schedule::Installed { .. }));
    // Not known until the command line inside this copy has been read, and then only where
    // it is not one a schedule would keep reaching.
    let cannot = machine
        .own
        .filter(|own| !own.lasting())
        .map(|own| words::cannot_schedule(seen.os, own.temporary));
    let (runs, scheduled_in, note) = match &machine.schedule {
        Some(Schedule::Installed {
            path,
            every_seconds,
        }) => (
            Some(words::runs_every(*every_seconds)),
            Some(path.clone()),
            None,
        ),
        Some(Schedule::Unsupported) => (None, None, Some(words::no_scheduler(seen.os))),
        Some(Schedule::Absent) | None => (None, None, cannot.clone()),
    };
    // Said once: a refusal the note under the switch says already is not said again.
    let failed = machine
        .schedule_failed
        .as_ref()
        .map(|failed| match failed {
            ScheduleFailure::CannotSchedule { temporary } => {
                words::cannot_schedule(seen.os, *temporary)
            }
            ScheduleFailure::Refused { message } => message.clone(),
        })
        .filter(|failed| note.as_ref() != Some(failed));
    ScheduleShown {
        schedule: machine.schedule.clone(),
        on: machine.scheduling.unwrap_or(renews_daily),
        changing: machine.scheduling.is_some(),
        enabled: machine.scheduling.is_none() && (renews_daily || cannot.is_none()),
        runs,
        scheduled_in,
        note,
        failed,
    }
}

/// Renew Now, and what the last renewal did, in `pitboard renew`'s words, or why it was
/// refused, in the core's: a run that renewed nothing because it was refused does not read
/// as one where nothing was due.
fn renewal(seen: &Seen) -> RenewalShown {
    let machine = &seen.state.machine;
    RenewalShown {
        renewing: machine.renewing,
        note: match &machine.renewals {
            None => words::RENEWS_WHAT_IS_DUE.to_owned(),
            Some(Ok(renewals)) => crate::renewal_note(renewals),
            Some(Err(refused)) => refused.to_string(),
        },
    }
}

/// Doctor's checks, as MachinePane.swift showed them.
fn checks(seen: &Seen) -> ChecksShown {
    let machine = &seen.state.machine;
    let checking = machine.checking > 0;
    let any = !machine.checks.is_empty();
    ChecksShown {
        lines: (0..)
            .zip(&machine.checks)
            .map(|(id, check)| CheckLine {
                id,
                code: check.code.clone(),
                name: check.name.clone(),
                level: check.level,
                spoken_level: words::spoken_level(check.level).to_owned(),
                detail: check.detail.clone(),
                advice: (check.level != Level::Ok && !check.advice.is_empty())
                    .then(|| check.advice.clone()),
            })
            .collect(),
        summary: any.then(|| crate::doctor_summary(&machine.checks)),
        checking,
        checked: machine
            .checked_at
            .filter(|_| any && !checking)
            .map(|at| words::checked_at(&seen.clock(at))),
        waiting: (!any).then(|| words::checking(seen.os)),
    }
}

/// Every change, newest first, as ActivityPane.swift listed them.
fn activity(seen: &Seen) -> ActivityShown {
    let log = &seen.state.machine.log;
    ActivityShown {
        lines: (0..)
            .zip(log)
            .map(|(id, change)| ActivityLine {
                id,
                date: seen.date_and_time(&change.at),
                change: words::change_verb(&change.verb),
                account: change.subject.clone(),
                result: words::change_outcome(&change.outcome),
                done: change.outcome == "ok",
                asked_by: words::change_caller(&change.caller),
            })
            .collect(),
        empty: log.is_empty().then(|| {
            let (title, detail) = words::NO_ACTIVITY;
            EmptyList {
                title: title.into(),
                detail: detail.into(),
            }
        }),
    }
}

/// The `pitboard` a terminal runs, as SettingsView.swift's command line pane showed it.
fn command_line(seen: &Seen) -> CommandLineShown {
    let machine = &seen.state.machine;
    let own = machine.own.unwrap_or_default();
    let nowhere = matches!(machine.command_line, Some(FoundCommandLine::Nowhere));
    CommandLineShown {
        found: machine.command_line.clone(),
        in_terminal: machine.command_line.as_ref().map(|found| match found {
            FoundCommandLine::Bundled { path } | FoundCommandLine::Another { path } => path.clone(),
            FoundCommandLine::Nowhere => "Not installed".into(),
        }),
        update_note: match machine.command_line {
            Some(FoundCommandLine::Bundled { .. }) => Some(words::update_note(true).to_owned()),
            Some(FoundCommandLine::Another { .. }) => Some(words::update_note(false).to_owned()),
            Some(FoundCommandLine::Nowhere) | None => None,
        },
        offers_link: nowhere && own.lasting(),
        cannot_link: (nowhere && own.temporary).then(|| words::cannot_link(seen.os)),
    }
}
