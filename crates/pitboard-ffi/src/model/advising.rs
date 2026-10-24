//! Advice about which account to switch to, and telling each run-out once, driven through
//! `State::apply` by hand. Telling is a notification posted through the app's system, which a
//! test's machine keeps, and the record of what was told, which it keeps too.

use super::advice::{Advice, Told};
use super::switching::switch;
use super::testing::machine::{Hand, Stub};
use super::testing::records::{already_active, refusal, status, switched, titled, warned, warning};
use super::{AtLimit, Intent, NotificationButton, Pane, RunOutNotice, Sheet, WindowRequest};
use crate::Account;
use crate::present::testing::{LimitExt, account, window};

/// `work`, in use, its five-hour limit `percent` used until `resets`.
fn work(percent: f64, resets: i64) -> Account {
    account(Some("work"))
        .signed_in()
        .limits(vec![window("session", percent).resets(Some(resets))])
        .build()
}

/// `personal`, to switch to, with room.
fn personal() -> Account {
    account(Some("personal"))
        .limits(vec![window("session", 10.0).resets(Some(9_000))])
        .build()
}

fn switches_to(model: &Hand) -> Vec<String> {
    model
        .state
        .advice
        .iter()
        .map(|advice| advice.switch_to.clone())
        .collect()
}

fn told_work() -> String {
    Advice::key("claude", "work", &window("session", 100.0))
}

/// A session's status line records that the account in use has run out, and the menu bar
/// shows it within seconds. The advice to switch, and its notification, came only with the
/// app's next read of its own, minutes later.
#[test]
fn numbers_that_run_an_account_out_advise_at_once_and_tell_it_once() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(90.0, 7_200), personal()])));
    model.refresh(&mut machine);
    assert!(model.state.advice.is_empty());

    machine.offline = Ok(status(vec![work(100.0, 7_200), personal()]));
    machine.readings += 1;
    model.notice(&mut machine);
    assert_eq!(switches_to(&model), ["claude/personal"]);
    assert_eq!(model.state.told, Told::from([(told_work(), 7_200)]));
    assert_eq!(machine.posted.len(), 1);

    // The same window again, as another source rounds its reset. Numbers move with every
    // session's response, and advice put away seconds after it was told would be gone
    // before anybody opened the panel.
    machine.offline = Ok(status(vec![work(100.0, 7_201), personal()]));
    machine.readings += 1;
    model.notice(&mut machine);
    assert_eq!(
        switches_to(&model),
        ["claude/personal"],
        "still true, so still said"
    );
    assert_eq!(
        model.state.told,
        Told::from([(told_work(), 7_200)]),
        "and told once"
    );
    assert_eq!(machine.posted.len(), 1);
}

/// Advice says the account in use has none of a limit left. Once what is recorded shows the
/// window after it, with room, that is no longer true.
#[test]
fn advice_the_numbers_no_longer_bear_out_is_put_away() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(100.0, 7_200), personal()])));
    model.refresh(&mut machine);
    assert_eq!(model.state.advice.len(), 1);

    machine.offline = Ok(status(vec![work(3.0, 25_200), personal()]));
    machine.readings += 1;
    model.notice(&mut machine);
    assert!(model.state.advice.is_empty());
}

/// Advice told when what sessions recorded ran the account out stays while the numbers bear
/// it out, whatever read comes next. The app's own read worked advice out afresh, leaving out
/// what had been told, so opening the panel put it away with the account still at 100%.
#[test]
fn advice_told_from_what_sessions_recorded_outlasts_the_apps_next_read() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(90.0, 7_200), personal()])));
    model.refresh(&mut machine);
    machine.offline = Ok(status(vec![work(100.0, 7_200), personal()]));
    machine.readings += 1;
    model.notice(&mut machine);
    assert_eq!(switches_to(&model), ["claude/personal"]);

    machine.answer = Ok(status(vec![work(100.0, 7_200), personal()]));
    model.refresh(&mut machine);
    assert_eq!(model.shown().menu_bar.name_and_usage, "work 100%");
    assert_eq!(switches_to(&model), ["claude/personal"]);
}

