//! Switching Claude Code by itself, where somebody turned it on in the settings: when the
//! model asks the core to, what it says of what came of it, and that nothing else switches
//! meanwhile. Driven through `State::apply` by hand; which account the core switches to, and
//! whether, is the core's own and has its own tests.

use super::advice::Told;
use super::preferences::Preferences;
use super::state::{Answer, Job};
use super::testing::{Hand, Machine, refusal, status, switched};
use super::{Intent, RunOutNotice};
use crate::present::testing::{LimitExt, account, window};
use crate::{Account, AutoSwitched};

/// `work`, in use, its five-hour limit `percent` used.
fn work(percent: f64) -> Account {
    account(Some("work"))
        .signed_in()
        .limits(vec![window("session", percent).resets(Some(9_000))])
        .build()
}

/// `personal`, with room.
fn personal() -> Account {
    account(Some("personal"))
        .limits(vec![window("session", 10.0).resets(Some(9_000))])
        .build()
}

/// The app's preferences as read from their file: switching by itself on or off, at the
/// default share.
fn read_preferences(model: &mut Hand, on: bool) {
    let preferences = Preferences {
        auto_switch: on,
        ..Preferences::default()
    };
    model.give(Answer::Kept {
        told: Told::new(),
        preferences: Some((preferences, true)),
        windows: None,
    });
}

fn auto_switch(job: &Job) -> bool {
    matches!(job, Job::AutoSwitch { .. })
}

fn switched_work_to_personal() -> Result<AutoSwitched, super::testing::Refusal> {
    Ok(AutoSwitched::Switched {
        switched: switched("claude", "work", "personal", Vec::new()).expect("a switch"),
        used: "97% of its 5-hour limit".into(),
    })
}

#[test]
fn nothing_is_switched_by_itself_unless_somebody_turned_it_on() {
    let mut model = Hand::new();
    read_preferences(&mut model, false);
    let mut machine = Machine::reading(Ok(status(vec![work(97.0), personal()])));
    model.refresh(&mut machine);
    assert_eq!(model.count(auto_switch), 0);
    assert!(!model.shown().machine.auto_switch.on);
}

#[test]
fn a_limit_at_the_share_asks_the_core_to_switch_and_nothing_else_switches_meanwhile() {
    let mut model = Hand::new();
    read_preferences(&mut model, true);
    let mut machine = Machine::reading(Ok(status(vec![work(94.0), personal()])));
    model.refresh(&mut machine);
    assert_eq!(model.count(auto_switch), 0, "below the share");

    machine.answer = Ok(status(vec![work(95.0), personal()]));
    model.send(Intent::Refresh { asked: false });
    model.run_but(&mut machine, auto_switch);
    assert_eq!(model.count(auto_switch), 1);
    assert!(
        model.state.switch_under_way().is_some(),
        "claimed, so a switch asked for meanwhile waits and the poll takes the change for this app's own"
    );
    assert_eq!(
        model.send(Intent::SwitchTo {
            qualified: "claude/personal".into()
        }),
        [],
        "one switch at a time"
    );
    model.run(&mut machine);
    assert_eq!(machine.auto_at, [95]);
    assert!(
        model.state.switch_under_way().is_none(),
        "nothing to do lets go"
    );
    assert!(machine.posted.is_empty(), "and says nothing");
}

/// A limit the app draws at 95% is at the share of 95%, as the core judges it.
#[test]
fn a_limit_shown_at_the_share_asks_the_core_to_switch() {
    let mut model = Hand::new();
    read_preferences(&mut model, true);
    let mut machine = Machine::reading(Ok(status(vec![work(94.4), personal()])));
    model.refresh(&mut machine);
    assert_eq!(model.count(auto_switch), 0, "drawn at 94%");

    machine.answer = Ok(status(vec![work(94.5), personal()]));
    model.send(Intent::Refresh { asked: false });
    model.run(&mut machine);
    assert_eq!(machine.auto_at, [95]);
}

/// A Codex account at its limit is never switched by itself: a running `codex` never follows
/// a switch. It is advised on as it always was.
#[test]
fn codex_is_never_switched_by_itself() {
    let mut model = Hand::new();
    read_preferences(&mut model, true);
    let codex = |label: &str, signed_in: bool, percent: f64| {
        let made = account(Some(label))
            .of("codex")
            .limits(vec![window("five_hour", percent).length(18_000)]);
        if signed_in { made.signed_in() } else { made }.build()
    };
    let mut machine = Machine::reading(Ok(status(vec![
        codex("job", true, 100.0),
        codex("spare", false, 0.0),
    ])));
    model.refresh(&mut machine);
    assert_eq!(model.count(auto_switch), 0);
}

