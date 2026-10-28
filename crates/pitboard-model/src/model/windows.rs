//! The account windows' bookkeeping, apart from any thread, clock or file: which store is
//! whose and the page each window was last on, which windows the app has open and what each
//! loads, which stores go once their account is forgotten, the link waiting for an account,
//! and the downloads. Each app's web code does what this decides and says what happened, so
//! the macOS app's WebKit and the Windows app's WebView2 follow the same rules.
//!
//! From the macOS app's AccountWindows.swift, StoreJanitor.swift (its decisions),
//! WebsiteData.swift's records, LinkInbox.swift, AccountPicker.swift and Downloads.swift, as
//! they were at a3e5ce0, before this moved here.
//!
//! What the windows show follows the accounts shown. What a read says about who is enrolled
//! is acted on wherever a read says it afresh: each read that answered, and what is known
//! read once the account index changed, whether or not reads fail meanwhile and whether or
//! not anything was shown before it. The Swift model told its windows of fewer: not of a read
//! that answered after one that had, saying what was shown, its numbers apart, to the second,
//! and not of the poll's read where nothing was shown before it. Acting on a read that says
//! what the one before said changes nothing, and the poll's read is of the account index
//! every read lists the accounts from. Not acted on: what stands in for a read that failed
//! before anything was shown, the last numbers measured, nor the numbers a session recorded,
//! taken onto what is shown. So a store is deleted only once such a read no longer derives it
//! from any enrolled account, and only where this Pitboard directory recorded it.

use super::DownloadEnd;
use crate::account_windows::records::{Entry, Loaded, store_key};
use crate::account_windows::{
    NavigationPolicy, WindowAccount, WindowNoteKind, is_site_page, opening_note, window_accounts,
    window_address, window_home,
};
use crate::present::{DownloadState, PageLoad, StoreDeletion};
use crate::{SiteLink, Status};
use pitboard_core::host::Os;
use pitboard_sites::LinkRefusal;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

/// The scheme the Pitboard links of an app that named none are on: a release's.
pub(crate) const LINK_SCHEME: &str = "pitboard";

/// The scheme the pages a window keeps are on, outside a fixture.
pub(crate) const WEB_SCHEME: &str = "https";

/// What there is of this launch's records.
#[derive(Debug)]
enum Kept {
    /// Not read yet: nothing is recorded, written or deleted until they are.
    Unread,
    /// Read, and written whole on each change.
    Read(Entry),
    /// Held for this launch alone and never written, and nothing deleted by them: the file is
    /// there and could not be read as records, so writing would lose what it holds, or the
    /// app named nowhere to keep them.
    Held(Entry),
}

impl Kept {
    fn entry(&mut self) -> Option<&mut Entry> {
        match self {
            Kept::Unread => None,
            Kept::Read(entry) | Kept::Held(entry) => Some(entry),
        }
    }

    fn get(&self) -> Option<&Entry> {
        match self {
            Kept::Unread => None,
            Kept::Read(entry) | Kept::Held(entry) => Some(entry),
        }
    }
}

/// A window the app has open, from the moment it says so until it says it has closed.
#[derive(Debug)]
pub(crate) struct Window {
    /// Its store, in lower case: its identity, one window per account.
    pub(crate) store: String,
    /// Whether this Pitboard directory had made its store before this opening, once known.
    made_before: Option<bool>,
    /// The write that records its store, which it waits for: a store WebKit made and nobody
    /// recorded would never be deleted.
    recorded_by: Option<u64>,
    /// The page it loads, numbered, once it can open, and what it says as it opens.
    pub(crate) shown: Option<(PageLoad, Option<WindowNoteKind>)>,
}

/// A Pitboard link the app was asked to open, or why it is not one.
#[derive(Debug, Clone)]
pub(crate) struct Arrival {
    /// One higher for each link that arrives, so the same link arriving twice is two.
    pub(crate) id: u64,
    pub(crate) link: Result<SiteLink, LinkRefusal>,
}