/// The same for advice a read told: the next read found the window already told about and
/// left it out, so it was gone a minute later, or at once if the panel was opened again.
#[test]
fn advice_told_by_a_read_outlasts_the_next_read_and_is_told_once() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(100.0, 7_200), personal()])));
    model.refresh(&mut machine);
    assert_eq!(model.state.advice.len(), 1);

    model.refresh(&mut machine);
    assert_eq!(switches_to(&model), ["claude/personal"]);
    assert_eq!(model.state.told, Told::from([(told_work(), 7_200)]));
    assert_eq!(machine.posted.len(), 1);
    assert_eq!(
        machine.kept,
        [Told::from([(told_work(), 7_200)])],
        "kept once"
    );
}

/// A change to the account index can leave the account in use run out, and the poll that
/// notices it reads who is signed in and the numbers again. What they show is advised then,
/// not at the app's next read of its own.
#[test]
fn the_account_index_changing_elsewhere_is_advised_on_at_once() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(20.0, 7_200), personal()])));
    model.refresh(&mut machine);
    assert!(model.state.advice.is_empty());

    machine.offline = Ok(status(vec![work(100.0, 7_200), personal()]));
    machine.changed += 1;
    model.notice(&mut machine);
    assert_eq!(switches_to(&model), ["claude/personal"]);
}

/// Advice offers the account of the same tool with the most left, and stays while the
/// account in use is still out. What it offers is worked out again from every read: the
/// account offered may have been forgotten, may need signing in again or may have run out
/// itself since, and choosing it then failed. What it says is left follows the numbers, and
/// with nothing left to offer the advice goes.
#[test]
fn advice_offers_what_can_still_be_used_after_every_read() {
    let spent = || work(100.0, 7_200);
    let other = |label: &str, percent: f64, switchable: bool| {
        account(Some(label))
            .switchable(switchable)
            .limits(vec![window("session", percent).resets(Some(9_000))])
            .build()
    };
    fn offered(model: &Hand) -> Vec<(&str, Option<i64>)> {
        model
            .state
            .advice
            .iter()
            .map(|advice| (advice.switch_to.as_str(), advice.left))
            .collect()
    }
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
        spent(),
        other("personal", 10.0, true),
        other("side", 40.0, true),
        other("extra", 50.0, true),
    ])));
    model.refresh(&mut machine);
    assert_eq!(offered(&model), [("claude/personal", Some(90))]);

    machine.answer = Ok(status(vec![
        spent(),
        other("personal", 30.0, true),
        other("side", 40.0, true),
        other("extra", 50.0, true),
    ]));
    model.refresh(&mut machine);
    assert_eq!(
        offered(&model),
        [("claude/personal", Some(70))],
        "what is left follows the numbers"
    );

    machine.answer = Ok(status(vec![
        spent(),
        other("side", 40.0, true),
        other("extra", 50.0, true),
    ]));
    model.refresh(&mut machine);
    assert_eq!(
        offered(&model),
        [("claude/side", Some(60))],
        "personal was forgotten"
    );

    machine.answer = Ok(status(vec![
        spent(),
        other("side", 40.0, false),
        other("extra", 50.0, true),
    ]));
    model.refresh(&mut machine);
    assert_eq!(
        offered(&model),
        [("claude/extra", Some(50))],
        "side needs signing in again"
    );

    machine.answer = Ok(status(vec![
        spent(),
        other("side", 40.0, false),
        other("extra", 100.0, true),
    ]));
    model.refresh(&mut machine);
    assert!(model.state.advice.is_empty(), "extra has run out too");
    let posted: Vec<&str> = machine
        .posted
        .iter()
        .map(|notice| notice.id.as_str())
        .collect();
    assert_eq!(
        posted,
        ["claude/work/session/-7200", "unusable/claude:side"],
        "told once, when work ran out, and once that side needs signing in again"
    );
}

