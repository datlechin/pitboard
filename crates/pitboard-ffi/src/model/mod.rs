//! The app's model in Rust: what both apps show, and every decision about it.
//!
//! An app makes one [`PitboardModel`], sends it what the person or the system asked for as
//! an [`Intent`], and shows the [`Snapshot`]s its [`ModelListener`] is told of. Nothing it
//! exports is async, and nothing it exports waits on the core: `send` puts an intent in the
//! model's mailbox and waits for nothing, `snapshot` takes the lock the actor holds only
//! while it compares and copies a snapshot, and `shutdown` waits for the actor to take the
//! messages already in its mailbox, none of which waits on anything, and for each sign-in
//! under way to stop.
//!
//! Inside, one thread, the actor, owns the `State` and nothing else touches it. It takes
//! each message in turn, has `State::apply` say what follows, which does no I/O at all, and
//! runs the jobs that returns on the lanes in `lanes.rs`, whose answers come back to it as
//! messages, so a read still answers while a switch waits on the core or an app is given its
//! time to quit. Each sign-in runs on a thread of its own, which reads what the tool says
//! until it stops, as the Swift model's SignInCalls read it. Between messages it waits until
//! the next timer is due. After each message it makes the snapshot, and where that differs
//! from the last one it numbers it one higher and hands it to the one notifier thread, which
//! tells the listener in order.
//!
//! It reads accounts, notices changes made elsewhere, switches, quits an app that holds a
//! login when asked to, keeps what each tool's last switch said, runs sign-ins, enrols,
//! renames and forgets, keeps the sheet over the main window, and says which account to
//! switch to once the one in use has run out, notifying it through the app's
//! `Notifications` once for each reset, across launches. It also keeps what is about this
//! machine rather than its accounts: the daily renewal schedule, repaired once a launch,
//! renewing now, doctor's checks, the activity log and the `pitboard` a terminal would run.
//! And it keeps the account windows' books, in `windows.rs`: which store is whose and the
//! page each window was last on, kept in a file of the app's own, which windows close and
//! which stores go after a read, the link waiting for an account and the downloads.
//! What each snapshot says of all that, every sentence and row the menu bar, the menu, the
//! window and the settings show, is made by `present`, in `crate::present`, which asks the
//! app's `LocalTime` for each clock time and date. A minute tick makes it again for what
//! depends on the time alone.

pub(crate) mod advice;
mod lanes;
pub(crate) mod machine;
pub(crate) mod preferences;
pub(crate) mod state;
pub(crate) mod windows;

#[cfg(test)]
mod advising;
#[cfg(test)]
mod automatic;
#[cfg(test)]
mod cadence;
#[cfg(test)]
mod changing;
#[cfg(test)]
mod keeping;
#[cfg(test)]
mod maintaining;
#[cfg(test)]
mod presenting;
#[cfg(test)]
mod reading;
#[cfg(test)]
mod signing;
#[cfg(test)]
mod switching;
#[cfg(test)]
mod testing;
#[cfg(test)]
mod threaded;
#[cfg(test)]
mod windowing;

use crate::account_windows::AlertText;
use crate::present::{
    AccountSection, AccountWindowsShown, AccountsShown, Footing, MachineShown, MenuBarText,
    MenuNotices, PanelNotice, Question, SetupStep, SheetText, SigningInText, present,
};
use crate::{Abandoned, AppCore, Status, Tool, Warning};
use lanes::Lanes;
use state::{Cadence, Msg, Now, State};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// What the app was started with, which the model reads as the command line reads its own:
/// the environment, and where the app is. Not called `Launch`, which the macOS app already
/// has a type of.
#[derive(Debug, Clone, uniffi::Record)]
pub struct AppLaunch {
    /// The environment the app was started with, every variable of it.
    pub environment: HashMap<String, String>,
    /// Where the app is: on macOS the `.app`, which names the command line inside it that the
    /// renewal schedule runs. `None` for anything that is not an app, such as a test or a
    /// build directory.
    pub app_location: Option<String>,
    /// The app's own preferences as its earlier store held them, before the model kept them
    /// in Pitboard's directory: on macOS what UserDefaults holds, for the model to take once.
    /// Where the model's own file is there it wins, and this is not read. `None` for an app
    /// with no earlier store.
    #[uniffi(default)]
    pub earlier_preferences: Option<EarlierPreferences>,
    /// Where the account windows' records are kept, and what this launch is to them. `None`
    /// for an app that keeps them nowhere: its windows are then recorded for as long as it
    /// runs, and no store is deleted.
    #[uniffi(default)]
    pub windows: Option<WindowsLaunch>,
}

