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
use crate::model::AtLimit;
use crate::model::machine::ScheduleFailure;
use crate::model::state::AutoStanding;
use crate::{AutoSwitched, Check, FoundCommandLine, Level, Schedule};
use pitboard_core::duty::autoswitch::Threshold;

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
    pub at_limit: AtLimitShown,
    /// Warming up accounts, as the settings show it.
    pub warm: WarmShown,
}

/// Warming up accounts, as the settings show it: the span of each day, each Claude Code
/// account with whether it is warmed up, what comes next, and what is said under them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WarmShown {
    /// The span, in minutes of the day, which `Intent::SetWarmRange` takes back: `to` up to
    /// 24 hours, and a `from` later than it runs past midnight.
    pub from: u16,
    pub to: u16,
    /// The span in words, on the person's clock: "6:00 AM to 10:00 PM", "6:00 AM to
    /// midnight", "All day".
    pub range_label: String,
    /// Each Claude Code account enrolled under a name, in the order they were enrolled.
    pub accounts: Vec<WarmAccountShown>,
    /// What comes next, once anything is turned on: the next warm-up, or why none is made.
    pub standing: Option<String>,
    /// What is said under them: what a warm-up does and what it never does.
    pub note: String,
    /// Whether anything here can be changed: once the core has said what it is.
    pub enabled: bool,
    pub terms: TermsShown,
}

/// One Claude Code account, as the settings' warm-up lists it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WarmAccountShown {
    /// Its label with its tool, which `Intent::SetWarm` takes back.
    pub qualified: String,
    /// Its name.
    pub title: String,
    /// The plan its login says it is on, where one does.
    pub plan: Option<String>,
    /// Whether it is warmed up.
    pub on: bool,
    /// Where its warm-up stands, while it is turned on: "Warms up again at 22:50".
    pub standing: Option<String>,
}

// What the app does once a limit of the account in use reaches a share, as the settings show it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AtLimitShown {
    pub choice: AtLimit,
    pub choices: Vec<AtLimitChoice>,
    // a whole percentage, which `Intent::SetAtLimit` takes back
    pub at: u8,
    pub lowest: u8,
    pub highest: u8,
    // not before the preferences are read, nor ever where they could not be
    pub enabled: bool,
    // and while something happens at the share
    pub share_enabled: bool,
    // "When a limit reaches 95%"
    pub at_label: String,
    // the control that sets the share
    pub share_label: String,
    pub note: Option<String>,
    // under Switch, what switching came to last
    pub standing: Option<String>,
    // under Switch
    pub terms: Option<TermsShown>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AtLimitChoice {
    pub choice: AtLimit,
    pub title: String,
}

// A sentence and the page that says whether Anthropic's and OpenAI's terms allow it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TermsShown {
    pub sentence: String,
    pub link_title: String,
    pub link: String,
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
    // what daily renewal does, under its switch
    pub about: String,
}

/// Doctor's checks, as the window shows them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChecksShown {
    // every check, in the order doctor makes them, then a notification the app's system refused
    pub lines: Vec<CheckLine>,
    // over them, in the words of the last line of `pitboard doctor`, once there are checks to head
    pub summary: Option<String>,
    // the worst of the checks, which the line over them shows in its colour and symbol
    pub level: Level,
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
        at_limit: at_limit(seen),
        warm: warm(seen),
    }
}