/// A switch puts away the advice about its own tool, which is about the account it has just
/// left, and leaves another tool's: as true as it was, and never told again.
///
/// What its notices say is presenting's.
#[test]
fn a_switch_puts_away_the_advice_about_its_own_tool() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
        work(100.0, 7_200),
        personal(),
        account(Some("side"))
            .of("codex")
            .signed_in()
            .limits(vec![window("five_hour", 100.0).resets(Some(7_200))])
            .build(),
        account(Some("job"))
            .of("codex")
            .limits(vec![window("five_hour", 10.0).resets(Some(7_200))])
            .build(),
    ])));
    model.refresh(&mut machine);
    assert_eq!(switches_to(&model), ["claude/personal", "codex/job"]);

    // The read after the switch fails, so what is said is what the switch left.
    machine.answer = Err(refusal("unreachable", "could not be reached", Vec::new()));
    machine.switched = switched("codex", "codex/side", "codex/job", Vec::new());
    switch(&mut model, &mut machine, "codex/job");
    assert_eq!(switches_to(&model), ["claude/personal"]);
}

/// The notification that an account ran out says what the window's notice says, in the
/// same words, and switches to the account offered: worked once for both, where the Swift
/// worded the panel's, the notification's and a sentence only its tests spoke each their own
/// way. Beside another tool's accounts it names the tool beneath its title.
#[test]
fn a_run_out_is_notified_in_the_words_the_window_says_it() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(100.0, 7_200), personal()])));
    model.refresh(&mut machine);
    assert_eq!(
        machine.posted,
        [RunOutNotice {
            id: "claude/work/session/-7200".into(),
            title: "work has no 5-hour limit left".into(),
            subtitle: None,
            body: "personal has 90% of its own left.".into(),
            clicked: "accounts".into(),
            button: Some(NotificationButton {
                title: "Switch".into(),
                clicked: "switch/claude/personal".into(),
            }),
        }]
    );
    let notice = model.shown().notices.remove(0);
    assert_eq!(notice.title, machine.posted[0].title);
    assert_eq!(notice.lines, [machine.posted[0].body.clone()]);

    let mut both = Hand::new();
    let mut beside = Stub::reading(Ok(status(vec![
        work(100.0, 7_200),
        personal(),
        account(Some("job")).of("codex").signed_in().build(),
    ])));
    both.refresh(&mut beside);
    assert_eq!(beside.posted[0].subtitle.as_deref(), Some("Claude Code"));
    assert_eq!(beside.posted[0].title, "work has no 5-hour limit left");
    assert_eq!(
        both.shown().notices[0].title,
        "Claude Code: work has no 5-hour limit left"
    );
}

// What the Swift model did not do.

/// A run-out notified before the app was quit is not notified again once it is opened, at
/// the same reset, by the core's rule for one: the record of what was told is kept in
/// Pitboard's directory, the owner's decision. The window still says it, as the Swift did
/// after a relaunch, and a reset after it is told. The Swift kept the record in memory, and
/// told the same run-out again after every relaunch.
///
/// Reaches people in PR 10.
#[test]
fn a_run_out_notified_before_a_relaunch_is_not_notified_again() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(100.0, 7_201), personal()])));
    machine.told_before = Told::from([(told_work(), 7_200)]);
    model.send(Intent::Start);
    model.run(&mut machine);
    assert!(machine.posted.is_empty(), "notified before the relaunch");
    assert!(machine.kept.is_empty(), "nothing new to keep");
    assert_eq!(
        switches_to(&model),
        ["claude/personal"],
        "the window says it"
    );
    assert_eq!(model.shown().notices[0].id, "advice/claude/work/session/");

    machine.answer = Ok(status(vec![work(100.0, 25_200), personal()]));
    model.refresh(&mut machine);
    assert_eq!(
        machine.posted.len(),
        1,
        "a reset after it is a run-out of its own"
    );
    assert_eq!(
        machine.kept,
        [Told::from([(told_work(), 25_200)])],
        "and what was told of it is kept"
    );
}

