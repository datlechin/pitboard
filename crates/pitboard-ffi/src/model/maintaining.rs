//! What the model keeps of this machine rather than its accounts, as MachineModelTests.swift
//! drove MachineModel.swift: the daily renewal schedule and the one change to it at a time,
//! the repair once a launch, renewing now, doctor's checks, the activity log and the
//! `pitboard` a terminal runs, each under its own name in snake case, with what the settings,
//! MachinePane.swift and ActivityPane.swift said of them. The state is driven through
//! `State::apply` by hand over a core the test scripts, and read as `present` makes it on a
//! Mac, its clock times in UTC. Opening at login and linking the command line with an
//! administrator's password are the app's own, and so are their tests.

use super::changing::a_look_landing_after;
use super::machine::ScheduleFailure;
use super::state::Job;
use super::testing::{
    Hand, Machine, PLIST, a_look_or_a_read, any_read, change, check, claude, codex_account,
    fresh_read, installed_ask, refusal, renewed, status, warned, warning,
};
use super::{Intent, Pane};
use crate::present::testing::Utc;
use crate::present::{CheckLine, MachineShown, present_on};
use crate::{FoundCommandLine, Level, OwnCommandLine, Schedule, Warning};
use pitboard_core::host::Os;
use std::time::Duration;

/// What a Mac's app shows of this machine now.
fn shown(model: &Hand) -> MachineShown {
    present_on(Os::MacOs, &model.state, model.now.epoch(), &Utc).machine
}

/// Whether the window says something went wrong: a read that did not answer, or something
/// asked for that did not happen. What is about the schedule is said beside its switch.
fn window_failure(model: &Hand) -> bool {
    let snapshot = model.shown();
    snapshot.read_failure.is_some() || snapshot.failure.is_some()
}

/// A machine with one account and nothing on its scheduler, as the core answers.
fn machine() -> Machine {
    Machine::reading(Ok(status(vec![claude("work", true, 10.0)])))
}

fn installed() -> Schedule {
    Schedule::Installed {
        path: PLIST.into(),
        every_seconds: 86_400,
    }
}

/// The command line inside a copy macOS runs from a temporary place.
const TEMPORARY: OwnCommandLine = OwnCommandLine {
    inside: true,
    temporary: true,
    runs: true,
};

/// The command line inside a copy, there and lasting, that nobody may run.
const UNRUNNABLE: OwnCommandLine = OwnCommandLine {
    inside: true,
    temporary: false,
    runs: false,
};

const NO_COMMAND_LINE: &str =
    "This copy of Pitboard has no command line inside it to run on a schedule.";

const MOVE_IT_FIRST: &str = "Move Pitboard to your Applications folder first. Until then macOS \
                             runs it from a temporary copy, which is gone once Pitboard quits.";

/// Daily renewal reads as on only while the scheduler has the job. A scheduler this Mac does
/// not have is not a schedule that is on. Nothing is read until the settings ask.
///
/// MachineModelTests.swift's dailyRenewalIsOnOnlyWhileTheSchedulerHasIt.
#[test]
fn daily_renewal_is_on_only_while_the_scheduler_has_it() {
    let mut model = Hand::new();
    let mut machine = machine().with_a_command_line_inside();
    let first = shown(&model).schedule;
    assert_eq!((first.schedule, first.on), (None, false));
    assert_eq!(machine.schedule_reads, 0);

    for (scheduled, on) in [
        (installed(), true),
        (Schedule::Absent, false),
        (Schedule::Unsupported, false),
    ] {
        machine.scheduled = scheduled.clone();
        assert_eq!(
            model.send(Intent::ReadSchedule),
            [Job::ReadSchedule {
                after_change: false
            }]
        );
        model.run(&mut machine);
        let schedule = shown(&model).schedule;
        assert_eq!(schedule.schedule, Some(scheduled), "{schedule:?}");
        assert_eq!(schedule.on, on);
    }
    assert_eq!(machine.schedule_reads, 3);
}

/// While the scheduler has it, the settings say how often it runs and where the scheduler
/// keeps it; on a machine with no scheduler Pitboard writes to, they say so, and nothing else.
#[test]
fn the_settings_say_how_often_it_runs_and_where_it_is() {
    let mut model = Hand::new();
    let mut machine = machine().with_a_command_line_inside();
    machine.scheduled = installed();
    model.send(Intent::ReadSchedule);
    model.run(&mut machine);
    let schedule = shown(&model).schedule;
    assert_eq!(schedule.runs.as_deref(), Some("Every day"));
    assert_eq!(schedule.scheduled_in.as_deref(), Some(PLIST));
    assert_eq!(schedule.note, None);

    machine.scheduled = Schedule::Unsupported;
    model.send(Intent::ReadSchedule);
    model.run(&mut machine);
    let schedule = shown(&model).schedule;
    assert_eq!((schedule.runs, schedule.scheduled_in), (None, None));
    assert_eq!(
        schedule.note.as_deref(),
        Some("This Mac has no scheduler Pitboard knows how to write to.")
    );
}

