//! What the model does by itself, and when: the timers the Swift model ran as loops of
//! `Task.sleep`, which its tests never ran. A read now and every five minutes after the last
//! one the timer asked for ended; a look now, then as the watch on Pitboard's directory hears
//! a write, and every two seconds after the last look ended only where nothing watches it;
//! with switching by itself on the core's look every 30 seconds; and nothing at all before
//! `Start`.

use super::state::{Answer, Cadence, Job};
use super::testing::machine::{Hand, Stub, a_look, a_watch, any_read, fresh_read};
use super::testing::records::{claude, status};
use super::{Intent, Tool};
use crate::AccountKey;
use std::time::Duration;

fn machine() -> Stub {
    Stub::reading(Ok(status(vec![claude("work", true, 10.0)])))
}

fn looks(jobs: &[Job]) -> usize {
    jobs.iter().filter(|job| matches!(job, Job::Look)).count()
}

/// Until an app says to start, nothing runs by itself: a test drives every read and knows
/// what set what, and the actor waits for a message with no timer to wake it.
#[test]
fn nothing_runs_by_itself_until_started() {
    let mut model = Hand::new();
    assert_eq!(model.state.next_due(), None);
    model.later(Duration::from_secs(3_600));
    assert_eq!(model.tick(), []);
    assert_eq!(model.state.next_due(), None);
}

/// Starting reads the accounts at once and looks for changes at once, as the Swift model's
/// loops did on their first turn, reads what the model keeps, the record of what was told,
/// repairs a schedule an older app wrote, as the Swift model's init did, and watches
/// Pitboard's directory for the next change. The read waits for what is installed, which the
/// first read asks, and not for what is kept, which only advice waits for.
#[test]
fn starting_reads_at_once_and_looks_for_changes() {
    let mut model = Hand::new();
    assert_eq!(
        model.send(Intent::Start),
        [
            Job::LoadKept,
            Job::RepairSchedule,
            Job::Watch,
            Job::Look,
            Job::AskInstalled
        ]
    );
    let _kept = model.next();
    let _repair = model.next();
    let _watch = model.next();
    let look = model.next();
    let ask = model.next();
    let mut machine = machine();
    model.give(machine.answer(look));
    let read = model.give(machine.answer(ask));
    assert!(matches!(read[..], [Job::Read { fresh: false, .. }]));
}

/// Started once: a second start, which an app reopening its window could send, starts no
/// second loop of either, and no second watch.
#[test]
fn a_second_start_starts_nothing_more() {
    let mut model = Hand::new();
    model.send(Intent::Start);
    assert_eq!(model.send(Intent::Start), []);
    model.run(&mut machine());
    assert_eq!(model.send(Intent::Start), []);
    assert_eq!(model.count(any_read), 1);
    assert_eq!(model.count(a_watch), 1);
}

/// Started, with the watch on Pitboard's directory running, as its thread answers nothing
/// until it hears a write.
fn watched(machine: &mut Stub) -> Hand {
    let mut model = Hand::new();
    model.send(Intent::Start);
    let _watch = model.take(a_watch);
    model.run(machine);
    model
}

/// While the watch runs, the look is asked once as the model starts and then only as the
/// watch hears a write, at once: no timer looks, an hour on.
#[test]
fn a_watched_directory_is_looked_at_only_as_it_is_written() {
    let mut machine = machine();
    let mut model = watched(&mut machine);
    assert_eq!(model.count(a_look), 1, "once, as it starts");
    for _ in 0..30 {
        model.later(Duration::from_secs(120));
        model.tick();
        model.run(&mut machine);
    }
    assert_eq!(model.count(a_look), 1, "no timer looks");

    assert_eq!(looks(&model.give(Answer::Written)), 1, "a write, at once");
    model.run(&mut machine);
    model.later(Duration::from_secs(2));
    assert_eq!(looks(&model.tick()), 0);
}

/// A write heard while a look is under way may have come after the look read the files' times,
/// so another look follows that one, and only one: one look at a time.
#[test]
fn a_write_heard_during_a_look_is_looked_at_once_it_is_over() {
    let mut machine = machine();
    let mut model = watched(&mut machine);
    assert_eq!(looks(&model.give(Answer::Written)), 1);
    assert_eq!(looks(&model.give(Answer::Written)), 0, "one look at a time");
    assert_eq!(looks(&model.give(Answer::Written)), 0);
    model.run(&mut machine);
    assert_eq!(model.count(a_look), 3, "once it is over, and once only");
    model.later(Duration::from_secs(2));
    assert_eq!(looks(&model.tick()), 0);
}

