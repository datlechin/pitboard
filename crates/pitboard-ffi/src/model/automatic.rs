//! Switching Claude Code by itself, where somebody turned it on in the settings: when the
//! model asks the core's look and when its decision, what it says of what came of them, and
//! that nothing else switches meanwhile. Driven through `State::apply` by hand; what the look
//! and the decision come to is the core's own and has its own tests.

use super::advice::Told;
use super::preferences::Preferences;
use super::state::{Answer, Job};
use super::testing::machine::{Hand, Stub, any_read, offline_read};
use super::testing::records::{Refusal, refusal, status, switched, watching};
use super::{AtLimit, Intent, NotificationButton, Place, RunOutNotice};
use crate::present::testing::{LimitExt, account, window};
use crate::present::words;
use crate::{Account, Adoption, AutoLooked, AutoSwitched, Status};
use pitboard_core::host::{OS, Os};
use pitboard_core::words as said;
use std::time::Duration;

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

/// The app's preferences read with switching by itself on, and what they lead to answered as
/// `machine` stands. Nothing is looked at before the first read.
fn turned_on(model: &mut Hand, machine: &mut Stub) {
    read_preferences(model, true);
    model.run(machine);
}

fn auto_switch(job: &Job) -> bool {
    matches!(job, Job::AutoSwitch { .. })
}

fn auto_look(job: &Job) -> bool {
    matches!(job, Job::AutoLook { .. })
}

fn switched_work_to_personal() -> Result<AutoSwitched, Refusal> {
    Ok(AutoSwitched::Switched {
        from: "work".into(),
        to: "personal".into(),
        adoption: Adoption::PollingWithin(33),
        warnings: Vec::new(),
        used: "97% of its 5-hour limit".into(),
        resets_at: Some(9_000),
    })
}

/// The accounts as read after a switch from `work` to `personal`.
fn personal_in_use() -> Status {
    status(vec![
        account(Some("work"))
            .limits(vec![window("session", 97.0).resets(Some(9_000))])
            .build(),
        account(Some("personal"))
            .signed_in()
            .limits(vec![window("session", 10.0).resets(Some(9_000))])
            .build(),
    ])
}

/// No account with room below the share for the five-hour limit of `work` that resets at
/// `resets`.
fn no_room(resets: i64) -> AutoSwitched {
    AutoSwitched::Skipped {
        key: format!("skipped/work/session//{resets}/no_room"),
        from: "work".into(),
        used: "97% of its 5-hour limit".into(),
        why: "no other Claude Code account has room below 95% in every limit".into(),
        code: "no_room",
        until: None,
    }
}

/// What the settings say while it watches `account`, with nothing more known of it.
fn watching_said(account: &str) -> String {
    words::typeset(&said::watching(&words::quoted(account), None, None, None))
}

/// A switch the core refused before it could record the attempt.
fn unwritable() -> Refusal {
    refusal(
        "home_unwritable",
        "could not write to Pitboard's directory at /Users/x/.pitboard: read-only",
        Vec::new(),
    )
}

/// `app.json` with switching by itself on.
const ON: &str = r#"{"has_been_seen":true,"auto_switch":true}"#;

/// The app started with switching by itself on, as `app.json` says, its first read and the
/// numbers that read recorded looked at: two seconds in, with the next look due at 32.
fn started_on(machine: &mut Stub) -> Hand {
    machine.preferences_file = Some(ON.into());
    let mut model = Hand::new();
    model.send(Intent::Start);
    model.run(machine);
    model.later(Duration::from_secs(2));
    model.tick();
    model.run(machine);
    model
}

/// `seconds` pass one at a time, the timers due in each answered, except the jobs that are
/// `but`, which wait.
fn seconds(model: &mut Hand, machine: &mut Stub, seconds: u64, but: fn(&Job) -> bool) {
    for _ in 0..seconds {
        model.later(Duration::from_secs(1));
        model.tick();
        model.run_but(machine, but);
    }
}

fn nothing(_: &Job) -> bool {
    false
}

/// What the settings say under the switch now.
fn standing(model: &Hand) -> Option<String> {
    model.shown().machine.at_limit.standing
}

#[test]
fn nothing_is_switched_by_itself_unless_somebody_turned_it_on() {
    let mut model = Hand::new();
    read_preferences(&mut model, false);
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    machine.look = Ok(AutoLooked::Act);
    model.refresh(&mut machine);
    assert_eq!(model.count(auto_look), 0, "the core is not even asked");
    assert_eq!(model.count(auto_switch), 0);
    let shown = model.shown().machine.at_limit;
    assert_eq!(shown.choice, AtLimit::Notify);
    assert_eq!(shown.standing, None);
}

