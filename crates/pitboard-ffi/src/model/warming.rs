//! Warming up accounts, where somebody turned it on for one: when the model asks the core to
//! warm up whichever is due, what it says of what came of it, and what the settings, each
//! row and each row's menu show of it. Driven through `State::apply` by hand; when an account
//! is due, and what a warm-up runs, are the core's own and have their own tests.

use super::state::{Answer, Job};
use super::testing::machine::{Hand, Stub};
use super::testing::records::{Refusal, refusal, status, titled};
use super::testing::world::World;
use super::{Intent, RunOutNotice};
use crate::present::testing::{LimitExt, account, window};
use crate::present::words;
use crate::{Account, PitboardError};
use pitboard_core::duty::warm::{Choice, Failure, Halt, Overview, Range, Standing};
use pitboard_core::error::Error;
use pitboard_core::switch::warm::{Apart, WarmUp, told_apart};
use pitboard_core::testing::{Ran, Started, WarmScript};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Two hours after the moment the hand's clock starts at, 10:00 in UTC on the same day.
const UNTIL: i64 = 1_800_007_200;

/// `personal`, parked, its five-hour period not running.
fn personal() -> Account {
    account(Some("personal"))
        .limits(vec![window("session", 0.0).resets(None)])
        .build()
}

/// What the core's files say of warming up: `personal` turned on or off, where it stands.
fn overview(on: bool, standing: Standing) -> Overview {
    Overview {
        range: Range::DEFAULT,
        halted: None,
        accounts: vec![Choice {
            account: "personal".into(),
            qualified: "claude/personal".into(),
            on,
            standing: on.then(|| standing.clone()),
            now: standing,
        }],
    }
}

fn warm_up(job: &Job) -> bool {
    matches!(job, Job::WarmUp)
}

/// The app started over `machine`, with its first read landed.
fn started(machine: &mut Stub) -> Hand {
    let mut model = Hand::new();
    model.send(Intent::Start);
    model.run(machine);
    model
}

fn reading(warm: Overview) -> Stub {
    let mut machine = Stub::reading(Ok(status(vec![personal()])));
    machine.warm = Some(warm);
    machine.warming = Ok(WarmUp::Stands(Box::new(overview(false, Standing::Due))));
    machine
}

fn came_to_nothing() -> (Apart, Refusal) {
    (
        told_apart(&Error::WarmUpFailed {
            label: "personal".into(),
            why: Failure::NotStarted,
        }),
        refusal(
            "warm_up_not_started",
            "`personal` was not warmed up: the request was answered, and Anthropic still has no \
             period running.",
            Vec::new(),
        ),
    )
}

#[test]
fn nothing_is_warmed_up_unless_an_account_is_turned_on() {
    let mut machine = reading(overview(false, Standing::Due));
    let model = started(&mut machine);
    assert!(machine.warmed.is_empty(), "the core is not even asked");
    let shown = model.shown().machine.warm;
    assert!(shown.enabled);
    assert_eq!(shown.range_label, "06:00 to 22:00");
    assert_eq!(shown.accounts.len(), 1);
    assert!(!shown.accounts[0].on);
    assert_eq!(shown.standing, None);
}

/// An account turned on is asked for after every read, and every 30 seconds though nothing
/// was written, as `pitboard watch` asks: a period resets with nobody writing anything.
#[test]
fn an_account_turned_on_is_asked_for_after_each_read_and_every_thirty_seconds() {
    let mut machine = reading(overview(true, Standing::Due));
    let mut model = started(&mut machine);
    assert_eq!(machine.warmed, [None], "after the first read");
    machine.warm = Some(overview(
        true,
        Standing::Running {
            until: UNTIL,
            next: Some(UNTIL),
        },
    ));
    model.later(Duration::from_secs(29));
    model.tick();
    model.run(&mut machine);
    assert_eq!(machine.warmed.len(), 1, "not before thirty seconds");
    model.later(Duration::from_secs(1));
    model.tick();
    model.run(&mut machine);
    assert_eq!(machine.warmed.len(), 2);
    assert_eq!(
        model.shown().machine.warm.standing.as_deref(),
        Some("Next warm-up: personal at 10:00."),
        "what the core said as it found nothing due"
    );
}

