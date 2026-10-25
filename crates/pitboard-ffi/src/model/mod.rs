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
//! doctor's checks, the activity log and the `pitboard` a terminal would run.
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
mod removing;
#[cfg(test)]
mod signing;
#[cfg(test)]
mod stowing;
#[cfg(test)]
mod switching;
#[cfg(test)]
pub(crate) mod testing;
#[cfg(test)]
mod threaded;
#[cfg(test)]
mod warming;
#[cfg(test)]
mod windowing;

use crate::account_windows::AlertText;
use crate::present::{
    AccountMenuShown, AccountSection, AccountWindowsShown, AccountsShown, FailureText, Footing,
    ItemOffer, MachineShown, MenuBarText, MenuNotices, PanelNotice, RemovalShown, SetupStep,
    SheetText, SigningInText, StowText, WindowQuestion, present,
};
use crate::{AppCore, Warning};
use lanes::Lanes;
use state::{Cadence, Msg, Now, State};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    /// The person let Pitboard quit the app the quit question last asked names, whatever it
    /// asked for: a switch, a sign-in, recording an account. It is asked to quit the way a
    /// person quits it, what was asked for is done once it has, and the copy that was running
    /// is opened again where the core says it may be. An app still open after thirty
    /// seconds, or one that leaves part of itself running, stops it before anything has
    /// changed.
    ///
    /// It names the question it answers, as AppModel.swift's quitAndSwitch took it, because
    /// an alert can say it closed before its button acts: it is taken before or after
    /// `KeepAppOpen` closed that question, and does nothing once taken, or once another
    /// switch has been asked for in that question's place.
    QuitAndSwitch { qualified: String },
    /// From the notification or notice of a switch by itself, which left `from` for `to`.
    SwitchBack { from: String, to: String },
    /// The person kept the app open, or the question was closed some other way: the quit
    /// question goes, and nothing is switched.
    KeepAppOpen,
    /// The person agreed to put the account `qualified` names in place of a login in use its
    /// service refuses for good, which the switch to it asked about. It names the question
    /// it answers, and is taken before or after `KeepLogin` closed that question, as
    /// `QuitAndSwitch` is.
    ReplaceAndSwitch { qualified: String },
    /// The person kept the login in use, or the question was closed some other way: the
    /// question goes, and nothing is switched.
    KeepLogin,
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
    /// Sign in to `provider`'s tool, a `Tool`'s code: again to the account `again` names, or to
    /// a new one, which the core adds under a name from its email as the sign-in finishes and
    /// `Sheet::Name` then offers to change. One at a time; a tool that signs in inside its app
    /// asks the quit question first.
    SignIn {
        provider: String,
        again: Option<String>,
    },
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
    /// running with nothing on screen to finish or stop it. A sheet that signs in, put up
    /// where none was, asks again which tools are installed.
    PresentSheet { sheet: Sheet },
    /// The sheet over the main window has gone, however it went, and what went wrong in it
    /// goes with it. A sign-in it started goes on until it ends or is cancelled.
    CloseSheet,
    /// Show `place`, or the main window as it was left: `Snapshot::window_request` moves.
    Show { place: Option<Place> },
    /// Anything that starts the app can hand it any text, so only what a notice asks for is read.
    NotificationClicked { argument: String },
    /// Somebody has read the failure the window said, and put it away.
    DismissFailure,
    /// The sheet's name or tool changed, so the failure about the last of them goes.
    DismissSheetFailure,
    /// Look again which tools are installed, while a sheet that signs in is up.
    LookForTools,
    /// Add the login `provider`'s tool has signed in now, with no browser, as `SignIn` adds a
    /// new one; where the tool stores none, its sign-in is offered instead.
    Enrol { provider: String },
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
    /// Write the account `qualified` names, the one in use, into its tool's config where that
    /// names another account, as `pitboard use` with it does: the button on the notice that
    /// says Claude Code's config names another account. A change like a rename and not a
    /// switch, so nothing said about a switch or the account in use goes. Refused, writing
    /// nothing, where another account is in use by then. What it warned of is said beside
    /// the read after it, and what stopped it in the window.
    UpdateConfig { qualified: String },
    /// Put away the login left in the file behind Claude Code's store, as the sheet for it
    /// says, `Sheet::Stow`: once that sheet has looked, and only while the file still holds
    /// what it looked at. Not where the login is of an account nobody enrolled, which the sheet
    /// says, and not a second time while one is under way. What goes wrong is said in the
    /// sheet; once it is done the sheet closes, what it did is said until somebody puts it
    /// away, and the accounts are read again.
    Stow,
    /// Somebody has read what putting away the login left in a file did, and put it away.
    DismissStowed,
    /// `pane` of the main window is shown, or its own button asks for what it shows to be
    /// read again, as Check Again and the activity's Refresh do: what it shows is read every
    /// time, as the Swift panes read it on every visit, since an app runs for days and a check
    /// fixed in a terminal since would otherwise still read as failing. For `Pane::Checks`,
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
    /// Taken only once the preferences are read, as `AtLimitShown::enabled` says.
    SetAtLimit { choice: AtLimit, at: u8 },
    /// "Not Now" to a setup tip, kept with the app's preferences.
    DeclineTip { tip: Tip },
    /// What the app's system says of opening the app at login. A setup step offers it for the
    /// app to do itself, which then says what its system says.
    OpensAtLogin { on: bool },
    /// The settings, on what happens at a limit, from the tip that asks, which it ends.
    ShowLimits,
    /// Turned on, an account due is warmed up at once.
    SetWarm { qualified: String, on: bool },
    /// Warm up accounts only from `from` to `to`, in minutes of the day, `to` up to 24 hours;
    /// a `from` later than `to` runs past midnight.
    SetWarmRange { from: u16, to: u16 },
    /// Warm up the account `qualified` names now, whatever the span of the day: its own menu's
    /// **Warm Up Now**. One warm-up at a time: asked for while one is under way, it does
    /// nothing. What goes wrong is said in the window.
    WarmNow { qualified: String },
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
    /// Once `Snapshot::uninstall` was confirmed: nothing more is read or written until it ends.
    Uninstall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Tip {
    UseCodex,
    OpenAtLogin,
    AtLimit,
}

