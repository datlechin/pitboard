//! What the model knows and decides, apart from any thread, clock or core.
//!
//! [`State::apply`] takes one message and the moment it arrived, changes what is known and
//! says which jobs to run. It calls nothing, reads no clock and waits on nothing. The actor
//! hands it every message in turn and runs the jobs it returns on the lanes, whose answers
//! come back as messages.
//!
//! A call the Swift model awaited is a job here, with two exceptions. A read's two stamps and
//! the read itself are one job, where the Swift awaited three calls, and a look's two stamps
//! are one, where it awaited two. The guards keep their meaning: a read's ticket is taken as
//! its job is made, which is after what is installed was asked, as in the Swift, and compared
//! when its answer lands. One interleaving goes with them. A look that noticed a change could
//! land between a read's stamps and the read in the Swift, which then dropped the read. Here
//! nothing runs between them on the lane of reads: the read lands with the stamps from
//! before the change, and the next look notices it, which ends where the Swift did.
//!
//! A switch is claimed here, as the intent is taken and before its first job is queued, so a
//! second asked for meanwhile does nothing, and it stays claimed until the read after it has
//! landed, as the Swift model's `switching` stayed set until its read returned. Meanwhile the
//! change poll leaves the account index alone: the change is the switch's own.
//!
//! The poll is a look at when the account index and the readings were last written, asked as
//! the watch on Pitboard's directory hears a write to either, on a thread of its own, and
//! every few seconds only once it says nothing watches them. A write heard while a look is
//! under way, or while a change of this app's own holds the index, is looked at once that is
//! over.
//!
//! Every other change this app makes to the account index holds it the same way, from the
//! moment it is asked for, a sign-in's from the moment it is told to enrol, and a change of an
//! app's login once the app is seen closed, until the read after it is over or the change has
//! failed. The Swift model held nothing for them, so a look that found the index as one had
//! just written it, and was compared once that change had taken its read's ticket, counted
//! the change as one made elsewhere and dropped that read.
//!
//! A sign-in is told apart by its id, where the Swift model compared objects with `===`. Its
//! thread hands on what the tool says as it says it, and once the tool has stopped saying
//! anything it waits to be told whether to enrol what it signed in to: only the sign-in still
//! under way is enrolled, which is the Swift's check of `signingIn === shown` after its loop.
//! So a cancel lands on one side of each answer or the other, at the three moments the Swift
//! handles: before the tool has started, while it waits on a browser, and while what it signed
//! in to is enrolled.
//!
//! A test hands `apply` answers in whatever order it likes, which is how the interleavings
//! the Swift model's tests reached with gates are reached here without a thread.

use super::advice::{Advice, HeadsUp, Told, rename_told};
use super::lanes::Lane;
use super::machine::{LOG_LIMIT, MachineState, Scheduled};
use super::preferences::Preferences;
use super::windows::Windows;
use super::{
    AtLimit, Back, Cause, Doing, Failure, Intent, LastSwitch, Notice, Opens, OutsideAsk, Pane,
    Place, QuitQuestion, ReadFailure, RestartNeeded, RunningSignIn, SettingsSection, Sheet,
    SignedIn, Tip, WindowRequest,
};
use crate::account_windows::records::{Entry, Loaded};
use crate::present::{Clicked, GlanceText, UNUSABLE};
use crate::{
    Abandoned, Account, Adoption, AutoLooked, AutoSwitched, Change, Doctored, Enrolled, EnrolledAs,
    FoundCommandLine, Front, GaveUp, OwnCommandLine, PitboardError, Schedule, Status, Surfaces,
    Switch, Switched, Uninstalled, Usage, WarmRefused, Warning, WatchService,
};
use pitboard_core::duty::autoswitch::Threshold;
use pitboard_core::duty::cadence::{
    DECIDE_EVERY_SECONDS, LOOK_EVERY_SECONDS, READ_EVERY_SECONDS, Refusals,
};
use pitboard_core::duty::warm::Overview;
use pitboard_core::provider::{self, ProviderId, SignsIn};
use pitboard_core::state::Key;
use pitboard_core::status::{Stale, Standing};
use pitboard_core::switch::warm::WarmUp;
use pitboard_core::switch::{Foreseen, Left, Stowed, WaitStep};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::time::Duration;

/// How often the model does what it does by itself, and how long it waits for an app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cadence {
    /// How often the poll looks where nothing watches Pitboard's directory.
    pub(crate) look_every: Duration,
    pub(crate) read_every: Duration,
    /// How often, with switching Claude Code by itself on, it asks the core's look though
    /// nothing was written: an account put in use stops settling, and the wait after a
    /// failed attempt ends, with nobody writing anything. As often as `pitboard watch`
    /// decides.
    pub(crate) decide_every: Duration,
    /// How old the numbers shown may be before somebody glancing at them has them read
    /// again: a menu is opened far more often than the numbers change.
    pub(crate) stale_after: Duration,
    /// How long an app has to quit once asked. Long enough for one that asks about work in
    /// progress to be answered; past it, nothing has changed.
    pub(crate) quit_within: Duration,
    /// How often, meanwhile, it is asked whether the app is still running.
    pub(crate) quit_checked_every: Duration,
    /// How often what is shown is made again for the time alone, once started: a reset's
    /// "resets in 2h 05m", a limit used up until a clock time, a parked login's days. A
    /// minute, which is as often as any of them changes, as the Swift app's
    /// `TimelineView(.everyMinute)` drew its bars.
    pub(crate) tick_every: Duration,
    /// How long a link waits, once the accounts to choose from appear, before the account
    /// picker's Open answers: long enough that a Return typed for another app as the picker
    /// came forward opens nothing, as AccountPicker.swift's `armingDelay` waited.
    pub(crate) arming: Duration,
}

impl Cadence {
    /// The app's, as the Swift model kept it.
    pub(crate) const APP: Cadence = Cadence {
        look_every: Duration::from_secs(LOOK_EVERY_SECONDS.unsigned_abs()),
        read_every: Duration::from_secs(READ_EVERY_SECONDS.unsigned_abs()),
        decide_every: Duration::from_secs(DECIDE_EVERY_SECONDS.unsigned_abs()),
        stale_after: Duration::from_secs(60),
        quit_within: Duration::from_secs(30),
        quit_checked_every: Duration::from_millis(200),
        tick_every: Duration::from_secs(60),
        arming: Duration::from_millis(750),
    };
}

/// When a message arrived, by two clocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Now {
    /// Since the model started, by a clock that never goes back: what the timers are set by,
    /// so the wall clock being put back does not hold them up. On macOS it stops while the
    /// machine sleeps, where the Swift loops slept on one that does not, so an app sends
    /// [`Intent::Woke`] as it wakes, which reads at once.
    pub(crate) running: Duration,
    /// Epoch milliseconds by the wall clock: what a snapshot says the time is, and what the
    /// age of the numbers is measured by, as the Swift model measured it by `Date`.
    pub(crate) epoch_ms: i64,
}

impl Now {
    /// Epoch seconds, as every timestamp the bindings carry is.
    pub(crate) fn epoch(self) -> i64 {
        self.epoch_ms.div_euclid(1000)
    }
}

/// What comes to the model, one at a time.
#[derive(Debug)]
pub(crate) enum Msg {
    /// What an app asked for.
    Intent(Intent),
    /// A timer may be due. The actor sends this when it has waited until the next one.
    Tick,
    /// What a job came to.
    Done(Answer),
    /// Stop: the actor ends, and with it the lanes. `apply` does nothing with it.
    Stop,
}

/// Something to do away from the model, on a lane, whose answer comes back as
/// [`Msg::Done`]. It carries what the model needs to make sense of the answer, which the
/// lane hands back untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Job {
    /// Read every account: first when the usage readings and the account index were last
    /// written, as they stand before the read, then the read itself. `fresh` asks each tool's
    /// service about every account whatever it was asked moments ago.
    Read { ticket: Ticket, fresh: bool },
    /// Read what is already known, asking nobody: the last numbers measured, and whose login
    /// each tool has stored, as its service last said.
    ReadOffline { why: Why },
    /// When the account index and the usage readings were last written.
    Look,
    /// Watch Pitboard's directory, on a thread of its own, until the model stops: each write
    /// to the account index or the readings is said as `Answer::Written`, and where nothing
    /// watches them `Answer::Unwatched` is said once, until a watch begins again.
    Watch,
    /// Which tools' programs were found. Its own lane, so it never waits behind a read on
    /// the network, and a read can wait for it.
    AskInstalled,
    /// Whether a running app holds the login of `account`'s tool, which the core answers from
    /// the process list and the app from what is running.
    Holding { account: Key, then: After },
    /// Ask the app the question names to quit, and wait until it has, for as long as the
    /// cadence gives it.
    Quit { question: QuitQuestion, then: After },
    Switch {
        account: Key,
        reopen: Option<String>,
    },
    /// Put `account` in place of a login in use its service refuses for good, as the person
    /// agreed to.
    Replace { account: Key },
    /// Ask the core's look whether to switch Claude Code by itself at `at`%, from its files
    /// alone: what stands, or that only a decision under its lock can say. It claims nothing.
    AutoLook { at: u8 },
    /// Switch Claude Code by itself where a limit of the account in use has reached `at`%
    /// and another account has room: asked once the look says only a decision under the
    /// core's lock can say, which decides whether, and to which, from whose login Claude Code
    /// has stored.
    AutoSwitch { at: u8 },
    /// Warm up the first account turned on that is due, as the core decides, on a lane of its
    /// own: the core looks at its files first, and a warm-up runs Claude Code for seconds.
    WarmUp,
    /// Warm up `account` now, whatever the span of the day.
    WarmNow { account: Key },
    /// Warm up `account` from now on, or stop.
    SetWarm { account: Key, on: bool },
    /// Warm up accounts from `from` to `to`, minutes of the day.
    SetWarmRange { from: u16, to: u16 },
    /// Open the app at `location` again.
    Open {
        location: String,
        tool: ProviderId,
        front: bool,
    },
    /// Give up on an interrupted switch, keeping every login it names.
    Abandon,
    /// A sign-in, on a thread of its own: start `tool`'s own sign-in, again for the account
    /// `again` names or for a new one, hand on everything the tool says until it stops, then
    /// wait for `SignInOver`.
    SignIn {
        id: u64,
        tool: ProviderId,
        again: Option<String>,
    },
    /// Type `code` back to the tool of the sign-in `id`.
    PasteCode { id: u64, code: String },
    /// Stop the tool of the sign-in `id`, and say whether it did. Once the tool has stopped
    /// saying anything this stops nothing, since what it signed in to may be being enrolled.
    StopSignIn { id: u64 },
    /// The tool of the sign-in `id` has stopped saying anything: enrol what it signed in to,
    /// or let it go.
    SignInOver { id: u64, enrol: bool },
    /// Enrol the login signed in now to `tool`, with no browser: as the account `again` names,
    /// or under a name the core picks. `from` is the sheet that was up when it was asked for,
    /// where what goes wrong is said.
    Enrol {
        tool: ProviderId,
        again: Option<String>,
        from: Option<Sheet>,
        // where the app quit for it was opened from
        reopen: Option<String>,
    },
    /// Give `account` the name `to`, inside its own tool.
    Rename {
        account: Key,
        to: String,
        from: Option<Sheet>,
    },
    /// Drop `account`, and the login parked for it.
    Forget { account: Key },
    /// Write `account` into its tool's config, while it is in use.
    UpdateConfig { account: Key },
    /// Look at the file behind Claude Code's store: what it holds, and whose the login in it
    /// is, which may ask Anthropic with that login's own access token, and whose the login
    /// stored is, as a read asks it. It changes nothing.
    LookLeft,
    /// Put away the file behind Claude Code's store, while it holds what `seen` was taken of.
    /// `from` is the sheet that was up when it was asked for, where what goes wrong is said.
    Stow { seen: String, from: Option<Sheet> },
    /// Read what the model keeps of its own in Pitboard's directory: the record of what was
    /// told, and the app's preferences.
    LoadKept,
    /// Keep the record of what was told, the whole of it.
    KeepTold { told: Told },
    /// Keep the app's preferences, the whole of them.
    KeepPreferences { preferences: Preferences },
    /// Keep this directory's account window records, the whole of them, as the `write`th
    /// write asked for.
    KeepWindows { entry: Entry, write: u64 },
    /// Ask the account windows' records which of `stores` another Pitboard directory that is
    /// still there recorded too.
    CheckShared { stores: Vec<String> },
    /// Post a notification through the app's own system.
    Post { notice: Notice },
    /// Point a renewal schedule an app up to 0.3.0 wrote, which runs that app and renews
    /// nothing, at the command line inside this one: once a launch.
    RepairSchedule,
    /// Read the renewal schedule. `after_change` is the read after a change to it, which
    /// ends that change once it lands.
    ReadSchedule { after_change: bool },
    /// Hand renewing parked logins to this machine's scheduler, or take it back. Turning it
    /// on is not asked of the scheduler where this copy of the app has no command line a
    /// schedule would keep reaching.
    SetSchedule { on: bool },
    /// Read the watch service. `after_change` is the read after a change to it, which ends
    /// that change once it lands.
    ReadWatchService { after_change: bool },
    /// Have this machine's service manager run `pitboard watch`, doing at a limit what
    /// `switching` says, or take it away, refused as `SetSchedule` is.
    SetWatchService {
        on: bool,
        switching: pitboard_core::watch_service::Switching,
    },
    /// Make every check `pitboard doctor` makes.
    Check,
    /// Read the newest `limit` changes Pitboard made.
    ReadLog { limit: u32 },
    /// Look for the `pitboard` a terminal would run.
    FindCommandLine,
    /// Remove Pitboard from this machine, with the app's own hold on its directory, once the
    /// watch on the directory has let go of it.
    Uninstall,
    /// Keep the glance in the file the app named for it.
    KeepGlance { text: String, reload: bool },
}

impl Job {
    // Answered once, by a lane that may write Pitboard's directory, which an uninstall waits for.
    fn writes(&self) -> bool {
        matches!(
            self.lane(),
            Lane::Reads | Lane::Changes | Lane::Warm | Lane::Kept
        )
    }
}

/// A read under way, as things stood when it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ticket {
    /// How many changes made elsewhere the poll had noticed, and made here, when the read
    /// started. A read that lands once there has been another was made with who was signed
    /// in before that change, and is dropped.
    changes_seen: u64,
    /// Whether the timer that reads every few minutes asked for it. That timer is set again
    /// once its read is over, as the Swift model slept only after its read.
    timed: bool,
    /// Whether it is the read after a switch, whose landing ends the switch.
    after_switch: bool,
    /// What the change it is the read after warned of, by its number in `said_after_read`,
    /// which is said once it has landed.
    saying: Option<u64>,
    /// Whether it is the read after a change of this app's own to the account index other
    /// than a switch, whose landing ends that change's hold on the index.
    after_change: bool,
    // the request from outside it answers once over
    request: Option<u64>,
}

/// Why what is already known is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Why {
    /// A read that failed before anything was shown: the last numbers measured are still
    /// true, and an empty list would say the accounts are gone.
    InPlaceOf(Ticket),
    /// The account index changed somewhere else, which can be a switch: who is signed in,
    /// the numbers, and whether a switch is stuck. `measured` is when the readings were
    /// written, as the look found it.
    Changed { measured: i64 },
    /// Only the readings moved, newer numbers some session or the command line recorded:
    /// the numbers and nothing else.
    Numbers { measured: i64 },
}

/// What came of asking an app to quit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum QuitOutcome {
    /// It quit. `copy` is where the one that was running was opened from, the one to open
    /// again.
    Quit { copy: String },
    /// It was not running, so there was nothing to quit and there is nothing to open.
    NotRunning,
    /// It is still running, as it was, or the app's own code could not say it had gone.
    StillRunning,
    /// It quit, and part of it still runs: nothing has changed, and the copy is opened again.
    Lingering { copy: String },
}

// What follows the quit question, kept by the model alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum After {
    Switch,
    // `again`: the account signed in to again, or none for a new one
    SignIn {
        tool: ProviderId,
        again: Option<String>,
        from: Option<Sheet>,
    },
    Enrol {
        tool: ProviderId,
        again: Option<String>,
        from: Option<Sheet>,
    },
    Uninstall,
}

// What an add of an account of a tool that signs in inside its app does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InAppAdd {
    // records the account the app is signed in to
    Current,
    // parks the account in use and waits for the app to quit signed in
    AfterSignIn,
}

// What a sheet recording the account an app shows is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Recording {
    Quitting,
    Reading,
}

