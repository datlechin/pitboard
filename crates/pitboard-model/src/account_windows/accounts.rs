//! Which accounts have a window, which of them a front offers, what each window is titled, how
//! the menus offer them, and what forgetting an account deletes of its window.

use super::{stores, this_machine};
use crate::Front;
use crate::present::words;
use crate::{Account, Site};
use pitboard_sites::Conjunction;

/// An enrolled account that has a window on a site: what the menus, the account picker and
/// the window itself say about it, and the store that keeps its data.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct WindowAccount {
    /// The site the window is on.
    pub site: Site,
    pub label: String,
    pub email: String,
    /// The store that keeps the window's data, and the window's identity too: one window per
    /// account. Compare one without regard to case.
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

/// Every enrolled account's window, from the accounts a read found, in the order `accounts`
/// lists them and then the sites' order: what the menus, the Dock's menu and the account
/// picker offer, so they cannot disagree.
///
/// An account has a window when its tool has a site, it has a label and an account id, and
/// Pitboard can place it. A login Pitboard has no name for, or a Codex API key, has none.
pub(crate) fn window_accounts(accounts: &[Account]) -> Vec<WindowAccount> {
    let placed: Vec<(&pitboard_sites::Site, &Account, &str)> = accounts
        .iter()
        .filter(|account| !account.unplaced && !account.account_id.is_empty())
        .filter_map(|account| Some((account, account.label.as_deref()?)))
        .flat_map(|(account, label)| {
            pitboard_sites::Site::of_provider(account.provider.code())
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
                    words::named_with_tool(label, site.name())
                } else {
                    label.to_owned()
                },
            }
        })
        .collect()
}

/// The windows `front` offers of `accounts`: each one's, where the front has account windows,
/// and none where it has none. What every menu, row, picker and quick action offers.
pub(crate) fn offered_windows(front: Front, accounts: &[Account]) -> Vec<WindowAccount> {
    if front.offers_account_windows() {
        window_accounts(accounts)
    } else {
        Vec::new()
    }
}

/// The windows `account` has among those `front` offers of `accounts`, one for each site of its
/// tool: what its row's own menu offers to open. The site is checked as well as the store: two
/// tools' accounts can share an account id.
pub(crate) fn windows_of_account(
    front: Front,
    account: &Account,
    accounts: &[Account],
) -> Vec<WindowAccount> {
    offered_windows(front, accounts)
        .into_iter()
        .filter(|window| {
            pitboard_sites::Site::of_provider(account.provider.code()).any(|site| {
                site.host == window.site.host
                    && stores::same(
                        &window.store,
                        &stores::derive(site.store_name, &account.account_id),
                    )
            })
        })
        .collect()
}

/// One menu entry per site that has an account with a window `front` offers, in the sites'
/// order.
pub(crate) fn site_menus(front: Front, accounts: &[Account]) -> Vec<SiteMenu> {
    let windows = offered_windows(front, accounts);
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
                        title: words::open_site_as(site.name(), &window.label),
                        window,
                    })
                }
                _ => Some(SiteMenu::Several {
                    title: words::open_site(site.name()),
                    site: (*site).into(),
                    windows: mine,
                }),
            }
        })
        .collect()
}

