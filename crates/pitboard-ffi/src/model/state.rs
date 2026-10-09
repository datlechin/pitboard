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
//! Every other change this app makes to the account index holds it the same way, from the
//! moment it is asked for, a sign-in's from the moment it is told to enrol, until the read
//! after it is over or the change has failed. The Swift model held nothing for them, so a look
//! that found the index as one had just written it, and was compared once that change had
//! taken its read's ticket, counted the change as one made elsewhere and dropped that read.
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

use super::advice::{Advice, Told, rename_told};
use super::machine::{LOG_LIMIT, MachineState, Scheduled};
use super::preferences::Preferences;
use super::windows::Windows;
use super::{Failure, Intent, LastSwitch, Pane, QuitQuestion, ReadFailure, RestartNeeded};
use super::{RunOutNotice, RunningSignIn, Sheet, WindowRequest};
use crate::account_windows::records::{Entry, Loaded};
use crate::{
    Abandoned, Account, Adoption, AutoLooked, AutoSwitched, Change, Check, Enrolled, EnrolledAs,
    FoundCommandLine, OwnCommandLine, PitboardError, Renewed, Schedule, Status, Switch, Switched,
    Tool, Usage, Warning,
};
use pitboard_core::autoswitch::{self, Threshold};
use pitboard_core::label::SEPARATOR;
use pitboard_core::provider::{self, ProviderId};
use pitboard_core::usage::same_reset;
use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

/// How often the model does what it does by itself, and how long it waits for an app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cadence {
    /// How often it looks whether anything on this machine has changed: one look at when
    /// Pitboard's account index and its usage readings were last written, which costs no
    /// network and no keychain. Before it, a switch typed in a terminal left the menu bar
    /// naming the account the person had just stopped using for as long as five minutes.
    pub(crate) look_every: Duration,
    /// How often it reads every account by itself. Usage is asked of each tool's service
    /// for every account, so it is asked sparingly.
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
    /// progress to be answered; past it, nothing has changed and the switch is not made.
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
        look_every: Duration::from_secs(2),
        read_every: Duration::from_secs(300),
        decide_every: Duration::from_secs(autoswitch::DECIDE_EVERY_SECONDS.unsigned_abs()),
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
    /// Which tools' programs were found. Its own lane, so it never waits behind a read on
    /// the network, and a read can wait for it.
    AskInstalled,
    /// Whether a running app holds the login of the tool `qualified` is of, which the core
    /// answers from the process list and the app from what is running.
    Holding { qualified: String },
    /// Ask the app the question names to quit, and wait until it has, for as long as the
    /// cadence gives it.
    Quit { question: QuitQuestion },
    /// The switch itself. `reopen` is where the app Pitboard quit for it was opened from,
    /// which is opened again once the switch is over, whether or not it worked.
    Switch {
        qualified: String,
        reopen: Option<String>,
    },
    /// Ask the core's look whether to switch Claude Code by itself at `at`%, from its files
    /// alone: what stands, or that only a decision under its lock can say. It claims nothing.
    AutoLook { at: u8 },
    /// Switch Claude Code by itself where a limit of the account in use has reached `at`%
    /// and another account has room: asked once the look says only a decision under the
    /// core's lock can say, which decides whether, and to which, from whose login Claude Code
    /// has stored.
    AutoSwitch { at: u8 },
    /// Open the app at `location` again.
    Open { location: String },
    /// Give up on an interrupted switch, keeping every login it names.
    Abandon,
    /// A sign-in, on a thread of its own: start the tool's own sign-in for the account
    /// `qualified` names, its name with its tool, hand on everything the tool says until it
    /// stops, then wait for `SignInOver`.
    SignIn { id: u64, qualified: String },
    /// Type `code` back to the tool of the sign-in `id`.
    PasteCode { id: u64, code: String },
    /// Stop the tool of the sign-in `id`, and say whether it did. Once the tool has stopped
    /// saying anything this stops nothing, since what it signed in to may be being enrolled.
    StopSignIn { id: u64 },
    /// The tool of the sign-in `id` has stopped saying anything: enrol what it signed in to,
    /// or let it go.
    SignInOver { id: u64, enrol: bool },
    /// Enrol the login signed in now to `provider`'s tool as `name`, with no browser. `from`
    /// is the sheet that was up when it was asked for, where what goes wrong is said.
    Enrol {
        provider: String,
        name: String,
        from: Option<Sheet>,
    },
    /// Give the account `label` of `provider`'s tool the name `to`, inside its own tool.
    Rename {
        provider: String,
        label: String,
        to: String,
        from: Option<Sheet>,
    },
    /// Drop the account `qualified` names, and the login parked for it.
    Forget { qualified: String },
    /// Write the account `qualified` names into its tool's config, while it is in use.
    UpdateConfig { qualified: String },
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
    Post { notice: RunOutNotice },
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
    /// Renew every parked login that is due.
    Renew,
    /// Make every check `pitboard doctor` makes.
    Check,
    /// Read the newest `limit` changes Pitboard made.
    ReadLog { limit: u32 },
    /// Look for the `pitboard` a terminal would run.
    FindCommandLine,
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
    /// Whether it is the read after a renewal, whose landing ends the renewal.
    after_renewal: bool,
    /// Whether it is the read after a change of this app's own to the account index other
    /// than a switch, whose landing ends that change's hold on the index.
    after_change: bool,
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
}