/// One under way holds the next back, so a slow run is never asked for twice.
#[test]
fn one_warm_up_runs_at_a_time() {
    let mut machine = reading(overview(true, Standing::Due));
    let mut model = started(&mut machine);
    let before = model.count(warm_up);
    model.later(Duration::from_secs(30));
    model.tick();
    model.run_but(&mut machine, warm_up);
    assert_eq!(model.count(warm_up), before + 1);
    model.later(Duration::from_secs(30));
    model.tick();
    assert_eq!(
        model.count(warm_up),
        before + 1,
        "the first is still under way"
    );
    assert!(
        model
            .send(Intent::WarmNow {
                qualified: "claude/personal".into()
            })
            .is_empty(),
        "nor is one asked for now"
    );
}

/// One by itself that came to nothing is notified once in the core's words, said under the
/// setting until one starts a period, and asked for again no sooner than the core waits
/// after as many attempts.
#[test]
fn a_warm_up_by_itself_that_came_to_nothing_is_notified_once_and_waits() {
    let mut machine = reading(overview(true, Standing::Due));
    machine.warming = Err(came_to_nothing());
    let mut model = started(&mut machine);
    assert_eq!(machine.warmed.len(), 1);
    assert_eq!(
        machine.posted,
        [RunOutNotice {
            id: "warm-failed/personal/warm_up_not_started".into(),
            title: words::COULD_NOT_WARM_ITSELF.into(),
            subtitle: None,
            body: "`personal` was not warmed up: the request was answered, and Anthropic \
                   still has no period running."
                .into(),
            clicked: "limits".into(),
            button: None,
        }]
    );
    assert!(
        model
            .shown()
            .machine
            .warm
            .standing
            .is_some_and(|said| said.starts_with("The last warm-up came to nothing: ")),
    );
    model.later(Duration::from_secs(59));
    model.tick();
    model.run(&mut machine);
    assert_eq!(machine.warmed.len(), 1, "it waits a minute");
    model.later(Duration::from_secs(1));
    model.tick();
    model.run(&mut machine);
    assert_eq!(machine.warmed.len(), 2);
    assert_eq!(machine.posted.len(), 1, "said once");

    machine.warming = Ok(WarmUp::Started {
        account: "personal".into(),
        until: Some(18_000),
    });
    model.later(Duration::from_secs(120));
    model.tick();
    model.run(&mut machine);
    assert!(
        !model
            .shown()
            .machine
            .warm
            .standing
            .is_some_and(|said| said.starts_with("The last warm-up came to nothing")),
        "a period started ends what was said"
    );
}

// Claude Code's run, failing in words of its own each time, as a request id in them does.
#[derive(Debug, Default)]
struct FailsInOtherWords(AtomicUsize);

impl WarmScript for FailsInOtherWords {
    fn run(&self, _: &Started) -> Ran {
        let run = self.0.fetch_add(1, Ordering::SeqCst);
        Ran {
            exit: Some(1),
            stdout: format!(
                r#"{{"type":"result","is_error":true,"result":"request {run} timed out"}}"#
            ),
            timed_out: false,
        }
    }
}

