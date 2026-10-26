//! What the welcome window shows at the first launch, driven through `State::apply` by hand.

use super::state::Job;
use super::testing::machine::{Hand, Stub};
use super::testing::records::{claude_code, codex, status};
use super::{Intent, Place, Sheet};
use crate::present::testing::account;
use crate::present::words;
use crate::{Choice, SetupLink, WelcomeLogin, WelcomeStep, WelcomeTool};
use pitboard_core::host::Os;

fn welcomed(machine: &mut Stub) -> Hand {
    let mut model = Hand::new();
    model.send(Intent::Start);
    model.run(machine);
    model
}

fn enrols(job: &Job) -> bool {
    matches!(job, Job::Enrol { .. })
}

fn adds(title: &str, intent: Intent) -> WelcomeStep {
    WelcomeStep::Add {
        choice: Choice {
            title: title.into(),
            intent,
            enabled: true,
        },
    }
}

#[test]
fn the_welcome_lists_each_tool_found_and_reads_no_codex_login_before_its_click() {
    let mut machine = Stub::reading(Ok(status(vec![
        account(None).signed_in().uuid("dana").build(),
    ])));
    machine.found = vec![claude_code(), codex()];
    let mut model = welcomed(&mut machine);
    assert_eq!(
        model.shown().welcome.tools,
        [
            WelcomeTool {
                code: "claude".into(),
                name: "Claude Code".into(),
                account: Some("dana@example.com".into()),
                step: adds(
                    words::ADD,
                    Intent::Enrol {
                        provider: "claude".into()
                    }
                ),
            },
            WelcomeTool {
                code: "codex".into(),
                name: "Codex".into(),
                account: None,
                step: adds(
                    words::ADD,
                    Intent::Enrol {
                        provider: "codex".into()
                    }
                ),
            },
        ]
    );
    assert_eq!(model.count(enrols), 0, "nothing read of a login yet");

    model.send(Intent::Enrol {
        provider: "codex".into(),
    });
    assert_eq!(model.count(enrols), 1, "the click reads it");
}

#[test]
fn a_tool_with_an_account_added_says_so() {
    let mut machine = Stub::reading(Ok(status(vec![
        account(Some("work")).signed_in().uuid("dana").build(),
    ])));
    let model = welcomed(&mut machine);
    assert_eq!(
        model.shown().welcome.tools,
        [WelcomeTool {
            code: "claude".into(),
            name: "Claude Code".into(),
            account: Some("work@example.com".into()),
            step: WelcomeStep::Added {
                said: words::ADDED.into()
            },
        }]
    );
}

#[test]
fn claude_code_signed_in_to_nobody_offers_its_sign_in() {
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    let model = welcomed(&mut machine);
    let [claude] = model.on_a_mac().welcome.tools.try_into().expect("one tool");
    assert_eq!(claude.account, None);
    assert_eq!(
        claude.step,
        adds(
            &words::sign_in_asking(Os::MacOs),
            Intent::PresentSheet {
                sheet: Sheet::Add {
                    provider: Some("claude".into())
                }
            }
        )
    );
}

#[test]
fn a_machine_with_no_tool_is_shown_where_to_install_claude_code() {
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    machine.found = Vec::new();
    let model = welcomed(&mut machine);
    let missing = words::missing(Os::MacOs, "claude").expect("Claude Code's");
    let [claude] = model.on_a_mac().welcome.tools.try_into().expect("one tool");
    assert_eq!(
        claude.step,
        WelcomeStep::Install {
            link: SetupLink {
                title: missing.link_title,
                link: missing.link,
            }
        }
    );
}

#[test]
fn the_welcome_offers_open_at_login_as_the_system_says_and_continues_to_the_accounts() {
    let mut machine = Stub::reading(Ok(status(Vec::new())));
    let mut model = welcomed(&mut machine);
    let login = |os, on| {
        let (title, note) = words::open_at_login(os);
        WelcomeLogin {
            title: title.into(),
            on,
            note: note.into(),
        }
    };
    assert_eq!(
        model.on_a_mac().welcome.open_at_login,
        login(Os::MacOs, false),
        "off until the system says"
    );
    model.send(Intent::OpensAtLogin { on: true });
    assert_eq!(
        model.on_windows().welcome.open_at_login,
        login(Os::Windows, true)
    );

    // Where the icon is, said once, with where to keep it in view beside it.
    let windows = model.on_windows().welcome;
    assert_eq!(windows.lines[1], words::where_it_lives(Os::Windows));
    assert_eq!(
        windows.icon_link,
        Some(SetupLink {
            title: words::TASKBAR_SETTINGS.into(),
            link: "ms-settings:taskbar".into(),
        })
    );
    let welcome = model.on_a_mac().welcome;
    assert_eq!(welcome.icon_link, None);
    assert_eq!(welcome.title, words::WELCOME);
    assert_eq!(welcome.lines[1], words::where_it_lives(Os::MacOs));
    assert_eq!(
        welcome.done,
        Choice {
            title: words::welcome_done(Os::MacOs),
            intent: Intent::Show {
                place: Some(Place::Accounts)
            },
            enabled: true,
        }
    );
    model.send(welcome.done.intent);
    assert_eq!(model.shown().window_request.place, Some(Place::Accounts));
}
