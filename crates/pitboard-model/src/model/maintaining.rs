//! What the model keeps of this machine rather than its accounts, as MachineModel.swift kept
//! it: the daily renewal schedule and the one change to it at a time, the repair once a
//! launch, doctor's checks, the activity log and the `pitboard` a terminal runs, with what the
//! settings, MachinePane.swift and ActivityPane.swift said of them; and the watch service, where
//! the front has no status item. The state is driven through `State::apply` by hand over a core
//! the test scripts, or, for the watch service, over the real core on a machine of the test's
//! own, and read as `present` makes it on a Mac, or on GNOME for the watch service, its clock
//! times in UTC. Opening at login and linking the command line with an administrator's password
//! are the app's own, and so are their tests.

use super::advice::Told;
use super::lanes::{Watching, Worker};
use super::machine::ScheduleFailure;
use super::preferences::Preferences;
use super::state::{Answer, Job};
use super::testing::machine::{Hand, Stub};
use super::testing::platform::Posted;
use super::testing::records::{PLIST, change, check, claude, refusal, status, warned, warning};
use super::testing::world::World;
use super::{AtLimit, Intent, Pane};
use crate::Front;
use crate::present::testing::Utc;
use crate::present::{CheckLine, MachineShown, WatchServiceShown, present_on, words};
use crate::{AppCore, Surfaces};
use crate::{FoundCommandLine, Level, OwnCommandLine, Schedule, Warning};
use pitboard_core::duty::autoswitch::Threshold;
use pitboard_core::provider::ProviderId;
use pitboard_core::state::Key;
use pitboard_core::watch_service::{Installed, Switching};
use std::sync::Arc;
use std::time::Duration;

/// What a Mac's app shows of this machine now.
fn shown(model: &Hand) -> MachineShown {
    present_on(Front::MacMenuBar, &model.state, model.now.epoch(), &Utc).machine
}

/// Whether the window says something went wrong: a read that did not answer, or something
/// asked for that did not happen. What is about the schedule is said beside its switch.
fn window_failure(model: &Hand) -> bool {
    model.state.failure.is_some() || model.state.presented.is_some()
}

/// A machine with one account and nothing on its scheduler, as the core answers.
fn machine() -> Stub {
    Stub::reading(Ok(status(vec![claude("work", true, 10.0)])))
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

fn no_command_line() -> String {
    words::cannot_schedule(Front::MacMenuBar, false)
}

fn move_it_first() -> String {
    words::cannot_schedule(Front::MacMenuBar, true)
}

fn one_issue() -> Option<String> {
    Some(words::check_issues(Front::MacMenuBar, 1))
}

/// Daily renewal reads as on only while the scheduler has the job. A scheduler this Mac does
/// not have is not a schedule that is on. Nothing is read until the settings ask.
#[test]
fn daily_renewal_is_on_only_while_the_scheduler_has_it() {
    let mut model = Hand::new();
    let mut machine = machine().with_a_command_line_inside();
    let first = shown(&model).schedule;
    assert_eq!((&model.state.machine.schedule, first.on), (&None, false));
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
        assert_eq!(
            model.state.machine.schedule,
            Some(scheduled),
            "{schedule:?}"
        );
        assert_eq!(schedule.on, on);
    }
    assert_eq!(machine.schedule_reads, 3);
}

// On a machine with no scheduler Pitboard writes to, the settings say so.
#[test]
fn the_settings_say_where_no_scheduler_is() {
    let mut model = Hand::new();
    let mut machine = machine().with_a_command_line_inside();
    machine.scheduled = installed();
    model.send(Intent::ReadSchedule);
    model.run(&mut machine);
    assert_eq!(shown(&model).schedule.note, None);

    machine.scheduled = Schedule::Unsupported;
    model.send(Intent::ReadSchedule);
    model.run(&mut machine);
    let schedule = shown(&model).schedule;
    assert_eq!(schedule.note, Some(words::no_scheduler(Front::MacMenuBar)));
}

