//! What the welcome window shows at the first launch, driven through `State::apply` by hand.

use super::state::Job;
use super::testing::machine::{Hand, Stub};
use super::testing::records::{claude_code, codex, refusal, status};
use super::{Intent, Place, Sheet};
use crate::Tool;
use crate::present::testing::account;
use crate::present::words;
use crate::{Choice, SetupLink, WelcomeLogin, WelcomeStep, WelcomeTool};
use pitboard_core::host::Os;
use pitboard_core::provider::ProviderId;

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
                step: adds(words::ADD_ASKING, Intent::Enrol { tool: Tool::Claude }),
            },
            WelcomeTool {
                code: "codex".into(),
                name: "Codex".into(),
                account: None,
                step: adds(words::ADD_ASKING, Intent::Enrol { tool: Tool::Codex }),
            },
        ]
    );
    assert_eq!(model.count(enrols), 0, "nothing read of a login yet");

    model.send(Intent::Enrol { tool: Tool::Codex });
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
                said: words::added_as("work")
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
                    tool: Some(Tool::Claude)
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
    let missing = words::missing(Os::MacOs, ProviderId::Claude);
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
    assert_eq!(
        windows.tools_heading.as_deref(),
        Some(words::FOUND_ON_THIS_PC)
    );
    let welcome = model.on_a_mac().welcome;
    assert_eq!((welcome.icon_link, welcome.tools_heading), (None, None));
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

// Plan 7: what the welcome's Add starts, a sheet, a question or a failure, comes up over the
// macOS welcome rather than in the accounts window behind it, until the welcome goes on to them.
// Windows puts them up in its window, whose dialog hosts them (#333).
#[test]
fn what_the_welcomes_add_starts_comes_up_over_the_welcome() {
    let over = |place: Option<Place>| {
        if pitboard_core::host::OS == Os::Windows {
            place
        } else {
            Some(Place::Welcome)
        }
    };
    let mut machine = Stub::reading(Ok(status(vec![
        account(None).signed_in().uuid("dana").build(),
    ])));
    let mut model = welcomed(&mut machine);
    let asked = model.shown().window_request;
    assert_eq!(asked.place, Some(Place::Welcome));
    model.send(Intent::Enrol { tool: Tool::Claude });
    model.run(&mut machine);
    let named = model.shown();
    assert!(
        matches!(named.sheet, Some(Sheet::Name { .. })),
        "{:?}",
        named.sheet
    );
    assert_eq!(named.window_request.place, over(Some(Place::Accounts)));
    assert!(
        named.window_request.serial > asked.serial,
        "brought to the front"
    );

    model.send(Intent::CloseSheet);
    machine.enrolling_current = Err(refusal(
        "keychain_locked",
        "The keychain is locked.",
        Vec::new(),
    ));
    model.send(Intent::Enrol { tool: Tool::Claude });
    model.run(&mut machine);
    let failed = model.shown();
    assert!(failed.failure_alert.is_some());
    assert_eq!(failed.window_request.place, over(None));

    model.send(Intent::DismissFailure);
    model.send(model.shown().welcome.done.intent);
    model.send(Intent::PresentSheet {
        sheet: Sheet::Add { tool: None },
    });
    assert_eq!(
        model.shown().window_request.place,
        Some(Place::Accounts),
        "once the welcome has gone on to them"
    );
}
