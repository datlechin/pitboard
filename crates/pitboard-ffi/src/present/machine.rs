//! What the settings and the window's other panes show of this machine rather than its
//! accounts, as SettingsView.swift, MachinePane.swift and ActivityPane.swift said it from
//! MachineModel.swift: daily renewal and why it cannot be turned on, doctor's checks with the
//! line over them, every change Pitboard made, and the `pitboard` a terminal runs and how it
//! is kept up to date.
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
use pitboard_core::host::Os;

/// What the app shows of this machine rather than its accounts. Not called `Machine`, the
/// name the model's tests give the core they script.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MachineShown {
    /// Daily renewal, the settings' switch and what is said around it.
    pub schedule: ScheduleShown,
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
    pub title: String,
    // minutes of the day, as `Intent::SetWarmRange` takes them; a `from` past `to` runs past
    // midnight
    pub from: u16,
    pub to: u16,
    // "6:00 AM to 10:00 PM", "6:00 AM to midnight", "All day", on the person's clock
    pub range_label: String,
    // each Claude Code account enrolled under a name, in the order they were enrolled
    pub accounts: Vec<WarmAccountShown>,
    // in place of `accounts` while there are none
    pub none_enrolled: Option<String>,
    // the next warm-up, or why none is made, once anything is turned on
    pub standing: Option<String>,
    // what a warm-up does and what it never does
    pub note: String,
    // once the core has said what it is
    pub enabled: bool,
    pub terms: TermsShown,
}

/// One Claude Code account, as the settings' warm-up lists it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WarmAccountShown {
    /// Its label with its tool, which `Intent::SetWarm` takes back.
    pub qualified: String,
    /// Its name, with the plan its login says it is on: "work · Team 5x".
    pub title: String,
    /// Whether it is warmed up.
    pub on: bool,
    /// Where its warm-up stands, while it is turned on: "Warms up again at 22:50".
    pub standing: Option<String>,
}

// What the app does once a limit of the account in use reaches a share, as the settings show it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AtLimitShown {
    // over the choice, where a system heads it
    pub title: String,
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
    // "When a limit reaches", beside the field that shows the share
    pub heading: String,
    // "When a limit reaches 95%", the choice's name for VoiceOver and Narrator
    pub at_label: String,
    // the control that sets the share
    pub share_label: String,
    // beside that control, where a system says its range
    pub share_range: String,
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
    // what was asked for while a change is under way, so the switch does not snap back
    pub on: bool,
    // turning renewal off always can, which is how a schedule that cannot work goes
    pub enabled: bool,
    // while the scheduler does not have it: none here, or why this copy cannot schedule
    pub note: Option<String>,
    // why the last change failed, until the next, where `note` does not say it
    pub failed: Option<String>,
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
    /// Where none was found on Windows, whose installer puts the command line on the `PATH`:
    /// which terminals see it.
    pub terminal_note: Option<String>,
}