/// Turning daily renewal on or off changes the scheduler, and the switch then shows what the
/// scheduler has, read again rather than assumed.
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
    assert_eq!(model.state.machine.schedule, Some(installed()));
    assert!(schedule.on && schedule.enabled && !model.state.machine.scheduling.is_some());
    assert_eq!(schedule.failed, None);

    model.send(Intent::SetSchedule { on: false });
    model.run(&mut machine);
    assert_eq!((machine.uninstalls, machine.schedule_reads), (1, 2));
    let schedule = shown(&model).schedule;
    assert_eq!(model.state.machine.schedule, Some(Schedule::Absent));
    assert!(!schedule.on);
}

/// The schedule runs the command line inside the app long after the app has quit. A copy
/// macOS runs from a temporary place is gone by then, and one with no command line inside it
/// has nothing to schedule, so turning renewal on there is refused with the reason and
/// nothing is asked of the scheduler, nor read. Turning it off never is: that is how a
/// schedule that cannot work is taken away.
///
/// A build directory with no command line inside it is `lanes.rs`'s, over a core made as the
/// app makes it.
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
        (TEMPORARY, move_it_first(), true),
        (OwnCommandLine::default(), no_command_line(), false),
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
        assert_eq!(schedule.note, Some(reason));
        assert_eq!(schedule.failed, None, "said once, under the switch");
        assert!(!model.state.machine.scheduling.is_some() && !schedule.enabled);
        assert!(
            !window_failure(&model),
            "said beside the switch, not in the window"
        );

        model.send(Intent::SetSchedule { on: false });
        model.run(&mut machine);
        assert_eq!(model.state.machine.schedule_failed, None);
        assert_eq!(machine.uninstalls, 1);
        assert_eq!(model.state.machine.schedule, Some(Schedule::Absent));
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
    assert_eq!(schedule.note, Some(move_it_first()));
}

/// A build run from Xcode is an app with no command line inside it, and a schedule of one
/// renews nothing, so none is offered. A command line inside the app that nobody can run is
/// the same as none, until it can be run, which is read again with the schedule. Nor is a
/// link to it offered, where none is found: it would run nothing.
///
/// Asking for a link is the app's own code to refuse, and `lanes.rs` has this over a real file.
#[test]
fn an_app_without_a_command_line_it_can_run_cannot_link_or_schedule() {
    let mut model = Hand::new();
    let mut machine = machine();
    machine.own = UNRUNNABLE;
    model.send(Intent::ReadSchedule);
    model.send(Intent::LookForCommandLine);
    model.run(&mut machine);
    let shown_now = shown(&model);
    assert_eq!(shown_now.schedule.note, Some(no_command_line()));
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
        schedule.note,
        Some(no_command_line()),
        "read with the repair at launch"
    );
    assert!(!schedule.enabled);

    machine.own.runs = true;
    assert_eq!(
        shown(&model).schedule.note,
        Some(no_command_line()),
        "not read again yet"
    );
    model.send(Intent::ReadSchedule);
    model.run(&mut machine);
    assert_eq!(shown(&model).schedule.note, None);

    machine.own = TEMPORARY;
    model.send(Intent::LookForCommandLine);
    model.run(&mut machine);
    assert_eq!(
        shown(&model).schedule.note,
        Some(move_it_first()),
        "read with the command line found"
    );
}

/// The switch shows the state asked for while the scheduler is being changed, rather than
/// snapping back to the old one until it answers and the schedule has been read back, which
/// can be a while behind a renewal or a switch. Pressed again meanwhile it does nothing: it
/// was pressed on a switch that did not yet show the first press.
#[test]
fn daily_renewal_shows_what_was_asked_for_while_it_changes() {
    let mut model = Hand::new();
    let mut machine = machine().with_a_command_line_inside();
    assert!(!model.state.machine.scheduling.is_some());

    model.send(Intent::SetSchedule { on: true });
    let schedule = shown(&model).schedule;
    assert!(schedule.on && model.state.machine.scheduling.is_some() && !schedule.enabled);
    assert_eq!(
        model.send(Intent::SetSchedule { on: false }),
        [],
        "the press made meanwhile"
    );
    let set = model.next();
    let read_back = model.give(machine.answer(set));
    assert_eq!(read_back, [Job::ReadSchedule { after_change: true }]);
    assert!(
        model.state.machine.scheduling.is_some(),
        "until the schedule is read back"
    );
    model.run(&mut machine);
    assert_eq!((machine.installs, machine.uninstalls), (1, 0));
    let schedule = shown(&model).schedule;
    assert!(schedule.on && !model.state.machine.scheduling.is_some());

    model.send(Intent::SetSchedule { on: false });
    assert!(!shown(&model).schedule.on, "what was asked for");
    assert_eq!(model.send(Intent::SetSchedule { on: true }), []);
    model.run(&mut machine);
    assert_eq!((machine.installs, machine.uninstalls), (1, 1));
    let schedule = shown(&model).schedule;
    assert!(!schedule.on && !model.state.machine.scheduling.is_some());
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
    assert!(model.state.machine.scheduling.is_some());
    model.give(machine.answer(set));
    model.run(&mut machine);
    assert!(!model.state.machine.scheduling.is_some());
}