/// Turning daily renewal on or off changes the scheduler, and the switch then shows what the
/// scheduler has, read again rather than assumed.
///
/// MachineModelTests.swift's turningDailyRenewalOnAndOffChangesTheScheduler.
#[test]
fn turning_daily_renewal_on_and_off_changes_the_scheduler() {
    let mut model = Hand::new();
    let mut machine = machine().with_a_command_line_inside();

    assert_eq!(
        model.send(Intent::SetSchedule { on: true }),
        [Job::SetSchedule { on: true }]
    );
    model.run(&mut machine);
    assert_eq!((machine.installs, machine.schedule_reads), (1, 1));
    let schedule = shown(&model).schedule;
    assert_eq!(schedule.schedule, Some(installed()));
    assert!(schedule.on && schedule.enabled && !schedule.changing);
    assert_eq!(schedule.failed, None);

    model.send(Intent::SetSchedule { on: false });
    model.run(&mut machine);
    assert_eq!((machine.uninstalls, machine.schedule_reads), (1, 2));
    let schedule = shown(&model).schedule;
    assert_eq!(schedule.schedule, Some(Schedule::Absent));
    assert!(!schedule.on);
}

/// The schedule runs the command line inside the app long after the app has quit. A copy
/// macOS runs from a temporary place is gone by then, and one with no command line inside it
/// has nothing to schedule, so turning renewal on there is refused with the reason and
/// nothing is asked of the scheduler, nor read. Turning it off never is: that is how a
/// schedule that cannot work is taken away.
///
/// MachineModelTests.swift's dailyRenewalIsRefusedWhereNothingWouldBeThereToRun, and
/// AppModelTests.swift's dailyRenewalIsTurnedOnOnlyFromAnAppThatStaysWhereItIs, whose build
/// directory with no command line inside it is `lanes.rs`'s, over a core made as the app
/// makes it.
#[test]
fn daily_renewal_is_refused_where_nothing_would_be_there_to_run() {
    let mut model = Hand::new();
    let mut able = machine().with_a_command_line_inside();
    model.send(Intent::ReadSchedule);
    model.run(&mut able);
    let schedule = shown(&model).schedule;
    assert_eq!(schedule.note, None, "a copy that can schedule it");
    assert!(schedule.enabled);

    for (own, reason, temporary) in [
        (TEMPORARY, MOVE_IT_FIRST, true),
        (OwnCommandLine::default(), NO_COMMAND_LINE, false),
    ] {
        let mut model = Hand::new();
        let mut machine = machine();
        machine.own = own;
        machine.scheduled = installed();

        model.send(Intent::SetSchedule { on: true });
        model.run(&mut machine);
        assert_eq!(
            model.state.machine.schedule_failed,
            Some(ScheduleFailure::CannotSchedule { temporary })
        );
        assert_eq!((machine.installs, machine.schedule_reads), (0, 0));
        let schedule = shown(&model).schedule;
        assert_eq!(schedule.note.as_deref(), Some(reason));
        assert_eq!(schedule.failed, None, "said once, under the switch");
        assert!(!schedule.changing && !schedule.enabled);
        assert!(
            !window_failure(&model),
            "said beside the switch, not in the window"
        );

        model.send(Intent::SetSchedule { on: false });
        model.run(&mut machine);
        assert_eq!(model.state.machine.schedule_failed, None);
        assert_eq!(machine.uninstalls, 1);
        assert_eq!(shown(&model).schedule.schedule, Some(Schedule::Absent));
    }
}

/// While the scheduler has a schedule this copy could not have made, the switch can still
/// turn it off, and says nothing of why it cannot be turned on: it is on.
#[test]
fn a_schedule_this_copy_cannot_make_can_still_be_taken_away() {
    let mut model = Hand::new();
    let mut machine = machine();
    machine.own = TEMPORARY;
    machine.scheduled = installed();
    model.send(Intent::ReadSchedule);
    model.run(&mut machine);
    let schedule = shown(&model).schedule;
    assert!(schedule.on && schedule.enabled);
    assert_eq!(schedule.note, None);

    model.send(Intent::SetSchedule { on: false });
    model.run(&mut machine);
    let schedule = shown(&model).schedule;
    assert!(!schedule.on && !schedule.enabled);
    assert_eq!(schedule.note.as_deref(), Some(MOVE_IT_FIRST));
}

/// A build run from Xcode is an app with no command line inside it, and a schedule of one
/// renews nothing, so none is offered. A command line inside the app that nobody can run is
/// the same as none, until it can be run, which is read again with the schedule. Nor is a
/// link to it offered, where none is found: it would run nothing.
///
/// MachineModelTests.swift's anAppWithoutACommandLineItCanRunCannotLinkOrSchedule, but for
/// asking for a link, which the app's own code refuses: `lanes.rs` has it over a real file.
#[test]
fn an_app_without_a_command_line_it_can_run_cannot_link_or_schedule() {
    let mut model = Hand::new();
    let mut machine = machine();
    machine.own = UNRUNNABLE;
    model.send(Intent::ReadSchedule);
    model.send(Intent::LookForCommandLine);
    model.run(&mut machine);
    let shown_now = shown(&model);
    assert_eq!(shown_now.schedule.note.as_deref(), Some(NO_COMMAND_LINE));
    assert!(!shown_now.schedule.enabled);
    assert_eq!(
        shown_now.command_line.found,
        Some(FoundCommandLine::Nowhere)
    );
    assert!(!shown_now.command_line.offers_link);
    assert_eq!(shown_now.command_line.cannot_link, None);

    model.send(Intent::SetSchedule { on: true });
    model.run(&mut machine);
    assert_eq!(machine.installs, 0);

    machine.own.runs = true;
    model.send(Intent::ReadSchedule);
    model.run(&mut machine);
    let shown_now = shown(&model);
    assert_eq!(shown_now.schedule.note, None);
    assert!(shown_now.schedule.enabled);
    assert!(shown_now.command_line.offers_link);
}

