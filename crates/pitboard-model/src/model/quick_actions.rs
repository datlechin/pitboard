//! The quick actions the model gives, the Windows Jump List's tasks and what GNOME's search and
//! KRunner find, driven through `State::apply` by hand.

use super::testing::machine::{Hand, Stub};
use super::testing::records::{claude_code, codex, status};
use super::{Intent, Place};
use crate::account_windows::stores::derive;
use crate::present::testing::account;
use crate::present::{glance_id, words};
use crate::{
    Account, Front, Notice, OutsideAnswer, QuickAction, QuickAsk, outside_refused, search,
};
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
    read_on(Front::WindowsNotificationArea, accounts)
}

fn read_on(front: Front, accounts: Vec<Account>) -> Hand {
    let mut machine = Stub::reading(Ok(status(accounts)));
    machine.found = vec![claude_code(), codex()];
    let mut model = Hand::on(front);
    model.send(Intent::Start);
    model.run(&mut machine);
    model
}

fn switch(title: &str, account: &str) -> QuickAction {
    QuickAction {
        title: words::switch_to(title),
        detail: None,
        ask: QuickAsk::Switch {
            account: glance_id(account),
        },
    }
}

fn window(site: &pitboard_sites::Site, label: &str, uuid: &str) -> QuickAction {
    QuickAction {
        title: words::open_site_as(site.name(), label),
        detail: None,
        ask: QuickAsk::OpenWindow {
            store: derive(site.store_name, uuid),
        },
    }
}

fn add() -> QuickAction {
    QuickAction {
        title: words::add_account(Front::WindowsNotificationArea, true),
        detail: None,
        ask: QuickAsk::AddAccount,
    }
}

// Each tool's in the window's order, then as the icon's menu goes on: Add account, the windows.
fn jump_list() -> Vec<QuickAction> {
    let claude = &pitboard_sites::CLAUDE;
    let chatgpt = &pitboard_sites::CHATGPT;
    let named = |label: &str, tool: &str| words::named_with_tool(label, tool);
    vec![
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
    ]
}

// Only the Windows app and the Linux app keep a list their system offers outside them: the Jump
// List, and what GNOME's search and KRunner find.
#[test]
fn quick_actions_are_offered_only_where_the_front_keeps_them() {
    for front in [
        Front::MacMenuBar,
        Front::WindowsNotificationArea,
        Front::Gnome,
    ] {
        let offered = read_on(front, accounts()).shown().quick_actions;
        match front {
            Front::WindowsNotificationArea => assert_eq!(offered, Some(jump_list())),
            Front::Gnome => assert!(offered.is_some()),
            Front::MacMenuBar => assert_eq!(offered, None, "{front:?}"),
        }
    }
}

// What a search finds of each account says what its menu says under it, and of the one in use
// that it is. The GNOME app has no account windows, so none is offered, and while Pitboard is
// being removed nothing is.
#[test]
fn the_gnome_front_offers_a_switch_for_each_account_and_add_account() {
    let mut model = read_on(Front::Gnome, accounts());
    let shown = model.shown();
    let line = |label: &str| {
        (shown.sections.iter())
            .flat_map(|section| &section.accounts)
            .find(|item| {
                item.qualified
                    .as_deref()
                    .is_some_and(|q| q.ends_with(label))
            })
            .map(|item| item.summary.clone())
            .expect("its menu's line")
    };
    let named = |label: &str, tool: &str| words::named_with_tool(label, tool);
    let switch = |title: &str, detail: String, account: &str| QuickAction {
        title: words::switch_to(title),
        detail: Some(detail),
        ask: QuickAsk::Switch {
            account: glance_id(account),
        },
    };
    let in_use = words::in_use(Front::Gnome);
    assert_eq!(
        shown.quick_actions,
        Some(vec![
            switch(&named("work", "Claude Code"), in_use.clone(), "claude:w"),
            switch(
                &named("personal", "Claude Code"),
                line("/personal"),
                "claude:p"
            ),
            switch(&named("old", "Claude Code"), line("/old"), "claude:o"),
            switch(&named("main", "Codex"), in_use, "codex:m"),
            switch(&named("spare", "Codex"), line("/spare"), "codex:s"),
            QuickAction {
                title: words::add_account(Front::Gnome, true),
                detail: None,
                ask: QuickAsk::AddAccount,
            },
        ])
    );
    assert_eq!(
        line("/old"),
        words::sign_in_standing(Standing::LoginExpired)
    );

    model.send(Intent::Uninstall);
    assert_eq!(
        model.shown().quick_actions,
        Some(Vec::new()),
        "while removing"
    );
}