/// Where the account windows' records are kept, and what this launch is to them.
///
/// The records are the app's whichever Pitboard directory it serves, as its web stores are:
/// WebKit keeps every store of one app under the person's own Library, whatever `HOME` says.
/// So they are kept in a directory of the app's own, not in Pitboard's directory, and each
/// Pitboard directory's are kept apart in them, under `key`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WindowsLaunch {
    /// A directory of the app's own, the same whatever `HOME` says: on macOS the app's folder
    /// in Application Support, named by its bundle id. The model keeps `windows.json` there,
    /// making the directory where it is not there yet.
    pub directory: String,
    /// What this launch's Pitboard directory is called in the records: on macOS the
    /// directory as Foundation standardises a file URL, as the app keyed `webStores` and
    /// `windowPages` before the model kept them. Another key would leave every store recorded
    /// before nobody's, never to be deleted, and every window without its last page.
    pub key: String,
    /// The scheme of the Pitboard links this build answers, as its Share extension writes
    /// them: `pitboard`, or `pitboard-debug` for a debug build.
    pub link_scheme: String,
    /// What the app's earlier store held of the records, before the model kept them: on macOS
    /// UserDefaults' `webStores` and `windowPages`, for the model to take once. Where
    /// `windows.json` is there it wins, and this is not read. `None` for an app with no
    /// earlier store.
    #[uniffi(default)]
    pub earlier: Option<EarlierWindowRecords>,
}

/// The account windows' records as an app's earlier store held them, every Pitboard
/// directory's, by the key each was kept under, as `WindowsLaunch` hands them over once. A
/// store id may be in either case: Foundation writes a UUID in upper case.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct EarlierWindowRecords {
    /// The stores each directory's windows were made with: `webStores`.
    pub stores: HashMap<String, Vec<String>>,
    /// The page each window was last on, by its store, for each directory: `windowPages`.
    pub pages: HashMap<String, HashMap<String, String>>,
}

/// How a download an account window started ended.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum DownloadEnd {
    /// The whole file is where it was saving to.
    Finished,
    /// It stopped, for the reason given, as the app's system says it.
    Failed { reason: String },
    /// Somebody stopped it, or Pitboard did, as it quit.
    Cancelled,
}

/// The app's own preferences as an app's earlier store held them, as `AppLaunch` hands them
/// over once.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EarlierPreferences {
    /// The tools, by code, somebody said "Not Now" to a second account for:
    /// `secondAccountDeclined`.
    pub second_account_declined: Vec<String>,
    /// Whether the app has ever shown anybody anything: `hasBeenSeen`.
    pub has_been_seen: bool,
    /// "Not Now" said before there was a second tool, which was about Claude Code:
    /// `hideSecondAccountNudge`.
    pub second_account_nudge_hidden: bool,
}

