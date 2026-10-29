//! What a front offers to start Pitboard with from outside it, where it keeps such a list: the
//! Windows Jump List's tasks, a switch to each account, Add account, the windows; and GNOME's
//! search and KRunner's results, a switch to each account and Add Account.

use super::accounts::summary;
use super::{Clicked, Seen, glance, in_order, words};
use crate::account_windows::offered_windows;
use crate::model::OutsideAnswer;
use crate::{Front, Notice};

// What the system offers beside Pitboard, which starts Pitboard with what it asks: on Windows
// a task the taskbar and Start offer, which starts Pitboard.exe; on Linux a search result.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct QuickAction {
    pub title: String,
    pub ask: QuickAsk,
    // under the title of a search result: the account's line in the menu, or that it is in use;
    // none in the Jump List, whose tasks stay the same
    #[uniffi(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum QuickAsk {
    // a request from outside, by the account's id in the widget feed, which a rename keeps
    Switch { account: String },
    AddAccount,
    // the window the accounts read list under this store
    OpenWindow { store: String },
}

// The same whatever Pitboard is doing on Windows, as Microsoft asks of tasks, and as each
// account stands now in a search. None where the front keeps none, and where the last list
// stays: before the accounts are read, and on Windows while uninstalling, whose MSI deletes it.
// A search finds nothing while Pitboard is being removed.
pub(crate) fn quick_actions(seen: &Seen) -> Option<Vec<QuickAction>> {
    let state = seen.state;
    if !seen.front.keeps_quick_actions() || state.status.is_none() {
        return None;
    }
    if state.removing.is_some() {
        return match seen.front {
            Front::MacMenuBar | Front::WindowsNotificationArea => None,
            Front::Gnome => Some(Vec::new()),
        };
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
            detail: match seen.front {
                Front::MacMenuBar | Front::WindowsNotificationArea => None,
                Front::Gnome if account.signed_in => Some(words::in_use(seen.front)),
                Front::Gnome => Some(summary(seen, account)),
            },
            ask: QuickAsk::Switch {
                account: glance::id(&account.id),
            },
        })
        .collect();
    actions.push(QuickAction {
        title: words::add_account(seen.front, true),
        detail: None,
        ask: QuickAsk::AddAccount,
    });
    let windows = offered_windows(seen.front, accounts);
    for site in pitboard_sites::ALL {
        actions.extend(
            (windows.iter())
                .filter(|window| window.site.host == site.host)
                .map(|window| QuickAction {
                    title: words::open_site_as(site.name(), &window.label),
                    detail: None,
                    ask: QuickAsk::OpenWindow {
                        store: window.store.clone(),
                    },
                }),
        );
    }
    Some(actions)
}

/// The actions with each term the start of a word of their title or detail, in any case.
pub fn search(front: Front, actions: &[QuickAction], terms: &[String]) -> Vec<QuickAction> {
    // No terms find nothing, and neither does a front that keeps no quick actions.
    if terms.is_empty() || !front.keeps_quick_actions() {
        return Vec::new();
    }
    let terms: Vec<String> = terms.iter().map(|term| term.to_lowercase()).collect();
    (actions.iter())
        .filter(|action| {
            let words: Vec<String> = words_of(&action.title)
                .chain(action.detail.iter().flat_map(|detail| words_of(detail)))
                .collect();
            (terms.iter()).all(|term| words.iter().any(|word| word.starts_with(term.as_str())))
        })
        .cloned()
        .collect()
}

// In lower case, without what stands around a word: "(Claude" is "claude", "Account…" "account".
fn words_of(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
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