// What the app does once a limit of the account in use reaches the share chosen. Only Claude
// Code switches: Codex is notified of under Switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AtLimit {
    Nothing,
    Notify,
    Switch,
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
    /// The account just added under `label`, which the core picked, to be given a name.
    Name {
        provider: String,
        label: String,
        email: String,
    },
    /// A new name for an enrolled account.
    Rename { provider: String, label: String },
    /// Putting away the login Claude Code left in a file behind the keychain: put up, it
    /// looks at the file, and says whose the login is once that is known, before anything is
    /// changed. `Snapshot::stow_text` is what it says.
    Stow,
}

/// A sign-in under way, as far as its tool has said.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RunningSignIn {
    /// Tells this sign-in from any other, where the Swift model compared objects: one higher
    /// for each sign-in asked for.
    pub id: u64,
    /// The address the tool printed for a person to open when the browser did not open by
    /// itself, as printed, for the app to open or to copy into another browser. Read by the
    /// tool's own module.
    pub url: Option<String>,
    /// Whether to offer a field for the code the browser shows, which the tool is waiting to
    /// have typed back. Read by the tool's own module.
    pub wants_code: bool,
}

// The app a quit question asks to quit, for the account `qualified` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QuitQuestion {
    pub(crate) qualified: String,
    // as the core's holder detection names it and `AppControl` takes it: on macOS its bundle
    // id, on Windows its Application User Model ID
    pub(crate) app_id: String,
    // as a person knows it: "ChatGPT"
    pub(crate) name: String,
}

// For a tool whose running sessions keep the account they started with until started again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RestartNeeded {
    // the command a person quits and starts again: `codex`
    pub program: String,
    // by its label alone, since the tool is named already
    pub from: String,
}

// What a tool's last switch said that the reads after it do not say again, until the tool no
// longer has that account signed in or somebody puts it away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LastSwitch {
    pub provider: String,
    // as the core types it: bare for Claude Code, `codex/work` for any other tool
    pub to: String,
    // when sessions already open will have picked it up, for a tool that follows by itself
    pub follows_at: Option<i64>,
    pub restart: Option<RestartNeeded>,
    // what a sign-in that replaced the account's login in use said, since no switch did
    pub said: Option<String>,
    pub warnings: Vec<Warning>,
    // For a tool whose app holds the login from when it is next opened: that, as a sentence.
    pub opens: Option<String>,
    // where switching by itself made it
    pub back: Option<Back>,
}