/// The core's look claims nothing, and below the share it stands. Where it says only a
/// decision under the core's lock can say, the switch is asked for, claimed as one somebody
/// asked for is.
#[test]
fn a_limit_at_the_share_asks_the_core_to_switch_and_nothing_else_switches_meanwhile() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(94.0), personal()])));
    turned_on(&mut model, &mut machine);
    model.refresh(&mut machine);
    assert_eq!(machine.looked_at, [95], "after the read");
    assert_eq!(
        model.count(auto_switch),
        0,
        "below the share the look stands"
    );
    assert!(model.state.switch_under_way().is_none());

    machine.answer = Ok(status(vec![work(95.0), personal()]));
    machine.look = Ok(AutoLooked::Act);
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

/// The look is asked every 30 seconds, as `pitboard watch` decides, though nothing was
/// written: an account put in use stops settling, and a wait after an attempt ends, with
/// nobody writing anything.
#[test]
fn it_looks_again_every_thirty_seconds_though_nothing_was_written() {
    let mut machine = Stub::reading(Ok(status(vec![work(40.0), personal()])));
    let mut model = started_on(&mut machine);
    let looked = machine.looked_at.len();
    assert!(looked > 0, "after the read and the numbers it recorded");

    seconds(&mut model, &mut machine, 29, nothing);
    assert_eq!(machine.looked_at.len(), looked, "not before 30 seconds");
    seconds(&mut model, &mut machine, 1, nothing);
    assert_eq!(
        machine.looked_at.len(),
        looked + 1,
        "30 seconds after the last"
    );
    assert_eq!(model.count(any_read), 1, "with no read between");
}

/// Under the switch, the settings say which account it watches, how much of its fullest
/// limit is used and when that was read, and until when Anthropic holds Pitboard off asking
/// about it. Nothing while it is off.
#[test]
fn the_settings_say_which_account_it_watches_and_how_old_its_reading_is() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(62.0), personal()])));
    turned_on(&mut model, &mut machine);
    let now = model.now.epoch();
    let watching = |held_until| {
        Ok(AutoLooked::Stands(AutoSwitched::Watching {
            account: "work".into(),
            used: Some("62% of its 5-hour limit".into()),
            as_of: Some(now - 20 * 60),
            held_until,
        }))
    };
    machine.look = watching(None);
    model.refresh(&mut machine);
    let used = "62% of its 5-hour limit";
    assert_eq!(
        standing(&model),
        Some(words::typeset(&said::watching(
            "work",
            Some(used),
            Some("07:40"),
            None
        )))
    );

    machine.look = watching(Some(now + 3_600));
    model.refresh(&mut machine);
    assert_eq!(
        standing(&model),
        Some(words::typeset(&said::watching(
            "work",
            Some(used),
            Some("07:40"),
            Some("09:00")
        )))
    );

    model.send(Intent::SetAtLimit {
        choice: AtLimit::Nothing,
        at: 95,
    });
    assert_eq!(standing(&model), None);
}

// A label is the person's, as they typed it.
#[test]
fn the_settings_name_the_account_as_it_was_typed() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    turned_on(&mut model, &mut machine);
    machine.look = Ok(AutoLooked::Stands(AutoSwitched::Watching {
        account: "dana's".into(),
        used: None,
        as_of: None,
        held_until: None,
    }));
    model.refresh(&mut machine);
    let said = standing(&model).expect("something said");
    assert!(said.contains("dana's"), "{said}");
}

/// Where it does not switch, the settings say why, with when it acts again where the core
/// says.
#[test]
fn the_settings_say_why_it_is_not_switching() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    turned_on(&mut model, &mut machine);
    let now = model.now.epoch();
    let says = |model: &mut Hand, machine: &mut Stub, looked: AutoSwitched| {
        machine.look = Ok(AutoLooked::Stands(looked));
        model.refresh(machine);
        standing(model).expect("something said")
    };
    assert_eq!(
        says(
            &mut model,
            &mut machine,
            AutoSwitched::Waiting {
                from: "work".into(),
                used: "97% of its 5-hour limit".into(),
                until: now + 60,
            }
        ),
        words::typeset(&said::auto_waiting(
            "work",
            "97% of its 5-hour limit",
            "08:01"
        ))
    );
    assert_eq!(
        says(
            &mut model,
            &mut machine,
            AutoSwitched::Skipped {
                key: "skipped/work/session//9000/settling".into(),
                from: "work".into(),
                used: "97% of its 5-hour limit".into(),
                why: "the account was put in use less than 5 minutes ago".into(),
                code: "settling",
                until: Some(now + 300),
            }
        ),
        words::typeset(&said::auto_skipped(
            "work",
            "97% of its 5-hour limit",
            "the account was put in use less than 5 minutes ago",
            Some("08:05")
        ))
    );
    assert_eq!(
        says(
            &mut model,
            &mut machine,
            AutoSwitched::NotWatching {
                key: "not-watching/not_identified/could not reach Anthropic".into(),
                why: "whose login Claude Code has stored could not be told (could not reach \
                      Anthropic)"
                    .into(),
                until: Some(now + 60),
            }
        ),
        words::typeset(&said::auto_not_watching(
            "whose login Claude Code has stored could not be told (could not reach Anthropic)",
            Some("08:01")
        ))
    );
    assert_eq!(
        says(
            &mut model,
            &mut machine,
            AutoSwitched::Switched {
                from: "work".into(),
                to: "personal".into(),
                adoption: Adoption::PollingWithin(33),
                warnings: Vec::new(),
                used: "97% of its 5-hour limit".into(),
                resets_at: Some(9_000),
            }
        ),
        words::typeset(&said::auto_switched(
            "work",
            "personal",
            "97% of its 5-hour limit"
        )),
        "as `pitboard watch` says it"
    );

    machine.look = Ok(AutoLooked::Act);
    machine.auto = Err(unwritable());
    model.refresh(&mut machine);
    assert_eq!(
        standing(&model),
        Some(words::auto_stopped(
            Some("08:01"),
            "could not write to Pitboard's directory at /Users/x/.pitboard: read-only"
        ))
    );

    machine.look = Err(refusal(
        "state_corrupt",
        "Pitboard's account list at /Users/x/.pitboard/state.json is damaged",
        Vec::new(),
    ));
    model.refresh(&mut machine);
    assert_eq!(
        standing(&model),
        Some(words::auto_stopped(
            None,
            "Pitboard's account list at /Users/x/.pitboard/state.json is damaged"
        )),
        "files the core could not read"
    );
}