/// Something the person or the system asked the model for.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Intent {
    /// Start what runs by itself: a read now and every five minutes, a look every two seconds
    /// for a change made somewhere else, which reads what is already known, and the minute
    /// tick; and read what the model keeps in Pitboard's directory, the record of what was
    /// told. It also repairs, once a launch, a daily renewal schedule an app up to 0.3.0
    /// wrote, which runs that app and renews nothing: where this home's schedule is such a
    /// one, the core writes it again in the machine's own scheduler, as `pitboard schedule
    /// install` would, to run the command line inside this copy, and logs the repair. That
    /// changes the machine, so only the app sends this, over the home it serves, and a test
    /// only over a scratch home. Until an app sends this, nothing runs by itself. Sent again,
    /// it starts nothing more.
    Start,
    /// The machine woke from sleep: numbers read before it slept say nothing about now. An
    /// app sends it every time: on macOS the model's timers count only the time the machine
    /// is awake, so without it the timer's read after a long sleep comes up to five minutes
    /// late.
    Woke,
    /// Somebody looked, by opening the app's menu: the accounts are read again where the
    /// numbers shown are a minute old.
    Glanced,
    /// Read the accounts again. `asked` is somebody asking, from a Refresh button, rather
    /// than something producing it, and has each tool's service asked about every account
    /// whatever it was asked moments ago.
    Refresh { asked: bool },
    /// Switch to the account `qualified` names, its label with its tool as
    /// `Account::qualified` gives it, from the window, the menu or a notification. One switch
    /// at a time: asked for while another is under way, it does nothing, beyond bringing a
    /// question about quitting an app back to the front. Where an app keeps the tool's login
    /// in memory, as ChatGPT keeps Codex's, the switch waits on `Snapshot::quit_question`.
    SwitchTo { qualified: String },
    /// The person let Pitboard quit the app the quit question for the switch to `qualified`
    /// names: it is asked to quit the way a person quits it, the switch is made once it has,
    /// and the copy that was running is opened again whether or not the switch worked. An
    /// app still open after thirty seconds stops the switch before anything has changed.
    ///
    /// It names the question it answers, as AppModel.swift's quitAndSwitch took it, because
    /// an alert can say it closed before its button acts: it is taken before or after
    /// `KeepAppOpen` closed that question, and does nothing once taken, or once another
    /// switch has been asked for in that question's place.
    QuitAndSwitch { qualified: String },
    /// The person kept the app open, or the question was closed some other way: the quit
    /// question goes, and nothing is switched. The answer of the button that closed it, sent
    /// after this, still switches.
    KeepAppOpen,
    /// Somebody has read what `provider`'s tool's last switch said, and put it away.
    DismissSwitch { provider: String },
    /// Somebody keeps one account of `provider`'s tool on purpose: "Not Now" to the nudge to
    /// add a second. Per tool, since that says nothing about another, and kept in the app's
    /// preferences in Pitboard's directory.
    DeclineSecondAccount { provider: String },
    /// Give up on an interrupted switch nothing can finish, keeping every login it names.
    AbandonStuckSwitch,
    /// Somebody has read what giving up on an interrupted switch kept, and put it away.
    DismissAbandoned,
    /// Sign in to `provider`'s tool, a `Tool`'s code, through the tool's own sign-in in a
    /// browser, as the account to be called `name`: a new account, or one enrolled already
    /// whose parked login can no longer be used, signed in to again, as the Swift model signs
    /// in to both. The name is taken without the white space around it, and one with nothing
    /// else in it starts nothing. One sign-in at a time: asked for while one is under way, it
    /// does nothing. What goes wrong is said in the sheet it was started from, or in the
    /// window where that sheet has gone, and the sheet closes once the sign-in has finished.
    /// Signing in again to the account in use is kept as what its tool's last switch said.
    SignIn { provider: String, name: String },
    /// Type back the code the browser showed, to the sign-in under way, once its tool asks
    /// for one: `RunningSignIn::wants_code`. Taken without the white space around it, and one
    /// with nothing else in it is not typed. A code is asked for once, and again once Claude
    /// Code has refused the one typed back, since it reads another in the same sign-in.
    PasteCode { code: String },
    /// Stop the sign-in under way, at whichever of three moments it comes. Before the tool
    /// has started, the tool is stopped once it has, rather than watched. While the tool waits
    /// on the browser, it is stopped now, and nothing is enrolled. Once the tool has finished
    /// and what it signed in to is being enrolled, it comes too late to stop that, so the
    /// accounts are read once it is, and nothing else is touched. Nothing is said of it, since
    /// somebody asked for it to stop, and its sheet stays until the app closes it.
    CancelSignIn,
    /// Put `sheet` over the main window, and open the window on the accounts it is about. A
    /// sign-in under way keeps its own sheet: replacing it would leave the tool's sign-in
    /// running with nothing on screen to finish or stop it. The sheet for a new account, put
    /// up where none was, asks again which tools are installed: the first answer may have come
    /// while the person's login shell was too slow to say.
    PresentSheet { sheet: Sheet },
    /// The sheet over the main window has gone, however it went, and what went wrong in it
    /// goes with it. A sign-in it started goes on until it ends or is cancelled.
    CloseSheet,
    /// Open the main window, on `pane` where it matters which, from the menu, a notification
    /// or anywhere else: `Snapshot::window_request` moves. The sheet up, if any, stays.
    ShowWindow { pane: Option<Pane> },
    /// Somebody has read the failure the window said, and put it away.
    DismissFailure,
    /// Enrol the login signed in now to `provider`'s tool under `name`, with no browser: the
    /// Name sheet's Save. The name is taken without the white space around it, as
    /// `name_to_save` takes it, and one with nothing else in it saves nothing; nor does a
    /// second Save from a sheet already saving. Once enrolled, the sheet naming a login of
    /// that tool closes, and only that one, and the accounts are read; what goes wrong is
    /// said in the sheet it was asked from, or in the window where that sheet has gone.
    Enrol { provider: String, name: String },
    /// Give the account `label` of `provider`'s tool the name `to`, inside its own tool: the
    /// Rename sheet's Save, by the same rules, and a name it has already saves nothing. What
    /// was said about the account is said about it under its new name.
    Rename {
        provider: String,
        label: String,
        to: String,
    },
    /// Drop the account `qualified` names, and the login parked for it, once somebody has
    /// answered the question its row asks first. What goes wrong is said in the window.
    Forget { qualified: String },
    /// `pane` of the main window is shown, or its own button asks for what it shows to be
    /// read again, as Check Again and the activity's Refresh do: what it shows is read every
    /// time, as the Swift panes read it on every visit, since an app runs for days and a check
    /// fixed in a terminal since would otherwise still read as failing. For `Pane::Machine`,
    /// every check `pitboard doctor` makes; for `Pane::Activity`, what Pitboard has changed;
    /// for `Pane::Accounts`, the accounts where the numbers shown are a minute old, as for
    /// `Glanced`. What was shown stays meanwhile.
    PaneShown { pane: Pane },
    /// Read the daily renewal schedule again, as the settings do when they show it: a
    /// terminal can change it while the app runs.
    ReadSchedule,
    /// Hand renewing parked logins to this machine's own scheduler, or take it back: the
    /// settings' switch. One change at a time: the switch shows what was asked for until the
    /// scheduler has answered and the schedule has been read back, and pressed again meanwhile
    /// it does nothing. Turning it on is refused, with why, where this copy of the app has no
    /// command line that a schedule would keep reaching, and nothing is asked of the
    /// scheduler; turning it off never is, since that is how such a schedule is taken away.
    SetSchedule { on: bool },
    /// Switch Claude Code by itself, or stop: the settings' switch, with the share of a limit
    /// it switches at, a whole percentage, taken as the nearest there can be. Kept in the
    /// app's preferences in Pitboard's directory, and taken only once they have been read,
    /// as `MachineShown::auto_switch` says by `enabled`. Turned on, it looks at once at what
    /// was read last.
    SetAutoSwitch { on: bool, at: u8 },
    /// Renew every parked login that is due, now, then read the accounts again once, asking
    /// every service, to show what it renewed. Never switches, and asks for no usage beyond
    /// that read. Asked for while a renewal is under way, it does nothing.
    RenewNow,
    /// Look for the `pitboard` a terminal would run, as the settings do when they show it, and
    /// as an app does once it has linked its own onto the `PATH`.
    LookForCommandLine,
    /// An account's window opened, the one whose store is `store`: from a menu, the Dock, the
    /// account picker, or as the system brought it back. Its store is recorded before the
    /// app makes it, since a store made and never recorded would never be deleted, and once
    /// it is, `AccountWindowsShown::open` lists the window with the page it starts at. A
    /// store is compared without regard to case.
    WindowOpened { store: String },
    /// The window whose store is `store` has closed.
    WindowClosed { store: String },
    /// The page of the window whose store is `store` is now `url`: kept as the page it opens
    /// at next time, where it is one of its site's own pages and not its sign-in.
    PageShown { store: String, url: String },
    /// Everything the window whose store is `store` keeps was removed, as somebody asked:
    /// the page it was last on goes too.
    WebsiteDataRemoved { store: String },
    /// The app deleted the store `store`, which `AccountWindowsShown::deleting` asked for,
    /// and everything in it.
    StoreDeleted { store: String },
    /// The app could not delete the store `store`, which something still holds: it stays
    /// recorded, and is asked for again after the next read.
    StoreHeld { store: String },
    /// A Pitboard link the app was asked to open, as `text`, such as one the Share extension
    /// wrote. Nothing opens by itself: it waits in `AccountWindowsShown::picker` for somebody
    /// to choose an account, replacing one still waiting.
    LinkArrived { text: String },
    /// Somebody chose the account whose store is `store` for the link `arrival` names, and
    /// the app opens that account's window: the link is its first page, or loads in it where
    /// it is open. Only once `LinkPicker::armed` says Open answers, and only for the link
    /// still waiting.
    OpenLink { arrival: u64, store: String },
    /// The picker on the link `arrival` names was closed without a choice.
    DismissLink { arrival: u64 },
    /// A page of the window whose store is `store` started a download, which the app calls
    /// `id`, named `name` after the address it came from where that names one.
    DownloadStarted {
        id: String,
        store: String,
        name: Option<String>,
    },
    /// The download `id` is saving to `file`, the path the app chose with
    /// `download_destination`.
    DownloadSaving { id: String, file: String },
    /// The download `id` has ended.
    DownloadEnded { id: String, end: DownloadEnd },
    /// Somebody cleared the downloads of the window whose store is `store` that have ended.
    ClearDownloads { store: String },
}

