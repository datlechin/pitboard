//! Uninstalling Pitboard from the app, driven through `State::apply` by hand.

use super::state::{After, Answer, Job};
use super::testing::machine::{Hand, Stub, a_watch, any_read, desktop_machine};
use super::testing::platform::StandInApps;
use super::testing::records::{CLAUDE, DANA, refusal, status};
use super::{Doing, Intent, OutsideAnswer, OutsideAsk, Place};
use crate::AccountKey;
use crate::Front;
use crate::Tool;
use crate::account_windows::records::{Entry, Records};
use crate::account_windows::stores::derive;
use crate::present::testing::{account, desktop_account};
use crate::present::words;
use crate::{Account, Choice, ItemOffer, Parked, Question, RemovalShown, Uninstalled};
use pitboard_core::provider::ProviderId;
use pitboard_core::state::Key;
use std::time::Duration;

fn two_accounts() -> Stub {
    Stub::reading(Ok(status(vec![
        account(Some("work")).signed_in().uuid("w").build(),
        account(Some("personal")).uuid("p").build(),
    ])))
}

fn started(machine: &mut Stub) -> Hand {
    let mut model = Hand::new();
    model.send(Intent::Start);
    model.run(machine);
    model
}

fn removal(model: &Hand) -> Option<RemovalShown> {
    model.shown().removal
}

fn uninstalling(job: &Job) -> bool {
    matches!(job, Job::Uninstall)
}

fn claude_store(uuid: &str) -> String {
    derive(pitboard_sites::CLAUDE.store_name, uuid)
}

// Claude open, `home`'s login saved in Pitboard.
pub(super) fn claude_holding_home() -> Stub {
    let mut machine = desktop_machine(true);
    machine.answer = Ok(status(vec![
        desktop_account("work", true),
        Account {
            parked: Some(Parked {
                refresh_expires_at: None,
                session_expires_at: None,
            }),
            ..desktop_account("home", false)
        },
    ]));
    machine
}

#[test]
fn uninstall_asks_first() {
    let mut machine = two_accounts();
    let model = started(&mut machine);
    let offer = |os| ItemOffer {
        title: words::uninstall_pitboard(os),
        intent: Intent::Uninstall,
        enabled: true,
        confirm: Some(Question {
            confirm_is_default: false,
            ..Question::asking(
                words::UNINSTALL_QUESTION.into(),
                words::UNINSTALL_SAYS.into(),
                words::UNINSTALL.into(),
            )
        }),
    };
    assert_eq!(model.on_a_mac().uninstall, offer(Front::MacMenuBar));
    assert_eq!(
        model.on_windows().uninstall,
        offer(Front::WindowsNotificationArea)
    );
    assert_eq!(removal(&model), None);
}

#[test]
fn uninstall_waits_for_what_is_under_way_then_nothing_runs_again() {
    let mut machine = two_accounts();
    let mut model = started(&mut machine);
    model.send(Intent::SwitchTo {
        account: AccountKey::new(Tool::Claude, "personal"),
    });
    assert!(!model.shown().uninstall.enabled, "not while a switch runs");
    model.run(&mut machine);

    model.send(Intent::Refresh { asked: true });
    model.run_but(&mut machine, any_read);
    model.send(Intent::Uninstall);
    assert_eq!(
        removal(&model),
        Some(RemovalShown::Removing {
            title: words::UNINSTALLING.into()
        })
    );
    assert!(!model.shown().uninstall.enabled);
    let asked = model.asked.len();
    model.later(Duration::from_secs(600));
    model.tick();
    model.send(Intent::Refresh { asked: true });
    model.send(Intent::SwitchTo {
        account: AccountKey::new(Tool::Claude, "work"),
    });
    assert_eq!(model.asked.len(), asked, "nothing more is asked");
    assert_eq!(model.count(uninstalling), 0, "until the read is over");

    model.run(&mut machine);
    assert_eq!(machine.removals, 1);
    assert_eq!(removal(&model), Some(RemovalShown::Done));
    let asked = model.asked.len();
    for _ in 0..10 {
        model.later(Duration::from_secs(600));
        model.tick();
        model.run(&mut machine);
    }
    model.send(Intent::Woke);
    model.send(Intent::DeclineTip {
        tip: crate::Tip::AtLimit,
    });
    assert!(!model.shown().uninstall.enabled, "nothing left to remove");
    model.send(Intent::Uninstall);
    assert_eq!(model.asked.len(), asked, "{:?}", &model.asked[asked..]);
}