/// What a job came to.
#[derive(Debug)]
pub(crate) enum Answer {
    Read {
        ticket: Ticket,
        /// When the usage readings were last written, in epoch milliseconds, before the read.
        readings_before: i64,
        /// When the account index was last written, in epoch milliseconds, before the read.
        changed_before: i64,
        read: Result<Status, PitboardError>,
        /// Each Claude Code account's warm-up, as the core's files say after the read.
        warm: Result<Overview, PitboardError>,
    },
    ReadOffline {
        why: Why,
        read: Result<Status, PitboardError>,
        warm: Result<Overview, PitboardError>,
    },
    Looked {
        /// When the account index was last written, in epoch milliseconds.
        changed: i64,
        /// When the usage readings were last written, in epoch milliseconds.
        measured: i64,
    },
    /// The watch heard a write to the account index or the readings, or began, so that one
    /// may have gone unheard before it.
    Written,
    /// Nothing watches the account index and the readings, until the watch says otherwise.
    Unwatched,
    Installed {
        tools: Vec<ProviderId>,
    },
    /// The app holding the login, where one is running, as the question to ask about it.
    Held {
        account: Key,
        question: Option<QuitQuestion>,
        then: After,
    },
    Quit {
        question: QuitQuestion,
        outcome: QuitOutcome,
        then: After,
    },
    Switched {
        account: Key,
        reopen: Option<String>,
        done: Result<Switched, PitboardError>,
    },
    /// What the core's look at whether to switch Claude Code by itself came to.
    AutoLooked {
        looked: Result<AutoLooked, PitboardError>,
    },
    /// What switching Claude Code by itself came to.
    AutoSwitched {
        done: Result<AutoSwitched, PitboardError>,
    },
    /// What a warm-up came to: one by itself, or of the account `asked`, somebody asked for
    /// now.
    WarmedUp {
        asked: Option<Key>,
        done: Result<WarmUp, WarmRefused>,
    },
    /// A change to which accounts are warmed up, or when, with each one's warm-up after it.
    WarmSet {
        done: Result<Overview, PitboardError>,
    },
    // `opened` where the lane called `reopen` and it answered
    Opened {
        tool: ProviderId,
        front: bool,
        opened: bool,
    },
    Abandoned(Result<GaveUp, PitboardError>),
    /// The tool of the sign-in `id` has started, or could not be.
    SignInStarted {
        id: u64,
        started: Result<(), PitboardError>,
        // the account the core parked for it, as `SwitchTo` takes it
        put_back: Option<Key>,
    },
    /// Something more the tool of the sign-in `id` said, as it came.
    SignInSaid {
        id: u64,
        text: String,
        step: Option<WaitStep>,
    },
    /// The tool of the sign-in `id` has stopped saying anything: it has ended, or been
    /// stopped. Its thread waits for `Job::SignInOver`.
    SignInQuiet {
        id: u64,
    },
    /// What enrolling what the sign-in `id` signed in to came to.
    SignInFinished {
        id: u64,
        done: Result<Enrolled, PitboardError>,
    },
    /// The sign-in `id` was stopped rather than enrolled: its tool has been stopped and
    /// waited for, and the core's one sign-in at a time let go of. Told by the stop on the
    /// lane of sign-in calls where it stopped the tool, and by the sign-in's own thread once
    /// the tool has stopped saying anything and the thread was told not to enrol, so twice of
    /// one stopped from the lane.
    SignInStopped {
        id: u64,
    },
    /// What enrolling the login signed in now came to.
    Enrolled {
        tool: ProviderId,
        again: Option<String>,
        from: Option<Sheet>,
        reopen: Option<String>,
        done: Result<Enrolled, PitboardError>,
    },
    /// What renaming `account` to `to` came to.
    Renamed {
        account: Key,
        to: String,
        from: Option<Sheet>,
        done: Result<(), PitboardError>,
    },
    /// What forgetting `account` came to.
    Forgot {
        account: Key,
        done: Result<(), PitboardError>,
    },
    /// What writing the account in use into its tool's config came to, with what its tool
    /// warned of.
    ConfigUpdated(Result<Vec<Warning>, PitboardError>),
    /// What is left in the file behind Claude Code's store, and what putting it away would
    /// do with it: `None` where nothing is.
    LookedLeft(Result<Option<Left>, PitboardError>),
    /// What putting away the file behind Claude Code's store came to, with what it warned of.
    Stowed {
        from: Option<Sheet>,
        done: Result<(Stowed, Vec<Warning>), PitboardError>,
    },
    /// What the model keeps of its own, as it was read: nothing told where nothing was kept
    /// or it could not be read, and the app's preferences, with whether they came from their
    /// file, as `Preferences::kept` reads them: from the file where it was there, and
    /// otherwise as the app's earlier store held them. `None` where the file is there and
    /// could not be read, which is not the same as no file. The account windows' records
    /// likewise, this directory's, `None` where their file is there and could not be read as
    /// records or the app keeps them nowhere.
    Kept {
        told: Told,
        preferences: Option<(Preferences, bool)>,
        windows: Option<Loaded>,
    },
    /// The account windows' records were written, or could not be, as the `write`th write.
    WindowsKept {
        write: u64,
    },
    /// Which of `stores` another Pitboard directory that is still there recorded too, or
    /// `None` where the records could not be read to say.
    SharedChecked {
        stores: Vec<String>,
        shared: Option<Vec<String>>,
    },
    /// What was kept was written, or could not be, which nothing here can mend: told again,
    /// a run-out is told once more after a relaunch, and nothing worse.
    Saved,
    // the notification titled `title` was posted, or the app's system refused it, and why
    Posted {
        title: String,
        refused: Option<String>,
    },
    /// A schedule an older app wrote was repaired, or nothing needed it, or it could not be,
    /// with what the command line inside this copy is.
    Repaired {
        repaired: Result<bool, PitboardError>,
        own: OwnCommandLine,
    },
    /// The renewal schedule as the core reads it, or `None` where nothing may be read, with
    /// what the command line inside this copy is.
    ScheduleRead {
        schedule: Option<Schedule>,
        own: OwnCommandLine,
        after_change: bool,
    },
    /// What a change to the schedule came to, with what the command line inside this copy is.
    ScheduleSet {
        own: OwnCommandLine,
        outcome: Scheduled,
    },
    /// The watch service as the core reads it, or `None` where nothing may be read, with
    /// what the command line inside this copy is.
    WatchServiceRead {
        service: Option<WatchService>,
        own: OwnCommandLine,
        after_change: bool,
    },
    /// What a change to the watch service came to, with what the command line inside this copy
    /// is.
    WatchServiceSet {
        own: OwnCommandLine,
        outcome: Scheduled,
    },
    /// Doctor's checks, or `None` where this build makes none.
    Checked {
        doctored: Option<Doctored>,
    },
    /// The newest changes Pitboard made, oldest first, as the core keeps them.
    Logged {
        changes: Vec<Change>,
    },
    /// The `pitboard` a terminal would run, with what the command line inside this copy is.
    CommandLineFound {
        found: FoundCommandLine,
        own: OwnCommandLine,
    },
    /// A code was typed back, or could not be, which the tool says itself if it matters.
    Pasted,
    /// A stop that found no tool to stop: one that had not started, or had stopped saying
    /// anything, or one a stop before had stopped.
    Stopped,
    Uninstalled(Result<Uninstalled, PitboardError>),
    /// The glance was written, or could not be, and the widgets reload where `reload`.
    GlanceKept {
        reload: bool,
    },
    /// The job stopped with a panic, or a sign-in's thread could not be started, and it came
    /// to nothing. The lane goes on, and the model goes on as though the job had answered
    /// with nothing new.
    Lost(Job),
}

impl Answer {
    // The answer of a job `Job::writes` counts.
    fn of_a_write(&self) -> bool {
        match self {
            Answer::Read { .. }
            | Answer::ReadOffline { .. }
            | Answer::Looked { .. }
            | Answer::AutoLooked { .. }
            | Answer::ScheduleRead { .. }
            | Answer::WatchServiceRead { .. }
            | Answer::Checked { .. }
            | Answer::Logged { .. }
            | Answer::LookedLeft(_)
            | Answer::Switched { .. }
            | Answer::AutoSwitched { .. }
            | Answer::Abandoned(_)
            | Answer::Enrolled { .. }
            | Answer::Renamed { .. }
            | Answer::Forgot { .. }
            | Answer::ConfigUpdated(_)
            | Answer::Stowed { .. }
            | Answer::Repaired { .. }
            | Answer::ScheduleSet { .. }
            | Answer::WatchServiceSet { .. }
            | Answer::WarmSet { .. }
            | Answer::WarmedUp { .. }
            | Answer::Kept { .. }
            | Answer::Saved
            | Answer::WindowsKept { .. }
            | Answer::SharedChecked { .. }
            | Answer::GlanceKept { .. }
            | Answer::Uninstalled(_) => true,
            Answer::Lost(job) => job.writes(),
            Answer::Installed { .. }
            | Answer::Written
            | Answer::Unwatched
            | Answer::Held { .. }
            | Answer::Quit { .. }
            | Answer::Opened { .. }
            | Answer::SignInStarted { .. }
            | Answer::SignInSaid { .. }
            | Answer::SignInQuiet { .. }
            | Answer::SignInFinished { .. }
            | Answer::SignInStopped { .. }
            | Answer::Posted { .. }
            | Answer::CommandLineFound { .. }
            | Answer::Pasted
            | Answer::Stopped => false,
        }
    }
}

// Removing Pitboard from this machine, once somebody confirmed Uninstall Pitboard.
#[derive(Debug)]
pub(crate) enum Removing {
    // `app`: one holding a saved sign-in is quitting first; (tool, copy) of it opens after.
    Ending {
        app: bool,
        reopen: Option<(ProviderId, String)>,
    },
    Running {
        reopen: Option<(ProviderId, String)>,
    },
    // the account windows' stores go before it is said
    Clearing(Uninstalled),
    Done(Uninstalled),
    // its job came to nothing, maybe after the core removed the accounts and let go of the folder
    Lost,
}

// How a request from outside ended, which `present` words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Answered {
    Switched { tool: ProviderId, label: String },
    InUse { label: String },
    Updated { at: i64 },
    Unknown,
    NeedsSignIn { label: String },
    Unusable { label: String },
    Busy,
    // the app holding its login is open, which the asker may let Pitboard quit
    AppOpen { question: QuitQuestion },
    // its login in use is refused for good, which only the app asks to replace
    Replace { label: String },
    // a question the person has open at the Mac would go
    Asking,
    Uninstalling,
    // while `doing`; why stays in the app, since the core's words can name an email or a path
    Failed { doing: Doing },
}

// The glance's last text, what a widget shows of it, how many times that changed, and whether
// the app named a file to write it to.
#[derive(Debug, Default)]
struct Glancing {
    text: Option<String>,
    shown: Option<String>,
    revision: u64,
    file: bool,
}

const ANSWERS_KEPT: usize = 10;

/// A timer of the model's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Timer {
    /// Not started.
    Off,
    /// Due at this moment, since the model started.
    Due(Duration),
    /// What it starts is under way, and the timer is set again once that is over.
    Running,
}

/// What the sheet for putting away the login left in a file has found of it.
#[derive(Debug)]
pub(crate) enum Leftover {
    /// It is looking.
    Looking,
    /// What it found: `None` where nothing is left behind Claude Code's store.
    Found(Option<Left>),
    /// It could not look, which its failure says.
    Unknown,
}

/// Warming up accounts, as the model knows and does it.
#[derive(Debug, Default)]
pub(crate) struct Warming {
    /// Each Claude Code account, whether it is warmed up and where it stands, and the span of
    /// the day, as the last read or warm-up found them.
    pub(crate) overview: Option<Overview>,
    /// A warm-up under way: of the account somebody asked for now, or one by itself, `None`.
    pub(crate) running: Option<Option<Key>>,
    /// Warm-ups by itself refused in a row, and the wait after them.
    refusals: Refusals,
    told: pitboard_core::switch::warm::Told,
    /// What the last warm-up by itself that came to nothing said, until the core next says
    /// where each account stands.
    pub(crate) failed: Option<String>,
    /// Why the core could not say which accounts are warmed up, until it can.
    pub(crate) unreadable: Option<String>,
}

impl Warming {
    /// Whether any account is turned on for warm-ups.
    pub(crate) fn any_on(&self) -> bool {
        self.overview
            .as_ref()
            .is_some_and(|overview| overview.accounts.iter().any(|choice| choice.on))
    }
}

/// What switching Claude Code by itself last came to, which the settings say under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AutoStanding {
    /// What the core's look, or its decision under its lock, came to.
    Came(AutoSwitched),
    /// What stopped the core, in its words. After a refused switch, `until` is when the model
    /// asks for the next, in epoch seconds; a look the core could not make has none.
    Stopped { message: String, until: Option<i64> },
}

/// A read somebody or something asked for. By default, a read whatever the age of the
/// numbers, which nobody asked for.
#[derive(Debug, Clone, Copy, Default)]
struct Asked {
    /// Whether somebody asked, which has the core ask every service again.
    fresh: bool,
    /// Nothing is read when the numbers shown are younger than this.
    older_than: Duration,
    /// Whether the timer that reads every few minutes asked.
    timed: bool,
    /// Whether a switch asked, which is over once this read lands.
    after_switch: bool,
    /// What the change that asked warned of, by its number in `said_after_read`, which is
    /// said once this read lands.
    saying: Option<u64>,
    /// Whether a change of this app's own to the account index asked, other than a switch,
    /// which holds the index until this read is over.
    after_change: bool,
    request: Option<u64>,
}

/// What a person asked for that did not happen, before it is numbered.
struct Refused {
    doing: Doing,
    cause: Cause,
    code: Option<String>,
    warnings: Vec<Warning>,
    account: Option<Key>,
}

impl Refused {
    /// `error`, which stopped `doing`.
    fn of(doing: Doing, error: PitboardError) -> Refused {
        let PitboardError::Failed {
            code,
            message,
            warnings,
            account,
        } = error;
        Refused {
            doing,
            cause: Cause::Refused { message },
            code: Some(code),
            warnings,
            account,
        }
    }

    /// A job doing `doing` that came to nothing: what it did is not known.
    fn lost(doing: Doing) -> Refused {
        Refused {
            doing,
            cause: Cause::NotKnownHowItWent,
            code: None,
            warnings: Vec::new(),
            account: None,
        }
    }
}

// What the windows and Try Again send while Pitboard is being removed.
fn taken_while_removing(intent: &Intent) -> bool {
    matches!(
        intent,
        Intent::Uninstall
            | Intent::SurfacesChanged { .. }
            | Intent::Show { .. }
            | Intent::DismissFailure
            | Intent::WindowOpened { .. }
            | Intent::WindowClosed { .. }
            | Intent::StoreDeleted { .. }
            | Intent::StoreHeld { .. }
            | Intent::DownloadEnded { .. }
    )
}

pub(crate) fn in_its_app(tool: ProviderId) -> bool {
    tool.signs_in() == SignsIn::InItsApp
}

/// `typed` without the white space around it, as the Swift sheets took it away with
/// Foundation's `whitespacesAndNewlines`, by the rule a link from outside is trimmed by.
pub(crate) fn trimmed(typed: &str) -> &str {
    pitboard_sites::trimmed(typed)
}

// What a quit question names an account of `tool` by before it has a name.
fn new_account(tool: ProviderId) -> Key {
    Key::new(tool, "")
}

// A switch under way: to the account somebody chose, or of Claude Code by itself, to the one
// the core chooses.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Switching {
    To(Key),
    Automatically,
}

impl Switching {
    fn of(&self) -> ProviderId {
        match self {
            Switching::To(account) => account.provider,
            Switching::Automatically => ProviderId::Claude,
        }
    }
}

// Where what switching or warming up by itself came to is said.
const LIMITS: Place = Place::Settings {
    section: SettingsSection::Limits,
};

// Until `until`, when the limit it reached resets, nothing says `from` was kept in use.
#[derive(Debug)]
struct SwitchedBack {
    from: Key,
    until: Option<i64>,
}

/// A rename, as its answer is taken in.
#[derive(Clone, Copy)]
struct Renaming<'a> {
    account: &'a Key,
    to: &'a str,
    /// The sheet it was asked from, where what goes wrong is said.
    from: Option<&'a Sheet>,
}

// A sign-in inside an app on its way to parking the account in use.
#[derive(Debug)]
struct Parking {
    id: u64,
    tool: ProviderId,
    // where the app quit for it was opened from
    reopen: Option<String>,
}

// An enrolment of the login an app or a tool holds now, as its answer is taken in.
#[derive(Clone, Copy)]
struct Enrolling<'a> {
    tool: ProviderId,
    again: Option<&'a str>,
    // the sheet it was asked from, where what goes wrong is said
    from: Option<&'a Sheet>,
}

/// The quit question last asked. A question closed unanswered is kept, unasked, because an
/// app can close it before it sends the answer that closed it: WindowAlerts.swift's alert
/// closes it from its binding and answers from its button on a task of its own, which
/// AppModel.swift's quitAndSwitch says runs once the alert has closed it.
#[derive(Debug)]
struct Quitting {
    question: QuitQuestion,
    /// Whether it is still asked, and so shown.
    asked: bool,
    then: After,
}

/// The question a switch asked before replacing a login in use its service refuses for good.
/// A question closed unanswered is kept, unasked, as [`Quitting`] is, for the answer an app
/// sends after closing it.
#[derive(Debug)]
struct Replacing {
    account: Key,
    /// Why, in the app's words, as the core's refusal said it.
    message: String,
    asked: bool,
}

/// A sign-in under way, as the Swift model's `SigningIn` held it.
///
/// The Swift kept the sheet it was started from, to close only that one once it finished and
/// say a failure only in that one. Here a sign-in under way keeps its sheet, so the sheet up
/// while it runs is always that one, or none once that one has been closed.
#[derive(Debug)]
pub(crate) struct SigningIn {
    id: u64,
    pub(crate) tool: ProviderId,
    // the account signed in to again, from its own sheet; none for a new one
    pub(crate) again: Option<String>,
    /// Everything the tool has said so far.
    said: String,
    /// How much of `said` the tool had said when a code was last typed back, if one was.
    /// What it says after that is what says whether it took the code.
    pasted_at: Option<usize>,
    /// Whether the tool has started, so that a cancel has a tool to stop. One cancelled
    /// before is stopped as it starts.
    pub(crate) started: bool,
    /// Whether its thread has been asked for. Not while a sign-in cancelled before it may
    /// still hold the core's one sign-in at a time.
    asked: bool,
    pub(crate) in_its_app: bool,
    // Pitboard is quitting the app before the sign-in starts
    pub(crate) quitting: bool,
    pub(crate) opened_for_it: bool,
    // `again` was the account in use as it was asked for
    in_use_before: bool,
    // the account the core parked for it, as SwitchTo takes it; None until the core says
    pub(crate) put_back: Option<Key>,
    pub(crate) step: Option<WaitStep>,
}

impl SigningIn {
    /// What it is shown as, read by its tool's own module. A code the tool refused since it
    /// was typed back is no longer one typed back: the tool reads another, so the field is
    /// offered again.
    fn shown(&self) -> RunningSignIn {
        let pasted = self.pasted_at.is_some() && !self.code_refused();
        let view = provider::sign_in_view(self.tool, &self.said, pasted);
        RunningSignIn {
            id: self.id,
            url: view.url,
            wants_code: view.wants_code,
        }
    }

    // Whether the tool refused the last code typed back, and reads another in its place.
    pub(crate) fn code_refused(&self) -> bool {
        self.pasted_at.is_some_and(|at| {
            provider::refused_code(self.tool, self.said.get(at..).unwrap_or_default())
        })
    }
}

