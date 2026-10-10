//! Which accounts have a window, what each window is titled, how the menus offer them, and
//! what forgetting an account deletes of its window.

use super::{stores, this_machine};
use crate::{Account, Site};
use pitboard_core::host::Os;
use pitboard_sites::Conjunction;

/// An enrolled account that has a window on a site: what the menus, the account picker and
/// the window itself say about it, and the store that keeps its data.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct WindowAccount {
    /// The site the window is on.
    pub site: Site,
    pub label: String,
    pub email: String,
    /// The store that keeps the window's data, as `store_id` derives it, and the window's
    /// identity too: one window per account. Compare one without regard to case.
    pub store: String,
    /// Whether the account is the one its tool is signed in to now.
    pub in_use: bool,
    /// What the window is titled: the label, or the label and the site when an account on
    /// another site has the same label, so a list of windows tells the two apart.
    pub title: String,
}

/// How a menu offers one site's windows: an item naming the only account, and a submenu of
/// windows for several. A submenu of one item is what a menu should not have.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum SiteMenu {
    /// "Open claude.ai as work", which opens the site's only window.
    One {
        title: String,
        window: WindowAccount,
    },
    /// "Open chatgpt.com", a submenu of the site's windows, each by its title.
    Several {
        title: String,
        site: Site,
        windows: Vec<WindowAccount>,
    },
}

/// Every enrolled account's window, in the order `accounts` lists them and then the sites'
/// order.
///
/// An account has a window when its tool has a site, it has a label and an account id, and
/// Pitboard can place it. A login Pitboard has no name for, or a Codex API key, has none.
pub(crate) fn windows(accounts: &[Account]) -> Vec<WindowAccount> {
    let placed: Vec<(&pitboard_sites::Site, &Account, &str)> = accounts
        .iter()
        .filter(|account| !account.unplaced && !account.account_id.is_empty())
        .filter_map(|account| Some((account, account.label.as_deref()?)))
        .flat_map(|(account, label)| {
            pitboard_sites::Site::of_provider(&account.provider)
                .map(move |site| (site, account, label))
        })
        .collect();
    placed
        .iter()
        .map(|&(site, account, label)| {
            let shared = placed
                .iter()
                .any(|&(other, _, same)| other.host != site.host && same == label);
            WindowAccount {
                site: site.into(),
                label: label.to_owned(),
                email: account.email.clone(),
                store: stores::derive(site.store_name, &account.account_id),
                in_use: account.signed_in,
                title: if shared {
                    format!("{label} ({})", site.name())
                } else {
                    label.to_owned()
                },
            }
        })
        .collect()
}

/// The windows `account` has among those of `accounts`, one for each site of its tool: what
/// its row's own menu offers to open, which the snapshot carries as `AccountItem::windows`.
/// The site is checked as well as the store: two tools' accounts can share an account id.
pub(crate) fn windows_of_account(account: &Account, accounts: &[Account]) -> Vec<WindowAccount> {
    windows(accounts)
        .into_iter()
        .filter(|window| {
            pitboard_sites::Site::of_provider(&account.provider).any(|site| {
                site.host == window.site.host
                    && stores::same(
                        &window.store,
                        &stores::derive(site.store_name, &account.account_id),
                    )
            })
        })
        .collect()
}

/// Every enrolled account's window, from the accounts a read found, in the order `accounts`
/// lists them and then the sites' order: what the menus, the Dock's menu and the account
/// picker offer, so they cannot disagree.
///
/// An account has a window when its tool has a site, it has a label and an account id, and
/// Pitboard can place it. A login Pitboard has no name for, or a Codex API key, has none.
#[uniffi::export]
pub fn window_accounts(accounts: Vec<Account>) -> Vec<WindowAccount> {
    windows(&accounts)
}

/// The window whose data `store` keeps, while its account is one of `accounts`. `store` is
/// compared without regard to case.
#[uniffi::export]
pub fn window_of_store(accounts: Vec<Account>, store: String) -> Option<WindowAccount> {
    windows(&accounts)
        .into_iter()
        .find(|window| stores::same(&window.store, &store))
}

/// One menu entry per site that has an account with a window, in the sites' order.
#[uniffi::export]
pub fn site_menus(accounts: Vec<Account>) -> Vec<SiteMenu> {
    let windows = windows(&accounts);
    pitboard_sites::ALL
        .iter()
        .filter_map(|site| {
            let mut mine: Vec<WindowAccount> = windows
                .iter()
                .filter(|window| window.site.host == site.host)
                .cloned()
                .collect();
            match mine.len() {
                0 => None,
                1 => {
                    let window = mine.remove(0);
                    Some(SiteMenu::One {
                        title: format!("Open {} as {}", site.name(), window.label),
                        window,
                    })
                }
                _ => Some(SiteMenu::Several {
                    title: format!("Open {}", site.name()),
                    site: (*site).into(),
                    windows: mine,
                }),
            }
        })
        .collect()
}

