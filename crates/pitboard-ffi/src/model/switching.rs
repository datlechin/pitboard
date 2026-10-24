//! Switching, quitting the app that holds a login, giving up on a stuck switch, and what each
//! tool's last switch said, as the Swift model did them: its tests on those, each under its
//! own name in snake case, driven through `State::apply` by hand. Where a Swift test also
//! checked what the window says of it, which comes with the model's wording, the rest of it
//! is kept here. The interleavings the Swift tests reached with gates are answers handed to
//! `apply` in the same order.

use super::changing::a_look_landing_after;
use super::state::{After, Answer, Job};
use super::testing::machine::{
    Hand, Stub, a_look_or_a_read, a_switch, any_read, desktop_machine, fresh_read, holding_ask,
    offline_read,
};
use super::testing::platform::StandInApps;
use super::testing::records::{
    CHATGPT, CLAUDE, already_active, chatgpt_holding, claude, codex_account, desktop_account,
    desktop_warning, percent, refusal, status, still_running, switched, warned, warning,
};
use super::{Intent, LastSwitch, Pane, QuitQuestion, RestartNeeded, Sheet, WindowRequest};
use crate::present::words;
use crate::{Abandoned, GaveUp};
use pitboard_core::host::{OS, Os};
use std::time::Duration;

/// Asks for a switch to `qualified` and answers everything it leads to, as the Swift
/// model's tests awaited `use` or `switchAsked`.
pub(super) fn switch(model: &mut Hand, machine: &mut Stub, qualified: &str) {
    model.send(Intent::SwitchTo {
        qualified: qualified.into(),
    });
    model.run(machine);
}

/// The person's answer that Pitboard may quit the app for the switch to `qualified`.
fn quit_and_switch(qualified: &str) -> Intent {
    Intent::QuitAndSwitch {
        qualified: qualified.into(),
    }
}

fn tos(model: &Hand) -> Vec<String> {
    model
        .state
        .last_switches
        .iter()
        .map(|last| last.to.clone())
        .collect()
}

/// A switch that failed must not read as one that worked. The failure goes back to whoever
/// asked for the switch rather than where a read's is said, where the next read replaced it
/// seconds later, before anyone had looked. Asked for from the menu or a notification, which
/// have nowhere to put a sentence, it is said in the window.
#[test]
fn a_failed_switch_says_so_and_changes_nothing() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.switched = Err(refusal("nothing_parked", "nothing parked", Vec::new()));
    switch(&mut model, &mut machine, "work");

    assert_eq!(machine.switched_to, ["work"]);
    let shown = model.shown();
    let failure = model.state.presented.clone().expect("the failure is said");
    assert_eq!(failure.title, words::could_not_switch("work"));
    assert_eq!(failure.message, "nothing parked");
    assert_eq!(failure.code.as_deref(), Some("nothing_parked"));
    assert_eq!(model.state.failure, None, "a switch is not a read");
    assert!(model.state.last_switches.is_empty());
    assert_eq!(
        shown.window_request,
        WindowRequest {
            serial: 1,
            pane: None,
            settings: false,
        }
    );
    assert_eq!(model.state.switch_under_way(), None);
    assert_eq!(
        model.count(any_read),
        0,
        "nothing moved, so nothing is read"
    );
}

/// After a switch the panel counts down to when open sessions follow.
#[test]
fn a_switch_records_when_open_sessions_follow() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
        claude("work", true, 0.0),
        claude("personal", false, 0.0),
    ])));
    machine.switched = switched("claude", "personal", "work", Vec::new());
    switch(&mut model, &mut machine, "claude/work");
    let last = model
        .state
        .last_switches
        .first()
        .expect("what the switch said");
    assert_eq!(last.follows_at.map(|at| at - model.now.epoch()), Some(33));
    assert_eq!(last.restart, None);
}

/// A running `codex` never picks a switch up, so a countdown there would promise something that
/// does not happen. When the core counted the sessions still running, its warning says so, with
/// the count and with what not to do in them, and it outlives the read after the switch, which
/// would otherwise replace it. The sentence comes with what the window says.
#[test]
fn a_switch_that_needs_a_restart_says_so_and_counts_nothing_down() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
        codex_account("work", true),
        claude("personal", false, 0.0),
    ])));
    machine.switched = switched(
        "codex",
        "codex/personal",
        "codex/work",
        vec![still_running()],
    );
    switch(&mut model, &mut machine, "codex/work");

    let last = model
        .state
        .last_switches
        .first()
        .expect("what the switch said");
    assert_eq!(last.follows_at, None, "no countdown");
    assert_eq!(
        last.restart,
        Some(RestartNeeded {
            program: "codex".into(),
            from: "personal".into(),
        }),
        "the core types the account with its tool; the sentence names the tool already"
    );
    assert_eq!(last.warnings, [still_running()]);
    assert!(
        model.state.warnings.is_empty(),
        "the read after it does not say it"
    );

    model.send(Intent::DismissSwitch {
        provider: "codex".into(),
    });
    assert!(model.state.last_switches.is_empty());
}

/// A switch replaces what its own tool's last switch said, and nothing another tool's said:
/// a Claude Code switch says nothing about Codex sessions still using the account Codex
/// just parked, and used to put away the warning not to sign out inside one.
#[test]
fn each_tool_keeps_what_its_own_last_switch_said() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
        claude("b", true, 0.0),
        codex_account("b", true),
    ])));
    machine.switched = switched("codex", "codex/a", "codex/b", vec![still_running()]);
    switch(&mut model, &mut machine, "codex/b");

    machine.switched = switched("claude", "a", "b", Vec::new());
    switch(&mut model, &mut machine, "claude/b");
    let providers: Vec<&str> = model
        .state
        .last_switches
        .iter()
        .map(|last| last.provider.as_str())
        .collect();
    assert_eq!(providers, ["codex", "claude"]);
    assert_eq!(model.state.last_switches[0].warnings, [still_running()]);
    assert!(model.state.last_switches[1].follows_at.is_some());

    machine.answer = Ok(status(vec![
        claude("b", true, 0.0),
        codex_account("c", true),
    ]));
    machine.switched = switched("codex", "codex/b", "codex/c", Vec::new());
    switch(&mut model, &mut machine, "codex/c");
    assert_eq!(tos(&model), ["codex/c", "b"]);
    assert!(
        model.state.last_switches[0].warnings.is_empty(),
        "the last Codex switch, not the one before"
    );
}

// The core types the account with its tool where another tool has one of that name.
#[test]
fn a_switch_to_a_name_two_tools_share_outlives_the_read_after_it() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
        claude("work", true, 0.0),
        claude("personal", false, 0.0),
        codex_account("work", true),
    ])));
    machine.switched = switched("claude", "personal", "claude/work", Vec::new());
    switch(&mut model, &mut machine, "claude/work");
    assert_eq!(tos(&model), ["claude/work"]);
}

/// Whatever else writes the account index leaves the sessions a switch described as they
/// were: a read that renews a lapsed parked login, a scheduled renewal, an enrolment. Only a
/// switch made somewhere else, which leaves another account signed in, puts it away.
#[test]
fn only_a_switch_made_elsewhere_puts_away_what_a_switch_said() {
    let after = status(vec![
        claude("mine", true, 0.0),
        codex_account("personal", false),
        codex_account("work", true),
    ]);
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(after.clone()));
    machine.switched = switched(
        "codex",
        "codex/personal",
        "codex/work",
        vec![still_running()],
    );
    switch(&mut model, &mut machine, "codex/work");

    machine.offline = Ok(after);
    machine.changed = 99;
    model.notice(&mut machine);
    assert_eq!(tos(&model), ["codex/work"], "written, and nothing switched");

    // Claude Code switched in a terminal: Codex's sessions are where they were.
    machine.offline = Ok(status(vec![
        claude("mine", false, 0.0),
        claude("theirs", true, 0.0),
        codex_account("personal", false),
        codex_account("work", true),
    ]));
    machine.changed = 100;
    model.notice(&mut machine);
    assert_eq!(tos(&model), ["codex/work"]);

    // Codex switched back in a terminal: what this switch said is no longer true.
    machine.offline = Ok(status(vec![
        claude("mine", true, 0.0),
        codex_account("personal", true),
        codex_account("work", false),
    ]));
    machine.changed = 101;
    model.notice(&mut machine);
    assert!(model.state.last_switches.is_empty());
}

/// A read that fails after a switch leaves the change unrecorded, so the poll finds it.
/// That is the app's own switch, and taking it for somebody else's put away what it said
/// within two seconds of it being said.
#[test]
fn a_failed_read_after_a_switch_does_not_put_away_what_it_said() {
    let before = status(vec![
        codex_account("personal", true),
        codex_account("work", false),
    ]);
    let after = status(vec![
        codex_account("personal", false),
        codex_account("work", true),
    ]);
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(before));
    model.refresh(&mut machine);

    machine.answer = Err(refusal(
        "unreachable",
        "OpenAI could not be reached",
        Vec::new(),
    ));
    machine.offline = Ok(after.clone());
    machine.switched = switched(
        "codex",
        "codex/personal",
        "codex/work",
        vec![still_running()],
    );
    switch(&mut model, &mut machine, "codex/work");
    machine.changed = 7;
    model.notice(&mut machine);

    assert_eq!(model.state.last_switches[0].warnings, [still_running()]);
    assert_eq!(
        model.state.status,
        Some(after),
        "and the panel shows the switch"
    );
}