/// When the scheduler refuses, the core's own words are said beside the switch, and the
/// switch shows the schedule as it is. The next try starts from nothing said.
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
        assert_eq!(
            model.state.machine.schedule.is_some(),
            reads == 1,
            "{schedule:?}"
        );
        assert_eq!(schedule.on, reads == 1);
        assert_eq!(schedule.failed, None);

        model.send(Intent::Start);
        model.run(&mut machine);
        assert_eq!(machine.repair_asks, 1, "once a launch");
    }
}

/// Doctor's checks are shown with when they were made, in place of the last ones, made again
/// every time the pane is shown, and the pane says it is checking while doctor runs. What
/// stands in for them before there are any says the machine is being checked.
#[test]
fn doctors_checks_are_shown_with_when_they_were_made() {
    let mut model = Hand::new();
    let mut machine = machine();
    machine.checks = vec![
        check("network", "network", Level::Ok, "direct", ""),
        check(
            "auth_source",
            "what a session authenticates with",
            Level::Warn,
            "something else: ANTHROPIC_API_KEY",
            "Remove it.",
        ),
    ];
    let checks = shown(&model).checks;
    assert!(checks.lines().is_empty());
    assert_eq!(checks.waiting, Some(words::checking(Front::MacMenuBar)));
    assert_eq!((checks.summary, checks.checked), (None, None));
    assert_eq!(machine.doctor_asks, 0);

    assert_eq!(
        model.send(Intent::PaneShown { pane: Pane::Checks }),
        [Job::Check]
    );
    assert!(shown(&model).checks.checking);
    model.run(&mut machine);
    let checks = shown(&model).checks;
    assert!(!checks.checking);
    assert_eq!(
        checks.lines(),
        [
            CheckLine {
                id: 0,
                name: words::check_name(Front::MacMenuBar, "network", "network"),
                level: Level::Ok,
                spoken_level: words::spoken_level(Level::Ok).into(),
                detail: words::check_text("direct"),
                advice: None,
                action: None,
                learn: None,
            },
            CheckLine {
                id: 1,
                name: words::check_name(Front::MacMenuBar, "auth_source", ""),
                level: Level::Warn,
                spoken_level: words::spoken_level(Level::Warn).into(),
                detail: words::check_text("something else: ANTHROPIC_API_KEY"),
                advice: Some(words::check_text("Remove it.")),
                action: None,
                learn: Some(crate::present::next::of_check(Front::MacMenuBar)),
            },
        ]
    );
    assert_eq!(checks.summary, one_issue());
    assert_eq!(checks.checked, Some(words::checked_at("08:00")));
    assert_eq!(checks.waiting, None);

    machine.checks = vec![check(
        "state",
        "accounts",
        Level::Fail,
        "unreadable",
        "Pitboard will not switch until its account list can be read.",
    )];
    model.later(Duration::from_secs(5 * 60));
    model.send(Intent::PaneShown { pane: Pane::Checks });
    let checks = shown(&model).checks;
    assert!(checks.checking);
    assert_eq!(checks.lines().len(), 2, "what was found stays up meanwhile");
    assert_eq!(
        checks.checked, None,
        "the line over them says it is checking"
    );
    model.run(&mut machine);
    let checks = shown(&model).checks;
    assert_eq!(checks.lines().len(), 1);
    assert_eq!(
        checks.lines()[0].advice,
        Some(words::check_text(
            "Pitboard will not switch until its account list can be read."
        ))
    );
    assert_eq!(
        (checks.summary, checks.verdict.as_deref()),
        (one_issue(), Some(words::checks_verdict(1, 1)))
    );
    assert_eq!(checks.checked, Some(words::checked_at("08:05")));
    assert_eq!(machine.doctor_asks, 2);
}

