//! What the account windows show of the model, from AccountWindowView.swift,
//! AccountPicker.swift, Downloads.swift and AppDelegate.swift as they were at a3e5ce0: the
//! windows each account has and the menus offering them, the windows open and the page each
//! loads, what a window shows until it can show its page, the windows to close and the
//! stores to delete, the Open Link window's link and accounts, each window's downloads, and
//! the question asked before quitting stops a download.

use super::{Choice, EmptyList, Question, Seen, words};
use crate::SiteLink;
use crate::account_windows::records::store_key;
use crate::account_windows::{
    SiteMenu, WindowAccount, WindowNoteKind, site_menus, window_accounts,
};
use crate::model::windows::{Download, Picking};
use crate::model::{Intent, Sheet};
use pitboard_core::host::Os;

/// The account windows, as the model has them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AccountWindowsShown {
    /// Every enrolled account's window, as the accounts shown list them: what the menus, the
    /// Dock's menu and the picker offer, and the account a window of each store shows.
    pub accounts: Vec<WindowAccount>,
    /// The menus' entries, one per site that has an account with a window.
    pub menus: Vec<SiteMenu>,
    /// The windows the app said are open that can show their page, each with its account as
    /// the accounts shown have it: a rename retitles the window.
    pub open: Vec<OpenWindow>,
    /// What a window the app said is open shows until `open` lists it: the accounts being
    /// read, or why they could not be.
    pub waiting: WindowWaiting,
    /// The stores of windows the app said are open whose account the accounts shown no longer
    /// list: forgotten, in the app or with `pitboard forget`. Each window closes, and says so
    /// with `Intent::WindowClosed`.
    pub closing: Vec<String>,
    // Each once, after any window of it closes, answered with `StoreDeleted` or `StoreHeld`.
    // Recorded by this directory alone and derived from no enrolled account, or on Windows
    // held, enrolled again or not.
    pub deleting: Vec<StoreDeletion>,
    /// What the Open Link window shows of the link waiting for an account, while one is.
    pub picker: Option<LinkPicker>,
    /// Every download the windows started, the newest first, for each window to list its own.
    /// What to ask before quitting while some are under way is `downloads_quit_question`'s.
    pub downloads: Vec<DownloadShown>,
    /// What the Open Link window shows while no link waits for an account.
    pub no_link: EmptyList,
    // On Windows, stores held by their deletion: no window of one opens, and its menu item
    // says why with `WindowNoteKind::StoreHeld`.
    #[uniffi(default)]
    pub held: Vec<String>,
}

/// A store the app is asked to delete.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StoreDeletion {
    /// The store, in lower case.
    pub store: String,
    /// One higher for each store asked for, so an app deletes each ask once: a store asked
    /// for again, after something held it, is a new ask, though a listener told only the
    /// newest snapshot never saw the one between without it.
    pub ask: u64,
}

/// A window the app has open, that can show its page.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OpenWindow {
    /// Its store, in lower case: the window's identity.
    pub store: String,
    pub account: WindowAccount,
    /// The page it loads: the page it starts at, then each link chosen for it while it is
    /// open, where Back returns to what it showed.
    pub load: PageLoad,
    /// What it says above its page as it opens: how to sign in, where its store is new to
    /// this Pitboard directory and so holds no sign-in.
    pub note: Option<WindowNoteKind>,
    /// The button of its list of downloads that takes away the ones that have ended. `None`
    /// on Windows, where the list is WebView2's own.
    pub clear_downloads: Option<Choice>,
}

/// What an account window shows in place of its page until it can show it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WindowWaiting {
    /// The window's title, until its account's page names it.
    pub window_title: String,
    pub shown: WaitingShown,
}

/// What a window waiting for its page says.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum WaitingShown {
    /// The accounts are being read, or, once they are, the window's store recorded.
    Reading { title: String },
    /// The accounts could not be read and nothing is known: why, in Pitboard's own words, and
    /// a way to try again.
    ReadFailed {
        title: String,
        detail: String,
        retry: Choice,
    },
}

/// A page for a window to load, once for each `serial`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PageLoad {
    /// One higher for each page asked of any window, so the same link chosen twice loads
    /// twice.
    pub serial: u64,
    /// On the window's scheme: the site itself, or a fixture's stand-in for it.
    pub url: String,
}