/// Everything the model knows. The actor thread owns it, and nothing else touches it.
#[derive(Debug)]
pub(crate) struct State {
    /// Which app is shown it, which says what differs by front.
    pub(crate) front: Front,
    /// What that front's system gives it now, as the front last said.
    pub(crate) surfaces: Surfaces,
    cadence: Cadence,
    /// Whether what runs by itself has been started. A test drives everything itself until
    /// it says so, as the Swift model's tests did.
    started: bool,
    /// What the last read gave, or the last numbers known, or nothing before anything has
    /// been read.
    pub(crate) status: Option<Status>,
    /// Everything that went wrong on the way, not only the first of them.
    pub(crate) warnings: Vec<Warning>,
    /// What went wrong with the last read, when it did not answer. The numbers shown are
    /// then the last ones measured.
    pub(crate) failure: Option<ReadFailure>,
    /// An interrupted switch nothing can finish, which the app offers a way out of.
    pub(crate) stuck: bool,
    /// When the accounts were last read, in epoch milliseconds by the wall clock.
    pub(crate) updated_ms: Option<i64>,
    /// Reads under way. Reads overlap, a timer's with one somebody asked for, so they are
    /// counted rather than flagged: the first to end would otherwise say none is running
    /// while the other still is.
    pub(crate) reads: u32,
    /// Counts the changes this app has made and the ones the poll noticed made elsewhere. A
    /// read that started before one lands after it with who was signed in before, and would
    /// put away what the change said, so it is dropped: the read the change starts itself
    /// says what is true now.
    changes_seen: u64,
    /// The tools whose program was found, once an answer has come. Asked by the first read,
    /// and by each read while none has been found: the core asks a login shell that was too
    /// slow to answer once more, and finding none is what says to install a tool.
    pub(crate) installed: Option<Vec<ProviderId>>,
    /// Whether that is being asked now. A read asked for meanwhile waits for the same answer
    /// rather than asking again.
    asking_installed: bool,
    // A tool the core found no program for since the ask under way began, which that ask's
    // answer may still name.
    gone_since_asked: Option<ProviderId>,
    /// Reads waiting for that answer, which go ahead once it comes.
    waiting: Vec<Asked>,
    /// When the account index was last written, as last seen. `None` until something has
    /// looked: a machine with no index reports 0, which is a real answer.
    changed_at: Option<i64>,
    /// The same for the usage readings, which every session's status line records into.
    readings_at: Option<i64>,
    /// The look, due where nothing watches Pitboard's directory, every `look_every`, or where
    /// a write heard is yet to be looked at.
    look: Timer,
    /// Whether the watch said nothing watches Pitboard's directory.
    unwatched: bool,
    /// Whether a write was heard, or a look was put off by a change of this app's own, that no
    /// look has read since.
    unlooked: bool,
    timed_read: Timer,
    /// The core's look at whether to switch Claude Code by itself, `decide_every` after the
    /// last one ended, while switching by itself is on.
    decide: Timer,
    /// Asking the core to warm up whichever account is due, `decide_every` after the last ask
    /// ended, while an account is turned on for it.
    warm_timer: Timer,
    /// Warming up accounts: which are turned on and where each stands, what is under way and
    /// what was said.
    pub(crate) warm: Warming,
    /// The switch running: from the moment it is asked for, through asking what holds its
    /// tool's login, quitting an app and the switch itself, until the read after it lands.
    switching: Option<Switching>,
    changing: u32,
    // sign-ins inside an app whose park holds the index, until their start is answered
    parking: Vec<Parking>,
    // (tool, copy) of an app quit for another change of the tool, opened after its switch
    reopen_after_switch: Option<(ProviderId, String)>,
    // (park, account) of a switch that goes on once that park of its tool has started
    switch_after_park: Option<(u64, Key)>,
    // (sheet, tool, step) of the sheet recording the account the tool's app shows
    recording: Option<(Option<Sheet>, ProviderId, Recording)>,
    // by tool, until a change of the tool lands: what the next add of its account opens on
    refused_by: BTreeMap<ProviderId, String>,
    // the moves of an app's login given up on, in the app's words, until somebody has read them
    moves_given_up: Vec<String>,
    quitting: Option<Quitting>,
    replacing: Option<Replacing>,
    /// What each tool's last switch said that is still true, one per tool at most.
    pub(crate) last_switches: Vec<LastSwitch>,
    /// What giving up on an interrupted switch kept, until somebody has read it.
    pub(crate) abandoned: Option<Abandoned>,
    /// What the sheet for putting away the login left in a file has found of it, while that
    /// sheet is up.
    pub(crate) left: Option<Leftover>,
    /// What putting away the login left in a file did, until somebody has read it.
    pub(crate) stowed: Option<Stowed>,
    /// The last thing asked for that did not happen.
    pub(crate) presented: Option<Failure>,
    /// How many failures have been said, which numbers the next.
    failures: u64,
    /// The requests for the main window.
    pub(crate) window: WindowRequest,
    signing_in: Option<SigningIn>,
    /// The sign-in that may still hold the core's one sign-in at a time, from when its thread
    /// is asked for until it has let go of it: until then its tool may still run, so the next
    /// sign-in waits. Cancelled, it is no longer the one under way, and still this until it
    /// has been stopped.
    sign_in_holding: Option<u64>,
    /// How many sign-ins have been asked for, which numbers the next.
    sign_ins: u64,
    /// What a finished sign-in or a config update warned about, said beside the read after it
    /// once that read has landed, which would otherwise put it away: by a number of its own.
    said_after_read: Vec<(u64, Vec<Warning>)>,
    /// How many have been kept there, which numbers the next.
    sayings: u64,
    /// The sheet over the main window.
    pub(crate) sheet: Option<Sheet>,
    /// What went wrong in the sheet that is up.
    pub(crate) sheet_failure: Option<Failure>,
    /// The sheets a name typed in is being saved from: an enrolment or a rename asked for from
    /// the sheet, until it has answered. A save from a sheet cannot be withdrawn, so the sheet
    /// holds back while it runs, as NameSheets.swift held back while `saving`.
    pub(crate) saving: Vec<Sheet>,
    /// The minute tick, which makes again what is shown for the time alone.
    tick: Timer,
    /// Accounts that have run out while another of the same tool has room, one per tool at
    /// most, the newer first. Said in the window whether or not notifications are allowed,
    /// so the advice does not depend on a permission.
    pub(crate) advice: Vec<Advice>,
    /// What has been told about since the model started, which decides what advice is new,
    /// as Notifier.swift's `told` decided it: the window offers what was told, after a
    /// relaunch too, for as long as the numbers bear it out.
    pub(crate) told_this_launch: Told,
    /// What has been told about in a notification, once per reset of a limit, in this
    /// launch or an earlier one: kept in Pitboard's directory, so a run-out notified before
    /// a relaunch is not notified again after it.
    pub(crate) told: Told,
    /// What switching Claude Code by itself has told about in a notification since the
    /// model started, or since the last switch it made, by what it was: each reason it did
    /// not switch away from a limit, for that limit and its reset, each reason it cannot judge
    /// and each refusal, said once rather than at every look.
    auto_told: BTreeSet<String>,
    switched_back: Option<SwitchedBack>,
    /// The accounts whose login was replaced outside Pitboard that the last read said and a
    /// notification has told about, by `Account.id`.
    replaced_told: BTreeSet<String>,
    /// What the last read that asked nobody said of Claude Code's config naming another
    /// account since Anthropic last named the login stored, which one read that asks has
    /// been started for.
    unconfirmed_asked: Option<String>,
    /// What switching Claude Code by itself last came to, said under its setting while that
    /// is on.
    pub(crate) auto_standing: Option<AutoStanding>,
    /// Switches by itself refused in a row, on the model's own clock, and the wait after
    /// them: as long as the core waits after attempts that failed in a row, since a refusal
    /// raised before the core could record it is a line in the activity log each time. Any
    /// outcome, of a look or of a decision, ends the row.
    auto_refusals: Refusals,
    /// The app's own preferences, kept in Pitboard's directory.
    pub(crate) preferences: Preferences,
    // as the app's system last said, which nothing guesses at before
    pub(crate) opens_at_login: Option<bool>,
    /// Whether the preferences have been read. Until they are, and for good where they
    /// could not be, nothing is written over what may be in their file.
    preferences_read: bool,
    /// Whether what the model keeps is being read, from the moment it is started until it
    /// is in. Nothing is advised on meanwhile, and what is shown is advised on once it is in,
    /// so a run-out notified before a relaunch is never notified again because the read
    /// after it landed first.
    loading_kept: bool,
    // the window asked for meanwhile, which the first launch's welcome stands in for
    window_waits: bool,
    /// Whether what is shown is what stands in for a read that failed before anything was
    /// shown, the last numbers measured, which nothing has advised on: as AppModel.swift's
    /// fallback, it is not advised on once what is kept is in either.
    standing_in: bool,
    /// What the model knows of this machine rather than its accounts.
    pub(crate) machine: MachineState,
    /// The account windows' bookkeeping.
    pub(crate) windows: Windows,
    pub(crate) removing: Option<Removing>,
    // jobs `Job::writes` counts, until each has answered
    at_work: u32,
    glancing: Glancing,
    pub(crate) answered: VecDeque<(u64, Answered)>,
    outside_switch: Option<OutsideSwitch>,
    // switches asked from outside before the accounts were first read
    outside_held: Vec<(u64, OutsideAsk)>,
}

// The request from outside the switch under way answers, and whether it let Pitboard quit an app.
#[derive(Debug, Clone, Copy)]
struct OutsideSwitch {
    request: u64,
    quit: bool,
}

impl State {
    pub(crate) fn new(front: Front, cadence: Cadence) -> State {
        State {
            front,
            surfaces: front.surfaces(),
            cadence,
            started: false,
            status: None,
            warnings: Vec::new(),
            failure: None,
            stuck: false,
            updated_ms: None,
            reads: 0,
            changes_seen: 0,
            installed: None,
            asking_installed: false,
            gone_since_asked: None,
            waiting: Vec::new(),
            changed_at: None,
            readings_at: None,
            look: Timer::Off,
            unwatched: false,
            unlooked: false,
            timed_read: Timer::Off,
            decide: Timer::Off,
            switching: None,
            changing: 0,
            parking: Vec::new(),
            reopen_after_switch: None,
            switch_after_park: None,
            recording: None,
            refused_by: BTreeMap::new(),
            moves_given_up: Vec::new(),
            quitting: None,
            replacing: None,
            last_switches: Vec::new(),
            abandoned: None,
            left: None,
            stowed: None,
            presented: None,
            failures: 0,
            window: WindowRequest {
                serial: 0,
                place: None,
            },
            signing_in: None,
            sign_in_holding: None,
            sign_ins: 0,
            said_after_read: Vec::new(),
            sayings: 0,
            sheet: None,
            sheet_failure: None,
            saving: Vec::new(),
            tick: Timer::Off,
            advice: Vec::new(),
            told_this_launch: Told::new(),
            auto_told: BTreeSet::new(),
            switched_back: None,
            replaced_told: BTreeSet::new(),
            unconfirmed_asked: None,
            auto_standing: None,
            auto_refusals: Refusals::default(),
            warm_timer: Timer::Off,
            warm: Warming::default(),
            told: Told::new(),
            preferences: Preferences::default(),
            opens_at_login: None,
            preferences_read: false,
            loading_kept: false,
            window_waits: false,
            standing_in: false,
            machine: MachineState::default(),
            windows: Windows::new(
                cadence.arming,
                front.offers_account_windows(),
                front.takes_pasted_links(),
            ),
            removing: None,
            at_work: 0,
            glancing: Glancing::default(),
            answered: VecDeque::new(),
            outside_switch: None,
            outside_held: Vec::new(),
        }
    }

    /// Takes in `msg`, which arrived at `now`, and says what to run. Any timer due by `now`
    /// goes off too, whatever the message, so a stream of messages cannot hold one back.
    pub(crate) fn apply(&mut self, msg: Msg, now: Now) -> Vec<Job> {
        let mut jobs = Vec::new();
        match msg {
            Msg::Intent(intent) => {
                if self.removing.is_none() || taken_while_removing(&intent) {
                    self.intent(intent, now, &mut jobs);
                } else if let Intent::Outside { request, .. } = intent {
                    self.answer_outside(request, Answered::Uninstalling);
                }
            }
            Msg::Done(answer) => {
                if answer.of_a_write() {
                    self.at_work = self.at_work.saturating_sub(1);
                }
                self.answer(answer, now, &mut jobs);
            }
            Msg::Tick | Msg::Stop => {}
        }
        self.go_off(now, &mut jobs);
        self.follow_windows(now, &mut jobs);
        self.take_held(now, &mut jobs);
        if self.removing.is_some() {
            jobs.retain(|job| {
                !matches!(
                    job,
                    Job::KeepTold { .. } | Job::KeepPreferences { .. } | Job::Post { .. }
                )
            });
        }
        let writes = jobs.iter().filter(|job| job.writes()).count();
        self.at_work += u32::try_from(writes).unwrap_or(u32::MAX);
        self.uninstall_once_idle(&mut jobs);
        self.cleared();
        jobs
    }

    /// The account windows follow what is known now: each window that can open opens, and
    /// the wait before a link can be opened starts again where what the picker offers moved.
    fn follow_windows(&mut self, now: Now, jobs: &mut Vec<Job>) {
        self.windows.open_what_can(self.status.as_ref(), jobs);
        let problem = self
            .failure
            .as_ref()
            .map(|failure| failure.message.as_str());
        self.windows
            .rearm(self.status.as_ref(), problem, now.running);
    }

    /// When the next timer is due, since the model started, if any is set.
    pub(crate) fn next_due(&self) -> Option<Duration> {
        [
            self.look,
            self.timed_read,
            self.tick,
            self.decide,
            self.warm_timer,
        ]
        .into_iter()
        .filter_map(|timer| match timer {
            Timer::Due(at) => Some(at),
            Timer::Off | Timer::Running => None,
        })
        .chain(self.windows.next_due())
        .min()
    }

    // Where no status item keeps Pitboard running once its window is closed, the watch service
    // runs what would stop with it.
    pub(crate) fn offers_watch_service(&self) -> bool {
        !self.surfaces.status_item
    }

    /// Whether a sign-in is under way.
    pub(crate) fn sign_in_under_way(&self) -> bool {
        self.signing_in.is_some()
    }

    /// The sign-in under way as an app is shown it, read by its tool's own module.
    pub(crate) fn shown_sign_in(&self) -> Option<RunningSignIn> {
        self.signing_in.as_ref().map(SigningIn::shown)
    }

    /// Whether a switch is running, or waiting on the quit question or the question before a
    /// replacement.
    pub(crate) fn switch_under_way(&self) -> bool {
        self.switching.is_some() || self.switching_to().is_some()
    }

    /// The account such a switch is for, where somebody chose it.
    pub(crate) fn switching_to(&self) -> Option<&Key> {
        match &self.switching {
            Some(Switching::To(account)) => Some(account),
            Some(Switching::Automatically) => None,
            None => self
                .asking_for()
                .filter(|(_, then)| **then == After::Switch)
                .map(|(question, _)| &question.account)
                .or(self.replace_asked().map(|(account, _)| account)),
        }
    }

    /// The account a switch asks to put in place of a login its service refuses for good,
    /// and why, while it asks.
    pub(crate) fn replace_asked(&self) -> Option<(&Key, &str)> {
        self.replacing
            .as_ref()
            .filter(|replacing| replacing.asked)
            .map(|replacing| (&replacing.account, replacing.message.as_str()))
    }

    /// Whether the app's preferences have been read, so that a change to them is kept.
    pub(crate) fn preferences_have_been_read(&self) -> bool {
        self.preferences_read
    }

    /// Whether the app's preferences are still being read, which nothing that depends on
    /// them should guess at meanwhile.
    pub(crate) fn reading_preferences(&self) -> bool {
        self.loading_kept
    }

    /// The quit question, while it is asked.
    pub(crate) fn asking(&self) -> Option<&QuitQuestion> {
        self.asking_for().map(|(question, _)| question)
    }

    pub(crate) fn asking_for(&self) -> Option<(&QuitQuestion, &After)> {
        self.quitting
            .as_ref()
            .filter(|quitting| quitting.asked)
            .map(|quitting| (&quitting.question, &quitting.then))
    }

    pub(crate) fn signing(&self) -> Option<&SigningIn> {
        self.signing_in.as_ref()
    }

    // The tool whose app's account the sheet up records, and what it waits for.
    pub(crate) fn recording(&self) -> Option<(ProviderId, Recording)> {
        self.recording
            .as_ref()
            .filter(|(from, ..)| from.is_some() && *from == self.sheet)
            .map(|(_, tool, step)| (*tool, *step))
    }

    // What an add of `tool`'s account does, by the first rule that holds.
    pub(crate) fn in_app_add(&self, tool: ProviderId) -> InAppAdd {
        let refused = self.refused_by(tool);
        if refused == Some("live_credential_absent") {
            return InAppAdd::AfterSignIn;
        }
        let Some(status) = &self.status else {
            return InAppAdd::Current;
        };
        let warned = |code: &str| {
            (self.warnings.iter())
                .any(|warning| warning.code == code && warning.provider == Some(tool))
        };
        if !status
            .accounts
            .iter()
            .any(|account| account.provider == tool)
            || status
                .awaiting
                .iter()
                .any(|awaiting| awaiting.provider == tool)
            || matches!(
                refused,
                Some(
                    "login_in_doubt"
                        | "live_account_not_enrolled"
                        | "recovery_undetermined"
                        | "move_mixed"
                )
            )
            || warned("in_use_unconfirmed")
            || warned("move_waiting")
        {
            InAppAdd::Current
        } else {
            InAppAdd::AfterSignIn
        }
    }

    pub(crate) fn refused_by(&self, tool: ProviderId) -> Option<&str> {
        self.refused_by.get(&tool).map(String::as_str)
    }

    pub(crate) fn moves_given_up(&self) -> &[String] {
        &self.moves_given_up
    }

    fn intent(&mut self, intent: Intent, now: Now, jobs: &mut Vec<Job>) {
        match intent {
            Intent::Start => self.start(now, jobs),
            // Numbers read before the machine slept say nothing about now.
            Intent::Woke => self.refresh(Asked::default(), now, jobs),
            // A menu opening is somebody looking at what it says, which is the glance the
            // whole app exists for.
            Intent::Glanced => self.refresh(
                Asked {
                    older_than: self.cadence.stale_after,
                    ..Asked::default()
                },
                now,
                jobs,
            ),
            Intent::SurfacesChanged { surfaces } => self.surfaces = surfaces,
            Intent::Refresh { asked } => self.refresh(
                Asked {
                    fresh: asked,
                    ..Asked::default()
                },
                now,
                jobs,
            ),
            Intent::SwitchTo { account } => self.switch_asked(account.into(), jobs),
            Intent::QuitAndSwitch { account } => self.quit_and_switch(&account.into(), jobs),
            Intent::SwitchBack { from, to } => self.switch_back(from.into(), &to.into(), jobs),
            Intent::ReplaceAndSwitch { account } => {
                self.replace_and_switch(account.into(), jobs);
            }
            Intent::KeepLogin => {
                if let Some(replacing) = self.replacing.as_mut() {
                    replacing.asked = false;
                }
            }
            // The question is gone, and nothing was switched. It is kept, unasked, for the
            // answer an app sends after closing it.
            Intent::KeepAppOpen => {
                if let Some(quitting) = self.quitting.as_mut().filter(|quitting| quitting.asked) {
                    quitting.asked = false;
                    let from = match &quitting.then {
                        After::Switch | After::Uninstall => None,
                        After::SignIn { from, .. } | After::Enrol { from, .. } => from.clone(),
                    };
                    self.saved(from.as_ref());
                }
            }
            Intent::DismissSwitch { tool } => {
                let tool = ProviderId::from(tool);
                self.last_switches.retain(|last| last.provider != tool);
            }
            Intent::DeclineSecondAccount { tool } => {
                // Kept once the preferences are read, with whatever they hold, and never over
                // a file that could not be read: said meanwhile, it holds until the app quits.
                if self.preferences.decline_second_account(tool.into()) && self.preferences_read {
                    jobs.push(Job::KeepPreferences {
                        preferences: self.preferences.clone(),
                    });
                }
            }
            Intent::SetAtLimit { choice, at } => self.at_limit_set(choice, at, jobs),
            Intent::DeclineTip { tip } => self.put_away(tip, jobs),
            Intent::OpensAtLogin { on } => self.opens_at_login = Some(on),
            // Keeping the choice shown there writes nothing, and the tip has done its work.
            Intent::ShowLimits => {
                self.show(Some(LIMITS));
                self.put_away(Tip::AtLimit, jobs);
            }
            Intent::SetWarm { account, on } => jobs.push(Job::SetWarm {
                account: account.into(),
                on,
            }),
            Intent::SetWarmRange { from, to } => jobs.push(Job::SetWarmRange { from, to }),
            Intent::WarmNow { account } => {
                if self.warm.running.is_none() {
                    let account = Key::from(account);
                    self.warm.running = Some(Some(account.clone()));
                    jobs.push(Job::WarmNow { account });
                }
            }
            Intent::AbandonStuckSwitch => self.change(Job::Abandon, jobs),
            Intent::DismissAbandoned => {
                self.abandoned = None;
                self.moves_given_up.clear();
            }
            Intent::SignIn { tool, again } => self.sign_in(tool.into(), again, jobs),
            Intent::PasteCode { code } => self.paste(&code, jobs),
            // Its button holds back while an app is quit for it; one sent anyway parks nothing.
            Intent::CancelSignIn => {
                // Before its tool has started there is nothing to stop yet: it is stopped as
                // it starts.
                if let Some(signing) = self.signing_in.take() {
                    if signing.started {
                        jobs.push(Job::StopSignIn { id: signing.id });
                    }
                    if let Some(location) = self.unpark(&signing) {
                        self.open_again(signing.tool, location, false, jobs);
                    }
                    // What it left parked is the core's record, which the read says.
                    if signing.in_its_app && !signing.quitting {
                        self.refresh(Asked::default(), now, jobs);
                    }
                }
            }
            Intent::PresentSheet { sheet } => self.present_sheet(sheet, jobs),
            // A save cannot be withdrawn, so its sheet stays until it answers.
            Intent::CloseSheet => {
                if self
                    .sheet
                    .as_ref()
                    .is_some_and(|sheet| self.saving.contains(sheet))
                {
                    return;
                }
                self.sheet = None;
                self.sheet_failure = None;
                self.left = None;
            }
            Intent::Show { place: None } if self.loading_kept => self.window_waits = true,
            Intent::Show { place } => self.show(place),
            Intent::NotificationClicked { argument } => match Clicked::read(&argument) {
                Clicked::Accounts => self.show(Some(Place::Accounts)),
                Clicked::Limits => self.show(Some(LIMITS)),
                Clicked::Switch(account) => self.switch_asked(account, jobs),
                Clicked::SwitchBack { from, to } => self.switch_back(from, &to, jobs),
                Clicked::SignInAgain(account) => {
                    let sheet = Sheet::SignInAgain {
                        account: account.into(),
                    };
                    self.present_sheet(sheet, jobs);
                }
            },
            Intent::DismissFailure => self.presented = None,
            Intent::DismissSheetFailure => self.sheet_failure = None,
            Intent::LookForTools => {
                if matches!(
                    self.sheet,
                    Some(Sheet::Add { .. } | Sheet::SignInAgain { .. })
                ) {
                    self.ask_installed(jobs);
                }
            }
            Intent::Enrol { tool } => {
                let tool = ProviderId::from(tool);
                if in_its_app(tool) {
                    if self.switching_of(tool) {
                        return;
                    }
                    let then = After::Enrol {
                        tool,
                        again: None,
                        from: None,
                    };
                    self.ask_before(new_account(tool), then, jobs);
                    return;
                }
                let enrol = Job::Enrol {
                    tool,
                    again: None,
                    from: None,
                    reopen: None,
                };
                self.change(enrol, jobs);
            }
            Intent::Rename { account, to } => {
                let kind = Sheet::Rename {
                    account: account.clone(),
                };
                if let Some(to) = self.saved_from(kind, &to) {
                    let from = self.saving_from();
                    let rename = Job::Rename {
                        account: account.into(),
                        to,
                        from,
                    };
                    self.change(rename, jobs);
                }
            }
            Intent::Forget { account } => self.change(
                Job::Forget {
                    account: account.into(),
                },
                jobs,
            ),
            Intent::UpdateConfig { account } => {
                let account = account.into();
                self.change(Job::UpdateConfig { account }, jobs);
            }
            Intent::Stow => self.stow(jobs),
            Intent::DismissStowed => self.stowed = None,
            Intent::PaneShown { pane } => self.pane_shown(pane, now, jobs),
            Intent::ReadSchedule => {
                jobs.push(Job::ReadSchedule {
                    after_change: false,
                });
                if self.offers_watch_service() {
                    jobs.push(Job::ReadWatchService {
                        after_change: false,
                    });
                }
            }
            Intent::SetSchedule { on } => {
                if self.machine.schedule(on) {
                    jobs.push(Job::SetSchedule { on });
                }
            }
            Intent::SetWatchService { on } => {
                if self.offers_watch_service() && self.machine.watch(on) {
                    jobs.push(Job::SetWatchService {
                        on,
                        switching: self.preferences.watch_switching(),
                    });
                }
            }
            Intent::LookForCommandLine => jobs.push(Job::FindCommandLine),
            Intent::WindowOpened { store } => self.windows.opened(&store),
            Intent::WindowClosed { store } => self.windows.closed(&store),
            Intent::PageShown { store, url } => {
                self.windows
                    .page_shown(&store, url, self.status.as_ref(), jobs);
            }
            Intent::WebsiteDataRemoved { store } => self.windows.data_removed(&store, jobs),
            Intent::StoreDeleted { store } => self.windows.store_deleted(&store, jobs),
            Intent::StoreHeld { store } => self.windows.store_held(&store, jobs),
            Intent::LinkArrived { text } => self.windows.link_arrived(&text),
            Intent::OpenLink { arrival, store } => {
                self.windows
                    .open_link(arrival, &store, self.status.as_ref());
            }
            Intent::DismissLink { arrival } => self.windows.dismiss_link(arrival),
            Intent::DownloadStarted { id, store, name } => {
                self.windows.download_started(id, &store, name);
            }
            Intent::DownloadSaving { id, file } => self.windows.download_saving(&id, file),
            Intent::DownloadEnded { id, end } => self.windows.download_ended(&id, end),
            Intent::ClearDownloads { store } => self.windows.clear_downloads(&store),
            Intent::Uninstall => self.uninstall(jobs),
            Intent::Outside { request, ask } => self.outside(request, ask, now, jobs),
        }
    }