/// Whether this copy can schedule renewal is read with the schedule, the repair at launch or
/// the command line, not as the settings draw: a command line made runnable since is taken
/// at the next read. The Swift settings asked the file system each time they drew. The
/// repair reads it even where it repairs nothing, so the switch says it from the start.
#[test]
fn whether_this_copy_can_schedule_is_read_with_the_schedule() {
    let mut model = Hand::new();
    let mut machine = machine();
    machine.own = UNRUNNABLE;
    model.send(Intent::Start);
    model.run(&mut machine);
    assert_eq!(
        machine.schedule_reads, 0,
        "nothing was repaired, so nothing was read"
    );
    let schedule = shown(&model).schedule;
    assert_eq!(
        schedule.note.as_deref(),
        Some(NO_COMMAND_LINE),
        "read with the repair at launch"
    );
    assert!(!schedule.enabled);

    machine.own.runs = true;
    assert_eq!(
        shown(&model).schedule.note.as_deref(),
        Some(NO_COMMAND_LINE),
        "not read again yet"
    );
    model.send(Intent::ReadSchedule);
    model.run(&mut machine);
    assert_eq!(shown(&model).schedule.note, None);

    machine.own = TEMPORARY;
    model.send(Intent::LookForCommandLine);
    model.run(&mut machine);
    assert_eq!(
        shown(&model).schedule.note.as_deref(),
        Some(MOVE_IT_FIRST),
        "read with the command line found"
    );
}

/// The switch shows the state asked for while the scheduler is being changed, rather than
/// snapping back to the old one until it answers and the schedule has been read back, which
/// can be a while behind a renewal or a switch. Pressed again meanwhile it does nothing: it
/// was pressed on a switch that did not yet show the first press.
///
/// MachineModelTests.swift's dailyRenewalShowsWhatWasAskedForWhileItChanges.
#[test]
fn daily_renewal_shows_what_was_asked_for_while_it_changes() {
    let mut model = Hand::new();
    let mut machine = machine().with_a_command_line_inside();
    assert!(!shown(&model).schedule.changing);

    model.send(Intent::SetSchedule { on: true });
    let schedule = shown(&model).schedule;
    assert!(schedule.on && schedule.changing && !schedule.enabled);
    assert_eq!(
        model.send(Intent::SetSchedule { on: false }),
        [],
        "the press made meanwhile"
    );
    let set = model.next();
    let read_back = model.give(machine.answer(set));
    assert_eq!(read_back, [Job::ReadSchedule { after_change: true }]);
    assert!(
        shown(&model).schedule.changing,
        "until the schedule is read back"
    );
    model.run(&mut machine);
    assert_eq!((machine.installs, machine.uninstalls), (1, 0));
    let schedule = shown(&model).schedule;
    assert!(schedule.on && !schedule.changing);

    model.send(Intent::SetSchedule { on: false });
    assert!(!shown(&model).schedule.on, "what was asked for");
    assert_eq!(model.send(Intent::SetSchedule { on: true }), []);
    model.run(&mut machine);
    assert_eq!((machine.installs, machine.uninstalls), (1, 1));
    let schedule = shown(&model).schedule;
    assert!(!schedule.on && !schedule.changing);
}

/// A read of the schedule that is not the one after a change leaves the change under way:
/// only the read back after it ends it.
#[test]
fn only_the_read_after_a_change_ends_it() {
    let mut model = Hand::new();
    let mut machine = machine().with_a_command_line_inside();
    model.send(Intent::SetSchedule { on: true });
    model.send(Intent::ReadSchedule);
    let set = model.next();
    let read = model.next();
    model.give(machine.answer(read));
    assert!(shown(&model).schedule.changing);
    model.give(machine.answer(set));
    model.run(&mut machine);
    assert!(!shown(&model).schedule.changing);
}

/// When the scheduler refuses, the core's own words are said beside the switch, and the
/// switch shows the schedule as it is. The next try starts from nothing said.
///
/// MachineModelTests.swift's aScheduleTheCoreCouldNotChangeSaysWhyUntilTheNextTry.
#[test]
fn a_schedule_the_core_could_not_change_says_why_until_the_next_try() {
    let mut model = Hand::new();
    let mut machine = machine().with_a_command_line_inside();
    let message = "launchctl refused the renewal job: Input/output error";
    machine.refusing = Some(refusal("schedule_refused", message, Vec::new()));

    model.send(Intent::SetSchedule { on: true });
    model.run(&mut machine);
    let schedule = shown(&model).schedule;
    assert_eq!(schedule.failed.as_deref(), Some(message));
    assert_eq!(machine.schedule_reads, 1);
    assert!(!schedule.on);
    assert!(
        !window_failure(&model),
        "said beside the switch, not in the window"
    );

    machine.refusing = None;
    model.send(Intent::SetSchedule { on: true });
    assert_eq!(shown(&model).schedule.failed, None, "the next try");
    model.run(&mut machine);
    let schedule = shown(&model).schedule;
    assert_eq!(schedule.failed, None);
    assert!(schedule.on);
}

