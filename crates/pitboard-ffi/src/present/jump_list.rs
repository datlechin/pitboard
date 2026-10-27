//! The Windows Jump List: a switch to each account, Add account, the windows.

use super::{Clicked, Seen, glance, in_order, words};
use crate::RunOutNotice;
use crate::account_windows::window_accounts;
use crate::model::OutsideAnswer;
use pitboard_core::host::{OS, Os};

// A task the taskbar and Start offer, which starts Pitboard.exe with what it asks.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct JumpTask {
    pub title: String,
    pub ask: JumpAsk,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum JumpAsk {
    // a request from outside, by the account's id in the glance, which a rename keeps
    Switch { account: String },
    AddAccount,
    // the window the accounts read list under this store
    OpenWindow { store: String },
}

// The same whatever Pitboard is doing, as Microsoft asks of tasks, so it changes only as the
// accounts do. None where the list stays as it was: before the accounts are read, and while
// Pitboard is being uninstalled, whose MSI deletes it.
pub(crate) fn jump_list(seen: &Seen) -> Option<Vec<JumpTask>> {
    let state = seen.state;
    if seen.os != Os::Windows || state.status.is_none() || state.removing.is_some() {
        return None;
    }
    let accounts = seen.accounts();
    let tools: Vec<&str> = accounts
        .iter()
        .map(|account| account.provider.as_str())
        .collect();
    let switches = in_order(&tools, &seen.tools).into_iter().flat_map(|tool| {
        accounts.iter().filter(move |account| {
            account.provider == tool && account.qualified.is_some() && !account.unplaced
        })
    });
    let mut tasks: Vec<JumpTask> = switches
        .map(|account| JumpTask {
            title: words::switch_to(&seen.name_of(account)),
            ask: JumpAsk::Switch {
                account: glance::id(&account.id),
            },
        })
        .collect();
    tasks.push(JumpTask {
        title: words::add_account(seen.os, true),
        ask: JumpAsk::AddAccount,
    });
    let windows = window_accounts(accounts);
    for site in pitboard_sites::ALL {
        tasks.extend(
            (windows.iter())
                .filter(|window| window.site.host == site.host)
                .map(|window| JumpTask {
                    title: words::open_site_as(site.name(), &window.label),
                    ask: JumpAsk::OpenWindow {
                        store: window.store.clone(),
                    },
                }),
        );
    }
    Some(tasks)
}

// A task's switch that did not happen, said where nothing else would say it.
#[uniffi::export]
pub fn jump_refused(answer: OutsideAnswer) -> RunOutNotice {
    jump_refused_on(OS, answer)
}

pub(crate) fn jump_refused_on(os: Os, answer: OutsideAnswer) -> RunOutNotice {
    RunOutNotice {
        id: format!("outside-{}", answer.request),
        title: words::could_not_switch_from_outside(os),
        subtitle: None,
        body: answer.said,
        clicked: Clicked::Accounts.argument(),
        button: None,
    }
}