#[test]
fn a_failure_in_other_words_is_notified_once() {
    let mut world = World::new("warm-other-words");
    world.enrolled("work", "work", 10.0);
    world.parked("personal", "personal", 0.0);
    world.resting("personal");
    world.elsewhere().status(true).expect("a read");
    world.warming_by(Arc::new(FailsInOtherWords::default()));
    let core = world.core();
    let refused = || {
        let refused = core
            .warm_now("claude/personal")
            .expect_err("came to nothing");
        let PitboardError::Failed {
            code,
            message,
            warnings,
        } = refused.error;
        (refused.told_apart, refusal(&code, &message, warnings))
    };
    let (first, second) = (refused(), refused());
    assert_ne!(first.1.message, second.1.message);

    let mut machine = reading(overview(true, Standing::Due));
    machine.warming = Err(first);
    let mut model = started(&mut machine);
    machine.warming = Err(second);
    model.later(Duration::from_secs(60));
    model.tick();
    model.run(&mut machine);
    assert_eq!(machine.warmed.len(), 2);
    let posted: Vec<&str> = machine
        .posted
        .iter()
        .map(|notice| notice.id.as_str())
        .collect();
    assert_eq!(posted, ["warm-failed/personal/warm_up_failed"]);
}

// As `pitboard watch` keeps it: another account warming up says nothing of this one.
#[test]
fn a_failure_is_not_notified_again_after_another_account_warms_up() {
    let mut machine = reading(overview(true, Standing::Due));
    machine.warming = Err(came_to_nothing());
    let mut model = started(&mut machine);
    machine.warming = Ok(WarmUp::Started {
        account: "work".into(),
        until: Some(UNTIL),
    });
    model.later(Duration::from_secs(60));
    model.tick();
    model.run(&mut machine);
    machine.warming = Err(came_to_nothing());
    model.later(Duration::from_secs(60));
    model.tick();
    model.run(&mut machine);
    assert_eq!(machine.warmed.len(), 4, "and once more as work warmed up");
    assert_eq!(machine.posted.len(), 1);
}

/// Asked for now, it warms that account up, and what went wrong is said in the window, as
/// anything somebody asks for is, and not notified.
#[test]
fn asked_for_now_what_went_wrong_is_said_in_the_window() {
    let mut machine = reading(overview(false, Standing::Due));
    machine.warming = Err(came_to_nothing());
    let mut model = started(&mut machine);
    model.send(Intent::WarmNow {
        qualified: "claude/personal".into(),
    });
    model.run(&mut machine);
    assert_eq!(machine.warmed, [Some("claude/personal".to_owned())]);
    let shown = model.shown();
    let failure = model.state.presented.clone().expect("said");
    assert_eq!(failure.title, "Couldn’t warm up personal");
    assert_eq!(failure.code.as_deref(), Some("warm_up_not_started"));
    assert!(machine.posted.is_empty());
    assert_eq!(
        shown.machine.warm.standing, None,
        "nor said under the setting"
    );
}

/// What one by itself came to nothing over is said under the setting only until the core next
/// says where each account stands.
#[test]
fn a_failure_is_said_until_the_core_says_where_each_account_stands() {
    let mut machine = reading(overview(true, Standing::Due));
    machine.warming = Err(came_to_nothing());
    let mut model = started(&mut machine);
    assert!(
        model
            .shown()
            .machine
            .warm
            .standing
            .is_some_and(|said| said.starts_with("The last warm-up came to nothing: "))
    );
    let held = overview(
        true,
        Standing::Held(pitboard_core::duty::warm::Hold::NoReading),
    );
    machine.warm = Some(held.clone());
    machine.warming = Ok(WarmUp::Stands(Box::new(held)));
    model.later(Duration::from_secs(60));
    model.tick();
    model.run(&mut machine);
    assert_eq!(machine.warmed.len(), 2);
    assert_eq!(model.shown().machine.warm.standing, None);
}

/// A warm-up whose answer never came is asked for again at the next tick, as though it had
/// come to nothing said.
#[test]
fn a_warm_up_lost_on_the_way_is_asked_for_again() {
    let mut machine = reading(overview(true, Standing::Due));
    let mut model = started(&mut machine);
    let before = model.count(warm_up);
    model.later(Duration::from_secs(30));
    model.tick();
    model.run_but(&mut machine, warm_up);
    assert_eq!(model.count(warm_up), before + 1);
    model.give(Answer::Lost(Job::WarmUp));
    model.later(Duration::from_secs(30));
    model.tick();
    assert_eq!(model.count(warm_up), before + 2);
}