// The account a switch by itself left, which Switch Back puts back in use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Back {
    pub from: String,
    // when the limit it left resets
    pub resets_at: Option<i64>,
}

// Something a person asked for that did not happen, numbered one higher for each said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Failure {
    pub id: u64,
    // what was being done, as an alert's title says it: "Couldn’t switch to personal"
    pub title: String,
    pub message: String,
    pub code: Option<String>,
    pub warnings: Vec<Warning>,
}

// What the model reads afresh each time an app shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Pane {
    Accounts,
    Activity,
    Checks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Place {
    Accounts,
    Activity,
    // doctor's checks, which the apps call Diagnostics
    Checks,
    Settings { section: SettingsSection },
    // what the first launch shows
    Welcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SettingsSection {
    General,
    Limits,
    Advanced,
}

// Shown each time `serial` moves; a failure or a question asked from a menu is said in a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct WindowRequest {
    pub serial: u64,
    // none: the main window as it was left
    pub place: Option<Place>,
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
    /// Whether a read of the accounts is under way, so a Refresh button can say so.
    pub reading: bool,
    /// The question asked before Pitboard quits an app that holds a tool's login, for a
    /// switch, a sign-in or the account it shows, while it is asked.
    pub quit_question: Option<WindowQuestion>,
    /// The question a switch asks before it replaces a login its service refuses for good.
    pub replace_question: Option<WindowQuestion>,
    pub window_request: WindowRequest,
    /// The sign-in under way, from the moment it is asked for until it has finished, failed
    /// or been cancelled. Nothing else is signed in to meanwhile, so the rows hold back.
    pub signing_in: Option<RunningSignIn>,
    /// The sheet over the main window, while one is asked for.
    pub sheet: Option<Sheet>,
    /// What went wrong in the sheet that is up, said inside it, where what was typed is still
    /// there to correct: a sign-in started from it that could not start or finish, or a name
    /// it could not save. Numbered with the failures said in the window, and gone with the
    /// sheet.
    pub sheet_failure: Option<FailureText>,
    /// What the menu bar item says beside its mark.
    pub menu_bar: MenuBarText,
    /// The accounts, a section per tool once there is more than one, each as its row.
    pub sections: Vec<AccountSection>,
    pub account_menu: AccountMenuShown,
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
    // when the accounts were last read
    pub updated_at: Option<i64>,
    /// When the accounts were read, as the menu's Refresh item says it: "Updated 14:05".
    pub updated_menu: String,
    /// The same, as the window's subtitle says it.
    pub updated_window: String,
    /// What the sheet over the main window says, while one is up that names an account.
    pub sheet_text: Option<SheetText>,
    /// What a sign-in under way says.
    pub signing_in_text: Option<SigningInText>,
    /// What the sheet for putting away the login left in a file says, while it is up.
    pub stow_text: Option<StowText>,
    /// The alert for something asked for that did not happen, until somebody has read it.
    pub failure_alert: Option<AlertText>,
    /// What the settings and the window's other panes show of this machine rather than its
    /// accounts: daily renewal, doctor's checks, the activity log and the command line a
    /// terminal runs.
    pub machine: MachineShown,
    /// The account windows: which accounts have one, the windows open and what each loads,
    /// the windows to close and the stores to delete, the link waiting for an account, and
    /// the downloads.
    pub account_windows: AccountWindowsShown,
    // Uninstall Pitboard…, with its question
    pub uninstall: ItemOffer,
    pub removal: Option<RemovalShown>,
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

// A notification, posted once for what it says: an account near or past a limit, a login to
// sign in to again, or what switching or warming up by itself came to.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RunOutNotice {
    // the same for the same news, so the system shows it once
    pub id: String,
    pub title: String,
    // the tool, beside another tool's accounts
    pub subtitle: Option<String>,
    pub body: String,
    // what a click on the notification hands back, as `Intent::NotificationClicked`
    pub clicked: String,
    pub button: Option<NotificationButton>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NotificationButton {
    pub title: String,
    // what a click on it hands back, as `Intent::NotificationClicked`
    pub clicked: String,
}

// A button any notification can carry, by its title, as macOS registers it at launch.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NotificationButtonKind {
    pub title: String,
    // what it asks for always opens a window, so the system brings the app forward with it
    pub opens_window: bool,
    // the SF Symbol macOS shows beside it
    pub symbol: String,
}