/// An app up to 0.3.0 scheduled itself, which renews nothing. The repair points that job at
/// the command line inside this app, once a launch, and the schedule is read again only when
/// it did. Nothing repaired, or a repair that failed, says nothing: the schedule is as it was,
/// and doctor still reports it.
///
/// MachineModelTests.swift's anOlderAppsScheduleIsReadAgainOnlyOnceItWasRepaired, and
/// AppModelTests.swift's anOldScheduleIsRepairedOnceTheAppStarts.
#[test]
fn an_older_apps_schedule_is_read_again_only_once_it_was_repaired() {
    let failed = refusal("schedule_refused", "the scheduler refused", Vec::new());
    for (answer, reads) in [(Ok(true), 1), (Ok(false), 0), (Err(failed), 0)] {
        let mut model = Hand::new();
        let mut machine = machine().with_a_command_line_inside();
        machine.scheduled = installed();
        machine.repairs = answer;

        model.send(Intent::Start);
        let repair = model.take(|job| matches!(job, Job::RepairSchedule));
        model.give(machine.answer(repair));
        assert!(!window_failure(&model), "nothing said in the window");
        model.run(&mut machine);
        assert_eq!(machine.repair_asks, 1);
        assert_eq!(machine.schedule_reads, reads);
        let schedule = shown(&model).schedule;
        assert_eq!(schedule.schedule.is_some(), reads == 1, "{schedule:?}");
        assert_eq!(schedule.on, reads == 1);
        assert_eq!(schedule.failed, None);

        model.send(Intent::Start);
        model.run(&mut machine);
        assert_eq!(machine.repair_asks, 1, "once a launch");
    }
}

/// Renewing now says what each parked login came to, shows it is running while it runs and
/// until the accounts have been read again, once, asking every service, with what it renewed.
///
/// MachineModelTests.swift's renewingNowSaysWhatItRenewedAndReadsTheAccountsOnce.
#[test]
fn renewing_now_says_what_it_renewed_and_reads_the_accounts_once() {
    let mut model = Hand::new();
    let mut machine = machine();
    machine.renewals = Ok(vec![
        renewed("work", "claude", "renewed"),
        renewed("codex/spare", "codex", "renewal_deferred"),
    ]);
    model.refresh(&mut machine);
    let reads = model.count(any_read);
    let renewal = shown(&model).renewal;
    assert!(!renewal.renewing);
    assert_eq!(renewal.note, "Renew every parked login that is due.");

    assert_eq!(model.send(Intent::RenewNow), [Job::Renew]);
    assert!(shown(&model).renewal.renewing);
    let renew = model.next();
    let read = model.give(machine.answer(renew));
    assert!(matches!(read[..], [Job::Read { fresh: true, .. }]));
    assert!(
        shown(&model).renewal.renewing,
        "until the accounts are read again"
    );
    model.run(&mut machine);
    assert_eq!(machine.renew_asks, 1);
    assert_eq!(model.count(any_read), reads + 1);
    assert_eq!(model.count(fresh_read), 1);
    let renewal = shown(&model).renewal;
    assert!(!renewal.renewing);
    assert_eq!(
        renewal.note,
        "Renewed 1 of 2; the rest are tried again next time."
    );
}

/// A renewal the core refused, as it refuses every change where Pitboard runs as root or
/// under sudo, is said in the core's words. The core answered an empty list, which read as
/// "No parked login was due." when nothing had been looked at. The accounts are read again
/// after it, as after any renewal.
#[test]
fn a_refused_renewal_says_why_rather_than_that_nothing_was_due() {
    let mut model = Hand::new();
    let mut machine = machine();
    let refused = "Pitboard changes nothing when it runs as root or with sudo. Run it as yourself.";
    machine.renewals = Err(refusal("elevated", refused, Vec::new()));
    model.refresh(&mut machine);
    let reads = model.count(any_read);

    model.send(Intent::RenewNow);
    model.run(&mut machine);

    assert_eq!(machine.renew_asks, 1);
    assert_eq!(model.count(any_read), reads + 1);
    let renewal = shown(&model).renewal;
    assert!(!renewal.renewing);
    assert_eq!(renewal.note, refused);
}

/// `changing.rs`'s look landing after a change, for renewing now: a renewal writes the account
/// index with each parked login it renews, and the read after it, asking every service, was
/// dropped as one that started before a change made elsewhere.
#[test]
fn a_look_landing_after_renewing_leaves_the_read_after_it() {
    let after = vec![
        codex_account("personal", true),
        codex_account("spare", false),
    ];
    a_look_landing_after(after, |model, machine| {
        model.send(Intent::RenewNow);
        model.run_but(machine, a_look_or_a_read);
        assert!(
            shown(model).renewal.renewing,
            "until the read after it is over"
        );
    });
}

