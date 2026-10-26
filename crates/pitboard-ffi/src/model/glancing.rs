//! The glance for widgets and intents, and what they ask, driven through `State::apply` by hand.

use super::state::Job;
use super::testing::machine::{Hand, Stub, any_read};
use super::testing::platform::StandInApps;
use super::testing::records::{CHATGPT, chatgpt_holding, codex, refusal, status, switched};
use super::{Intent, OutsideAnswer, OutsideAsk};
use crate::present::testing::{LimitExt, account, unplaced, window};
use crate::present::{glance_id, words};
use crate::{Account, Status};
use std::time::Duration;

const NOW: i64 = 1_800_000_000;

fn work() -> Account {
    Account {
        plan: Some("Max 5x".into()),
        ..account(Some("work"))
            .signed_in()
            .uuid("w")
            .limits(vec![
                window("session", 42.0).resets(Some(NOW + 3_600)),
                window("weekly_all", 12.0).resets(Some(NOW + 3 * 86_400)),
            ])
            .build()
    }
}

fn personal(percent: f64) -> Account {
    account(Some("personal"))
        .uuid("p")
        .limits(vec![window("session", percent).resets(Some(NOW + 3_600))])
        .build()
}

fn two(percent: f64) -> Status {
    status(vec![work(), personal(percent)])
}

fn glancing(machine: &mut Stub) -> Hand {
    let mut model = Hand::new();
    model.state.glance_to_a_file();
    model.refresh(machine);
    model
}

fn glances(job: &Job) -> bool {
    matches!(job, Job::KeepGlance { .. })
}

fn answers(model: &Hand) -> Vec<OutsideAnswer> {
    model.shown().answers
}

fn switch_to(account: &str) -> OutsideAsk {
    OutsideAsk::Switch {
        account: glance_id(account),
    }
}

#[test]
fn nothing_is_written_without_a_glance_file() {
    let mut machine = Stub::reading(Ok(two(10.0)));
    let mut model = Hand::new();
    model.refresh(&mut machine);
    assert_eq!(model.publish().glance_revision, 0);
    assert_eq!(model.count(glances), 0);
}

// A widget reading between the launch and the first read would show no accounts.
#[test]
fn nothing_is_written_before_the_accounts_are_read() {
    let mut model = Hand::new();
    model.state.glance_to_a_file();
    model.send(Intent::Start);
    assert_eq!(model.publish().glance_revision, 0);
    assert_eq!(model.count(glances), 0);
}

#[test]
fn the_glance_is_written_once_per_change_and_never_by_the_minute() {
    let mut machine = Stub::reading(Ok(two(10.0)));
    let mut model = glancing(&mut machine);
    assert_eq!(model.publish().glance_revision, 1);
    model.publish();
    model.later(Duration::from_secs(60));
    model.tick();
    model.run(&mut machine);
    assert_eq!(
        model.publish().glance_revision,
        1,
        "the minute changes nothing"
    );
    assert_eq!(model.count(glances), 1);

    machine.answer = Ok(two(30.0));
    model.refresh(&mut machine);
    assert_eq!(model.publish().glance_revision, 2);
    model.run(&mut machine);
    assert_eq!(machine.glances.len(), 2);
}

#[test]
fn the_glance_holds_no_email_token_uuid_path_or_notice_line() {
    let line = "/Users/dana/.claude/.credentials.json is there with no Claude Code login in it.";
    let mut machine = Stub::reading(Ok(super::testing::records::warned(
        vec![
            work(),
            personal(10.0),
            account(None).signed_in().uuid("dana-uuid").build(),
            unplaced("codex", false),
        ],
        vec![super::testing::records::warning("fallback_login", line)],
    )));
    let mut model = glancing(&mut machine);
    model.publish();
    model.run(&mut machine);
    let [glance] = machine.glances.as_slice() else {
        panic!("one glance: {:?}", machine.glances);
    };
    for private in ["@", "dana-uuid", "/Users", "credentials", line, "claude:w"] {
        assert!(!glance.contains(private), "{private} in {glance}");
    }
}