#[test]
fn a_switch_it_made_is_said_and_taken_as_a_switch_somebody_asked_for() {
    let mut model = Hand::new();
    read_preferences(&mut model, true);
    let mut machine = Machine::reading(Ok(status(vec![work(97.0), personal()])));
    machine.auto = switched_work_to_personal();
    model.send(Intent::Refresh { asked: false });
    model.run_but(&mut machine, auto_switch);
    machine.answer = Ok(status(vec![
        account(Some("work"))
            .limits(vec![window("session", 97.0).resets(Some(9_000))])
            .build(),
        account(Some("personal"))
            .signed_in()
            .limits(vec![window("session", 10.0).resets(Some(9_000))])
            .build(),
    ]));
    model.run(&mut machine);

    assert_eq!(
        machine.posted,
        [RunOutNotice {
            id: format!("auto/work/personal/{}", model.now.epoch()),
            title: "Switched Claude Code to personal".into(),
            subtitle: None,
            body: "work had used 97% of its 5-hour limit. Sessions already running follow \
                   within 33 seconds."
                .into(),
            switch_to: None,
        }]
    );
    let last = &model.shown().last_switches;
    assert_eq!(
        last.iter()
            .map(|s| (s.provider.as_str(), s.to.as_str()))
            .collect::<Vec<_>>(),
        [("claude", "personal")],
        "the window says it as it says a switch somebody asked for"
    );
    assert!(
        model.state.switch_under_way().is_none(),
        "over once read after"
    );
    assert_eq!(
        machine.auto_at.len(),
        1,
        "and the account switched to has room"
    );
}

/// A reason it did not switch is said once, not at every look while the numbers stand.
#[test]
fn a_reason_it_did_not_switch_is_said_once() {
    let mut model = Hand::new();
    read_preferences(&mut model, true);
    let mut machine = Machine::reading(Ok(status(vec![work(97.0), personal()])));
    machine.auto = Ok(AutoSwitched::Skipped {
        from: "work".into(),
        used: "97% of its 5-hour limit".into(),
        code: "auth_overridden".into(),
        why: "Claude Code signs in another way, set by apiKeyHelper, so a switch would change \
              nothing its sessions use"
            .into(),
    });
    model.refresh(&mut machine);
    model.refresh(&mut machine);
    assert_eq!(machine.auto_at.len(), 2);
    assert_eq!(machine.posted.len(), 1, "{:?}", machine.posted);
    assert_eq!(machine.posted[0].title, "Claude Code was not switched");
    assert_eq!(
        machine.posted[0].body,
        "work has used 97% of its 5-hour limit. Claude Code signs in another way, set by \
         apiKeyHelper, so a switch would change nothing its sessions use."
    );
    assert_eq!(machine.posted[0].switch_to, None);
}

/// A refusal is said once too, in the core's words, and leaves nothing claimed.
#[test]
fn a_refusal_is_said_once_and_lets_go() {
    let mut model = Hand::new();
    read_preferences(&mut model, true);
    let mut machine = Machine::reading(Ok(status(vec![work(97.0), personal()])));
    machine.auto = Err(refusal(
        "identity_unverifiable",
        "Pitboard could not confirm with Anthropic which account is signed in.",
        Vec::new(),
    ));
    model.refresh(&mut machine);
    model.refresh(&mut machine);
    assert_eq!(machine.posted.len(), 1);
    assert_eq!(
        machine.posted[0].title,
        "Pitboard could not switch Claude Code"
    );
    assert!(model.state.switch_under_way().is_none());
    assert!(model.shown().failure.is_none(), "nobody asked, so no alert");
}

/// After a refusal it waits for the app's next read before it asks again, rather than at
/// every number a session records: each refusal is a line in the activity log.
#[test]
fn after_a_refusal_it_waits_for_the_next_read() {
    let mut model = Hand::new();
    read_preferences(&mut model, true);
    let mut machine = Machine::reading(Ok(status(vec![work(97.0), personal()])));
    machine.auto = Err(refusal(
        "home_unwritable",
        "Pitboard's folder cannot be written.",
        Vec::new(),
    ));
    model.refresh(&mut machine);
    assert_eq!(machine.auto_at.len(), 1);

    machine.offline = Ok(status(vec![work(98.0), personal()]));
    machine.readings += 1;
    model.notice(&mut machine);
    assert_eq!(
        machine.auto_at.len(),
        1,
        "not at numbers a session recorded"
    );

    model.refresh(&mut machine);
    assert_eq!(machine.auto_at.len(), 2, "at the next read");
}