/// A second Renew Now while one runs does nothing, as the Swift settings held its button
/// back meanwhile: two renewals would ask each service twice for nothing.
#[test]
fn renew_now_while_a_renewal_runs_does_nothing() {
    let mut model = Hand::new();
    let mut machine = machine();
    model.refresh(&mut machine);
    model.send(Intent::RenewNow);
    assert_eq!(model.send(Intent::RenewNow), []);
    model.run(&mut machine);
    assert_eq!(machine.renew_asks, 1);
    assert_eq!(
        model.send(Intent::RenewNow),
        [Job::Renew],
        "once it is over"
    );
}

/// The read after a renewal ends it however that read goes: one that started before a change
/// and is dropped, and one that has to wait for what is installed first.
#[test]
fn a_renewal_is_over_once_the_read_after_it_is() {
    let mut model = Hand::new();
    let mut machine = machine();
    model.send(Intent::RenewNow);
    let renew = model.next();
    let asked = model.give(machine.answer(renew));
    assert!(
        asked.iter().any(installed_ask),
        "waits for what is installed"
    );
    assert!(shown(&model).renewal.renewing);
    model.run(&mut machine);
    assert!(!shown(&model).renewal.renewing);

    model.send(Intent::RenewNow);
    let renew = model.next();
    model.give(machine.answer(renew));
    let read = model.next();
    machine.changed += 1;
    model.notice(&mut machine);
    model.give(machine.answer(read));
    assert!(
        !shown(&model).renewal.renewing,
        "dropped, and over all the same"
    );
}

/// Doctor's checks are shown with when they were made, in place of the last ones, made again
/// every time the pane is shown, and the pane says it is checking while doctor runs. What
/// stands in for them before there are any says the machine is being checked.
///
/// MachineModelTests.swift's doctorsChecksAreShownWithWhenTheyWereMade.
#[test]
fn doctors_checks_are_shown_with_when_they_were_made() {
    let mut model = Hand::new();
    let mut machine = machine();
    machine.checks = vec![
        check(
            "keychain",
            "Keychain",
            Level::Ok,
            "login keychain, unlocked",
            "",
        ),
        check(
            "schedule",
            "Daily renewal",
            Level::Warn,
            "not scheduled",
            "Turn on daily renewal in Pitboard's settings.",
        ),
    ];
    let checks = shown(&model).checks;
    assert!(checks.lines.is_empty());
    assert_eq!(checks.waiting.as_deref(), Some("Checking this Mac…"));
    assert_eq!((checks.summary, checks.checked), (None, None));
    assert_eq!(machine.doctor_asks, 0);

    assert_eq!(
        model.send(Intent::PaneShown {
            pane: Pane::Machine
        }),
        [Job::Check]
    );
    assert!(shown(&model).checks.checking);
    model.run(&mut machine);
    let checks = shown(&model).checks;
    assert!(!checks.checking);
    assert_eq!(
        checks.lines,
        [
            CheckLine {
                id: 0,
                code: "keychain".into(),
                name: "Keychain".into(),
                level: Level::Ok,
                spoken_level: "Passed".into(),
                detail: "Login keychain, unlocked".into(),
                advice: None,
            },
            CheckLine {
                id: 1,
                code: "schedule".into(),
                name: "Daily renewal".into(),
                level: Level::Warn,
                spoken_level: "Worth looking at".into(),
                detail: "Not scheduled".into(),
                advice: Some("Turn on daily renewal in Pitboard's settings.".into()),
            },
        ]
    );
    assert_eq!(
        checks.summary.as_deref(),
        Some("One thing is worth looking at.")
    );
    assert_eq!(checks.checked.as_deref(), Some("Checked at 08:00"));
    assert_eq!(checks.waiting, None);

    machine.checks = vec![check(
        "keychain",
        "Keychain",
        Level::Fail,
        "locked",
        "Unlock the login keychain.",
    )];
    model.later(Duration::from_secs(5 * 60));
    model.send(Intent::PaneShown {
        pane: Pane::Machine,
    });
    let checks = shown(&model).checks;
    assert!(checks.checking);
    assert_eq!(checks.lines.len(), 2, "what was found stays up meanwhile");
    assert_eq!(
        checks.checked, None,
        "the line over them says it is checking"
    );
    model.run(&mut machine);
    let checks = shown(&model).checks;
    assert_eq!(checks.lines.len(), 1);
    assert_eq!(
        checks.lines[0].advice.as_deref(),
        Some("Unlock the login keychain.")
    );
    assert_eq!(
        checks.summary.as_deref(),
        Some("1 broken: do not switch accounts until fixed.")
    );
    assert_eq!(checks.checked.as_deref(), Some("Checked at 08:05"));
    assert_eq!(machine.doctor_asks, 2);
}