/// A switch that failed moved nothing, so what the last one said is still true, and the
/// failure is said as a failure with everything it warned about.
#[test]
fn a_failed_switch_leaves_what_the_last_switch_said() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
        claude("work", true, 0.0),
        codex_account("work", true),
    ])));
    machine.switched = switched(
        "codex",
        "codex/personal",
        "codex/work",
        vec![still_running()],
    );
    switch(&mut model, &mut machine, "codex/work");
    let before = model.state.last_switches.clone();

    let overridden = warning("auth_overridden", "ANTHROPIC_API_KEY is set");
    machine.switched = Err(refusal(
        "parked_login_expired",
        "spare's parked login has expired",
        vec![overridden.clone()],
    ));
    switch(&mut model, &mut machine, "claude/spare");

    assert_eq!(model.state.last_switches, before);
    let failure = model.state.presented.clone().expect("the failure is said");
    assert_eq!(failure.message, "spare's parked login has expired");
    assert_eq!(failure.warnings, std::slice::from_ref(&overridden));
    assert_eq!(
        model.state.warnings,
        [overridden],
        "and the panel says it beside the accounts"
    );
}

/// A switch to the account already in use moves nothing, so a notification pressed after
/// the switch it advised was made elsewhere leaves what that switch said.
#[test]
fn a_switch_to_the_account_in_use_leaves_what_the_last_one_said() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
        codex_account("work", true),
        claude("home", true, 0.0),
    ])));
    machine.switched = switched(
        "codex",
        "codex/personal",
        "codex/work",
        vec![still_running()],
    );
    switch(&mut model, &mut machine, "codex/work");
    let before = model.state.last_switches.clone();

    machine.switched = already_active("codex/work", Vec::new());
    switch(&mut model, &mut machine, "codex/work");
    assert_eq!(model.state.last_switches, before);

    // Nor does one of a tool with no last switch, which has nothing to say either.
    machine.switched = already_active("home", Vec::new());
    switch(&mut model, &mut machine, "claude/home");
    assert_eq!(model.state.last_switches, before);
}

/// What a switch to the account in use warned about is said beside what the last switch
/// said, each once, and stays with that switch; with no last switch it stands alone.
#[test]
fn a_switch_to_the_account_in_use_says_what_it_warned_beside_the_last() {
    let overridden = warning("auth_overridden", "OPENAI_API_KEY is set");
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
        claude("home", true, 0.0),
        codex_account("work", true),
    ])));
    machine.switched = switched(
        "codex",
        "codex/personal",
        "codex/work",
        vec![still_running()],
    );
    switch(&mut model, &mut machine, "codex/work");

    machine.switched = already_active("codex/work", vec![still_running(), overridden.clone()]);
    switch(&mut model, &mut machine, "codex/work");
    assert_eq!(
        model.state.last_switches[0].warnings,
        [still_running(), overridden.clone()]
    );
    assert!(
        model.state.last_switches[0].restart.is_some(),
        "the switch's own"
    );

    machine.switched = already_active("home", vec![overridden.clone()]);
    switch(&mut model, &mut machine, "claude/home");
    assert_eq!(
        model.state.last_switches[1],
        LastSwitch {
            provider: "claude".into(),
            to: "home".into(),
            follows_at: None,
            restart: None,
            said: None,
            warnings: vec![overridden],
            opens: None,
        }
    );
}

/// A reading moving says nothing about who is signed in, so only its numbers are taken. Here
/// the read that brings them names the account before the switch, and taken whole it would put
/// away the one warning the switch gave. Numbers move several times a minute, so within seconds
/// of the switch.
#[test]
fn numbers_moving_leave_who_is_signed_in_and_what_a_switch_said() {
    let lagging = warning("config_write_failed", "the config did not update");
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
        claude("a", false, 10.0),
        claude("b", true, 10.0),
    ])));
    machine.switched = switched("claude", "a", "b", vec![lagging.clone()]);
    switch(&mut model, &mut machine, "claude/b");
    assert_eq!(
        model.state.last_switches[0].warnings,
        std::slice::from_ref(&lagging)
    );

    machine.offline = Ok(status(vec![
        claude("a", true, 10.0),
        claude("b", false, 30.0),
    ]));
    machine.readings += 1;
    model.notice(&mut machine);

    let shown = model.shown();
    assert_eq!(percent(&shown, 1), Some(30.0));
    let signed_in: Vec<bool> = model
        .state
        .status
        .as_ref()
        .expect("accounts")
        .accounts
        .iter()
        .map(|a| a.signed_in)
        .collect();
    assert_eq!(signed_in, [false, true], "only the numbers moved");
    assert_eq!(
        model.state.last_switches[0].warnings,
        [lagging],
        "and what the switch said stands"
    );
}

/// A switch this app has in flight is its own change, so the poll leaves it alone. Numbers a
/// session records meanwhile are somebody else's, and a switch that fails reads nothing
/// after it: seen then, they were never shown.
#[test]
fn numbers_recorded_during_a_switch_are_taken_once_it_is_over() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![claude("work", true, 20.0)])));
    machine.switched = Err(refusal("nothing_parked", "nothing parked", Vec::new()));
    model.refresh(&mut machine);
    model.notice(&mut machine);

    model.send(Intent::SwitchTo {
        qualified: "claude/personal".into(),
    });
    model.run_but(&mut machine, a_switch);
    machine.offline = Ok(status(vec![claude("work", true, 22.0)]));
    machine.readings += 1;
    model.look();
    model.run_but(&mut machine, a_switch);
    let held = model.take(a_switch);
    model.give(machine.answer(held));
    assert_eq!(percent(&model.shown(), 0), Some(20.0));

    model.notice(&mut machine);
    assert_eq!(percent(&model.shown(), 0), Some(22.0));
}

/// Two tools can each have a `work`, and the switch names the one meant, with its tool, as the
/// row gives it. Forgetting comes with the account changes, and the row's name with what the
/// window says.
#[test]
fn two_tools_work_accounts_are_switched_by_their_own_name() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
        claude("work", true, 0.0),
        claude("personal", false, 0.0),
        codex_account("work", false),
        codex_account("spare", true),
    ])));
    model.refresh(&mut machine);
    let codex_work = model
        .state
        .status
        .clone()
        .expect("accounts")
        .accounts
        .into_iter()
        .find(|account| account.provider == "codex" && account.label.as_deref() == Some("work"))
        .and_then(|account| account.qualified)
        .expect("Codex's work");
    switch(&mut model, &mut machine, &codex_work);
    assert_eq!(machine.switched_to, ["codex/work"]);
}

/// What `change` does while a read started before it waits on a service: the read lands
/// after it, and is dropped.
///
/// A read that started before a change this app makes is dropped when it lands. A read waits on
/// a service with who was signed in when it started, and whatever is changed meanwhile is
/// changed before the read lands. Taken as it was, the read showed the accounts as they had
/// been and put away what the change had said, a Codex switch's warning that sessions keep the
/// account it left among it. It is dropped: the read the change starts itself says what is true
/// now.
pub(super) fn a_read_that_started_before(change: impl FnOnce(&mut Hand, &mut Stub)) {
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
    let mut machine = Stub::reading(Ok(status(before.clone())));
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
    model.later(Duration::from_secs(3));
    change(&mut model, &mut machine);
    assert_eq!(model.state.status.as_ref(), Some(&after));
    let said = (
        model.state.last_switches.clone(),
        model.state.warnings.clone(),
        model.state.updated_ms,
    );

    model.later(Duration::from_secs(3));
    model.give(held);
    assert_eq!(
        model.state.status.as_ref(),
        Some(&after),
        "not the accounts as they were before the change"
    );
    assert_eq!(
        (
            model.state.last_switches.clone(),
            model.state.warnings.clone(),
            model.state.updated_ms,
        ),
        said
    );
    assert!(!model.shown().reading);
}

/// The case the stale read was found in: a Codex switch whose warning, that two sessions
/// still use the account it parked and must not sign out, the read put away.
#[test]
fn a_read_that_started_before_a_switch_is_dropped_when_it_lands() {
    a_read_that_started_before(|model, machine| {
        machine.switched = switched(
            "codex",
            "codex/personal",
            "codex/work",
            vec![still_running()],
        );
        switch(model, machine, "codex/work");
        assert_eq!(tos(model), ["codex/work"]);
        assert_eq!(model.state.last_switches[0].warnings, [still_running()]);
    });
}