/// What a job came to.
#[derive(Debug)]
pub(crate) enum Answer {
    Read {
        ticket: Ticket,
        /// When the usage readings were last written, in epoch milliseconds, before the read.
        readings_before: i64,
        /// When the account index was last written, in epoch seconds, before the read.
        changed_before: i64,
        read: Result<Status, PitboardError>,
    },
    ReadOffline {
        why: Why,
        read: Result<Status, PitboardError>,
    },
    Looked {
        /// When the account index was last written, in epoch seconds.
        changed: i64,
        /// When the usage readings were last written, in epoch milliseconds.
        measured: i64,
    },
    Installed {
        tools: Vec<Tool>,
    },
    /// The app holding the login, where one is running, as the question to ask about it.
    Held {
        qualified: String,
        question: Option<QuitQuestion>,
    },
    Quit {
        question: QuitQuestion,
        outcome: QuitOutcome,
    },
    Switched {
        qualified: String,
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
    /// The app was opened again, or could not be, which nothing here can mend.
    Opened,
    Abandoned(Result<Option<Abandoned>, PitboardError>),
    /// The tool of the sign-in `id` has started, or could not be.
    SignInStarted {
        id: u64,
        started: Result<(), PitboardError>,
    },
    /// Something more the tool of the sign-in `id` said, as it came.
    SignInSaid {
        id: u64,
        text: String,
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
        provider: String,
        from: Option<Sheet>,
        done: Result<Enrolled, PitboardError>,
    },
    /// What renaming the account `label` of `provider`'s tool to `to` came to.
    Renamed {
        provider: String,
        label: String,
        to: String,
        from: Option<Sheet>,
        done: Result<(), PitboardError>,
    },
    /// What forgetting the account `qualified` names came to.
    Forgot {
        qualified: String,
        done: Result<(), PitboardError>,
    },
    /// What writing the account in use into its tool's config came to, with what its tool
    /// warned of.
    ConfigUpdated(Result<Vec<Warning>, PitboardError>),
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
    /// A notification was posted, or could not be, which is the app's system's to say: the
    /// window says the same either way.
    Posted,
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
    /// What renewing every parked login that was due came to, or why it was refused.
    Renewed {
        renewals: Result<Vec<Renewed>, PitboardError>,
    },
    /// Doctor's checks, or `None` where this build makes none.
    Checked {
        checks: Option<Vec<Check>>,
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
    /// The job stopped with a panic, or a sign-in's thread could not be started, and it came
    /// to nothing. The lane goes on, and the model goes on as though the job had answered
    /// with nothing new.
    Lost(Job),
}

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
    /// Whether a renewal asked, which is over once this read is.
    after_renewal: bool,
    /// Whether a change of this app's own to the account index asked, other than a switch,
    /// which holds the index until this read is over.
    after_change: bool,
}

/// What a person asked for that did not happen, before it is numbered.
struct Refused {
    title: String,
    message: String,
    code: Option<String>,
    warnings: Vec<Warning>,
}

impl Refused {
    /// `error`, said under `title`.
    fn of(title: String, error: PitboardError) -> Refused {
        let PitboardError::Failed {
            code,
            message,
            warnings,
            ..
        } = error;
        Refused {
            title,
            message,
            code: Some(code),
            warnings,
        }
    }

    /// A job that came to nothing, under `title`: what it did is not known.
    fn lost(title: String) -> Refused {
        Refused {
            title,
            message: "Pitboard stopped before it could say how this went. Refresh to see \
                      where things stand."
                .into(),
            code: None,
            warnings: Vec::new(),
        }
    }
}

/// "Couldn’t switch to personal", for the account `qualified` names.
fn could_not_switch(qualified: &str) -> String {
    format!("Couldn’t switch to {}", split(qualified).1)
}

const COULD_NOT_GIVE_UP: &str = "Couldn’t give up on the interrupted switch";

/// "Couldn’t sign in to work", for a sign-in of the account to be called `name`.
fn could_not_sign_in(name: &str) -> String {
    format!("Couldn’t sign in to {name}")
}

/// "Couldn’t forget work", for the account `qualified` names.
fn could_not_forget(qualified: &str) -> String {
    format!("Couldn’t forget {}", split(qualified).1)
}

/// "Couldn’t rename work", for the account `label`.
fn could_not_rename(label: &str) -> String {
    format!("Couldn’t rename {label}")
}

const COULD_NOT_UPDATE_CONFIG: &str = "Couldn’t update Claude Code’s config";

const COULD_NOT_NAME: &str = "Couldn’t name this account";

/// What a sign-in that put the new login of the account `name` in use says it did: signed in
/// to `again` where the account was enrolled already, and enrolled by it where it was not.
fn signed_in_now(name: &str, again: bool) -> String {
    if again {
        format!("Signed in to {name} again. Its new login is the one in use now.")
    } else {
        format!("Enrolled {name}, the account signed in now. Its new login is the one in use.")
    }
}

/// `typed` without the white space around it, as the Swift sheets took it away with
/// Foundation's `whitespacesAndNewlines`, by the rule a link from outside is trimmed by.
pub(crate) fn trimmed(typed: &str) -> &str {
    pitboard_sites::trimmed(typed)
}

/// The account `name` of `provider`'s tool as the core types it: bare for Claude Code, and
/// with its tool for any other.
fn typed_as_core(provider: &str, name: &str) -> String {
    if provider == ProviderId::Claude.code() {
        name.to_owned()
    } else {
        format!("{provider}{SEPARATOR}{name}")
    }
}

/// A label as the core types it, taken apart: `codex/work` is Codex's `work`, and a bare
/// `work` is Claude Code's, which is what a bare label has always meant.
/// What `switching` holds while Claude Code is switched by itself, which names no account:
/// the core chooses which.
const AUTOMATICALLY: &str = "claude";

/// An account's name with its tool, as `Account::qualified` gives it, from the name a switch
/// said it by, which names its tool only where it is not Claude Code or another tool shares
/// the name.
fn qualified_claude(said: &str) -> String {
    let (tool, label) = split(said);
    format!("{tool}{SEPARATOR}{label}")
}

pub(crate) fn split(typed: &str) -> (&str, &str) {
    typed
        .split_once(SEPARATOR)
        .unwrap_or((ProviderId::Claude.code(), typed))
}

/// An account as somebody types it at the command line, which is how the core names the
/// account a switch went to: bare for Claude Code, and with its tool for any other.
fn typed(account: &Account) -> Option<&str> {
    if account.provider == ProviderId::Claude.code() {
        account.label.as_deref()
    } else {
        account.qualified.as_deref()
    }
}

/// A rename, as its answer is taken in.
#[derive(Clone, Copy)]
struct Renaming<'a> {
    provider: &'a str,
    label: &'a str,
    to: &'a str,
    /// The sheet it was asked from, where what goes wrong is said.
    from: Option<&'a Sheet>,
}

/// The quit question last asked. A question closed unanswered is kept, unasked, because an
/// app can close it before it sends the answer that closed it: MainWindow.swift's alert
/// closes it from its binding and answers from its button on a task of its own, which
/// AppModel.swift's quitAndSwitch says runs once the alert has closed it.
#[derive(Debug)]
struct Quitting {
    question: QuitQuestion,
    /// Whether it is still asked, and so shown and holding every other switch back.
    asked: bool,
}

/// A sign-in under way, as the Swift model's `SigningIn` held it.
///
/// The Swift kept the sheet it was started from, to close only that one once it finished and
/// say a failure only in that one. Here a sign-in under way keeps its sheet, so the sheet up
/// while it runs is always that one, or none once that one has been closed.
#[derive(Debug)]
struct SigningIn {
    id: u64,
    /// The tool, as a `Tool`'s code.
    provider: String,
    /// The name to enrol it under, bare.
    name: String,
    /// Everything the tool has said so far.
    said: String,
    /// How much of `said` the tool had said when a code was last typed back, if one was.
    /// What it says after that is what says whether it took the code.
    pasted_at: Option<usize>,
    /// Whether the tool has started, so that a cancel has a tool to stop. One cancelled
    /// before is stopped as it starts.
    started: bool,
    /// Whether its thread has been asked for. Not while a sign-in cancelled before it may
    /// still hold the core's one sign-in at a time.
    asked: bool,
}

