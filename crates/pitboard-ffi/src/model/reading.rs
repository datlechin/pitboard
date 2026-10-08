//! Reading the accounts, noticing what changed elsewhere, and what is installed, as the
//! Swift model did them: its tests on those, each under its own name in snake case, driven
//! through `State::apply` by hand. Where a Swift test also checked what is said about a
//! read, which comes with the model's wording, the rest of it is kept here.

use super::state::{Answer, Job};
use super::testing::{
    Hand, Machine, any_read, claude, claude_code, codex, codex_account, fresh_read, installed_ask,
    offline_read, percent, refusal, status, warned, warning,
};
use super::{Intent, ReadFailure};
use crate::Warning;
use std::time::Duration;

/// AppModelTests.swift's aReadFillsInTheTitleAndTheRows. The title is said with the menu
/// bar's wording; the number it says is here.
#[test]
fn a_read_fills_in_the_rows() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(vec![claude("work", true, 42.0)])));
    model.refresh(&mut machine);
    let shown = model.shown();
    assert_eq!(shown.status.as_ref().map(|s| s.accounts.len()), Some(1));
    assert_eq!(percent(&shown, 0), Some(42.0));
    assert_eq!(shown.read_failure, None);
}

/// Pitboard's errors already say what to do, so the panel shows the message as it is.
#[test]
fn a_failed_read_is_shown_as_its_own_message() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Err(refusal(
        "state_wrong_machine",
        "was written on another computer",
        Vec::new(),
    )));
    model.refresh(&mut machine);
    let shown = model.shown();
    assert_eq!(
        shown.read_failure,
        Some(ReadFailure {
            code: "state_wrong_machine".into(),
            message: "was written on another computer".into(),
        })
    );
    assert_eq!(
        shown.status.map(|s| s.accounts.is_empty()),
        Some(true),
        "the panel falls back to what is already known, which here is nothing"
    );
}

/// A timer producing a reading is not somebody asking for one, and the core decides whether
/// to go to Anthropic from that. The panel's own Refresh is asking; everything else is not.
#[test]
fn only_asking_for_a_reading_asks_anthropic_again() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(Vec::new())));

    model.refresh(&mut machine);
    assert_eq!(
        model.count(fresh_read),
        0,
        "a poll takes whatever the core already knows"
    );

    model.send(Intent::Refresh { asked: true });
    model.run(&mut machine);
    assert_eq!(model.count(fresh_read), 1);
}

/// Three front ends run on one machine and none of them could tell when another had changed
/// anything. A switch typed in a terminal left the menu bar naming the account the person
/// had just stopped using, for as long as five minutes.
#[test]
fn a_change_made_somewhere_else_is_noticed_without_asking_anthropic() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(Vec::new())));
    machine.offline = Ok(status(vec![claude("work", true, 5.0)]));

    // A read establishes where things stand, and costs one offline read at most.
    model.refresh(&mut machine);
    let before = model.count(offline_read);

    // Something else changes the account index.
    machine.changed = 42;
    model.notice(&mut machine);

    assert_eq!(
        model.count(offline_read),
        before + 1,
        "it read what is already known"
    );
    assert_eq!(model.count(fresh_read), 0, "and asked Anthropic nothing");
    let shown = model.shown();
    let accounts = shown.status.expect("accounts").accounts;
    assert_eq!(accounts[0].label.as_deref(), Some("work"));
}

/// Every read records its answers, and every session's status line what its session has
/// seen where it moves a limit an answer gave. The menu bar read 20% while every status line
/// said 22%, because it only ever showed what it had asked Anthropic itself. It follows the
/// readings the way it follows a switch made elsewhere: from what is already known, asking
/// nobody.
#[test]
fn numbers_a_session_recorded_reach_the_menu_bar_without_asking_anyone() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(vec![claude("work", true, 20.0)])));
    model.refresh(&mut machine);
    assert_eq!(percent(&model.shown(), 0), Some(20.0));

    machine.offline = Ok(status(vec![claude("work", true, 22.0)]));
    machine.readings += 1;
    model.notice(&mut machine);

    assert_eq!(percent(&model.shown(), 0), Some(22.0));
    assert_eq!(
        model.count(offline_read),
        1,
        "it read what is already known"
    );
    assert_eq!(model.count(fresh_read), 0, "and asked Anthropic nothing");
}