#[test]
fn a_read_that_started_before_giving_up_on_a_switch_is_dropped_when_it_lands() {
    a_read_that_started_before(|model, machine| {
        machine.abandoned = Ok(GaveUp {
            switch: Some(Abandoned {
                from: "personal".into(),
                to: "work".into(),
                logins_kept: 2,
            }),
            ..GaveUp::default()
        });
        model.send(Intent::AbandonStuckSwitch);
        model.run(machine);
    });
}

/// The same for a read that fails: started before a switch and landing after it, what it
/// failed on is the machine as it was, and it is not said over the read the switch started.
#[test]
fn a_failed_read_that_started_before_a_switch_says_nothing_once_it_lands() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
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
    machine.answer = Ok(after.clone());
    machine.switched = switched(
        "codex",
        "codex/personal",
        "codex/work",
        vec![still_running()],
    );
    switch(&mut model, &mut machine, "codex/work");

    model.give(held);
    assert_eq!(model.state.failure, None);
    assert!(model.state.warnings.is_empty());
    assert_eq!(model.state.status, Some(after));
    assert_eq!(model.state.last_switches[0].warnings, [still_running()]);
}

/// `changing.rs`'s look landing after a change, for giving up on an interrupted switch, which
/// writes the account index as it keeps every login the switch named.
#[test]
fn a_look_landing_after_giving_up_leaves_the_read_after_it() {
    let after = vec![
        codex_account("personal", true),
        codex_account("spare", false),
    ];
    a_look_landing_after(after, |model, machine| {
        model.send(Intent::AbandonStuckSwitch);
        model.run_but(machine, a_look_or_a_read);
    });
}

/// A switch that fails can still have moved who is signed in, by finishing a switch that was
/// interrupted before it. The poll leaves the account index alone while a switch runs, since
/// the change could be the switch's own, and a failed switch reads nothing after it, so the
/// poll finds the change on its next look rather than counting it as seen.
#[test]
fn a_change_the_poll_saw_during_a_switch_is_noticed_once_the_switch_is_over() {
    let before = status(vec![
        claude("work", true, 0.0),
        claude("personal", false, 0.0),
    ]);
    let finished = status(vec![
        claude("work", false, 0.0),
        claude("personal", true, 0.0),
    ]);
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(before.clone()));
    machine.switched = Err(refusal(
        "parked_login_expired",
        "spare's parked login has expired",
        vec![warning(
            "interrupted_switch_finished",
            "An interrupted switch to personal was finished.",
        )],
    ));
    model.refresh(&mut machine);

    model.send(Intent::SwitchTo {
        qualified: "claude/spare".into(),
    });
    model.run_but(&mut machine, a_switch);
    machine.changed += 1;
    machine.offline = Ok(finished.clone());
    model.look();
    model.run_but(&mut machine, a_switch);
    let held = model.take(a_switch);
    model.give(machine.answer(held));
    assert_eq!(
        model.state.status,
        Some(before),
        "left alone while the switch ran"
    );
    assert_eq!(model.count(offline_read), 0);

    model.notice(&mut machine);
    assert_eq!(model.state.status, Some(finished));
    assert_eq!(model.count(offline_read), 1);
}

/// One switch at a time. Another chosen from the menu or a notification while one runs was
/// chosen from a menu that did not yet show the first, and would switch again behind it.
#[test]
fn a_switch_asked_for_while_one_runs_does_nothing() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
        claude("personal", true, 0.0),
        claude("work", false, 0.0),
        claude("spare", false, 0.0),
    ])));
    machine.switched = switched("claude", "work", "personal", Vec::new());
    model.send(Intent::SwitchTo {
        qualified: "claude/personal".into(),
    });
    model.run_but(&mut machine, a_switch);
    assert_eq!(
        model.send(Intent::SwitchTo {
            qualified: "claude/spare".into(),
        }),
        []
    );
    model.run(&mut machine);
    let shown = model.shown();
    assert_eq!(machine.switched_to, ["claude/personal"]);
    assert_eq!(model.state.presented, None);
    assert_eq!(shown.window_request.serial, 0);
}

/// The switch is claimed as it is asked for, before what holds the login is asked about, so
/// the menu and the rows hold back at once.
#[test]
fn a_switch_is_claimed_before_anything_is_asked() {
    let mut model = Hand::new();
    let jobs = model.send(Intent::SwitchTo {
        qualified: "codex/work".into(),
    });
    assert_eq!(
        jobs,
        [Job::Holding {
            qualified: "codex/work".into(),
            then: After::Switch,
        }]
    );
    assert_eq!(model.state.switch_under_way(), Some("codex/work"));
}

/// A switch is under way until the read after it has landed, as the Swift model's was until
/// its read returned: another switch asked for meanwhile does nothing, and the poll leaves
/// the index alone, since what changed is the switch's own.
#[test]
fn a_switch_is_under_way_until_the_read_after_it_lands() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(vec![
        claude("work", true, 0.0),
        claude("personal", false, 0.0),
    ])));
    machine.switched = switched("claude", "personal", "work", Vec::new());
    model.refresh(&mut machine);
    model.send(Intent::SwitchTo {
        qualified: "claude/work".into(),
    });
    model.run_but(&mut machine, any_read);
    let read = model.take(any_read);
    let shown = model.shown();
    assert_eq!(model.state.switch_under_way(), Some("claude/work"));
    assert!(shown.reading);
    assert_eq!(
        model.state.updated_ms, None,
        "what was read before the switch is gone"
    );
    assert_eq!(
        model.send(Intent::SwitchTo {
            qualified: "claude/personal".into(),
        }),
        []
    );
    machine.changed += 1;
    model.notice(&mut machine);
    assert_eq!(model.count(offline_read), 0, "the switch's own change");

    model.give(machine.answer(read));
    assert_eq!(model.state.switch_under_way(), None);
    assert!(model.state.updated_ms.is_some());
    assert_eq!(machine.switched_to, ["claude/work"]);
}

/// A failure is numbered, one higher than the last, so an app says each once and tells a new
/// one from one it has shown, and it stays until another takes its place.
#[test]
fn each_failure_is_numbered_so_an_app_says_it_once() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.switched = Err(refusal("nothing_parked", "nothing parked", Vec::new()));
    switch(&mut model, &mut machine, "claude/spare");
    assert_eq!(model.state.presented.clone().map(|f| f.id), Some(1));
    model.refresh(&mut machine);
    assert_eq!(
        model.state.presented.clone().map(|f| f.id),
        Some(1),
        "still there"
    );

    machine.switched = Err(refusal(
        "label_unknown",
        "no account is called x",
        Vec::new(),
    ));
    switch(&mut model, &mut machine, "claude/x");
    let failure = model.state.presented.clone().expect("the newer failure");
    assert_eq!(
        (failure.id, failure.title.as_str()),
        (2, words::could_not_switch("claude/x").as_str())
    );
    assert_eq!(model.shown().window_request.serial, 2);
}

// An app that holds a tool's login.

/// A Codex machine with ChatGPT open and running Codex's login, about to switch to `spare`.
fn chatgpt_open(quits: bool) -> (Hand, Stub) {
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.held.insert("codex".into(), vec![chatgpt_holding()]);
    machine.switched = switched("codex", "codex/main", "codex/spare", Vec::new());
    machine.apps = StandInApps::new(&[CHATGPT], quits);
    (Hand::new(), machine)
}

/// Switched under it, ChatGPT goes on with the account left behind, and its own Log Out
/// would revoke the login Pitboard has just parked. So the switch waits to be told, asked in
/// the window, and holds every other switch back meanwhile.
#[test]
fn a_switch_waits_for_the_app_holding_the_login_to_be_quit() {
    let (mut model, mut machine) = chatgpt_open(true);
    switch(&mut model, &mut machine, "codex/spare");
    assert_eq!(
        model.state.asking(),
        Some(&QuitQuestion {
            qualified: "codex/spare".into(),
            app_id: CHATGPT.into(),
            name: "ChatGPT".into(),
        })
    );
    let shown = model.shown();
    assert!(machine.switched_to.is_empty());
    assert!(
        machine.apps.asked().is_empty(),
        "nothing is quit before the person says so"
    );
    assert_eq!(
        shown.window_request.pane,
        Some(Pane::Accounts),
        "asked in the window"
    );
    assert_eq!(model.state.switch_under_way(), Some("codex/spare"));
}

#[test]
fn quitting_the_app_switches_and_opens_it_again() {
    let (mut model, mut machine) = chatgpt_open(true);
    switch(&mut model, &mut machine, "codex/spare");
    model.send(quit_and_switch("codex/spare"));
    model.run(&mut machine);
    assert_eq!(
        machine.apps.asked(),
        [format!("quit {CHATGPT}"), format!("open {CHATGPT}")]
    );
    assert_eq!(machine.switched_to, ["codex/spare"]);
    let shown = model.shown();
    assert_eq!(shown.quit_question, None);
    assert_eq!(model.state.switch_under_way(), None);
    assert_eq!(model.state.presented, None);
}