/// Warming up accounts, from what the core last said of it.
fn warm(seen: &Seen) -> WarmShown {
    let state = seen.state;
    let overview = state.warm.overview.as_ref();
    let range = overview.map_or(pitboard_core::duty::warm::Range::DEFAULT, |o| o.range);
    let clock = |at| seen.clock(at);
    let accounts: Vec<WarmAccountShown> = overview
        .map(|overview| {
            overview
                .accounts
                .iter()
                .map(|choice| WarmAccountShown {
                    qualified: choice.qualified.clone(),
                    title: choice.account.clone(),
                    plan: seen
                        .accounts()
                        .iter()
                        .find(|account| account.qualified.as_deref() == Some(&choice.qualified))
                        .and_then(|account| account.plan.clone()),
                    on: choice.on,
                    standing: choice.standing.as_ref().map(|standing| {
                        words::capitalised(&pitboard_core::words::warm_standing(standing, &clock))
                    }),
                })
                .collect()
        })
        .unwrap_or_default();
    let unreadable = state.warm.unreadable.as_deref().map(words::capitalised);
    let standing = unreadable.or_else(|| {
        overview
            .filter(|_| state.warm.any_on())
            .and_then(|overview| {
                if let Some(halt) = &overview.halted {
                    return Some(format!(
                        "Pitboard is not warming up accounts: {}.",
                        pitboard_core::words::not_warming(halt)
                    ));
                }
                if let Some(failed) = &state.warm.failed {
                    return Some(format!("The last warm-up came to nothing: {failed}"));
                }
                let (account, at) = overview.next(seen.now)?;
                Some(if at <= seen.now {
                    format!("Warming up {account} now.")
                } else {
                    format!("Next warm-up: {account} at {}.", seen.clock(at))
                })
            })
    });
    WarmShown {
        from: range.from(),
        to: range.to(),
        range_label: words::warm_hours(range.from(), range.to(), |minutes| {
            seen.time_of_day(minutes)
        })
        .unwrap_or_else(|| range.to_string()),
        accounts,
        standing,
        note: "Pitboard starts the five-hour period of each account turned on as soon as the last \
               one resets, within these hours, while the app is open, so it resets as early as it \
               can. Claude Code sends one short request with that account's own login. Pitboard \
               warms up Claude Code accounts only, and none with a limit used up."
            .into(),
        enabled: overview.is_some(),
        terms: terms(),
    }
}

fn terms() -> TermsShown {
    TermsShown {
        sentence: format!("{}.", pitboard_core::words::TERMS_MAY_COVER_THIS),
        link_title: "Is this allowed?".into(),
        link: pitboard_core::words::IS_THIS_ALLOWED.into(),
    }
}

// How soon running sessions follow is promised only while the last read found no file behind
// the store.
fn at_limit(seen: &Seen) -> AtLimitShown {
    let state = seen.state;
    let at = state.preferences.threshold().percent();
    let choice = state.preferences.at_limit();
    let follows = if state
        .warnings
        .iter()
        .any(|warning| warning.code == "fallback_login")
    {
        format!(
            "While a login file is left behind {}, Claude Code sessions already running {}.",
            pitboard_core::words::store_in_front(seen.os),
            pitboard_core::words::kept_until_renewed()
        )
    } else {
        format!(
            "Sessions already running follow within about {} seconds.",
            pitboard_core::switch::ADOPTION_CEILING_SECONDS
        )
    };
    let note = match choice {
        AtLimit::Nothing => None,
        AtLimit::Notify => Some(format!(
            "Pitboard notifies you once for each limit of the account in use that reaches {at}%, \
             while the app is open, with a Switch button where another account has room."
        )),
        AtLimit::Switch => Some(format!(
            "Pitboard switches Claude Code to another of your accounts with room below {at}% in \
             every limit, once a limit of the one in use reaches {at}%, while the app is open. \
             {follows} It never switches back by itself. It notifies you of Codex instead: a \
             running codex keeps its account until restarted."
        )),
    };
    let switching = choice == AtLimit::Switch;
    AtLimitShown {
        choice,
        choices: [
            (AtLimit::Nothing, "Do Nothing", "Do nothing"),
            (AtLimit::Notify, "Notify Me", "Notify me"),
            (
                AtLimit::Switch,
                "Switch Automatically",
                "Switch automatically",
            ),
        ]
        .into_iter()
        .map(|(choice, title, sentence)| AtLimitChoice {
            choice,
            title: words::titled(seen.os, title, sentence),
        })
        .collect(),
        at,
        lowest: Threshold::LOWEST,
        highest: Threshold::HIGHEST,
        enabled: state.preferences_have_been_read(),
        share_enabled: state.preferences_have_been_read() && choice != AtLimit::Nothing,
        at_label: format!("When a limit reaches {at}%"),
        share_label: "Share of a limit".into(),
        note,
        standing: state
            .auto_standing
            .as_ref()
            .filter(|_| switching)
            .map(|standing| auto_standing(seen, standing)),
        terms: switching.then(terms),
    }
}