fn found() -> Vec<QuickAction> {
    let switch = |title: &str, detail: &str, account: &str| QuickAction {
        title: title.into(),
        detail: Some(detail.into()),
        ask: QuickAsk::Switch {
            account: account.into(),
        },
    };
    vec![
        switch("Switch to work (Claude Code)", "In Use", "claude:w"),
        switch(
            "Switch to personal (Claude Code)",
            "5-hour 42% used, weekly 12%",
            "claude:p",
        ),
        switch("Switch to main (Codex)", "In Use", "codex:m"),
        QuickAction {
            title: "Add Account…".into(),
            detail: None,
            ask: QuickAsk::AddAccount,
        },
    ]
}

fn searched(front: Front, terms: &[&str]) -> Vec<String> {
    let terms: Vec<String> = terms.iter().map(|&term| term.into()).collect();
    (search(front, &found(), &terms).into_iter())
        .map(|action| action.title)
        .collect()
}

// Each term starts a word of the title or the detail, in any case, and what is found keeps
// the order the actions have.
#[test]
fn search_matches_every_term_against_an_actions_words() {
    let gnome = Front::Gnome;
    assert_eq!(searched(gnome, &["WO"]), ["Switch to work (Claude Code)"]);
    assert_eq!(
        searched(gnome, &["claude"]),
        [
            "Switch to work (Claude Code)",
            "Switch to personal (Claude Code)"
        ]
    );
    assert_eq!(
        searched(gnome, &["switch", "cod"]),
        [
            "Switch to work (Claude Code)",
            "Switch to personal (Claude Code)",
            "Switch to main (Codex)"
        ]
    );
    assert_eq!(
        searched(gnome, &["week", "per"]),
        ["Switch to personal (Claude Code)"],
        "a word of the detail"
    );
    assert_eq!(
        searched(gnome, &["in", "use", "codex"]),
        ["Switch to main (Codex)"]
    );
    assert_eq!(searched(gnome, &["account"]), ["Add Account…"]);
    assert_eq!(
        searched(gnome, &["ork"]),
        [] as [&str; 0],
        "a word's start only"
    );
    assert_eq!(searched(gnome, &["work", "main"]), [] as [&str; 0]);
    assert_eq!(
        searched(Front::MacMenuBar, &["work"]),
        [] as [&str; 0],
        "a front that keeps no quick actions"
    );
}

#[test]
fn search_of_no_terms_finds_nothing() {
    assert_eq!(searched(Front::Gnome, &[]), [] as [&str; 0]);
}

// A switch to an account in use answers that it is, and one to an account that can't be
// switched to says why in a notification.
#[test]
fn the_jump_list_switches_to_each_account_then_adds_then_opens_each_window() {
    assert_eq!(read(accounts()).shown().quick_actions, Some(jump_list()));
}

#[test]
fn with_one_tool_a_switch_names_the_account_alone() {
    let claude_only: Vec<Account> = (accounts().into_iter())
        .filter(|account| account.provider == ProviderId::Claude)
        .collect();
    let tasks = read(claude_only).shown().quick_actions.expect("read");
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
        read(switched).shown().quick_actions,
        read(accounts()).shown().quick_actions
    );
}

// The taskbar keeps the last list it was given, which Pitboard's MSI deletes as it uninstalls.
#[test]
fn the_jump_list_stays_as_it_was_before_the_read_and_while_uninstalling() {
    let mut model = Hand::on(Front::WindowsNotificationArea);
    model.send(Intent::Start);
    assert_eq!(model.shown().quick_actions, None, "before the read");

    let mut model = read(accounts());
    model.send(Intent::Uninstall);
    assert_eq!(model.shown().quick_actions, None, "while uninstalling");
}

#[test]
fn a_task_refused_is_said_in_a_notification_that_opens_the_accounts() {
    let said = words::SWITCH_UNDER_WAY.to_owned();
    let notice = outside_refused(
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
        Notice {
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