impl SigningIn {
    /// What it is shown as, read by its tool's own module. A code the tool refused since it
    /// was typed back is no longer one typed back: the tool reads another, so the field is
    /// offered again.
    fn shown(&self) -> RunningSignIn {
        let (view, code_refused) = match ProviderId::parse(&self.provider) {
            Some(tool) => {
                let refused = self.pasted_at.is_some_and(|at| {
                    provider::refused_code(tool, self.said.get(at..).unwrap_or_default())
                });
                let pasted = self.pasted_at.is_some() && !refused;
                (provider::sign_in_view(tool, &self.said, pasted), refused)
            }
            None => (provider::SignInView::default(), false),
        };
        RunningSignIn {
            id: self.id,
            provider: self.provider.clone(),
            name: self.name.clone(),
            said: self.said.clone(),
            url: view.url,
            wants_code: view.wants_code,
            code_refused,
        }
    }
}

/// Everything the model knows. The actor thread owns it, and nothing else touches it.
#[derive(Debug)]
pub(crate) struct State {
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
    pub(crate) installed: Option<Vec<Tool>>,
    /// Whether that is being asked now. A read asked for meanwhile waits for the same answer
    /// rather than asking again.
    asking_installed: bool,
    /// Reads waiting for that answer, which go ahead once it comes.
    waiting: Vec<Asked>,
    /// When the account index was last written, as last seen. `None` until something has
    /// looked: a machine with no index reports 0, which is a real answer.
    changed_at: Option<i64>,
    /// The same for the usage readings, which every session's status line records into.
    readings_at: Option<i64>,
    look: Timer,
    timed_read: Timer,
    /// The core's look at whether to switch Claude Code by itself, `decide_every` after the
    /// last one ended, while switching by itself is on.
    decide: Timer,
    /// The account a switch is running for, as its label with its tool: from the moment it
    /// is asked for, through asking what holds its tool's login, quitting an app and the
    /// switch itself, until the read after it lands.
    switching: Option<String>,
    /// How many of this app's other changes to the account index are under way: naming the
    /// login signed in now, a sign-in's enrolment, a rename, forgetting, writing the account
    /// in use into its tool's config, giving up on an interrupted switch and renewing parked
    /// logins. Each holds the index from the moment it is asked for, a sign-in's from the
    /// moment it is told to enrol, until the read after it is over, or until it has failed.
    /// Meanwhile the change poll leaves the index alone, as it does while a switch runs: what
    /// it finds there may be the change's own, which taken for one made elsewhere dropped the
    /// read the change asked for.
    changing: u32,
    /// A switch waiting for the person to let Pitboard quit an app first: the question last
    /// asked, while it is asked and after it is closed unanswered, until another switch is
    /// asked for.
    quitting: Option<Quitting>,
    /// What each tool's last switch said that is still true, one per tool at most.
    pub(crate) last_switches: Vec<LastSwitch>,
    /// What giving up on an interrupted switch kept, until somebody has read it.
    pub(crate) abandoned: Option<Abandoned>,
    /// The last thing asked for that did not happen.
    pub(crate) presented: Option<Failure>,
    /// How many failures have been said, which numbers the next.
    failures: u64,
    /// The requests for the main window.
    pub(crate) window: WindowRequest,
    /// The sign-in under way, from the moment it is asked for until it has finished, failed
    /// or been cancelled.
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
    /// Since a switch by itself was last refused: until when, since the model started, the
    /// next is not asked for, and how many were refused in a row. As long as the core waits
    /// after attempts that failed in a row, since a refusal raised before the core could
    /// record it is a line in the activity log each time. Any outcome, of a look or of a
    /// decision, ends the row.
    auto_wait: Option<(Duration, u32)>,
    /// The app's own preferences, kept in Pitboard's directory.
    pub(crate) preferences: Preferences,
    /// Whether the preferences have been read. Until they are, and for good where they
    /// could not be, nothing is written over what may be in their file.
    preferences_read: bool,
    /// Whether what the model keeps is being read, from the moment it is started until it
    /// is in. Nothing is advised on meanwhile, and what is shown is advised on once it is in,
    /// so a run-out notified before a relaunch is never notified again because the read
    /// after it landed first.
    loading_kept: bool,
    /// Whether what is shown is what stands in for a read that failed before anything was
    /// shown, the last numbers measured, which nothing has advised on: as AppModel.swift's
    /// fallback, it is not advised on once what is kept is in either.
    standing_in: bool,
    /// What the model knows of this machine rather than its accounts.
    pub(crate) machine: MachineState,
    /// The account windows' bookkeeping.
    pub(crate) windows: Windows,
}