/// A sheet over the main window, as the Swift model's `AccountSheet` has them. Not called
/// `AccountSheet`, which the macOS app already has a type of.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Sheet {
    /// A new account, which means the tool's own sign-in in a browser. The tool it starts on,
    /// as a `Tool`'s code, when something already said which.
    Add { provider: Option<String> },
    /// An enrolled account whose parked login can no longer be used, signed in to again
    /// through the same sign-in as a new one.
    SignInAgain { provider: String, label: String },
    /// The login signed in now to this tool, to be recorded under a name: no browser.
    Name { provider: String, email: String },
    /// A new name for an enrolled account.
    Rename { provider: String, label: String },
}

/// A sign-in under way, and what its tool has said so far.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RunningSignIn {
    /// Tells this sign-in from any other, where the Swift model compared objects: one higher
    /// for each sign-in asked for.
    pub id: u64,
    /// The tool, as a `Tool`'s code.
    pub provider: String,
    /// The name the account is to be enrolled under, bare, as it was asked for.
    pub name: String,
    /// Everything the tool has said so far, as it said it.
    pub said: String,
    /// The address the tool printed for a person to open when the browser did not open by
    /// itself, as printed, to make a link of. Read by the tool's own module.
    pub url: Option<String>,
    /// Whether to offer a field for the code the browser shows, which the tool is waiting to
    /// have typed back. Read by the tool's own module.
    pub wants_code: bool,
    /// Whether the tool refused the last code typed back, and reads another in its place:
    /// the field is offered again, and the sheet can say why.
    pub code_refused: bool,
}

/// A switch waiting for the person to let Pitboard quit an app first: the app keeps the
/// tool's login in memory, would go on with the account switched away from, and its own
/// sign-out would revoke the login Pitboard had just parked.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct QuitQuestion {
    /// The account to switch to, its label with its tool.
    pub qualified: String,
    /// The app, as the core's holder detection names it and `AppControl` takes it: on macOS
    /// its bundle id.
    pub app_id: String,
    /// The app as a person knows it: "ChatGPT".
    pub name: String,
}

/// What a switch means for a tool's running sessions, for a tool whose sessions never pick
/// a switch up: they keep the account they started with until they are started again.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RestartNeeded {
    /// The command a person quits and starts again: `codex`.
    pub program: String,
    /// The account they keep using, by its label alone, since the tool is named already.
    pub from: String,
}

/// What a tool's last switch said that the read after it does not say again, until the
/// tool no longer has the account it switched to signed in, or somebody puts it away. One
/// per tool: a switch of one tool says nothing about another's sessions.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LastSwitch {
    /// The tool, as a `Tool`'s code.
    pub provider: String,
    /// The account switched to, as the core types it: bare for Claude Code, `codex/work`
    /// for any other tool.
    pub to: String,
    /// When sessions already open will have picked it up, in epoch seconds, for a tool that
    /// follows a switch by itself.
    pub follows_at: Option<i64>,
    /// For a tool whose running sessions never pick a switch up.
    pub restart: Option<RestartNeeded>,
    /// What a sign-in that put a new login in use in place of the account's old one said it
    /// did, where that is what this is: sessions already running are left on the old login
    /// exactly as a switch leaves them on the old account. Kept beside what the tool's last
    /// switch said, which it leaves as it was, since the tool did not switch.
    pub said: Option<String>,
    /// What the switch warned about, every warning of it.
    pub warnings: Vec<Warning>,
}