/// What the account picker shows for the link waiting, from the link and the accounts shown.
/// AccountPicker.swift's `PickerState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Picking {
    /// The accounts are being read for the first time.
    Reading,
    /// The accounts could not be read, for the reason given.
    ReadFailed(String),
    /// What arrived is not opened.
    Refused(LinkRefusal),
    /// No enrolled account has a window on the link's site.
    NoAccount(SiteLink),
    /// The accounts with a window on the link's site, and the one chosen until the person
    /// chooses another: chosen last for the site, else the one in use, else the first.
    Choose {
        link: SiteLink,
        accounts: Vec<WindowAccount>,
        chosen: String,
    },
}

/// A download an account window started, as the app says it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Download {
    pub(crate) id: String,
    /// The store of the window whose list it is in.
    pub(crate) store: String,
    pub(crate) name: String,
    pub(crate) state: DownloadState,
}

impl Download {
    /// Waiting for a destination, or for the person to say it may go ahead, or saving: what
    /// quitting would stop.
    pub(crate) fn running(&self) -> bool {
        matches!(
            self.state,
            DownloadState::Starting | DownloadState::Running { .. }
        )
    }
}

/// The account windows' state, which the model's state holds.
#[derive(Debug)]
pub(crate) struct Windows {
    /// The scheme of the Pitboard links this build answers.
    link_scheme: String,
    /// The scheme every page a window keeps is on: `https`, or a fixture's own.
    web_scheme: String,
    /// How long a link waits before it can be opened.
    arming_delay: Duration,
    // On Windows, where WebView2 deletes a profile's folder only as its browser process exits.
    deletion_lingers: bool,
    kept: Kept,
    /// The stores of the accounts the last read that said who is enrolled found, while the
    /// records are unread: what is put away is put away once they are in.
    enrolled: Option<BTreeSet<String>>,
    /// Stores a sweep has claimed, while another directory is asked about and while the app
    /// deletes them: a sweep meanwhile leaves them, and a window of one waits.
    claimed: BTreeSet<String>,
    /// Stores the app is asked to delete, in the order asked, until it says how it went.
    pub(crate) deleting: Vec<StoreDeletion>,
    /// Stores asked for, which numbers the next.
    asks: u64,
    /// The windows the app has open, in the order they opened.
    pub(crate) open: Vec<Window>,
    /// Links chosen for a window that has not opened yet, its first page, by its store.
    waiting: BTreeMap<String, String>,
    /// Pages asked for, which numbers the next.
    loads: u64,
    /// Writes of the records asked for, and the last one answered.
    writes: u64,
    written: u64,
    /// The link waiting for somebody to choose an account, replaced by the next.
    pub(crate) arrival: Option<Arrival>,
    arrivals: u64,
    /// The account last chosen for each site, by the site's host, chosen first next time, as
    /// Safari opens a link in the profile used last. Kept for as long as the app runs.
    last_chosen: BTreeMap<String, String>,
    /// Whether Open answers yet. Each link waits a moment once the accounts to choose from
    /// appear, so a Return typed for another app as the picker came forward opens nothing.
    pub(crate) armed: bool,
    /// What the wait is for: the link, and whether accounts are offered for it.
    armed_for: Option<(u64, bool)>,
    /// When the wait is over, since the model started.
    arming: Option<Duration>,
    /// Every download, oldest first.
    pub(crate) downloads: Vec<Download>,
    // while Pitboard is being removed: every window closes, and none opens
    closing_all: bool,
}

impl Windows {
    pub(crate) fn new(arming_delay: Duration) -> Windows {
        Windows {
            link_scheme: LINK_SCHEME.into(),
            web_scheme: WEB_SCHEME.into(),
            arming_delay,
            deletion_lingers: false,
            kept: Kept::Unread,
            enrolled: None,
            claimed: BTreeSet::new(),
            deleting: Vec::new(),
            asks: 0,
            open: Vec::new(),
            waiting: BTreeMap::new(),
            loads: 0,
            writes: 0,
            written: 0,
            arrival: None,
            arrivals: 0,
            last_chosen: BTreeMap::new(),
            armed: false,
            armed_for: None,
            arming: None,
            downloads: Vec::new(),
            closing_all: false,
        }
    }