#[test]
fn no_account_with_room_says_nothing_beyond_what_a_run_out_says() {
    let mut model = Hand::new();
    read_preferences(&mut model, true);
    let mut machine = Machine::reading(Ok(status(vec![work(97.0), personal()])));
    machine.auto = Ok(AutoSwitched::NoRoom);
    model.refresh(&mut machine);
    assert_eq!(machine.auto_at.len(), 1);
    assert!(machine.posted.is_empty());
    assert!(model.state.switch_under_way().is_none());
}

/// Not while a switch somebody asked for is under way: theirs is the one to make.
#[test]
fn not_while_another_switch_is_under_way() {
    let mut model = Hand::new();
    read_preferences(&mut model, true);
    let mut machine = Machine::reading(Ok(status(vec![work(90.0), personal()])));
    model.refresh(&mut machine);
    model.send(Intent::SwitchTo {
        qualified: "claude/personal".into(),
    });
    machine.answer = Ok(status(vec![work(97.0), personal()]));
    model.send(Intent::Refresh { asked: false });
    model.run_but(&mut machine, |job| {
        matches!(job, Job::Holding { .. } | Job::Switch { .. })
    });
    assert_eq!(model.count(auto_switch), 0);
}

/// Nor while another change this app made is under way, such as a renewal waiting on the
/// network: the switch would wait behind it on the lane of changes, and hold every switch
/// somebody asks for meanwhile. It is asked for at the next look once that change is over.
#[test]
fn not_while_another_change_is_under_way() {
    let mut model = Hand::new();
    read_preferences(&mut model, true);
    let mut machine = Machine::reading(Ok(status(vec![work(90.0), personal()])));
    model.refresh(&mut machine);
    model.send(Intent::RenewNow);
    machine.answer = Ok(status(vec![work(97.0), personal()]));
    model.send(Intent::Refresh { asked: false });
    model.run_but(&mut machine, |job| matches!(job, Job::Renew));
    assert_eq!(model.count(auto_switch), 0);

    model.run(&mut machine);
    assert_eq!(
        model.count(auto_switch),
        1,
        "once the renewal and the read after it are over"
    );
}

/// The settings' switch is kept in the preferences, and turned on, looks at once at what was
/// read last. Before the preferences are read it is not taken: what they say would win.
#[test]
fn turning_it_on_is_kept_and_looks_at_once() {
    let mut model = Hand::new();
    assert_eq!(
        model.send(Intent::SetAutoSwitch { on: true, at: 90 }),
        [],
        "not before the preferences are read"
    );
    assert!(!model.shown().machine.auto_switch.enabled);

    read_preferences(&mut model, false);
    let mut machine = Machine::reading(Ok(status(vec![work(92.0), personal()])));
    model.refresh(&mut machine);
    assert_eq!(model.count(auto_switch), 0);

    let jobs = model.send(Intent::SetAutoSwitch { on: true, at: 90 });
    assert_eq!(
        jobs,
        [
            Job::KeepPreferences {
                preferences: Preferences {
                    auto_switch: true,
                    auto_switch_at: Some(90),
                    has_been_seen: true,
                    ..Preferences::default()
                }
            },
            Job::AutoSwitch { at: 90 },
        ]
    );
    let shown = model.shown().machine.auto_switch;
    assert!(shown.on && shown.enabled);
    assert_eq!((shown.at, shown.lowest, shown.highest), (90, 50, 99));
    assert_eq!(shown.at_label, "Switch when a limit reaches 90%");

    model.run(&mut machine);
    assert_eq!(
        model.send(Intent::SetAutoSwitch { on: true, at: 90 }),
        [],
        "what it already is"
    );
    let jobs = model.send(Intent::SetAutoSwitch { on: false, at: 120 });
    assert_eq!(jobs.len(), 1);
    assert_eq!(
        model.state.preferences.auto_switch_at,
        Some(99),
        "the nearest share there can be"
    );
}