/// Something a person asked for that did not happen, for the window to say: an alert's
/// title and its message.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Failure {
    /// One higher for each failure said, so an app tells a new one from one it has shown.
    pub id: u64,
    /// What was being done, as an alert's title says it: "Couldn’t switch to personal".
    pub title: String,
    /// What went wrong. Pitboard's errors already say what to do, so it is shown as it is.
    pub message: String,
    /// The stable code behind it, for deciding what to offer, where the core gave one.
    pub code: Option<String>,
    /// Everything else it warned about.
    pub warnings: Vec<Warning>,
}

/// A pane of the main window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Pane {
    Accounts,
    Activity,
    Machine,
}

/// What the model wants of the main window: it is opened each time `serial` moves, on
/// `pane` when that says which. A menu has nowhere to put a sentence or a question, so a
/// failure or a quit question asked for from one is said in the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct WindowRequest {
    /// How many times the window has been asked for.
    pub serial: u64,
    /// The pane the last request wants shown, when it wants one.
    pub pane: Option<Pane>,
}

/// Why the last read of the accounts did not answer.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReadFailure {
    /// Stable, for the app to branch on: an offer survives its message changing.
    pub code: String,
    /// Pitboard's own sentence, which already says what to do, to show as it is.
    pub message: String,
}

/// Everything the model has to show, as of one moment.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Snapshot {
    /// One higher than the snapshot made before it. The listener is told only the newest of
    /// those waiting, and `snapshot` can give one it was never told, so an app can see 3 and
    /// then 5. It keeps the snapshot with the highest it has seen and drops any other.
    pub revision: u64,
    /// When this snapshot was made, in epoch seconds: the moment anything in it that depends
    /// on the time was worked out at. Not every passing second makes a new snapshot.
    pub now: i64,
    /// Whether a read of the accounts is under way, so a Refresh button can say so.
    pub reading: bool,
    /// When the accounts were last read, in epoch seconds. `None` before the first read
    /// that answered.
    pub updated_at: Option<i64>,
    /// The accounts, as the last read gave them or with newer numbers some session has
    /// recorded since, or the last numbers measured where no read has answered yet. `None`
    /// before anything is known, which is not a machine without accounts.
    pub status: Option<Status>,
    /// Everything the last read warned about, or what its failure did, and before those what
    /// a switch that failed since warned about.
    pub warnings: Vec<Warning>,
    /// Why the last read did not answer, when it did not. `status` is then the last
    /// numbers measured.
    pub read_failure: Option<ReadFailure>,
    /// An interrupted switch nothing can finish, which the app offers a way out of.
    pub stuck: bool,
    /// The tools whose program was found, in the order a listing shows them, once the
    /// first read has asked. A tool missing here may still be on some `PATH`, so this
    /// narrows what is offered and never forbids anything.
    pub installed: Option<Vec<Tool>>,
    /// The account a switch is under way for, or waiting on the quit question for, as its
    /// label with its tool, from the moment it is asked for until the read after it has
    /// landed. Nothing else is switched meanwhile, so the menu and the rows hold back.
    pub switch_under_way: Option<String>,
    /// A switch waiting for the person to let Pitboard quit an app first, to ask in the
    /// window: `Intent::QuitAndSwitch` with its `qualified`, or `Intent::KeepAppOpen`,
    /// answers it.
    pub quit_question: Option<QuitQuestion>,
    /// What each tool's last switch said that is still true, one per tool at most: a tool's
    /// newer switch takes the place of what its last said, and one kept once the last was put
    /// away goes after every other tool's. Apart from `warnings`, which the read after every
    /// switch replaces: these are about the switch, and stay true until the tool no longer
    /// has the account it switched to signed in, or somebody puts them away.
    pub last_switches: Vec<LastSwitch>,
    /// What giving up on an interrupted switch kept, until somebody has read it.
    pub abandoned: Option<Abandoned>,
    /// The last thing asked for that did not happen, which an app says once: a newer one
    /// has a higher `id`. It stays here until somebody has read it, `Intent::DismissFailure`,
    /// or another takes its place.
    pub failure: Option<Failure>,
    /// What the model wants of the main window.
    pub window_request: WindowRequest,
    /// The sign-in under way, from the moment it is asked for until it has finished, failed
    /// or been cancelled. Nothing else is signed in to meanwhile, so the rows hold back.
    pub signing_in: Option<RunningSignIn>,
    /// The sheet over the main window, while one is asked for.
    pub sheet: Option<Sheet>,
    /// What went wrong in the sheet that is up, said inside it, where what was typed is still
    /// there to correct: a sign-in started from it that could not start or finish, or a name
    /// it could not save. Numbered with `failure`, and gone with the sheet.
    pub sheet_failure: Option<Failure>,
    /// What the menu bar item says beside its mark.
    pub menu_bar: MenuBarText,
    /// The accounts, a section per tool once there is more than one, each as its row.
    pub sections: Vec<AccountSection>,
    /// Whether accounts of more than one tool are shown, which is when anything says which
    /// tool an account is for.
    pub shows_tools: bool,
    /// Everything to tell somebody that is not an account, the most pressing first.
    pub notices: Vec<PanelNotice>,
    /// What the menu says of the notices.
    pub menu_notices: MenuNotices,
    /// How far setting Pitboard up this machine is.
    pub footing: Footing,
    /// The one next thing to do on a machine that is not set up yet.
    pub setup: Option<SetupStep>,
    /// What the window's accounts pane shows: its list, or what stands in for one.
    pub accounts_shown: AccountsShown,
    /// What the menu says where it has no accounts to list.
    pub menu_accounts_note: Option<String>,
    /// When the accounts were read, as the menu's Refresh item says it: "Updated 14:05".
    pub updated_menu: String,
    /// The same, as the window's subtitle says it.
    pub updated_window: String,
    /// What the sheet over the main window says, while one is up.
    pub sheet_text: Option<SheetText>,
    /// What a sign-in under way says.
    pub signing_in_text: Option<SigningInText>,
    /// What the quit question asks, while it is asked.
    pub quit_confirmation: Option<Question>,
    /// The alert for `failure`, while there is one to say.
    pub failure_alert: Option<AlertText>,
    /// What the settings and the window's other panes show of this machine rather than its
    /// accounts: daily renewal, Renew Now, doctor's checks, the activity log and the command
    /// line a terminal runs.
    pub machine: MachineShown,
    /// The account windows: which accounts have one, the windows open and what each loads,
    /// the windows to close and the stores to delete, the link waiting for an account, and
    /// the downloads.
    pub account_windows: AccountWindowsShown,
}

