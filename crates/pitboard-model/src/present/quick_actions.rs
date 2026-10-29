//! What a front offers to start Pitboard with from outside it, where it keeps such a list:
//! the Windows Jump List's tasks, a switch to each account, Add account, the windows.

use super::{Clicked, Seen, glance, in_order, words};
use crate::account_windows::window_accounts;
use crate::model::OutsideAnswer;
use crate::{Front, Notice};

// What the system offers beside Pitboard, which starts Pitboard with what it asks: on Windows
// a task the taskbar and Start offer, which starts Pitboard.exe.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct QuickAction {
    pub title: String,
    pub ask: QuickAsk,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum QuickAsk {
    // a request from outside, by the account's id in the widget feed, which a rename keeps
    Switch { account: String },
    AddAccount,
    // the window the accounts read list under this store
    OpenWindow { store: String },
}

// The same whatever Pitboard is doing, as Microsoft asks of tasks. None where the front keeps
// none, and where Windows keeps the last one: before the accounts are read, and while
// uninstalling, whose MSI deletes it.
pub(crate) fn quick_actions(seen: &Seen) -> Option<Vec<QuickAction>> {
    let state = seen.state;
    if !seen.front.keeps_quick_actions() || state.status.is_none() || state.removing.is_some() {
        return None;
    }
    let accounts = seen.accounts();
    let tools: Vec<_> = accounts.iter().map(|account| account.provider).collect();
    let switches = in_order(&tools, &seen.tools).into_iter().flat_map(|tool| {
        accounts.iter().filter(move |account| {
            account.provider == tool && account.key.is_some() && !account.unplaced
        })
    });
    let mut actions: Vec<QuickAction> = switches
        .map(|account| QuickAction {
            title: words::switch_to(&seen.name_of(account)),
            ask: QuickAsk::Switch {
                account: glance::id(&account.id),
            },
        })
        .collect();
    actions.push(QuickAction {
        title: words::add_account(seen.front, true),
        ask: QuickAsk::AddAccount,
    });
    let windows = window_accounts(accounts);
    for site in pitboard_sites::ALL {
        actions.extend(
            (windows.iter())
                .filter(|window| window.site.host == site.host)
                .map(|window| QuickAction {
                    title: words::open_site_as(site.name(), &window.label),
                    ask: QuickAsk::OpenWindow {
                        store: window.store.clone(),
                    },
                }),
        );
    }
    Some(actions)
}

// A switch asked from outside that did not happen, said where nothing else would say it.
#[uniffi::export]
pub fn outside_refused(front: Front, answer: OutsideAnswer) -> Notice {
    Notice {
        id: format!("outside-{}", answer.request),
        title: words::could_not_switch_from_outside(front),
        subtitle: None,
        body: answer.said,
        clicked: Clicked::Accounts.argument(),
        button: None,
    }
}