    pub(crate) fn on(&mut self, os: Os, link_scheme: &str, web_scheme: &str) {
        self.deletion_lingers = os == Os::Windows;
        link_scheme.clone_into(&mut self.link_scheme);
        web_scheme.clone_into(&mut self.web_scheme);
    }

    /// The rules of `account`'s window.
    pub(crate) fn policy(&self, account: &WindowAccount) -> NavigationPolicy {
        NavigationPolicy {
            site: account.site.clone(),
            scheme: self.web_scheme.clone(),
        }
    }

    /// When the wait before a link can be opened is over, if one is under way.
    pub(crate) fn next_due(&self) -> Option<Duration> {
        self.arming
    }

    /// The wait is over by `now`: Open answers.
    pub(crate) fn go_off(&mut self, now: Duration) {
        if self.arming.is_some_and(|at| at <= now) {
            self.arming = None;
            self.armed = true;
        }
    }

    // MARK: The records

    /// The records as the model starts, or `None` where there are none to write: the file is
    /// there and cannot be read as records, or the app named nowhere to keep them. Kept at
    /// once where the app's earlier store was handed over, so that store is read once. A read
    /// that said who is enrolled meanwhile is acted on now.
    pub(crate) fn loaded(&mut self, loaded: Option<Loaded>, jobs: &mut Vec<super::state::Job>) {
        if !matches!(self.kept, Kept::Unread) {
            return;
        }
        match loaded {
            Some(Loaded {
                entry, handed_over, ..
            }) => {
                self.kept = Kept::Read(entry);
                if handed_over {
                    self.keep(jobs);
                }
            }
            None => self.kept = Kept::Held(Entry::default()),
        }
        if let Some(keeping) = self.enrolled.take() {
            self.put_away(&keeping, jobs);
        }
    }

    /// The records as they are, written whole where they are written at all.
    fn keep(&mut self, jobs: &mut Vec<super::state::Job>) {
        if let Kept::Read(entry) = &self.kept {
            self.writes += 1;
            jobs.push(super::state::Job::KeepWindows {
                entry: entry.clone(),
                write: self.writes,
            });
        }
    }

    /// The write `write` has answered, whether or not it could be made: a window waiting for
    /// its store to be recorded opens all the same, and the record is written again with the
    /// next write.
    pub(crate) fn written(&mut self, write: u64) {
        self.written = self.written.max(write);
    }

    // MARK: After a read

    /// A read said afresh who is enrolled, `status`: a link waiting for a window of an
    /// account no longer enrolled goes, the last page of each such window goes, and every
    /// store this directory recorded that no enrolled account derives is put away. Every
    /// such read lists every enrolled account, from the account index, so that is a
    /// forgotten account, forgotten in the app or with `pitboard forget`.
    pub(crate) fn read_enrolled(&mut self, status: &Status, jobs: &mut Vec<super::state::Job>) {
        let keeping: BTreeSet<String> = window_accounts(&status.accounts)
            .into_iter()
            .map(|window| store_key(&window.store))
            .collect();
        self.waiting.retain(|store, _| keeping.contains(store));
        if matches!(self.kept, Kept::Unread) {
            self.enrolled = Some(keeping);
            return;
        }
        self.put_away(&keeping, jobs);
    }