// Plan 4.6: Copy Report copies doctor's report as `pitboard doctor --json` gives it.
#[test]
fn copy_report_copies_doctors_own_report() {
    let mut model = Hand::new();
    let mut machine = machine();
    machine.checks = vec![check("keychain", "Keychain", Level::Ok, "unlocked", "")];
    machine.report = "{\"checks\": [{\"code\": \"keychain\"}]}".into();
    assert_eq!(shown(&model).checks.report, None);
    model.send(Intent::PaneShown { pane: Pane::Checks });
    model.run(&mut machine);
    assert_eq!(shown(&model).checks.report, Some(machine.report.clone()));
}

/// Each check is a line of its own, though doctor names several by one code: it makes one
/// check of each enrolled account's parked login, and every one of a tool's is
/// `parked_login`. MachinePane.swift listed them by their code, so two accounts' checks were
/// two rows of one identity, which a SwiftUI list does not allow.
#[test]
fn each_check_is_a_line_of_its_own_though_two_share_a_code() {
    let mut model = Hand::new();
    let mut machine = machine();
    let of = |account: Key, check| crate::Check {
        account: Some(account),
        ..check
    };
    machine.checks = vec![
        of(
            Key::new(ProviderId::Claude, "work"),
            check(
                "parked_login",
                "account work",
                Level::Ok,
                "lasts 9 days",
                "",
            ),
        ),
        of(
            Key::new(ProviderId::Claude, "personal"),
            check(
                "parked_login",
                "account personal",
                Level::Warn,
                "its parked login expires in 2 days",
                "Run `pitboard renew`.",
            ),
        ),
        of(
            Key::new(ProviderId::Codex, "main"),
            check(
                "codex_parked_login",
                "account codex/main",
                Level::Ok,
                "kept",
                "",
            ),
        ),
    ];
    model.send(Intent::PaneShown { pane: Pane::Checks });
    model.run(&mut machine);
    let lines = shown(&model).checks.lines();
    let ids: Vec<(u64, String)> = lines
        .iter()
        .map(|line| (line.id, line.name.clone()))
        .collect();
    assert_eq!(
        ids,
        [
            (0, "work".into()),
            (1, "personal".into()),
            (2, "main".into())
        ]
    );
    let checks = &model.state.machine.checks;
    assert_eq!(checks[0].code, checks[1].code, "as doctor names them");
}

/// What to do about a check is said only of one that did not pass, and only where doctor
/// says something: a check that passed says nothing more, whatever it carries, as
/// MachinePane.swift's rows said nothing more.
#[test]
fn only_a_check_that_did_not_pass_says_what_to_do() {
    let mut model = Hand::new();
    let mut machine = machine();
    machine.checks = vec![
        check("network", "network", Level::Ok, "direct", "Keep it so."),
        check(
            "state",
            "Accounts",
            Level::Fail,
            "unreadable",
            "Run pitboard doctor.",
        ),
        check(
            "auth_source",
            "what a session authenticates with",
            Level::Warn,
            "something else",
            "",
        ),
    ];
    model.send(Intent::PaneShown { pane: Pane::Checks });
    model.run(&mut machine);
    let advice: Vec<Option<String>> = shown(&model)
        .checks
        .lines()
        .into_iter()
        .map(|line| line.advice)
        .collect();
    assert_eq!(
        advice,
        [None, Some(words::check_text("Run pitboard doctor.")), None]
    );
}