#[test]
fn a_switch_it_made_is_said_and_taken_as_a_switch_somebody_asked_for() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    turned_on(&mut model, &mut machine);
    machine.look = Ok(AutoLooked::Act);
    machine.auto = switched_work_to_personal();
    model.send(Intent::Refresh { asked: false });
    model.run_but(&mut machine, auto_switch);
    machine.answer = Ok(personal_in_use());
    machine.look = Ok(AutoLooked::Stands(watching("personal")));
    model.run(&mut machine);

    assert_eq!(
        machine.posted,
        [RunOutNotice {
            id: format!("auto/work/personal/{}", model.now.epoch()),
            title: words::switched_to(Some("Claude Code"), "personal"),
            subtitle: None,
            body: words::used_then(
                "work",
                "97% of its 5-hour limit",
                &words::follows(&Adoption::PollingWithin(33))
            ),
            clicked: "accounts".into(),
            button: Some(NotificationButton {
                title: words::switch_back(OS),
                clicked: r#"switch-back/["claude/work","claude/personal"]"#.into(),
            }),
        }]
    );
    let last = &model.state.last_switches;
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
    assert_eq!(
        standing(&model),
        Some(watching_said("personal")),
        "the look after the read after it"
    );
}

// What switching from `work` to `personal` by itself posted, read after with `personal` in use.
fn switched_by_itself(model: &mut Hand, machine: &mut Stub) -> RunOutNotice {
    let resets = model.now.epoch() + 3_600;
    turned_on(model, machine);
    machine.look = Ok(AutoLooked::Act);
    machine.auto = Ok(AutoSwitched::Switched {
        from: "work".into(),
        to: "personal".into(),
        adoption: Adoption::PollingWithin(33),
        warnings: Vec::new(),
        used: "97% of its 5-hour limit".into(),
        resets_at: Some(resets),
    });
    model.send(Intent::Refresh { asked: false });
    model.run_but(machine, auto_switch);
    machine.answer = Ok(personal_in_use());
    machine.look = Ok(AutoLooked::Stands(watching("personal")));
    model.run(machine);
    machine.posted.pop().expect("the switch posted")
}

fn work_in_use() -> Status {
    status(vec![
        work(97.0),
        account(Some("personal"))
            .limits(vec![window("session", 10.0).resets(Some(9_000))])
            .build(),
    ])
}

fn back_to_work() -> Intent {
    Intent::SwitchBack {
        from: "claude/work".into(),
        to: "claude/personal".into(),
    }
}

#[test]
fn only_a_switch_by_itself_offers_switch_back_from_its_notification_and_its_notice() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    let posted = switched_by_itself(&mut model, &mut machine);

    let button = posted.button.expect("Switch Back");
    assert_eq!(button.title, words::switch_back(OS));
    assert_eq!(
        crate::notification_buttons()
            .into_iter()
            .find(|kind| kind.title == button.title)
            .map(|kind| (kind.opens_window, kind.symbol)),
        Some((false, "arrow.uturn.backward".into()))
    );
    model.later(Duration::from_secs(60));
    let shown = model.shown();
    let notice = (shown.notices.iter())
        .find(|notice| notice.id == "switch/claude")
        .expect("kept past its countdown while it offers Switch Back");
    let offered: Vec<(&str, &Intent, bool)> = (notice.actions.iter())
        .map(|action| (action.title.as_str(), &action.intent, action.switches))
        .collect();
    assert_eq!(
        offered[0],
        (words::switch_back(OS).as_str(), &back_to_work(), true)
    );

    machine.switched = switched("claude", "personal", "work", Vec::new());
    model.send(Intent::SwitchTo {
        qualified: "claude/work".into(),
    });
    machine.answer = Ok(work_in_use());
    model.run(&mut machine);
    assert!(
        (model.shown().notices.iter())
            .filter(|notice| notice.id == "switch/claude")
            .all(|notice| notice.actions.iter().all(|action| !action.switches)),
        "{:?}",
        model.shown().notices
    );
}

