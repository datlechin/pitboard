//! The Windows Jump List the model gives, driven through `State::apply` by hand.

use super::testing::machine::{Hand, Stub};
use super::testing::records::{claude_code, codex, status};
use super::{Intent, Place};
use crate::account_windows::stores::derive;
use crate::present::testing::account;
use crate::present::{glance_id, words};
use crate::{Account, Front, JumpAsk, JumpTask, OutsideAnswer, RunOutNotice, jump_refused};
use pitboard_core::provider::ProviderId;
use pitboard_core::status::Standing;

fn accounts() -> Vec<Account> {
    vec![
        account(Some("work")).signed_in().uuid("w").build(),
        account(Some("personal")).uuid("p").build(),
        account(Some("old"))
            .uuid("o")
            .standing(Standing::LoginExpired)
            .build(),
        account(Some("main"))
            .of(ProviderId::Codex)
            .signed_in()
            .uuid("m")
            .build(),
        account(Some("spare"))
            .of(ProviderId::Codex)
            .uuid("s")
            .build(),
    ]
}

fn read(accounts: Vec<Account>) -> Hand {
    let mut machine = Stub::reading(Ok(status(accounts)));
    machine.found = vec![claude_code(), codex()];
    let mut model = Hand::new();
    model.send(Intent::Start);
    model.run(&mut machine);
    model
}

fn switch(title: &str, account: &str) -> JumpTask {
    JumpTask {
        title: words::switch_to(title),
        ask: JumpAsk::Switch {
            account: glance_id(account),
        },
    }
}

fn window(site: &pitboard_sites::Site, label: &str, uuid: &str) -> JumpTask {
    JumpTask {
        title: words::open_site_as(site.name(), label),
        ask: JumpAsk::OpenWindow {
            store: derive(site.store_name, uuid),
        },
    }
}

fn add() -> JumpTask {
    JumpTask {
        title: words::add_account(Front::WindowsNotificationArea, true),
        ask: JumpAsk::AddAccount,
    }
}

// Each tool's in the window's order, then as the icon's menu goes on: Add account, the windows.
// A switch to an account in use answers that it is, and one to an account that can't be
// switched to says why in a notification.
#[test]
fn the_jump_list_switches_to_each_account_then_adds_then_opens_each_window() {
    let model = read(accounts());
    let claude = &pitboard_sites::CLAUDE;
    let chatgpt = &pitboard_sites::CHATGPT;
    let named = |label: &str, tool: &str| words::named_with_tool(label, tool);
    assert_eq!(
        model.on_windows().jump_list,
        Some(vec![
            switch(&named("work", "Claude Code"), "claude:w"),
            switch(&named("personal", "Claude Code"), "claude:p"),
            switch(&named("old", "Claude Code"), "claude:o"),
            switch(&named("main", "Codex"), "codex:m"),
            switch(&named("spare", "Codex"), "codex:s"),
            add(),
            window(claude, "work", "w"),
            window(claude, "personal", "p"),
            window(claude, "old", "o"),
            window(chatgpt, "main", "m"),
            window(chatgpt, "spare", "s"),
        ])
    );
}

#[test]
fn with_one_tool_a_switch_names_the_account_alone() {
    let claude_only: Vec<Account> = (accounts().into_iter())
        .filter(|account| account.provider == ProviderId::Claude)
        .collect();
    let tasks = read(claude_only).on_windows().jump_list.expect("read");
    assert_eq!(
        tasks[..4],
        [
            switch("work", "claude:w"),
            switch("personal", "claude:p"),
            switch("old", "claude:o"),
            add()
        ]
    );
}

// Microsoft's Taskbar Extensions: "It is strongly recommended that the task list be static. It
// should remain the same regardless of the state or status of the application."
#[test]
fn the_jump_list_stays_the_same_across_a_switch() {
    let switched = vec![
        account(Some("work"))
            .uuid("w")
            .standing(Standing::LoginExpired)
            .build(),
        account(Some("personal")).signed_in().uuid("p").build(),
        account(Some("old")).uuid("o").build(),
        account(Some("main"))
            .of(ProviderId::Codex)
            .uuid("m")
            .build(),
        account(Some("spare"))
            .of(ProviderId::Codex)
            .signed_in()
            .uuid("s")
            .build(),
    ];
    assert_eq!(
        read(switched).on_windows().jump_list,
        read(accounts()).on_windows().jump_list
    );
}

// The taskbar keeps the last list it was given, which Pitboard's MSI deletes as it uninstalls.
#[test]
fn the_jump_list_stays_as_it_was_before_the_read_and_while_uninstalling() {
    let mut model = Hand::new();
    model.send(Intent::Start);
    assert_eq!(model.on_windows().jump_list, None, "before the read");

    let mut model = read(accounts());
    assert_eq!(model.on_a_mac().jump_list, None, "a Mac has none");
    model.send(Intent::Uninstall);
    assert_eq!(model.on_windows().jump_list, None, "while uninstalling");
}

#[test]
fn a_task_refused_is_said_in_a_notification_that_opens_the_accounts() {
    let said = words::SWITCH_UNDER_WAY.to_owned();
    let notice = jump_refused(
        Front::WindowsNotificationArea,
        OutsideAnswer {
            request: 3,
            done: false,
            said: said.clone(),
            question: None,
        },
    );
    assert_eq!(
        notice,
        RunOutNotice {
            id: "outside-3".into(),
            title: words::could_not_switch_from_outside(Front::WindowsNotificationArea),
            subtitle: None,
            body: said,
            clicked: notice.clicked.clone(),
            button: None,
        }
    );
    let mut model = read(accounts());
    model.send(Intent::NotificationClicked {
        argument: notice.clicked,
    });
    assert_eq!(model.shown().window_request.place, Some(Place::Accounts));
}