/// Each check is a line of its own, though doctor names several by one code: it makes one
/// check of each enrolled account's parked login, and every one of a tool's is
/// `parked_login`. MachinePane.swift listed them by their code, so two accounts' checks were
/// two rows of one identity, which a SwiftUI list does not allow.
#[test]
fn each_check_is_a_line_of_its_own_though_two_share_a_code() {
    let mut model = Hand::new();
    let mut machine = machine();
    machine.checks = vec![
        check(
            "parked_login",
            "account work",
            Level::Ok,
            "lasts 9 days",
            "",
        ),
        check(
            "parked_login",
            "account personal",
            Level::Warn,
            "its parked login expires in 2 days",
            "Run `pitboard renew`.",
        ),
        check(
            "codex_parked_login",
            "account codex/main",
            Level::Ok,
            "kept",
            "",
        ),
    ];
    model.send(Intent::PaneShown {
        pane: Pane::Machine,
    });
    model.run(&mut machine);
    let lines = shown(&model).checks.lines;
    let ids: Vec<(u64, &str)> = lines
        .iter()
        .map(|line| (line.id, line.name.as_str()))
        .collect();
    assert_eq!(
        ids,
        [
            (0, "Account work"),
            (1, "Account personal"),
            (2, "Account codex/main")
        ]
    );
    assert_eq!(lines[0].code, lines[1].code, "as doctor names them");
}

/// What to do about a check is said only of one that did not pass, and only where doctor
/// says something: a check that passed says nothing more, whatever it carries, as
/// MachinePane.swift's rows said nothing more.
#[test]
fn only_a_check_that_did_not_pass_says_what_to_do() {
    let mut model = Hand::new();
    let mut machine = machine();
    machine.checks = vec![
        check(
            "keychain",
            "Keychain",
            Level::Ok,
            "unlocked",
            "Keep it unlocked.",
        ),
        check(
            "schedule",
            "Daily renewal",
            Level::Warn,
            "not scheduled",
            "",
        ),
        check(
            "state",
            "Accounts",
            Level::Fail,
            "unreadable",
            "Run pitboard doctor.",
        ),
    ];
    model.send(Intent::PaneShown {
        pane: Pane::Machine,
    });
    model.run(&mut machine);
    let advice: Vec<Option<String>> = shown(&model)
        .checks
        .lines
        .into_iter()
        .map(|line| line.advice)
        .collect();
    assert_eq!(advice, [None, None, Some("Run pitboard doctor.".into())]);
}

/// Checks asked for while others run are counted, as reads are: the pane says it is checking
/// until the last has come in. The Swift model's one flag was put down by the first to end.
#[test]
fn the_pane_checks_until_the_last_check_asked_for_is_in() {
    let mut model = Hand::new();
    let mut machine = machine();
    machine.checks = vec![check("keychain", "Keychain", Level::Ok, "unlocked", "")];
    let pane = Intent::PaneShown {
        pane: Pane::Machine,
    };
    model.send(pane.clone());
    model.send(pane);
    let first = model.next();
    model.give(machine.answer(first));
    let checks = shown(&model).checks;
    assert!(checks.checking, "one is still under way");
    assert_eq!(checks.checked, None);
    model.run(&mut machine);
    assert!(!shown(&model).checks.checking);
    assert_eq!(machine.doctor_asks, 2);
}

/// `pitboard doctor` looks at everything on this machine, so its checks are made when
/// somebody asks to see them, and not with the model's start, a read or a glance, and they
/// say when they were made.
///
/// AppModelTests.swift's doctorIsOnlyReadWhenAskedFor.
#[test]
fn doctor_is_only_read_when_asked_for() {
    let mut model = Hand::new();
    let mut machine = machine();
    machine.checks = vec![check("state", "Accounts", Level::Ok, "readable", "")];
    model.send(Intent::Start);
    model.run(&mut machine);
    model.send(Intent::Refresh { asked: true });
    model.run(&mut machine);
    model.later(Duration::from_secs(60));
    model.send(Intent::Glanced);
    model.send(Intent::PaneShown {
        pane: Pane::Accounts,
    });
    model.run(&mut machine);
    assert_eq!(machine.doctor_asks, 0);
    let checks = shown(&model).checks;
    assert!(checks.lines.is_empty());
    assert_eq!(checks.checked, None);

    model.send(Intent::PaneShown {
        pane: Pane::Machine,
    });
    model.run(&mut machine);
    let checks = shown(&model).checks;
    let codes: Vec<&str> = checks.lines.iter().map(|line| line.code.as_str()).collect();
    assert_eq!(codes, ["state"]);
    assert!(checks.checked.is_some());
    assert_eq!(machine.doctor_asks, 1);
}

// The 2026-10-18 session saw no replaced-login toast, and nothing said whether Windows refused it.
#[test]
fn a_notification_the_system_refused_is_said_after_doctors_checks_until_one_is_posted() {
    let replaced = |label: &str| Warning {
        account: Some(format!("claude:{label}")),
        ..warning(
            "login_replaced",
            &format!("{label}’s login was replaced by a sign-in outside Pitboard."),
        )
    };
    let accounts = || vec![claude("home", true, 5.0), claude("work", false, 0.0)];
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(warned(accounts(), vec![replaced("work")])));
    machine.checks = vec![check("keychain", "Keychain", Level::Ok, "unlocked", "")];
    machine.post_refused = Some("COMException 0x80070490".into());

    model.refresh(&mut machine);
    assert_eq!(machine.posted.len(), 1);
    let checks = shown(&model).checks;
    assert!(checks.lines.is_empty(), "said beside doctor's checks alone");
    assert_eq!(checks.waiting.as_deref(), Some("Checking this Mac…"));

    model.send(Intent::PaneShown {
        pane: Pane::Machine,
    });
    model.run(&mut machine);
    let checks = shown(&model).checks;
    assert_eq!(
        checks.lines[1..],
        [CheckLine {
            id: 1,
            code: "notification_not_posted".into(),
            name: "Notifications".into(),
            level: Level::Warn,
            spoken_level: "Worth looking at".into(),
            detail: "“A login was replaced outside Pitboard” not posted at 08:00 · \
                     COMException 0x80070490"
                .into(),
            advice: None,
        }]
    );
    assert_eq!(
        (checks.summary.as_deref(), checks.level),
        (Some("One thing is worth looking at."), Level::Warn)
    );

    machine.post_refused = None;
    machine.answer = Ok(warned(accounts(), vec![replaced("work"), replaced("home")]));
    model.send(Intent::Refresh { asked: true });
    model.run(&mut machine);
    assert_eq!(machine.posted.len(), 2);
    let checks = shown(&model).checks;
    let codes: Vec<&str> = checks.lines.iter().map(|line| line.code.as_str()).collect();
    assert_eq!(codes, ["keychain"], "gone once one is posted");
    assert_eq!(checks.level, Level::Ok);
}