/// The app's own read records what it measured, which moves the readings, and a session can
/// record something newer while the app is asking. Either costs one look at what is
/// recorded, which is a file, and never a second read of anyone.
#[test]
fn the_apps_own_read_costs_one_look_at_the_readings_at_most() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(vec![claude("work", true, 20.0)])));
    machine.offline = machine.answer.clone();
    model.notice(&mut machine);
    model.refresh(&mut machine);
    model.notice(&mut machine);
    model.notice(&mut machine);
    assert_eq!(model.count(offline_read), 1);
    assert_eq!(model.count(fresh_read), 0);
    assert_eq!(percent(&model.shown(), 0), Some(20.0));
}

/// Anthropic's answer can be behind what a busy session records while the app is waiting
/// on it. The file then holds the session's newer numbers, and the app's own read writes
/// nothing over them. Noted as seen when the read was done, they reached the menu bar only
/// once some session wrote again.
#[test]
fn numbers_a_session_recorded_during_the_apps_own_read_are_shown_on_the_next_look() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(vec![claude("work", true, 21.0)])));
    model.notice(&mut machine);
    machine.offline = Ok(status(vec![claude("work", true, 22.0)]));
    model.refresh(&mut machine);
    assert_eq!(percent(&model.shown(), 0), Some(21.0));

    model.notice(&mut machine);
    assert_eq!(percent(&model.shown(), 0), Some(22.0));
}

/// A read that could not reach Anthropic still has something true to show. An empty panel
/// says the accounts are gone, which is not what happened.
#[test]
fn a_failed_first_read_falls_back_to_what_is_already_known() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Err(refusal(
        "unreachable",
        "could not reach Anthropic",
        Vec::new(),
    )));
    machine.offline = Ok(status(vec![claude("work", true, 5.0)]));

    model.refresh(&mut machine);

    let shown = model.shown();
    assert_eq!(
        shown.read_failure.map(|f| f.message).as_deref(),
        Some("could not reach Anthropic")
    );
    assert_eq!(
        shown.status.map(|s| s.accounts.len()),
        Some(1),
        "the last numbers measured are still true"
    );
}

/// Every warning, not only the first. A switch can warn about an overriding environment
/// variable and a config that did not update, and showing one is how somebody fixes the
/// wrong thing. A read that answered did not fail, however much it warns about, so none of
/// them is said as a read that could not be done.
#[test]
fn every_warning_is_kept_not_only_the_first() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(warned(
        Vec::new(),
        vec![
            warning("auth_overridden", "ANTHROPIC_API_KEY is set"),
            warning("config_write_failed", "the config did not update"),
        ],
    )));
    model.refresh(&mut machine);
    let shown = model.shown();
    let codes: Vec<&str> = shown.warnings.iter().map(|w| w.code.as_str()).collect();
    assert_eq!(codes, ["auth_overridden", "config_write_failed"]);
    assert_eq!(shown.read_failure, None);
}

/// A failure carries its own warnings. Leaving the last successful read's in place put a
/// fresh network error above warnings about things that may have been fixed since.
#[test]
fn a_failed_read_shows_its_own_warnings_rather_than_the_last_ones() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(warned(
        Vec::new(),
        vec![warning(
            "state_on_synced_drive",
            "~/.pitboard is on iCloud Drive",
        )],
    )));
    model.refresh(&mut machine);
    let shown = model.shown();
    assert_eq!(shown.warnings[0].code, "state_on_synced_drive");
    assert_eq!(shown.read_failure, None, "a warning is not a failed read");

    machine.answer = Err(refusal(
        "identity_unverifiable",
        "Anthropic could not be reached",
        vec![warning("overriding_env", "ANTHROPIC_API_KEY is set")],
    ));
    model.refresh(&mut machine);
    let shown = model.shown();
    let codes: Vec<&str> = shown.warnings.iter().map(|w| w.code.as_str()).collect();
    assert_eq!(codes, ["overriding_env"]);
    assert_eq!(
        shown.read_failure.map(|f| f.message).as_deref(),
        Some("Anthropic could not be reached")
    );
}