/// Which accounts are warmed up, where the core cannot read it, is said under the setting,
/// and a change to it is refused in the window.
#[test]
fn a_setting_that_cannot_be_read_is_said_and_a_change_to_it_refused() {
    let mut machine = reading(overview(false, Standing::Due));
    let message = "which accounts Pitboard warms up, at /home/warm.json, cannot be read \
                   (expected value). Delete it, then turn warm-up on again for each account \
                   you want it for.";
    machine.warm_refused = Some(refusal("warm_setting_corrupt", message, Vec::new()));
    let mut model = started(&mut machine);
    assert_eq!(
        model.shown().machine.warm.standing.as_deref(),
        Some(
            "Which accounts Pitboard warms up, at /home/warm.json, cannot be read (expected \
             value). Delete it, then turn warm-up on again for each account you want it for."
        )
    );
    model.send(Intent::SetWarm {
        qualified: "claude/personal".into(),
        on: true,
    });
    model.run(&mut machine);
    let failure = model.state.presented.clone().expect("refused");
    assert_eq!(
        failure.title,
        "Couldn’t change which accounts are warmed up"
    );
    assert_eq!(failure.code.as_deref(), Some("warm_setting_corrupt"));
}

/// Turning an account on is the core's to keep, and an account due is warmed up at once.
#[test]
fn turning_an_account_on_warms_it_up_where_it_is_due() {
    let mut machine = reading(overview(false, Standing::Due));
    let mut model = started(&mut machine);
    assert!(machine.warmed.is_empty());
    model.send(Intent::SetWarm {
        qualified: "claude/personal".into(),
        on: true,
    });
    model.run(&mut machine);
    assert_eq!(machine.warm_turned, [("claude/personal".to_owned(), true)]);
    assert_eq!(machine.warmed, [None]);
    model.send(Intent::SetWarmRange {
        from: 420,
        to: 1380,
    });
    model.run(&mut machine);
    assert_eq!(machine.warm_ranges, [(420, 1380)]);
    let shown = model.shown().machine.warm;
    assert_eq!((shown.from, shown.to), (420, 1380));
    assert_eq!(shown.range_label, "07:00 to 23:00");
}

// Turning it on and off is Settings' alone, where the hours and every account's state are.
#[test]
fn a_row_says_where_its_warm_up_stands_and_its_menu_offers_warm_up_now() {
    let mut machine = reading(overview(
        true,
        Standing::Running {
            until: UNTIL,
            next: Some(UNTIL),
        },
    ));
    machine.warming = Ok(WarmUp::Stands(Box::new(overview(
        true,
        Standing::Running {
            until: UNTIL,
            next: Some(UNTIL),
        },
    ))));
    let model = started(&mut machine);
    let shown = model.shown();
    let item = shown
        .sections
        .iter()
        .flat_map(|section| &section.accounts)
        .find(|item| item.title == "personal")
        .expect("personal");
    assert_eq!(item.warm_note.as_deref(), Some("Warms up again at 10:00"));
    let warm: Vec<(String, bool)> = item
        .offers
        .iter()
        .filter(|offer| offer.title.starts_with("Warm"))
        .map(|offer| (offer.title.clone(), offer.enabled))
        .collect();
    assert_eq!(
        warm,
        [(titled("Warm Up Now", "Warm up now"), false)],
        "its period runs, so nothing to start now"
    );
}

/// Where nothing can be warmed up here, the settings say why, and nothing is offered now.
#[test]
fn a_halt_is_said_under_the_setting() {
    let mut halted = overview(true, Standing::Due);
    halted.halted = Some(Halt::ProgramMissing);
    let mut machine = reading(halted.clone());
    machine.warming = Ok(WarmUp::Stands(Box::new(halted)));
    let model = started(&mut machine);
    assert_eq!(
        model.shown().machine.warm.standing.as_deref(),
        Some(
            "Pitboard is not warming up accounts: Claude Code is not installed where Pitboard looks."
        )
    );
}