// The app's system's notifications, called on a thread of the model's own. Asking for
// permission is the app's, and a refusal is no error: the window says the same either way.
#[uniffi::export(with_foreign)]
pub trait Notifications: Send + Sync {
    // This Mac and This PC show why it was refused, so the reason names no path or secret.
    fn post(&self, notice: RunOutNotice) -> Result<(), PlatformError>;
}

/// The person's own clock, as the app's system formats it: their locale, and whether they read
/// 12 or 24 hours. Asked as a snapshot is made, on the model's own thread, and as doctor's
/// checks are made, on the lane that makes them, which every method answers at once without
/// waiting on anything. Where one throws, the command line's form is said instead.
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
    // `minutes` past midnight, under a day, as the person's clock says it: "06:00", "6:00 AM".
    fn time_of_day(&self, minutes: u16) -> Result<String, PlatformError>;
}

/// Told of every snapshot, by the app.
#[uniffi::export(with_foreign)]
pub trait ModelListener: Send + Sync {
    /// A newer snapshot, on a thread of the model's own, never the app's main thread, and
    /// one call at a time, in order. It may call `snapshot` and `send` on the model, and
    /// should hand the snapshot to the app's main thread and return: the next one waits.
    fn changed(&self, snapshot: Snapshot) -> Result<(), PlatformError>;
}

#[uniffi::export(with_foreign)]
pub trait AppControl: Send + Sync {
    /// Where the running copy of the app was opened from, which is what `reopen` takes, or
    /// `None` when it is not running.
    fn running(&self, app: String) -> Result<Option<String>, PlatformError>;
    /// Asks the app to quit the way a person quits it, which lets it ask about work in
    /// progress. Returns at once; the app may take a while, or decline.
    fn request_quit(&self, app: String) -> Result<(), PlatformError>;
    // Not `open`, which UniFFI 0.31.2 backticks into the C header and no C compiler takes.
    // front: brought to the front, for a sign-in in the app; otherwise behind Pitboard's window
    fn reopen(&self, location: String, front: bool) -> Result<(), PlatformError>;
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
    #[cfg(test)]
    next_look: Arc<lanes::NextLook>,
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
            state.windows.on(
                pitboard_core::host::OS,
                &place.launch.link_scheme,
                &place.web_scheme,
            );
        }
        let first = present(&state, clock.now().epoch(), platform.local_time.as_ref());
        let shown = Arc::new(Mutex::new(first));
        let stopped = Arc::new(AtomicBool::new(false));
        let (mailbox, mail) = channel();
        let lanes = Lanes::open(
            core,
            platform.apps,
            platform.notifications,
            Arc::clone(&platform.local_time),
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
        #[cfg(test)]
        let next_look = lanes.next_look();
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
            #[cfg(test)]
            next_look,
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
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

// The timers count time awake, which stops while the machine sleeps, and a snapshot's time is
// the wall clock's.
struct Clock {
    started: Duration,
}

impl Clock {
    fn starting() -> Clock {
        Clock {
            started: pitboard_core::host::awake(),
        }
    }

    fn now(&self) -> Now {
        let epoch_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| {
                i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
            });
        Now {
            running: pitboard_core::host::awake().saturating_sub(self.started),
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

    // Made before the lock is taken: making it asks the app's `LocalTime`, and the model never
    // holds a lock while it calls out.
    fn publish(&self, now: Now) {
        let mut made = present(&self.state, now.epoch(), self.local_time.as_ref());
        let mut shown = lock(&self.shown);
        made.revision = shown.revision;
        if made == *shown {
            return;
        }
        made.revision = shown.revision + 1;
        *shown = made.clone();
        drop(shown);
        let _ = self.tell.send(made);
    }
}