    pub(crate) fn glance_to_a_file(&mut self) {
        self.glancing.file = true;
    }

    pub(crate) fn glance(&self) -> Option<&str> {
        self.glancing.text.as_deref()
    }

    pub(crate) fn widget_revision(&self) -> u64 {
        self.glancing.revision
    }

    // Made again as each read changes it, from the first read on, until Pitboard is being
    // removed, and written where the app named a file. A widget reads that file, so the
    // revision the app reloads widgets on moves once it is written.
    pub(crate) fn glanced(&mut self, glance: GlanceText) -> Option<Job> {
        let glancing = &mut self.glancing;
        if self.status.is_none()
            || self.removing.is_some()
            || glancing.text.as_ref() == Some(&glance.file)
        {
            return None;
        }
        let reload = glancing.shown.as_ref() != Some(&glance.shown);
        if reload {
            glancing.shown = Some(glance.shown);
        }
        glancing.text = Some(glance.file.clone());
        if !glancing.file {
            if reload {
                glancing.revision += 1;
            }
            return None;
        }
        self.at_work += 1;
        Some(Job::KeepGlance {
            text: glance.file,
            reload,
        })
    }

    // As a click in the app would, short of a question only the app can ask.
    fn outside(&mut self, request: u64, ask: OutsideAsk, now: Now, jobs: &mut Vec<Job>) {
        let (account, quit) = match ask {
            OutsideAsk::Switch { account } => (account, false),
            OutsideAsk::QuitAndSwitch { account } => (account, true),
            OutsideAsk::Refresh => {
                let asked = Asked {
                    fresh: true,
                    request: Some(request),
                    ..Asked::default()
                };
                return self.refresh(asked, now, jobs);
            }
        };
        let Some(read) = &self.status else {
            let ask = if quit {
                OutsideAsk::QuitAndSwitch { account }
            } else {
                OutsideAsk::Switch { account }
            };
            self.outside_held.push((request, ask));
            return;
        };
        let found = (read.accounts.iter())
            .find(|known| !known.unplaced && crate::present::glance_id(&known.id) == account);
        let Some((found, key)) = found.and_then(|found| Some((found, found.key.clone()?))) else {
            return self.answer_outside(request, Answered::Unknown);
        };
        let label = key.label.clone();
        let refused = if found.signed_in {
            Some(Answered::InUse { label })
        } else if crate::present::needs_sign_in(found) {
            Some(Answered::NeedsSignIn { label })
        } else if !found.standing.switchable() {
            Some(Answered::Unusable { label })
        } else if self.switch_under_way() || self.sign_in_under_way() {
            Some(Answered::Busy)
        } else if self.quitting.is_some() {
            Some(Answered::Asking)
        } else {
            None
        };
        match refused {
            Some(answered) => self.answer_outside(request, answered),
            None => {
                self.switch_asked(key, jobs);
                self.outside_switch = Some(OutsideSwitch { request, quit });
            }
        }
    }

    // Once the accounts are first read, or could not be, the switches asked before it.
    fn take_held(&mut self, now: Now, jobs: &mut Vec<Job>) {
        if self.removing.is_some() {
            for (request, _) in std::mem::take(&mut self.outside_held) {
                self.answer_outside(request, Answered::Uninstalling);
            }
        } else if self.status.is_some() {
            for (request, ask) in std::mem::take(&mut self.outside_held) {
                self.outside(request, ask, now, jobs);
            }
        } else if self.failure.is_some() && self.reads == 0 {
            for (request, _) in std::mem::take(&mut self.outside_held) {
                let doing = Doing::ReadUsage;
                self.answer_outside(request, Answered::Failed { doing });
            }
        }
    }

    fn answer_outside(&mut self, request: u64, answered: Answered) {
        self.answered.push_back((request, answered));
        while self.answered.len() > ANSWERS_KEPT {
            self.answered.pop_front();
        }
    }

    // Once more where the folder stayed, never once it is gone.
    pub(crate) fn may_uninstall(&self) -> bool {
        let again = match &self.removing {
            None | Some(Removing::Lost) => true,
            Some(Removing::Done(stayed)) => *stayed != Uninstalled::Removed,
            Some(_) => false,
        };
        again && self.signing_in.is_none() && !self.switch_under_way() && self.changing == 0
    }

    // Claude holding a saved sign-in quits first: the core deletes that only while it is closed.
    fn uninstall(&mut self, jobs: &mut Vec<Job>) {
        if !self.may_uninstall() {
            return;
        }
        let app = self.status.as_ref().and_then(|read| {
            read.accounts
                .iter()
                .find(|account| in_its_app(account.provider) && account.parked.is_some())
                .map(|account| account.provider)
                .or_else(|| read.awaiting.first().map(|awaiting| awaiting.provider))
        });
        match app {
            Some(tool) => self.ask_before(new_account(tool), After::Uninstall, jobs),
            None => self.end(false),
        }
    }

    // Nothing runs by itself from here on; `quitting`: an app is asked to quit first.
    fn end(&mut self, quitting: bool) {
        self.drop_question();
        for timer in [
            &mut self.look,
            &mut self.timed_read,
            &mut self.decide,
            &mut self.warm_timer,
        ] {
            *timer = Timer::Off;
        }
        self.windows.close_all(true);
        self.removing = Some(Removing::Ending {
            app: quitting,
            reopen: None,
        });
    }

    // Asked for once nothing that writes Pitboard's directory is under way.
    fn uninstall_once_idle(&mut self, jobs: &mut Vec<Job>) {
        let Some(Removing::Ending { app: false, reopen }) = self.removing.as_mut() else {
            return;
        };
        // A read waits on the tools found, and a sign-in enrols on a thread of its own.
        if self.at_work > 0 || self.asking_installed || self.sign_in_holding.is_some() {
            return;
        }
        self.removing = Some(Removing::Running {
            reopen: reopen.take(),
        });
        self.at_work += 1;
        jobs.push(Job::Uninstall);
    }

    // The stores go once the core has removed the accounts; one refused leaves both as they were.
    fn uninstalled(
        &mut self,
        done: Result<Uninstalled, Option<PitboardError>>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        if let Some(Removing::Running {
            reopen: Some((tool, location)),
        }) = self.removing.take()
        {
            jobs.push(Job::Open {
                location,
                tool,
                front: false,
            });
        }
        match done {
            Ok(stayed) => {
                self.removing = Some(Removing::Clearing(stayed));
                self.windows.remove_all(jobs);
                // The glance file is gone, and the widgets reload to find no accounts.
                self.glancing.text = None;
                self.glancing.shown = None;
                self.glancing.revision += 1;
            }
            Err(Some(error)) => {
                self.resume(now, jobs);
                self.present(Refused::of(Doing::Uninstall, error));
            }
            Err(None) => self.removing = Some(Removing::Lost),
        }
    }

    // Said once the app has deleted every store, or said it could not.
    fn cleared(&mut self) {
        if let Some(Removing::Clearing(stayed)) = self.removing
            && self.windows.stores_gone()
        {
            self.removing = Some(Removing::Done(stayed));
        }
    }

    // Uninstalling stopped before the core removed anything: what runs by itself runs again,
    // the watch the uninstall stopped with it.
    fn resume(&mut self, now: Now, jobs: &mut Vec<Job>) {
        self.removing = None;
        self.windows.close_all(false);
        if self.started {
            self.look = Timer::Due(now.running);
            self.timed_read = Timer::Due(now.running);
            jobs.push(Job::Watch);
        }
        self.decide_later(now);
        self.warm_later(now);
    }

    /// A pane of the main window is shown, or its button asks for what it shows again: what
    /// it shows is read, every time, as each Swift pane's `.task` read it on every visit. A
    /// menu bar app runs for days, and a check fixed in a terminal since would otherwise still
    /// read as failing. What was shown stays meanwhile.
    fn pane_shown(&mut self, pane: Pane, now: Now, jobs: &mut Vec<Job>) {
        match pane {
            // As AccountsPane.swift's `.task` read them: where the numbers are a minute old.
            Pane::Accounts => self.refresh(
                Asked {
                    older_than: self.cadence.stale_after,
                    ..Asked::default()
                },
                now,
                jobs,
            ),
            Pane::Activity => jobs.push(Job::ReadLog { limit: LOG_LIMIT }),
            Pane::Checks => {
                self.machine.checking += 1;
                jobs.push(Job::Check);
            }
        }
    }

    /// The name `typed` saves as, by the rule a sheet of `kind`'s offers Save by, unless the
    /// sheet up is already saving one: a save cannot be withdrawn, and the Swift sheets held
    /// their Save back until it had answered.
    fn saved_from(&self, kind: Sheet, typed: &str) -> Option<String> {
        if self
            .sheet
            .as_ref()
            .is_some_and(|sheet| self.saving.contains(sheet))
        {
            return None;
        }
        crate::present::saved_name(&kind, typed).ok().flatten()
    }

    /// The sheet up, as the one a save is asked from, which holds back until it answers.
    fn saving_from(&mut self) -> Option<Sheet> {
        let from = self.sheet.clone();
        if let Some(sheet) = &from {
            self.saving.push(sheet.clone());
        }
        from
    }

    /// A save from `from` has answered.
    fn saved(&mut self, from: Option<&Sheet>) {
        if let Some(from) = from
            && let Some(at) = self.saving.iter().position(|sheet| sheet == from)
        {
            self.saving.remove(at);
        }
    }

    /// What went wrong with a save, said in the sheet it was asked from while that is still
    /// up, where the name typed is there to correct, and in the window otherwise: something
    /// asked for that did not happen is said somewhere. Its warnings are said with it, and
    /// not beside the accounts, as the Swift's enrol and rename handed them back.
    fn save_failed(&mut self, from: Option<&Sheet>, refused: Refused) {
        if from.is_some() && self.sheet.as_ref() == from {
            let failure = self.number(refused);
            self.sheet_failure = Some(failure);
        } else {
            self.present(refused);
        }
    }

    // One at a time: a second would take the place of the one on screen and leave the first
    // tool waiting on a browser with nothing to stop it.
    fn sign_in(&mut self, tool: ProviderId, again: Option<String>, jobs: &mut Vec<Job>) {
        let held = self
            .sheet
            .as_ref()
            .is_some_and(|sheet| self.saving.contains(sheet));
        if self.signing_in.is_some() || held {
            return;
        }
        if in_its_app(tool) {
            if self.switching_of(tool) {
                return;
            }
            let account = match &again {
                Some(label) => Key::new(tool, label),
                None => new_account(tool),
            };
            let from = self.sheet.clone();
            let then = match self.in_app_add(tool) {
                InAppAdd::Current => After::Enrol { tool, again, from },
                InAppAdd::AfterSignIn => After::SignIn { tool, again, from },
            };
            self.ask_before(account, then, jobs);
            return;
        }
        let signing = self.new_sign_in(tool, again);
        self.signing_in = Some(signing);
        self.ask_for_sign_in(jobs);
    }

    // From the sheet that is up, which says nothing more of what went wrong the last time.
    fn new_sign_in(&mut self, tool: ProviderId, again: Option<String>) -> SigningIn {
        self.sign_ins += 1;
        self.sheet_failure = None;
        let in_use_before = self.status.as_ref().is_some_and(|status| {
            status.accounts.iter().any(|account| {
                account.provider == tool
                    && account.signed_in
                    && account.label.is_some()
                    && account.label == again
            })
        });
        SigningIn {
            in_use_before,
            id: self.sign_ins,
            in_its_app: in_its_app(tool),
            again,
            tool,
            said: String::new(),
            pasted_at: None,
            started: false,
            asked: false,
            quitting: false,
            opened_for_it: false,
            put_back: None,
            step: None,
        }
    }

    fn ask_before(&mut self, account: Key, then: After, jobs: &mut Vec<Job>) {
        self.drop_question();
        if let After::SignIn {
            from: Some(from), ..
        }
        | After::Enrol {
            from: Some(from), ..
        } = &then
        {
            self.saving.push(from.clone());
        }
        jobs.push(Job::Holding { account, then });
    }

    fn drop_question(&mut self) {
        if let Some(Quitting {
            asked: true,
            then: After::SignIn { from, .. } | After::Enrol { from, .. },
            ..
        }) = self.quitting.take()
        {
            self.saved(from.as_ref());
        }
    }

    // `quit`: Pitboard asked the app to quit, so its sign-in is the one made then.
    fn go_on(
        &mut self,
        account: Key,
        then: After,
        reopen: Option<String>,
        quit: bool,
        jobs: &mut Vec<Job>,
    ) {
        match then {
            After::Switch => {
                let tool = account.provider;
                let kept = self
                    .reopen_after_switch
                    .take_if(|(of, _)| *of == tool)
                    .map(|(_, copy)| copy);
                let mut reopen = reopen.or(kept);
                // Beside a park of its tool, one of the two would move the other's files.
                if let Some(id) = self.park_under_way(tool) {
                    self.reopen_after_switch = reopen.map(|copy| (tool, copy));
                    self.switch_after_park = Some((id, account));
                    return;
                }
                if let Some(signing) = self
                    .signing_in
                    .take_if(|signing| signing.in_its_app && signing.tool == tool)
                {
                    if signing.started {
                        jobs.push(Job::StopSignIn { id: signing.id });
                    }
                    reopen = reopen.or(self.unpark(&signing));
                    self.sheet = None;
                    self.sheet_failure = None;
                }
                jobs.push(Job::Switch { account, reopen });
            }
            After::SignIn { tool, again, from } => {
                self.saved(from.as_ref());
                let waiting = self.signing_in.as_ref().is_some_and(|signing| {
                    signing.quitting && signing.tool == tool && signing.again == again
                });
                let switching = self.switching_of(tool);
                if quit && (!waiting || switching) {
                    // A switch or a cancel took its place: nothing is parked.
                    if waiting {
                        self.signing_in = None;
                        self.sheet = None;
                        self.sheet_failure = None;
                    }
                    if let Some(location) = reopen {
                        self.open_again(tool, location, false, jobs);
                    }
                    return;
                }
                if !quit {
                    // One sign-in at a time, and no park beside a switch of its tool.
                    if self.signing_in.is_some() || switching {
                        return;
                    }
                    let signing = self.new_sign_in(tool, again);
                    self.signing_in = Some(signing);
                }
                let Some(signing) = self.signing_in.as_mut() else {
                    return;
                };
                signing.quitting = false;
                let id = signing.id;
                // An earlier park of the tool, cancelled, leaves its copy to open after this one.
                let reopen = reopen.or_else(|| {
                    (self.parking.iter_mut())
                        .filter(|parking| parking.tool == tool)
                        .find_map(|parking| parking.reopen.take())
                });
                self.parking.push(Parking { id, tool, reopen });
                self.changing += 1;
                self.ask_for_sign_in(jobs);
            }
            After::Uninstall => {
                if !quit {
                    if self.may_uninstall() {
                        self.end(false);
                    }
                } else if let Some(Removing::Ending { app, reopen: kept }) = self.removing.as_mut()
                {
                    *app = false;
                    *kept = reopen.map(|copy| (account.provider, copy));
                }
            }
            After::Enrol { tool, again, from } => {
                self.recording = Some((from.clone(), tool, Recording::Reading));
                let enrol = Job::Enrol {
                    tool,
                    again,
                    from,
                    reopen,
                };
                self.change(enrol, jobs);
            }
        }
    }

    // Taken before its thread was asked for, a sign-in parked nothing: its hold goes.
    fn unpark(&mut self, signing: &SigningIn) -> Option<String> {
        if signing.asked {
            return None;
        }
        let at = self
            .parking
            .iter()
            .position(|parking| parking.id == signing.id)?;
        self.change_over();
        self.parking.remove(at).reopen
    }

    fn switching_of(&self, tool: ProviderId) -> bool {
        self.switching
            .as_ref()
            .is_some_and(|switching| switching.of() == tool)
    }

    // A park of `tool` whose thread runs: one not yet asked for is still `signing_in`.
    fn park_under_way(&self, tool: ProviderId) -> Option<u64> {
        let unasked = self
            .signing_in
            .as_ref()
            .filter(|signing| !signing.asked)
            .map(|signing| signing.id);
        self.parking
            .iter()
            .find(|parking| parking.tool == tool && Some(parking.id) != unasked)
            .map(|parking| parking.id)
    }

    // Opened while a switch of its tool runs, the app could land in the middle of its move.
    fn open_again(&mut self, tool: ProviderId, location: String, front: bool, jobs: &mut Vec<Job>) {
        if self.switching_of(tool) {
            self.reopen_after_switch = Some((tool, location));
        } else {
            jobs.push(Job::Open {
                location,
                tool,
                front,
            });
        }
    }

    // Its warnings go beside the accounts: what it found is about the machine.
    fn sign_in_refused(&mut self, signing: &SigningIn, refused: Refused, jobs: &mut Vec<Job>) {
        if signing.in_its_app
            && let Some(code) = &refused.code
        {
            self.refused_by.insert(signing.tool, code.clone());
        }
        self.keep(&refused.warnings, jobs);
        if self.sheet.is_some() {
            let failure = self.number(refused);
            self.sheet_failure = Some(failure);
        } else {
            self.present(refused);
        }
    }

    /// Asks for the thread of the sign-in under way, once no sign-in before it may still hold
    /// the core's one sign-in at a time.
    ///
    /// One cancelled is stopped on the lane of sign-in calls, or by its own thread as its tool
    /// starts or goes quiet, and holds that lock until its tool has been stopped and waited
    /// for. Asked for meanwhile, the new one started on a thread of its own at once and was
    /// refused as a sign-in already waiting. It waits instead, shown as a sign-in whose tool
    /// has not started yet, which is what it is.
    fn ask_for_sign_in(&mut self, jobs: &mut Vec<Job>) {
        if self.sign_in_holding.is_some() {
            return;
        }
        let Some(signing) = self
            .signing_in
            .as_mut()
            .filter(|signing| !signing.asked && !signing.quitting)
        else {
            return;
        };
        signing.asked = true;
        self.sign_in_holding = Some(signing.id);
        jobs.push(Job::SignIn {
            id: signing.id,
            tool: signing.tool,
            again: signing.again.clone(),
        });
    }