/// What a platform's own code could not do. Every method of a trait an app implements
/// returns it, so an exception the app's code throws reaches the model as an error rather
/// than as a crash.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum PlatformError {
    #[error("{reason}")]
    Failed { reason: String },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for PlatformError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> PlatformError {
        PlatformError::Failed {
            reason: error.reason,
        }
    }
}

/// A notification: that an account in use has run out, and which account of the same tool has
/// room, posted once for each reset of a limit, whether the app was quit and opened again
/// meanwhile or not; or what came of switching Claude Code by itself, where somebody turned
/// that on.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RunOutNotice {
    /// The same for the same run-out, so the system shows it once: the account, the limit
    /// and its reset.
    pub id: String,
    /// "work has no 5-hour limit left".
    pub title: String,
    /// The tool, beside another tool's accounts: "Claude Code".
    pub subtitle: Option<String>,
    /// "spare has 80% of its own left.", or "seat has no such limit."
    pub body: String,
    /// The account its Switch button switches to, its label with its tool, for
    /// `Intent::SwitchTo`. `None` for a notification with nothing to switch to, which has no
    /// button: one saying what Pitboard switched by itself.
    pub switch_to: Option<String>,
}

/// The app's system's notifications. Called on a thread of the model's own, never the app's
/// main thread, and answers at once: asking for permission, the first time there is
/// something to say, is the app's to do then, and a refusal is no error, since the window
/// says the same either way.
#[uniffi::export(with_foreign)]
pub trait Notifications: Send + Sync {
    /// Posts `notice`, with a button that switches to its `switch_to` where it names one.
    fn post(&self, notice: RunOutNotice) -> Result<(), PlatformError>;
}

/// The person's own clock, as the app's system formats it: their locale, and whether they read
/// 12 or 24 hours. Asked as a snapshot is made, on the model's own thread, which every method
/// answers at once without waiting on anything. Where one throws, the command line's form is
/// said instead.
#[uniffi::export(with_foreign)]
pub trait LocalTime: Send + Sync {
    /// The time of `epoch`, in epoch seconds, as a clock shows it: "14:05", "2:05 PM". With
    /// its weekday abbreviated before it where `with_weekday` says so: "Wed 14:05".
    fn clock(&self, epoch: i64, with_weekday: bool) -> Result<String, PlatformError>;
    /// Whether two moments, in epoch seconds, fall on one day of the person's calendar, in
    /// their time zone: whether a clock time needs its weekday, which only the system knows.
    fn same_day(&self, first: i64, second: i64) -> Result<bool, PlatformError>;
    /// The date and time of `epoch`, in epoch seconds, as a list of what happened when says
    /// it, a date abbreviated and a time short. Where it throws, a change's own time is said
    /// instead, as the log keeps it and `pitboard log` prints it.
    fn date_and_time(&self, epoch: i64) -> Result<String, PlatformError>;
}

/// Told of every snapshot, by the app.
#[uniffi::export(with_foreign)]
pub trait ModelListener: Send + Sync {
    /// A newer snapshot, on a thread of the model's own, never the app's main thread, and
    /// one call at a time, in order. It may call `snapshot` and `send` on the model, and
    /// should hand the snapshot to the app's main thread and return: the next one waits.
    fn changed(&self, snapshot: Snapshot) -> Result<(), PlatformError>;
}