#[test]
fn switch_back_switches_to_the_account_left() {
    for click in [true, false] {
        let mut model = Hand::new();
        let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
        let posted = switched_by_itself(&mut model, &mut machine);
        let window = model.shown().window_request;
        machine.switched = switched("claude", "personal", "work", Vec::new());
        if click {
            model.send(Intent::NotificationClicked {
                argument: posted.button.expect("Switch Back").clicked,
            });
        } else {
            model.send(back_to_work());
        }
        model.run(&mut machine);
        assert_eq!(machine.switched_to, ["claude/work"]);
        assert_eq!(model.shown().window_request, window, "no window");
    }
}

#[test]
fn switch_back_after_another_switch_only_opens_the_accounts() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    switched_by_itself(&mut model, &mut machine);
    let serial = model.shown().window_request.serial;
    model.send(Intent::SwitchTo {
        qualified: "claude/personal".into(),
    });
    model.send(back_to_work());
    let shown = model.shown().window_request;
    assert_eq!(
        (shown.serial, shown.place),
        (serial + 1, Some(Place::Accounts))
    );
    model.run(&mut machine);

    let spare = account(Some("spare")).signed_in().build();
    machine.answer = Ok(status(vec![work(97.0), spare]));
    model.refresh(&mut machine);
    model.send(back_to_work());
    model.run(&mut machine);
    assert_eq!(machine.switched_to, ["claude/personal"]);
    assert_eq!(model.shown().window_request.serial, serial + 2);
}

#[test]
fn after_switch_back_nothing_says_claude_code_was_not_switched_until_the_limit_resets() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    let resets = model.now.epoch() + 3_600;
    switched_by_itself(&mut model, &mut machine);
    machine.switched = switched("claude", "personal", "work", Vec::new());
    model.send(back_to_work());
    machine.answer = Ok(work_in_use());
    model.run(&mut machine);
    let skipped = |code: &'static str, resets: i64| AutoSwitched::Skipped {
        key: format!("skipped/work/session//{resets}/{code}"),
        from: "work".into(),
        used: "97% of its 5-hour limit".into(),
        why: code.into(),
        code,
        until: None,
    };
    for code in ["settling", "already_switched"] {
        machine.look = Ok(AutoLooked::Stands(skipped(code, resets)));
        model.refresh(&mut machine);
    }
    assert!(machine.posted.is_empty(), "{:?}", machine.posted);
    assert_eq!(
        standing(&model),
        Some(words::typeset(&said::auto_skipped(
            "work",
            "97% of its 5-hour limit",
            "already_switched",
            None
        ))),
        "the settings still say it"
    );
    machine.look = Ok(AutoLooked::Stands(no_room(resets)));
    model.refresh(&mut machine);
    assert_eq!(machine.posted.len(), 1, "another reason");

    model.later(Duration::from_secs(3_600));
    machine.look = Ok(AutoLooked::Stands(skipped(
        "already_switched",
        resets + 18_000,
    )));
    model.refresh(&mut machine);
    assert_eq!(machine.posted.len(), 2, "once the limit has reset");
}

#[test]
fn a_switch_back_argument_that_cannot_be_read_opens_the_accounts() {
    for argument in [
        "switch-back/",
        r#"switch-back/["claude/work"]"#,
        "switch-back/claude/work",
    ] {
        let mut model = Hand::new();
        let jobs = model.send(Intent::NotificationClicked {
            argument: argument.into(),
        });
        assert_eq!(jobs, [], "{argument}");
        assert_eq!(model.shown().window_request.place, Some(Place::Accounts));
    }
}

/// The setting promises that running sessions follow within 33 seconds only while the last
/// read found no file behind the keychain; with one there, it says they keep their account
/// until their login is next renewed.
#[test]
fn the_setting_promises_33_seconds_only_while_no_file_is_behind_the_keychain() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(40.0), personal()])));
    turned_on(&mut model, &mut machine);
    model.refresh(&mut machine);
    let note = |os, file_left| {
        words::at_limit_note(
            AtLimit::Switch,
            95,
            &words::sessions_follow(os, file_left, 33),
        )
    };
    assert_eq!(model.shown().machine.at_limit.note, note(OS, false));

    machine.answer = Ok(super::testing::records::warned(
        vec![work(40.0), personal()],
        vec![super::testing::records::warning(
            "fallback_login",
            "/Users/x/.claude/.credentials.json is there with no Claude Code login in it.",
        )],
    ));
    model.refresh(&mut machine);
    assert_eq!(
        model.on_a_mac().machine.at_limit.note,
        note(Os::MacOs, true)
    );
    assert_eq!(
        model.on_windows().machine.at_limit.note,
        note(Os::Windows, true)
    );
    assert_ne!(note(Os::MacOs, true), note(Os::MacOs, false));
}