/// Pitboard opens the app as soon as the switch is made, and not after the read that
/// follows it, which can wait on the network; and it is under way until that read lands.
#[test]
fn the_app_is_opened_again_before_the_read_after_the_switch() {
    let (mut model, mut machine) = chatgpt_open(true);
    model.refresh(&mut machine);
    switch(&mut model, &mut machine, "codex/spare");
    model.send(quit_and_switch("codex/spare"));
    let quit = model.next();
    let jobs = model.give(machine.answer(quit));
    let switch = model.take(a_switch);
    assert!(matches!(
        jobs[..],
        [Job::Switch {
            reopen: Some(_),
            ..
        }]
    ));
    let after = model.give(machine.answer(switch));
    assert!(
        matches!(after[..], [Job::Open { .. }, Job::Read { .. }]),
        "{after:?}"
    );
    assert_eq!(model.state.switch_under_way(), Some("codex/spare"));
}

/// Pitboard closed it, so Pitboard opens it, whether or not the switch worked.
#[test]
fn the_app_is_opened_again_when_the_switch_fails() {
    let (mut model, mut machine) = chatgpt_open(true);
    machine.switched = Err(refusal(
        "parked_login_expired",
        "spare's login has expired.",
        Vec::new(),
    ));
    switch(&mut model, &mut machine, "codex/spare");
    model.send(quit_and_switch("codex/spare"));
    model.run(&mut machine);
    assert_eq!(
        machine.apps.asked(),
        [format!("quit {CHATGPT}"), format!("open {CHATGPT}")]
    );
    let failure = model.state.presented.clone().expect("the failure is said");
    assert_eq!(failure.message, "spare's login has expired.");
}

/// An app busy with work, or whose person said no, stays open, and nothing changes.
#[test]
fn an_app_that_does_not_quit_stops_the_switch_before_anything_changes() {
    let (mut model, mut machine) = chatgpt_open(false);
    switch(&mut model, &mut machine, "codex/spare");
    model.send(quit_and_switch("codex/spare"));
    model.run(&mut machine);
    assert_eq!(
        machine.apps.asked(),
        [format!("quit {CHATGPT}")],
        "and it is not opened: it never closed"
    );
    assert!(machine.switched_to.is_empty());
    assert_eq!(model.state.switch_under_way(), None);
    let failure = model.state.presented.clone().expect("the failure is said");
    assert_eq!(failure.title, words::could_not_switch("codex/spare"));
    assert_eq!(
        failure.message,
        words::still_open("ChatGPT", "switch again"),
        "{failure:?}"
    );
}

#[test]
fn keeping_the_app_open_switches_nothing() {
    let (mut model, mut machine) = chatgpt_open(true);
    switch(&mut model, &mut machine, "codex/spare");
    assert_eq!(model.send(Intent::KeepAppOpen), []);
    let shown = model.shown();
    assert_eq!(shown.quit_question, None);
    assert_eq!(model.state.switch_under_way(), None);
    assert!(machine.switched_to.is_empty());
    assert!(machine.apps.asked().is_empty());
}

/// WindowAlerts.swift's alert closes the question from its binding and switches from its
/// button on a task of its own, which AppModel.swift's quitAndSwitch says runs once the alert
/// has closed it, and so takes the question it answers rather than reading it. The answer
/// names its question here for the same reason: an app whose alert says it closed before or
/// after its button acts sends `KeepAppOpen` and `QuitAndSwitch` in either order, and the
/// switch is made either way.
#[test]
fn the_answer_to_quit_switches_whether_the_question_closed_before_or_after_it() {
    let closed_first = [Intent::KeepAppOpen, quit_and_switch("codex/spare")];
    let answered_first = [quit_and_switch("codex/spare"), Intent::KeepAppOpen];
    for answers in [closed_first, answered_first] {
        let (mut model, mut machine) = chatgpt_open(true);
        switch(&mut model, &mut machine, "codex/spare");
        for answer in answers.clone() {
            model.send(answer);
        }
        assert_eq!(model.shown().quit_question, None, "{answers:?}");
        assert_eq!(
            model.state.switch_under_way(),
            Some("codex/spare"),
            "{answers:?}"
        );
        model.run(&mut machine);
        assert_eq!(
            machine.apps.asked(),
            [format!("quit {CHATGPT}"), format!("open {CHATGPT}")],
            "{answers:?}"
        );
        assert_eq!(machine.switched_to, ["codex/spare"], "{answers:?}");
        assert_eq!(model.state.presented, None, "{answers:?}");
    }
}

/// An answer is to the question last asked, and is taken once: one naming another account,
/// a second one, and one to a question closed unanswered that another switch asked for since
/// has taken the place of, each does nothing, so no answer starts a second switch.
#[test]
fn an_answer_to_quit_is_taken_once_and_only_for_the_question_asked() {
    let (mut model, mut machine) = chatgpt_open(true);
    switch(&mut model, &mut machine, "codex/spare");
    assert_eq!(
        model.send(quit_and_switch("codex/main")),
        [],
        "not the question asked"
    );
    assert!(model.shown().quit_question.is_some());
    let quit = model.send(quit_and_switch("codex/spare"));
    assert!(matches!(quit[..], [Job::Quit { .. }]), "{quit:?}");
    assert_eq!(model.send(quit_and_switch("codex/spare")), [], "taken once");
    model.run(&mut machine);
    assert_eq!(machine.switched_to, ["codex/spare"]);

    switch(&mut model, &mut machine, "codex/main");
    assert!(
        model.shown().quit_question.is_some(),
        "ChatGPT is open again"
    );
    model.send(Intent::KeepAppOpen);
    machine.switched = already_active("personal", Vec::new());
    switch(&mut model, &mut machine, "claude/personal");
    assert_eq!(
        model.send(quit_and_switch("codex/main")),
        [],
        "another switch was asked for in its place"
    );
    assert_eq!(model.state.switch_under_way(), None);
    assert_eq!(machine.switched_to, ["codex/spare", "claude/personal"]);
    assert_eq!(
        machine.apps.asked(),
        [format!("quit {CHATGPT}"), format!("open {CHATGPT}")]
    );
}

/// What is left of an app that has gone cannot be quit, and a tool nothing holds is
/// switched at once.
#[test]
fn only_a_running_app_holding_the_tool_is_asked_about() {
    let (mut model, mut machine) = chatgpt_open(true);
    machine.apps.set_running(&[]);
    switch(&mut model, &mut machine, "codex/spare");
    assert_eq!(model.shown().quit_question, None);
    assert_eq!(machine.switched_to, ["codex/spare"]);

    machine.apps.set_running(&[CHATGPT]);
    switch(&mut model, &mut machine, "claude/personal");
    assert_eq!(
        model.shown().quit_question,
        None,
        "ChatGPT holds Codex's login, not Claude Code's"
    );
    assert_eq!(machine.switched_to, ["codex/spare", "claude/personal"]);
}

/// Somebody who quit ChatGPT themselves before answering did not ask for it back: Pitboard
/// opens only what it closed.
#[test]
fn an_app_already_gone_is_not_opened_again() {
    let (mut model, mut machine) = chatgpt_open(true);
    switch(&mut model, &mut machine, "codex/spare");
    machine.apps.set_running(&[]);
    model.send(quit_and_switch("codex/spare"));
    model.run(&mut machine);
    assert!(machine.apps.asked().is_empty());
    assert_eq!(machine.switched_to, ["codex/spare"]);
}

/// A switch is claimed before the core is asked what holds the login, so a second one asked
/// for meanwhile waits. While the question about quitting waits, another switch brings it
/// back to the front instead of being dropped unseen, and every account holds back.
#[test]
fn one_switch_at_a_time_while_the_app_question_waits() {
    let (mut model, mut machine) = chatgpt_open(true);
    model.send(Intent::SwitchTo {
        qualified: "codex/spare".into(),
    });
    assert_eq!(
        model.send(Intent::SwitchTo {
            qualified: "claude/personal".into(),
        }),
        [],
        "the second waited for the first"
    );
    model.run(&mut machine);
    assert!(machine.switched_to.is_empty());

    let shown = model.shown();
    assert_eq!(model.state.switch_under_way(), Some("codex/spare"));
    let asked = shown.window_request.serial;
    assert_eq!(
        model.send(Intent::SwitchTo {
            qualified: "claude/personal".into(),
        }),
        []
    );
    assert!(machine.switched_to.is_empty());
    assert_eq!(
        model.shown().window_request,
        WindowRequest {
            serial: asked + 1,
            pane: Some(Pane::Accounts),
            settings: false,
        },
        "the question came back to the front"
    );
}

// Jobs that came to nothing.

/// A switch whose job panicked may or may not have moved anything, so it is said as a
/// failure of its own, the switch is over, and the app Pitboard quit for it is opened again.
#[test]
fn a_switch_lost_to_a_panic_is_said_and_over() {
    let (mut model, mut machine) = chatgpt_open(true);
    switch(&mut model, &mut machine, "codex/spare");
    model.send(quit_and_switch("codex/spare"));
    let quit = model.next();
    model.give(machine.answer(quit));
    let lost = model.take(a_switch);
    let jobs = model.give(Answer::Lost(lost));
    assert!(matches!(jobs[..], [Job::Open { .. }]), "{jobs:?}");
    assert_eq!(model.state.switch_under_way(), None);
    let failure = model.state.presented.clone().expect("said");
    assert_eq!(failure.title, words::could_not_switch("codex/spare"));
    assert_eq!(failure.code, None);
}