/// What a read says while a sign-in outside Pitboard has replaced the only login of `who`,
/// with Claude Code holding `now`'s, as the core words it and names the account.
fn replaced(who: &str, now: &str) -> Warning {
    Warning {
        account: Some(format!("claude:{who}")),
        ..warning(
            "login_replaced",
            &format!(
                "`{who}`'s login was replaced by a sign-in outside Pitboard: Claude Code now has \
                 `{now}`'s login stored, and Pitboard holds no login for `{who}`. Run `pitboard \
                 enroll {who} --sign-in` to sign in to it again."
            ),
        )
    }
}

/// A sign-in outside Pitboard that replaced the only login of an account is said on every
/// read until it is put right, and posted once while it stands, since nobody may have the
/// window open: a read that carries it again posts nothing, and one after it went and came
/// back posts it again.
#[test]
fn a_login_replaced_outside_is_posted_once_while_it_stands() {
    let replaced = replaced("work", "home");
    let accounts = || vec![claude("home", true, 5.0), claude("work", false, 0.0)];
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(warned(accounts(), vec![replaced.clone()])));

    model.refresh(&mut machine);
    let posted: Vec<(&str, &str, Option<&str>)> = machine
        .posted
        .iter()
        .map(|notice| {
            (
                notice.title.as_str(),
                notice.body.as_str(),
                notice.switch_to.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        posted,
        [(
            "A login was replaced outside Pitboard",
            replaced.message.as_str(),
            None
        )]
    );

    model.send(Intent::Refresh { asked: true });
    model.run(&mut machine);
    assert_eq!(machine.posted.len(), 1, "once while it stands");

    machine.answer = Ok(warned(accounts(), Vec::new()));
    model.send(Intent::Refresh { asked: true });
    model.run(&mut machine);
    machine.answer = Ok(warned(accounts(), vec![replaced]));
    model.send(Intent::Refresh { asked: true });
    model.run(&mut machine);
    assert_eq!(machine.posted.len(), 2, "and again once it is back");
}

/// What a replaced login's words say Claude Code holds now changes with every switch made
/// since, and it is still the one account's login replaced, which was posted already.
/// Another account replaced is posted on its own.
#[test]
fn a_login_replaced_outside_is_posted_once_whatever_is_stored_since() {
    let accounts = || {
        vec![
            claude("home", false, 5.0),
            claude("spare", true, 0.0),
            claude("work", false, 0.0),
        ]
    };
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(warned(accounts(), vec![replaced("work", "home")])));
    model.refresh(&mut machine);
    assert_eq!(machine.posted.len(), 1);

    machine.answer = Ok(warned(accounts(), vec![replaced("work", "spare")]));
    model.send(Intent::Refresh { asked: true });
    model.run(&mut machine);
    assert_eq!(
        machine.posted.len(),
        1,
        "the same account's login, posted once"
    );

    let both = vec![replaced("work", "spare"), replaced("home", "spare")];
    machine.answer = Ok(warned(accounts(), both.clone()));
    model.send(Intent::Refresh { asked: true });
    model.run(&mut machine);
    let posted: Vec<(&str, &str)> = machine
        .posted
        .iter()
        .map(|notice| (notice.id.as_str(), notice.body.as_str()))
        .collect();
    assert_eq!(
        posted[1..],
        [(
            "warning/login_replaced/claude:home",
            both[1].message.as_str()
        )]
    );
}

/// AppModelTests.swift's whatIsInstalledIsAskedAgainWhileNothingIsFound, apart from what
/// the window says about it. A login shell too slow to answer finds nothing the first time,
/// and the core asks it once more later, so a read asks again while nothing has been found.
/// Once a tool is found the question stops.
#[test]
fn what_is_installed_is_asked_again_while_nothing_is_found() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(Vec::new())));
    machine.found = Vec::new();
    model.refresh(&mut machine);
    assert_eq!(model.shown().installed, Some(Vec::new()));

    machine.found = vec![claude_code()];
    model.refresh(&mut machine);
    assert_eq!(model.count(installed_ask), 2);
    assert_eq!(model.shown().installed, Some(vec![claude_code()]));
    model.refresh(&mut machine);
    assert_eq!(model.count(installed_ask), 2);
}