/// When the checks were made is a clock time, made again on the minute tick: once the day
/// they were made on is over it names that day.
#[test]
fn when_the_checks_were_made_names_its_day_once_that_is_over() {
    let mut model = Hand::new();
    // 23:59 UTC on Friday 15 January 2027.
    model.now.epoch_ms = (1_800_000_000 + 15 * 3600 + 59 * 60) * 1000;
    let mut machine = machine();
    machine.checks = vec![check("keychain", "Keychain", Level::Ok, "unlocked", "")];
    model.send(Intent::Start);
    model.send(Intent::PaneShown {
        pane: Pane::Machine,
    });
    model.run(&mut machine);
    assert_eq!(
        shown(&model).checks.checked.as_deref(),
        Some("Checked at 23:59")
    );

    model.later(Duration::from_secs(60));
    model.tick();
    assert_eq!(
        shown(&model).checks.checked.as_deref(),
        Some("Checked at Fri 23:59")
    );
}

/// The core's log is kept newest last, and the activity pane lists what changed last first,
/// read every time the pane is shown, the newest 500 of them. Each change is said in words:
/// what was done, to what, how it ended and who asked, with when, as the person's clock says
/// it. The list says what it is for while it has nothing in it.
///
/// MachineModelTests.swift's changesAreShownNewestFirst, with PresentationTests.swift's
/// words for a change, which ActivityPane.swift said.
#[test]
fn changes_are_shown_newest_first() {
    let mut model = Hand::new();
    let mut machine = machine();
    machine.history = vec![
        change("2026-09-24T09:00:00Z", "cli", "enroll", "work", "ok"),
        change(
            "2026-09-26T09:00:00+02:00",
            "app",
            "switch",
            "personal",
            "ok",
        ),
        change(
            "2026-09-27T09:00:00Z",
            "app",
            "switch",
            "codex/spare",
            "nothing_parked",
        ),
    ];
    let activity = shown(&model).activity;
    assert!(activity.lines.is_empty());
    let empty = activity.empty.expect("what stands in for the list");
    assert_eq!(
        (empty.title.as_str(), empty.detail.as_str()),
        ("No Activity", "Pitboard lists every change it makes here.")
    );

    assert_eq!(
        model.send(Intent::PaneShown {
            pane: Pane::Activity
        }),
        [Job::ReadLog { limit: 500 }]
    );
    model.run(&mut machine);
    let activity = shown(&model).activity;
    assert_eq!(activity.empty, None);
    let lines: Vec<_> = activity
        .lines
        .iter()
        .map(|line| {
            (
                line.id,
                line.date.as_str(),
                line.change.as_str(),
                line.account.as_str(),
                line.result.as_str(),
                line.done,
                line.asked_by.as_str(),
            )
        })
        .collect();
    assert_eq!(
        lines,
        [
            (
                0,
                "Sun 09:00",
                "Switch",
                "codex/spare",
                "Nothing parked",
                false,
                "Pitboard app"
            ),
            (
                1,
                "Sat 07:00",
                "Switch",
                "personal",
                "Done",
                true,
                "Pitboard app"
            ),
            (
                2,
                "Thu 09:00",
                "Enrol",
                "work",
                "Done",
                true,
                "Command line"
            ),
        ]
    );

    machine
        .history
        .push(change("yesterday", "schedule", "sign_in", "work", "ok"));
    model.send(Intent::PaneShown {
        pane: Pane::Activity,
    });
    model.run(&mut machine);
    let newest = &shown(&model).activity.lines[0];
    assert_eq!(
        (
            newest.date.as_str(),
            newest.change.as_str(),
            newest.asked_by.as_str()
        ),
        ("yesterday", "Sign in", "Schedule"),
        "a time that does not read as one is said as the log keeps it"
    );
    assert_eq!(machine.log_limits, [500, 500]);
}

/// The accounts pane shown reads the accounts where the numbers shown are a minute old, as
/// AccountsPane.swift's `.task` did, and not otherwise.
#[test]
fn the_accounts_pane_shown_reads_numbers_a_minute_old() {
    let mut model = Hand::new();
    let mut machine = machine();
    model.refresh(&mut machine);
    let accounts = Intent::PaneShown {
        pane: Pane::Accounts,
    };
    assert_eq!(model.send(accounts.clone()), []);
    model.later(Duration::from_secs(60));
    assert!(matches!(
        model.send(accounts)[..],
        [Job::Read { fresh: false, .. }]
    ));
}