/// Asking what holds a login that panicked is taken as nothing holding it, as the Swift model
/// took a holding it could not read, and as the core takes a process list it cannot read;
/// asking an app to quit that panicked is taken as the app still open, so nothing is
/// switched under it.
#[test]
fn what_holds_a_login_lost_to_a_panic_holds_nothing_and_a_lost_quit_quit_nothing() {
    let (mut model, mut machine) = chatgpt_open(true);
    model.send(Intent::SwitchTo {
        qualified: "codex/spare".into(),
    });
    let asked = model.take(holding_ask);
    let jobs = model.give(Answer::Lost(asked));
    assert!(matches!(jobs[..], [Job::Switch { reopen: None, .. }]));
    model.run(&mut machine);
    assert_eq!(machine.switched_to, ["codex/spare"]);

    switch(&mut model, &mut machine, "codex/spare");
    model.send(quit_and_switch("codex/spare"));
    let quit = model.next();
    assert_eq!(model.give(Answer::Lost(quit)), []);
    assert_eq!(machine.switched_to, ["codex/spare"]);
    assert_eq!(model.state.switch_under_way(), None);
    assert!(model.state.presented.is_some());
}

// Giving up on a stuck switch.

/// Giving up on an interrupted switch deletes nothing, and what it kept is kept to say until
/// somebody puts it away. Nothing is stuck once it is given up, and every service is asked
/// again, since who is signed in is what the switch had left unknown. What the notice says is
/// presenting's.
#[test]
fn giving_up_on_a_switch_says_what_was_kept_until_put_away() {
    let interrupted = warning(
        "recovery_undetermined",
        "An interrupted switch cannot be finished until Anthropic answers.",
    );
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(warned(
        vec![claude("work", true, 0.0)],
        vec![interrupted],
    )));
    model.refresh(&mut machine);
    assert!(model.state.stuck);

    machine.answer = Ok(status(vec![claude("work", true, 0.0)]));
    let kept = |logins_kept| Abandoned {
        from: "personal".into(),
        to: "work".into(),
        logins_kept,
    };
    machine.abandoned = Ok(GaveUp {
        switch: Some(kept(2)),
        ..GaveUp::default()
    });
    assert_eq!(model.send(Intent::AbandonStuckSwitch), [Job::Abandon]);
    let given_up = model.next();
    model.give(machine.answer(given_up));
    assert!(
        !model.state.stuck,
        "nothing is stuck, before the read lands"
    );
    assert!(model.shown().reading);
    model.run(&mut machine);
    assert_eq!(model.state.abandoned, Some(kept(2)));
    assert!(!model.state.stuck);
    assert_eq!(model.state.presented, None);
    assert_eq!(model.count(fresh_read), 1, "every service asked again");

    machine.abandoned = Ok(GaveUp {
        switch: Some(kept(1)),
        ..GaveUp::default()
    });
    model.send(Intent::AbandonStuckSwitch);
    model.run(&mut machine);
    assert_eq!(model.state.abandoned, Some(kept(1)));

    model.send(Intent::DismissAbandoned);
    assert_eq!(model.state.abandoned, None);
}

/// The tests here hand the model a read that says a switch is stuck. This one has the real
/// core read a machine where a switch was interrupted and nothing can finish it, so what
/// they assume the core says is what it says: the read is not refused, it carries the
/// refusal the next change would make, and the model offers to give up on it at once.
#[test]
fn the_cores_own_read_of_a_stuck_switch_offers_to_give_up_on_it() {
    let world = super::testing::world::World::new("stuck-read");
    world.enrolled("work", "here", 10.0);
    world.parked("personal", "there", 20.0);
    world.stuck_switching_to("personal");

    let read = world.core().status(false).expect("a read is not refused");
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(read));
    model.refresh(&mut machine);

    let shown = model.shown();
    assert!(model.state.stuck);
    let notice = shown.notices.first().expect("said first");
    assert_eq!(notice.id, "stuck");
    assert!(
        notice
            .actions
            .iter()
            .any(|action| action.intent == Intent::AbandonStuckSwitch),
        "{notice:?}"
    );
    assert!(
        !shown
            .notices
            .iter()
            .any(|notice| notice.id.starts_with("warning/")),
        "and not said a second time: {:?}",
        shown.notices
    );
}

/// Every read says whether a switch is stuck, the one made when the account index moves too. A
/// switch interrupted in a terminal that nothing can finish is offered to give up on as the
/// index moves. One given up on in a terminal stopped being offered only at the next read every
/// few minutes, and offering it meanwhile offered nothing to give up on. A read that asks
/// nobody says nothing of it then, which is also what it says of a switch only a service could
/// judge, so the model asks: the read that asks says which, and until it lands what is offered
/// stays.
#[test]
fn a_stuck_switch_is_taken_from_the_read_made_when_the_index_moves() {
    let accounts = || vec![claude("work", true, 0.0)];
    let interrupted = warning("recovery_undetermined", "An interrupted switch is waiting.");
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(accounts())));
    model.refresh(&mut machine);
    assert!(!model.state.stuck);

    machine.offline = Ok(warned(accounts(), vec![interrupted.clone()]));
    machine.changed = 42;
    model.notice(&mut machine);
    assert!(model.state.stuck, "offered as the index moves");
    assert_eq!(model.count(any_read), 1, "asking nobody");

    // Given up on in a terminal.
    machine.offline = Ok(status(accounts()));
    machine.changed = 43;
    model.look();
    model.run_but(&mut machine, any_read);
    assert!(model.state.stuck, "until the read that asks says otherwise");
    assert_eq!(model.pending(), 1);
    model.run(&mut machine);
    assert!(!model.state.stuck);
    assert_eq!(model.count(any_read), 2);
    assert_eq!(model.count(fresh_read), 0, "asked as the timer asks");

    // Nothing was stuck, so a read that asks nobody and says nothing of it is the end of it.
    machine.changed = 44;
    model.notice(&mut machine);
    assert_eq!(model.count(any_read), 2);

    // Stuck where only the service can tell, which the read that asks says.
    machine.answer = Ok(warned(accounts(), vec![interrupted]));
    model.refresh(&mut machine);
    assert!(model.state.stuck);
    machine.changed = 45;
    model.notice(&mut machine);
    assert!(model.state.stuck, "kept");
    assert_eq!(model.count(any_read), 4);
}

/// Giving up that is refused is said in the window, as the panel's button said it, and
/// leaves the switch stuck and nothing read.
#[test]
fn giving_up_that_is_refused_is_said_in_the_window() {
    let interrupted = warning("recovery_undetermined", "An interrupted switch is waiting.");
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(warned(
        vec![claude("work", true, 0.0)],
        vec![interrupted],
    )));
    model.refresh(&mut machine);
    machine.abandoned = Err(refusal(
        "lock_busy",
        "another Pitboard is changing accounts",
        Vec::new(),
    ));
    model.send(Intent::AbandonStuckSwitch);
    model.run(&mut machine);
    let shown = model.shown();
    let failure = model.state.presented.clone().expect("the failure is said");
    assert_eq!(failure.title, words::COULD_NOT_GIVE_UP);
    assert_eq!(failure.message, "another Pitboard is changing accounts");
    assert_eq!(failure.code.as_deref(), Some("lock_busy"));
    assert_eq!(shown.window_request.serial, 1);
    assert!(model.state.stuck);
    assert_eq!(model.count(any_read), 1, "only the read before it");
}

/// Giving up when there was nothing to give up on says nothing was kept.
#[test]
fn giving_up_on_nothing_keeps_nothing_to_say() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.abandoned = Ok(GaveUp::default());
    model.send(Intent::AbandonStuckSwitch);
    model.run(&mut machine);
    assert_eq!(model.state.abandoned, None);
    assert_eq!(model.state.presented, None);
}

// Switching an app's login: Claude Desktop's.

fn add_sheet() -> Sheet {
    Sheet::Add {
        provider: Some("desktop".into()),
    }
}

// travel's add waiting, work parked for it, and Claude since opened by the person
fn waiting_in_claude() -> (Hand, Stub, u64) {
    let mut model = Hand::new();
    let mut machine = desktop_machine(false);
    machine.put_back = Some("desktop/work".into());
    model.refresh(&mut machine);
    model.send(Intent::PresentSheet { sheet: add_sheet() });
    model.run(&mut machine);
    model.send(Intent::SignIn {
        provider: "desktop".into(),
        name: "travel".into(),
    });
    model.run(&mut machine);
    let id = model.shown().signing_in.expect("waiting").id;
    machine.apps.set_running(&[CLAUDE]);
    (model, machine, id)
}