    /// The sign-in `id` has let go of the core's one sign-in at a time, and the sign-in asked
    /// for since, if one is, starts now. Said once the stop on the lane of sign-in calls has
    /// stopped its tool and waited for it, or once its thread has given its last answer: its
    /// tool could not start, or finished, or was stopped and waited for, or what it did came
    /// to nothing.
    ///
    /// The stop says so for itself because the thread's last answer comes only once the
    /// tool's output has closed too, which a program the tool started can hold open for as
    /// long as it runs, as `codex login` does once Codex's npm launcher is killed. Waiting for
    /// it, every sign-in after a cancel showed as starting until that program ended.
    fn sign_in_let_go(&mut self, id: u64, jobs: &mut Vec<Job>) {
        if self.sign_in_holding == Some(id) {
            self.sign_in_holding = None;
        }
        self.ask_for_sign_in(jobs);
    }

    /// Types `code` back to the sign-in under way, where its tool asks for one: which it does
    /// only once it has started and said so, and once for each code it reads.
    fn paste(&mut self, code: &str, jobs: &mut Vec<Job>) {
        let code = trimmed(code);
        let Some(signing) = self.signing_in.as_mut() else {
            return;
        };
        if code.is_empty() || !signing.shown().wants_code {
            return;
        }
        signing.pasted_at = Some(signing.said.len());
        jobs.push(Job::PasteCode {
            id: signing.id,
            code: code.to_owned(),
        });
    }

    /// Puts `sheet` over the main window, opening it on the accounts the sheet is about, or
    /// over the macOS welcome while it is shown.
    fn present_sheet(&mut self, sheet: Sheet, jobs: &mut Vec<Job>) {
        // A sign-in under way keeps its sheet, and so does a change of an app's login from
        // the moment Pitboard asks whether the app runs: replacing it would leave the change
        // running with nothing on screen to finish or stop it.
        if self.signing_in.is_none() && !self.held_for_its_app() {
            if self.sheet.is_none()
                && matches!(sheet, Sheet::Add { .. } | Sheet::SignInAgain { .. })
            {
                self.ask_installed(jobs);
            }
            if self.sheet.as_ref() != Some(&sheet) {
                self.sheet_failure = None;
                // The sheet for a login left in a file looks each time it is put up, since the
                // file may have changed since it last did, and what it found goes with it.
                self.left = (sheet == Sheet::Stow).then_some(Leftover::Looking);
                if self.left.is_some() {
                    jobs.push(Job::LookLeft);
                }
            }
            self.sheet = Some(sheet);
        }
        self.show_over(Some(Place::Accounts));
    }

    // A save that changes no app's login lets its sheet be replaced and answers in the window.
    fn held_for_its_app(&self) -> bool {
        self.sheet.as_ref().is_some_and(|sheet| {
            self.saving.contains(sheet)
                && matches!(sheet, Sheet::Add { .. } | Sheet::SignInAgain { .. })
        })
    }

    /// Asks which tools are installed, unless that is being asked already, whose answer
    /// does as well.
    fn ask_installed(&mut self, jobs: &mut Vec<Job>) {
        if !self.asking_installed {
            self.asking_installed = true;
            jobs.push(Job::AskInstalled);
        }
    }

    /// Starts what runs by itself: what the model keeps read, a read now and every few
    /// minutes, a look for changes made somewhere else now, and the watch that asks the next.
    /// Once: a second start starts nothing.
    fn start(&mut self, now: Now, jobs: &mut Vec<Job>) {
        if self.started {
            return;
        }
        self.started = true;
        self.loading_kept = true;
        jobs.push(Job::LoadKept);
        // Once a launch, as the Swift model's init did: after that the schedule runs a command
        // line, or was never the app's to repair.
        jobs.push(Job::RepairSchedule);
        jobs.push(Job::Watch);
        self.timed_read = Timer::Due(now.running);
        self.look = Timer::Due(now.running);
        self.tick = Timer::Due(now.running + self.cadence.tick_every);
    }

    fn go_off(&mut self, now: Now, jobs: &mut Vec<Job>) {
        self.windows.go_off(now.running);
        // Nothing to do but be shown again, at the time it is now.
        if matches!(self.tick, Timer::Due(at) if at <= now.running) {
            self.tick = Timer::Due(now.running + self.cadence.tick_every);
        }
        if self.removing.is_some() {
            return;
        }
        if self.started && self.unlooked && self.look != Timer::Running && !self.holds_index() {
            self.look = Timer::Due(now.running);
        }
        if matches!(self.look, Timer::Due(at) if at <= now.running) {
            self.look = Timer::Running;
            self.unlooked = false;
            jobs.push(Job::Look);
        }
        if matches!(self.timed_read, Timer::Due(at) if at <= now.running) {
            self.timed_read = Timer::Running;
            self.refresh(
                Asked {
                    timed: true,
                    ..Asked::default()
                },
                now,
                jobs,
            );
        }
        if matches!(self.decide, Timer::Due(at) if at <= now.running) {
            self.look_to_switch(jobs);
        }
        if matches!(self.warm_timer, Timer::Due(at) if at <= now.running) {
            self.warm_tick(now, jobs);
        }
    }

    /// A read, once what is installed is known. The first read asks which tools are
    /// installed, and each read asks again while none has been found.
    fn refresh(&mut self, asked: Asked, now: Now, jobs: &mut Vec<Job>) {
        if self.installed.as_ref().is_none_or(Vec::is_empty) {
            self.waiting.push(asked);
            self.ask_installed(jobs);
            return;
        }
        self.read(asked, now, jobs);
    }

    /// What the model keeps is in, or came to nothing. What it had told before is not
    /// notified again, and what is shown, read while it was read, is advised on now, unless
    /// it only stands in for a read that failed.
    ///
    /// The preferences read are the app's from now on, with any "Not Now" said meanwhile, and
    /// kept at once where they did not come from their file, so the earlier store is taken
    /// once. The first time the app has ever been opened, the welcome opens, once. Where they
    /// could not be read, `None`, nothing is kept in place of what may be there, then or
    /// later, and the welcome does not open as though for the first time.
    ///
    /// With switching by itself on, the core's look waits for the launch's first read, which
    /// asks Anthropic for the reading it judges by: before it, the readings may be an older
    /// Pitboard's, which the look cannot judge, and a reason posted then is gone seconds
    /// later. Only what was read while the preferences were is looked at now.
    fn kept(
        &mut self,
        told: Told,
        preferences: Option<(Preferences, bool)>,
        windows: Option<Loaded>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        self.windows.loaded(windows, jobs);
        for (key, at) in told {
            self.told.entry(key).or_insert(at);
        }
        if let Some((read, from_file)) = preferences {
            let mut preferences = read.clone();
            preferences
                .second_account_declined
                .extend(self.preferences.second_account_declined.iter().cloned());
            if !preferences.has_been_seen {
                preferences.has_been_seen = true;
                self.window_waits = false;
                self.show(Some(Place::Welcome));
            }
            if preferences != read || !from_file {
                jobs.push(Job::KeepPreferences {
                    preferences: preferences.clone(),
                });
            }
            self.preferences = preferences;
            self.preferences_read = true;
        }
        self.loading_kept = false;
        if std::mem::take(&mut self.window_waits) {
            self.show(None);
        }
        if let Some(status) = self.status.clone()
            && !self.standing_in
        {
            self.advise(&status, jobs);
            self.look_to_switch(jobs);
        } else {
            self.decide_later(now);
        }
    }

    /// Advice about a read, the app's own or numbers taken from what is recorded. What is new
    /// is told, and what was said before stays for as long as the numbers bear it out. Advice
    /// worked out afresh would leave out what has been told, and put it away at the next
    /// read, seconds after it was said, with the account still out. One per tool, the newer
    /// first. Nothing while what was told is still being read.
    ///
    /// What reached the share chosen is notified only where no launch has notified it at the
    /// same reset, by the core's rule for one reset, and the record of that is kept.
    fn advise(&mut self, read: &Status, jobs: &mut Vec<Job>) {
        if self.loading_kept {
            return;
        }
        let tools = crate::tools_on(self.front.os());
        let new = Advice::about(read, &tools, &self.told_this_launch);
        for advice in &new {
            self.told_this_launch
                .insert(advice.key_of(), advice.window.resets_at.unwrap_or(0));
        }
        let mut kept = self.notify_at_limit(read, &tools, jobs);
        kept |= self.tell_unusable(read, jobs);
        if kept {
            jobs.push(Job::KeepTold {
                told: self.told.clone(),
            });
        }
        let standing: Vec<Advice> = new
            .into_iter()
            .chain(self.advice.iter().filter_map(|advice| advice.renewed(read)))
            .collect();
        let providers: Vec<ProviderId> = standing.iter().map(|a| a.provider).collect();
        self.advice = crate::present::in_order(&providers, &tools)
            .iter()
            .filter_map(|&provider| standing.iter().find(|a| a.provider == provider))
            .cloned()
            .collect();
    }

    // Under Switch, Claude Code's own notices say what switching came to.
    fn notify_at_limit(
        &mut self,
        read: &Status,
        tools: &[ProviderId],
        jobs: &mut Vec<Job>,
    ) -> bool {
        let choice = self.preferences.at_limit();
        if choice == AtLimit::Nothing {
            return false;
        }
        let at = self.preferences.threshold().percent();
        let mut kept = false;
        for heads_up in HeadsUp::about(read, tools, &self.told, at) {
            if choice == AtLimit::Switch && heads_up.provider == ProviderId::Claude {
                continue;
            }
            self.told
                .insert(heads_up.key_of(), heads_up.window.resets_at.unwrap_or(0));
            kept = true;
            jobs.push(Job::Post {
                notice: crate::present::at_limit_notice(self.front, &heads_up, read.now),
            });
        }
        kept
    }

    // Kept with what was told, so it is posted once for each account across launches, until
    // its login can be used again. One replaced outside Pitboard has a notice of its own.
    fn tell_unusable(&mut self, read: &Status, jobs: &mut Vec<Job>) -> bool {
        let unusable: BTreeMap<String, &Account> = read
            .accounts
            .iter()
            .filter(|account| {
                crate::present::needs_sign_in(account)
                    && account.standing != Standing::LoginReplaced
            })
            .map(|account| (format!("{UNUSABLE}{}", account.id), account))
            .collect();
        let before = self.told.len();
        self.told
            .retain(|key, _| !key.starts_with(UNUSABLE) || unusable.contains_key(key));
        let mut kept = self.told.len() != before;
        for (key, account) in unusable {
            if self.told.insert(key, 0).is_none() {
                kept = true;
                jobs.push(Job::Post {
                    notice: crate::present::unusable_notice(self.front, account),
                });
            }
        }
        kept
    }

    // Taken only once the preferences have been read, so never kept over a file that could
    // not be, nor lost under what the file says. Leaving Switch ends what it said and its wait.
    fn at_limit_set(&mut self, choice: AtLimit, at: u8, jobs: &mut Vec<Job>) {
        if !self.preferences_read {
            return;
        }
        let at = Some(i64::from(Threshold::clamped(i64::from(at)).percent()));
        if self.preferences.at_limit() == choice && self.preferences.auto_switch_at == at {
            return;
        }
        self.preferences.choose(choice);
        self.preferences.auto_switch_at = at;
        jobs.push(Job::KeepPreferences {
            preferences: self.preferences.clone(),
        });
        if choice != AtLimit::Switch {
            self.auto_standing = None;
            self.auto_refusals.ended();
        }
        if let Some(status) = self.status.clone()
            && !self.standing_in
        {
            self.advise(&status, jobs);
        }
        self.look_to_switch(jobs);
        self.watch_as_limits_says(jobs);
    }

    // `pitboard watch` does at a limit what it was installed to, so the service is installed
    // again as Limits has it, switching at its share under Switch and never otherwise: once a
    // change to it under way has been read back, and not after one that failed, which says so
    // beside its switch until the next is asked for.
    fn watch_as_limits_says(&mut self, jobs: &mut Vec<Job>) {
        let switching = self.preferences.watch_switching();
        let otherwise = matches!(
            self.machine.watch_service,
            Some(WatchService::Installed { switching: Some(installed) }) if installed != switching
        );
        if self.offers_watch_service()
            && self.preferences_read
            && otherwise
            && self.machine.watch_failed.is_none()
            && self.machine.watch(true)
        {
            jobs.push(Job::SetWatchService {
                on: true,
                switching,
            });
        }
    }

    fn switches(&self) -> bool {
        self.preferences.at_limit() == AtLimit::Switch
    }

    /// Asks the core's look whether to switch Claude Code by itself, where somebody turned
    /// that on: after every read, after numbers or a change the poll found, as it is turned
    /// on, and every `decide_every`. The look reads files alone and claims nothing, so it is
    /// asked whatever else is under way.
    fn look_to_switch(&mut self, jobs: &mut Vec<Job>) {
        if !self.preferences_read || !self.switches() || self.removing.is_some() {
            self.decide = Timer::Off;
            return;
        }
        self.decide = Timer::Running;
        jobs.push(Job::AutoLook {
            at: self.preferences.threshold().percent(),
        });
    }

    /// The core's look is next due `decide_every` from now, or as a refusal's wait ends where
    /// that comes first, while switching by itself is on and the model runs by itself.
    fn decide_later(&mut self, now: Now) {
        self.decide = if self.started && self.switches() {
            Timer::Due(
                self.auto_refusals
                    .due(now.running, self.cadence.decide_every),
            )
        } else {
            Timer::Off
        };
    }

    /// What the core's look came to, unless switching by itself was turned off meanwhile.
    /// What stands is said, and where only a decision under the core's lock can say, that is
    /// asked for. Files the core could not read decide nothing, which each read says too.
    fn auto_looked(
        &mut self,
        looked: Result<AutoLooked, PitboardError>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        self.decide_later(now);
        if !self.switches() {
            return;
        }
        match looked {
            Ok(AutoLooked::Stands(outcome)) => self.auto_stands(*outcome, now, jobs),
            Ok(AutoLooked::Act) => self.auto_act(now, jobs),
            Err(PitboardError::Failed { message, .. }) => {
                self.auto_standing = Some(AutoStanding::Stopped {
                    message,
                    until: None,
                });
            }
        }
    }

    /// The look says only a decision under the core's lock can say: asked for, and claimed
    /// as a switch is, so nothing else switches meanwhile and the poll takes its change for
    /// this app's own. Not while another switch, the question before one, or any other change
    /// of this app's own is under way, behind which it would wait on the lane of changes
    /// holding every switch somebody asks for, nor while a refusal's wait runs: a later look
    /// asks again.
    fn auto_act(&mut self, now: Now, jobs: &mut Vec<Job>) {
        if self.switch_under_way() || self.changing > 0 || self.auto_refusals.waits(now.running) {
            return;
        }
        self.switching = Some(Switching::Automatically);
        jobs.push(Job::AutoSwitch {
            at: self.preferences.threshold().percent(),
        });
    }

    // A switch made as the setting was turned off is still said: nobody was there to ask for it.
    fn auto_switched(
        &mut self,
        done: Result<AutoSwitched, PitboardError>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        match done {
            Ok(AutoSwitched::Switched {
                from,
                to,
                accounts,
                adoption,
                warnings,
                used,
                resets_at,
            }) => {
                jobs.push(Job::Post {
                    notice: crate::present::auto_switched_notice(
                        self.front,
                        &from,
                        &to,
                        &accounts,
                        &used,
                        &adoption,
                        now.epoch(),
                    ),
                });
                self.auto_told.clear();
                self.switched_back = None;
                self.auto_refusals.ended();
                let (left, put) = accounts.clone();
                let back = Back {
                    from: left.clone(),
                    resets_at,
                };
                let switched = Switched {
                    outcome: Switch::Switched {
                        provider: ProviderId::Claude,
                        from: left.label,
                        to: put.clone(),
                        adoption: adoption.clone(),
                    },
                    warnings: warnings.clone(),
                };
                self.auto_standing = Some(AutoStanding::Came(AutoSwitched::Switched {
                    from,
                    to,
                    accounts,
                    adoption,
                    warnings,
                    used,
                    resets_at,
                }));
                self.switched(&put, None, Ok(switched), now, jobs);
                if let Some(last) =
                    (self.last_switches.iter_mut()).find(|last| last.provider == ProviderId::Claude)
                {
                    last.back = Some(back);
                }
            }
            _ if !self.switches() => self.switching = None,
            Ok(outcome) => {
                self.switching = None;
                self.auto_stands(outcome, now, jobs);
            }
            Err(PitboardError::Failed { code, message, .. }) => {
                self.switching = None;
                let wait = self.auto_refusals.refused(now.running);
                self.auto_standing = Some(AutoStanding::Stopped {
                    message: message.clone(),
                    until: Some(now.epoch() + wait),
                });
                if self.auto_told.insert(format!("refused/{code}")) {
                    jobs.push(Job::Post {
                        notice: crate::present::auto_refused_notice(self.front, &code, &message),
                    });
                }
            }
        }
    }

    // A reason is posted once for what tells it apart until the next switch.
    fn auto_stands(&mut self, outcome: AutoSwitched, now: Now, jobs: &mut Vec<Job>) {
        self.auto_refusals.ended();
        self.switched_back
            .take_if(|back| back.until.is_some_and(|until| until <= now.epoch()));
        let told = match &outcome {
            AutoSwitched::Skipped { from, code, .. }
                if matches!(*code, "already_switched" | "settling")
                    && (self.switched_back.as_ref())
                        .is_some_and(|back| back.from.typed() == *from) =>
            {
                None
            }
            AutoSwitched::Skipped {
                key,
                from,
                used,
                why,
                ..
            } => Some((
                key.clone(),
                crate::present::auto_skipped_notice(self.front, key, from, used, why),
            )),
            AutoSwitched::NotWatching { key, why, .. } => Some((
                key.clone(),
                crate::present::auto_not_watching_notice(self.front, key, why),
            )),
            AutoSwitched::Watching { .. }
            | AutoSwitched::Waiting { .. }
            | AutoSwitched::Switched { .. } => None,
        };
        if let Some((key, notice)) = told
            && self.auto_told.insert(key)
        {
            jobs.push(Job::Post { notice });
        }
        self.auto_standing = Some(AutoStanding::Came(outcome));
    }

    /// What a read found of warming up: where it could not be read, what was known stands,
    /// with why.
    fn warm_read(&mut self, warm: Result<Overview, PitboardError>) {
        match warm {
            Ok(warm) => {
                self.warm.overview = Some(warm);
                self.warm.unreadable = None;
            }
            Err(PitboardError::Failed { message, .. }) => self.warm.unreadable = Some(message),
        }
    }

    /// Asks the core to warm up whichever account turned on is due, where one is turned on,
    /// none is under way and no refusal's wait runs. The core looks at its files first, so
    /// asking costs nothing where none is due. Asked after every read, as its timer goes off
    /// every `decide_every`, after a change to which accounts are warmed up, and once a
    /// warm-up has started a period, for the next.
    fn warm_tick(&mut self, now: Now, jobs: &mut Vec<Job>) {
        if !self.started || !self.warm.any_on() || self.removing.is_some() {
            self.warm_timer = Timer::Off;
            return;
        }
        if self.warm.running.is_some() {
            return;
        }
        if self.warm.refusals.waits(now.running) {
            self.warm_later(now);
            return;
        }
        self.warm.running = Some(None);
        self.warm_timer = Timer::Running;
        jobs.push(Job::WarmUp);
    }

    /// The next ask is due `decide_every` from now, or as a refusal's wait ends where that
    /// comes first, while an account is turned on.
    fn warm_later(&mut self, now: Now) {
        self.warm_timer = if self.started && self.warm.any_on() {
            Timer::Due(
                self.warm
                    .refusals
                    .due(now.running, self.cadence.decide_every),
            )
        } else {
            Timer::Off
        };
    }