#[test]
fn uninstall_quits_claude_first_where_it_holds_a_saved_sign_in_and_opens_it_after() {
    let mut machine = claude_holding_home();
    let mut model = started(&mut machine);
    let jobs = model.send(Intent::Uninstall);
    assert_eq!(
        jobs,
        [Job::Holding {
            account: Key::new(ProviderId::Desktop, ""),
            then: After::Uninstall,
        }]
    );
    model.run(&mut machine);
    assert_eq!(machine.removals, 0, "nothing removed before the answer");
    let quit = model.on_a_mac().quit_question.expect("asked first");
    assert_eq!(quit.question.title, words::quit_to_uninstall("Claude"));
    assert_eq!(quit.question.message, words::quit_to_delete("Claude"));
    assert_eq!(
        quit.question.confirm,
        words::quit_and_uninstall(Front::MacMenuBar, "Claude")
    );

    model.send(quit.confirmed);
    model.run(&mut machine);
    assert_eq!(machine.removals, 1);
    assert_eq!(
        machine.apps.asked(),
        [format!("quit {CLAUDE}"), format!("open {CLAUDE}")]
    );
    assert_eq!(removal(&model), Some(RemovalShown::Done));
}

#[test]
fn keeping_claude_open_uninstalls_nothing_and_pitboard_goes_on() {
    let mut machine = claude_holding_home();
    let mut model = started(&mut machine);
    model.send(Intent::Uninstall);
    model.run(&mut machine);
    let quit = model.shown().quit_question.expect("asked first");
    model.send(quit.declined);
    model.run(&mut machine);
    assert_eq!(machine.removals, 0);
    assert!(machine.apps.asked().is_empty());
    assert_eq!(removal(&model), None);
    assert!(model.shown().uninstall.enabled);

    let reads = model.count(any_read);
    model.later(Duration::from_secs(300));
    model.tick();
    assert_eq!(model.count(any_read), reads + 1, "the timer's read again");
}

#[test]
fn claude_still_open_uninstalls_nothing_and_says_why() {
    let mut machine = claude_holding_home();
    machine.apps = StandInApps::new(&[CLAUDE], false);
    let mut model = started(&mut machine);
    model.send(Intent::Uninstall);
    model.run(&mut machine);
    let quit = model.shown().quit_question.expect("asked first");
    model.send(quit.confirmed);
    model.run(&mut machine);
    assert_eq!(machine.removals, 0);
    assert_eq!(removal(&model), None);
    let alert = model.shown().failure_alert.expect("said");
    assert_eq!(alert.title, words::could_not(&Doing::Uninstall));
    assert_eq!(alert.message, words::still_open("Claude", "try again"));
}

#[test]
fn uninstall_closes_every_window_and_deletes_every_store_once_the_accounts_are_gone() {
    let (work, personal) = (claude_store("w"), claude_store("p"));
    let mut machine = two_accounts();
    let mut records = Records::default();
    records.set(
        DANA,
        &Entry {
            stores: [work.clone(), personal.clone()].into(),
            ..Entry::default()
        },
    );
    machine.windows_file = records.text();
    let mut model = started(&mut machine);
    model.send(Intent::WindowOpened {
        store: work.clone(),
    });
    model.run(&mut machine);

    model.send(Intent::Uninstall);
    let shown = model.shown().account_windows;
    assert_eq!(shown.closing, std::slice::from_ref(&work));
    assert!(shown.deleting.is_empty(), "not before the core");
    model.send(Intent::WindowClosed {
        store: work.clone(),
    });
    model.run(&mut machine);
    assert_eq!(machine.removals, 1);
    let deleting: Vec<String> = (model.shown().account_windows.deleting.into_iter())
        .map(|asked| asked.store)
        .collect();
    assert_eq!(deleting.len(), 2);
    assert!(deleting.contains(&work) && deleting.contains(&personal));
    assert!(matches!(
        removal(&model),
        Some(RemovalShown::Removing { .. })
    ));

    model.send(Intent::StoreDeleted { store: work });
    assert!(matches!(
        removal(&model),
        Some(RemovalShown::Removing { .. })
    ));
    model.send(Intent::StoreHeld { store: personal });
    model.run(&mut machine);
    assert_eq!(removal(&model), Some(RemovalShown::Done));
}