#[test]
fn a_glance_is_schema_1() {
    let mut machine = Stub::reading(Ok(two(10.0)));
    let mut model = glancing(&mut machine);
    let utc = crate::present::testing::Utc;
    let text = crate::present::glance_on(
        pitboard_core::host::Os::MacOs,
        &model.state,
        model.now.epoch(),
        &utc,
    );
    let read_at = model.state.updated_ms.expect("read").div_euclid(1000);
    let golden = serde_json::json!({
        "schema": 1,
        "read_at": read_at,
        "accounts": [
            {
                "id": "35c0d1c9-6b25-5cdc-b543-64c0c0d8de46",
                "tool": "claude",
                "tool_name": "Claude Code",
                "name": "work",
                "in_use": true,
                "standing": "in_use",
                "plan": "Max 5x",
                "said": "5-hour 42% used, Weekly 12%",
                "limits": [
                    {
                        "kind": "five_hour",
                        "model": null,
                        "name": "5-hour limit",
                        "used": 42.0,
                        "level": "plenty",
                        "resets_at": NOW + 3_600
                    },
                    {
                        "kind": "weekly",
                        "model": null,
                        "name": "Weekly limit",
                        "used": 12.0,
                        "level": "plenty",
                        "resets_at": NOW + 3 * 86_400
                    }
                ]
            },
            {
                "id": "c17c8cd1-64cc-5e37-bb6c-a31d984738ec",
                "tool": "claude",
                "tool_name": "Claude Code",
                "name": "personal",
                "in_use": false,
                "standing": "ready",
                "plan": null,
                "said": "5-hour 10% used",
                "limits": [
                    {
                        "kind": "five_hour",
                        "model": null,
                        "name": "5-hour limit",
                        "used": 10.0,
                        "level": "plenty",
                        "resets_at": NOW + 3_600
                    }
                ]
            }
        ]
    });
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).expect("JSON"),
        golden
    );
    model.publish();
}

// A widget or a shortcut keeps an account by it, so it never changes, and a rename keeps it.
#[test]
fn a_glance_id_is_the_accounts_own_and_never_changes() {
    assert_eq!(
        glance_id("claude:w"),
        "35c0d1c9-6b25-5cdc-b543-64c0c0d8de46"
    );
    assert_eq!(
        glance_id("claude:4f3c2a10-8b7e-4d2a-9c1e-5a6b7c8d9e0f"),
        glance_id("CLAUDE:4F3C2A10-8B7E-4D2A-9C1E-5A6B7C8D9E0F")
    );
    let renamed = Account {
        label: Some("office".into()),
        qualified: Some("claude/office".into()),
        ..work()
    };
    assert_eq!(renamed.id, work().id);
}

#[test]
fn a_switch_asked_from_outside_answers_its_request() {
    let mut machine = Stub::reading(Ok(two(10.0)));
    let mut model = glancing(&mut machine);
    machine.switched = switched("claude", "work", "personal", Vec::new());
    model.send(Intent::Outside {
        request: 7,
        ask: switch_to("claude:p"),
    });
    model.run(&mut machine);
    assert_eq!(machine.switched_to, ["claude/personal"]);
    assert_eq!(
        answers(&model),
        [OutsideAnswer {
            request: 7,
            done: true,
            said: format!("{}.", words::switched_to(Some("Claude Code"), "personal")),
        }]
    );
    assert_eq!(model.shown().window_request.serial, 0, "no window");
}

#[test]
fn what_a_switch_from_outside_cannot_do_is_refused_in_words() {
    let mut machine = Stub::reading(Ok(two(10.0)));
    let mut model = glancing(&mut machine);
    for (request, ask) in [(1, switch_to("claude:gone")), (2, switch_to("claude:w"))] {
        model.send(Intent::Outside { request, ask });
    }
    model.send(Intent::SwitchTo {
        qualified: "claude/personal".into(),
    });
    model.send(Intent::Outside {
        request: 3,
        ask: switch_to("claude:p"),
    });
    assert_eq!(
        answers(&model),
        [
            OutsideAnswer {
                request: 1,
                done: false,
                said: words::NO_SUCH_ACCOUNT.into(),
            },
            OutsideAnswer {
                request: 2,
                done: true,
                said: words::already_in_use("work"),
            },
            OutsideAnswer {
                request: 3,
                done: false,
                said: words::SWITCH_UNDER_WAY.into(),
            },
        ]
    );
    assert!(machine.switched_to.is_empty());
}

// A widget cannot ask whether to quit ChatGPT, so Pitboard says to open it instead.
#[test]
fn a_switch_from_outside_that_would_quit_an_app_asks_nothing_and_says_so() {
    let mut machine = Stub::reading(Ok(status(vec![
        account(Some("main"))
            .of("codex")
            .signed_in()
            .uuid("m")
            .build(),
        account(Some("spare")).of("codex").uuid("s").build(),
    ])));
    machine.found = vec![codex()];
    machine.held.insert("codex".into(), vec![chatgpt_holding()]);
    machine.apps = StandInApps::new(&[CHATGPT], true);
    let mut model = glancing(&mut machine);
    model.send(Intent::Outside {
        request: 4,
        ask: switch_to("codex:s"),
    });
    model.run(&mut machine);
    assert_eq!(model.shown().quit_question, None);
    assert!(machine.apps.asked().is_empty(), "nothing quit");
    assert!(model.state.switch_under_way().is_none());
    assert_eq!(
        answers(&model),
        [OutsideAnswer {
            request: 4,
            done: false,
            said: words::open_pitboard_to_switch("ChatGPT"),
        }]
    );
}