/// A terminal runs the first `pitboard` on its login shell's `PATH`, then looks where each
/// way of installing Pitboard puts one, and the settings say which it found: where it is and
/// how it is kept up to date, the one inside this app or one installed apart from it, or
/// that there is none, and only then offer to link this app's own, where it has one a link
/// would keep reaching. It is looked for only when the settings ask.
///
/// MachineModelTests.swift's theCommandLineIsLookedForWhereATerminalWouldFindIt, as the
/// settings say it; `lanes.rs` has where it is looked for, over real files.
#[test]
fn the_command_line_is_said_as_a_terminal_would_find_it() {
    let mut model = Hand::new();
    let mut machine = machine().with_a_command_line_inside();
    let command_line = shown(&model).command_line;
    assert_eq!((command_line.found, command_line.in_terminal), (None, None));
    assert!(!command_line.offers_link);

    assert_eq!(
        model.send(Intent::LookForCommandLine),
        [Job::FindCommandLine]
    );
    model.run(&mut machine);
    let command_line = shown(&model).command_line;
    assert_eq!(command_line.found, Some(FoundCommandLine::Nowhere));
    assert_eq!(command_line.in_terminal.as_deref(), Some("Not installed"));
    assert_eq!(command_line.update_note, None);
    assert!(command_line.offers_link);
    assert_eq!(command_line.cannot_link, None);

    for (found, note) in [
        (
            FoundCommandLine::Bundled {
                path: "/usr/local/bin/pitboard".into(),
            },
            "The one inside this app, so it updates with the app.",
        ),
        (
            FoundCommandLine::Another {
                path: "/Users/x/.cargo/bin/pitboard".into(),
            },
            "Installed apart from this app, so update it the way you installed it.",
        ),
    ] {
        machine.command_line = found.clone();
        model.send(Intent::LookForCommandLine);
        model.run(&mut machine);
        let command_line = shown(&model).command_line;
        let (FoundCommandLine::Bundled { path } | FoundCommandLine::Another { path }) = &found
        else {
            unreachable!("a command line found");
        };
        assert_eq!(command_line.in_terminal.as_ref(), Some(path));
        assert_eq!(command_line.update_note.as_deref(), Some(note));
        assert!(!command_line.offers_link, "one is there already");
    }
    assert_eq!(machine.command_line_asks, 3);
}

/// A copy run from a temporary place is not offered a link, and is told why: a link to that
/// would break once it quits. Only where none was found, as SettingsView.swift said it under
/// `.nowhere` alone: where a terminal finds one, no link is offered, so none is explained.
#[test]
fn a_copy_run_from_a_temporary_place_is_told_why_it_cannot_link() {
    let mut model = Hand::new();
    let mut machine = machine();
    machine.own = TEMPORARY;
    model.send(Intent::LookForCommandLine);
    model.run(&mut machine);
    let command_line = shown(&model).command_line;
    assert!(!command_line.offers_link);
    assert_eq!(
        command_line.cannot_link.as_deref(),
        Some(
            "Move Pitboard to your Applications folder first. Until then macOS runs it from a \
             temporary copy, and a link to that would break."
        )
    );

    for found in [
        FoundCommandLine::Bundled {
            path: "/usr/local/bin/pitboard".into(),
        },
        FoundCommandLine::Another {
            path: "/Users/x/.cargo/bin/pitboard".into(),
        },
    ] {
        machine.command_line = found.clone();
        model.send(Intent::LookForCommandLine);
        model.run(&mut machine);
        let command_line = shown(&model).command_line;
        assert!(!command_line.offers_link, "{found:?}");
        assert_eq!(command_line.cannot_link, None, "{found:?}");
    }
}

/// What came to nothing on a lane leaves what was shown, and nothing under way for good: a
/// change to the schedule reads it back, a check is no longer counted, and a renewal reads
/// the accounts all the same.
#[test]
fn what_came_to_nothing_holds_nothing_back() {
    use super::state::Answer;
    let mut model = Hand::new();
    let mut machine = machine().with_a_command_line_inside();
    model.refresh(&mut machine);

    model.send(Intent::SetSchedule { on: true });
    let set = model.next();
    assert_eq!(
        model.give(Answer::Lost(set)),
        [Job::ReadSchedule { after_change: true }]
    );
    let read = model.next();
    model.give(Answer::Lost(read));
    assert!(!shown(&model).schedule.changing);
    assert_eq!(shown(&model).schedule.failed, None);

    model.send(Intent::PaneShown {
        pane: Pane::Machine,
    });
    let check = model.next();
    model.give(Answer::Lost(check));
    assert!(!shown(&model).checks.checking);

    model.send(Intent::RenewNow);
    let renew = model.next();
    assert!(matches!(
        model.give(Answer::Lost(renew))[..],
        [Job::Read { fresh: true, .. }]
    ));
    model.run(&mut machine);
    let renewal = shown(&model).renewal;
    assert!(!renewal.renewing);
    assert_eq!(
        renewal.note, "Renew every parked login that is due.",
        "nothing said of what it did"
    );
}
