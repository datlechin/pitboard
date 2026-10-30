//! What Settings, Diagnostics and Activity show of this machine rather than its accounts.

use super::checks::{ChecksShown, checks};
use super::{Seen, words};
use crate::Front;
use crate::model::AtLimit;
use crate::model::machine::ScheduleFailure;
use crate::model::state::AutoStanding;
use crate::{AccountKey, AutoSwitched, FoundCommandLine, Schedule, WatchService};
use pitboard_core::duty::autoswitch::Threshold;
use pitboard_core::state::Key;

/// What the app shows of this machine rather than its accounts. Not called `Machine`, the
/// name the model's tests give the core they script.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MachineShown {
    /// Daily renewal, the settings' switch and what is said around it.
    pub schedule: ScheduleShown,
    pub checks: ChecksShown,
    /// Every change Pitboard made, as the window's activity pane lists them.
    pub activity: ActivityShown,
    /// The `pitboard` a terminal runs, as the settings show it.
    pub command_line: CommandLineShown,
    pub at_limit: AtLimitShown,
    /// Warming up accounts, as the settings show it.
    pub warm: WarmShown,
    /// The watch service, while no status item keeps Pitboard running once its window closes.
    #[uniffi(default)]
    pub watch_service: Option<WatchServiceShown>,
}

/// The watch service, as the settings show it, beside their own title for it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WatchServiceShown {
    // what was asked for while a change is under way, so the switch does not snap back
    pub on: bool,
    // turning it off always can
    pub enabled: bool,
    // while it is not installed: why this machine or this copy cannot run it
    pub note: Option<String>,
    // why the last change failed, until the next, where `note` does not say it
    pub failed: Option<String>,
    // what it runs once the window is closed, under its switch
    pub about: String,
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
}

/// One Claude Code account, as the settings' warm-up lists it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WarmAccountShown {
    /// What `Intent::SetWarm` takes back.
    pub account: AccountKey,
    /// Its label with its tool, `claude/work`, which its toggle is known by.
    pub qualified: String,
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
    // whatever is chosen, as the terms may cover warm-ups too
    pub terms: TermsShown,
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
    /// What was done, in a word: "Switch", "Add".
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
        watch_service: watch_service(seen),
    }
}

/// Whether the watch service runs once the window is closed, where nothing else then would:
/// `None` where a status item keeps Pitboard running, so what runs goes on.
fn once_closed(seen: &Seen) -> Option<bool> {
    let machine = &seen.state.machine;
    let installed = matches!(machine.watch_service, Some(WatchService::Installed { .. }));
    seen.state
        .offers_watch_service()
        .then(|| machine.watching.unwrap_or(installed))
}

/// The watch service, as daily renewal is shown beside it, where the front offers it.
fn watch_service(seen: &Seen) -> Option<WatchServiceShown> {
    let on = once_closed(seen)?;
    let machine = &seen.state.machine;
    let installed = match machine.watch_service {
        Some(WatchService::Installed { switching }) => Some(switching),
        _ => None,
    };
    let cannot = machine
        .own
        .filter(|own| !own.lasting())
        .map(|own| words::cannot_watch(seen.front, own.temporary));
    let note = match &machine.watch_service {
        Some(WatchService::Installed { .. }) => None,
        Some(WatchService::Unsupported) => Some(words::no_scheduler(seen.front)),
        Some(WatchService::Absent) | None => cannot.clone(),
    };
    let failed = machine
        .watch_failed
        .as_ref()
        .map(|failed| match failed {
            ScheduleFailure::CannotSchedule { temporary } => {
                words::cannot_watch(seen.front, *temporary)
            }
            ScheduleFailure::Refused { message } => message.clone(),
        })
        .filter(|failed| note.as_ref() != Some(failed));
    let switching = installed
        .flatten()
        .unwrap_or_else(|| seen.state.preferences.watch_switching());
    Some(WatchServiceShown {
        on,
        enabled: machine.watching.is_none() && (installed.is_some() || cannot.is_none()),
        note,
        failed,
        about: words::watch_service_about(seen.front, switching),
    })
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
                    account: Key::parse(&choice.qualified).into(),
                    qualified: choice.qualified.clone(),
                    title: choice.account.clone(),
                    on: choice.on,
                    standing: choice.standing.as_ref().map(|standing| {
                        words::typeset(&pitboard_core::words::capitalised(&words::warm_standing(
                            standing, &clock,
                        )))
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
                    return Some(words::warming_halted(halt));
                }
                if let Some(failed) = &state.warm.failed {
                    return Some(words::warm_up_failed(failed));
                }
                let (account, at) = overview.next(seen.now)?;
                let at = (at > seen.now).then(|| seen.clock(at));
                Some(words::next_warm_up(account, at.as_deref()))
            })
    });
    WarmShown {
        title: words::warm_up(seen.front),
        none_enrolled: accounts.is_empty().then(|| words::NONE_TO_WARM.into()),
        from: range.from(),
        to: range.to(),
        range_label: words::warm_hours(range.from(), range.to(), |minutes| {
            seen.time_of_day(minutes)
        })
        .unwrap_or_else(|| range.to_string()),
        accounts,
        standing,
        note: words::warm_up_note(once_closed(seen)),
        enabled: overview.is_some(),
    }
}

