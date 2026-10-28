//! The app's own preferences, which the model keeps in `app.json` in Pitboard's directory, as
//! the Swift app kept them in UserDefaults: the per-tool "Not Now", and the first launch's
//! window from MenuBarLabel.swift, driven through `State::apply` by hand.

use super::preferences::Preferences;
use super::state::{Answer, Job};
use super::testing::machine::{Hand, Stub};
use super::testing::records::status;
use super::{AtLimit, EarlierPreferences, Intent, Place, SettingsSection, Tip, WindowRequest};
use crate::AccountKey;
use crate::Footing;
use crate::Tool;
use crate::present::testing::account;
use pitboard_core::provider::ProviderId;
use std::collections::BTreeSet;

/// Both tools with one account each, each in use.
fn one_of_each() -> Stub {
    Stub::reading(Ok(status(vec![
        account(Some("work")).signed_in().build(),
        account(Some("job"))
            .of(ProviderId::Codex)
            .signed_in()
            .build(),
    ])))
}

fn only_one(tool: Tool, label: &str) -> Footing {
    Footing::OnlyOne {
        account: AccountKey::new(tool, label),
    }
}

fn decline(tool: Tool) -> Intent {
    Intent::DeclineSecondAccount { tool }
}

/// A model as the app starts it, what it keeps read from `machine` and its first read in.
fn started(machine: &mut Stub) -> Hand {
    let mut model = Hand::new();
    model.send(Intent::Start);
    model.run(machine);
    model
}

fn declined(preferences: &Preferences) -> Vec<&str> {
    preferences
        .second_account_declined
        .iter()
        .map(String::as_str)
        .collect()
}

/// Keeping one Claude Code account on purpose says nothing about Codex. The prompt for a
/// second account is declined per tool, and a declined one does not stand in front of the
/// next tool's. It is remembered, in Pitboard's directory.
#[test]
fn a_second_account_is_declined_per_tool() {
    let mut machine = one_of_each();
    let mut model = started(&mut machine);
    assert_eq!(model.shown().footing, only_one(Tool::Claude, "work"));

    model.send(decline(Tool::Claude));
    model.run(&mut machine);
    assert_eq!(model.shown().footing, only_one(Tool::Codex, "job"));
    let kept = machine.kept_preferences.last().expect("kept");
    assert_eq!(declined(kept), ["claude"]);

    let mut later = started(&mut machine);
    assert_eq!(
        later.shown().footing,
        only_one(Tool::Codex, "job"),
        "and it is remembered"
    );
    later.send(decline(Tool::Codex));
    later.run(&mut machine);
    assert_eq!(later.shown().footing, Footing::Ready);
}