/// AppModelTests.swift's whatIsInstalledIsAskedOnce, apart from what the sheet offers. What
/// the app found does not change while it runs, so it is asked once, and not each time what
/// is shown is taken, nor on every read.
///
/// Not when the model is made either: finding a tool can mean waiting on the person's login
/// shell.
#[test]
fn what_is_installed_is_asked_once() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(Vec::new())));
    machine.found = vec![claude_code(), codex()];
    assert_eq!(model.count(installed_ask), 0);
    assert_eq!(
        model.shown().installed,
        None,
        "nothing is known to be here until a read asks"
    );
    model.refresh(&mut machine);
    for _ in 0..3 {
        model.shown();
    }
    model.refresh(&mut machine);
    assert_eq!(model.count(installed_ask), 1);
    assert_eq!(model.shown().installed, Some(vec![claude_code(), codex()]));
}

/// AppModelTests.swift's aReadThatStartedBeforeAChangeIsDroppedWhenItLands, for a change the
/// poll noticed. The cases of a switch and of giving up are in `switching.rs`; enrolling,
/// renaming, forgetting and signing in come with those changes.
///
/// A read waits on a service with who was signed in when it started, and whatever is changed
/// meanwhile, by this app or somewhere else the poll noticed, is changed before the read
/// lands. Taken as it was, the read showed the accounts as they had been and put away what
/// the change had said. It is dropped: the read the change starts itself says what is true
/// now.
#[test]
fn a_read_that_started_before_a_change_is_dropped_when_it_lands() {
    let before = vec![
        codex_account("personal", true),
        codex_account("work", false),
        codex_account("spare", false),
    ];
    let after = status(vec![
        codex_account("personal", false),
        codex_account("work", true),
    ]);
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(before.clone())));
    model.refresh(&mut machine);

    // What the held read finds when it starts, with a warning nothing after it carries.
    machine.answer = Ok(warned(
        before,
        vec![warning("auth_overridden", "OPENAI_API_KEY is set")],
    ));
    model.send(Intent::Refresh { asked: true });
    let held = machine.answer(model.next());

    machine.answer = Ok(after.clone());
    machine.offline = Ok(after.clone());
    machine.changed += 1;
    model.later(Duration::from_secs(3));
    model.notice(&mut machine);
    let said = model.shown();
    assert_eq!(said.status.as_ref(), Some(&after));

    model.later(Duration::from_secs(3));
    model.give(held);
    let shown = model.shown();
    assert_eq!(
        shown.status.as_ref(),
        Some(&after),
        "not the accounts as they were before the change"
    );
    assert_eq!(shown.warnings, said.warnings);
    assert_eq!(shown.updated_at, said.updated_at);
    assert!(!shown.reading);
}

/// AppModelTests.swift's aFailedReadThatStartedBeforeASwitchSaysNothingOnceItLands is a
/// switch's, and is in `switching.rs`; this is the same for a change the poll noticed. A
/// read that fails, started before the change and landing after it, failed on the machine
/// as it was, and is not said over what the change showed.
#[test]
fn a_failed_read_that_started_before_a_change_says_nothing_once_it_lands() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(vec![
        codex_account("personal", true),
        codex_account("work", false),
    ])));
    model.refresh(&mut machine);

    let unreachable = warning("unreachable", "OpenAI could not be reached");
    machine.answer = Err(refusal(
        "unreachable",
        &unreachable.message,
        vec![unreachable.clone()],
    ));
    model.send(Intent::Refresh { asked: true });
    let held = machine.answer(model.next());

    let after = status(vec![
        codex_account("personal", false),
        codex_account("work", true),
    ]);
    machine.offline = Ok(after.clone());
    machine.changed += 1;
    model.notice(&mut machine);

    model.give(held);
    let shown = model.shown();
    assert_eq!(shown.read_failure, None);
    assert!(shown.warnings.is_empty());
    assert_eq!(shown.status, Some(after));
    assert!(!shown.reading);
}