    // What no account in `keeping` derives, and each held store, enrolled again or not.
    fn put_away(&mut self, keeping: &BTreeSet<String>, jobs: &mut Vec<super::state::Job>) {
        let Some(entry) = self.kept.entry() else {
            return;
        };
        let before = entry.pages.len();
        entry.pages.retain(|store, _| keeping.contains(store));
        let pages_went = entry.pages.len() != before;
        let orphans: Vec<String> = match &self.kept {
            Kept::Read(entry) => entry
                .stores
                .iter()
                .filter(|store| !keeping.contains(*store) || entry.held.contains(*store))
                .filter(|store| !self.claimed.contains(*store))
                .cloned()
                .collect(),
            Kept::Unread | Kept::Held(_) => Vec::new(),
        };
        if pages_went {
            self.keep(jobs);
        }
        if !orphans.is_empty() {
            // Claimed before anything is asked, so a sweep that starts meanwhile leaves them.
            self.claimed.extend(orphans.iter().cloned());
            jobs.push(super::state::Job::CheckShared { stores: orphans });
        }
    }

    /// Which of `stores` another Pitboard directory recorded too, as the file said, or `None`
    /// where it could not be read: those are that directory's to keep, and only taken off this
    /// one's record, and the rest are asked of the app. Where nobody could say, none is
    /// deleted, and the next read puts them away.
    pub(crate) fn shared(
        &mut self,
        stores: Vec<String>,
        shared: Option<Vec<String>>,
        jobs: &mut Vec<super::state::Job>,
    ) {
        let Some(shared) = shared else {
            for store in &stores {
                self.claimed.remove(store);
            }
            return;
        };
        let mut unrecorded = false;
        for store in stores {
            // A held store's deletion is made already, and another record cannot undo it.
            let held = self
                .kept
                .get()
                .is_some_and(|entry| entry.held.contains(&store));
            if shared.contains(&store) && !held {
                self.claimed.remove(&store);
                if let Some(entry) = self.kept.entry() {
                    unrecorded |= entry.stores.remove(&store);
                }
            } else if !self.deleting.iter().any(|asked| asked.store == store) {
                self.asks += 1;
                self.deleting.push(StoreDeletion {
                    store,
                    ask: self.asks,
                });
            }
        }
        if unrecorded {
            self.keep(jobs);
        }
    }

    /// The app deleted `store` and everything in it: it is no longer recorded.
    pub(crate) fn store_deleted(&mut self, store: &str, jobs: &mut Vec<super::state::Job>) {
        let store = store_key(store);
        if !self.unclaim(&store) {
            return;
        }
        if let Some(entry) = self.kept.entry() {
            entry.held.remove(&store);
            let recorded = entry.stores.remove(&store);
            let paged = entry.pages.remove(&store).is_some();
            if recorded || paged {
                self.keep(jobs);
            }
        }
    }

    // It stays recorded for the next read to ask again; on Windows it is held until
    // `StoreDeleted`, enrolled again or not.
    pub(crate) fn store_held(&mut self, store: &str, jobs: &mut Vec<super::state::Job>) {
        let store = store_key(store);
        if !self.unclaim(&store) || !self.deletion_lingers {
            return;
        }
        if self
            .kept
            .entry()
            .is_some_and(|entry| entry.held.insert(store))
        {
            self.keep(jobs);
        }
    }