/// What was told before the model started is in before anything is advised on, so a read
/// that lands first is never notified because the record came second: advice waits for the
/// record, and is worked out from what was read once it is in.
#[test]
fn nothing_is_told_before_what_was_told_is_in() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(100.0, 7_200), personal()])));
    machine.told_before = Told::from([(told_work(), 7_200)]);
    model.send(Intent::Start);
    model.run_but(&mut machine, |job| {
        matches!(job, super::state::Job::LoadKept)
    });
    assert_eq!(
        model.count(super::testing::machine::any_read),
        1,
        "the read goes ahead"
    );
    assert!(model.state.advice.is_empty(), "and is not advised on yet");
    model.run(&mut machine);
    assert_eq!(
        switches_to(&model),
        ["claude/personal"],
        "once the record is in"
    );
    assert!(machine.posted.is_empty(), "notified before");
}

/// A record of what was told that came to nothing is taken as nothing told, and what was
/// read meanwhile is advised on.
#[test]
fn a_record_that_came_to_nothing_is_nothing_told() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(100.0, 7_200), personal()])));
    model.send(Intent::Start);
    let kept = model.take(|job| matches!(job, super::state::Job::LoadKept));
    model.run(&mut machine);
    assert!(machine.posted.is_empty());
    model.give(super::state::Answer::Lost(kept));
    assert_eq!(switches_to(&model), ["claude/personal"]);
    model.run(&mut machine);
    assert_eq!(machine.posted.len(), 1);
}

/// What stands in for a read that failed before anything was shown, the last numbers
/// measured, is never advised on, as AppModel.swift's fallback was not: an app opened on a
/// machine that cannot reach a service does not notify a run-out from numbers no read has
/// borne out, even once the record of what was told is in. Newer numbers recorded since are
/// advised on, as the Swift advised on them.
#[test]
fn what_stands_in_for_a_failed_read_is_not_advised_on() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Err(refusal(
        "unreachable",
        "could not be reached",
        Vec::new(),
    )));
    machine.offline = Ok(status(vec![work(100.0, 7_200), personal()]));
    model.send(Intent::Start);
    let kept = model.take(|job| matches!(job, super::state::Job::LoadKept));
    model.run(&mut machine);
    assert!(
        model.state.status.is_some(),
        "the last numbers measured are shown"
    );
    let answer = machine.answer(kept);
    model.give(answer);
    model.run(&mut machine);
    assert!(model.state.advice.is_empty(), "nothing advised");
    assert!(machine.posted.is_empty(), "nothing notified");

    machine.readings += 1;
    model.notice(&mut machine);
    assert_eq!(
        switches_to(&model),
        ["claude/personal"],
        "numbers recorded since"
    );
    assert_eq!(machine.posted.len(), 1);
}

/// Advice goes once the account in use is back under the limit, at the same reset too: the
/// service's own answer can say less than a session recorded.
#[test]
fn advice_goes_once_the_account_is_back_under_its_limit() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(100.0, 7_200), personal()])));
    model.refresh(&mut machine);
    assert_eq!(model.state.advice.len(), 1);
    machine.answer = Ok(status(vec![work(97.0, 7_200), personal()]));
    model.refresh(&mut machine);
    assert!(model.state.advice.is_empty());
}

/// A change noticed elsewhere while what was told is still being read is shown, and advised
/// on once the record is in: a run-out the record has is not told again because the look
/// came first.
#[test]
fn a_change_noticed_before_what_was_told_is_in_is_advised_on_after() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(100.0, 7_200), personal()])));
    machine.told_before = Told::from([(told_work(), 7_200)]);
    model.send(Intent::Start);
    let kept = model.take(|job| matches!(job, super::state::Job::LoadKept));
    model.run(&mut machine);
    machine.offline = Ok(status(vec![work(100.0, 7_200), personal()]));
    machine.changed += 1;
    model.notice(&mut machine);
    assert!(model.state.status.is_some(), "the change is shown");
    assert!(model.state.advice.is_empty(), "and not advised on yet");

    let answer = machine.answer(kept);
    model.give(answer);
    model.run(&mut machine);
    assert_eq!(switches_to(&model), ["claude/personal"]);
    assert!(machine.posted.is_empty(), "told before");
}