/// A reason it did not switch is said once for the limit and its reset, whether the decision
/// under the core's lock gave it or a look stood on it later, and once more for the limit's
/// next window.
#[test]
fn a_reason_it_did_not_switch_is_said_once() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    turned_on(&mut model, &mut machine);
    let why = "Claude Code signs in another way, set by apiKeyHelper, so a switch would change \
               nothing its sessions use";
    let overridden = |resets: i64| AutoSwitched::Skipped {
        key: format!("skipped/work/session//{resets}/auth_overridden"),
        from: "work".into(),
        used: "97% of its 5-hour limit".into(),
        why: why.into(),
        code: "auth_overridden",
        until: None,
    };
    machine.look = Ok(AutoLooked::Act);
    machine.auto = Ok(overridden(9_000));
    model.refresh(&mut machine);
    machine.look = Ok(AutoLooked::Stands(overridden(9_000)));
    model.refresh(&mut machine);
    assert_eq!(
        machine.auto_at.len(),
        1,
        "the look stands on what was recorded"
    );
    assert_eq!(machine.posted.len(), 1, "{:?}", machine.posted);
    assert_eq!(machine.posted[0].title, words::NOT_SWITCHED);
    assert_eq!(
        machine.posted[0].body,
        words::used_then("work", "97% of its 5-hour limit", &words::sentence(why))
    );
    assert_eq!(machine.posted[0].button, None);
    assert_eq!(machine.posted[0].clicked, "limits");

    machine.look = Ok(AutoLooked::Stands(overridden(27_000)));
    model.refresh(&mut machine);
    assert_eq!(
        machine.posted.len(),
        2,
        "the limit's next window, recorded by another front end"
    );
}

/// A reason the core cannot judge whether to switch at all is said once for each reason and
/// what it names, and leaves nothing claimed.
#[test]
fn a_reason_it_is_not_watching_is_said_once() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    turned_on(&mut model, &mut machine);
    let why = "a switch was interrupted, and the next change you make finishes it";
    machine.look = Ok(AutoLooked::Stands(AutoSwitched::NotWatching {
        key: "not-watching/switch_interrupted".into(),
        why: why.into(),
        until: None,
    }));
    model.refresh(&mut machine);
    model.refresh(&mut machine);
    assert!(machine.auto_at.is_empty());
    assert_eq!(
        machine.posted,
        [RunOutNotice {
            id: "auto-not-watching/switch_interrupted".into(),
            title: words::NOT_SWITCHING.into(),
            subtitle: None,
            body: words::sentence(why),
            clicked: "limits".into(),
            button: None,
        }]
    );
    assert!(model.state.switch_under_way().is_none());

    machine.look = Ok(AutoLooked::Stands(AutoSwitched::NotWatching {
        key: "not-watching/not_enrolled/me@example.com".into(),
        why: "Claude Code has me@example.com's login stored, and that account is not enrolled"
            .into(),
        until: None,
    }));
    model.refresh(&mut machine);
    assert_eq!(
        machine.posted.len(),
        2,
        "another reason: {:?}",
        machine.posted
    );
}

/// A refusal is said once too, in the core's words, and leaves nothing claimed.
#[test]
fn a_refusal_is_said_once_and_lets_go() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    turned_on(&mut model, &mut machine);
    machine.look = Ok(AutoLooked::Act);
    machine.auto = Err(refusal(
        "identity_unverifiable",
        "Pitboard could not confirm with Anthropic which account is signed in.",
        Vec::new(),
    ));
    model.refresh(&mut machine);
    model.later(Duration::from_secs(60));
    model.refresh(&mut machine);
    assert_eq!(machine.auto_at.len(), 2);
    assert_eq!(machine.posted.len(), 1);
    assert_eq!(machine.posted[0].title, words::COULD_NOT_SWITCH_ITSELF);
    assert!(model.state.switch_under_way().is_none());
    assert!(model.state.presented.is_none(), "nobody asked, so no alert");
}