    /// What a warm-up came to. A period started is quiet: the reading the core took after it
    /// shows on the account, and the next due account is asked for at once. One by itself that
    /// came to nothing is said once for its words in a notification, and under the setting
    /// until the core next says where each account stands; one somebody asked for is said in
    /// the window, as anything they ask for is. A refusal of one by itself waits as the core
    /// waits after as many attempts.
    fn warmed_up(
        &mut self,
        asked: Option<Key>,
        done: Result<WarmUp, WarmRefused>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        self.warm.running = None;
        match done {
            Ok(WarmUp::Started { account, .. }) => {
                self.warm.refusals.ended();
                self.warm.failed = None;
                self.warm.told.warmed(&account);
                self.warm_tick(now, jobs);
            }
            Ok(WarmUp::Stands(overview)) => {
                self.warm.refusals.ended();
                self.warm.failed = None;
                self.warm.overview = Some(*overview);
                self.warm_later(now);
            }
            Ok(WarmUp::Running { .. } | WarmUp::Nothing) => {
                self.warm.refusals.ended();
                self.warm.failed = None;
                self.warm_later(now);
            }
            Err(WarmRefused { told_apart, error }) => {
                let PitboardError::Failed { message, .. } = &*error;
                match asked {
                    Some(account) => {
                        self.present(Refused::of(Doing::Warm { account }, *error));
                    }
                    None => {
                        self.warm.failed = Some(message.clone());
                        self.warm.refusals.refused(now.running);
                        if self.warm.told.untold(&told_apart) {
                            jobs.push(Job::Post {
                                notice: crate::present::warm_failed_notice(
                                    self.front,
                                    &told_apart.key,
                                    message,
                                ),
                            });
                        }
                    }
                }
                self.warm_later(now);
            }
        }
    }

    /// A change to which accounts are warmed up, or when, is over: what the core says of each
    /// one after it is shown, and the one now due is asked for.
    fn warm_set(&mut self, done: Result<Overview, PitboardError>, now: Now, jobs: &mut Vec<Job>) {
        match done {
            Ok(overview) => {
                self.warm.overview = Some(overview);
                self.warm_tick(now, jobs);
            }
            Err(error) => self.present(Refused::of(Doing::ChangeWarm, error)),
        }
    }

    fn read(&mut self, asked: Asked, now: Now, jobs: &mut Vec<Job>) {
        let older_than = i64::try_from(asked.older_than.as_millis()).unwrap_or(i64::MAX);
        let ticket = Ticket {
            changes_seen: self.changes_seen,
            timed: asked.timed,
            after_switch: asked.after_switch,
            saying: asked.saying,
            after_change: asked.after_change,
            request: asked.request,
        };
        if let Some(at) = self.updated_ms
            && now.epoch_ms.saturating_sub(at) < older_than
        {
            self.over(ticket, now);
            return;
        }
        self.reads += 1;
        jobs.push(Job::Read {
            ticket,
            fresh: asked.fresh,
        });
    }

    /// A read is over, or was not needed: the timer that asked for it is set again.
    fn read_over(&mut self, timed: bool, now: Now) {
        if timed && self.started {
            self.timed_read = Timer::Due(now.running + self.cadence.read_every);
        }
    }

    /// A read that was under way has ended, one way or another.
    fn landed(&mut self, ticket: Ticket, now: Now) {
        self.reads = self.reads.saturating_sub(1);
        self.over(ticket, now);
    }

    /// A read is over, or was not needed. The read after a switch ending ends the switch, the
    /// read after any other change of this app's own ends that change, and what a sign-in or
    /// a config update warned about is said once the read after it is over, whatever it came
    /// to, as the Swift model said a sign-in's once its `refresh` had returned.
    fn over(&mut self, ticket: Ticket, now: Now) {
        if ticket.after_switch {
            self.switching = None;
        }
        if let Some(request) = ticket.request {
            let answered = match &self.failure {
                Some(_) => Answered::Failed {
                    doing: Doing::ReadUsage,
                },
                None => Answered::Updated {
                    at: self
                        .updated_ms
                        .map_or(now.epoch(), |ms| ms.div_euclid(1000)),
                },
            };
            self.answer_outside(request, answered);
        }
        if ticket.after_change {
            self.change_over();
        }
        if let Some(number) = ticket.saying
            && let Some(at) = self
                .said_after_read
                .iter()
                .position(|(of, _)| *of == number)
        {
            let (_, said) = self.said_after_read.remove(at);
            let new: Vec<Warning> = said
                .into_iter()
                .filter(|warning| !self.warnings.contains(warning))
                .collect();
            self.warnings.extend(new);
        }
        self.read_over(ticket.timed, now);
    }

    // The next look is the watch's to ask, but where nothing watches.
    fn look_over(&mut self, now: Now) {
        if self.started {
            self.look = if self.unwatched {
                Timer::Due(now.running + self.cadence.look_every)
            } else {
                Timer::Off
            };
        }
    }

    /// The watch heard a write, or began: looked at once nothing holds the look back.
    fn written(&mut self) {
        self.unwatched = false;
        if self.started && self.removing.is_none() {
            self.unlooked = true;
        }
    }

    /// Nothing watches Pitboard's directory: it is looked at every `look_every` instead.
    fn unwatched(&mut self, now: Now) {
        self.unwatched = true;
        if self.started && self.removing.is_none() && self.look == Timer::Off {
            self.look = Timer::Due(now.running + self.cadence.look_every);
        }
    }

    fn answer(&mut self, answer: Answer, now: Now, jobs: &mut Vec<Job>) {
        match answer {
            Answer::Read {
                ticket,
                readings_before,
                changed_before,
                read,
                warm,
            } => {
                self.warm_read(warm);
                self.read_landed(ticket, readings_before, changed_before, read, now, jobs);
                self.warm_tick(now, jobs);
            }
            Answer::ReadOffline { why, read, warm } => {
                self.warm_read(warm);
                self.known(why, read.ok(), now, jobs);
            }
            Answer::WarmedUp { asked, done } => self.warmed_up(asked, done, now, jobs),
            Answer::WarmSet { done } => self.warm_set(done, now, jobs),
            Answer::Looked { changed, measured } => self.looked(changed, measured, now, jobs),
            Answer::Written => self.written(),
            Answer::Unwatched => self.unwatched(now),
            Answer::Installed { mut tools } => {
                let gone = self.gone_since_asked.take();
                if let Some(gone) = gone {
                    tools.retain(|&tool| tool != gone);
                }
                self.installed = Some(tools);
                self.installed_over(now, jobs);
                if gone.is_some() {
                    self.ask_installed(jobs);
                }
            }
            Answer::Held {
                account,
                question,
                then,
            } => self.held(account, question, then, jobs),
            Answer::Quit {
                question,
                outcome,
                then,
            } => self.quit_over(question, outcome, then, now, jobs),
            Answer::Switched {
                account,
                reopen,
                done,
            } => self.switched(&account, reopen, done.map_err(Some), now, jobs),
            Answer::AutoLooked { looked } => self.auto_looked(looked, now, jobs),
            Answer::AutoSwitched { done } => self.auto_switched(done, now, jobs),
            // The page says Pitboard opened the app only where it did.
            Answer::Opened {
                tool,
                front: true,
                opened,
            } => {
                if let Some(signing) = self
                    .signing_in
                    .as_mut()
                    .filter(|signing| signing.in_its_app && signing.tool == tool)
                {
                    signing.opened_for_it = opened;
                }
            }
            Answer::Opened { front: false, .. }
            | Answer::Pasted
            | Answer::Stopped
            | Answer::Saved
            | Answer::GlanceKept { reload: false } => {}
            Answer::GlanceKept { reload: true } => self.glancing.revision += 1,
            Answer::Posted { title, refused } => self.machine.posted(title, refused, now.epoch()),
            Answer::Kept {
                told,
                preferences,
                windows,
            } => self.kept(told, preferences, windows, now, jobs),
            Answer::WindowsKept { write } => self.windows.written(write),
            Answer::SharedChecked { stores, shared } => {
                self.windows.shared(stores, shared, jobs);
            }
            Answer::Enrolled {
                tool,
                again,
                from,
                reopen,
                done,
            } => {
                let enrolling = Enrolling {
                    tool,
                    again: again.as_deref(),
                    from: from.as_ref(),
                };
                self.enrolled_now(&enrolling, reopen, done.map_err(Some), now, jobs);
            }
            Answer::Renamed {
                account,
                to,
                from,
                done,
            } => {
                let renaming = Renaming {
                    account: &account,
                    to: &to,
                    from: from.as_ref(),
                };
                self.renamed(&renaming, done.map_err(Some), now, jobs);
            }
            Answer::Forgot { account, done } => {
                self.forgot(&account, done.map_err(Some), now, jobs);
            }
            Answer::ConfigUpdated(done) => self.config_updated(done.map_err(Some), now, jobs),
            Answer::LookedLeft(found) => self.looked_left(found.map_err(Some)),
            Answer::Stowed { from, done } => {
                self.stowed(from.as_ref(), done.map_err(Some), now, jobs);
            }
            Answer::Abandoned(done) => self.abandon_over(done.map_err(Some), now, jobs),
            Answer::SignInStarted {
                id,
                started,
                put_back,
            } => {
                // A tool that could not start leaves its thread nothing more to do.
                let over = started.is_err();
                self.sign_in_started(id, started.map_err(Some), put_back, now, jobs);
                if over {
                    self.sign_in_let_go(id, jobs);
                }
            }
            Answer::SignInSaid { id, text, step } => {
                if let Some(signing) = self.signing_in.as_mut().filter(|s| s.id == id) {
                    signing.said.push_str(&text);
                    if step.is_some() {
                        signing.step = step;
                    }
                }
            }
            // Only the sign-in still under way is enrolled. One cancelled since was stopped,
            // and what its stopped tool left is not something that went wrong.
            Answer::SignInQuiet { id } => {
                let enrol = self.signing_in.as_ref().is_some_and(|s| s.id == id);
                let over = Job::SignInOver { id, enrol };
                if enrol {
                    self.change(over, jobs);
                } else {
                    jobs.push(over);
                }
            }
            Answer::SignInFinished { id, done } => {
                self.sign_in_finished(id, done.map_err(Some), now, jobs);
                self.sign_in_let_go(id, jobs);
            }
            Answer::SignInStopped { id } => self.sign_in_let_go(id, jobs),
            Answer::Repaired { repaired, own } => {
                self.machine.own = Some(own);
                // Shown again only where it was repaired. Nothing repaired, or a repair that
                // failed, says nothing: the schedule is as it was, and doctor reports it.
                if matches!(repaired, Ok(true)) {
                    jobs.push(Job::ReadSchedule {
                        after_change: false,
                    });
                }
            }
            Answer::ScheduleRead {
                schedule,
                own,
                after_change,
            } => self
                .machine
                .schedule_read(schedule, Some(own), after_change),
            Answer::ScheduleSet { own, outcome } => {
                if self.machine.scheduled(own, outcome) {
                    jobs.push(Job::ReadSchedule { after_change: true });
                }
            }
            Answer::WatchServiceRead {
                service,
                own,
                after_change,
            } => {
                self.machine
                    .watch_service_read(service, Some(own), after_change);
                if after_change {
                    self.watch_as_limits_says(jobs);
                }
            }
            Answer::WatchServiceSet { own, outcome } => {
                if self.machine.watched(own, outcome) {
                    jobs.push(Job::ReadWatchService { after_change: true });
                }
            }
            Answer::Checked { doctored } => self.machine.checked(doctored, now.epoch()),
            Answer::Logged { changes } => self.machine.logged(changes),
            Answer::CommandLineFound { found, own } => {
                self.machine.command_line = Some(found);
                self.machine.own = Some(own);
            }
            Answer::Uninstalled(done) => self.uninstalled(done.map_err(Some), now, jobs),
            Answer::Lost(job) => match job {
                Job::Read { ticket, .. } => self.landed(ticket, now),
                Job::ReadOffline { why } => self.known(why, None, now, jobs),
                Job::Look => self.look_over(now),
                Job::Watch => self.unwatched(now),
                Job::AskInstalled => self.installed_over(now, jobs),
                // As the Swift model took a holding it could not read: nothing holds it.
                Job::Holding { account, then } => self.held(account, None, then, jobs),
                // Nothing is switched under an app nobody saw go.
                Job::Quit { question, then } => {
                    self.quit_over(question, QuitOutcome::StillRunning, then, now, jobs);
                }
                Job::Switch { account, reopen } => {
                    self.switched(&account, reopen, Err(None), now, jobs);
                }
                Job::Replace { account } => {
                    self.switched(&account, None, Err(None), now, jobs);
                }
                // Nothing is said of a look that came to nothing: the next is at its time.
                Job::AutoLook { .. } => self.decide_later(now),
                // Whether it warmed anything up is not known; the next ask finds out.
                Job::WarmUp | Job::WarmNow { .. } => {
                    self.warm.running = None;
                    self.warm_later(now);
                }
                Job::SetWarm { .. } | Job::SetWarmRange { .. } => {
                    self.present(Refused::lost(Doing::ChangeWarm));
                }
                // Whether it switched is not known, and nothing is said of it: the accounts
                // are read again to show who is in use, and the next look decides again.
                Job::AutoSwitch { .. } => {
                    self.switching = None;
                    self.refresh(Asked::default(), now, jobs);
                }
                Job::Open { .. } => {}
                Job::Abandon => self.abandon_over(Err(None), now, jobs),
                // Its tool may or may not have started, and nothing can be said of what it
                // did: the sign-in is over, said as a failure of its own. Its thread has
                // ended, or never started, and what it held was let go of as it unwound.
                Job::SignIn { id, .. } => {
                    self.sign_in_started(id, Err(None), None, now, jobs);
                    self.sign_in_let_go(id, jobs);
                }
                Job::SignInOver { id, enrol } => {
                    if enrol {
                        self.sign_in_finished(id, Err(None), now, jobs);
                    }
                    self.sign_in_let_go(id, jobs);
                }
                // A stop lost to a panic leaves it to the sign-in's own thread to say it has
                // let go of the core's one sign-in at a time.
                Job::PasteCode { .. } | Job::StopSignIn { .. } => {}
                Job::Enrol {
                    tool,
                    again,
                    from,
                    reopen,
                } => {
                    let enrolling = Enrolling {
                        tool,
                        again: again.as_deref(),
                        from: from.as_ref(),
                    };
                    self.enrolled_now(&enrolling, reopen, Err(None), now, jobs);
                }
                Job::Rename { account, to, from } => {
                    let renaming = Renaming {
                        account: &account,
                        to: &to,
                        from: from.as_ref(),
                    };
                    self.renamed(&renaming, Err(None), now, jobs);
                }
                Job::Forget { account } => self.forgot(&account, Err(None), now, jobs),
                Job::UpdateConfig { .. } => self.config_updated(Err(None), now, jobs),
                Job::LookLeft => self.looked_left(Err(None)),
                Job::Stow { from, .. } => self.stowed(from.as_ref(), Err(None), now, jobs),
                // Nothing kept could be read: nothing was told before, as far as anyone knows,
                // and the windows' records are held for this launch, never written.
                Job::LoadKept => self.kept(Told::new(), None, None, now, jobs),
                Job::KeepTold { .. }
                | Job::KeepPreferences { .. }
                | Job::KeepGlance { .. }
                | Job::Post { .. } => {}
                // What it wrote is not known: a window waiting for it opens all the same, and
                // the next write writes the records whole.
                Job::KeepWindows { write, .. } => self.windows.written(write),
                // Nobody could say: none is deleted, and the next read asks again.
                Job::CheckShared { stores } => self.windows.shared(stores, None, jobs),
                // The schedule is as it was, as a repair that failed leaves it.
                Job::RepairSchedule => {}
                Job::ReadSchedule { after_change } => {
                    self.machine.schedule_read(None, None, after_change);
                }
                // What it did is not known: the schedule is read back to show where it is.
                Job::SetSchedule { .. } => jobs.push(Job::ReadSchedule { after_change: true }),
                Job::ReadWatchService { after_change } => {
                    self.machine.watch_service_read(None, None, after_change);
                }
                Job::SetWatchService { .. } => {
                    jobs.push(Job::ReadWatchService { after_change: true });
                }
                Job::Check => self.machine.checked(None, now.epoch()),
                Job::ReadLog { .. } | Job::FindCommandLine => {}
                Job::Uninstall => self.uninstalled(Err(None), now, jobs),
            },
        }
    }

    /// The tool of the sign-in `id` has started, or could not be. `Err(None)` is a start
    /// that came to nothing.
    fn sign_in_started(
        &mut self,
        id: u64,
        started: Result<(), Option<PitboardError>>,
        put_back: Option<Key>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        let current = self.signing_in.as_ref().is_some_and(|s| s.id == id);
        let parked = self
            .parking
            .iter()
            .position(|parking| parking.id == id)
            .map(|at| self.parking.remove(at));
        let switch = self
            .switch_after_park
            .take_if(|(park, _)| *park == id)
            .map(|(_, account)| account);
        let reopen = parked
            .as_ref()
            .and_then(|parked| Some((parked.tool, parked.reopen.clone()?)));
        match started {
            Ok(()) => {
                match self.signing_in.as_mut().filter(|_| current) {
                    Some(signing) => {
                        signing.started = true;
                        if signing.in_its_app {
                            signing.put_back = put_back;
                        }
                    }
                    // Cancelled while it was starting: what started is stopped rather than
                    // watched.
                    None => jobs.push(Job::StopSignIn { id }),
                }
                if let Some((tool, location)) = reopen {
                    self.open_again(tool, location, current, jobs);
                }
                if parked.is_some() {
                    self.changes_seen += 1;
                    self.read_after_change(now, jobs);
                }
            }
            Err(error) => {
                if let Some((tool, location)) = reopen {
                    self.open_again(tool, location, false, jobs);
                }
                if parked.is_some() {
                    self.change_over();
                }
                // Cancelled, and it never started: nothing to say.
                if let Some(signing) = self.signing_in.take_if(|_| current) {
                    self.sign_in_failed(&signing, error, jobs);
                }
            }
        }
        if let Some(account) = switch {
            jobs.push(Job::Holding {
                account,
                then: After::Switch,
            });
        }
    }

    /// What enrolling what the sign-in `id` signed in to came to. `Err(None)` is a finish
    /// that came to nothing.
    ///
    /// Cancelled while it was being finished, too late to stop the tool, what it signed in
    /// to is enrolled all the same, so it is read, and nothing else is touched: a sheet or a
    /// sign-in started since is somebody else's. Failed once cancelled, the cancel reached the
    /// tool first, which was somebody's answer, and says nothing.
    fn sign_in_finished(
        &mut self,
        id: u64,
        done: Result<Enrolled, Option<PitboardError>>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        let signing = self.signing_in.take_if(|s| s.id == id);
        if done.is_err() {
            self.change_over();
        }
        match (done, signing) {
            (Ok(done), signing) => {
                if let Some(signing) = &signing {
                    self.refused_by.remove(&signing.tool);
                }
                self.changes_seen += 1;
                self.updated_ms = None;
                let mut asked = Asked {
                    after_change: true,
                    ..Asked::default()
                };
                if let Some(signing) = signing {
                    // The sheet it was started from, if it is still up, gives way to the name.
                    self.sheet_failure = None;
                    if self.sheet.take().is_some() {
                        self.name_next(signing.tool, &done, jobs);
                    }
                    let said = self.enrolled(&signing, done);
                    asked.saying = self.say_after_read(said);
                }
                self.refresh(asked, now, jobs);
            }
            (Err(error), Some(signing)) => self.sign_in_failed(&signing, error, jobs),
            (Err(_), None) => {}
        }
    }

    // A program the core could not find is one the last look found, or one asked for before
    // any look: it is said missing where the sheet says so of any tool, and looked for again.
    fn sign_in_failed(
        &mut self,
        signing: &SigningIn,
        error: Option<PitboardError>,
        jobs: &mut Vec<Job>,
    ) {
        let doing = Doing::SignIn {
            again: signing.again.clone(),
        };
        let mut refused = match error {
            Some(error) => Refused::of(doing, error),
            None => Refused::lost(doing),
        };
        let missing_code = format!("{}_program_missing", signing.tool.code());
        if refused.code.as_deref() == Some(missing_code.as_str()) {
            if let Some(tools) = self.installed.as_mut() {
                tools.retain(|&tool| tool != signing.tool);
            }
            if self.asking_installed {
                self.gone_since_asked = Some(signing.tool);
            } else {
                self.ask_installed(jobs);
            }
            if self.sheet.is_some() {
                self.keep(&refused.warnings, jobs);
                return;
            }
            refused.cause = Cause::ProgramMissing { tool: signing.tool };
        }
        self.sign_in_refused(signing, refused, jobs);
    }