/// What switching Claude Code by itself came to last, as the line under the setting says it:
/// the core's words for the account watched and each reason, in sentences of the app's own,
/// which `pitboard watch` frames apart, with clock times as the person's own clock says them.
fn auto_standing(seen: &Seen, standing: &AutoStanding) -> String {
    match standing {
        AutoStanding::Came(AutoSwitched::Watching {
            account,
            used,
            as_of,
            held_until,
        }) => pitboard_core::words::watching(
            account,
            used.as_deref(),
            as_of.map(|at| seen.clock(at)).as_deref(),
            held_until.map(|until| seen.clock(until)).as_deref(),
        ),
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
        about: words::renewal_about(
            seen.os,
            seen.accounts()
                .iter()
                .any(|account| crate::model::state::in_its_app(&account.provider)),
        ),
    }
}

/// Doctor's checks, as MachinePane.swift showed them.
fn checks(seen: &Seen) -> ChecksShown {
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
            name: "Notifications".into(),
            level: Level::Warn,
            detail: words::not_posted(&refused.title, &seen.clock(refused.at), &refused.reason),
            advice: String::new(),
        });
    let checks: Vec<Check> = machine.checks.iter().cloned().chain(refused).collect();
    ChecksShown {
        lines: (0..)
            .zip(&checks)
            .map(|(id, check)| CheckLine {
                id,
                code: check.code.clone(),
                name: words::check_name(&check.code, &check.name),
                level: check.level,
                spoken_level: words::spoken_level(check.level).to_owned(),
                detail: words::check_text(&check.detail),
                advice: (check.level != Level::Ok && !check.advice.is_empty())
                    .then(|| words::check_text(&check.advice)),
            })
            .collect(),
        summary: any.then(|| crate::doctor_summary(&checks)),
        level: crate::doctor_level(&checks),
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
            let (title, detail) = words::no_activity(seen.os);
            EmptyList {
                title,
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

#[cfg(test)]
mod tests {
    use super::{CheckLine, ChecksShown};
    use crate::launch::Check;
    use crate::present::present_on;
    use crate::present::testing::{
        Unreadable, Utc, account, claude_code, claude_desktop, desktop_account, read, status,
    };
    use crate::{Level, LocalTime, PlatformError};
    use pitboard_core::duty::warm::{Overview, Range};
    use pitboard_core::host::Os;

    // An American's clock, in 12 hours.
    struct Twelve;

    impl LocalTime for Twelve {
        fn clock(&self, epoch: i64, with_weekday: bool) -> Result<String, PlatformError> {
            Utc.clock(epoch, with_weekday)
        }

        fn same_day(&self, first: i64, second: i64) -> Result<bool, PlatformError> {
            Utc.same_day(first, second)
        }

        fn date_and_time(&self, epoch: i64) -> Result<String, PlatformError> {
            Utc.date_and_time(epoch)
        }

        fn time_of_day(&self, minutes: u16) -> Result<String, PlatformError> {
            let (hours, minutes) = (minutes / 60, minutes % 60);
            let half = if hours < 12 { "AM" } else { "PM" };
            Ok(format!("{}:{minutes:02} {half}", (hours + 11) % 12 + 1))
        }
    }

    fn hours(from: u16, to: u16, clock: &dyn LocalTime) -> String {
        let mut state = read(status(Vec::new(), Vec::new()), vec![claude_code()]);
        state.warm.overview = Some(Overview {
            range: Range::new(from, to).expect("a span"),
            accounts: Vec::new(),
            halted: None,
        });
        present_on(Os::Windows, &state, 0, clock)
            .machine
            .warm
            .range_label
    }

    // The summary said 06:00 to 22:00 beside time pickers in AM and PM.
    #[test]
    fn the_range_is_said_on_the_persons_clock() {
        assert_eq!(hours(6 * 60, 22 * 60, &Twelve), "6:00 AM to 10:00 PM");
        assert_eq!(hours(22 * 60 + 30, 6 * 60, &Twelve), "10:30 PM to 6:00 AM");
        assert_eq!(hours(0, 12 * 60, &Twelve), "12:00 AM to 12:00 PM");
        assert_eq!(hours(6 * 60, 22 * 60, &Utc), "06:00 to 22:00");
        assert_eq!(
            hours(6 * 60, 22 * 60, &Unreadable),
            "06:00 to 22:00",
            "as the core writes it where the clock cannot say"
        );
    }

    #[test]
    fn a_range_to_the_end_of_the_day_ends_at_midnight() {
        assert_eq!(hours(6 * 60, 24 * 60, &Twelve), "6:00 AM to midnight");
        assert_eq!(hours(6 * 60, 0, &Utc), "06:00 to midnight");
    }

    #[test]
    fn a_range_of_the_whole_day_is_all_day() {
        assert_eq!(hours(0, 24 * 60, &Twelve), "All day");
        assert_eq!(hours(0, 24 * 60, &Unreadable), "All day");
    }

    fn checks(checks: &[(&str, &str, Level, &str, &str)]) -> ChecksShown {
        let mut state = read(status(Vec::new(), Vec::new()), vec![claude_code()]);
        state.machine.checks = checks
            .iter()
            .map(|&(code, name, level, detail, advice)| Check {
                code: code.into(),
                name: name.into(),
                level,
                detail: detail.into(),
                advice: advice.into(),
            })
            .collect();
        present_on(Os::Windows, &state, 0, &Utc).machine.checks
    }

    fn lines(shown: &[(&str, &str, Level, &str, &str)]) -> Vec<CheckLine> {
        checks(shown).lines
    }

    // The line over the checks is as bad as the worst of them, in its colour as in its words.
    #[test]
    fn the_line_over_the_checks_takes_the_worst_level() {
        let level = |levels: &[Level]| {
            let shown: Vec<_> = levels
                .iter()
                .map(|&level| ("state", "accounts", level, "", ""))
                .collect();
            let shown = checks(&shown);
            (shown.level, shown.summary)
        };
        assert_eq!(
            level(&[Level::Ok, Level::Ok]),
            (
                Level::Ok,
                Some("Everything Pitboard checks is in order.".into())
            )
        );
        assert_eq!(
            level(&[Level::Warn, Level::Ok]),
            (Level::Warn, Some("One thing is worth looking at.".into()))
        );
        assert_eq!(
            level(&[Level::Ok, Level::Fail, Level::Warn]),
            (
                Level::Fail,
                Some("1 broken: do not switch accounts until fixed.".into())
            )
        );
        assert_eq!(level(&[]), (Level::Ok, None));
    }

    fn names(checks: &[(&str, &str)]) -> Vec<String> {
        let checks: Vec<_> = checks
            .iter()
            .map(|&(code, name)| (code, name, Level::Ok, "", ""))
            .collect();
        lines(&checks).into_iter().map(|line| line.name).collect()
    }

    // Doctor names a check for a terminal's table: "account personal", "config file".
    #[test]
    fn each_check_is_named_in_sentence_case() {
        assert_eq!(
            names(&[
                ("config_file", "config file"),
                ("parked_login", "account personal"),
                ("interrupted_switch", "interrupted switch"),
                ("claude_version", "Claude Code build"),
                ("no_argv", "PITBOARD_NO_ARGV"),
                ("machine_id", "machine id"),
            ]),
            [
                "Config file",
                "Account personal",
                "Interrupted switch",
                "Claude Code build",
                "PITBOARD_NO_ARGV",
                "Machine ID",
            ]
        );
    }

    #[test]
    fn storage_v5_is_named_for_what_it_is() {
        assert_eq!(
            names(&[("storage_v5", "storage v5")]),
            ["Claude Code’s new login storage"]
        );
    }

    #[test]
    fn a_prose_detail_starts_with_a_capital_and_data_keeps_its_case() {
        let details: Vec<String> = lines(&[
            ("storage_v5", "storage v5", Level::Ok, "inactive", ""),
            ("state", "accounts", Level::Ok, "none enrolled yet", ""),
            (
                "network",
                "network",
                Level::Ok,
                "direct: no variable names a proxy",
                "",
            ),
            (
                "desktop_settings",
                "Claude settings",
                Level::Ok,
                "config.json read",
                "",
            ),
            (
                "home",
                "Pitboard home",
                Level::Ok,
                r"C:\Users\dana\AppData\Local\Pitboard",
                "",
            ),
            (
                "identity",
                "identity",
                Level::Ok,
                "dana@example.com · org 1f2e",
                "",
            ),
            (
                "claude_version",
                "Claude Code build",
                Level::Ok,
                "2.1.299",
                "",
            ),
            ("state", "accounts", Level::Ok, "1 enrolled", ""),
            (
                "pending_parks",
                "parks being reclaimed",
                Level::Ok,
                "none outstanding",
                "",
            ),
        ])
        .into_iter()
        .map(|line| line.detail)
        .collect();
        assert_eq!(
            details,
            [
                "Inactive",
                "None enrolled yet",
                "Direct: no variable names a proxy",
                "config.json read",
                r"C:\Users\dana\AppData\Local\Pitboard",
                "dana@example.com · org 1f2e",
                "2.1.299",
                "1 enrolled",
                "None outstanding",
            ]
        );
    }

    // A terminal marks a command with backticks; an app shows them as typed.
    #[test]
    fn no_check_line_says_a_backtick() {
        let shown = lines(&[
            (
                "identity",
                "identity",
                Level::Warn,
                "Claude Code has not recorded a signed-in account",
                "It writes this after its first successful call. Run `claude` once.",
            ),
            (
                "desktop_login",
                "Claude Desktop in use",
                Level::Warn,
                "`work` is recorded in use and also holds a parked login",
                "Run `pitboard enroll work --sign-in`.",
            ),
        ]);
        assert_eq!(
            shown[0].advice.as_deref(),
            Some("It writes this after its first successful call. Run claude once.")
        );
        assert_eq!(
            shown[1].detail,
            "work is recorded in use and also holds a parked login"
        );
        assert_eq!(
            shown[1].advice.as_deref(),
            Some("Run pitboard enroll work --sign-in.")
        );
    }

    #[test]
    fn the_renewal_note_says_a_claude_desktop_login_is_never_renewed() {
        let about = |accounts| {
            let state = read(
                status(accounts, Vec::new()),
                vec![claude_code(), claude_desktop()],
            );
            present_on(Os::MacOs, &state, 0, &Utc).machine.renewal.about
        };
        let claude_code_alone = about(vec![account(Some("work")).signed_in().build()]);
        assert_eq!(
            claude_code_alone,
            "A parked login is renewed whenever Pitboard runs, and otherwise not, so one you \
             leave alone for weeks expires and needs a browser sign-in. Daily renewal hands \
             that to your Mac’s own scheduler. It renews your parked logins and does nothing \
             else: it never switches account and never asks for usage."
        );
        assert_eq!(
            about(vec![
                account(Some("work")).signed_in().build(),
                desktop_account("work", true)
            ]),
            format!("{claude_code_alone} A parked Claude Desktop login is never renewed.")
        );
    }

    #[test]
    fn windows_sets_warm_ups_as_macos_does() {
        use pitboard_core::duty::warm::{Choice, Halt, Overview, Range, Standing};
        let mut state = read(
            status(vec![account(Some("work")).signed_in().build()], Vec::new()),
            vec![claude_code()],
        );
        let overview = |on: bool| Overview {
            range: Range::DEFAULT,
            halted: Some(Halt::ProgramMissing),
            accounts: vec![Choice {
                account: "work".into(),
                qualified: "claude/work".into(),
                on,
                standing: on.then_some(Standing::Due),
                now: Standing::Due,
            }],
        };
        for (on, said) in [
            (false, None),
            (
                true,
                Some(
                    "Pitboard is not warming up accounts: Claude Code is not installed where \
                     Pitboard looks.",
                ),
            ),
        ] {
            state.warm.overview = Some(overview(on));
            let windows = present_on(Os::Windows, &state, 0, &Utc).machine.warm;
            assert_eq!(windows, present_on(Os::MacOs, &state, 0, &Utc).machine.warm);
            assert!(windows.enabled, "{on}");
            assert_eq!(windows.standing.as_deref(), said, "{on}");
            assert!(
                windows
                    .note
                    .starts_with("Pitboard starts the five-hour period"),
                "{}",
                windows.note
            );
        }
    }
}