/// After a refusal it asks again no sooner than the core asks again after an attempt that
/// came to nothing: a minute, then twice as long after each refusal in a row, whatever was
/// read meanwhile. Each refusal is a line in the activity log. Anything else the core says
/// ends the row.
#[test]
fn after_a_refusal_it_waits_as_the_core_waits() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    turned_on(&mut model, &mut machine);
    machine.look = Ok(AutoLooked::Act);
    machine.auto = Err(unwritable());
    model.refresh(&mut machine);
    assert_eq!(machine.auto_at.len(), 1);

    model.later(Duration::from_secs(30));
    machine.offline = Ok(status(vec![work(98.0), personal()]));
    machine.readings += 1;
    model.notice(&mut machine);
    assert_eq!(
        machine.auto_at.len(),
        1,
        "not at numbers a session recorded"
    );
    model.refresh(&mut machine);
    assert_eq!(machine.auto_at.len(), 1, "nor at a read, within the minute");

    model.later(Duration::from_secs(30));
    model.refresh(&mut machine);
    assert_eq!(machine.auto_at.len(), 2, "a minute after");

    model.later(Duration::from_secs(60));
    model.refresh(&mut machine);
    assert_eq!(machine.auto_at.len(), 2, "twice as long after the second");
    model.later(Duration::from_secs(60));
    model.refresh(&mut machine);
    assert_eq!(machine.auto_at.len(), 3);

    machine.auto = Ok(AutoSwitched::Waiting {
        from: "work".into(),
        used: "97% of its 5-hour limit".into(),
        until: model.now.epoch() + 60,
    });
    model.later(Duration::from_secs(240));
    model.refresh(&mut machine);
    assert_eq!(machine.auto_at.len(), 4);
    machine.auto = Err(unwritable());
    model.refresh(&mut machine);
    assert_eq!(machine.auto_at.len(), 5, "the core said something else");
    model.later(Duration::from_secs(60));
    model.refresh(&mut machine);
    assert_eq!(machine.auto_at.len(), 6, "a row begun again waits a minute");
}

/// The next decision after a refusal is asked for as its wait ends, by a look due then, and
/// not at the next look 30 seconds after the last.
#[test]
fn the_next_switch_is_asked_for_as_a_refusals_wait_ends() {
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    machine.look = Ok(AutoLooked::Act);
    machine.auto = Err(unwritable());
    machine.preferences_file = Some(ON.into());
    let mut model = Hand::new();
    model.send(Intent::Start);
    model.run_but(&mut machine, auto_switch);
    seconds(&mut model, &mut machine, 5, auto_switch);
    let decision = model.take(auto_switch);
    let refused = machine.answer(decision);
    model.give(refused);
    assert_eq!(machine.auto_at.len(), 1, "refused 5 seconds in, until 65");

    // The numbers the read recorded are looked at as the claim ends, at 6, and the timer
    // looks again at 36.
    seconds(&mut model, &mut machine, 59, nothing);
    assert_eq!(machine.auto_at.len(), 1, "not within the wait");
    seconds(&mut model, &mut machine, 1, nothing);
    assert_eq!(machine.auto_at.len(), 2, "at 65, before the look due at 66");
}

/// A switch ends a row of refusals too: the next refusal waits a minute, not twice as long.
#[test]
fn a_switch_ends_a_row_of_refusals() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    turned_on(&mut model, &mut machine);
    machine.look = Ok(AutoLooked::Act);
    machine.auto = Err(unwritable());
    model.refresh(&mut machine);

    model.later(Duration::from_secs(60));
    machine.auto = switched_work_to_personal();
    model.send(Intent::Refresh { asked: false });
    model.run_but(&mut machine, auto_switch);
    let decision = model.take(auto_switch);
    let switched = machine.answer(decision);
    machine.auto = Err(unwritable());
    model.give(switched);
    model.run(&mut machine);
    assert_eq!(
        machine.auto_at.len(),
        3,
        "switched, then refused at the look after the read after it"
    );

    model.later(Duration::from_secs(60));
    model.refresh(&mut machine);
    assert_eq!(machine.auto_at.len(), 4, "a minute after, a row of one");
}

/// Turned off and on again while a refusal's wait runs, it is somebody asking: the core is
/// asked at once.
#[test]
fn turned_off_and_on_again_it_asks_at_once_though_a_refusals_wait_runs() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    turned_on(&mut model, &mut machine);
    machine.look = Ok(AutoLooked::Act);
    machine.auto = Err(unwritable());
    model.refresh(&mut machine);
    assert_eq!(machine.auto_at.len(), 1);

    model.later(Duration::from_secs(10));
    model.send(Intent::SetAtLimit {
        choice: AtLimit::Nothing,
        at: 95,
    });
    model.send(Intent::SetAtLimit {
        choice: AtLimit::Switch,
        at: 95,
    });
    model.run(&mut machine);
    assert_eq!(machine.auto_at.len(), 2);
}

/// A look that says only a decision can say, answered once the setting was turned off, asks
/// for none and claims nothing: the person said stop.
#[test]
fn a_look_answered_after_it_was_turned_off_asks_for_no_switch() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    turned_on(&mut model, &mut machine);
    machine.look = Ok(AutoLooked::Act);
    model.send(Intent::Refresh { asked: false });
    model.run_but(&mut machine, auto_look);
    model.send(Intent::SetAtLimit {
        choice: AtLimit::Nothing,
        at: 95,
    });
    model.run(&mut machine);
    assert_eq!(machine.looked_at.len(), 1);
    assert_eq!(model.count(auto_switch), 0);
    assert!(model.state.switch_under_way().is_none());
}