impl State {
    pub(crate) fn new(cadence: Cadence) -> State {
        State {
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
            waiting: Vec::new(),
            changed_at: None,
            readings_at: None,
            look: Timer::Off,
            timed_read: Timer::Off,
            decide: Timer::Off,
            switching: None,
            changing: 0,
            quitting: None,
            last_switches: Vec::new(),
            abandoned: None,
            presented: None,
            failures: 0,
            window: WindowRequest {
                serial: 0,
                pane: None,
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
            replaced_told: BTreeSet::new(),
            unconfirmed_asked: None,
            auto_standing: None,
            auto_wait: None,
            told: Told::new(),
            preferences: Preferences::default(),
            preferences_read: false,
            loading_kept: false,
            standing_in: false,
            machine: MachineState::default(),
            windows: Windows::new(cadence.arming),
        }
    }

    /// Takes in `msg`, which arrived at `now`, and says what to run. Any timer due by `now`
    /// goes off too, whatever the message, so a stream of messages cannot hold one back.
    pub(crate) fn apply(&mut self, msg: Msg, now: Now) -> Vec<Job> {
        let mut jobs = Vec::new();
        match msg {
            Msg::Intent(intent) => self.intent(intent, now, &mut jobs),
            Msg::Done(answer) => self.answer(answer, now, &mut jobs),
            Msg::Tick | Msg::Stop => {}
        }
        self.go_off(now, &mut jobs);
        self.follow_windows(now, &mut jobs);
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
        [self.look, self.timed_read, self.tick, self.decide]
            .into_iter()
            .filter_map(|timer| match timer {
                Timer::Due(at) => Some(at),
                Timer::Off | Timer::Running => None,
            })
            .chain(self.windows.next_due())
            .min()
    }

    /// Whether a sign-in is under way.
    pub(crate) fn sign_in_under_way(&self) -> bool {
        self.signing_in.is_some()
    }

    /// The sign-in under way as an app is shown it, read by its tool's own module.
    pub(crate) fn shown_sign_in(&self) -> Option<RunningSignIn> {
        self.signing_in.as_ref().map(SigningIn::shown)
    }

    /// The account a switch is running for, or waiting on the quit question for.
    pub(crate) fn switch_under_way(&self) -> Option<&str> {
        self.switching
            .as_deref()
            .or(self.asking().map(|question| question.qualified.as_str()))
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
        self.quitting
            .as_ref()
            .filter(|quitting| quitting.asked)
            .map(|quitting| &quitting.question)
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
            Intent::Refresh { asked } => self.refresh(
                Asked {
                    fresh: asked,
                    ..Asked::default()
                },
                now,
                jobs,
            ),
            Intent::SwitchTo { qualified } => self.switch_asked(qualified, jobs),
            Intent::QuitAndSwitch { qualified } => self.quit_and_switch(&qualified, jobs),
            // The question is gone, and nothing was switched. It is kept, unasked, for the
            // answer an app sends after closing it.
            Intent::KeepAppOpen => {
                if let Some(quitting) = &mut self.quitting {
                    quitting.asked = false;
                }
            }
            Intent::DismissSwitch { provider } => {
                self.last_switches.retain(|last| last.provider != provider);
            }
            Intent::DeclineSecondAccount { provider } => {
                // Kept once the preferences are read, with whatever they hold, and never over
                // a file that could not be read: said meanwhile, it holds until the app quits.
                if self.preferences.second_account_declined.insert(provider)
                    && self.preferences_read
                {
                    jobs.push(Job::KeepPreferences {
                        preferences: self.preferences.clone(),
                    });
                }
            }
            Intent::SetAutoSwitch { on, at } => self.auto_switch_set(on, at, jobs),
            Intent::AbandonStuckSwitch => self.change(Job::Abandon, jobs),
            Intent::DismissAbandoned => self.abandoned = None,
            Intent::SignIn { provider, name } => self.sign_in(provider, &name, jobs),
            Intent::PasteCode { code } => self.paste(&code, jobs),
            Intent::CancelSignIn => {
                // Before its tool has started there is nothing to stop yet: it is stopped as
                // it starts.
                if let Some(signing) = self.signing_in.take()
                    && signing.started
                {
                    jobs.push(Job::StopSignIn { id: signing.id });
                }
            }
            Intent::PresentSheet { sheet } => self.present_sheet(sheet, jobs),
            Intent::CloseSheet => {
                self.sheet = None;
                self.sheet_failure = None;
            }
            Intent::ShowWindow { pane } => self.show_window(pane),
            Intent::DismissFailure => self.presented = None,
            Intent::Enrol { provider, name } => {
                let kind = Sheet::Name {
                    provider: provider.clone(),
                    email: String::new(),
                };
                if let Some(name) = self.saved_from(kind, &name) {
                    let from = self.saving_from();
                    let enrol = Job::Enrol {
                        provider,
                        name,
                        from,
                    };
                    self.change(enrol, jobs);
                }
            }
            Intent::Rename {
                provider,
                label,
                to,
            } => {
                let kind = Sheet::Rename {
                    provider: provider.clone(),
                    label: label.clone(),
                };
                if let Some(to) = self.saved_from(kind, &to) {
                    let from = self.saving_from();
                    let rename = Job::Rename {
                        provider,
                        label,
                        to,
                        from,
                    };
                    self.change(rename, jobs);
                }
            }
            Intent::Forget { qualified } => self.change(Job::Forget { qualified }, jobs),
            Intent::UpdateConfig { qualified } => {
                self.change(Job::UpdateConfig { qualified }, jobs);
            }
            Intent::PaneShown { pane } => self.pane_shown(pane, now, jobs),
            Intent::ReadSchedule => jobs.push(Job::ReadSchedule {
                after_change: false,
            }),
            Intent::SetSchedule { on } => {
                if self.machine.schedule(on) {
                    jobs.push(Job::SetSchedule { on });
                }
            }
            // Its button holds back while one runs, as the Swift settings' did.
            Intent::RenewNow => {
                if !self.machine.renewing {
                    self.machine.renewing = true;
                    self.change(Job::Renew, jobs);
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
            Intent::StoreHeld { store } => self.windows.store_held(&store),
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
        }
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
            Pane::Machine => {
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
        crate::present::name_to_save(kind, typed.to_owned())
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

    /// A sign-in of `provider`'s tool, for the account to be called `name`, as the Swift
    /// model's `signIn` started one. One at a time: a second would take the place of the one
    /// on screen and leave the first tool waiting on a browser with nothing to stop it.
    fn sign_in(&mut self, provider: String, name: &str, jobs: &mut Vec<Job>) {
        let name = trimmed(name);
        if self.signing_in.is_some() || name.is_empty() {
            return;
        }
        self.sign_ins += 1;
        let id = self.sign_ins;
        // Started from the sheet that is up, which says nothing more of what went wrong the
        // last time.
        self.sheet_failure = None;
        self.signing_in = Some(SigningIn {
            id,
            provider,
            name: name.to_owned(),
            said: String::new(),
            pasted_at: None,
            started: false,
            asked: false,
        });
        self.ask_for_sign_in(jobs);
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
        let Some(signing) = self.signing_in.as_mut().filter(|signing| !signing.asked) else {
            return;
        };
        signing.asked = true;
        self.sign_in_holding = Some(signing.id);
        jobs.push(Job::SignIn {
            id: signing.id,
            qualified: format!("{}{SEPARATOR}{}", signing.provider, signing.name),
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

    /// Puts `sheet` over the main window, opening it on the accounts the sheet is about.
    fn present_sheet(&mut self, sheet: Sheet, jobs: &mut Vec<Job>) {
        // A sign-in under way keeps its sheet: replacing it would leave the tool's sign-in
        // running with nothing on screen to finish or stop it.
        if self.signing_in.is_none() {
            // The first answer to what is installed may have come while the person's login
            // shell was too slow to say, and the core asks it once more when that was so.
            if self.sheet.is_none() && matches!(sheet, Sheet::Add { .. }) {
                self.ask_installed(jobs);
            }
            if self.sheet.as_ref() != Some(&sheet) {
                self.sheet_failure = None;
            }
            self.sheet = Some(sheet);
        }
        self.show_window(Some(Pane::Accounts));
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
    /// minutes, and a look for changes made somewhere else now and every few seconds. Once: a
    /// second start starts nothing.
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
        if matches!(self.look, Timer::Due(at) if at <= now.running) {
            self.look = Timer::Running;
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
    /// once. The first time the app has ever been opened, the window opens, once. Where they
    /// could not be read, `None`, nothing is kept in place of what may be there, then or
    /// later, and the window does not open as though for the first time.
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
                self.show_window(None);
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
    /// What is new this launch is notified only where no launch has notified it at the same
    /// reset, by the core's rule for one reset, and the record of that is kept.
    fn advise(&mut self, read: &Status, jobs: &mut Vec<Job>) {
        if self.loading_kept {
            return;
        }
        let tools = crate::tools();
        let new = Advice::about(read, &tools, &self.told_this_launch);
        let mut kept = false;
        for advice in &new {
            let (key, at) = (advice.key_of(), advice.window.resets_at.unwrap_or(0));
            self.told_this_launch.insert(key.clone(), at);
            if self
                .told
                .get(&key)
                .is_some_and(|&told| same_reset(told, at))
            {
                continue;
            }
            self.told.insert(key, at);
            kept = true;
            jobs.push(Job::Post {
                notice: crate::present::run_out_notice(advice),
            });
        }
        if kept {
            jobs.push(Job::KeepTold {
                told: self.told.clone(),
            });
        }
        let standing: Vec<Advice> = new
            .into_iter()
            .chain(self.advice.iter().filter_map(|advice| advice.renewed(read)))
            .collect();
        let codes: Vec<&str> = standing.iter().map(|a| a.provider.as_str()).collect();
        self.advice = crate::present::in_order(&codes, &tools)
            .iter()
            .filter_map(|provider| standing.iter().find(|a| &a.provider == provider))
            .cloned()
            .collect();
    }

    /// The settings' switch for switching Claude Code by itself, with its share. Taken only
    /// once the preferences have been read, so it is never kept over a file that could not
    /// be, and never lost under what the file says once it is. Turned on, or at another
    /// share, the core looks at once. Turned off, nothing is said under it, and a refusal's
    /// wait is over: turned on again, it is somebody asking.
    fn auto_switch_set(&mut self, on: bool, at: u8, jobs: &mut Vec<Job>) {
        if !self.preferences_read {
            return;
        }
        let at = Some(i64::from(Threshold::clamped(i64::from(at)).percent()));
        if self.preferences.auto_switch == on && self.preferences.auto_switch_at == at {
            return;
        }
        self.preferences.auto_switch = on;
        self.preferences.auto_switch_at = at;
        jobs.push(Job::KeepPreferences {
            preferences: self.preferences.clone(),
        });
        if !on {
            self.auto_standing = None;
            self.auto_wait = None;
        }
        self.look_to_switch(jobs);
    }

    /// Asks the core's look whether to switch Claude Code by itself, where somebody turned
    /// that on: after every read, after numbers or a change the poll found, as it is turned
    /// on, and every `decide_every`. The look reads files alone and claims nothing, so it is
    /// asked whatever else is under way.
    fn look_to_switch(&mut self, jobs: &mut Vec<Job>) {
        if !self.preferences_read || !self.preferences.auto_switch {
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
        self.decide = if self.started && self.preferences.auto_switch {
            let next = now.running + self.cadence.decide_every;
            Timer::Due(match self.auto_wait {
                Some((until, _)) if until > now.running => next.min(until),
                _ => next,
            })
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
        if !self.preferences.auto_switch {
            return;
        }
        match looked {
            Ok(AutoLooked::Stands(outcome)) => self.auto_stands(outcome, jobs),
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
        if self.switch_under_way().is_some()
            || self.changing > 0
            || self.auto_wait.is_some_and(|(until, _)| now.running < until)
        {
            return;
        }
        self.switching = Some(AUTOMATICALLY.to_owned());
        jobs.push(Job::AutoSwitch {
            at: self.preferences.threshold().percent(),
        });
    }

    /// What the core's decision under its lock came to. A switch is taken as one somebody
    /// asked for is, and said in a notification, since nobody was there to ask for it, even
    /// where the setting was turned off while it was made. Anything else it came to after it
    /// was turned off moved nothing, and is neither said nor waited on: turned on again, it
    /// is somebody asking. A refusal is said once for its code until the next switch, and the
    /// next is asked for no sooner than the core waits after as many attempts failed in a row.
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
                adoption,
                warnings,
                used,
            }) => {
                jobs.push(Job::Post {
                    notice: crate::present::auto_switched_notice(
                        &from,
                        &to,
                        &used,
                        &adoption,
                        now.epoch(),
                    ),
                });
                self.auto_told.clear();
                self.auto_wait = None;
                let qualified = qualified_claude(&to);
                let switched = Switched {
                    outcome: Switch::Switched {
                        provider: ProviderId::Claude.code().into(),
                        from: from.clone(),
                        to: to.clone(),
                        adoption: adoption.clone(),
                    },
                    warnings: warnings.clone(),
                };
                self.auto_standing = Some(AutoStanding::Came(AutoSwitched::Switched {
                    from,
                    to,
                    adoption,
                    warnings,
                    used,
                }));
                self.switched(&qualified, None, Ok(switched), now, jobs);
            }
            _ if !self.preferences.auto_switch => self.switching = None,
            Ok(outcome) => {
                self.switching = None;
                self.auto_stands(outcome, jobs);
            }
            Err(PitboardError::Failed { code, message, .. }) => {
                self.switching = None;
                let refusals = self.auto_wait.map_or(0, |(_, refusals)| refusals) + 1;
                let wait = autoswitch::retry_after(refusals);
                self.auto_wait = Some((
                    now.running + Duration::from_secs(wait.unsigned_abs()),
                    refusals,
                ));
                self.auto_standing = Some(AutoStanding::Stopped {
                    message: message.clone(),
                    until: Some(now.epoch() + wait),
                });
                if self.auto_told.insert(format!("refused/{code}")) {
                    jobs.push(Job::Post {
                        notice: crate::present::auto_refused_notice(&code, &message),
                    });
                }
            }
        }
    }

    /// The core's look or its decision came to `outcome`, which is no switch and ends a row of
    /// refusals. Said under the setting, and a reason it did not switch away from a limit, or
    /// cannot judge at all, posted once for what tells it apart until the next switch.
    fn auto_stands(&mut self, outcome: AutoSwitched, jobs: &mut Vec<Job>) {
        self.auto_wait = None;
        let told = match &outcome {
            AutoSwitched::Skipped {
                key,
                from,
                used,
                why,
                ..
            } => Some((
                key.clone(),
                crate::present::auto_skipped_notice(key, from, used, why),
            )),
            AutoSwitched::NotWatching { key, why, .. } => Some((
                key.clone(),
                crate::present::auto_not_watching_notice(key, why),
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

    fn read(&mut self, asked: Asked, now: Now, jobs: &mut Vec<Job>) {
        let older_than = i64::try_from(asked.older_than.as_millis()).unwrap_or(i64::MAX);
        let ticket = Ticket {
            changes_seen: self.changes_seen,
            timed: asked.timed,
            after_switch: asked.after_switch,
            saying: asked.saying,
            after_renewal: asked.after_renewal,
            after_change: asked.after_change,
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
        if ticket.after_change {
            self.change_over();
        }
        if ticket.after_renewal {
            self.machine.renewing = false;
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

    fn look_over(&mut self, now: Now) {
        if self.started {
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
            } => self.read_landed(ticket, readings_before, changed_before, read, now, jobs),
            Answer::ReadOffline { why, read } => self.known(why, read.ok(), now, jobs),
            Answer::Looked { changed, measured } => self.looked(changed, measured, now, jobs),
            Answer::Installed { tools } => {
                self.installed = Some(tools);
                self.installed_over(now, jobs);
            }
            Answer::Held {
                qualified,
                question,
            } => self.held(qualified, question, jobs),
            Answer::Quit { question, outcome } => self.quit_over(question, outcome, jobs),
            Answer::Switched {
                qualified,
                reopen,
                done,
            } => self.switched(&qualified, reopen, done.map_err(Some), now, jobs),
            Answer::AutoLooked { looked } => self.auto_looked(looked, now, jobs),
            Answer::AutoSwitched { done } => self.auto_switched(done, now, jobs),
            Answer::Opened | Answer::Pasted | Answer::Stopped | Answer::Saved | Answer::Posted => {}
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
                provider,
                from,
                done,
            } => {
                let done = done.map(drop).map_err(Some);
                self.enrolled_now(&provider, from.as_ref(), done, now, jobs);
            }
            Answer::Renamed {
                provider,
                label,
                to,
                from,
                done,
            } => {
                let renaming = Renaming {
                    provider: &provider,
                    label: &label,
                    to: &to,
                    from: from.as_ref(),
                };
                self.renamed(&renaming, done.map_err(Some), now, jobs);
            }
            Answer::Forgot { qualified, done } => {
                self.forgot(&qualified, done.map_err(Some), now, jobs);
            }
            Answer::ConfigUpdated(done) => self.config_updated(done.map_err(Some), now, jobs),
            Answer::Abandoned(done) => self.abandon_over(done.map_err(Some), now, jobs),
            Answer::SignInStarted { id, started } => {
                // A tool that could not start leaves its thread nothing more to do.
                let over = started.is_err();
                self.sign_in_started(id, started.map_err(Some), jobs);
                if over {
                    self.sign_in_let_go(id, jobs);
                }
            }
            Answer::SignInSaid { id, text } => {
                if let Some(signing) = self.signing_in.as_mut().filter(|s| s.id == id) {
                    signing.said.push_str(&text);
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
            Answer::Renewed { renewals } => {
                self.machine.renewals = Some(renewals);
                self.read_after_renewal(now, jobs);
            }
            Answer::Checked { checks } => self.machine.checked(checks, now.epoch()),
            Answer::Logged { changes } => self.machine.logged(changes),
            Answer::CommandLineFound { found, own } => {
                self.machine.command_line = Some(found);
                self.machine.own = Some(own);
            }
            Answer::Lost(job) => match job {
                Job::Read { ticket, .. } => self.landed(ticket, now),
                Job::ReadOffline { why } => self.known(why, None, now, jobs),
                Job::Look => self.look_over(now),
                Job::AskInstalled => self.installed_over(now, jobs),
                // As the Swift model took a holding it could not read: nothing holds it.
                Job::Holding { qualified } => self.held(qualified, None, jobs),
                // Nothing is switched under an app nobody saw go.
                Job::Quit { question } => self.quit_over(question, QuitOutcome::StillRunning, jobs),
                Job::Switch { qualified, reopen } => {
                    self.switched(&qualified, reopen, Err(None), now, jobs);
                }
                // Nothing is said of a look that came to nothing: the next is at its time.
                Job::AutoLook { .. } => self.decide_later(now),
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
                    self.sign_in_started(id, Err(None), jobs);
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
                Job::Enrol { provider, from, .. } => {
                    self.enrolled_now(&provider, from.as_ref(), Err(None), now, jobs);
                }
                Job::Rename {
                    provider,
                    label,
                    to,
                    from,
                } => {
                    let renaming = Renaming {
                        provider: &provider,
                        label: &label,
                        to: &to,
                        from: from.as_ref(),
                    };
                    self.renamed(&renaming, Err(None), now, jobs);
                }
                Job::Forget { qualified } => self.forgot(&qualified, Err(None), now, jobs),
                Job::UpdateConfig { .. } => self.config_updated(Err(None), now, jobs),
                // Nothing kept could be read: nothing was told before, as far as anyone knows,
                // and the windows' records are held for this launch, never written.
                Job::LoadKept => self.kept(Told::new(), None, None, now, jobs),
                Job::KeepTold { .. } | Job::KeepPreferences { .. } | Job::Post { .. } => {}
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
                // What it renewed is not known, and is not said; the accounts are read all
                // the same, to show where things stand.
                Job::Renew => self.read_after_renewal(now, jobs),
                Job::Check => self.machine.checked(None, now.epoch()),
                Job::ReadLog { .. } | Job::FindCommandLine => {}
            },
        }
    }

    /// The read after a renewal, asking every service, as MachineModel.swift's `renewed` had
    /// the accounts read as somebody asking: the renewal is over once it is.
    fn read_after_renewal(&mut self, now: Now, jobs: &mut Vec<Job>) {
        self.refresh(
            Asked {
                fresh: true,
                after_renewal: true,
                after_change: true,
                ..Asked::default()
            },
            now,
            jobs,
        );
    }

    /// The tool of the sign-in `id` has started, or could not be. `Err(None)` is a start
    /// that came to nothing.
    fn sign_in_started(
        &mut self,
        id: u64,
        started: Result<(), Option<PitboardError>>,
        jobs: &mut Vec<Job>,
    ) {
        let current = self.signing_in.as_ref().is_some_and(|s| s.id == id);
        match started {
            Ok(()) => match self.signing_in.as_mut().filter(|_| current) {
                Some(signing) => signing.started = true,
                // Cancelled while it was starting: what started is stopped rather than
                // watched.
                None => jobs.push(Job::StopSignIn { id }),
            },
            Err(error) => {
                // Cancelled, and it never started: nothing to say.
                if let Some(signing) = self.signing_in.take_if(|_| current) {
                    self.sign_in_failed(&signing, error);
                }
            }
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
                self.changes_seen += 1;
                self.updated_ms = None;
                let mut asked = Asked {
                    after_change: true,
                    ..Asked::default()
                };
                if let Some(signing) = signing {
                    // The sheet it was started from, if it is still up.
                    self.sheet = None;
                    self.sheet_failure = None;
                    let said = self.enrolled(&signing, done);
                    asked.saying = self.say_after_read(said);
                }
                self.refresh(asked, now, jobs);
            }
            (Err(error), Some(signing)) => self.sign_in_failed(&signing, error),
            (Err(_), None) => {}
        }
    }

    /// A sign-in that could not start or finish, said in the sheet it was started from,
    /// which stays up to say it with the name still in it, and its warnings beside the
    /// accounts, since what it found is about the machine and outlives the sheet. Where that
    /// sheet has gone, or none was up, it is said in the window: something asked for that did
    /// not happen is said somewhere.
    fn sign_in_failed(&mut self, signing: &SigningIn, error: Option<PitboardError>) {
        let title = could_not_sign_in(&signing.name);
        let refused = match error {
            Some(error) => Refused::of(title, error),
            None => Refused::lost(title),
        };
        self.keep(&refused.warnings);
        if self.sheet.is_some() {
            let failure = self.number(refused);
            self.sheet_failure = Some(failure);
        } else {
            self.present(refused);
        }
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
        let provider = signing.provider.as_str();
        let to = typed_as_core(provider, &signing.name);
        let mut said = self
            .last_switches
            .iter()
            .find(|last| last.provider == provider && last.to == to)
            .cloned()
            .unwrap_or_else(|| LastSwitch {
                provider: provider.to_owned(),
                to,
                follows_at: None,
                restart: None,
                said: None,
                warnings: Vec::new(),
            });
        said.said = Some(signed_in_now(&signing.name, again));
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
                self.stuck = read
                    .warnings
                    .iter()
                    .any(|w| w.code == "recovery_undetermined");
                self.forget_switches_undone(&read);
                self.tell_replaced(&read.warnings, jobs);
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
    fn tell_replaced(&mut self, warnings: &[Warning], jobs: &mut Vec<Job>) {
        let replaced: Vec<(&str, &Warning)> = warnings
            .iter()
            .filter(|warning| warning.code == "login_replaced")
            .filter_map(|warning| Some((warning.account.as_deref()?, warning)))
            .collect();
        for (account, warning) in &replaced {
            if !self.replaced_told.contains(*account) {
                jobs.push(Job::Post {
                    notice: crate::present::standing_notice(warning),
                });
            }
        }
        self.replaced_told = replaced
            .into_iter()
            .map(|(account, _)| account.to_owned())
            .collect();
    }

    /// Whether `read`, which asked nobody, says Claude Code's config has named another
    /// account since Anthropic last named the login stored, in words no read that asks has
    /// been started for yet. A sign-in leaves the config so, and a session's status line
    /// records numbers soon after one, so the read after them asks Anthropic whose the login
    /// is now, once for each thing the config is said to name.
    fn unconfirmed(&mut self, read: &Status) -> bool {
        let said = read
            .warnings
            .iter()
            .find(|warning| warning.code == "in_use_unconfirmed")
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
        if self.switching.is_some() || self.changing > 0 {
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

    /// A switch asked for from the window, the menu or a notification.
    ///
    /// Where an app runs the tool with its login in memory, as ChatGPT runs Codex, the switch
    /// waits for the person to let Pitboard quit it first: switched under it, the app would go
    /// on with the account left behind, and its own sign-out would revoke the login Pitboard
    /// has just parked.
    fn switch_asked(&mut self, qualified: String, jobs: &mut Vec<Job>) {
        // One switch at a time: the second would wait behind the first anyway, and its
        // choice was made from a menu that did not yet show the first. A question waiting to
        // be answered comes back to the front rather than being left behind unseen.
        if self.switch_under_way().is_some() {
            if self.asking().is_some() {
                self.show_window(Some(Pane::Accounts));
            }
            return;
        }
        // Claimed before anything is asked, so a second request made meanwhile waits too,
        // and in place of a question closed unanswered, which no answer takes after this.
        self.quitting = None;
        self.switching = Some(qualified.clone());
        jobs.push(Job::Holding { qualified });
    }

    /// What holds the login is known: an app to ask about, or nothing, and then the switch.
    fn held(&mut self, qualified: String, question: Option<QuitQuestion>, jobs: &mut Vec<Job>) {
        match question {
            Some(question) => {
                self.switching = None;
                self.quitting = Some(Quitting {
                    question,
                    asked: true,
                });
                self.show_window(Some(Pane::Accounts));
            }
            None => jobs.push(Job::Switch {
                qualified,
                reopen: None,
            }),
        }
    }

    /// The person let Pitboard quit the app for the switch to `qualified`: the switch is
    /// under way again from here, and the question is gone. Taken whether or not the question
    /// is still asked, since an app can close it first, but only as the answer to the
    /// question last asked, and once. A switch asked for since has taken that question's
    /// place, so an answer never starts a second switch.
    fn quit_and_switch(&mut self, qualified: &str, jobs: &mut Vec<Job>) {
        let Some(Quitting { question, .. }) = self
            .quitting
            .take_if(|quitting| quitting.question.qualified == qualified)
        else {
            return;
        };
        self.switching = Some(question.qualified.clone());
        jobs.push(Job::Quit { question });
    }

    /// Quits the app, switches, and opens the same copy of the app again: Pitboard closed it,
    /// so Pitboard opens it. An app that is already gone is not opened. One that does not
    /// quit, because it was busy or its person said no, stops everything before anything has
    /// changed.
    fn quit_over(&mut self, question: QuitQuestion, outcome: QuitOutcome, jobs: &mut Vec<Job>) {
        let QuitQuestion {
            qualified, name, ..
        } = question;
        match outcome {
            QuitOutcome::StillRunning => {
                self.switching = None;
                self.present(Refused {
                    title: could_not_switch(&qualified),
                    message: format!(
                        "{name} is still open, so nothing has changed. Quit it, then switch again."
                    ),
                    code: None,
                    warnings: Vec::new(),
                });
            }
            QuitOutcome::NotRunning => jobs.push(Job::Switch {
                qualified,
                reopen: None,
            }),
            QuitOutcome::Quit { copy } => jobs.push(Job::Switch {
                qualified,
                reopen: Some(copy),
            }),
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
        qualified: &str,
        reopen: Option<String>,
        done: Result<Switched, Option<PitboardError>>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        // Pitboard closed it, so Pitboard opens it, whether or not the switch worked: as
        // soon as the switch is made, and not after the read that follows it, which can wait
        // on the network.
        if let Some(location) = reopen {
            jobs.push(Job::Open { location });
        }
        match done {
            Ok(done) => {
                self.changes_seen += 1;
                // Advice about this tool is about the account it has just left. Another
                // tool's stays: it is as true as it was, and it is never told again. Nothing
                // moved where the account was already in use, and advice about it stands
                // until a read no longer bears it out.
                if matches!(done.outcome, Switch::Switched { .. }) {
                    let provider = split(qualified).0;
                    self.advice.retain(|advice| advice.provider != provider);
                }
                self.said(qualified, done, now);
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
            Err(error) => {
                self.switching = None;
                let title = could_not_switch(qualified);
                let refused = match error {
                    Some(error) => Refused::of(title, error),
                    None => Refused::lost(title),
                };
                // Nothing moved, so what the last switch said stands too.
                self.keep(&refused.warnings);
                self.present(refused);
            }
        }
    }

    /// Keeps what a switch that worked said, in place of what its own tool's last switch
    /// said and beside what every other tool's did.
    fn said(&mut self, qualified: &str, done: Switched, now: Now) {
        match done.outcome {
            Switch::Switched {
                provider,
                from,
                to,
                adoption,
            } => {
                let (follows_at, restart) = match adoption {
                    Adoption::Follows { within_seconds } => {
                        (Some(now.epoch() + i64::from(within_seconds)), None)
                    }
                    Adoption::Renewal { .. } => (None, None),
                    Adoption::Restart { program } => (
                        None,
                        Some(RestartNeeded {
                            program,
                            from: split(&from).1.to_owned(),
                        }),
                    ),
                };
                self.remember(LastSwitch {
                    provider,
                    to,
                    follows_at,
                    restart,
                    said: None,
                    warnings: done.warnings,
                });
            }
            Switch::AlreadyActive { label } => {
                // Nothing moved, so what this tool's last switch said still stands, and
                // anything this one warned about is said beside it.
                if done.warnings.is_empty() {
                    return;
                }
                let provider = split(qualified).0;
                let mut said = self
                    .last_switches
                    .iter()
                    .find(|last| last.provider == provider && last.to == label)
                    .cloned()
                    .unwrap_or_else(|| LastSwitch {
                        provider: provider.to_owned(),
                        to: label,
                        follows_at: None,
                        restart: None,
                        said: None,
                        warnings: Vec::new(),
                    });
                for warning in done.warnings {
                    if !said.warnings.contains(&warning) {
                        said.warnings.push(warning);
                    }
                }
                self.remember(said);
            }
        }
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
                    && typed(account) == Some(last.to.as_str())
            })
        });
    }

    /// The login signed in now to `provider`'s tool has been enrolled, or could not be. Once it
    /// is, any sheet naming a login of that tool closes, as AppModel.swift's `enrol` closed
    /// one: a sheet for another tool, or another sheet put up meanwhile, is somebody else's,
    /// and closing it threw away whatever was in it. `Err(None)` is a save that came to
    /// nothing.
    fn enrolled_now(
        &mut self,
        provider: &str,
        from: Option<&Sheet>,
        done: Result<(), Option<PitboardError>>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        self.saved(from);
        match done {
            Ok(()) => {
                self.changes_seen += 1;
                if matches!(&self.sheet, Some(Sheet::Name { provider: of, .. }) if of == provider) {
                    self.sheet = None;
                    self.sheet_failure = None;
                }
                self.updated_ms = None;
                self.read_after_change(now, jobs);
            }
            Err(error) => {
                self.change_over();
                let refused = match error {
                    Some(error) => Refused::of(COULD_NOT_NAME.into(), error),
                    None => Refused::lost(COULD_NOT_NAME.into()),
                };
                self.save_failed(from, refused);
            }
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
        let Renaming {
            provider,
            label,
            to,
            from,
        } = *renaming;
        self.saved(from);
        match done {
            Ok(()) => {
                self.changes_seen += 1;
                self.carry(provider, label, to, jobs);
                let own = Sheet::Rename {
                    provider: provider.to_owned(),
                    label: label.to_owned(),
                };
                if self.sheet.as_ref() == Some(&own) {
                    self.sheet = None;
                    self.sheet_failure = None;
                }
                self.updated_ms = None;
                self.read_after_change(now, jobs);
            }
            Err(error) => {
                self.change_over();
                let title = could_not_rename(label);
                let refused = match error {
                    Some(error) => Refused::of(title, error),
                    None => Refused::lost(title),
                };
                self.save_failed(from, refused);
            }
        }
    }

    /// What was said about the account `label` of `provider`'s tool, said about it as `to`:
    /// what its tool's last switch said, advice about it, and what was told about it.
    fn carry(&mut self, provider: &str, label: &str, to: &str, jobs: &mut Vec<Job>) {
        for advice in &mut self.advice {
            if advice.provider == provider {
                *advice = advice.renaming(label, to);
            }
        }
        rename_told(&mut self.told_this_launch, provider, label, to);
        let before = self.told.clone();
        rename_told(&mut self.told, provider, label, to);
        if self.told != before {
            jobs.push(Job::KeepTold {
                told: self.told.clone(),
            });
        }
        let (old, new) = (typed_as_core(provider, label), typed_as_core(provider, to));
        for last in &mut self.last_switches {
            if last.provider != provider {
                continue;
            }
            if last.to == old {
                last.to.clone_from(&new);
            }
            if let Some(restart) = &mut last.restart
                && restart.from == label
            {
                restart.from = to.to_owned();
            }
        }
    }

    /// The account `qualified` names has been forgotten, and the login parked for it, or it
    /// could not be, which is said in the window it was asked from.
    fn forgot(
        &mut self,
        qualified: &str,
        done: Result<(), Option<PitboardError>>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        match done {
            Ok(()) => {
                self.changes_seen += 1;
                self.updated_ms = None;
                self.read_after_change(now, jobs);
            }
            Err(error) => {
                self.change_over();
                let title = could_not_forget(qualified);
                self.present(match error {
                    Some(error) => Refused::of(title, error),
                    None => Refused::lost(title),
                });
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
                    Some(error) => Refused::of(COULD_NOT_UPDATE_CONFIG.into(), error),
                    None => Refused::lost(COULD_NOT_UPDATE_CONFIG.into()),
                });
            }
        }
    }

    /// Giving up on an interrupted switch is over: what it kept is said and the accounts are
    /// read again, asking each service, or what stopped it is said in the window.
    fn abandon_over(
        &mut self,
        done: Result<Option<Abandoned>, Option<PitboardError>>,
        now: Now,
        jobs: &mut Vec<Job>,
    ) {
        match done {
            Ok(abandoned) => {
                self.abandoned = abandoned;
                self.changes_seen += 1;
                self.stuck = false;
                self.refresh(
                    Asked {
                        fresh: true,
                        after_change: true,
                        ..Asked::default()
                    },
                    now,
                    jobs,
                );
            }
            Err(error) => {
                self.change_over();
                self.present(match error {
                    Some(error) => Refused::of(COULD_NOT_GIVE_UP.into(), error),
                    None => Refused::lost(COULD_NOT_GIVE_UP.into()),
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
        self.show_window(None);
    }

    /// A failure, numbered one higher than the last, wherever it is said.
    fn number(&mut self, refused: Refused) -> Failure {
        self.failures += 1;
        Failure {
            id: self.failures,
            title: refused.title,
            message: refused.message,
            code: refused.code,
            warnings: refused.warnings,
        }
    }

    /// A failure's warnings said beside the ones already shown, first, with nothing the last
    /// read found dropped to make room for them, as AppModel.swift's `keep` said them.
    fn keep(&mut self, warned: &[Warning]) {
        let mut warnings = warned.to_vec();
        warnings.extend(
            self.warnings
                .drain(..)
                .filter(|warning| !warned.contains(warning)),
        );
        self.warnings = warnings;
    }

    /// Asks for the main window, on `pane` when it matters which.
    fn show_window(&mut self, pane: Option<Pane>) {
        self.window.serial += 1;
        self.window.pane = pane;
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
    }
}