fn desktop_switched() -> Result<crate::Switched, super::testing::records::Refusal> {
    switched("desktop", "desktop/work", "desktop/home", Vec::new())
}

// What a notice's buttons send: the sheet that adds the account Claude shows, and giving up.
fn notice_actions(model: &Hand, starts: &str) -> Vec<Intent> {
    model
        .shown()
        .notices
        .into_iter()
        .find(|notice| notice.id.starts_with(starts))
        .expect("the notice")
        .actions
        .into_iter()
        .map(|action| action.intent)
        .collect()
}

fn add_shown() -> Intent {
    Intent::PresentSheet { sheet: add_sheet() }
}

#[test]
fn a_switch_reopens_the_app_behind_naming_its_tool() {
    let mut model = Hand::new();
    let mut machine = desktop_machine(true);
    machine.switched = desktop_switched();
    model.refresh(&mut machine);
    switch(&mut model, &mut machine, "desktop/home");
    assert_eq!(
        model.state.asking().map(|asked| asked.name.as_str()),
        Some("Claude")
    );
    model.send(quit_and_switch("desktop/home"));
    model.run(&mut machine);
    assert!(model.asked.contains(&Job::Open {
        location: StandInApps::copy_of(CLAUDE),
        tool: "desktop".into(),
        front: false,
    }));
    assert_eq!(
        machine.apps.asked(),
        [format!("quit {CLAUDE}"), format!("open {CLAUDE}")]
    );
}

#[test]
fn put_back_that_cannot_quit_claude_keeps_the_wait_and_says_why() {
    for (quits, lingers, message) in [
        (false, false, words::still_open("Claude", "switch again")),
        (
            true,
            true,
            words::part_still_running(OS, "Claude", "switch again"),
        ),
    ] {
        let (mut model, mut machine, id) = waiting_in_claude();
        machine.apps = StandInApps::new(&[CLAUDE], quits);
        machine.lingers = lingers;
        switch(&mut model, &mut machine, "desktop/work");
        model.send(quit_and_switch("desktop/work"));
        model.run(&mut machine);
        let shown = model.on_a_mac();
        let alert = shown.failure_alert.expect("said in an alert");
        assert_eq!(alert.title, words::could_not_switch("desktop/work"));
        assert_eq!(alert.message, message);
        assert_eq!(shown.sheet, Some(add_sheet()), "{message}");
        assert_eq!(shown.signing_in.map(|signing| signing.id), Some(id));
        assert!(machine.switched_to.is_empty());
        assert!(!model.asked.contains(&Job::StopSignIn { id }));
    }
}

#[test]
fn a_use_refused_in_doubt_is_kept_for_the_add_sheet() {
    let mut model = Hand::new();
    let mut machine = desktop_machine(false);
    machine.switched = Err(refusal("login_in_doubt", "in doubt", Vec::new()));
    model.refresh(&mut machine);
    switch(&mut model, &mut machine, "desktop/home");
    assert!(model.state.presented.is_some());
    model.send(Intent::PresentSheet { sheet: add_sheet() });
    model.run(&mut machine);
    assert_eq!(
        model.on_a_mac().sheet_text.expect("the form").confirm,
        words::add_account_shown(Os::MacOs, "Claude", false)
    );
    model.send(Intent::CloseSheet);
    machine.switched = desktop_switched();
    switch(&mut model, &mut machine, "desktop/home");
    model.send(Intent::PresentSheet { sheet: add_sheet() });
    model.run(&mut machine);
    assert_eq!(
        model.on_a_mac().sheet_text.expect("the form").confirm,
        words::park("work"),
        "a change of it that lands clears it"
    );
}

#[test]
fn an_app_that_leaves_part_of_itself_running_changes_nothing_and_is_opened_again() {
    let mut model = Hand::new();
    let mut machine = desktop_machine(true);
    machine.lingers = true;
    model.refresh(&mut machine);
    switch(&mut model, &mut machine, "desktop/home");
    model.send(quit_and_switch("desktop/home"));
    model.run(&mut machine);
    assert!(machine.switched_to.is_empty(), "nothing changed");
    assert_eq!(
        machine.apps.asked(),
        [format!("quit {CLAUDE}"), format!("open {CLAUDE}")]
    );
    let failure = model.state.presented.clone().expect("said");
    assert_eq!(
        failure.message,
        words::part_still_running(OS, "Claude", "switch again")
    );
    assert_eq!(model.state.switch_under_way(), None);
}

#[test]
fn putting_the_parked_account_back_stops_the_wait_only_once_claude_has_quit() {
    let (mut model, mut machine, id) = waiting_in_claude();
    machine.switched = Ok(crate::Switched {
        outcome: crate::Switch::Installed {
            provider: "desktop".into(),
            to: "desktop/work".into(),
            adoption: crate::Adoption::NextOpen { app: "Claude" },
        },
        warnings: Vec::new(),
    });
    switch(&mut model, &mut machine, "desktop/work");
    let shown = model.shown();
    assert!(shown.quit_question.is_some());
    assert_eq!(shown.signing_in.map(|signing| signing.id), Some(id));
    assert!(!model.asked.contains(&Job::StopSignIn { id }));
    model.send(quit_and_switch("desktop/work"));
    model.run(&mut machine);
    assert!(model.asked.contains(&Job::StopSignIn { id }));
    assert_eq!(machine.switched_to, ["desktop/work"]);
    let shown = model.shown();
    assert_eq!(shown.signing_in, None);
    assert_eq!(shown.sheet, None);
}

#[test]
fn keeping_claude_open_after_put_back_keeps_the_wait() {
    let (mut model, mut machine, id) = waiting_in_claude();
    switch(&mut model, &mut machine, "desktop/work");
    assert_eq!(model.send(Intent::KeepAppOpen), []);
    let shown = model.shown();
    assert_eq!(shown.signing_in.map(|signing| signing.id), Some(id));
    assert_eq!(shown.sheet, Some(add_sheet()));
    assert!(!model.asked.contains(&Job::StopSignIn { id }));
    assert!(machine.switched_to.is_empty());
}

#[test]
fn a_switch_asked_while_claude_quits_for_a_sign_in_parks_nothing() {
    let mut model = Hand::new();
    let mut machine = desktop_machine(true);
    machine.switched = desktop_switched();
    model.refresh(&mut machine);
    model.send(Intent::PresentSheet { sheet: add_sheet() });
    model.run(&mut machine);
    model.send(Intent::SignIn {
        provider: "desktop".into(),
        name: "travel".into(),
    });
    model.run(&mut machine);
    model.send(quit_and_switch("desktop/travel"));
    model.send(Intent::SwitchTo {
        qualified: "desktop/home".into(),
    });
    // The lane of processes quits Claude before it asks what holds the switch's login.
    let quit = model.take(|job| matches!(job, Job::Quit { .. }));
    let answer = machine.answer(quit);
    model.give(answer);
    model.run(&mut machine);
    assert!(machine.signed_in.is_empty(), "nothing parked");
    assert_eq!(machine.switched_to, ["desktop/home"]);
    assert!(
        model.asked.contains(&Job::Switch {
            qualified: "desktop/home".into(),
            reopen: Some(StandInApps::copy_of(CLAUDE)),
        }),
        "Claude is opened again after the switch"
    );
    let shown = model.shown();
    assert_eq!(shown.signing_in, None);
    assert_eq!(shown.sheet, None);
}

#[test]
fn claude_left_partly_running_for_an_add_opens_only_after_a_switch_asked_meanwhile() {
    // An add that parks for a sign-in, and one that records the account Claude shows.
    for recording in [false, true] {
        let mut model = Hand::new();
        let mut machine = desktop_machine(true);
        machine.lingers = true;
        machine.switched = desktop_switched();
        if recording {
            machine.answer = Ok(warned(
                vec![
                    desktop_account("work", true),
                    desktop_account("home", false),
                ],
                vec![desktop_warning(
                    "in_use_unconfirmed",
                    "Claude’s settings have named home since Pitboard last read work in use.",
                )],
            ));
        }
        model.refresh(&mut machine);
        model.send(Intent::PresentSheet { sheet: add_sheet() });
        model.run(&mut machine);
        model.send(Intent::SignIn {
            provider: "desktop".into(),
            name: "travel".into(),
        });
        model.run(&mut machine);
        model.send(quit_and_switch("desktop/travel"));
        model.send(Intent::SwitchTo {
            qualified: "desktop/home".into(),
        });
        let quit = model.take(|job| matches!(job, Job::Quit { .. }));
        let answer = machine.answer(quit);
        model.give(answer);
        model.run_but(&mut machine, a_switch);
        assert_eq!(
            machine.apps.asked(),
            [format!("quit {CLAUDE}")],
            "{recording}: nothing opens Claude while the switch is claimed"
        );
        let switch = model.take(a_switch);
        assert_eq!(
            switch,
            Job::Switch {
                qualified: "desktop/home".into(),
                reopen: Some(StandInApps::copy_of(CLAUDE)),
            },
            "{recording}"
        );
        let answer = machine.answer(switch);
        model.give(answer);
        model.run(&mut machine);
        assert_eq!(
            machine.apps.asked(),
            [format!("quit {CLAUDE}"), format!("open {CLAUDE}")],
            "{recording}"
        );
        assert!(machine.signed_in.is_empty() && machine.enrolled.is_empty());
        assert_eq!(machine.switched_to, ["desktop/home"]);
    }
}