// What the core says can name an email or a path, and Shortcuts may speak an answer aloud.
#[test]
fn a_switch_from_outside_the_core_refuses_says_only_that_it_failed_and_shows_no_alert() {
    let mut machine = Stub::reading(Ok(two(10.0)));
    let mut model = glancing(&mut machine);
    machine.switched = Err(refusal(
        "parked_login_belongs_elsewhere",
        "personal's saved sign-in belongs to dana@example.com.",
        Vec::new(),
    ));
    model.send(Intent::Outside {
        request: 5,
        ask: switch_to("claude:p"),
    });
    model.run(&mut machine);
    assert_eq!(model.shown().failure_alert, None);
    let said = words::failed_outside(&words::could_not_switch("claude/personal"));
    assert!(!said.contains('@') && !said.contains('/'), "{said}");
    assert_eq!(
        answers(&model),
        [OutsideAnswer {
            request: 5,
            done: false,
            said,
        }]
    );
}

#[test]
fn a_refresh_from_outside_that_fails_says_only_that_it_failed() {
    let mut machine = Stub::reading(Ok(two(10.0)));
    let mut model = glancing(&mut machine);
    machine.answer = Err(refusal(
        "state_unreadable",
        "/Users/dana/.pitboard/state.json could not be read.",
        Vec::new(),
    ));
    model.send(Intent::Outside {
        request: 6,
        ask: OutsideAsk::Refresh,
    });
    model.run(&mut machine);
    assert_eq!(
        answers(&model),
        [OutsideAnswer {
            request: 6,
            done: false,
            said: words::failed_outside(words::COULD_NOT_READ_USAGE),
        }]
    );
}

// A widget tapped while the person answers a question at the Mac leaves the question up.
#[test]
fn a_switch_from_outside_waits_behind_a_question_the_person_has_open() {
    let mut machine = super::removing::claude_holding_home();
    let mut model = Hand::new();
    model.state.glance_to_a_file();
    model.send(Intent::Start);
    model.run(&mut machine);
    model.send(Intent::Uninstall);
    model.run(&mut machine);
    let asked = model.shown().quit_question.expect("asked first");
    let home = (model.state.status.as_ref())
        .and_then(|read| read.accounts.iter().find(|account| !account.signed_in))
        .map(|account| account.id.clone())
        .expect("home");
    model.send(Intent::Outside {
        request: 2,
        ask: switch_to(&home),
    });
    model.run(&mut machine);
    assert_eq!(model.shown().quit_question, Some(asked));
    assert!(machine.switched_to.is_empty());
    assert_eq!(
        answers(&model),
        [OutsideAnswer {
            request: 2,
            done: false,
            said: words::QUESTION_OPEN.into(),
        }]
    );
}

#[test]
fn a_refresh_from_outside_is_answered_once_read() {
    let mut machine = Stub::reading(Ok(two(10.0)));
    let mut model = glancing(&mut machine);
    model.later(Duration::from_secs(5));
    model.send(Intent::Outside {
        request: 6,
        ask: OutsideAsk::Refresh,
    });
    assert!(answers(&model).is_empty(), "not before the read");
    let reads = model.count(any_read);
    model.run(&mut machine);
    assert_eq!(model.count(any_read), reads);
    assert_eq!(
        answers(&model),
        [OutsideAnswer {
            request: 6,
            done: true,
            said: words::updated("08:00"),
        }]
    );
}

#[test]
fn the_ten_newest_answers_are_kept() {
    let mut machine = Stub::reading(Ok(two(10.0)));
    let mut model = glancing(&mut machine);
    for request in 1..=12 {
        model.send(Intent::Outside {
            request,
            ask: switch_to("claude:gone"),
        });
    }
    let kept: Vec<u64> = answers(&model)
        .iter()
        .map(|answer| answer.request)
        .collect();
    assert_eq!(kept, (3..=12).collect::<Vec<_>>());
}

// Signing in again, and replacing a login its service refuses, are the app's questions.
#[test]
fn a_switch_from_outside_needing_a_question_only_the_app_asks_says_to_open_it() {
    let old = account(Some("old"))
        .uuid("o")
        .standing(pitboard_core::status::Standing::LoginExpired)
        .build();
    let mut machine = Stub::reading(Ok(status(vec![work(), personal(10.0), old])));
    let mut model = glancing(&mut machine);
    machine.switched = Err(refusal(
        "live_login_refused",
        "Anthropic refuses the login in use for good.",
        Vec::new(),
    ));
    model.send(Intent::Outside {
        request: 8,
        ask: switch_to("claude:o"),
    });
    model.send(Intent::Outside {
        request: 9,
        ask: switch_to("claude:p"),
    });
    model.run(&mut machine);
    assert_eq!(model.shown().replace_question, None);
    assert!(model.state.switch_under_way().is_none());
    assert_eq!(
        answers(&model),
        [
            OutsideAnswer {
                request: 8,
                done: false,
                said: words::sign_in_first("old"),
            },
            OutsideAnswer {
                request: 9,
                done: false,
                said: words::open_pitboard_to_replace("personal"),
            },
        ]
    );
}
