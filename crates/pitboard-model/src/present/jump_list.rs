//! The Windows Jump List: a switch to each account, Add account, the windows.

use super::{Clicked, Seen, glance, in_order, words};
use crate::account_windows::window_accounts;
use crate::model::OutsideAnswer;
use crate::{Front, RunOutNotice};

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

// The same whatever Pitboard is doing, as Microsoft asks of tasks. None where Windows keeps the
// last one: before the accounts are read, and while uninstalling, whose MSI deletes it.
pub(crate) fn jump_list(seen: &Seen) -> Option<Vec<JumpTask>> {
    let state = seen.state;
    let keeps = match seen.front {
        Front::WindowsNotificationArea => true,
        Front::MacMenuBar | Front::Gnome => false,
    };
    if !keeps || state.status.is_none() || state.removing.is_some() {
        return None;
    }
    let accounts = seen.accounts();
    let tools: Vec<_> = accounts.iter().map(|account| account.provider).collect();
    let switches = in_order(&tools, &seen.tools).into_iter().flat_map(|tool| {
        accounts.iter().filter(move |account| {
            account.provider == tool && account.key.is_some() && !account.unplaced
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
        title: words::add_account(seen.front, true),
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
pub fn jump_refused(front: Front, answer: OutsideAnswer) -> RunOutNotice {
    RunOutNotice {
        id: format!("outside-{}", answer.request),
        title: words::could_not_switch_from_outside(front),
        subtitle: None,
        body: answer.said,
        clicked: Clicked::Accounts.argument(),
        button: None,
    }
}