// Not Now is kept beside a tip an earlier version kept.
#[test]
fn setup_goes_on_to_what_happens_at_a_limit() {
    let mut machine = Stub::reading(Ok(status(vec![
        account(Some("work")).signed_in().build(),
        account(Some("personal")).build(),
    ])));
    machine.preferences_file =
        Some(r#"{"has_been_seen":true,"tips_declined":["open_at_login"]}"#.into());
    let mut model = started(&mut machine);
    assert_eq!(model.shown().footing, Footing::AtLimitUnchosen);
    model.send(Intent::DeclineTip { tip: Tip::AtLimit });
    model.run(&mut machine);
    assert_eq!(model.shown().footing, Footing::Ready);
    let kept = machine.kept_preferences.last().expect("kept");
    assert_eq!(
        kept.tips_declined
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["at_limit", "open_at_login"]
    );

    let later = started(&mut machine);
    assert_eq!(
        later.shown().footing,
        Footing::Ready,
        "and it is remembered"
    );

    let mut chose = started(&mut Stub::reading(Ok(status(vec![
        account(Some("work")).signed_in().build(),
        account(Some("personal")).build(),
    ]))));
    chose.send(Intent::SetAtLimit {
        choice: AtLimit::Notify,
        at: 95,
    });
    assert_eq!(chose.shown().footing, Footing::Ready, "a choice made");
}

/// Opening the settings from the tip takes it up, whether or not the choice shown there
/// changes: keeping Notify Me writes nothing of its own.
#[test]
fn opening_the_settings_from_the_tip_ends_it() {
    let mut machine = Stub::reading(Ok(status(vec![
        account(Some("work")).signed_in().build(),
        account(Some("personal")).build(),
    ])));
    let mut model = started(&mut machine);
    assert_eq!(model.shown().footing, Footing::AtLimitUnchosen);
    model.send(Intent::ShowLimits);
    model.run(&mut machine);
    assert_eq!(
        model.shown().window_request.place,
        Some(Place::Settings {
            section: SettingsSection::Limits
        })
    );
    assert_eq!(model.shown().footing, Footing::Ready);
    assert_eq!(
        started(&mut machine).shown().footing,
        Footing::Ready,
        "and it is remembered"
    );
}

/// "Not now" said before there was a second tool was said about Claude Code, the only tool
/// there was, and does not hide the prompt for a first Codex account. It is moved into the
/// model's file once, so the earlier store is read no more.
///
/// Removing the old key from UserDefaults is the macOS app's to do as it hands it over.
#[test]
fn a_nudge_declined_before_codex_was_about_claude_code() {
    let mut machine = one_of_each();
    machine.earlier = Some(EarlierPreferences {
        second_account_declined: Vec::new(),
        has_been_seen: true,
        second_account_nudge_hidden: true,
    });
    let model = started(&mut machine);
    assert_eq!(model.shown().footing, only_one(Tool::Codex, "job"));
    assert_eq!(declined(&model.state.preferences), ["claude"]);
    assert_eq!(
        machine
            .kept_preferences
            .iter()
            .map(declined)
            .collect::<Vec<_>>(),
        [vec!["claude"]],
        "moved once"
    );
}

/// An app with no Dock icon that launches straight into a menu bar item shows somebody who
/// has just installed it nothing at all, so the very first launch asks for the welcome, once,
/// and never again.
///
/// MenuBarLabel.swift's `hasBeenSeen`, which the Swift tested nowhere.
#[test]
fn the_first_launch_asks_for_the_welcome_once() {
    let mut machine = one_of_each();
    let first = started(&mut machine);
    assert_eq!(
        first.shown().window_request,
        WindowRequest {
            serial: 1,
            place: Some(Place::Welcome),
        }
    );
    assert!(machine.kept_preferences.last().expect("kept").has_been_seen);

    let again = started(&mut machine);
    assert_eq!(again.shown().window_request.serial, 0);

    let mut seen_before = one_of_each();
    seen_before.earlier = Some(EarlierPreferences {
        second_account_declined: Vec::new(),
        has_been_seen: true,
        second_account_nudge_hidden: false,
    });
    assert_eq!(started(&mut seen_before).shown().window_request.serial, 0);
}

/// What the app's earlier store held is kept in the model's file as soon as it is taken,
/// whatever it holds, so the next launch reads the file and not the store.
#[test]
fn the_earlier_store_is_taken_once() {
    let mut machine = one_of_each();
    machine.earlier = Some(EarlierPreferences {
        second_account_declined: vec!["codex".into()],
        has_been_seen: true,
        second_account_nudge_hidden: false,
    });
    started(&mut machine);
    assert_eq!(machine.kept_preferences.len(), 1);
    machine.earlier = Some(EarlierPreferences {
        second_account_declined: Vec::new(),
        has_been_seen: false,
        second_account_nudge_hidden: false,
    });
    let later = started(&mut machine);
    assert_eq!(
        declined(&later.state.preferences),
        ["codex"],
        "the file wins"
    );
    assert_eq!(
        machine.kept_preferences.len(),
        1,
        "and nothing more to keep"
    );
}

// At the first launch the welcome stands in for the window asked for while the preferences are read.
#[test]
fn a_window_asked_for_while_the_preferences_are_read_waits_for_them() {
    let launch = |machine: &mut Stub, place| {
        let mut model = Hand::new();
        model.send(Intent::Start);
        model.send(Intent::Show { place });
        let meanwhile = model.shown().window_request;
        model.run(machine);
        (meanwhile.serial, model.shown().window_request)
    };
    let request = |place| WindowRequest { serial: 1, place };

    let mut machine = one_of_each();
    assert_eq!(
        launch(&mut machine, None),
        (0, request(Some(Place::Welcome))),
        "the first launch"
    );
    assert_eq!(
        launch(&mut machine, None),
        (0, request(None)),
        "a later launch"
    );
    assert_eq!(
        launch(&mut machine, Some(Place::Accounts)),
        (1, request(Some(Place::Accounts))),
        "a notification's click"
    );
    let mut unreadable = one_of_each();
    unreadable.preferences_unreadable = true;
    assert_eq!(
        launch(&mut unreadable, None),
        (0, request(None)),
        "preferences that cannot be read"
    );
}

/// A "Not Now" said while the preferences are still being read is kept with what they hold,
/// once they are in, rather than in place of it.
#[test]
fn a_not_now_said_while_the_preferences_are_read_is_kept_with_them() {
    let mut machine = one_of_each();
    machine.preferences_file = Preferences {
        second_account_declined: BTreeSet::from(["codex".to_owned()]),
        has_been_seen: true,
        ..Preferences::default()
    }
    .text();
    let mut model = Hand::new();
    model.send(Intent::Start);
    let kept = model.take(|job| matches!(job, Job::LoadKept));
    assert_eq!(
        model.send(decline(Tool::Claude)),
        [],
        "kept once they are in"
    );
    let answer = machine.answer(kept);
    model.give(answer);
    model.run(&mut machine);
    assert_eq!(declined(&model.state.preferences), ["claude", "codex"]);
    assert_eq!(
        machine
            .kept_preferences
            .iter()
            .map(declined)
            .collect::<Vec<_>>(),
        [vec!["claude", "codex"]]
    );
}

/// Preferences that came to nothing are left as they are: nothing is kept in place of what
/// may be there, not even a "Not Now" said after, and the window does not open as though for
/// the first time. The "Not Now" holds for as long as the app is open.
#[test]
fn preferences_that_came_to_nothing_are_left_as_they_are() {
    let mut machine = one_of_each();
    let mut model = Hand::new();
    model.send(Intent::Start);
    let kept = model.take(|job| matches!(job, Job::LoadKept));
    model.run(&mut machine);
    model.give(Answer::Lost(kept));
    model.run(&mut machine);
    assert!(machine.kept_preferences.is_empty());
    assert_eq!(model.shown().window_request.serial, 0);

    assert_eq!(
        model.send(decline(Tool::Claude)),
        [],
        "nothing written over them"
    );
    assert_eq!(model.shown().footing, only_one(Tool::Codex, "job"));
}

/// An `app.json` that is there and cannot be read, because it is not this user's, a disk
/// failed or another program holds it, is not taken as no file: the earlier store and the
/// defaults would be written over the "Not Now"s it holds, and the window would open as
/// though for the first time. It is left as it is, and nothing said while the app is open is
/// written over it.
#[test]
fn preferences_that_cannot_be_read_are_left_as_they_are() {
    let mut machine = one_of_each();
    machine.preferences_unreadable = true;
    machine.earlier = Some(EarlierPreferences {
        second_account_declined: Vec::new(),
        has_been_seen: false,
        second_account_nudge_hidden: false,
    });
    let mut model = started(&mut machine);
    assert!(
        machine.kept_preferences.is_empty(),
        "nothing in their place"
    );
    assert_eq!(
        model.shown().window_request.serial,
        0,
        "not opened as if new"
    );

    model.send(decline(Tool::Claude));
    model.run(&mut machine);
    assert!(machine.kept_preferences.is_empty(), "nor over them later");
    assert_eq!(model.shown().footing, only_one(Tool::Codex, "job"));
}

/// No tool is nudged toward a second account while the preferences are still being read:
/// a read that lands first would show the step to somebody who declined it, for as long as
/// `app.json` takes, at every launch. The Swift read its UserDefaults as it worked out the
/// footing, and never showed it.
#[test]
fn no_tool_is_nudged_while_the_preferences_are_read() {
    let mut machine = one_of_each();
    machine.preferences_file = Preferences {
        second_account_declined: BTreeSet::from(["claude".to_owned()]),
        has_been_seen: true,
        ..Preferences::default()
    }
    .text();
    let mut model = Hand::new();
    model.send(Intent::Start);
    model.run_but(&mut machine, |job| matches!(job, Job::LoadKept));
    assert!(model.state.status.is_some(), "the read has landed");
    assert_eq!(
        model.shown().footing,
        Footing::Ready,
        "and nobody is nudged yet"
    );
    assert_eq!(model.shown().setup, None);
    model.run(&mut machine);
    assert_eq!(model.shown().footing, only_one(Tool::Codex, "job"));
}

/// A tool declined twice is kept once.
#[test]
fn a_tool_declined_twice_is_kept_once() {
    let mut machine = one_of_each();
    let mut model = started(&mut machine);
    let before = machine.kept_preferences.len();
    model.send(decline(Tool::Claude));
    model.run(&mut machine);
    assert_eq!(model.send(decline(Tool::Claude)), []);
    assert_eq!(machine.kept_preferences.len(), before + 1);
}