/// Other apps on this machine, by an app id, which Pitboard may quit and open again around a
/// switch: one that runs a tool for itself and keeps the tool's login in memory while it is
/// open, as ChatGPT does with Codex's. The id is the one the core's holder detection names
/// an app by, which on macOS is its bundle id. What names an app on Windows is still the
/// owner's to decide, and the id is one string so that a Windows id fits.
///
/// Only quitting the way a person quits an app and opening one again are offered: never a
/// forced quit, which would lose whatever the app had not saved. Called on a thread of the
/// model's own, never the app's main thread, one call at a time. An error from `running` or
/// `request_quit` is taken as the app still running, since Pitboard never switches under an
/// app it could not see go; one from `reopen` leaves the app for the person to open.
#[uniffi::export(with_foreign)]
pub trait AppControl: Send + Sync {
    /// Where the running copy of the app was opened from, which is what `reopen` takes, or
    /// `None` when it is not running.
    fn running(&self, app: String) -> Result<Option<String>, PlatformError>;
    /// Asks the app to quit the way a person quits it, which lets it ask about work in
    /// progress. Returns at once; the app may take a while, or decline.
    fn request_quit(&self, app: String) -> Result<(), PlatformError>;
    /// Opens the app at `location` again, as `running` gave it, the way the system's own
    /// shell would, without bringing it to the front: whatever Pitboard has to say about the
    /// switch stays in front of it. Not called `open`, which UniFFI 0.31.2's list of Swift
    /// keywords has: it writes a name on that list in backticks into the C header's table
    /// of calls as well as into the Swift, and no C compiler takes it there.
    fn reopen(&self, location: String) -> Result<(), PlatformError>;
}

/// The app's model. Making one starts its threads and asks nothing of anyone: the core is
/// made when the first read needs it, which can mean asking the person's login shell.
#[derive(uniffi::Object)]
pub struct PitboardModel {
    mailbox: Sender<Msg>,
    shown: Arc<Mutex<Snapshot>>,
    /// Set once the model is stopped, after which the listener is told nothing more.
    stopped: Arc<AtomicBool>,
    actor: Mutex<Option<JoinHandle<()>>>,
    /// The sign-ins under way, which `shutdown` waits for.
    sign_ins: Arc<lanes::SignIns>,
}

#[uniffi::export]
impl PitboardModel {
    /// The app's model, over the core the app's environment and location make, read the
    /// way the command line reads its own; over `apps`, the other apps on this machine;
    /// posting what has run out through `notifications`; and saying clock times as
    /// `local_time` does, the person's own. Nothing runs by itself until the app sends
    /// `Intent::Start`.
    #[uniffi::constructor]
    pub fn new(
        launch: AppLaunch,
        listener: Arc<dyn ModelListener>,
        apps: Arc<dyn AppControl>,
        notifications: Arc<dyn Notifications>,
        local_time: Arc<dyn LocalTime>,
    ) -> Arc<Self> {
        PitboardModel::over(
            AppCore::for_app(launch.environment, launch.app_location),
            listener,
            Platform {
                apps,
                notifications,
                local_time,
                earlier: launch.earlier_preferences,
                windows: launch.windows.map(|launch| WindowsPlace {
                    launch,
                    web_scheme: windows::WEB_SCHEME.into(),
                }),
            },
            Cadence::APP,
        )
    }

    /// The last snapshot, for a first paint and whenever an app wants it. A lock and a
    /// copy, on any thread, the listener's own included.
    pub fn snapshot(&self) -> Snapshot {
        lock(&self.shown).clone()
    }

    /// Asks the model for `intent`. Waits for nothing and calls nothing back: what comes of
    /// it is told to the listener. Once the model has stopped, nothing comes of it.
    pub fn send(&self, intent: Intent) {
        let _ = self.mailbox.send(Msg::Intent(intent));
    }

    /// Stops the model: its timers, its lanes once each has finished what it is doing, its
    /// listener, which is called no more once this returns, beyond a call already under way
    /// on its own thread, and every sign-in under way, which has stopped by the time this
    /// returns. Waits for the actor to end, which it does once it has taken the messages
    /// already in its mailbox, none of which waits on anything; then for the lane of calls
    /// to sign-ins, which stops each sign-in once the actor has gone, to have stopped each
    /// one's tool and waited for it, and for a sign-in still starting, or enrolling what it
    /// signed in to, to end, for no longer than ten seconds in all. Nothing else, and
    /// nothing a sign-in does waits on the listener, so the listener may call it too.
    ///
    /// For an app to call as it quits: the listener holds the app, which holds the model, so
    /// neither is freed before that, and the app's process ends once this returns, which
    /// ends no tool it started. Returning before the lane had stopped them, it let the
    /// process end first.
    pub fn shutdown(&self) {
        self.stop();
        let actor = self.actor.lock().ok().and_then(|mut actor| actor.take());
        if let Some(actor) = actor
            && actor.thread().id() != std::thread::current().id()
        {
            let _ = actor.join();
            let _ = self.sign_ins.stopped_within(SIGN_INS_STOP_WITHIN);
        }
    }
}

/// How long `shutdown` waits for the sign-ins under way to stop. One still starting can wait
/// on the person's login shell, which the core gives five seconds
/// (`host::unix::shell::PATIENCE`), and stops its tool at once once started. Past this the
/// app quits all the same.
const SIGN_INS_STOP_WITHIN: Duration = Duration::from_secs(10);