/// A switch to the account already in use, as a notification pressed after a switch made
/// elsewhere asks for, moves nothing, so advice that it has run out stands until a read no
/// longer bears it out: it is still in use and still out, and the advice is not told again.
#[test]
fn a_switch_to_the_account_in_use_keeps_the_advice_about_it() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(100.0, 7_200), personal()])));
    model.refresh(&mut machine);
    assert_eq!(switches_to(&model), ["claude/personal"]);

    machine.switched = already_active("work", Vec::new());
    switch(&mut model, &mut machine, "claude/work");
    assert_eq!(switches_to(&model), ["claude/personal"]);
}

/// Writing the account in use into Claude Code's config moves nothing either, so advice that
/// it has run out stands. The account in use can be out of a limit while the config names
/// another account, as on the machine of 8 October, and advice put away then is not given
/// again until that limit resets.
#[test]
fn updating_the_config_keeps_the_advice_about_the_account_in_use() {
    let names_another = warning(
        "config_names_another",
        "Claude Code’s config names `personal`, and the login Claude Code has stored is \
         `work`’s.",
    );
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(warned(
        vec![work(100.0, 7_200), personal()],
        vec![names_another],
    )));
    model.refresh(&mut machine);
    let update = model
        .shown()
        .notices
        .into_iter()
        .flat_map(|notice| notice.actions)
        .find(|action| {
            action.title == titled("Update Claude Code’s Config", "Update Claude Code’s config")
        })
        .expect("offered");

    machine.answer = Ok(status(vec![work(100.0, 7_200), personal()]));
    model.send(update.intent);
    model.run(&mut machine);
    assert_eq!(machine.configs_updated, ["claude/work"]);
    assert_eq!(switches_to(&model), ["claude/personal"]);
    assert_eq!(machine.posted.len(), 1, "told once");
}

// What a click on a notification asks for.

/// A run-out's Switch button switches as the window's notice does, and a click on the
/// notification itself opens the accounts, where that notice is.
#[test]
fn a_run_outs_button_switches_and_a_click_on_it_opens_the_accounts() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(100.0, 7_200), personal()])));
    model.refresh(&mut machine);
    let posted = machine.posted[0].clone();

    model.send(Intent::NotificationClicked {
        argument: posted.clicked,
    });
    assert_eq!(
        model.shown().window_request,
        WindowRequest {
            serial: 1,
            pane: Some(Pane::Accounts),
            settings: false,
        }
    );

    let button = posted.button.expect("a Switch button");
    assert!(
        crate::notification_buttons()
            .iter()
            .any(|kind| kind.title == button.title && !kind.opens_window),
        "a switch needs no window"
    );
    machine.switched = switched("claude", "work", "personal", Vec::new());
    model.send(Intent::NotificationClicked {
        argument: button.clicked,
    });
    model.run(&mut machine);
    assert_eq!(machine.switched_to, ["claude/personal"]);
}

/// A login replaced outside Pitboard is signed in to again from its notification's button,
/// which puts that account's sheet up over the accounts.
#[test]
fn a_sign_in_again_button_puts_up_the_accounts_sheet() {
    model_clicked("sign-in-again/codex/work", |model| {
        assert_eq!(
            model.state.sheet,
            Some(Sheet::SignInAgain {
                provider: "codex".into(),
                label: "work".into(),
            })
        );
        assert_eq!(model.shown().window_request.pane, Some(Pane::Accounts));
    });
}

/// What switching or warming up by itself came to is said under its setting, so a click on
/// its notification opens the settings there.
#[test]
fn a_click_on_what_came_of_switching_or_warming_up_by_itself_opens_the_limits() {
    for notice in [
        crate::present::auto_refused_notice("refused", "It was refused."),
        crate::present::warm_failed_notice("warm_up_not_started", "It came to nothing."),
    ] {
        model_clicked(&notice.clicked, |model| {
            assert_eq!(
                model.shown().window_request,
                WindowRequest {
                    serial: 1,
                    pane: None,
                    settings: true,
                }
            );
        });
    }
}