/// The link waiting for an account, and what the Open Link window says of it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LinkPicker {
    /// Which link it is, one higher for each that arrived: a link that arrives again is shown
    /// as new, with the choice made afresh, and `Intent::OpenLink` and
    /// `Intent::DismissLink` name it.
    pub arrival: u64,
    /// Whether Open answers yet. Each link waits a moment once the accounts to choose from
    /// appear, so a Return typed for another app as the window came forward opens nothing.
    pub armed: bool,
    pub shown: PickerShown,
}

/// What the Open Link window shows for the link waiting.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum PickerShown {
    /// The accounts are being read for the first time.
    Reading { title: String },
    /// The accounts could not be read: why, in Pitboard's own words, and a way to try again.
    ReadFailed {
        title: String,
        detail: String,
        retry: Choice,
    },
    /// What arrived is not opened, and why.
    Refused { title: String, reason: String },
    /// No enrolled account has a window on the link's site: it can be opened in the browser,
    /// or an account added, and the link waits meanwhile.
    NoAccount {
        title: String,
        link: SiteLink,
        /// The link on one line, without its scheme.
        link_text: String,
        detail: String,
        /// What the button that hands the link to the browser says.
        in_browser: String,
        add: Choice,
    },
    /// The accounts with a window on the link's site, to choose one.
    Choose {
        title: String,
        link: SiteLink,
        /// The link on one line, without its scheme.
        link_text: String,
        accounts: Vec<PickerAccount>,
        /// The store of the account chosen until somebody chooses another: the one chosen
        /// last for the site, else the one in use, else the first.
        chosen: String,
        /// What the button that opens it says.
        open: String,
    },
}

/// An account the link can open as.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PickerAccount {
    pub window: WindowAccount,
    /// The line under its label: its email, and whether its window is open, where the link
    /// replaces what the window shows.
    pub detail: String,
}

/// A download an account window started.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DownloadShown {
    /// What the app calls it.
    pub id: String,
    /// The store of the window whose list it is in.
    pub store: String,
    /// What it is called: the name it is saved under once it has one, and before that what
    /// its address calls it.
    pub name: String,
    pub state: DownloadState,
    /// What its row says once it has ended: "Downloaded", "Cancelled", or why it failed.
    pub said: Option<String>,
    /// Whether it is still under way, which quitting would stop and which can be cancelled.
    pub running: bool,
}

/// Where a download is.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum DownloadState {
    /// Waiting for a name to save it under, or for the person to say it may go ahead.
    Starting,
    /// Saving to `file`.
    Running { file: String },
    /// All of it is in `file`.
    Finished { file: String },
    /// It stopped, and why.
    Failed { reason: String },
    /// Somebody stopped it.
    Cancelled,
}

/// What the account windows show.
pub(crate) fn account_windows(seen: &Seen) -> AccountWindowsShown {
    let windows = &seen.state.windows;
    let status = seen.state.status.as_ref();
    let accounts = window_accounts(seen.accounts());
    let open = windows
        .open
        .iter()
        .filter_map(|window| {
            let (load, note) = window.shown.clone()?;
            let account = accounts
                .iter()
                .find(|account| store_key(&account.store) == window.store)?;
            Some(OpenWindow {
                store: window.store.clone(),
                account: account.clone(),
                load,
                note,
                // Held back until one of its own downloads has ended: before then it would
                // clear nothing.
                clear_downloads: (!matches!(seen.os, Os::Windows)).then(|| Choice {
                    title: "Clear".into(),
                    intent: Intent::ClearDownloads {
                        store: window.store.clone(),
                    },
                    enabled: windows
                        .downloads
                        .iter()
                        .any(|download| download.store == window.store && !download.running()),
                }),
            })
        })
        .collect();
    AccountWindowsShown {
        menus: site_menus(seen.accounts()),
        accounts,
        open,
        waiting: waiting(seen),
        closing: windows.closing(status),
        deleting: windows.deleting.clone(),
        picker: picker(seen),
        downloads: windows.downloads.iter().rev().map(download).collect(),
        no_link: {
            let (title, detail) = words::no_link(seen.os);
            EmptyList { title, detail }
        },
        held: windows.held(),
    }
}

/// What a window shows until it can show its page: why the accounts could not be read, where
/// nothing is known, else that they are being read, as the Open Link window says either.
fn waiting(seen: &Seen) -> WindowWaiting {
    WindowWaiting {
        window_title: "Account".into(),
        shown: match (&seen.state.status, seen.problem()) {
            (None, Some(problem)) => WaitingShown::ReadFailed {
                title: words::could_not_read_accounts(seen.os),
                detail: problem.to_owned(),
                retry: retry(seen.os),
            },
            _ => WaitingShown::Reading {
                title: READING.into(),
            },
        },
    }
}