    /// What a finished sign-in says beyond the row it adds, and what it warned about that is
    /// to be said beside the read that follows, as AppModel.swift's `enrolled` said it.
    ///
    /// Signing in to the account in use puts its new login in use at once, and what that
    /// means for sessions already running is kept the way a switch's is. The tool did not
    /// switch, so what its last switch said stays true and stays with it; a count of the
    /// same sessions naming this account's old login, beside one naming the account the
    /// switch left, would contradict it. Where the switch could not tell what was running,
    /// the sign-in's word that it could not tell either is the same thing said again, and
    /// the switch's keeps what not to do in them.
    fn enrolled(&mut self, signing: &SigningIn, done: Enrolled) -> Vec<Warning> {
        let EnrolledAs::InUse { again } = done.outcome else {
            return done.warnings;
        };
        let provider = signing.tool;
        let to = Key::new(provider, &done.label);
        let mut said = self
            .last_switches
            .iter()
            .find(|last| last.provider == provider && last.to == to)
            .cloned()
            .unwrap_or_else(|| LastSwitch {
                provider,
                to,
                follows_at: None,
                restart: None,
                said: None,
                warnings: Vec::new(),
                opens: None,
                back: None,
            });
        let signed_in = SignedIn {
            label: done.label,
            again,
            in_its_app: signing.in_its_app,
        };
        // An account an app had not in use before is said as a switch to it.
        if signing.in_its_app && !signing.in_use_before {
            said.opens = Some(Opens::SignedIn(signed_in));
        } else {
            said.said = Some(signed_in);
        }
        let switch_said = |code: &str| said.warnings.iter().any(|warning| warning.code == code);
        let counted = switch_said("sessions_still_running");
        let untold = switch_said("sessions_unknown");
        let new: Vec<Warning> = done
            .warnings
            .into_iter()
            .filter(|warning| {
                !said.warnings.contains(warning)
                    && !(counted && warning.code == "sessions_keep_old_login")
                    && !(untold && warning.code == "sessions_unknown")
            })
            .collect();
        said.warnings.extend(new);
        self.remember(said);
        Vec::new()
    }

    /// The question of what is installed is answered, or came to nothing: the reads that
    /// waited for it go ahead, as the Swift model's read did once its question returned.
    fn installed_over(&mut self, now: Now, jobs: &mut Vec<Job>) {
        self.asking_installed = false;
        for asked in std::mem::take(&mut self.waiting) {
            self.read(asked, now, jobs);
        }
    }

    fn read_landed(
        &mut self,
        ticket: Ticket,
        readings_before: i64,
        changed_before: i64,
        read: Result<Status, PitboardError>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        // Started before a change, so with who was signed in before it.
        if ticket.changes_seen != self.changes_seen {
            self.landed(ticket, now);
            return;
        }
        match read {
            Ok(read) => {
                let read = match &self.status {
                    Some(shown) => refusals_kept(read, shown),
                    None => read,
                };
                self.stuck = read
                    .warnings
                    .iter()
                    .any(|w| w.code == "recovery_undetermined");
                self.forget_switches_undone(&read);
                self.tell_replaced(&read, jobs);
                self.warnings = read.warnings.clone();
                self.advise(&read, jobs);
                self.windows.read_enrolled(&read, jobs);
                self.status = Some(read);
                self.standing_in = false;
                self.failure = None;
                self.updated_ms = Some(now.epoch_ms);
                // As they stood before the read, because a session can record newer
                // numbers, and a terminal can switch, while the read waits on a service, and
                // the read then shows nothing of either. Taken after, they counted as seen
                // though nothing had shown them. What the read writes itself costs the next
                // look one read of what is known.
                self.changed_at = Some(changed_before);
                self.readings_at = Some(readings_before);
                self.landed(ticket, now);
                self.look_to_switch(jobs);
            }
            Err(PitboardError::Failed {
                code,
                message,
                warnings,
                ..
            }) => {
                // Said as the read fails, so a read that lands before what is known is in
                // says its own and keeps it. The Swift said it once its fallback was in, over
                // whatever a read that landed meanwhile had said.
                self.stuck = code == "recovery_undetermined";
                self.failure = Some(ReadFailure { code, message });
                // What went wrong this time, in place of what was wrong last time. A failure
                // carries its own warnings, and leaving the previous read's in place showed a
                // fresh network error above warnings that may have been fixed since.
                self.tell_replaced_found(&warnings, jobs);
                self.warnings = warnings;
                if self.status.is_none() {
                    // Still reading until what is known is in.
                    jobs.push(Job::ReadOffline {
                        why: Why::InPlaceOf(ticket),
                    });
                } else {
                    self.landed(ticket, now);
                }
            }
        }
    }

    /// A login replaced outside Pitboard is said by every read until it is put right, and
    /// posted once for its account while it stands, since nobody may have the window open to
    /// see it: again only once a read no longer says it and a later one does. Its words name
    /// whose login is stored now, which every switch changes, so it is known by its account.
    fn tell_replaced(&mut self, read: &Status, jobs: &mut Vec<Job>) {
        self.replaced_told = self.post_replaced(&read.warnings, &read.accounts, jobs);
    }

    // A failed read or a refused change says what it found, and nothing of what it leaves out.
    fn tell_replaced_found(&mut self, warnings: &[Warning], jobs: &mut Vec<Job>) {
        let accounts = self
            .status
            .as_ref()
            .map_or(&[][..], |status| &status.accounts);
        let found = self.post_replaced(warnings, accounts, jobs);
        self.replaced_told.extend(found);
    }

    fn post_replaced(
        &self,
        warnings: &[Warning],
        accounts: &[Account],
        jobs: &mut Vec<Job>,
    ) -> BTreeSet<String> {
        let mut replaced = BTreeSet::new();
        for warning in warnings
            .iter()
            .filter(|warning| warning.code == "login_replaced")
        {
            let Some(account) = warning.account.as_deref() else {
                continue;
            };
            if replaced.insert(account.to_owned()) && !self.replaced_told.contains(account) {
                let again = accounts
                    .iter()
                    .find(|enrolled| enrolled.id == account)
                    .and_then(|enrolled| enrolled.key.as_ref());
                jobs.push(Job::Post {
                    notice: crate::present::standing_notice(self.front, warning, again),
                });
            }
        }
        replaced
    }

    /// Whether `read`, which asked nobody, says Claude Code's config has named another
    /// account since Anthropic last named the login stored, in words no read that asks has
    /// been started for yet. A sign-in leaves the config so, and a session's status line
    /// records numbers soon after one, so the read after them asks Anthropic whose the login
    /// is now, once for each thing the config is said to name.
    fn unconfirmed(&mut self, read: &Status) -> bool {
        // A read that asks settles nothing of a tool no service is asked about.
        let said = read
            .warnings
            .iter()
            .find(|warning| {
                warning.code == "in_use_unconfirmed" && !warning.provider.is_some_and(in_its_app)
            })
            .map(|warning| warning.message.clone());
        let new = said.is_some() && said != self.unconfirmed_asked;
        self.unconfirmed_asked = said;
        new
    }

    /// What is already known came in, or could not be read.
    fn known(&mut self, why: Why, read: Option<Status>, now: Now, jobs: &mut Vec<Job>) {
        match why {
            Why::InPlaceOf(ticket) => {
                // Only where there is still nothing to show: a read that has landed since
                // knows better.
                if self.status.is_none() {
                    self.standing_in = read.is_some();
                    self.status = read;
                }
                self.landed(ticket, now);
            }
            Why::Changed { measured } => {
                self.look_to_switch(jobs);
                if let Some(read) = read {
                    // A read that asks nobody knows no plan for a login nobody enrolled.
                    let read = match &self.status {
                        Some(shown) => {
                            answers_kept(plans_kept(refusals_kept(read, shown), shown), shown)
                        }
                        None => read,
                    };
                    // A read that asks nobody says a switch is stuck wherever that can be
                    // told without a request. Saying nothing, it cannot tell a switch given
                    // up on or finished elsewhere from one only a service could judge, so
                    // where one is offered the read that asks says which.
                    let waiting = read
                        .warnings
                        .iter()
                        .any(|w| w.code == "recovery_undetermined");
                    let unsaid = self.stuck && !waiting;
                    self.stuck |= waiting;
                    if self.unconfirmed(&read) || unsaid {
                        self.refresh(Asked::default(), now, jobs);
                    }
                    self.forget_switches_undone(&read);
                    self.advise(&read, jobs);
                    // What the account index says now, whether or not reads that ask a
                    // service fail meanwhile: `pitboard forget` in a terminal closes the
                    // account's window and deletes its store on a Mac that cannot reach one.
                    self.windows.read_enrolled(&read, jobs);
                    self.status = Some(read);
                    self.standing_in = false;
                    self.readings_at = Some(measured);
                }
                self.look_over(now);
            }
            Why::Numbers { measured } => {
                self.look_to_switch(jobs);
                if read.as_ref().is_some_and(|read| self.unconfirmed(read)) {
                    self.refresh(Asked::default(), now, jobs);
                }
                // Onto what is shown as the numbers land, which a read may have replaced
                // while they were read. The Swift put them onto what was shown when the
                // look found them, and so put back what that read had replaced.
                if let (Some(read), Some(shown)) = (read, self.status.as_ref()) {
                    let overlaid = numbers(&read, shown);
                    self.advise(&overlaid, jobs);
                    self.status = Some(overlaid);
                    self.standing_in = false;
                    self.readings_at = Some(measured);
                }
                self.look_over(now);
            }
        }
    }

    /// Has anything on this machine changed since the last look.
    ///
    /// Two things are looked at, because they mean different things. The account index
    /// changing can be a switch made somewhere else, so who is signed in is read again. The
    /// readings changing is only numbers, newer ones a session or the command line has seen,
    /// so only the numbers are taken: a reading moving says nothing about who is signed in.
    fn looked(&mut self, changed: i64, measured: i64, now: Now, jobs: &mut Vec<Job>) {
        // The first look only records where things stand; there is nothing to compare to.
        let (Some(seen), Some(seen_readings)) = (self.changed_at, self.readings_at) else {
            self.changed_at = Some(changed);
            self.readings_at = Some(measured);
            self.look_over(now);
            return;
        };
        // A switch this app has under way is its own change and not somebody else's, and
        // taking it for one put away what the switch had just said. So is any other change
        // this app has under way, until the read after it has landed: the look may have
        // found the index as the change wrote it, and taking that for a change made
        // elsewhere dropped the read the change asked for, as one that started before a
        // change. What changed meanwhile stays unseen until it is shown: a change that fails
        // reads nothing after it, and a switch that failed after finishing an interrupted
        // switch has still moved who is signed in.
        if self.holds_index() {
            self.unlooked = true;
            self.look_over(now);
            return;
        }
        self.changed_at = Some(changed);
        if seen != changed {
            self.changes_seen += 1;
            jobs.push(Job::ReadOffline {
                why: Why::Changed { measured },
            });
        } else if seen_readings != measured && self.status.is_some() {
            jobs.push(Job::ReadOffline {
                why: Why::Numbers { measured },
            });
        } else {
            self.look_over(now);
        }
    }

    // Once `to` is no longer in use somebody switched since.
    fn switch_back(&mut self, from: Key, to: &Key, jobs: &mut Vec<Job>) {
        let in_use = self.status.as_ref().is_some_and(|read| {
            (read.accounts.iter())
                .any(|account| account.signed_in && account.key.as_ref() == Some(to))
        });
        if !in_use || self.switch_under_way() {
            self.show(Some(Place::Accounts));
            return;
        }
        let until = (self.last_switches.iter())
            .filter_map(|last| last.back.as_ref())
            .find(|back| back.from == from)
            .and_then(|back| back.resets_at);
        self.switched_back = Some(SwitchedBack {
            from: from.clone(),
            until,
        });
        self.switch_asked(from, jobs);
    }

    /// A switch asked for from the window, the menu or a notification.
    ///
    /// Where an app runs the tool with its login in memory, as ChatGPT runs Codex, the switch
    /// waits for the person to let Pitboard quit it first: switched under it, the app would go
    /// on with the account left behind, and its own sign-out would revoke the login Pitboard
    /// has just parked.
    fn switch_asked(&mut self, account: Key, jobs: &mut Vec<Job>) {
        // One switch at a time: the second would wait behind the first anyway, and its
        // choice was made from a menu that did not yet show the first. A question waiting to
        // be answered comes back to the front rather than being left behind unseen.
        if self.switch_under_way() {
            if self.asking().is_some() || self.replace_asked().is_some() {
                self.show_over(Some(Place::Accounts));
            }
            return;
        }
        // Claimed before anything is asked, so a second request made meanwhile waits too,
        // and in place of a question closed unanswered, which no answer takes after this. A
        // sign-in inside the app waiting meanwhile stays until the switch goes on.
        self.drop_question();
        self.replacing = None;
        self.switching = Some(Switching::To(account.clone()));
        jobs.push(Job::Holding {
            account,
            then: After::Switch,
        });
    }

    /// The answer to the question a switch asked about replacing a login its service refuses
    /// for good: taken once, for the account it asked about, whether or not the app closed
    /// the question first.
    fn replace_and_switch(&mut self, account: Key, jobs: &mut Vec<Job>) {
        if self.switching.is_some()
            || !self
                .replacing
                .as_ref()
                .is_some_and(|replacing| replacing.account == account)
        {
            return;
        }
        self.replacing = None;
        self.switching = Some(Switching::To(account.clone()));
        jobs.push(Job::Replace { account });
    }

    fn held(
        &mut self,
        account: Key,
        question: Option<QuitQuestion>,
        then: After,
        jobs: &mut Vec<Job>,
    ) {
        match question {
            Some(question) => {
                if then == After::Switch {
                    // The app runs again, so a quit gives the copy to open again, if any.
                    self.reopen_after_switch = None;
                    match self.outside_switch {
                        Some(OutsideSwitch { quit: true, .. }) => {
                            jobs.push(Job::Quit { question, then });
                            return;
                        }
                        Some(OutsideSwitch { request, .. }) => {
                            self.switching = None;
                            self.outside_switch = None;
                            return self.answer_outside(request, Answered::AppOpen { question });
                        }
                        None => self.switching = None,
                    }
                }
                self.drop_question();
                self.quitting = Some(Quitting {
                    question,
                    asked: true,
                    then,
                });
                self.show_over(Some(Place::Accounts));
            }
            None => self.go_on(account, then, None, false, jobs),
        }
    }

    fn quit_and_switch(&mut self, account: &Key, jobs: &mut Vec<Job>) {
        let Some(Quitting { question, then, .. }) = self
            .quitting
            .take_if(|quitting| quitting.question.account == *account)
        else {
            return;
        };
        match &then {
            After::Switch => self.switching = Some(Switching::To(question.account.clone())),
            After::SignIn { tool, again, from } => {
                // One sign-in at a time: another started from a sheet put up once the question
                // was closed, which let this one's sheet go.
                if self.signing_in.is_some() {
                    return;
                }
                self.hold(from.as_ref());
                let mut signing = self.new_sign_in(*tool, again.clone());
                signing.quitting = true;
                self.signing_in = Some(signing);
            }
            After::Enrol { tool, from, .. } => {
                self.hold(from.as_ref());
                self.recording = Some((from.clone(), *tool, Recording::Quitting));
            }
            After::Uninstall => {
                if !self.may_uninstall() {
                    return;
                }
                self.end(true);
            }
        }
        jobs.push(Job::Quit { question, then });
    }

    // The sheet `from` holds its button back, where nothing holds it already.
    fn hold(&mut self, from: Option<&Sheet>) {
        if let Some(from) = from
            && !self.saving.contains(from)
        {
            self.saving.push(from.clone());
        }
    }

    fn quit_over(
        &mut self,
        question: QuitQuestion,
        outcome: QuitOutcome,
        then: After,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        let QuitQuestion {
            account, name: app, ..
        } = question;
        let cause = match &outcome {
            QuitOutcome::Lingering { .. } => Cause::PartStillRunning { app },
            _ => Cause::StillOpen { app },
        };
        let failed = |doing: Doing| Refused {
            doing,
            cause: cause.clone(),
            code: Some("app_open".into()),
            warnings: Vec::new(),
            account: Some(account.clone()),
        };
        let lingering = match outcome {
            QuitOutcome::Quit { copy } => {
                return self.go_on(account, then, Some(copy), true, jobs);
            }
            QuitOutcome::NotRunning => return self.go_on(account, then, None, true, jobs),
            QuitOutcome::StillRunning => None,
            QuitOutcome::Lingering { copy } => Some(copy),
        };
        match then {
            After::Switch => {
                self.switching = None;
                self.reopen_after_switch = None;
                let doing = Doing::Switch {
                    account: account.clone(),
                };
                match self.outside_switch.take() {
                    Some(OutsideSwitch { request, .. }) => {
                        self.answer_outside(request, Answered::Failed { doing });
                    }
                    None => self.present(failed(doing)),
                }
            }
            After::SignIn { again, from, .. } => {
                self.saved(from.as_ref());
                if let Some(signing) = self.signing_in.take_if(|signing| signing.quitting) {
                    self.sign_in_refused(&signing, failed(Doing::SignIn { again }), jobs);
                }
            }
            After::Enrol { again, from, .. } => {
                self.recording = None;
                self.saved(from.as_ref());
                self.save_failed(from.as_ref(), failed(Doing::SignIn { again }));
            }
            After::Uninstall => {
                self.resume(now, jobs);
                self.present(failed(Doing::Uninstall));
            }
        }
        // Here, once a switch this quit was for has let go: one asked meanwhile holds it back.
        if let Some(copy) = lingering {
            self.open_again(account.provider, copy, false, jobs);
        }
    }

    /// The switch is over: what it said is kept and the accounts are read, or what stopped
    /// it is said in the window. `Err(None)` is a switch that came to nothing.
    ///
    /// What the switch means for sessions already running depends on the tool. One that
    /// follows by itself gets when it will have; one that never does gets said so, since a
    /// countdown there would promise something that is not going to happen. One that follows
    /// at its login's next renewal gets neither: when that happens is up to the session, and
    /// the switch's warning about the file that holds it back says the rest.
    fn switched(
        &mut self,
        account: &Key,
        reopen: Option<String>,
        done: Result<Switched, Option<PitboardError>>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        // Pitboard closed it, so Pitboard opens it, whether or not the switch worked, where its
        // folder is whole: as soon as the switch is made, and not after the read that follows
        // it, which can wait on the network.
        let tool = account.provider;
        let kept = self
            .reopen_after_switch
            .take()
            .filter(|(of, _)| *of == tool)
            .map(|(_, copy)| copy);
        if let Some(location) = reopen.or(kept) {
            jobs.push(Job::Open {
                location,
                tool,
                front: false,
            });
        }
        let outside = self.outside_switch.take().map(|outside| outside.request);
        let label = account.label.clone();
        match done {
            Ok(done) => {
                if let Some(request) = outside {
                    self.answer_outside(request, Answered::Switched { tool, label });
                }
                self.refused_by.remove(&tool);
                self.changes_seen += 1;
                // Advice about this tool is about the account it has just left. Another
                // tool's stays: it is as true as it was, and it is never told again. Nothing
                // moved where the account was already in use, and advice about it stands
                // until a read no longer bears it out.
                if matches!(
                    done.outcome,
                    Switch::Switched { .. } | Switch::Installed { .. }
                ) {
                    self.advice.retain(|advice| advice.provider != tool);
                }
                self.said(account, done, now);
                self.updated_ms = None;
                self.refresh(
                    Asked {
                        after_switch: true,
                        ..Asked::default()
                    },
                    now,
                    jobs,
                );
            }
            Err(Some(PitboardError::Failed {
                code,
                message,
                warnings,
                ..
            })) if code == "live_login_refused" => {
                self.switching = None;
                self.keep(&warnings, jobs);
                if let Some(request) = outside {
                    return self.answer_outside(request, Answered::Replace { label });
                }
                self.replacing = Some(Replacing {
                    account: account.clone(),
                    message,
                    asked: true,
                });
                self.show_over(Some(Place::Accounts));
            }
            Err(error) => {
                self.switching = None;
                let doing = Doing::Switch {
                    account: account.clone(),
                };
                let refused = match error {
                    Some(error) => Refused::of(doing, error),
                    None => Refused::lost(doing),
                };
                self.refused(tool, &refused);
                // Nothing moved, so what the last switch said stands too.
                self.keep(&refused.warnings, jobs);
                match outside {
                    Some(request) => {
                        let doing = refused.doing;
                        self.answer_outside(request, Answered::Failed { doing });
                    }
                    None => self.present(refused),
                }
            }
        }
    }