/// Anything that starts the app can hand it any text: what no notification asks for opens
/// the accounts, and does nothing else.
#[test]
fn a_click_that_asks_for_nothing_a_notification_asks_for_opens_the_accounts() {
    for argument in ["", "accounts", "forget/claude/work", "switch/", "settings"] {
        let mut model = Hand::new();
        let jobs = model.send(Intent::NotificationClicked {
            argument: argument.into(),
        });
        assert!(jobs.is_empty(), "{argument}");
        assert_eq!(
            model.shown().window_request,
            WindowRequest {
                serial: 1,
                pane: Some(Pane::Accounts),
                settings: false,
            },
            "{argument}"
        );
    }
}

fn model_clicked(argument: &str, then: impl FnOnce(&Hand)) {
    let mut model = Hand::new();
    model.send(Intent::NotificationClicked {
        argument: argument.into(),
    });
    then(&model);
}

// What happens once a limit reaches the share chosen.

/// The app's preferences read from their file, with `choice` made at the default share.
fn chose(model: &mut Hand, machine: &mut Stub, choice: AtLimit) {
    let mut preferences = super::preferences::Preferences::default();
    preferences.choose(choice);
    model.give(super::state::Answer::Kept {
        told: Told::new(),
        preferences: Some((preferences, true)),
        windows: None,
    });
    model.run(machine);
}

/// A fresh install notifies once a limit of the account in use reaches 95%, before it runs
/// out, with the account to switch to, and once only for that limit and reset, whatever it
/// reaches after.
#[test]
fn a_limit_at_the_share_is_notified_once_before_it_runs_out() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(94.0, 7_200), personal()])));
    model.refresh(&mut machine);
    assert!(machine.posted.is_empty(), "below the share");

    machine.answer = Ok(status(vec![work(96.0, 7_200), personal()]));
    model.refresh(&mut machine);
    assert_eq!(
        machine.posted,
        [RunOutNotice {
            id: "claude/work/session/-7200".into(),
            title: "work has used 96% of its 5-hour limit".into(),
            subtitle: None,
            body: "personal has 90% of its own left.".into(),
            clicked: "accounts".into(),
            button: Some(NotificationButton {
                title: "Switch".into(),
                clicked: "switch/claude/personal".into(),
            }),
        }]
    );
    assert!(
        model.state.advice.is_empty(),
        "the window offers it at 100%"
    );

    machine.answer = Ok(status(vec![work(100.0, 7_200), personal()]));
    model.refresh(&mut machine);
    assert_eq!(machine.posted.len(), 1, "one limit, one notification");
    assert_eq!(switches_to(&model), ["claude/personal"]);
}

// Back as the core judges it, before a reading says so.
#[test]
fn a_limit_past_its_reset_is_neither_notified_nor_advised_on() {
    for percent in [96.0, 100.0] {
        let mut model = Hand::new();
        let mut read = status(vec![work(percent, 1_800_000_000), personal()]);
        read.now = 1_800_000_000;
        let mut machine = Stub::reading(Ok(read));
        model.refresh(&mut machine);
        assert!(machine.posted.is_empty(), "{percent}");
        assert!(switches_to(&model).is_empty(), "{percent}");
    }
}

/// With no other account to switch to, it says when the limit resets.
#[test]
fn a_limit_at_the_share_with_nowhere_to_go_says_when_it_resets() {
    let mut model = Hand::new();
    let mut read = status(vec![work(97.0, 1_800_003_900)]);
    read.now = 1_800_000_000;
    let mut machine = Stub::reading(Ok(read));
    model.refresh(&mut machine);
    let notice = machine.posted.first().expect("posted");
    assert_eq!(notice.title, "work has used 97% of its 5-hour limit");
    assert_eq!(notice.body, "Resets in 1h 05m.");
    assert_eq!(notice.button, None);
}