/// A decision under way as the setting is turned off that comes to anything but a switch is
/// neither posted nor waited on: turned on again, the core is asked at once, and what it
/// says then is said.
#[test]
fn a_decision_answered_after_it_was_turned_off_says_nothing_and_holds_nothing_back() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    turned_on(&mut model, &mut machine);
    machine.look = Ok(AutoLooked::Act);
    for came in [Err(unwritable()), Ok(no_room(9_000))] {
        machine.auto = came;
        model.send(Intent::Refresh { asked: false });
        model.run_but(&mut machine, auto_switch);
        model.send(Intent::SetAtLimit {
            choice: AtLimit::Nothing,
            at: 95,
        });
        model.run(&mut machine);
        assert!(machine.posted.is_empty(), "{:?}", machine.posted);
        assert!(model.state.switch_under_way().is_none(), "let go");

        let asked = machine.auto_at.len();
        model.later(Duration::from_secs(5));
        machine.auto = Ok(watching("work"));
        model.send(Intent::SetAtLimit {
            choice: AtLimit::Switch,
            at: 95,
        });
        model.run(&mut machine);
        assert_eq!(machine.auto_at.len(), asked + 1, "asked at once");
        assert_eq!(standing(&model), Some(watching_said("work")));
    }
}

/// A switch the core made as the setting was turned off happened, so it is said, and taken
/// as a switch somebody asked for.
#[test]
fn a_switch_made_as_it_was_turned_off_is_still_said() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    turned_on(&mut model, &mut machine);
    machine.look = Ok(AutoLooked::Act);
    machine.auto = switched_work_to_personal();
    model.send(Intent::Refresh { asked: false });
    model.run_but(&mut machine, auto_switch);
    model.send(Intent::SetAtLimit {
        choice: AtLimit::Nothing,
        at: 95,
    });
    machine.answer = Ok(personal_in_use());
    model.run(&mut machine);
    assert_eq!(
        machine
            .posted
            .iter()
            .map(|notice| notice.title.as_str())
            .collect::<Vec<_>>(),
        [words::switched_to(Some("Claude Code"), "personal")]
    );
    assert_eq!(
        model
            .state
            .last_switches
            .iter()
            .map(|s| s.to.as_str())
            .collect::<Vec<_>>(),
        ["personal"]
    );
    assert!(model.state.switch_under_way().is_none());
}

/// The first look of a launch follows its first read, which asks Anthropic for the reading
/// the look judges by. Before it, the readings may be an older Pitboard's, which the core
/// cannot judge, and a reason posted then would be gone seconds later.
#[test]
fn the_first_look_of_a_launch_follows_its_first_read() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(40.0), personal()])));
    machine.preferences_file = Some(ON.into());
    machine.look = Ok(AutoLooked::Stands(AutoSwitched::NotWatching {
        key: "not-watching/no_reading/work".into(),
        why: "there is no reading of work from Anthropic yet".into(),
        until: None,
    }));
    model.send(Intent::Start);
    model.run_but(&mut machine, any_read);
    assert!(machine.looked_at.is_empty(), "not before the read");
    assert_eq!(standing(&model), None);

    machine.look = Ok(AutoLooked::Stands(watching("work")));
    model.run(&mut machine);
    assert_eq!(machine.looked_at.len(), 1, "once it lands");
    assert!(machine.posted.is_empty(), "{:?}", machine.posted);
    assert_eq!(standing(&model), Some(watching_said("work")));
}

/// A read that lands before the preferences are read is looked at as they are.
#[test]
fn a_read_that_landed_before_the_preferences_is_looked_at_as_they_are_read() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(40.0), personal()])));
    machine.preferences_file = Some(ON.into());
    model.send(Intent::Start);
    model.run_but(&mut machine, |job| matches!(job, Job::LoadKept));
    assert_eq!(model.count(any_read), 1);
    assert!(machine.looked_at.is_empty(), "not before the preferences");

    model.run(&mut machine);
    assert_eq!(machine.looked_at.len(), 1);
}

/// A look lost to a panic says nothing, and the next comes 30 seconds later.
#[test]
fn after_a_look_is_lost_the_next_comes_at_its_time() {
    let mut machine = Stub::reading(Ok(status(vec![work(40.0), personal()])));
    let mut model = started_on(&mut machine);
    seconds(&mut model, &mut machine, 30, auto_look);
    let look = model.take(auto_look);
    model.give(Answer::Lost(look));
    let looked = machine.looked_at.len();

    seconds(&mut model, &mut machine, 29, nothing);
    assert_eq!(machine.looked_at.len(), looked);
    seconds(&mut model, &mut machine, 1, nothing);
    assert_eq!(machine.looked_at.len(), looked + 1);
}

/// A change the poll found elsewhere, such as a switch typed in a terminal, is looked at.
#[test]
fn a_change_made_elsewhere_is_looked_at() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(40.0), personal()])));
    turned_on(&mut model, &mut machine);
    model.refresh(&mut machine);
    let looked = machine.looked_at.len();

    machine.changed += 1;
    model.notice(&mut machine);
    assert_eq!(model.count(offline_read), 1);
    assert_eq!(machine.looked_at.len(), looked + 1);
}