/// What the app shows of this machine.
pub(crate) fn machine(seen: &Seen) -> MachineShown {
    MachineShown {
        schedule: schedule(seen),
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
                    title: words::named_in_menu(
                        seen.os,
                        &choice.account,
                        seen.accounts()
                            .iter()
                            .find(|account| account.qualified.as_deref() == Some(&choice.qualified))
                            .and_then(|account| account.plan.as_deref()),
                        None,
                    ),
                    on: choice.on,
                    standing: choice.standing.as_ref().map(|standing| {
                        words::typeset(&pitboard_core::words::capitalised(
                            &pitboard_core::words::warm_standing(standing, &clock),
                        ))
                    }),
                })
                .collect()
        })
        .unwrap_or_default();
    let unreadable = state
        .warm
        .unreadable
        .as_deref()
        .map(pitboard_core::words::capitalised);
    let standing = unreadable.or_else(|| {
        overview
            .filter(|_| state.warm.any_on())
            .and_then(|overview| {
                if let Some(halt) = &overview.halted {
                    return Some(words::typeset(&pitboard_core::words::warming_halted(halt)));
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
        title: "Warm up limits".into(),
        none_enrolled: accounts
            .is_empty()
            .then(|| "No Claude Code account is enrolled yet.".into()),
        from: range.from(),
        to: range.to(),
        range_label: words::warm_hours(range.from(), range.to(), |minutes| {
            seen.time_of_day(minutes)
        })
        .unwrap_or_else(|| range.to_string()),
        accounts,
        standing,
        note: "Pitboard starts each account’s five-hour period as soon as the last one resets, \
               within these hours, while the app is open."
            .into(),
        enabled: overview.is_some(),
        terms: terms(),
    }
}

fn terms() -> TermsShown {
    TermsShown {
        sentence: words::sentence(pitboard_core::words::TERMS_MAY_COVER_THIS),
        link_title: "Is this allowed?".into(),
        link: pitboard_core::words::IS_THIS_ALLOWED_URL.into(),
    }
}

// How soon running sessions follow is promised only while the last read found no file behind
// the store.
const AT_LIMIT: &str = "When a limit reaches";

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
            words::typeset(pitboard_core::words::kept_until_renewed())
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
            "Pitboard notifies you once a limit of the account in use reaches {at}%, while the app \
             is open."
        )),
        AtLimit::Switch => Some(format!(
            "Pitboard switches Claude Code once a limit of the account in use reaches {at}%, never \
             back by itself, and notifies you of Codex. {follows}"
        )),
    };
    let switching = choice == AtLimit::Switch;
    AtLimitShown {
        title: "Before an account runs out".into(),
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
        heading: AT_LIMIT.into(),
        at_label: format!("{AT_LIMIT} {at}%"),
        share_label: "Share of a limit".into(),
        share_range: words::share_range(Threshold::LOWEST, Threshold::HIGHEST),
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
/// the core's sentences, as `pitboard watch` says them, with clock times as the person's own
/// clock says them.
fn auto_standing(seen: &Seen, standing: &AutoStanding) -> String {
    let came = match standing {
        AutoStanding::Came(AutoSwitched::Watching {
            account,
            used,
            as_of,
            held_until,
        }) => pitboard_core::words::watching(
            &words::quoted(account),
            used.as_deref(),
            as_of.map(|at| seen.clock(at)).as_deref(),
            held_until.map(|until| seen.clock(until)).as_deref(),
        ),
        AutoStanding::Came(AutoSwitched::Waiting { from, used, until }) => {
            pitboard_core::words::auto_waiting(&words::quoted(from), used, &seen.clock(*until))
        }
        AutoStanding::Came(AutoSwitched::Switched { from, to, used, .. }) => {
            pitboard_core::words::auto_switched(&words::quoted(from), &words::quoted(to), used)
        }
        AutoStanding::Came(AutoSwitched::Skipped {
            from,
            used,
            why,
            until,
            ..
        }) => pitboard_core::words::auto_skipped(
            &words::quoted(from),
            used,
            why,
            until.map(|until| seen.clock(until)).as_deref(),
        ),
        AutoStanding::Came(AutoSwitched::NotWatching { why, until, .. }) => {
            pitboard_core::words::auto_not_watching(
                why,
                until.map(|until| seen.clock(until)).as_deref(),
            )
        }
        AutoStanding::Stopped { message, until } => {
            return words::auto_stopped(until.map(|until| seen.clock(until)).as_deref(), message);
        }
    };
    words::typeset(&came)
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
    let note = match &machine.schedule {
        Some(Schedule::Installed { .. }) => None,
        Some(Schedule::Unsupported) => Some(words::no_scheduler(seen.os)),
        Some(Schedule::Absent) | None => cannot.clone(),
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
        on: machine.scheduling.unwrap_or(renews_daily),
        enabled: machine.scheduling.is_none() && (renews_daily || cannot.is_none()),
        note,
        failed,
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
            name: words::NOTIFICATIONS.into(),
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
                name: words::check_name(&check.code, &check.name),
                level: check.level,
                spoken_level: words::spoken_level(check.level).to_owned(),
                detail: words::check_text(&check.detail),
                advice: (check.level != Level::Ok && !check.advice.is_empty())
                    .then(|| words::check_text(&check.advice)),
            })
            .collect(),
        summary: any.then(|| words::typeset(&crate::doctor_summary(&checks))),
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
        terminal_note: (nowhere && seen.os == Os::Windows).then(|| words::NEW_TERMINALS.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{CheckLine, ChecksShown, words};
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
    fn warm_up_says_so_while_no_claude_code_account_is_enrolled() {
        let state = read(status(Vec::new(), Vec::new()), vec![claude_code()]);
        assert_eq!(
            present_on(Os::Windows, &state, 0, &Utc)
                .machine
                .warm
                .none_enrolled
                .as_deref(),
            Some("No Claude Code account is enrolled yet.")
        );
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

    // The line over the checks is as bad as the worst of them, in its colour as in doctor's
    // words.
    #[test]
    fn the_line_over_the_checks_takes_the_worst_level() {
        let level = |levels: &[Level]| {
            let shown: Vec<_> = levels
                .iter()
                .map(|&level| ("state", "accounts", level, "", ""))
                .collect();
            let shown = checks(&shown);
            let said = (!levels.is_empty()).then(|| {
                words::typeset(&pitboard_core::words::doctor_summary(levels.iter().map(
                    |level| match level {
                        Level::Ok => pitboard_core::doctor::Level::Ok,
                        Level::Warn => pitboard_core::doctor::Level::Warn,
                        Level::Fail => pitboard_core::doctor::Level::Fail,
                    },
                )))
            });
            assert_eq!(shown.summary, said);
            shown.level
        };
        assert_eq!(level(&[Level::Ok, Level::Ok]), Level::Ok);
        assert_eq!(level(&[Level::Warn, Level::Ok]), Level::Warn);
        assert_eq!(level(&[Level::Ok, Level::Fail, Level::Warn]), Level::Fail);
        assert_eq!(level(&[]), Level::Ok);
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
                ("schedule_proxy", "renewal schedule's proxy"),
            ]),
            [
                "Config file",
                "Account personal",
                "Interrupted switch",
                "Claude Code build",
                "PITBOARD_NO_ARGV",
                "Machine ID",
                "Renewal schedule’s proxy",
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
            present_on(Os::MacOs, &state, 0, &Utc)
                .machine
                .schedule
                .about
        };
        let claude_code_alone = about(vec![account(Some("work")).signed_in().build()]);
        assert_eq!(
            claude_code_alone,
            "Your Mac’s own scheduler renews your parked logins once a day, so one you leave \
             alone doesn’t expire."
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
        let mut work = account(Some("work")).signed_in().build();
        work.plan = Some("Team 5x".into());
        let mut state = read(status(vec![work], Vec::new()), vec![claude_code()]);
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
                Some(words::typeset(&pitboard_core::words::warming_halted(
                    &Halt::ProgramMissing,
                ))),
            ),
        ] {
            state.warm.overview = Some(overview(on));
            let windows = present_on(Os::Windows, &state, 0, &Utc).machine.warm;
            assert_eq!(windows, present_on(Os::MacOs, &state, 0, &Utc).machine.warm);
            assert!(windows.enabled, "{on}");
            assert_eq!(windows.none_enrolled, None);
            assert_eq!(
                windows.accounts[0].title,
                words::named_in_menu(Os::Windows, "work", Some("Team 5x"), None),
                "with its plan, as the menu names it"
            );
            assert_eq!(windows.standing, said, "{on}");
            assert_eq!(
                windows.note,
                "Pitboard starts each account’s five-hour period as soon as the last one resets, \
                 within these hours, while the app is open."
            );
        }
    }
}