// What Remove Account… asks, which names what the account's windows keep too.
pub(crate) fn forget_message(front: Front, account: &Account, accounts: &[Account]) -> String {
    if crate::model::state::in_its_app(account.provider) {
        let app = account.provider.program();
        return format!(
            "Pitboard deletes its saved sign-in and the folders it kept for it, so using it again \
             needs a sign-in in {app}."
        );
    }
    let windows = windows_of_account(front, account, accounts);
    if windows.is_empty() {
        return "Pitboard deletes its saved sign-in, so using it again needs a sign-in in your \
                browser."
            .into();
    }
    let sites: Vec<&str> = windows
        .iter()
        .map(|window| window.site.name.as_str())
        .collect();
    format!(
        "Pitboard deletes its saved sign-in and what its {} window keeps on {}, so using it again \
         needs a sign-in in your browser.",
        pitboard_sites::listed(&sites, Conjunction::And),
        this_machine(front)
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use pitboard_core::provider::ProviderId;
    use pitboard_core::state::Key;
    use pitboard_core::status::Standing;

    /// An account as the core reports one, as the Swift tests made them. `label` `None` is a
    /// login signed in and not enrolled.
    pub(crate) fn account(label: Option<&str>, provider: ProviderId, uuid: &str) -> Account {
        Account {
            id: format!("{provider}:{uuid}"),
            provider,
            label: label.map(str::to_owned),
            key: label.map(|label| Key::new(provider, label)),
            unplaced: false,
            email: format!("{}@example.com", label.unwrap_or(uuid)),
            account_id: uuid.into(),
            signed_in: false,
            standing: if label.is_some() {
                Standing::Ready
            } else {
                Standing::NothingParked
            },
            parked: None,
            usage: None,
            stale: None,
            stale_explanation: None,
            plan: None,
            read_at: None,
        }
    }

    /// A tool's login that belongs to no account Pitboard can name: no label, no email and no
    /// account id.
    fn unplaced(provider: ProviderId) -> Account {
        Account {
            id: format!("{provider}:login"),
            label: None,
            key: None,
            unplaced: true,
            email: String::new(),
            account_id: String::new(),
            standing: Standing::Unplaced,
            ..account(None, provider, "")
        }
    }

    fn only(account: Account) -> Option<WindowAccount> {
        window_accounts(&[account]).into_iter().next()
    }

    #[test]
    fn an_enrolled_account_has_a_window_on_its_tools_site() {
        let work = only(account(
            Some("work"),
            ProviderId::Claude,
            "4f3c2a10-8b7e-4d2a-9c1e-5a6b7c8d9e0f",
        ))
        .expect("a window");
        assert_eq!(work.store, "7e15c34f-69ec-55b4-9542-f1c1fe3d7085");
        assert_eq!(work.site.host, "claude.ai");
        assert_eq!(work.email, "work@example.com");
        let main = only(account(Some("main"), ProviderId::Codex, "team_user-1")).expect("a window");
        assert_eq!(main.site.host, "chatgpt.com");
        assert_eq!(main.label, "main");
        assert_eq!(main.store, "8ff9e0e6-a7e2-53e2-a594-6e53ddadd38a");
        let in_use = only(Account {
            signed_in: true,
            ..account(Some("home"), ProviderId::Claude, "h")
        });
        assert!(in_use.expect("a window").in_use);
    }

    #[test]
    fn a_rename_keeps_the_store() {
        let before = only(account(Some("work"), ProviderId::Claude, "4f3c2a10")).expect("a window");
        let after =
            only(account(Some("office"), ProviderId::Claude, "4f3c2a10")).expect("a window");
        assert_eq!(before.store, after.store);
    }

    #[test]
    fn unnamed_unplaced_and_siteless_tools_have_no_window() {
        assert_eq!(only(account(None, ProviderId::Claude, "dana")), None);
        assert_eq!(only(account(None, ProviderId::Codex, "dana")), None);
        assert_eq!(only(unplaced(ProviderId::Claude)), None);
        assert_eq!(only(unplaced(ProviderId::Codex)), None, "an API key login");
        assert_eq!(only(account(Some("work"), ProviderId::Claude, "")), None);
        assert_eq!(
            only(account(Some("x"), ProviderId::Desktop, "x")),
            None,
            "a tool with no site"
        );
        assert!(window_accounts(&[]).is_empty());
    }

    /// A list of windows shows each by its title alone, so one label on two sites says which.
    #[test]
    fn a_label_on_two_sites_says_which_site_in_the_title() {
        let windows = window_accounts(&[
            account(Some("work"), ProviderId::Claude, "a"),
            account(Some("work"), ProviderId::Codex, "b"),
            account(Some("home"), ProviderId::Claude, "c"),
        ]);
        let titles: Vec<&str> = windows.iter().map(|w| w.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                words::named_with_tool("work", "claude.ai").as_str(),
                &words::named_with_tool("work", "chatgpt.com"),
                "home"
            ]
        );
        let labels: Vec<&str> = windows.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(labels, ["work", "work", "home"]);
    }

    #[test]
    fn a_site_with_one_account_is_one_item_and_with_several_a_submenu() {
        let menus = site_menus(
            Front::MacMenuBar,
            &[
                account(Some("work"), ProviderId::Claude, "a"),
                account(Some("main"), ProviderId::Codex, "b"),
                account(Some("spare"), ProviderId::Codex, "c"),
                account(None, ProviderId::Claude, "d"),
            ],
        );
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
        assert_eq!(*title, words::open_site_as("claude.ai", "work"));
        assert_eq!(window.label, "work");
        assert_eq!(*several, words::open_site("chatgpt.com"));
        assert_eq!(site.host, "chatgpt.com");
        let labels: Vec<&str> = windows.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(labels, ["main", "spare"]);
        assert!(
            site_menus(Front::MacMenuBar, &[account(None, ProviderId::Claude, "d")]).is_empty()
        );
    }

    /// An account's own windows, not another tool's account with the same account id.
    #[test]
    fn an_accounts_windows_are_its_own() {
        let all = || {
            vec![
                account(Some("work"), ProviderId::Claude, "same"),
                account(Some("main"), ProviderId::Codex, "same"),
            ]
        };
        let of = |account| windows_of_account(Front::MacMenuBar, &account, &all());
        let claude = of(account(Some("work"), ProviderId::Claude, "same"));
        assert_eq!(claude.len(), 1);
        assert_eq!(claude[0].site.host, "claude.ai");
        let codex = of(account(Some("main"), ProviderId::Codex, "same"));
        assert_eq!(codex.len(), 1);
        assert_eq!(codex[0].site.host, "chatgpt.com");
        assert!(of(unplaced(ProviderId::Codex)).is_empty());
    }

    // The GNOME app has none yet, so nothing offers one there.
    #[test]
    fn a_front_without_account_windows_offers_none() {
        let accounts = [
            account(Some("work"), ProviderId::Claude, "a"),
            account(Some("main"), ProviderId::Codex, "b"),
        ];
        assert_eq!(offered_windows(Front::MacMenuBar, &accounts).len(), 2);
        assert_eq!(offered_windows(Front::Gnome, &accounts), []);
        assert_eq!(site_menus(Front::Gnome, &accounts), []);
        assert_eq!(
            windows_of_account(Front::Gnome, &accounts[0], &accounts),
            []
        );
    }

    /// Removing an account that has a window deletes what that window keeps too, and a
    /// person deciding should know.
    #[test]
    fn the_remove_alert_names_the_windows_data() {
        let work = || account(Some("work"), ProviderId::Claude, "a");
        assert_eq!(
            forget_message(Front::MacMenuBar, &work(), &[work()]),
            "Pitboard deletes its saved sign-in and what its claude.ai window keeps on this Mac, \
             so using it again needs a sign-in in your browser."
        );
        assert_eq!(
            forget_message(Front::Gnome, &work(), &[work()]),
            "Pitboard deletes its saved sign-in, so using it again needs a sign-in in your \
             browser.",
            "a front with no account windows keeps nothing of one"
        );
        assert_eq!(
            forget_message(Front::WindowsNotificationArea, &work(), &[work()]),
            "Pitboard deletes its saved sign-in and what its claude.ai window keeps on this PC, \
             so using it again needs a sign-in in your browser."
        );
        let api = || unplaced(ProviderId::Codex);
        let said = "Pitboard deletes its saved sign-in, so using it again needs a sign-in in your \
                    browser.";
        for front in [
            Front::MacMenuBar,
            Front::Gnome,
            Front::WindowsNotificationArea,
        ] {
            assert_eq!(forget_message(front, &api(), &[api()]), said);
        }
    }

    #[test]
    fn removing_a_claude_desktop_account_says_it_needs_a_sign_in_in_claude() {
        let home = account(Some("home"), ProviderId::Desktop, "5b1e");
        assert_eq!(
            forget_message(Front::MacMenuBar, &home, std::slice::from_ref(&home)),
            "Pitboard deletes its saved sign-in and the folders it kept for it, so using it again \
             needs a sign-in in Claude."
        );
    }
}