/// A switch typed in a terminal while the app's read waits on a service changes the account
/// index after the read has taken who is signed in. Counted as seen once the read landed, it
/// was never shown; counted as things stood when the read started, the next look finds it.
#[test]
fn a_change_made_during_a_read_is_noticed_on_the_next_look() {
    let before = status(vec![
        claude("work", true, 0.0),
        claude("personal", false, 0.0),
    ]);
    let elsewhere = status(vec![
        claude("work", false, 0.0),
        claude("personal", true, 0.0),
    ]);
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(before.clone()));
    model.send(Intent::Refresh { asked: false });
    let ask = model.next();
    model.give(machine.answer(ask));
    let read = machine.answer(model.next());
    machine.changed += 1;
    machine.offline = Ok(elsewhere.clone());
    model.give(read);
    assert_eq!(model.shown().status, Some(before));

    model.notice(&mut machine);
    assert_eq!(model.shown().status, Some(elsewhere));
    assert_eq!(model.count(offline_read), 1);
    assert_eq!(model.count(fresh_read), 0);
}

/// Reads overlap: the timer's with one somebody asked for, a menu opening with the read after
/// a switch. The refresh buttons say a read is running while either is, and the first to end
/// used to say none was while the other still waited on a service.
#[test]
fn a_read_is_running_while_either_of_two_is() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(vec![claude("work", true, 0.0)])));
    assert!(!model.shown().reading);
    model.send(Intent::Refresh { asked: false });
    model.send(Intent::Refresh { asked: true });
    let ask = model.next();
    model.give(machine.answer(ask));
    let first = machine.answer(model.next());
    let second = machine.answer(model.next());
    assert!(model.shown().reading);

    model.give(first);
    assert!(model.shown().updated_at.is_some());
    assert!(model.shown().reading, "the other is still waiting");

    model.give(second);
    assert!(!model.shown().reading);
}

/// Opening a menu reads the accounts again only once the numbers shown are a minute old:
/// usage is asked of each tool's service for every account, and a menu is opened far more
/// often than the numbers change. Before anything has been read there is nothing to keep.
#[test]
fn opening_a_menu_reads_only_numbers_a_minute_old() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(vec![claude("work", true, 10.0)])));
    assert_eq!(
        super::state::Cadence::APP.stale_after,
        Duration::from_secs(60)
    );
    model.send(Intent::Glanced);
    model.run(&mut machine);
    assert_eq!(model.count(any_read), 1, "nothing was read yet");

    model.later(Duration::from_secs(59));
    model.send(Intent::Glanced);
    model.run(&mut machine);
    assert_eq!(model.count(any_read), 1, "read under a minute ago");

    model.later(Duration::from_secs(1));
    model.send(Intent::Glanced);
    model.run(&mut machine);
    assert_eq!(model.count(any_read), 2, "a minute old");
    assert_eq!(model.count(fresh_read), 0, "and nobody asked for it");
}

/// Two reads asked for while the question of what is installed is still out wait for its
/// one answer, rather than each asking it again.
#[test]
fn reads_waiting_for_what_is_installed_ask_it_once() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(Vec::new())));
    assert_eq!(
        model.send(Intent::Refresh { asked: false }),
        [Job::AskInstalled]
    );
    assert_eq!(model.send(Intent::Glanced), []);
    assert!(!model.shown().reading, "nothing is read before the answer");
    model.run(&mut machine);
    assert_eq!(model.count(installed_ask), 1);
    assert_eq!(model.count(any_read), 2);
}

/// What is already known fills in for a failed read only where nothing is shown yet: a read
/// that has landed meanwhile knows better, and is not put back to the last numbers measured.
#[test]
fn what_is_known_fills_in_only_where_nothing_is_shown_yet() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Err(refusal("unreachable", "unreachable", Vec::new())));
    machine.offline = Ok(status(vec![claude("work", true, 5.0)]));
    model.send(Intent::Refresh { asked: false });
    model.send(Intent::Refresh { asked: true });
    let ask = model.next();
    model.give(machine.answer(ask));
    let failed = machine.answer(model.next());
    machine.answer = Ok(status(vec![claude("work", true, 40.0)]));
    let answered = machine.answer(model.next());

    let fallback = model.give(failed);
    assert!(matches!(fallback[..], [Job::ReadOffline { .. }]));
    model.give(answered);
    assert_eq!(percent(&model.shown(), 0), Some(40.0));
    model.run(&mut machine);
    assert_eq!(percent(&model.shown(), 0), Some(40.0));
    assert!(!model.shown().reading);
}