#[test]
fn a_switch_asked_while_claude_parks_for_a_sign_in_waits_for_the_park() {
    for held_first in [true, false] {
        let mut model = Hand::new();
        let mut machine = desktop_machine(true);
        machine.put_back = Some("desktop/work".into());
        machine.switched = desktop_switched();
        model.refresh(&mut machine);
        model.send(Intent::PresentSheet { sheet: add_sheet() });
        model.run(&mut machine);
        model.send(Intent::SignIn {
            provider: "desktop".into(),
            name: "travel".into(),
        });
        model.run(&mut machine);
        model.send(quit_and_switch("desktop/travel"));
        let quit = model.next();
        let answer = machine.answer(quit);
        model.give(answer);
        let park = model.take(|job| matches!(job, Job::SignIn { .. }));
        let Job::SignIn { id, .. } = park.clone() else {
            unreachable!("taken as a sign-in");
        };
        model.send(Intent::SwitchTo {
            qualified: "desktop/home".into(),
        });
        if held_first {
            model.run(&mut machine);
            assert_eq!(model.count(a_switch), 0, "nothing switched beside the park");
        }
        let started = machine.answer(park);
        model.give(started);
        model.run_but(&mut machine, a_switch);
        let switch = model.take(a_switch);
        assert_eq!(
            switch,
            Job::Switch {
                qualified: "desktop/home".into(),
                reopen: Some(StandInApps::copy_of(CLAUDE)),
            },
            "{held_first}"
        );
        assert_eq!(
            machine.apps.asked(),
            [format!("quit {CLAUDE}")],
            "{held_first}: nothing opens Claude while the switch runs"
        );
        let answer = machine.answer(switch);
        model.give(answer);
        model.run(&mut machine);
        assert_eq!(
            machine.apps.asked(),
            [format!("quit {CLAUDE}"), format!("open {CLAUDE}")],
            "{held_first}"
        );
        assert!(
            model.asked.contains(&Job::StopSignIn { id }),
            "{held_first}"
        );
        assert_eq!(machine.switched_to, ["desktop/home"]);
        let shown = model.shown();
        assert_eq!(shown.signing_in, None);
        assert_eq!(shown.sheet, None);
    }
}

#[test]
fn claude_kept_for_a_switch_is_never_opened_by_another_tools_switch() {
    let mut model = Hand::new();
    let mut machine = desktop_machine(true);
    model.refresh(&mut machine);
    model.send(Intent::PresentSheet { sheet: add_sheet() });
    model.run(&mut machine);
    model.send(Intent::SignIn {
        provider: "desktop".into(),
        name: "travel".into(),
    });
    model.run(&mut machine);
    model.send(quit_and_switch("desktop/travel"));
    model.send(Intent::SwitchTo {
        qualified: "desktop/home".into(),
    });
    let quit = model.take(|job| matches!(job, Job::Quit { .. }));
    let answer = machine.answer(quit);
    model.give(answer);
    // Opened again by the person before the switch asks what holds its login.
    machine.apps.set_running(&[CLAUDE]);
    model.run(&mut machine);
    assert!(model.shown().quit_question.is_some());
    model.send(Intent::KeepAppOpen);
    switch(&mut model, &mut machine, "codex/work");
    assert!(model.asked.contains(&Job::Switch {
        qualified: "codex/work".into(),
        reopen: None,
    }));
    assert!(
        !model
            .asked
            .iter()
            .any(|job| matches!(job, Job::Open { tool, .. } if tool == "codex")),
        "{:?}",
        model.asked
    );
}

#[test]
fn a_question_for_a_sign_in_holds_back_no_switch() {
    let mut model = Hand::new();
    let mut machine = desktop_machine(true);
    model.refresh(&mut machine);
    model.send(Intent::PresentSheet { sheet: add_sheet() });
    model.run(&mut machine);
    model.send(Intent::SignIn {
        provider: "desktop".into(),
        name: "travel".into(),
    });
    model.run(&mut machine);
    let shown = model.shown();
    assert!(shown.quit_question.is_some());
    assert_eq!(model.state.switch_under_way(), None);
    assert!(
        rows_enabled(&model),
        "the rows do not hold back for a sign-in's question"
    );
    assert_eq!(
        model.send(Intent::SwitchTo {
            qualified: "desktop/home".into(),
        }),
        [Job::Holding {
            qualified: "desktop/home".into(),
            then: After::Switch,
        }]
    );
}

fn rows_enabled(model: &Hand) -> bool {
    model
        .on_a_mac()
        .sections
        .iter()
        .flat_map(|section| &section.accounts)
        .all(|row| !row.switching())
}

#[test]
fn a_switch_left_signed_out_does_not_reopen_claude() {
    let mut model = Hand::new();
    let mut machine = desktop_machine(true);
    machine.switched = Err(refusal(
        "switch_left_signed_out",
        "work is parked, and home couldn’t be put in use.",
        Vec::new(),
    ));
    machine.whole = false;
    model.refresh(&mut machine);
    switch(&mut model, &mut machine, "desktop/home");
    model.send(quit_and_switch("desktop/home"));
    model.run(&mut machine);
    assert!(
        model.asked.contains(&Job::Open {
            location: StandInApps::copy_of(CLAUDE),
            tool: "desktop".into(),
            front: false,
        }),
        "behind, which the lane opens only where its folder is whole"
    );
    assert_eq!(machine.apps.asked(), [format!("quit {CLAUDE}")]);
}

fn move_waiting() -> crate::Warning {
    desktop_warning(
        "move_waiting",
        "A move of work’s login was interrupted. Quit Claude, then use any Claude Desktop \
         account: Pitboard settles the move first.",
    )
}

#[test]
fn a_stopped_claude_desktop_move_offers_give_up_and_the_account_claude_shows() {
    let mut model = Hand::new();
    let mut machine = desktop_machine(false);
    machine.answer = Ok(warned(
        vec![
            desktop_account("work", false),
            desktop_account("home", false),
        ],
        vec![move_waiting()],
    ));
    machine.switched = Err(refusal("recovery_undetermined", "undetermined", Vec::new()));
    model.refresh(&mut machine);
    switch(&mut model, &mut machine, "desktop/home");
    let shown = model.on_a_mac();
    let notice = shown
        .notices
        .iter()
        .find(|notice| notice.id.starts_with("warning/move_waiting/"))
        .expect("the notice");
    assert_eq!(
        notice
            .actions
            .iter()
            .map(|action| action.intent.clone())
            .collect::<Vec<_>>(),
        [add_shown(), Intent::AbandonStuckSwitch]
    );
    assert_eq!(
        notice.actions[1]
            .confirm
            .as_ref()
            .map(|asked| asked.title.as_str()),
        Some(words::GIVE_UP_ON_MOVE)
    );
}

#[test]
fn a_waiting_move_offers_no_give_up_before_a_change_has_tried_to_settle_it() {
    let mut model = Hand::new();
    let mut machine = desktop_machine(false);
    machine.answer = Ok(warned(
        vec![
            desktop_account("work", false),
            desktop_account("home", false),
        ],
        vec![move_waiting()],
    ));
    model.refresh(&mut machine);
    assert_eq!(
        notice_actions(&model, "warning/move_waiting/"),
        [add_shown()]
    );
    machine.switched = Err(refusal("app_open", "Claude is open.", Vec::new()));
    switch(&mut model, &mut machine, "desktop/home");
    assert_eq!(
        notice_actions(&model, "warning/move_waiting/"),
        [add_shown()]
    );
}

#[test]
fn a_forget_refused_while_a_move_waits_offers_to_give_it_up() {
    let mut model = Hand::new();
    let mut machine = desktop_machine(false);
    machine.answer = Ok(warned(
        vec![
            desktop_account("work", false),
            desktop_account("home", false),
        ],
        vec![move_waiting()],
    ));
    machine.forgetting = Err(refusal("recovery_undetermined", "undetermined", Vec::new()));
    model.refresh(&mut machine);
    model.send(Intent::Forget {
        qualified: "desktop/home".into(),
    });
    model.run(&mut machine);
    assert_eq!(
        model.state.presented.clone().expect("said").title,
        words::could_not_forget("desktop/home")
    );
    assert_eq!(
        notice_actions(&model, "warning/move_waiting/"),
        [add_shown(), Intent::AbandonStuckSwitch]
    );
    machine.forgetting = Ok(());
    model.send(Intent::Forget {
        qualified: "desktop/home".into(),
    });
    model.run(&mut machine);
    assert_eq!(
        notice_actions(&model, "warning/move_waiting/"),
        [add_shown()],
        "a forget that lands settled the move first"
    );
}