/// No account with room is said once for the limit and its reset, and again once the limit's
/// next window reaches the share.
#[test]
fn no_account_with_room_is_said_once_for_a_limit_and_its_reset() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(97.0), personal()])));
    turned_on(&mut model, &mut machine);
    machine.look = Ok(AutoLooked::Act);
    machine.auto = Ok(no_room(9_000));
    model.refresh(&mut machine);
    model.refresh(&mut machine);
    assert_eq!(machine.auto_at.len(), 2);
    assert_eq!(machine.posted.len(), 1, "{:?}", machine.posted);
    assert_eq!(machine.posted[0].title, words::NOT_SWITCHED);
    let AutoSwitched::Skipped { why, .. } = no_room(9_000) else {
        unreachable!("no room is a switch skipped");
    };
    assert_eq!(
        machine.posted[0].body,
        words::used_then("work", "97% of its 5-hour limit", &words::sentence(&why))
    );
    assert!(model.state.switch_under_way().is_none());

    machine.auto = Ok(no_room(27_000));
    model.refresh(&mut machine);
    assert_eq!(machine.posted.len(), 2, "the limit's next window");
}

/// Not while a switch somebody asked for is under way: theirs is the one to make.
#[test]
fn not_while_another_switch_is_under_way() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![work(90.0), personal()])));
    turned_on(&mut model, &mut machine);
    model.refresh(&mut machine);
    model.send(Intent::SwitchTo {
        qualified: "claude/personal".into(),
    });
    machine.answer = Ok(status(vec![work(97.0), personal()]));
    machine.look = Ok(AutoLooked::Act);
    model.send(Intent::Refresh { asked: false });
    model.run_but(&mut machine, |job| {
        matches!(job, Job::Holding { .. } | Job::Switch { .. })
    });
    assert!(model.count(auto_look) > 0, "the look claims nothing");
    assert_eq!(model.count(auto_switch), 0);
}

// The switch would wait behind the other on the lane of changes, holding every switch asked for.
#[test]
fn not_while_another_change_is_under_way() {
    let mut model = Hand::new();
    let spare = account(Some("spare")).build();
    let mut machine = Stub::reading(Ok(status(vec![work(90.0), personal(), spare])));
    turned_on(&mut model, &mut machine);
    model.refresh(&mut machine);
    model.send(Intent::Forget {
        qualified: "claude/spare".into(),
    });
    machine.answer = Ok(status(vec![work(97.0), personal()]));
    machine.look = Ok(AutoLooked::Act);
    model.send(Intent::Refresh { asked: false });
    model.run_but(&mut machine, |job| matches!(job, Job::Forget { .. }));
    assert_eq!(model.count(auto_switch), 0);

    model.run(&mut machine);
    assert_eq!(
        model.count(auto_switch),
        1,
        "once the forget and the read after it are over"
    );
}

/// The settings' switch is kept in the preferences, and turned on, the core looks at once.
/// Before the preferences are read it is not taken: what they say would win.
#[test]
fn turning_it_on_is_kept_and_looks_at_once() {
    let mut model = Hand::new();
    assert_eq!(
        model.send(Intent::SetAtLimit {
            choice: AtLimit::Switch,
            at: 90,
        }),
        [],
        "not before the preferences are read"
    );
    assert!(!model.shown().machine.at_limit.enabled);

    read_preferences(&mut model, false);
    let mut machine = Stub::reading(Ok(status(vec![work(92.0), personal()])));
    model.refresh(&mut machine);
    assert_eq!(model.count(auto_look), 0);

    let jobs = model.send(Intent::SetAtLimit {
        choice: AtLimit::Switch,
        at: 90,
    });
    assert_eq!(
        jobs,
        [
            Job::KeepPreferences {
                preferences: Preferences {
                    auto_switch: true,
                    auto_switch_at: Some(90),
                    at_limit: Some("switch".into()),
                    has_been_seen: true,
                    ..Preferences::default()
                }
            },
            Job::AutoLook { at: 90 },
        ]
    );
    let shown = model.shown().machine.at_limit;
    assert!(shown.choice == AtLimit::Switch && shown.enabled);
    assert_eq!((shown.at, shown.lowest, shown.highest), (90, 50, 99));
    assert_eq!(shown.at_label, words::at_limit_label(90));

    model.run(&mut machine);
    assert_eq!(
        model.send(Intent::SetAtLimit {
            choice: AtLimit::Switch,
            at: 90,
        }),
        [],
        "what it already is"
    );
    let jobs = model.send(Intent::SetAtLimit {
        choice: AtLimit::Nothing,
        at: 120,
    });
    assert_eq!(jobs.len(), 1);
    assert_eq!(
        model.state.preferences.auto_switch_at,
        Some(99),
        "the nearest share there can be"
    );
}