/// Whether a switch is stuck is said as a read fails, so a read that lands while the failed
/// one falls back to what is known says its own and keeps it. The Swift said it once its
/// fallback was in, over what the read that landed meanwhile had said.
#[test]
fn a_read_landing_while_a_failed_one_falls_back_keeps_what_it_says_of_a_stuck_switch() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Err(refusal(
        "recovery_undetermined",
        "an interrupted switch cannot be finished",
        Vec::new(),
    )));
    machine.offline = Ok(status(vec![claude("work", true, 5.0)]));
    model.send(Intent::Refresh { asked: false });
    model.send(Intent::Refresh { asked: true });
    let ask = model.next();
    model.give(machine.answer(ask));
    let failed = machine.answer(model.next());
    machine.answer = Ok(status(vec![claude("work", true, 40.0)]));
    let answered = machine.answer(model.next());

    model.give(failed);
    assert!(model.shown().stuck);
    model.give(answered);
    model.run(&mut machine);
    let shown = model.shown();
    assert!(!shown.stuck, "the read that landed found nothing stuck");
    assert_eq!(shown.read_failure, None);
    assert_eq!(percent(&shown, 0), Some(40.0));
}

/// Numbers a session recorded go onto what is shown as they land. A read that lands while
/// they are being read is kept, with the newer numbers on it. The Swift put them onto what
/// was shown when the look found them, and so put back the accounts as they were before that
/// read.
#[test]
fn numbers_land_on_a_read_that_landed_while_they_were_read() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(vec![claude("work", true, 20.0)])));
    model.refresh(&mut machine);

    machine.readings += 1;
    model.look();
    model.send(Intent::Refresh { asked: false });
    let look = model.next();
    let found = machine.answer(look);
    assert!(matches!(model.give(found)[..], [Job::ReadOffline { .. }]));
    machine.answer = Ok(status(vec![
        claude("work", true, 20.0),
        claude("personal", false, 0.0),
    ]));
    machine.offline = Ok(status(vec![claude("work", true, 22.0)]));
    let read = machine.answer(model.next());
    model.give(read);
    model.run(&mut machine);

    let shown = model.shown();
    let labels: Vec<Option<&str>> = shown
        .status
        .as_ref()
        .expect("accounts")
        .accounts
        .iter()
        .map(|account| account.label.as_deref())
        .collect();
    assert_eq!(labels, [Some("work"), Some("personal")], "the read is kept");
    assert_eq!(
        percent(&shown, 0),
        Some(22.0),
        "with the newer numbers on it"
    );
}

/// A read whose job stopped with a panic is over: the button does not say a read is
/// running for ever, and the next read is asked for as any other.
#[test]
fn a_read_lost_to_a_panic_is_over() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(Vec::new())));
    model.send(Intent::Refresh { asked: false });
    let ask = model.next();
    model.give(machine.answer(ask));
    let lost = model.next();
    assert!(model.shown().reading);
    model.give(Answer::Lost(lost));
    assert!(!model.shown().reading);
    assert_eq!(model.pending(), 0);
    model.refresh(&mut machine);
    assert!(model.shown().status.is_some());
}

/// The poll counts a change it notices, so a read under way when it does is dropped; looks
/// that find nothing new count nothing.
#[test]
fn only_a_change_the_poll_notices_counts_as_one() {
    let mut model = Hand::new();
    let mut machine = Machine::reading(Ok(status(Vec::new())));
    model.notice(&mut machine);
    model.notice(&mut machine);
    machine.readings += 1;
    model.notice(&mut machine);
    assert_eq!(model.state.changes_seen(), 0);
    machine.changed += 1;
    model.notice(&mut machine);
    assert_eq!(model.state.changes_seen(), 1);
}