/// What the model asks of the app's own system.
pub(crate) struct Platform {
    /// Other apps on this machine.
    pub(crate) apps: Arc<dyn AppControl>,
    /// The system's notifications.
    pub(crate) notifications: Arc<dyn Notifications>,
    /// The person's own clock.
    pub(crate) local_time: Arc<dyn LocalTime>,
    /// The app's preferences as its earlier store held them, taken once.
    pub(crate) earlier: Option<EarlierPreferences>,
    /// Where the account windows' records are kept, for an app that keeps them.
    pub(crate) windows: Option<WindowsPlace>,
}

/// Where the account windows' records are kept, and the scheme their pages are on.
pub(crate) struct WindowsPlace {
    pub(crate) launch: WindowsLaunch,
    /// The scheme every page a window keeps is on: `https`, or a fixture's own.
    pub(crate) web_scheme: String,
}

impl PitboardModel {
    /// A model over `core` and `platform`, doing what it does by itself every `cadence`.
    pub(crate) fn over(
        core: Arc<AppCore>,
        listener: Arc<dyn ModelListener>,
        platform: Platform,
        cadence: Cadence,
    ) -> Arc<PitboardModel> {
        let clock = Clock::starting();
        let mut state = State::new(cadence);
        if let Some(place) = &platform.windows {
            state
                .windows
                .on(&place.launch.link_scheme, &place.web_scheme);
        }
        let first = present(&state, clock.now().epoch(), platform.local_time.as_ref());
        let shown = Arc::new(Mutex::new(first));
        let stopped = Arc::new(AtomicBool::new(false));
        let (mailbox, mail) = channel();
        let lanes = Lanes::open(
            core,
            platform.apps,
            platform.notifications,
            lanes::Keeping {
                earlier: platform.earlier,
                windows: platform
                    .windows
                    .map(|place| lanes::RecordsFile::of(place.launch)),
            },
            cadence,
            &mailbox,
        );
        let sign_ins = lanes.sign_ins();
        let actor = Actor {
            state,
            lanes,
            shown: Arc::clone(&shown),
            tell: lanes::notifier(listener, Arc::clone(&stopped)),
            clock,
            local_time: platform.local_time,
        };
        let actor = std::thread::Builder::new()
            .name("pitboard-model".into())
            .spawn(move || actor.run(&mail))
            .expect("the system starts a thread for the model");
        Arc::new(PitboardModel {
            mailbox,
            shown,
            stopped,
            actor: Mutex::new(Some(actor)),
            sign_ins,
        })
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = self.mailbox.send(Msg::Stop);
    }
}

/// Stops the model and waits for nothing: .NET's finalizer thread can be the one dropping
/// it, and must never wait on the model's threads.
impl Drop for PitboardModel {
    fn drop(&mut self) {
        self.stop();
    }
}

fn lock(shown: &Mutex<Snapshot>) -> MutexGuard<'_, Snapshot> {
    // A snapshot is whole whenever the lock is let go, so one held through a panic is still
    // worth showing.
    shown
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The model's two clocks: `Instant` for the timers, which on macOS is `CLOCK_UPTIME_RAW` and
/// stops while the machine sleeps, and the wall clock for what a snapshot says the time is.
struct Clock {
    started: Instant,
}

impl Clock {
    fn starting() -> Clock {
        Clock {
            started: Instant::now(),
        }
    }

    fn now(&self) -> Now {
        let epoch_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| {
                i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
            });
        Now {
            running: self.started.elapsed(),
            epoch_ms,
        }
    }
}

/// The actor thread's own: the state, the lanes, and where snapshots go.
struct Actor {
    state: State,
    lanes: Lanes,
    shown: Arc<Mutex<Snapshot>>,
    tell: Sender<Snapshot>,
    clock: Clock,
    local_time: Arc<dyn LocalTime>,
}

impl Actor {
    /// Takes each message in turn, waiting no longer than the next timer, until told to
    /// stop. Dropping the lanes and the notifier's sender then ends their threads.
    fn run(mut self, mail: &Receiver<Msg>) {
        loop {
            let msg = match self.state.next_due() {
                None => mail.recv().unwrap_or(Msg::Stop),
                Some(due) => {
                    match mail.recv_timeout(due.saturating_sub(self.clock.now().running)) {
                        Ok(msg) => msg,
                        Err(RecvTimeoutError::Timeout) => Msg::Tick,
                        Err(RecvTimeoutError::Disconnected) => Msg::Stop,
                    }
                }
            };
            if matches!(msg, Msg::Stop) {
                return;
            }
            let now = self.clock.now();
            for job in self.state.apply(msg, now) {
                self.lanes.run(job);
            }
            self.publish(now);
        }
    }

    /// Hands the listener a snapshot where anything in it changed. Compared with the last
    /// one under its revision and time, so neither moving alone makes a new one, and made
    /// before the lock is taken: making it asks the app's `LocalTime`, and the model never
    /// holds a lock while it calls out.
    fn publish(&self, now: Now) {
        let mut made = present(&self.state, now.epoch(), self.local_time.as_ref());
        let mut shown = lock(&self.shown);
        (made.revision, made.now) = (shown.revision, shown.now);
        if made == *shown {
            return;
        }
        (made.revision, made.now) = (shown.revision + 1, now.epoch());
        *shown = made.clone();
        drop(shown);
        let _ = self.tell.send(made);
    }
}