/// An account past the share has no room by the rule the automatic switch keeps, though it is
/// under 100%.
#[test]
fn an_account_past_the_share_is_not_offered_at_the_share() {
    let mut model = Hand::new();
    let past = account(Some("personal"))
        .limits(vec![window("session", 98.0).resets(Some(1_800_009_000))])
        .build();
    let mut read = status(vec![work(96.0, 1_800_003_900), past]);
    read.now = 1_800_000_000;
    let mut machine = Stub::reading(Ok(read));
    model.refresh(&mut machine);
    let notice = machine.posted.first().expect("posted");
    assert_eq!(notice.body, "Resets in 1h 05m.");
    assert_eq!(notice.button, None);
}

/// Do Nothing posts nothing, and the window still offers a switch once the account runs out.
#[test]
fn do_nothing_notifies_nothing() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(100.0, 7_200), personal()])));
    chose(&mut model, &mut machine, AtLimit::Nothing);
    model.refresh(&mut machine);
    assert!(machine.posted.is_empty());
    assert_eq!(switches_to(&model), ["claude/personal"]);
}

/// Under Switch, Claude Code's own notices say what switching came to, and Codex, which is
/// never switched by itself, is notified as under Notify.
#[test]
fn switch_notifies_of_codex_alone() {
    let codex = |label: &str, percent: f64, signed_in: bool| {
        let made = account(Some(label)).of("codex").limits(vec![
            window("five_hour", percent)
                .length(18_000)
                .resets(Some(7_200)),
        ]);
        if signed_in { made.signed_in() } else { made }.build()
    };
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
        work(97.0, 7_200),
        personal(),
        codex("job", 97.0, true),
        codex("side", 10.0, false),
    ])));
    machine.look = Ok(crate::AutoLooked::Stands(
        super::testing::records::watching("work"),
    ));
    chose(&mut model, &mut machine, AtLimit::Switch);
    model.refresh(&mut machine);
    let posted: Vec<(&str, Option<&str>)> = machine
        .posted
        .iter()
        .map(|notice| (notice.title.as_str(), notice.subtitle.as_deref()))
        .collect();
    assert_eq!(
        posted,
        [("job has used 97% of its 5-hour limit", Some("Codex"))]
    );
}

/// The settings offer the three choices in the system's own case, and the sentence on the
/// terms beside switching while it is chosen, and beside warm-up always.
#[test]
fn the_settings_offer_three_choices_and_the_terms_beside_switching() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(10.0, 7_200), personal()])));
    chose(&mut model, &mut machine, AtLimit::Notify);
    let shown = model.on_a_mac().machine;
    let titles: Vec<String> = shown
        .at_limit
        .choices
        .iter()
        .map(|choice| choice.title.clone())
        .collect();
    assert_eq!(titles, ["Do Nothing", "Notify Me", "Switch Automatically"]);
    assert_eq!(
        model.on_windows().machine.at_limit.choices[1].title,
        "Notify me"
    );
    assert_eq!(shown.at_limit.title, "Before an account runs out");
    assert_eq!(shown.warm.title, "Warm up limits");
    // The field beside the heading shows the share, and the choice is named with it.
    assert_eq!(shown.at_limit.heading, "When a limit reaches");
    assert_eq!(shown.at_limit.at_label, "When a limit reaches 95%");
    assert_eq!(
        shown.at_limit.note.as_deref(),
        Some(
            "Pitboard notifies you once a limit of the account in use reaches 95%, while the app is open."
        )
    );
    assert!(shown.at_limit.share_enabled);
    assert_eq!(shown.at_limit.terms, None);
    assert_eq!(
        shown.warm.terms.link,
        "https://docs.usepitboard.com/concepts/limits#is-this-allowed"
    );

    model.send(Intent::SetAtLimit {
        choice: AtLimit::Switch,
        at: 95,
    });
    let terms = model
        .shown()
        .machine
        .at_limit
        .terms
        .expect("beside switching");
    assert_eq!(
        terms.sentence,
        "Anthropic's terms may cover automatic switching and warm-ups."
    );
    assert_eq!(terms.link_title, "Is this allowed?");

    model.send(Intent::SetAtLimit {
        choice: AtLimit::Nothing,
        at: 95,
    });
    let shown = model.shown().machine.at_limit;
    assert!(!shown.share_enabled, "nothing happens at the share");
    assert_eq!((shown.note, shown.terms), (None, None));
}