/// A write heard while a change of this app's own holds the account index is looked at only
/// once the change is over, here as the read after it lands: the look leaves the index to the
/// change meanwhile, and a change that fails, which reads nothing after it, is shown by it.
#[test]
fn a_write_heard_during_a_change_of_the_apps_own_is_looked_at_once_it_is_over() {
    let mut machine = machine();
    let mut model = watched(&mut machine);
    model.send(Intent::Rename {
        account: AccountKey::new(Tool::Claude, "work"),
        to: "office".into(),
    });
    assert_eq!(looks(&model.give(Answer::Written)), 0, "the change's own");
    let looked = model.count(a_look);
    model.run(&mut machine);
    assert_eq!(model.count(a_look), looked + 1, "once the change is over");
}

/// Where the watch says nothing watches Pitboard's directory, the poll looks again two
/// seconds after its last look ended, and not while one is still under way: a look that
/// found a change waits for what is known to be read, and a read on the network holds the
/// look up behind it on the one lane of reads. Once a watch begins again, it looks as the
/// watch hears a write alone.
#[test]
fn the_poll_looks_every_two_seconds_only_where_nothing_watches() {
    let mut model = Hand::new();
    let mut machine = machine();
    model.send(Intent::Start);
    // The stub's watch answers that nothing is watched.
    model.run(&mut machine);
    assert_eq!(model.state.next_due(), Some(Duration::from_secs(2)));

    model.later(Duration::from_millis(1_999));
    assert_eq!(looks(&model.tick()), 0);
    model.later(Duration::from_millis(1));
    let look = model.tick();
    assert_eq!(looks(&look), 1);

    machine.changed += 1;
    let answer = machine.answer(model.next());
    assert!(matches!(model.give(answer)[..], [Job::ReadOffline { .. }]));
    model.later(Duration::from_secs(5));
    assert_eq!(looks(&model.tick()), 0, "one look at a time");
    model.run(&mut machine);
    assert_eq!(
        model.state.next_due(),
        Some(Duration::from_secs(9)),
        "two seconds after it ended"
    );

    assert_eq!(looks(&model.give(Answer::Written)), 1, "watched again");
    model.run(&mut machine);
    model.later(Duration::from_secs(2));
    assert_eq!(looks(&model.tick()), 0, "and no timer looks");
}

/// The timer reads every five minutes after its own last read ended. A read somebody asked
/// for meanwhile leaves it where it was, as the Swift model's loop slept on regardless.
#[test]
fn the_accounts_are_read_five_minutes_after_the_timers_last_read() {
    let mut model = Hand::new();
    let mut machine = machine();
    model.send(Intent::Start);
    model.run(&mut machine);
    assert_eq!(Cadence::APP.read_every, Duration::from_secs(300));

    model.later(Duration::from_secs(100));
    model.send(Intent::Refresh { asked: true });
    model.run(&mut machine);
    assert_eq!(model.count(any_read), 2);

    // Every look in between, as the actor would wake for each.
    for _ in 0..99 {
        model.later(Duration::from_secs(2));
        model.tick();
        model.run(&mut machine);
    }
    assert_eq!(model.count(any_read), 2, "not before five minutes");
    model.later(Duration::from_secs(2));
    model.tick();
    model.run(&mut machine);
    assert_eq!(model.count(any_read), 3);
    assert_eq!(
        model.count(fresh_read),
        1,
        "the timer's is not somebody asking"
    );
}

/// With switching by itself on, the core's look is asked every 30 seconds whatever was
/// written, as often as `pitboard watch` decides: by the core's one number.
#[test]
fn the_core_is_asked_whether_to_switch_as_often_as_watch_decides() {
    assert_eq!(Cadence::APP.decide_every, Duration::from_secs(30));
    assert_eq!(
        Cadence::APP.decide_every.as_secs(),
        pitboard_core::duty::cadence::DECIDE_EVERY_SECONDS.unsigned_abs()
    );
}

/// Waking reads whatever the age of the numbers, since numbers read before the machine
/// slept say nothing about now, and it is not somebody asking.
#[test]
fn waking_reads_whatever_the_age_of_the_numbers() {
    let mut model = Hand::new();
    let mut machine = machine();
    model.refresh(&mut machine);
    model.later(Duration::from_secs(1));
    let woke = model.send(Intent::Woke);
    assert!(matches!(woke[..], [Job::Read { fresh: false, .. }]));
}

/// A timer that is due goes off with whatever message comes next, so messages arriving one
/// after another cannot hold it back past its time: here the poll, where nothing watches.
#[test]
fn a_timer_due_goes_off_with_any_message() {
    let mut model = Hand::new();
    let mut machine = machine();
    model.send(Intent::Start);
    model.run(&mut machine);
    model.later(Duration::from_secs(2));
    let glanced = model.send(Intent::Glanced);
    assert_eq!(
        glanced,
        [Job::Look],
        "the numbers are fresh, and the poll is due"
    );
}