/// Checks asked for while others run are counted, as reads are: the pane says it is checking
/// until the last has come in. The Swift model's one flag was put down by the first to end.
#[test]
fn the_pane_checks_until_the_last_check_asked_for_is_in() {
    let mut model = Hand::new();
    let mut machine = machine();
    machine.checks = vec![check("keychain", "Keychain", Level::Ok, "unlocked", "")];
    let pane = Intent::PaneShown { pane: Pane::Checks };
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
#[test]
fn doctor_is_only_read_when_asked_for() {
    let mut model = Hand::new();
    let mut machine = machine();
    machine.checks = vec![check("network", "network", Level::Ok, "direct", "")];
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
    assert!(checks.lines().is_empty());
    assert_eq!(checks.checked, None);

    model.send(Intent::PaneShown { pane: Pane::Checks });
    model.run(&mut machine);
    let checks = shown(&model).checks;
    assert_eq!(checks.lines().len(), 1);
    let codes: Vec<&str> = (model.state.machine.checks.iter())
        .map(|check| check.code.as_str())
        .collect();
    assert_eq!(codes, ["network"]);
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
    let mut machine = Stub::reading(Ok(warned(accounts(), vec![replaced("work")])));
    machine.checks = vec![check("network", "network", Level::Ok, "direct", "")];
    machine.post_refused = Some("COMException 0x80070490".into());

    model.refresh(&mut machine);
    assert_eq!(machine.posted.len(), 1);
    let checks = shown(&model).checks;
    assert!(
        checks.lines().is_empty(),
        "said beside doctor's checks alone"
    );
    assert_eq!(checks.waiting, Some(words::checking(Front::MacMenuBar)));

    model.send(Intent::PaneShown { pane: Pane::Checks });
    model.run(&mut machine);
    let checks = shown(&model).checks;
    assert_eq!(
        checks.lines()[1..],
        [CheckLine {
            id: 1,
            name: words::NOTIFICATIONS.into(),
            level: Level::Warn,
            spoken_level: words::spoken_level(Level::Warn).into(),
            detail: words::check_text(&words::not_posted(
                &words::sign_in_replaced(Front::MacMenuBar),
                "08:00",
                "COMException 0x80070490"
            )),
            advice: None,
            action: None,
            learn: Some(crate::present::next::of_check(Front::MacMenuBar)),
        }]
    );
    assert_eq!((checks.summary, checks.level), (one_issue(), Level::Warn));

    machine.post_refused = None;
    machine.answer = Ok(warned(accounts(), vec![replaced("work"), replaced("home")]));
    model.send(Intent::Refresh { asked: true });
    model.run(&mut machine);
    assert_eq!(machine.posted.len(), 2);
    let checks = shown(&model).checks;
    let names: Vec<String> = (checks.lines().iter())
        .map(|line| line.name.clone())
        .collect();
    assert_eq!(names, ["Network"], "gone once one is posted");
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
    model.send(Intent::PaneShown { pane: Pane::Checks });
    model.run(&mut machine);
    assert_eq!(
        shown(&model).checks.checked,
        Some(words::checked_at("23:59"))
    );

    model.later(Duration::from_secs(60));
    model.tick();
    assert_eq!(
        shown(&model).checks.checked,
        Some(words::checked_at("Fri 23:59"))
    );
}

/// The core's log is kept newest last, and the activity pane lists what changed last first,
/// read every time the pane is shown, the newest 500 of them. Each change is said in words:
/// what was done, to what, how it ended and who asked, with when, as the person's clock says
/// it. The list says what it is for while it has nothing in it.
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
    let (title, detail) = words::no_activity(Front::MacMenuBar);
    assert_eq!((empty.title, empty.detail.as_str()), (title, detail));

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
                line.change.clone(),
                line.account.as_str(),
                line.result.clone(),
                line.done,
                line.asked_by.clone(),
            )
        })
        .collect();
    let said = |verb, outcome, caller| {
        (
            words::change_verb(verb),
            words::change_outcome(outcome),
            words::change_caller(caller),
        )
    };
    let line = |id, date, (change, result, asked_by), account, done| {
        (id, date, change, account, result, done, asked_by)
    };
    assert_eq!(
        lines,
        [
            line(
                0,
                "Sun 09:00",
                said("switch", "nothing_parked", "app"),
                "codex/spare",
                false
            ),
            line(
                1,
                "Sat 07:00",
                said("switch", "ok", "app"),
                "personal",
                true
            ),
            line(2, "Thu 09:00", said("enroll", "ok", "cli"), "work", true),
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
            newest.change.clone(),
            newest.asked_by.clone()
        ),
        (
            "yesterday",
            words::change_verb("sign_in"),
            words::change_caller("schedule")
        ),
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
/// `lanes.rs` has where it is looked for, over real files.
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
    assert_eq!(
        command_line.in_terminal.as_deref(),
        Some(words::NOT_INSTALLED)
    );
    assert_eq!(command_line.update_note, None);
    assert!(command_line.offers_link);
    assert_eq!(command_line.cannot_link, None);
    assert_eq!(
        command_line.terminal_note, None,
        "a Mac offers to link it instead"
    );
    assert_eq!(
        present_on(
            Front::WindowsNotificationArea,
            &model.state,
            model.now.epoch(),
            &Utc
        )
        .machine
        .command_line
        .terminal_note
        .as_deref(),
        Some(words::NEW_TERMINALS),
        "Windows' installer puts it on the PATH"
    );
    assert_eq!(
        present_on(Front::Gnome, &model.state, model.now.epoch(), &Utc)
            .machine
            .command_line
            .terminal_note,
        None,
        "a Linux package installs it where every terminal looks"
    );

    for (found, note) in [
        (
            FoundCommandLine::Bundled {
                path: "/usr/local/bin/pitboard".into(),
            },
            words::update_note(Front::MacMenuBar, true),
        ),
        (
            FoundCommandLine::Another {
                path: "/Users/x/.cargo/bin/pitboard".into(),
            },
            words::update_note(Front::MacMenuBar, false),
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
        assert_eq!(command_line.terminal_note, None);
    }
    assert_eq!(machine.command_line_asks, 3);

    machine.command_line = FoundCommandLine::Bundled {
        path: "/usr/bin/pitboard".into(),
    };
    model.send(Intent::LookForCommandLine);
    model.run(&mut machine);
    assert_eq!(
        present_on(Front::Gnome, &model.state, model.now.epoch(), &Utc)
            .machine
            .command_line
            .update_note
            .as_deref(),
        Some(words::update_note(Front::Gnome, true)),
        "the Linux app's packages put it beside the app, in a package of its own"
    );
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
        command_line.cannot_link,
        Some(words::cannot_link(Front::MacMenuBar))
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
/// change to the schedule reads it back, and a check is no longer counted.
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
    assert!(!model.state.machine.scheduling.is_some());
    assert_eq!(shown(&model).schedule.failed, None);

    model.send(Intent::PaneShown { pane: Pane::Checks });
    let check = model.next();
    model.give(Answer::Lost(check));
    assert!(!shown(&model).checks.checking);
}

/// A lane's worker over `core`, which answers each job as the model's lanes do.
fn worker(core: Arc<AppCore>) -> Worker {
    Worker {
        core,
        other_apps: None,
        notifier: Arc::new(Posted::default()),
        local_time: Arc::new(Utc),
        earlier: None,
        windows: None,
        widget_feed: None,
        quit_within: Duration::from_millis(50),
        quit_checked_every: Duration::from_millis(5),
        sign_ins: Arc::default(),
        watch: Watching::default(),
        next_look: Arc::default(),
    }
}

/// A scratch machine of the real core's, its scheduler in memory, whose default home holds an
/// app with a command line beside it for the service to run.
fn service_machine(name: &str) -> World {
    let mut world = World::new(name);
    world.in_the_default_home();
    world.app_with_a_command_line();
    world
}

fn switch_at(percent: u8) -> Switching {
    Switching::At(Threshold::new(percent).expect("a share"))
}

/// What GNOME's app shows of the watch service now.
fn watch_service(model: &Hand) -> Option<WatchServiceShown> {
    present_on(Front::Gnome, &model.state, model.now.epoch(), &Utc)
        .machine
        .watch_service
}

/// Without a status item, closing the window quits Pitboard, so the settings offer to run the
/// automatic switch and the warm-ups in the watch service, which the real core installs at the
/// share the settings have, and the notes say what runs once the window is closed.
#[test]
fn without_a_status_item_the_watch_service_is_offered_and_turning_it_on_installs_it() {
    let world = service_machine("watch-service");
    let lanes = worker(world.core_on(Front::Gnome));
    let mut model = Hand::on(Front::Gnome);
    model.state.preferences.at_limit = Some("switch".into());
    model.send(Intent::ReadSchedule);
    model.run_by(|job| lanes.work(job));
    let about = words::watch_service_about(Front::Gnome, switch_at(95));
    assert_eq!(
        watch_service(&model),
        Some(WatchServiceShown {
            on: false,
            enabled: true,
            note: None,
            failed: None,
            about: about.clone(),
        })
    );
    let notes = |model: &Hand| {
        let machine = present_on(Front::Gnome, &model.state, 0, &Utc).machine;
        (machine.at_limit.note.unwrap_or_default(), machine.warm.note)
    };
    let (at_limit, warm) = notes(&model);
    assert!(
        at_limit.ends_with(&words::switching_once_closed(false)),
        "{at_limit}"
    );
    assert_eq!(warm, words::warm_up_note(Some(false)));

    assert_eq!(
        model.send(Intent::SetWatchService { on: true }),
        [Job::SetWatchService {
            on: true,
            switching: Switching::At(Threshold::DEFAULT),
        }]
    );
    assert_eq!(
        watch_service(&model).map(|shown| (shown.on, shown.enabled)),
        Some((true, false)),
        "what was asked for, while it changes"
    );
    model.run_by(|job| lanes.work(job));
    assert_eq!(
        watch_service(&model),
        Some(WatchServiceShown {
            on: true,
            enabled: true,
            note: None,
            failed: None,
            about,
        })
    );
    let Installed::Yes { switching, .. } = world.elsewhere().watch_service() else {
        panic!("installed in the scratch machine");
    };
    assert_eq!(switching, Some(Switching::At(Threshold::DEFAULT)));
    let (at_limit, warm) = notes(&model);
    assert!(
        at_limit.ends_with(&words::switching_once_closed(true)),
        "{at_limit}"
    );
    assert_eq!(warm, words::warm_up_note(Some(true)));

    model.send(Intent::SetWatchService { on: false });
    model.run_by(|job| lanes.work(job));
    assert_eq!(watch_service(&model).map(|shown| shown.on), Some(false));
    assert_eq!(world.elsewhere().watch_service(), Installed::No);
}

/// A status item keeps Pitboard running once its window is closed, so nothing is offered, read
/// or installed, on a front that has one from the start or once its desktop gives it one.
#[test]
fn with_a_status_item_it_is_not_offered() {
    let mut mac = Hand::new();
    let mut gnome = Hand::on(Front::Gnome);
    gnome.send(Intent::SurfacesChanged {
        surfaces: Surfaces { status_item: true },
    });
    for (model, front) in [(&mut mac, Front::MacMenuBar), (&mut gnome, Front::Gnome)] {
        assert_eq!(
            model.send(Intent::ReadSchedule),
            [Job::ReadSchedule {
                after_change: false
            }],
            "{front:?}"
        );
        assert_eq!(model.send(Intent::SetWatchService { on: true }), []);
        let machine = present_on(front, &model.state, 0, &Utc).machine;
        assert_eq!(machine.watch_service, None, "{front:?}");
        assert_eq!(machine.warm.note, words::warm_up_note(None));
    }
}

/// The watch service does at a limit what Limits says. Under Switch a new share installs it
/// again at that one, and under Notify Me or Do Nothing it is installed again to never switch,
/// each once a change to it under way has been read back. Limits and the service's line say
/// what it does once the window is closed.
#[test]
fn the_watch_service_follows_the_share_and_limits_says_what_it_does() {
    let world = service_machine("watch-service-share");
    let lanes = worker(world.core_on(Front::Gnome));
    let mut model = Hand::on(Front::Gnome);
    let preferences = Preferences {
        auto_switch: true,
        has_been_seen: true,
        ..Preferences::default()
    };
    model.give(Answer::Kept {
        told: Told::new(),
        preferences: Some((preferences, true)),
        windows: None,
    });
    model.send(Intent::SetWatchService { on: true });
    model.run_by(|job| lanes.work(job));
    let installed = || match world.elsewhere().watch_service() {
        Installed::Yes { switching, .. } => switching,
        Installed::No | Installed::Unsupported => None,
    };
    assert_eq!(installed(), Some(switch_at(95)));
    let reinstalls = |jobs: &[Job]| {
        (jobs.iter())
            .filter(|job| matches!(job, Job::SetWatchService { .. }))
            .cloned()
            .collect::<Vec<_>>()
    };
    let reinstall = |switching| {
        [Job::SetWatchService {
            on: true,
            switching,
        }]
    };
    let set = |model: &mut Hand, choice, at| model.send(Intent::SetAtLimit { choice, at });
    let note = |model: &Hand| {
        present_on(Front::Gnome, &model.state, 0, &Utc)
            .machine
            .at_limit
            .note
            .unwrap_or_default()
    };
    let about = |model: &Hand| watch_service(model).map(|shown| shown.about);

    let jobs = set(&mut model, AtLimit::Switch, 80);
    assert_eq!(reinstalls(&jobs), reinstall(switch_at(80)));
    let jobs = set(&mut model, AtLimit::Switch, 85);
    assert_eq!(reinstalls(&jobs), [], "a change to it is under way");
    model.run_by(|job| lanes.work(job));
    assert_eq!(installed(), Some(switch_at(85)));
    assert_eq!(
        about(&model),
        Some(words::watch_service_about(Front::Gnome, switch_at(85)))
    );
    assert!(
        note(&model).ends_with(&words::switching_once_closed(true)),
        "{}",
        note(&model)
    );

    let jobs = set(&mut model, AtLimit::Notify, 85);
    assert_eq!(reinstalls(&jobs), reinstall(Switching::Never));
    model.run_by(|job| lanes.work(job));
    assert_eq!(installed(), Some(Switching::Never));
    assert!(
        note(&model).ends_with(words::NOTIFYING_ONCE_CLOSED),
        "{}",
        note(&model)
    );
    assert_eq!(
        about(&model),
        Some(words::watch_service_about(Front::Gnome, Switching::Never))
    );
    let jobs = set(&mut model, AtLimit::Notify, 70);
    assert_eq!(reinstalls(&jobs), [], "it switches at no share");
    let jobs = set(&mut model, AtLimit::Nothing, 70);
    assert_eq!(reinstalls(&jobs), [], "it never switched");
    assert_eq!(note(&model), words::ONLY_WARMING_UP);

    let jobs = set(&mut model, AtLimit::Switch, 70);
    assert_eq!(reinstalls(&jobs), reinstall(switch_at(70)));
    let jobs = set(&mut model, AtLimit::Nothing, 70);
    assert_eq!(reinstalls(&jobs), [], "a change to it is under way");
    model.run_by(|job| lanes.work(job));
    assert_eq!(
        installed(),
        Some(Switching::Never),
        "as Limits says once read back"
    );

    model.send(Intent::SetWatchService { on: false });
    model.run_by(|job| lanes.work(job));
    let jobs = set(&mut model, AtLimit::Switch, 90);
    assert_eq!(reinstalls(&jobs), [], "nothing is installed to follow it");
    assert_eq!(world.elsewhere().watch_service(), Installed::No);
    set(&mut model, AtLimit::Notify, 90);
    assert!(
        !note(&model).contains(words::NOTIFYING_ONCE_CLOSED),
        "{}",
        note(&model)
    );
}

/// A terminal can install or take away the watch service while the app runs, so each visit to
/// the settings reads it again from the core, as it reads daily renewal.
#[test]
fn the_watch_service_is_read_on_each_visit() {
    let world = service_machine("watch-service-read");
    let lanes = worker(world.core_on(Front::Gnome));
    let mut model = Hand::on(Front::Gnome);
    assert_eq!(watch_service(&model).map(|shown| shown.on), Some(false));
    assert_eq!(
        model.send(Intent::ReadSchedule),
        [
            Job::ReadSchedule {
                after_change: false
            },
            Job::ReadWatchService {
                after_change: false
            }
        ]
    );
    model.run_by(|job| lanes.work(job));
    assert_eq!(watch_service(&model).map(|shown| shown.on), Some(false));

    world
        .elsewhere()
        .watch_service_install(switch_at(90))
        .expect("installed from a terminal");
    model.send(Intent::ReadSchedule);
    model.run_by(|job| lanes.work(job));
    let shown = watch_service(&model).expect("offered");
    assert!(shown.on);
    assert_eq!(
        shown.about,
        words::watch_service_about(Front::Gnome, switch_at(90))
    );

    world
        .elsewhere()
        .watch_service_uninstall()
        .expect("taken away from a terminal");
    model.send(Intent::ReadSchedule);
    model.run_by(|job| lanes.work(job));
    assert_eq!(watch_service(&model).map(|shown| shown.on), Some(false));
}