/// What the alert asking to forget `account` says, among `accounts`, on `os`: the question
/// an account's Forget… asks first, which the snapshot carries as `AccountItem::forget`.
/// Forgetting an account that has a window deletes what that window keeps too, and a person
/// deciding should know.
pub(crate) fn forget_message_on(os: Os, account: &Account, accounts: &[Account]) -> String {
    let windows = windows_of_account(account, accounts);
    if windows.is_empty() {
        return "Pitboard deletes the login it parked for this account. Using it again needs a \
                sign-in in your browser."
            .into();
    }
    let sites: Vec<&str> = windows
        .iter()
        .map(|window| window.site.name.as_str())
        .collect();
    format!(
        "Pitboard deletes the login it parked for this account, and everything its {} window \
         keeps on {}, its sign-in included. Using it again needs a sign-in in your browser.",
        pitboard_sites::listed(&sites, Conjunction::And),
        this_machine(os)
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// An account as the core reports one, as the Swift tests made them. `label` `None` is a
    /// login signed in and not enrolled.
    pub(crate) fn account(label: Option<&str>, provider: &str, uuid: &str) -> Account {
        Account {
            id: format!("{provider}:{uuid}"),
            provider: provider.into(),
            label: label.map(str::to_owned),
            qualified: label.map(|label| format!("{provider}/{label}")),
            unplaced: false,
            email: format!("{}@example.com", label.unwrap_or(uuid)),
            account_id: uuid.into(),
            signed_in: false,
            switchable: label.is_some(),
            parked: None,
            usage: None,
            stale: None,
            stale_explanation: None,
            plan: None,
        }
    }

    /// A tool's login that belongs to no account Pitboard can name: no label, no email and no
    /// account id.
    fn unplaced(provider: &str) -> Account {
        Account {
            id: format!("{provider}:login"),
            label: None,
            qualified: None,
            unplaced: true,
            email: String::new(),
            account_id: String::new(),
            switchable: false,
            ..account(None, provider, "")
        }
    }

    fn only(account: Account) -> Option<WindowAccount> {
        windows(&[account]).into_iter().next()
    }

    #[test]
    fn an_enrolled_account_has_a_window_on_its_tools_site() {
        let work = only(account(
            Some("work"),
            "claude",
            "4f3c2a10-8b7e-4d2a-9c1e-5a6b7c8d9e0f",
        ))
        .expect("a window");
        assert_eq!(work.store, "7e15c34f-69ec-55b4-9542-f1c1fe3d7085");
        assert_eq!(work.site.host, "claude.ai");
        assert_eq!(work.email, "work@example.com");
        let main = only(account(Some("main"), "codex", "team_user-1")).expect("a window");
        assert_eq!(main.site.host, "chatgpt.com");
        assert_eq!(main.label, "main");
        assert_eq!(main.store, "8ff9e0e6-a7e2-53e2-a594-6e53ddadd38a");
        let in_use = only(Account {
            signed_in: true,
            ..account(Some("home"), "claude", "h")
        });
        assert!(in_use.expect("a window").in_use);
    }

    #[test]
    fn a_rename_keeps_the_store() {
        let before = only(account(Some("work"), "claude", "4f3c2a10")).expect("a window");
        let after = only(account(Some("office"), "claude", "4f3c2a10")).expect("a window");
        assert_eq!(before.store, after.store);
    }

    #[test]
    fn unnamed_unplaced_and_unknown_tools_have_no_window() {
        assert_eq!(only(account(None, "claude", "dana")), None);
        assert_eq!(only(account(None, "codex", "dana")), None);
        assert_eq!(only(unplaced("claude")), None);
        assert_eq!(only(unplaced("codex")), None, "an API key login");
        assert_eq!(only(account(Some("work"), "claude", "")), None);
        assert_eq!(
            only(account(Some("x"), "gemini", "x")),
            None,
            "a tool with no site"
        );
        assert!(window_accounts(Vec::new()).is_empty());
    }

    /// A list of windows shows each by its title alone, so one label on two sites says which.
    #[test]
    fn a_label_on_two_sites_says_which_site_in_the_title() {
        let windows = window_accounts(vec![
            account(Some("work"), "claude", "a"),
            account(Some("work"), "codex", "b"),
            account(Some("home"), "claude", "c"),
        ]);
        let titles: Vec<&str> = windows.iter().map(|w| w.title.as_str()).collect();
        assert_eq!(titles, ["work (claude.ai)", "work (chatgpt.com)", "home"]);
        let labels: Vec<&str> = windows.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(labels, ["work", "work", "home"]);
    }

    #[test]
    fn a_site_with_one_account_is_one_item_and_with_several_a_submenu() {
        let menus = site_menus(vec![
            account(Some("work"), "claude", "a"),
            account(Some("main"), "codex", "b"),
            account(Some("spare"), "codex", "c"),
            account(None, "claude", "d"),
        ]);
        let [
            SiteMenu::One { title, window },
            SiteMenu::Several {
                title: several,
                site,
                windows,
            },
        ] = menus.as_slice()
        else {
            panic!("an item for claude.ai and a submenu for chatgpt.com: {menus:?}");
        };
        assert_eq!(title, "Open claude.ai as work");
        assert_eq!(window.label, "work");
        assert_eq!(several, "Open chatgpt.com");
        assert_eq!(site.host, "chatgpt.com");
        let labels: Vec<&str> = windows.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(labels, ["main", "spare"]);
        assert!(site_menus(vec![account(None, "claude", "d")]).is_empty());
    }

    /// The window an app knows by its store is found whatever case the store is written in,
    /// and none is found once its account is gone.
    #[test]
    fn a_window_is_found_by_its_store_in_any_case() {
        let accounts = || {
            vec![
                account(
                    Some("work"),
                    "claude",
                    "4f3c2a10-8b7e-4d2a-9c1e-5a6b7c8d9e0f",
                ),
                account(Some("main"), "codex", "team_user-1"),
            ]
        };
        let found = window_of_store(accounts(), "7E15C34F-69EC-55B4-9542-F1C1FE3D7085".into());
        assert_eq!(found.map(|w| w.label).as_deref(), Some("work"));
        let found = window_of_store(accounts(), "8ff9e0e6-a7e2-53e2-a594-6e53ddadd38a".into());
        assert_eq!(found.map(|w| w.label).as_deref(), Some("main"));
        assert_eq!(
            window_of_store(accounts(), "323d12fb-2c52-55b5-baec-df74fb60bc24".into()),
            None
        );
    }

    /// An account's own windows, not another tool's account with the same account id.
    #[test]
    fn an_accounts_windows_are_its_own() {
        let all = || {
            vec![
                account(Some("work"), "claude", "same"),
                account(Some("main"), "codex", "same"),
            ]
        };
        let claude = windows_of_account(&account(Some("work"), "claude", "same"), &all());
        assert_eq!(claude.len(), 1);
        assert_eq!(claude[0].site.host, "claude.ai");
        let codex = windows_of_account(&account(Some("main"), "codex", "same"), &all());
        assert_eq!(codex.len(), 1);
        assert_eq!(codex[0].site.host, "chatgpt.com");
        assert!(windows_of_account(&unplaced("codex"), &all()).is_empty());
    }

    /// Forgetting an account that has a window deletes what that window keeps too, and a
    /// person deciding should know. The Mac's words are the Swift test's.
    #[test]
    fn the_forget_alert_names_the_windows_data() {
        let work = || account(Some("work"), "claude", "a");
        assert_eq!(
            forget_message_on(Os::MacOs, &work(), &[work()]),
            "Pitboard deletes the login it parked for this account, and everything its \
             claude.ai window keeps on this Mac, its sign-in included. Using it again needs a \
             sign-in in your browser."
        );
        assert_eq!(
            forget_message_on(Os::Linux, &work(), &[work()]),
            "Pitboard deletes the login it parked for this account, and everything its \
             claude.ai window keeps on this computer, its sign-in included. Using it again \
             needs a sign-in in your browser."
        );
        assert_eq!(
            forget_message_on(Os::Windows, &work(), &[work()]),
            "Pitboard deletes the login it parked for this account, and everything its \
             claude.ai window keeps on this PC, its sign-in included. Using it again needs a \
             sign-in in your browser."
        );
        let api = || unplaced("codex");
        let said = "Pitboard deletes the login it parked for this account. Using it again needs \
                    a sign-in in your browser.";
        for os in [Os::MacOs, Os::Linux, Os::Windows] {
            assert_eq!(forget_message_on(os, &api(), &[api()]), said);
        }
    }
}