#[test]
fn giving_up_gives_up_a_waiting_move_too() {
    let mut model = Hand::new();
    let mut machine = desktop_machine(false);
    machine.answer = Ok(warned(
        vec![
            desktop_account("work", false),
            desktop_account("home", false),
        ],
        vec![move_waiting()],
    ));
    machine.switched = Err(refusal("recovery_undetermined", "undetermined", Vec::new()));
    model.refresh(&mut machine);
    switch(&mut model, &mut machine, "desktop/home");
    let given = "Pitboard gave up the move of work's login and kept every folder: 1 parked, 1 set \
                 aside.";
    machine.abandoned = Ok(GaveUp {
        switch: None,
        moves: vec![given.into()],
        ..GaveUp::default()
    });
    model.send(Intent::AbandonStuckSwitch);
    model.run(&mut machine);
    let shown = model.on_a_mac();
    let notice = shown
        .notices
        .iter()
        .find(|notice| notice.id == "moves_given_up")
        .expect("what was given up");
    assert_eq!(notice.title, words::GAVE_UP_ON_MOVE);
    assert_eq!(notice.lines, [given]);
    assert_eq!(
        notice_actions(&model, "warning/move_waiting/"),
        [add_shown()],
        "what refused the last change is given up too"
    );
    model.send(Intent::DismissAbandoned);
    assert!(
        !model
            .shown()
            .notices
            .iter()
            .any(|notice| notice.id == "moves_given_up")
    );
}

#[test]
fn a_move_the_give_up_settles_is_said_beside_the_read_after_it() {
    let mut model = Hand::new();
    let mut machine = desktop_machine(false);
    machine.answer = Ok(warned(
        vec![
            desktop_account("work", false),
            desktop_account("home", false),
        ],
        vec![move_waiting()],
    ));
    machine.switched = Err(refusal("recovery_undetermined", "undetermined", Vec::new()));
    model.refresh(&mut machine);
    switch(&mut model, &mut machine, "desktop/home");
    machine.abandoned = Ok(GaveUp {
        warnings: vec![desktop_warning(
            "interrupted_move_finished",
            "Pitboard finished the interrupted move of work’s login.",
        )],
        ..GaveUp::default()
    });
    model.send(Intent::AbandonStuckSwitch);
    model.run(&mut machine);
    let shown = model.on_a_mac();
    assert!(
        !shown
            .notices
            .iter()
            .any(|notice| notice.id == "moves_given_up")
    );
    assert!(
        shown
            .notices
            .iter()
            .any(|notice| notice.id.starts_with("warning/interrupted_move_finished/")),
        "{:?}",
        shown.notices
    );
    assert_eq!(
        notice_actions(&model, "warning/move_waiting/"),
        [add_shown()],
        "what refused the last change is settled too"
    );
}

#[test]
fn a_move_the_give_up_left_waiting_is_still_offered_to_give_up() {
    let mut model = Hand::new();
    let mut machine = desktop_machine(false);
    machine.answer = Ok(warned(
        vec![
            desktop_account("work", false),
            desktop_account("home", false),
        ],
        vec![move_waiting()],
    ));
    machine.switched = Err(refusal("recovery_undetermined", "undetermined", Vec::new()));
    model.refresh(&mut machine);
    switch(&mut model, &mut machine, "desktop/home");
    machine.abandoned = Ok(GaveUp {
        switch: Some(Abandoned {
            from: "personal".into(),
            to: "work".into(),
            logins_kept: 2,
        }),
        warnings: vec![desktop_warning(
            "move_not_given_up",
            "Claude is open, so Pitboard didn’t give up the interrupted move of work’s login.",
        )],
        ..GaveUp::default()
    });
    model.send(Intent::AbandonStuckSwitch);
    model.run(&mut machine);
    let shown = model.on_a_mac();
    assert!(
        shown
            .notices
            .iter()
            .any(|notice| notice.id.starts_with("warning/move_not_given_up/")),
        "{:?}",
        shown.notices
    );
    assert_eq!(
        notice_actions(&model, "warning/move_waiting/"),
        [add_shown(), Intent::AbandonStuckSwitch],
        "what refused the last change still stands"
    );
}

#[test]
fn a_claude_desktop_warning_starts_no_read_that_asks() {
    let mut model = Hand::new();
    let mut machine = desktop_machine(false);
    model.refresh(&mut machine);
    machine.offline = Ok(warned(
        vec![
            desktop_account("work", true),
            desktop_account("home", false),
        ],
        vec![desktop_warning(
            "in_use_unconfirmed",
            "Claude’s settings have named home since Pitboard last read work in use.",
        )],
    ));
    let reads = model.count(any_read);
    machine.changed += 1;
    model.notice(&mut machine);
    assert_eq!(model.count(offline_read), 1);
    assert_eq!(model.count(any_read), reads, "no service is asked about it");
}

const REFUSED_FOR_GOOD: &str = "Anthropic no longer accepts Claude Code's login in use (its login has expired and \
     cannot be renewed), so putting work's login in its place loses nothing.";

/// A switch from a login Anthropic refuses for good asks, instead of failing, and moves
/// nothing until the answer comes.
#[test]
fn a_switch_from_a_login_refused_for_good_asks_to_replace_it() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.switched = Err(refusal("live_login_refused", REFUSED_FOR_GOOD, Vec::new()));
    switch(&mut model, &mut machine, "work");

    let shown = model.shown();
    assert_eq!(model.state.presented, None, "asked, not failed");
    let asked = shown.replace_question.expect("asked");
    assert_eq!(
        asked.confirmed,
        Intent::ReplaceAndSwitch {
            qualified: "work".into()
        }
    );
    assert_eq!(asked.question.message, REFUSED_FOR_GOOD);
    assert_eq!(asked.question.confirm, words::REPLACE);
    assert_eq!(model.state.switch_under_way(), Some("work"));
    assert!(machine.replaced_with.is_empty());
}

/// Agreed to, the account's login goes in, and the question goes.
#[test]
fn an_agreed_replacement_is_made_once() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.switched = Err(refusal("live_login_refused", REFUSED_FOR_GOOD, Vec::new()));
    switch(&mut model, &mut machine, "work");
    machine.switched = switched("claude", "personal", "work", Vec::new());

    model.send(Intent::ReplaceAndSwitch {
        qualified: "work".into(),
    });
    model.send(Intent::ReplaceAndSwitch {
        qualified: "work".into(),
    });
    model.run(&mut machine);

    assert_eq!(machine.replaced_with, ["work"]);
    let shown = model.shown();
    assert_eq!(shown.replace_question, None);
    assert_eq!(model.state.presented, None);
}

/// An app closes the alert before its button answers, as the quit question's does: the
/// answer is taken after the close, and nothing is replaced after a close alone.
#[test]
fn a_replacement_answered_after_the_question_closed_is_made() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.switched = Err(refusal("live_login_refused", REFUSED_FOR_GOOD, Vec::new()));
    switch(&mut model, &mut machine, "work");

    model.send(Intent::KeepLogin);
    model.run(&mut machine);
    assert_eq!(model.shown().replace_question, None, "closed");
    assert_eq!(model.state.switch_under_way(), None);
    assert!(
        machine.replaced_with.is_empty(),
        "nothing replaced on a close"
    );

    machine.switched = switched("claude", "personal", "work", Vec::new());
    model.send(Intent::ReplaceAndSwitch {
        qualified: "work".into(),
    });
    model.run(&mut machine);
    assert_eq!(machine.replaced_with, ["work"]);

    model.send(Intent::ReplaceAndSwitch {
        qualified: "work".into(),
    });
    model.run(&mut machine);
    assert_eq!(machine.replaced_with, ["work"], "taken once");
}

/// Another switch asked for in its place takes the question away.
#[test]
fn another_switch_takes_the_replace_question_away() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.switched = Err(refusal("live_login_refused", REFUSED_FOR_GOOD, Vec::new()));
    switch(&mut model, &mut machine, "work");
    model.send(Intent::KeepLogin);
    machine.switched = Err(refusal("nothing_parked", "nothing parked", Vec::new()));
    switch(&mut model, &mut machine, "spare");

    model.send(Intent::ReplaceAndSwitch {
        qualified: "work".into(),
    });
    model.run(&mut machine);
    assert!(machine.replaced_with.is_empty());
}

/// Another switch asked for while the question is up waits behind it, and the question comes
/// back to the front, as the quit question does.
#[test]
fn a_switch_asked_for_meanwhile_brings_the_replace_question_back() {
    let mut model = Hand::new();
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.switched = Err(refusal("live_login_refused", REFUSED_FOR_GOOD, Vec::new()));
    switch(&mut model, &mut machine, "work");
    let serial = model.shown().window_request.serial;

    switch(&mut model, &mut machine, "spare");

    let shown = model.shown();
    assert_eq!(machine.switched_to, ["work"], "nothing else is switched");
    assert!(shown.replace_question.is_some());
    assert!(
        shown.window_request.serial > serial,
        "the window comes to the front"
    );
}