#[test]
fn a_partial_uninstall_says_what_stayed_and_tries_again() {
    let mut machine = two_accounts();
    machine.removing = Ok(Uninstalled::Pending(2));
    let mut model = started(&mut machine);
    model.send(Intent::Uninstall);
    model.run(&mut machine);
    let again = Choice {
        title: words::try_again(Front::MacMenuBar),
        intent: Intent::Uninstall,
        enabled: true,
    };
    assert_eq!(
        removal(&model),
        Some(RemovalShown::Stopped {
            title: words::KEPT_ITS_FOLDER.into(),
            line: words::folder_kept(Uninstalled::Pending(2)).expect("a reason"),
            again: again.clone(),
        })
    );
    let reads = model.count(any_read);
    model.later(Duration::from_secs(600));
    model.tick();
    assert_eq!(model.count(any_read), reads, "the accounts are gone");

    machine.removing = Ok(Uninstalled::Removed);
    model.send(again.intent);
    model.run(&mut machine);
    assert_eq!(machine.removals, 2);
    assert_eq!(removal(&model), Some(RemovalShown::Done));
}

#[test]
fn a_refused_uninstall_says_why_and_pitboard_goes_on() {
    let mut machine = two_accounts();
    machine.removing = Err(refusal(
        "pitboard_open",
        "Another Pitboard is open.",
        Vec::new(),
    ));
    let mut model = started(&mut machine);
    model.send(Intent::Uninstall);
    model.run(&mut machine);
    assert_eq!(removal(&model), None);
    let alert = model.shown().failure_alert.expect("said");
    assert_eq!(alert.title, words::could_not(&Doing::Uninstall));
    assert!(model.shown().uninstall.enabled);
    model.send(Intent::Show {
        place: Some(Place::Accounts),
    });

    let reads = model.count(any_read);
    model.later(Duration::from_secs(300));
    model.tick();
    assert_eq!(model.count(any_read), reads + 1);
    assert_eq!(
        model.count(a_watch),
        2,
        "the watch the uninstall stopped, begun again"
    );
}

// A widget or a shortcut waits on its request's answer.
#[test]
fn a_request_from_outside_while_uninstalling_is_answered() {
    let mut machine = two_accounts();
    let mut model = started(&mut machine);
    model.send(Intent::Uninstall);
    for (request, ask) in [
        (1, OutsideAsk::Refresh),
        (
            2,
            OutsideAsk::Switch {
                account: crate::present::glance_id("claude:p"),
            },
        ),
    ] {
        model.send(Intent::Outside { request, ask });
    }
    let answered = |request| OutsideAnswer {
        request,
        done: false,
        said: words::BEING_UNINSTALLED.into(),
        question: None,
    };
    assert_eq!(model.shown().answers, [answered(1), answered(2)]);
    model.run(&mut machine);
    assert!(machine.switched_to.is_empty());
}

#[test]
fn a_switch_from_outside_waiting_for_the_first_read_is_answered_once_uninstalling() {
    let mut machine = two_accounts();
    let mut model = Hand::new();
    model.send(Intent::Start);
    model.send(Intent::Outside {
        request: 1,
        ask: OutsideAsk::Switch {
            account: crate::present::glance_id("claude:p"),
        },
    });
    model.send(Intent::Uninstall);
    model.run(&mut machine);
    assert!(machine.switched_to.is_empty());
    assert_eq!(
        model.shown().answers,
        [OutsideAnswer {
            request: 1,
            done: false,
            said: words::BEING_UNINSTALLED.into(),
            question: None,
        }]
    );
}

// The core may have removed the accounts and let go of Pitboard's folder before the job was lost.
#[test]
fn an_uninstall_that_came_to_nothing_runs_nothing_more_and_offers_try_again() {
    let mut machine = two_accounts();
    let mut model = started(&mut machine);
    model.send(Intent::Uninstall);
    let job = model.take(uninstalling);
    model.give(Answer::Lost(job));
    let again = Choice {
        title: words::try_again(Front::MacMenuBar),
        intent: Intent::Uninstall,
        enabled: true,
    };
    assert_eq!(
        removal(&model),
        Some(RemovalShown::Stopped {
            title: words::could_not(&Doing::Uninstall),
            line: words::UNINSTALL_NOT_KNOWN.into(),
            again: again.clone(),
        })
    );
    assert_eq!(model.shown().failure_alert, None);
    let asked = model.asked.len();
    for _ in 0..10 {
        model.later(Duration::from_secs(600));
        model.tick();
        model.run(&mut machine);
    }
    model.send(Intent::Woke);
    assert_eq!(model.asked.len(), asked, "{:?}", &model.asked[asked..]);

    model.send(again.intent);
    model.run(&mut machine);
    assert_eq!(machine.removals, 1);
    assert_eq!(removal(&model), Some(RemovalShown::Done));
}