    // Kept for the next add of `tool`'s account, where it signs in inside its app.
    fn refused(&mut self, tool: ProviderId, refused: &Refused) {
        if in_its_app(tool)
            && let Some(code) = &refused.code
        {
            self.refused_by.insert(tool, code.clone());
        }
    }

    /// Keeps what a switch that worked said, in place of what its own tool's last switch
    /// said and beside what every other tool's did.
    fn said(&mut self, account: &Key, done: Switched, now: Now) {
        // With no account before it where the tool held nobody's login.
        let (provider, from, to, adoption) = match done.outcome {
            Switch::Switched {
                provider,
                from,
                to,
                adoption,
            } => (provider, Some(from), to, adoption),
            Switch::Installed {
                provider,
                to,
                adoption,
            } => (provider, None, to, adoption),
            Switch::AlreadyActive { account: to } => {
                // Nothing moved, so what this tool's last switch said still stands, and
                // anything this one warned about is said beside it.
                if done.warnings.is_empty() {
                    return;
                }
                let provider = account.provider;
                let mut said = self
                    .last_switches
                    .iter()
                    .find(|last| last.provider == provider && last.to == to)
                    .cloned()
                    .unwrap_or_else(|| LastSwitch {
                        provider,
                        to,
                        follows_at: None,
                        restart: None,
                        said: None,
                        warnings: Vec::new(),
                        opens: None,
                        back: None,
                    });
                for warning in done.warnings {
                    if !said.warnings.contains(&warning) {
                        said.warnings.push(warning);
                    }
                }
                self.remember(said);
                return;
            }
        };
        let (follows_at, restart, opens) = match adoption {
            Adoption::PollingWithin(within_seconds) => {
                (Some(now.epoch() + i64::from(within_seconds)), None, None)
            }
            Adoption::AtRenewal { .. } => (None, None, None),
            Adoption::NextOpen { app } => (None, None, Some(Opens::Switched { app })),
            Adoption::RestartRequired { program, .. } => (
                None,
                from.map(|from| RestartNeeded {
                    program: program.to_owned(),
                    from,
                }),
                None,
            ),
        };
        self.remember(LastSwitch {
            provider,
            to,
            follows_at,
            restart,
            said: None,
            warnings: done.warnings,
            opens,
            back: None,
        });
    }

    /// In place of what the same tool's last switch said, or after the others.
    fn remember(&mut self, said: LastSwitch) {
        match self
            .last_switches
            .iter_mut()
            .find(|last| last.provider == said.provider)
        {
            Some(last) => *last = said,
            None => self.last_switches.push(said),
        }
    }

    /// Puts away what a switch said once its tool no longer has the account it switched to
    /// signed in: a switch made somewhere else, or a sign-out. Anything else that writes the
    /// account index leaves the sessions it describes exactly as they were, and taking every
    /// write for a switch put away the one warning that keeps somebody from revoking the
    /// login a Codex switch had just parked.
    fn forget_switches_undone(&mut self, read: &Status) {
        self.last_switches.retain(|last| {
            read.accounts.iter().any(|account| {
                account.provider == last.provider
                    && account.signed_in
                    && account.key.as_ref() == Some(&last.to)
            })
        });
    }

    // Only the sheet it was asked from closes: another put up meanwhile is somebody else's.
    // `Err(None)` is a save that came to nothing.
    fn enrolled_now(
        &mut self,
        enrolling: &Enrolling,
        reopen: Option<String>,
        done: Result<Enrolled, Option<PitboardError>>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        let Enrolling { tool, again, from } = *enrolling;
        self.saved(from);
        if self
            .recording
            .as_ref()
            .is_some_and(|(recording, ..)| recording.as_ref() == from)
        {
            self.recording = None;
        }
        if let Some(location) = reopen {
            self.open_again(tool, location, false, jobs);
        }
        match done {
            Ok(done) => {
                self.changes_seen += 1;
                self.refused_by.remove(&tool);
                let its_own = from.is_some() && self.sheet.as_ref() == from;
                if its_own {
                    self.sheet = None;
                    self.sheet_failure = None;
                }
                if its_own || from.is_none() {
                    self.name_next(tool, &done, jobs);
                }
                self.updated_ms = None;
                self.read_after_change(now, jobs);
            }
            Err(error) => {
                self.change_over();
                let doing = Doing::SignIn {
                    again: again.map(str::to_owned),
                };
                let refused = match error {
                    Some(error) => Refused::of(doing, error),
                    None => Refused::lost(doing),
                };
                self.refused(tool, &refused);
                let none_stored = refused.code.as_deref() == Some("live_credential_absent");
                if from.is_none() && none_stored {
                    let add = Sheet::Add {
                        tool: Some(tool.into()),
                    };
                    self.present_sheet(add, jobs);
                    return;
                }
                self.save_failed(from, refused);
            }
        }
    }

    // Where no other sheet is up, which would be somebody else's.
    fn name_next(&mut self, tool: ProviderId, done: &Enrolled, jobs: &mut Vec<Job>) {
        if done.added && self.sheet.is_none() {
            let name = Sheet::Name {
                account: Key::new(tool, &done.label).into(),
                email: done.email.clone(),
            };
            self.present_sheet(name, jobs);
        }
    }

    /// An account has been renamed, or could not be. Only its own sheet closes, as
    /// AppModel.swift's `rename` closed it.
    ///
    /// Everything said about the account is said about it under its new name: what its tool's
    /// last switch said. Keyed by the old name, the read after the rename would take the
    /// switch for undone, and put away what it said.
    fn renamed(
        &mut self,
        renaming: &Renaming,
        done: Result<(), Option<PitboardError>>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        let Renaming { account, to, from } = *renaming;
        self.saved(from);
        match done {
            Ok(()) => {
                self.changes_seen += 1;
                self.carry(account, to, jobs);
                let own = |sheet: &Sheet| match sheet {
                    Sheet::Rename { account: of } | Sheet::Name { account: of, .. } => {
                        Key::from(of.clone()) == *account
                    }
                    _ => false,
                };
                if self.sheet.as_ref().is_some_and(own) {
                    self.sheet = None;
                    self.sheet_failure = None;
                }
                self.updated_ms = None;
                self.read_after_change(now, jobs);
            }
            Err(error) => {
                self.change_over();
                let doing = Doing::Rename {
                    account: account.clone(),
                };
                let refused = match error {
                    Some(error) => Refused::of(doing, error),
                    None => Refused::lost(doing),
                };
                self.save_failed(from, refused);
            }
        }
    }

    /// What was said about `account`, said about it as `to`: what its tool's last switch said,
    /// advice about it, and what was told about it.
    fn carry(&mut self, account: &Key, to: &str, jobs: &mut Vec<Job>) {
        let Key { provider, label } = account;
        for advice in &mut self.advice {
            if advice.provider == *provider {
                *advice = advice.renaming(label, to);
            }
        }
        rename_told(&mut self.told_this_launch, *provider, label, to);
        let before = self.told.clone();
        rename_told(&mut self.told, *provider, label, to);
        if self.told != before {
            jobs.push(Job::KeepTold {
                told: self.told.clone(),
            });
        }
        for last in &mut self.last_switches {
            if last.provider != *provider {
                continue;
            }
            if last.to == *account {
                last.to = Key::new(*provider, to);
            }
            if let Some(restart) = &mut last.restart
                && restart.from == *label
            {
                restart.from = to.to_owned();
            }
        }
    }

    /// `account` has been forgotten, and the login parked for it, or it could not be, which is
    /// said in the window it was asked from.
    fn forgot(
        &mut self,
        account: &Key,
        done: Result<(), Option<PitboardError>>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        let tool = account.provider;
        match done {
            Ok(()) => {
                self.changes_seen += 1;
                self.refused_by.remove(&tool);
                self.updated_ms = None;
                self.read_after_change(now, jobs);
            }
            Err(error) => {
                self.change_over();
                let doing = Doing::Remove {
                    account: account.clone(),
                };
                let refused = match error {
                    Some(error) => Refused::of(doing, error),
                    None => Refused::lost(doing),
                };
                self.refused(tool, &refused);
                self.present(refused);
            }
        }
    }

    /// Writing the account in use into its tool's config is over. Nothing moved, so what was
    /// said of the last switch and of the account in use stands. What it warned of, such as a
    /// config it could not write, is said beside the read after it, and what refused it in
    /// the window.
    fn config_updated(
        &mut self,
        done: Result<Vec<Warning>, Option<PitboardError>>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        match done {
            Ok(warned) => {
                self.changes_seen += 1;
                self.updated_ms = None;
                let saying = self.say_after_read(warned);
                self.refresh(
                    Asked {
                        after_change: true,
                        saying,
                        ..Asked::default()
                    },
                    now,
                    jobs,
                );
            }
            Err(error) => {
                self.change_over();
                self.present(match error {
                    Some(error) => Refused::of(Doing::UpdateConfig, error),
                    None => Refused::lost(Doing::UpdateConfig),
                });
            }
        }
    }

    /// Puts away the file the sheet for it looked at, as it found it, from that sheet, which
    /// holds back until it answers. Nothing is asked before the look has found a login it can
    /// put away, nor while one is being put away.
    fn stow(&mut self, jobs: &mut Vec<Job>) {
        if self.sheet != Some(Sheet::Stow) || self.saving.contains(&Sheet::Stow) {
            return;
        }
        let Some(Leftover::Found(Some(left))) = &self.left else {
            return;
        };
        if let Foreseen::NotEnrolled(_) = left.login {
            return;
        }
        let seen = left.seen.clone();
        self.sheet_failure = None;
        let from = self.saving_from();
        self.change(Job::Stow { seen, from }, jobs);
    }

    /// What the look at the file behind Claude Code's store found, taken in while its sheet
    /// is up, or why it could not look, said in that sheet. `Err(None)` is a look that came
    /// to nothing.
    fn looked_left(&mut self, found: Result<Option<Left>, Option<PitboardError>>) {
        if self.sheet != Some(Sheet::Stow) {
            return;
        }
        match found {
            Ok(found) => self.left = Some(Leftover::Found(found)),
            Err(error) => {
                self.left = Some(Leftover::Unknown);
                let refused = match error {
                    Some(error) => Refused::of(Doing::TellWhose, error),
                    None => Refused::lost(Doing::TellWhose),
                };
                self.sheet_failure = Some(self.number(refused));
            }
        }
    }

    /// Putting away the file behind Claude Code's store is over. Done, its sheet closes,
    /// what it did is said until somebody puts it away, what it warned of beside the read
    /// after it, and the accounts are read again. Refused, it is said in the sheet it was
    /// asked from, which looks at the file again, and in the window where that has gone.
    fn stowed(
        &mut self,
        from: Option<&Sheet>,
        done: Result<(Stowed, Vec<Warning>), Option<PitboardError>>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        self.saved(from);
        match done {
            Ok((stowed, warned)) => {
                self.changes_seen += 1;
                if self.sheet == Some(Sheet::Stow) {
                    self.sheet = None;
                    self.sheet_failure = None;
                    self.left = None;
                }
                self.stowed = Some(stowed);
                self.updated_ms = None;
                let saying = self.say_after_read(warned);
                self.refresh(
                    Asked {
                        after_change: true,
                        saying,
                        ..Asked::default()
                    },
                    now,
                    jobs,
                );
            }
            Err(error) => {
                self.change_over();
                let refused = match error {
                    Some(error) => Refused::of(Doing::CleanUp, error),
                    None => Refused::lost(Doing::CleanUp),
                };
                self.save_failed(from, refused);
                // What it found is out of date: the file may have changed, or been renewed
                // and written back, before it stopped, and Put Away would only send it again.
                if self.sheet == Some(Sheet::Stow) {
                    self.left = Some(Leftover::Looking);
                    jobs.push(Job::LookLeft);
                }
            }
        }
    }

    /// Giving up on an interrupted switch is over: what it kept is said and the accounts are
    /// read again, asking each service, or what stopped it is said in the window.
    fn abandon_over(
        &mut self,
        done: Result<GaveUp, Option<PitboardError>>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        match done {
            Ok(gave_up) => {
                self.abandoned = gave_up.switch;
                let settled = (gave_up.warnings.iter())
                    .filter(|warning| warning.code != "move_not_given_up")
                    .any(|warning| warning.provider.is_some_and(in_its_app));
                // A move given up or settled is no longer what refused the last change.
                if !gave_up.moves.is_empty() || settled {
                    self.refused_by.retain(|&tool, _| !in_its_app(tool));
                }
                self.moves_given_up = gave_up.moves;
                self.changes_seen += 1;
                self.stuck = false;
                let saying = self.say_after_read(gave_up.warnings);
                self.refresh(
                    Asked {
                        fresh: true,
                        after_change: true,
                        saying,
                        ..Asked::default()
                    },
                    now,
                    jobs,
                );
            }
            Err(error) => {
                self.change_over();
                self.present(match error {
                    Some(error) => Refused::of(Doing::CancelSwitch, error),
                    None => Refused::lost(Doing::CancelSwitch),
                });
            }
        }
    }

    /// Asks for `job`, a change of this app's own to the account index, which the change poll
    /// leaves alone until the change is over: until the read after it is over, or it has
    /// failed.
    fn change(&mut self, job: Job, jobs: &mut Vec<Job>) {
        self.changing += 1;
        jobs.push(job);
    }

    /// One of this app's own changes to the account index is over, and no longer holds it.
    fn change_over(&mut self) {
        self.changing = self.changing.saturating_sub(1);
    }

    /// Whether a change of this app's own holds the account index, which the poll leaves
    /// alone meanwhile.
    fn holds_index(&self) -> bool {
        self.switching.is_some() || self.changing > 0
    }

    /// Keeps what a change warned about to be said beside the read after it once that read is
    /// over, which would otherwise put it away: the number that read is asked with, where
    /// there is anything to say.
    fn say_after_read(&mut self, said: Vec<Warning>) -> Option<u64> {
        if said.is_empty() {
            return None;
        }
        self.sayings += 1;
        self.said_after_read.push((self.sayings, said));
        Some(self.sayings)
    }

    /// The read after one of this app's own changes, which ends that change once it is over.
    fn read_after_change(&mut self, now: Now, jobs: &mut Vec<Job>) {
        self.refresh(
            Asked {
                after_change: true,
                ..Asked::default()
            },
            now,
            jobs,
        );
    }

    /// Says a failure in the window.
    fn present(&mut self, refused: Refused) {
        self.presented = Some(self.number(refused));
        self.show_over(None);
    }

    /// A failure, numbered one higher than the last, wherever it is said.
    fn number(&mut self, refused: Refused) -> Failure {
        self.failures += 1;
        Failure {
            id: self.failures,
            doing: refused.doing,
            cause: refused.cause,
            code: refused.code,
            warnings: refused.warnings,
            account: refused.account,
        }
    }

    /// A failure's warnings said beside the ones already shown, first, with nothing the last
    /// read found dropped to make room for them, as AppModel.swift's `keep` said them.
    fn keep(&mut self, warned: &[Warning], jobs: &mut Vec<Job>) {
        self.tell_replaced_found(warned, jobs);
        let mut warnings = warned.to_vec();
        warnings.extend(
            self.warnings
                .drain(..)
                .filter(|warning| !warned.contains(warning)),
        );
        self.warnings = warnings;
    }

    fn show(&mut self, place: Option<Place>) {
        self.window = WindowRequest {
            serial: self.window.serial + 1,
            place,
        };
    }

    // Where a sheet, a question or a failure comes up: over the welcome while it is shown, where
    // the front's welcome hosts them.
    fn show_over(&mut self, place: Option<Place>) {
        let welcome =
            self.window.place == Some(Place::Welcome) && self.front.welcome_hosts_sheets();
        self.show(if welcome { Some(Place::Welcome) } else { place });
    }

    fn put_away(&mut self, tip: Tip, jobs: &mut Vec<Job>) {
        if self.preferences.decline(tip) && self.preferences_read {
            jobs.push(Job::KeepPreferences {
                preferences: self.preferences.clone(),
            });
        }
    }

    /// A look now, as the timer's would be, for a test that must not wait for the timer.
    #[cfg(test)]
    pub(super) fn look_now(&mut self) -> Vec<Job> {
        if self.look != Timer::Off {
            self.look = Timer::Running;
        }
        vec![Job::Look]
    }

    #[cfg(test)]
    pub(super) fn changes_seen(&self) -> u64 {
        self.changes_seen
    }
}

/// `read`, where a login nobody enrolled that it has no plan for keeps the one `shown` has.
fn plans_kept(mut read: Status, shown: &Status) -> Status {
    for account in read
        .accounts
        .iter_mut()
        .filter(|a| a.label.is_none() && a.plan.is_none())
    {
        account.plan = shown
            .accounts
            .iter()
            .find(|a| a.provider == account.provider && a.id == account.id)
            .and_then(|a| a.plan.clone());
    }
    read
}

// `read`, where a parked login `shown` says its service refused, and `read` did not ask about
// again, is still refused, and not switchable, as the core says of a refused one. Parked
// again, it is another login.
fn refusals_kept(mut read: Status, shown: &Status) -> Status {
    // what a read says of a login it did not ask its service about
    let unasked = |stale: Option<Stale>| {
        matches!(
            stale,
            Some(
                Stale::NotAsked
                    | Stale::AskedRecently
                    | Stale::Unreachable
                    | Stale::RateLimited
                    | Stale::ServerError
                    | Stale::Interrupted
                    | Stale::ParkedAccessExpired
            )
        )
    };
    for account in read
        .accounts
        .iter_mut()
        .filter(|account| !account.signed_in && unasked(account.stale))
    {
        if let Some(refused) = shown.accounts.iter().find(|refused| {
            refused.id == account.id
                && refused.parked == account.parked
                && refused.stale == Some(Stale::LoginRefused)
        }) {
            account.stale = refused.stale;
            account
                .stale_explanation
                .clone_from(&refused.stale_explanation);
            account.standing = refused.standing;
        }
    }
    read
}

// `read`, where an account in use or parked as `shown` has it keeps how fresh `shown` says its
// numbers are: the index a read writes as it records a login says nothing of them.
fn answers_kept(mut read: Status, shown: &Status) -> Status {
    for account in read
        .accounts
        .iter_mut()
        .filter(|account| account.stale == Some(Stale::NotAsked))
    {
        if let Some(asked) = shown.accounts.iter().find(|asked| {
            asked.id == account.id
                && asked.signed_in == account.signed_in
                && asked.parked == account.parked
                && matches!(asked.stale, None | Some(Stale::AskedRecently))
        }) {
            account.stale = asked.stale;
            account
                .stale_explanation
                .clone_from(&asked.stale_explanation);
            if account.usage.is_none() {
                account.usage.clone_from(&asked.usage);
            }
        }
    }
    read
}

/// `shown` with each account's numbers as `read` has them, and everything else as it was. An
/// account `read` has no numbers for keeps its own.
fn numbers(read: &Status, shown: &Status) -> Status {
    let mut measured: HashMap<&str, &Usage> = HashMap::new();
    for account in &read.accounts {
        if let Some(usage) = &account.usage {
            measured.entry(account.id.as_str()).or_insert(usage);
        }
    }
    Status {
        now: shown.now,
        accounts: shown
            .accounts
            .iter()
            .map(|account| match measured.get(account.id.as_str()) {
                Some(usage) => Account {
                    usage: Some((*usage).clone()),
                    ..account.clone()
                },
                None => account.clone(),
            })
            .collect(),
        warnings: shown.warnings.clone(),
        awaiting: shown.awaiting.clone(),
    }
}