fn terms() -> TermsShown {
    TermsShown {
        sentence: words::sentence(pitboard_core::words::TERMS_MAY_COVER_THIS),
        link_title: words::IS_THIS_ALLOWED.into(),
        link: pitboard_core::words::IS_THIS_ALLOWED_URL.into(),
    }
}

fn at_limit(seen: &Seen) -> AtLimitShown {
    let state = seen.state;
    let at = state.preferences.threshold().percent();
    let choice = state.preferences.at_limit();
    let switching = choice == AtLimit::Switch;
    let note = words::at_limit_note(
        seen.front,
        choice,
        at,
        state
            .warnings
            .iter()
            .any(|warning| warning.code == "fallback_login"),
        pitboard_core::switch::ADOPTION_CEILING_SECONDS,
    );
    let closed = once_closed(seen).and_then(|watched| match (choice, watched) {
        (AtLimit::Switch, watched) => Some(words::switching_once_closed(watched)),
        (AtLimit::Notify, true) => Some(words::NOTIFYING_ONCE_CLOSED.into()),
        (AtLimit::Nothing, true) => Some(words::ONLY_WARMING_UP.into()),
        (AtLimit::Notify | AtLimit::Nothing, false) => None,
    });
    let said: Vec<String> = note.into_iter().chain(closed).collect();
    let note = (!said.is_empty()).then(|| said.join(" "));
    AtLimitShown {
        title: words::BEFORE_RUNNING_OUT.into(),
        choice,
        choices: [AtLimit::Nothing, AtLimit::Notify, AtLimit::Switch]
            .into_iter()
            .map(|choice| AtLimitChoice {
                choice,
                title: words::at_limit_choice(seen.front, choice),
            })
            .collect(),
        at,
        lowest: Threshold::LOWEST,
        highest: Threshold::HIGHEST,
        enabled: state.preferences_have_been_read(),
        share_enabled: state.preferences_have_been_read() && choice != AtLimit::Nothing,
        heading: words::AT_LIMIT.into(),
        at_label: words::at_limit_label(at),
        share_label: words::AT.into(),
        share_range: words::share_range(Threshold::LOWEST, Threshold::HIGHEST),
        note,
        standing: state
            .auto_standing
            .as_ref()
            .filter(|_| switching)
            .map(|standing| auto_standing(seen, standing)),
        terms: terms(),
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
        .map(|own| words::cannot_schedule(seen.front, own.temporary));
    let note = match &machine.schedule {
        Some(Schedule::Installed { .. }) => None,
        Some(Schedule::Unsupported) => Some(words::no_scheduler(seen.front)),
        Some(Schedule::Absent) | None => cannot.clone(),
    };
    // Said once: a refusal the note under the switch says already is not said again.
    let failed = machine
        .schedule_failed
        .as_ref()
        .map(|failed| match failed {
            ScheduleFailure::CannotSchedule { temporary } => {
                words::cannot_schedule(seen.front, *temporary)
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
            seen.front,
            seen.accounts()
                .iter()
                .any(|account| crate::model::state::in_its_app(account.provider)),
        ),
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
            let (title, detail) = words::no_activity(seen.front);
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
            FoundCommandLine::Nowhere => words::NOT_INSTALLED.into(),
        }),
        update_note: match machine.command_line {
            Some(FoundCommandLine::Bundled { .. }) => {
                Some(words::update_note(seen.front, true).to_owned())
            }
            Some(FoundCommandLine::Another { .. }) => {
                Some(words::update_note(seen.front, false).to_owned())
            }
            Some(FoundCommandLine::Nowhere) | None => None,
        },
        offers_link: nowhere && own.lasting(),
        cannot_link: (nowhere && own.temporary).then(|| words::cannot_link(seen.front)),
        terminal_note: match seen.front {
            Front::WindowsNotificationArea => nowhere.then(|| words::NEW_TERMINALS.into()),
            Front::MacMenuBar | Front::Gnome => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::words;
    use crate::Front;
    use crate::present::present_on;
    use crate::present::testing::{
        Unreadable, Utc, account, claude_code, claude_desktop, desktop_account, read, status,
    };
    use crate::{LocalTime, PlatformError};
    use pitboard_core::duty::warm::{Overview, Range};

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

        fn date(&self, epoch: i64) -> Result<String, PlatformError> {
            Utc.date(epoch)
        }
    }

    fn hours(from: u16, to: u16, clock: &dyn LocalTime) -> String {
        let mut state = read(status(Vec::new(), Vec::new()), vec![claude_code()]);
        state.warm.overview = Some(Overview {
            range: Range::new(from, to).expect("a span"),
            accounts: Vec::new(),
            halted: None,
        });
        present_on(Front::WindowsNotificationArea, &state, 0, clock)
            .machine
            .warm
            .range_label
    }

    // The summary said 06:00 to 22:00 beside time pickers in AM and PM.
    #[test]
    fn the_range_is_said_on_the_persons_clock() {
        let on = |from: u16, to: u16, clock: &dyn LocalTime| {
            words::warm_hours(from, to, |minutes| clock.time_of_day(minutes).ok())
        };
        for (from, to) in [(6 * 60, 22 * 60), (22 * 60 + 30, 6 * 60), (6 * 60, 24 * 60)] {
            assert_eq!(
                hours(from, to, &Twelve),
                on(from, to, &Twelve).expect("a clock")
            );
            assert_eq!(hours(from, to, &Utc), on(from, to, &Utc).expect("a clock"));
        }
        assert_eq!(
            hours(6 * 60, 0, &Utc),
            on(6 * 60, 24 * 60, &Utc).expect("a clock"),
            "a range to 00:00 ends at the end of the day"
        );
        assert_eq!(
            hours(6 * 60, 22 * 60, &Unreadable),
            Range::new(6 * 60, 22 * 60).expect("a span").to_string(),
            "as the core writes it where the clock cannot say"
        );
        assert_eq!(
            hours(0, 24 * 60, &Unreadable),
            on(0, 24 * 60, &Unreadable).expect("the whole day needs no clock")
        );
    }

    // A sentence of the core's as the apps say one: its apostrophe curled and a full stop.
    #[test]
    fn the_terms_are_said_in_one_sentence_beside_their_link() {
        let terms = super::terms();
        assert_eq!(
            terms.sentence,
            "Anthropic’s terms may cover automatic switching and warm-ups."
        );
        assert_eq!(
            (terms.link_title.as_str(), terms.link.as_str()),
            (
                words::IS_THIS_ALLOWED,
                pitboard_core::words::IS_THIS_ALLOWED_URL
            )
        );
    }

    #[test]
    fn warm_up_says_so_while_no_claude_code_account_is_enrolled() {
        let state = read(status(Vec::new(), Vec::new()), vec![claude_code()]);
        assert_eq!(
            present_on(Front::WindowsNotificationArea, &state, 0, &Utc)
                .machine
                .warm
                .none_enrolled
                .as_deref(),
            Some(words::NONE_TO_WARM)
        );
    }

    #[test]
    fn the_renewal_note_says_a_claude_desktop_login_is_never_renewed() {
        let about = |accounts| {
            let state = read(
                status(accounts, Vec::new()),
                vec![claude_code(), claude_desktop()],
            );
            present_on(Front::MacMenuBar, &state, 0, &Utc)
                .machine
                .schedule
                .about
        };
        assert_eq!(
            about(vec![account(Some("work")).signed_in().build()]),
            words::renewal_about(Front::MacMenuBar, false)
        );
        assert_eq!(
            about(vec![
                account(Some("work")).signed_in().build(),
                desktop_account("work", true)
            ]),
            words::renewal_about(Front::MacMenuBar, true)
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
            (true, Some(words::warming_halted(&Halt::ProgramMissing))),
        ] {
            state.warm.overview = Some(overview(on));
            let windows = present_on(Front::WindowsNotificationArea, &state, 0, &Utc)
                .machine
                .warm;
            let mac = present_on(Front::MacMenuBar, &state, 0, &Utc).machine.warm;
            assert_eq!(
                windows,
                super::WarmShown {
                    title: words::warm_up(Front::WindowsNotificationArea),
                    ..mac
                },
                "but in its own case"
            );
            assert!(windows.enabled, "{on}");
            assert_eq!(windows.none_enrolled, None);
            assert_eq!(windows.accounts[0].title, "work", "its name alone");
            assert_eq!(windows.standing, said, "{on}");
            assert_eq!(windows.note, words::warm_up_note(None));
        }
    }
}