/// What a window and the Open Link window say while the accounts are read for the first time.
const READING: &str = "Reading accounts…";

/// Reading the accounts again, as somebody asked. Offered while a read runs too, as
/// AccountWindowView.swift and AccountPicker.swift offered it, where the accounts pane holds
/// its own back.
fn retry(os: Os) -> Choice {
    Choice {
        title: words::try_again(os),
        intent: Intent::Refresh { asked: true },
        enabled: true,
    }
}

/// The link on one line, without its scheme, as the picker shows it; the whole of it is the
/// link's own `url`.
fn link_text(url: &str) -> String {
    url.strip_prefix("https://").unwrap_or(url).to_owned()
}

/// What the Open Link window shows, while a link waits.
fn picker(seen: &Seen) -> Option<LinkPicker> {
    let windows = &seen.state.windows;
    let arrival = windows.arrival.as_ref()?.id;
    let shown = match windows.picking(seen.state.status.as_ref(), seen.problem())? {
        Picking::Reading => PickerShown::Reading {
            title: READING.into(),
        },
        Picking::ReadFailed(problem) => PickerShown::ReadFailed {
            title: words::could_not_read_accounts(seen.os),
            detail: problem,
            retry: retry(seen.os),
        },
        Picking::Refused(refusal) => PickerShown::Refused {
            title: words::cannot_open_link(seen.os),
            reason: refusal.to_string(),
        },
        Picking::NoAccount(link) => {
            let name = link.site.name.clone();
            PickerShown::NoAccount {
                title: words::titled(
                    seen.os,
                    &format!("No {name} Account"),
                    &format!("No {name} account"),
                ),
                link_text: link_text(&link.url),
                detail: format!(
                    "None of the accounts Pitboard has opens {name}. Add one, and this link \
                     waits here until you choose it."
                ),
                in_browser: words::titled(seen.os, "Open in Browser", "Open in browser"),
                add: Choice {
                    title: words::titled(seen.os, "Add Account…", "Add account…"),
                    intent: Intent::PresentSheet {
                        sheet: Sheet::Add {
                            provider: Some(link.site.provider.clone()),
                        },
                    },
                    enabled: true,
                },
                link,
            }
        }
        Picking::Choose {
            link,
            accounts,
            chosen,
        } => PickerShown::Choose {
            title: words::open_link_as(&link.site.name),
            link_text: link_text(&link.url),
            accounts: accounts
                .into_iter()
                .map(|window| {
                    let open = windows.is_showing(&window.store);
                    PickerAccount {
                        detail: if open {
                            format!("{}, window open", window.email)
                        } else {
                            window.email.clone()
                        },
                        window,
                    }
                })
                .collect(),
            chosen,
            open: "Open".into(),
            link,
        },
    };
    Some(LinkPicker {
        arrival,
        armed: windows.armed,
        shown,
    })
}

/// A download as its window's list shows it.
fn download(download: &Download) -> DownloadShown {
    DownloadShown {
        id: download.id.clone(),
        store: download.store.clone(),
        name: download.name.clone(),
        said: match &download.state {
            DownloadState::Finished { .. } => Some("Downloaded".into()),
            DownloadState::Failed { reason } => Some(reason.clone()),
            DownloadState::Cancelled => Some("Cancelled".into()),
            DownloadState::Starting | DownloadState::Running { .. } => None,
        },
        running: download.running(),
        state: download.state.clone(),
    }
}

/// What to ask before quitting while `running` downloads are under way, as Safari asks:
/// quitting stops them, and they do not resume. Nothing while none is.
///
/// The app counts what it has under way as it is asked to quit, and does not take the count
/// from a snapshot: a download started a moment before Quit is under way before the snapshot
/// that lists it, and Pitboard would quit without asking.
#[uniffi::export]
pub fn downloads_quit_question(running: u32) -> Option<Question> {
    let title = match running {
        0 => return None,
        1 => "A download is in progress. Quit anyway?".to_owned(),
        _ => format!("{running} downloads are in progress. Quit anyway?"),
    };
    Some(Question {
        title,
        message: "Quitting Pitboard stops them, and they will not resume.".into(),
        confirm: "Quit".into(),
    })
}