    pub(crate) fn held(&self) -> Vec<String> {
        self.kept
            .get()
            .map(|entry| entry.held.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub(crate) fn close_all(&mut self, on: bool) {
        self.closing_all = on;
    }

    // Pitboard's accounts are gone: every store this directory recorded alone goes.
    pub(crate) fn remove_all(&mut self, jobs: &mut Vec<super::state::Job>) {
        self.waiting.clear();
        if matches!(self.kept, Kept::Unread) {
            self.enrolled = Some(BTreeSet::new());
            return;
        }
        self.put_away(&BTreeSet::new(), jobs);
    }

    // Once the records are read: an app that keeps none has none to delete.
    pub(crate) fn stores_gone(&self) -> bool {
        !matches!(self.kept, Kept::Unread) && self.claimed.is_empty() && self.deleting.is_empty()
    }

    /// `store` is no longer asked of the app, and no longer claimed. Whether it was asked.
    fn unclaim(&mut self, store: &str) -> bool {
        let Some(at) = self.deleting.iter().position(|asked| asked.store == store) else {
            return false;
        };
        self.deleting.remove(at);
        self.claimed.remove(store);
        true
    }

    // MARK: Windows

    /// The app opened the window of `store`: from a menu, the Dock, the account picker, or
    /// as the system brought it back.
    pub(crate) fn opened(&mut self, store: &str) {
        let store = store_key(store);
        if !self.open.iter().any(|window| window.store == store) {
            self.open.push(Window {
                store,
                made_before: None,
                recorded_by: None,
                shown: None,
            });
        }
    }

    /// The window of `store` has closed.
    pub(crate) fn closed(&mut self, store: &str) {
        let store = store_key(store);
        self.open.retain(|window| window.store != store);
    }

    /// Opens each window that can open now: one whose account is shown, whose store this
    /// directory's records say whether it made, recorded first where it did not, and is not
    /// being deleted. Its first page is a link chosen for it, else the page it was last on
    /// where that is still one of the site's own pages, else the site's home.
    pub(crate) fn open_what_can(
        &mut self,
        status: Option<&Status>,
        jobs: &mut Vec<super::state::Job>,
    ) {
        let Some(status) = status else {
            return;
        };
        if self.closing_all || self.open.iter().all(|window| window.shown.is_some()) {
            return;
        }
        let accounts = window_accounts(&status.accounts);
        for at in 0..self.open.len() {
            if self.open[at].shown.is_some() {
                continue;
            }
            let store = self.open[at].store.clone();
            let Some(account) = accounts
                .iter()
                .find(|account| store_key(&account.store) == store)
            else {
                continue;
            };
            if self.claimed.contains(&store) {
                continue;
            }
            let Some(entry) = self.kept.entry() else {
                continue;
            };
            if entry.held.contains(&store) {
                continue;
            }
            if self.open[at].made_before.is_none() {
                let made = entry.stores.contains(&store);
                if !made {
                    entry.stores.insert(store.clone());
                }
                self.open[at].made_before = Some(made);
                if !made && matches!(self.kept, Kept::Read(_)) {
                    self.keep(jobs);
                    self.open[at].recorded_by = Some(self.writes);
                }
            }
            if self.open[at]
                .recorded_by
                .is_some_and(|write| self.written < write)
            {
                continue;
            }
            let policy = self.policy(account);
            let last = self.kept.entry().and_then(|entry| entry.pages.get(&store));
            let url = self
                .waiting
                .remove(&store)
                .or_else(|| last.filter(|page| restorable(&policy, page)).cloned())
                .unwrap_or_else(|| window_home(policy));
            self.loads += 1;
            let made_before = self.open[at].made_before.unwrap_or(false);
            self.open[at].shown = Some((
                PageLoad {
                    serial: self.loads,
                    url,
                },
                opening_note(made_before),
            ));
        }
    }

    /// The window of `store` is on `url`: kept as its last page where that is one of the
    /// site's own, so the window opens there again.
    pub(crate) fn page_shown(
        &mut self,
        store: &str,
        url: String,
        status: Option<&Status>,
        jobs: &mut Vec<super::state::Job>,
    ) {
        let store = store_key(store);
        let Some(account) = status.and_then(|status| window_of(status, &store)) else {
            return;
        };
        let policy = self.policy(&account);
        if !restorable(&policy, &url) {
            return;
        }
        let Some(entry) = self.kept.entry() else {
            return;
        };
        if entry.pages.get(&store) == Some(&url) {
            return;
        }
        entry.pages.insert(store, url);
        self.keep(jobs);
    }

    /// Everything the window of `store` kept was removed: its last page goes with it.
    pub(crate) fn data_removed(&mut self, store: &str, jobs: &mut Vec<super::state::Job>) {
        let store = store_key(store);
        if self
            .kept
            .entry()
            .is_some_and(|entry| entry.pages.remove(&store).is_some())
        {
            self.keep(jobs);
        }
    }

    /// The stores of the windows the app has open whose account the accounts shown no longer
    /// list, which close.
    pub(crate) fn closing(&self, status: Option<&Status>) -> Vec<String> {
        self.open
            .iter()
            .filter(|window| {
                self.closing_all
                    || status.is_some_and(|status| window_of(status, &window.store).is_none())
            })
            .map(|window| window.store.clone())
            .collect()
    }

    // MARK: The link waiting

    /// A Pitboard link the app was asked to open, replacing one still waiting. It is read as
    /// strictly as any link from outside, since anything on the machine can open one.
    pub(crate) fn link_arrived(&mut self, text: &str) {
        self.arrivals += 1;
        self.arrival = Some(Arrival {
            id: self.arrivals,
            link: pitboard_sites::read_pitboard_link(text, &self.link_scheme).map(SiteLink::from),
        });
    }

    /// The person closed the picker on the link `arrival` without choosing.
    pub(crate) fn dismiss_link(&mut self, arrival: u64) {
        if self.arrival.as_ref().is_some_and(|a| a.id == arrival) {
            self.arrival = None;
        }
    }

    /// The person chose the account of `store` for the link `arrival`: it is opened in that
    /// account's window, where Back returns to what it showed, or as the first page of the
    /// window about to open. Only once Open answers, only for the link still waiting, and only
    /// as an account the picker offers.
    pub(crate) fn open_link(&mut self, arrival: u64, store: &str, status: Option<&Status>) {
        if !self.armed || self.arrival.as_ref().is_none_or(|a| a.id != arrival) {
            return;
        }
        let Some(Picking::Choose { link, accounts, .. }) = self.picking(status, None) else {
            return;
        };
        let store = store_key(store);
        let Some(account) = accounts
            .iter()
            .find(|account| store_key(&account.store) == store)
        else {
            return;
        };
        self.last_chosen
            .insert(link.site.host.clone(), store.clone());
        self.arrival = None;
        let url = window_address(&self.policy(account), &link);
        match self
            .open
            .iter_mut()
            .find(|window| window.store == store && window.shown.is_some())
        {
            Some(window) => {
                self.loads += 1;
                if let Some((load, _)) = &mut window.shown {
                    *load = PageLoad {
                        serial: self.loads,
                        url,
                    };
                }
            }
            None => {
                self.waiting.insert(store, url);
            }
        }
    }

    /// What the picker shows for the link waiting, if one is, from the accounts shown and why
    /// the last read failed, `problem`.
    pub(crate) fn picking(
        &self,
        status: Option<&Status>,
        problem: Option<&str>,
    ) -> Option<Picking> {
        let arrival = self.arrival.as_ref()?;
        let link = match &arrival.link {
            Err(refusal) => return Some(Picking::Refused(refusal.clone())),
            Ok(link) => link.clone(),
        };
        let Some(status) = status else {
            return Some(problem.map_or(Picking::Reading, |problem| {
                Picking::ReadFailed(problem.to_owned())
            }));
        };
        let accounts: Vec<WindowAccount> = window_accounts(&status.accounts)
            .into_iter()
            .filter(|account| account.site.host == link.site.host)
            .collect();
        let mine = |store: &String| {
            accounts
                .iter()
                .any(|account| store_key(&account.store) == *store)
        };
        let chosen = self
            .last_chosen
            .get(&link.site.host)
            .filter(|store| mine(store))
            .cloned()
            .or_else(|| {
                accounts
                    .iter()
                    .find(|account| account.in_use)
                    .map(|account| store_key(&account.store))
            })
            .or_else(|| accounts.first().map(|account| store_key(&account.store)));
        Some(match chosen {
            Some(chosen) => Picking::Choose {
                link,
                accounts,
                chosen,
            },
            None => Picking::NoAccount(link),
        })
    }

    /// Starts the wait again where the link waiting, or whether accounts are offered for it,
    /// has changed since: armed from when the accounts to choose from appear, whether the link
    /// has just arrived or the accounts were read only now.
    pub(crate) fn rearm(&mut self, status: Option<&Status>, problem: Option<&str>, now: Duration) {
        let choosing = matches!(self.picking(status, problem), Some(Picking::Choose { .. }));
        let waiting_for = self.arrival.as_ref().map(|arrival| (arrival.id, choosing));
        if waiting_for == self.armed_for {
            return;
        }
        self.armed_for = waiting_for;
        self.armed = false;
        self.arming = choosing.then_some(now + self.arming_delay);
    }

    // MARK: Downloads

    /// A page of the window of `store` started the download `id`, called `name` by the address
    /// it came from where that names one. Downloads carry on after their window closes: the
    /// person asked for the file, not for the window to stay open.
    pub(crate) fn download_started(&mut self, id: String, store: &str, name: Option<String>) {
        if self.downloads.iter().any(|download| download.id == id) {
            return;
        }
        self.downloads.push(Download {
            id,
            store: store_key(store),
            name: name
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "Download".into()),
            state: DownloadState::Starting,
        });
    }

    /// The download `id` is saving to `file`, whose name it is called by from now on. Only
    /// one still starting: one cancelled or failed while it waited stays as it ended.
    pub(crate) fn download_saving(&mut self, id: &str, file: String) {
        if let Some(download) = self
            .downloads
            .iter_mut()
            .find(|download| download.id == id && download.state == DownloadState::Starting)
        {
            if let Some(name) = std::path::Path::new(&file).file_name() {
                download.name = name.to_string_lossy().into_owned();
            }
            download.state = DownloadState::Running { file };
        }
    }

    /// The download `id` has ended as `end` says. Only one running ends, once: what ended is
    /// not ended again by what comes after. A download that finished before it was given a
    /// file to save to is left as it was, as the Swift left it.
    pub(crate) fn download_ended(&mut self, id: &str, end: DownloadEnd) {
        let Some(download) = self
            .downloads
            .iter_mut()
            .find(|download| download.id == id && download.running())
        else {
            return;
        };
        download.state = match (end, &download.state) {
            (DownloadEnd::Finished, DownloadState::Running { file }) => {
                DownloadState::Finished { file: file.clone() }
            }
            (DownloadEnd::Finished, _) => return,
            (DownloadEnd::Failed { reason }, _) => DownloadState::Failed { reason },
            (DownloadEnd::Cancelled, _) => DownloadState::Cancelled,
        };
    }

    /// The downloads of the window of `store` that have ended are taken away.
    pub(crate) fn clear_downloads(&mut self, store: &str) {
        let store = store_key(store);
        self.downloads
            .retain(|download| download.store != store || download.running());
    }

    /// Whether the window of `store` is open and showing its page.
    pub(crate) fn is_showing(&self, store: &str) -> bool {
        let store = store_key(store);
        self.open
            .iter()
            .any(|window| window.store == store && window.shown.is_some())
    }

    /// The links waiting for a window still to open, by store, for a test.
    #[cfg(test)]
    pub(crate) fn waiting(&self) -> &BTreeMap<String, String> {
        &self.waiting
    }

    /// This directory's records, as the model holds them, for a test.
    #[cfg(test)]
    pub(crate) fn records(&self) -> Option<&Entry> {
        match &self.kept {
            Kept::Unread => None,
            Kept::Read(entry) | Kept::Held(entry) => Some(entry),
        }
    }
}

/// The window of the account whose store is `store`, in lower case, among the accounts
/// `status` lists.
fn window_of(status: &Status, store: &str) -> Option<WindowAccount> {
    window_accounts(&status.accounts)
        .into_iter()
        .find(|account| store_key(&account.store) == store)
}

/// Whether `url` is a page a window may open on again: one of the site's own, and not its
/// sign-in, which would sign the window in again with whatever it carried.
fn restorable(policy: &NavigationPolicy, url: &str) -> bool {
    if !is_site_page(policy, url) {
        return false;
    }
    let path = pitboard_sites::WebAddress::parse(url);
    pitboard_sites::Site::serving(&policy.site.host).is_none_or(|site| !site.signs_in(path.path()))
}
