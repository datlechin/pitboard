//! What the model's tests share: records as the core reports them, the state driven by hand,
//! other apps as a test says they are, and, for the threaded tests, a machine of the test's
//! own that the real core runs on.

use super::EarlierPreferences;
use super::advice::Told;
use super::lanes;
use super::preferences::Preferences;
use super::state::{Answer, Cadence, Job, Msg, Now, State};
use super::{AppControl, Intent, Notifications, PlatformError, RunOutNotice, Snapshot};
use crate::account_windows::records::{self, Entry, Records};
use crate::{
    Abandoned, Account, Adoption, AppCore, AutoLooked, AutoSwitched, Change, Check, Enrolled,
    EnrolledAs, FoundCommandLine, Holding, Level, Limit, Made, OwnCommandLine, PitboardError,
    Remedy, Renewed, Schedule, Source, Status, Switch, Switched, Tool, Usage, Warning,
};
use pitboard_core::context::Context;
use pitboard_core::provider::ProviderId;
use pitboard_core::service;
use pitboard_core::testing::{MemoryHost, ScriptedApi, live_service};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The ChatGPT app's bundle id, which is how the core's holder detection names it.
pub(super) const CHATGPT: &str = "com.openai.codex";

/// Where ChatGPT runs its own `codex`, as the process list of a Mac running ChatGPT
/// 26.928.31416 gave it, which is how the core tells the app is open.
pub(super) const CHATGPT_CODEX: &str =
    "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex";

pub(super) fn claude_code() -> Tool {
    crate::tool(ProviderId::Claude)
}

pub(super) fn codex() -> Tool {
    crate::tool(ProviderId::Codex)
}

/// A limit as the core reports one.
pub(super) fn window(kind: &str, percent: f64) -> Limit {
    Limit {
        kind: kind.into(),
        length_seconds: None,
        scope: None,
        percent,
        resets_at: Some(100),
        severity: None,
        is_active: true,
    }
}

/// An account as the core reports one. `label` `None` is a login signed in and not
/// enrolled. Switchable unless it is the one signed in, and its numbers every limit it has
/// where its tool is Claude Code, as a real one is.
pub(super) fn account(
    label: Option<&str>,
    provider: &str,
    signed_in: bool,
    windows: Vec<Limit>,
) -> Account {
    let uuid = label.unwrap_or("someone");
    Account {
        id: format!("{provider}:{uuid}"),
        provider: provider.into(),
        label: label.map(str::to_owned),
        qualified: label.map(|label| format!("{provider}/{label}")),
        unplaced: false,
        email: format!("{uuid}@example.com"),
        account_id: uuid.into(),
        signed_in,
        switchable: !signed_in && label.is_some(),
        parked: None,
        usage: Some(Usage {
            source: Source::Live,
            observed_at: Some(0),
            windows,
            lists_every_limit: provider == "claude",
        }),
        stale: None,
        stale_explanation: None,
    }
}

/// A Claude Code account whose five-hour window is `percent` used.
pub(super) fn claude(label: &str, signed_in: bool, percent: f64) -> Account {
    account(
        Some(label),
        "claude",
        signed_in,
        vec![window("session", percent)],
    )
}

/// A Codex account with no limits read.
pub(super) fn codex_account(label: &str, signed_in: bool) -> Account {
    account(Some(label), "codex", signed_in, Vec::new())
}

pub(super) fn status(accounts: Vec<Account>) -> Status {
    warned(accounts, Vec::new())
}

pub(super) fn warned(accounts: Vec<Account>, warnings: Vec<Warning>) -> Status {
    Status {
        now: 0,
        accounts,
        warnings,
    }
}

pub(super) fn warning(code: &str, message: &str) -> Warning {
    Warning {
        code: code.into(),
        message: message.into(),
        account: None,
    }
}

/// The warning a Codex switch carries when `codex` sessions it cannot reach still run, as
/// the core words it.
pub(super) fn still_running() -> Warning {
    warning(
        "sessions_still_running",
        "2 `codex` sessions started before this switch are still running and still using \
         `codex/personal`. Quit them and start again to use the new account. Do not sign out \
         in any of them: signing out there revokes `codex/personal`'s login, which Pitboard \
         has just parked.",
    )
}

/// A switch of `provider`'s tool, as the core reports one: `from` and `to` typed the way
/// the core types them, bare for Claude Code and with the tool for any other. Codex's open
/// sessions never follow; Claude Code's follow within 33 seconds.
pub(super) fn switched(
    provider: &str,
    from: &str,
    to: &str,
    warnings: Vec<Warning>,
) -> Result<Switched, Refusal> {
    let adoption = if provider == "codex" {
        Adoption::Restart {
            program: "codex".into(),
        }
    } else {
        Adoption::Follows { within_seconds: 33 }
    };
    Ok(Switched {
        outcome: Switch::Switched {
            provider: provider.into(),
            from: from.into(),
            to: to.into(),
            adoption,
        },
        warnings,
    })
}

/// Nothing to switch by itself: no limit of `account`, the account in use, has reached the
/// share, with no reading to say how much of which it has used.
pub(super) fn watching(account: &str) -> AutoSwitched {
    AutoSwitched::Watching {
        account: account.into(),
        used: None,
        as_of: None,
        held_until: None,
    }
}

/// A switch to the account already in use, as the core reports one.
pub(super) fn already_active(label: &str, warnings: Vec<Warning>) -> Result<Switched, Refusal> {
    Ok(Switched {
        outcome: Switch::AlreadyActive {
            label: label.into(),
        },
        warnings,
    })
}

/// What a sign-in enrolled, as the core reports it.
pub(super) fn enrolled_as(
    outcome: EnrolledAs,
    warnings: Vec<Warning>,
) -> Result<Enrolled, Refusal> {
    Ok(Enrolled {
        email: "w@example.com".into(),
        outcome,
        warnings,
    })
}

/// ChatGPT running Codex's login, as the core's holder detection reports it.
pub(super) fn chatgpt_holding() -> Holding {
    Holding {
        kind: "chatgpt_app".into(),
        phrase: "the ChatGPT app".into(),
        pids: vec![4242, 4243],
        remedy: Remedy::ReopenApp {
            app_id: CHATGPT.into(),
            name: "ChatGPT".into(),
        },
    }
}

/// What else an app quitting does, given the app's id.
type OnQuit = Box<dyn Fn(&str) + Send>;

/// Other apps, as a test says they are, and nothing on the machine running the tests: this
/// machine may have ChatGPT open. Records every app asked to quit and every app opened.
pub(super) struct StandInApps {
    running: Mutex<BTreeSet<String>>,
    /// Whether an app asked to quit does. One busy with work, or whose person said no, does
    /// not.
    quits: AtomicBool,
    asked: Mutex<Vec<String>>,
    running_calls: AtomicUsize,
    fail_running: AtomicBool,
    fail_quit: AtomicBool,
    fail_open: AtomicBool,
    /// Holds each call of `running` until the test lets it go, telling the test as it
    /// arrives, where a test says so.
    hold: Mutex<Option<(Sender<()>, Receiver<()>)>>,
    /// What else an app quitting does, such as its processes leaving the process list.
    on_quit: Mutex<Option<OnQuit>>,
}

impl StandInApps {
    pub(super) fn new(running: &[&str], quits: bool) -> Arc<StandInApps> {
        Arc::new(StandInApps {
            running: Mutex::new(running.iter().map(|&id| id.to_owned()).collect()),
            quits: AtomicBool::new(quits),
            asked: Mutex::new(Vec::new()),
            running_calls: AtomicUsize::new(0),
            fail_running: AtomicBool::new(false),
            fail_quit: AtomicBool::new(false),
            fail_open: AtomicBool::new(false),
            hold: Mutex::new(None),
            on_quit: Mutex::new(None),
        })
    }

    /// From now on an app that quits does `then` too, with its id.
    pub(super) fn on_quit(&self, then: impl Fn(&str) + Send + 'static) {
        *self.on_quit.lock().expect("a test's own lock") = Some(Box::new(then));
    }

    /// Where a test's app is, which is nowhere on the machine running it.
    pub(super) fn copy_of(app: &str) -> String {
        format!("/stand-in/{app}.app")
    }

    /// Every app asked to quit and every app opened, in order: "quit com.openai.codex".
    pub(super) fn asked(&self) -> Vec<String> {
        self.asked.lock().expect("a test's own lock").clone()
    }

    pub(super) fn set_running(&self, apps: &[&str]) {
        *self.running.lock().expect("a test's own lock") =
            apps.iter().map(|&id| id.to_owned()).collect();
    }

    pub(super) fn is_running(&self, app: &str) -> bool {
        self.running
            .lock()
            .expect("a test's own lock")
            .contains(app)
    }

    /// How many times it was asked whether an app runs.
    pub(super) fn running_calls(&self) -> usize {
        self.running_calls.load(Ordering::SeqCst)
    }

    /// From now on asking whether an app runs throws, as an app's own code can.
    pub(super) fn fail_running(&self, fails: bool) {
        self.fail_running.store(fails, Ordering::SeqCst);
    }

    pub(super) fn fail_quit(&self, fails: bool) {
        self.fail_quit.store(fails, Ordering::SeqCst);
    }

    pub(super) fn fail_open(&self, fails: bool) {
        self.fail_open.store(fails, Ordering::SeqCst);
    }

    /// Holds every call of `running` from now on until the returned sender is dropped,
    /// telling the returned receiver as each arrives.
    pub(super) fn hold_running(&self) -> (Receiver<()>, Sender<()>) {
        let (arrived, arrivals) = channel();
        let (release, held) = channel();
        *self.hold.lock().expect("a test's own lock") = Some((arrived, held));
        (arrivals, release)
    }

    fn refused(what: &str) -> PlatformError {
        PlatformError::Failed {
            reason: format!("the stand-in was told to fail {what}"),
        }
    }
}

impl AppControl for StandInApps {
    fn running(&self, app: String) -> Result<Option<String>, PlatformError> {
        self.running_calls.fetch_add(1, Ordering::SeqCst);
        if let Some((arrived, held)) = self.hold.lock().expect("a test's own lock").as_ref() {
            let _ = arrived.send(());
            let _ = held.recv();
        }
        if self.fail_running.load(Ordering::SeqCst) {
            return Err(StandInApps::refused("to say what runs"));
        }
        Ok(self.is_running(&app).then(|| StandInApps::copy_of(&app)))
    }

    fn request_quit(&self, app: String) -> Result<(), PlatformError> {
        if self.fail_quit.load(Ordering::SeqCst) {
            return Err(StandInApps::refused("to ask an app to quit"));
        }
        self.asked
            .lock()
            .expect("a test's own lock")
            .push(format!("quit {app}"));
        if self.quits.load(Ordering::SeqCst) {
            self.running.lock().expect("a test's own lock").remove(&app);
            if let Some(then) = self.on_quit.lock().expect("a test's own lock").as_ref() {
                then(&app);
            }
        }
        Ok(())
    }

    fn reopen(&self, location: String) -> Result<(), PlatformError> {
        if self.fail_open.load(Ordering::SeqCst) {
            return Err(StandInApps::refused("to open an app"));
        }
        let app = location
            .strip_prefix("/stand-in/")
            .and_then(|rest| rest.strip_suffix(".app"))
            .unwrap_or(&location)
            .to_owned();
        self.asked
            .lock()
            .expect("a test's own lock")
            .push(format!("open {app}"));
        self.running.lock().expect("a test's own lock").insert(app);
        Ok(())
    }
}

/// The system's notifications, as a test has them: every notification posted is kept, and
/// none reaches the machine running the tests.
#[derive(Default)]
pub(super) struct Posted {
    posted: Mutex<Vec<RunOutNotice>>,
}

impl Posted {
    pub(super) fn posted(&self) -> Vec<RunOutNotice> {
        self.posted.lock().expect("a test's own lock").clone()
    }
}

impl Notifications for Posted {
    fn post(&self, notice: RunOutNotice) -> Result<(), PlatformError> {
        self.posted.lock().expect("a test's own lock").push(notice);
        Ok(())
    }
}

/// A failure as the core reports one.
#[derive(Debug, Clone)]
pub(super) struct Refusal {
    pub code: String,
    pub message: String,
    pub warnings: Vec<Warning>,
}

impl Refusal {
    pub(super) fn error(&self) -> PitboardError {
        PitboardError::Failed {
            code: self.code.clone(),
            message: self.message.clone(),
            warnings: self.warnings.clone(),
        }
    }
}

pub(super) fn refusal(code: &str, message: &str, warnings: Vec<Warning>) -> Refusal {
    Refusal {
        code: code.into(),
        message: message.into(),
        warnings,
    }
}

/// The percentage the first window of the account at `index` shows.
pub(super) fn percent(snapshot: &Snapshot, index: usize) -> Option<f64> {
    let usage = snapshot
        .status
        .as_ref()?
        .accounts
        .get(index)?
        .usage
        .as_ref()?;
    Some(usage.windows.first()?.percent)
}

/// What the core answers, as a test says, the way the Swift tests' stub answered.
pub(super) struct Machine {
    /// What a read gives.
    pub answer: Result<Status, Refusal>,
    /// What reading what is already known gives.
    pub offline: Result<Status, Refusal>,
    /// When the account index was last written, in seconds.
    pub changed: i64,
    /// When the usage readings were last written. A read moves it, as the core's does: what
    /// it measured is recorded.
    pub readings: i64,
    /// The tools whose program the app found.
    pub found: Vec<Tool>,
    /// What a switch gives.
    pub switched: Result<Switched, Refusal>,
    /// Every account switched to, as the model named it.
    pub switched_to: Vec<String>,
    /// What the core's look at whether to switch Claude Code by itself gives.
    pub look: Result<AutoLooked, Refusal>,
    /// The share each look was asked at, in order.
    pub looked_at: Vec<u8>,
    /// What switching Claude Code by itself gives.
    pub auto: Result<AutoSwitched, Refusal>,
    /// The share each switch by itself was asked at, in order.
    pub auto_at: Vec<u8>,
    /// What runs each tool with its login in memory, by the tool's code, as the core's
    /// holder detection finds it in the process list.
    pub held: HashMap<String, Vec<Holding>>,
    /// The other apps on this machine.
    pub apps: Arc<StandInApps>,
    /// What giving up on an interrupted switch gives.
    pub abandoned: Result<Option<Abandoned>, Refusal>,
    /// What starting a sign-in's tool gives.
    pub starting: Result<(), Refusal>,
    /// What enrolling what a sign-in signed in to gives.
    pub enrolling: Result<Enrolled, Refusal>,
    /// Every sign-in started, as the model named the account.
    pub signed_in: Vec<String>,
    /// Every code typed back, to which sign-in.
    pub pasted: Vec<(u64, String)>,
    /// Every sign-in whose tool was stopped from outside.
    pub stopped: Vec<u64>,
    /// Every sign-in told whether to enrol, with what it was told.
    pub over: Vec<(u64, bool)>,
    /// What enrolling the login signed in now gives.
    pub enrolling_current: Result<Enrolled, Refusal>,
    /// What a rename gives.
    pub renaming: Result<(), Refusal>,
    /// What forgetting gives.
    pub forgetting: Result<(), Refusal>,
    /// What writing the account in use into its tool's config gives: what its tool warned of.
    pub updating_config: Result<Vec<Warning>, Refusal>,
    /// Every login enrolled as it is signed in now, as the model named it.
    pub enrolled: Vec<String>,
    /// Every rename, from and to, as the model named them.
    pub renamed: Vec<(String, String)>,
    /// Every account forgotten, as the model named it.
    pub forgot: Vec<String>,
    /// Every account written into its tool's config, as the model named it.
    pub configs_updated: Vec<String>,
    /// What was told about before the model started, as kept in Pitboard's directory.
    pub told_before: Told,
    /// Every record of what was told the model kept, in order.
    pub kept: Vec<Told>,
    /// Every notification posted, in order.
    pub posted: Vec<RunOutNotice>,
    /// What `app.json` held before the model started, where it was there.
    pub preferences_file: Option<String>,
    /// `app.json` is there and cannot be read, whatever it holds.
    pub preferences_unreadable: bool,
    /// What the app's earlier store held of its preferences.
    pub earlier: Option<EarlierPreferences>,
    /// Every set of preferences the model kept, in order.
    pub kept_preferences: Vec<Preferences>,
    /// The account windows' records, `windows.json`'s text, where it is there.
    pub windows_file: Option<String>,
    /// `windows.json` is there and the lane cannot read it, whatever it holds.
    pub windows_unreadable: bool,
    /// What the app's earlier store held of the account windows' records.
    pub windows_earlier: Option<Records>,
    /// The app named nowhere to keep the account windows' records.
    pub windows_nowhere: bool,
    /// The key this launch's records are kept under.
    pub windows_key: String,
    /// The Pitboard directories that are there, by their keys.
    pub directories: Vec<String>,
    /// Every set of this directory's records the model kept, in order.
    pub kept_windows: Vec<Entry>,
    /// What the scheduler has.
    pub scheduled: Schedule,
    /// How many times the schedule was read.
    pub schedule_reads: usize,
    /// How many times the scheduler was asked to take the schedule, and to take it away.
    pub installs: usize,
    pub uninstalls: usize,
    /// What the scheduler refuses each change with, while it does.
    pub refusing: Option<Refusal>,
    /// What repairing a schedule an older app wrote gives.
    pub repairs: Result<bool, Refusal>,
    pub repair_asks: usize,
    /// The command line inside this copy of the app: none, unless a test says.
    pub own: OwnCommandLine,
    /// What renewing gives, or what it is refused with.
    pub renewals: Result<Vec<Renewed>, Refusal>,
    pub renew_asks: usize,
    /// Doctor's checks.
    pub checks: Vec<Check>,
    pub doctor_asks: usize,
    /// What Pitboard has changed, oldest first, as the core's log keeps it.
    pub history: Vec<Change>,
    /// How many changes each read of the log asked for, in order.
    pub log_limits: Vec<u32>,
    /// The `pitboard` a terminal would run.
    pub command_line: FoundCommandLine,
    pub command_line_asks: usize,
}

impl Machine {
    pub(super) fn reading(answer: Result<Status, Refusal>) -> Machine {
        Machine {
            answer,
            offline: Ok(status(Vec::new())),
            changed: 0,
            readings: 0,
            found: vec![claude_code()],
            switched: already_active("work", Vec::new()),
            switched_to: Vec::new(),
            look: Ok(AutoLooked::Stands(watching("work"))),
            looked_at: Vec::new(),
            auto: Ok(watching("work")),
            auto_at: Vec::new(),
            held: HashMap::new(),
            apps: StandInApps::new(&[], true),
            abandoned: Ok(None),
            starting: Ok(()),
            enrolling: enrolled_as(EnrolledAs::SignedIn, Vec::new()),
            signed_in: Vec::new(),
            pasted: Vec::new(),
            stopped: Vec::new(),
            over: Vec::new(),
            enrolling_current: enrolled_as(EnrolledAs::Current, Vec::new()),
            renaming: Ok(()),
            forgetting: Ok(()),
            updating_config: Ok(Vec::new()),
            enrolled: Vec::new(),
            renamed: Vec::new(),
            forgot: Vec::new(),
            configs_updated: Vec::new(),
            told_before: Told::new(),
            kept: Vec::new(),
            posted: Vec::new(),
            preferences_file: None,
            preferences_unreadable: false,
            earlier: None,
            kept_preferences: Vec::new(),
            windows_file: None,
            windows_unreadable: false,
            windows_earlier: None,
            windows_nowhere: false,
            windows_key: DANA.into(),
            directories: vec![DANA.into()],
            kept_windows: Vec::new(),
            scheduled: Schedule::Absent,
            schedule_reads: 0,
            installs: 0,
            uninstalls: 0,
            refusing: None,
            repairs: Ok(false),
            repair_asks: 0,
            own: OwnCommandLine::default(),
            renewals: Ok(Vec::new()),
            renew_asks: 0,
            checks: Vec::new(),
            doctor_asks: 0,
            history: Vec::new(),
            log_limits: Vec::new(),
            command_line: FoundCommandLine::Nowhere,
            command_line_asks: 0,
        }
    }

    /// A copy of the app with a command line inside it that a schedule and a link would keep
    /// reaching.
    pub(super) fn with_a_command_line_inside(mut self) -> Machine {
        self.own = OwnCommandLine {
            inside: true,
            temporary: false,
            runs: true,
        };
        self
    }

    /// `windows.json` as the lane would read it now.
    fn windows_read(&self) -> std::io::Result<Option<String>> {
        if self.windows_unreadable {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "the test says it cannot be read",
            ));
        }
        Ok(self.windows_file.clone())
    }

    /// This launch's records as `windows.json` holds them now.
    pub(super) fn windows_records(&self) -> Entry {
        self.windows_file
            .as_deref()
            .and_then(Records::read)
            .map(|records| records.entry(&self.windows_key))
            .unwrap_or_default()
    }

    /// What the core answers `job` with.
    pub(super) fn answer(&mut self, job: Job) -> Answer {
        match job {
            Job::Read { ticket, .. } => {
                let (readings_before, changed_before) = (self.readings, self.changed);
                self.readings += 1;
                Answer::Read {
                    ticket,
                    readings_before,
                    changed_before,
                    read: self.answer.clone().map_err(|refused| refused.error()),
                }
            }
            Job::ReadOffline { why } => Answer::ReadOffline {
                why,
                read: self.offline.clone().map_err(|refused| refused.error()),
            },
            Job::Look => Answer::Looked {
                changed: self.changed,
                measured: self.readings,
            },
            Job::AskInstalled => Answer::Installed {
                tools: self.found.clone(),
            },
            // What holds the login, and quitting an app, by the lanes' own rules over what
            // the test says runs, with no time given to quit: an app quits when asked, or
            // is still running.
            Job::Holding { qualified } => {
                let provider = super::state::split(&qualified).0;
                let holdings = self.held.get(provider).map_or(&[][..], Vec::as_slice);
                Answer::Held {
                    question: lanes::holder(holdings, self.apps.as_ref(), &qualified),
                    qualified,
                }
            }
            Job::Quit { question } => Answer::Quit {
                outcome: lanes::quit(
                    self.apps.as_ref(),
                    &question.app_id,
                    Duration::ZERO,
                    Duration::ZERO,
                ),
                question,
            },
            Job::Switch { qualified, reopen } => {
                self.switched_to.push(qualified.clone());
                Answer::Switched {
                    qualified,
                    reopen,
                    done: self.switched.clone().map_err(|refused| refused.error()),
                }
            }
            Job::AutoLook { at } => {
                self.looked_at.push(at);
                Answer::AutoLooked {
                    looked: self.look.clone().map_err(|refused| refused.error()),
                }
            }
            Job::AutoSwitch { at } => {
                self.auto_at.push(at);
                Answer::AutoSwitched {
                    done: self.auto.clone().map_err(|refused| refused.error()),
                }
            }
            Job::Open { location } => {
                let _ = self.apps.reopen(location);
                Answer::Opened
            }
            Job::Abandon => {
                Answer::Abandoned(self.abandoned.clone().map_err(|refused| refused.error()))
            }
            // A sign-in's thread answers once its tool has started, as here, then with what
            // the tool says, which a test hands on itself, and once it has stopped.
            Job::SignIn { id, qualified } => {
                self.signed_in.push(qualified);
                Answer::SignInStarted {
                    id,
                    started: self.starting.clone().map_err(|refused| refused.error()),
                }
            }
            Job::PasteCode { id, code } => {
                self.pasted.push((id, code));
                Answer::Pasted
            }
            // Answered as a stop that found nothing to stop, which leaves the sign-in's thread
            // to say it has let go of the core's one sign-in at a time. A test gives the
            // lane's answer of a stop that stopped a tool still speaking itself.
            Job::StopSignIn { id } => {
                self.stopped.push(id);
                Answer::Stopped
            }
            Job::Enrol {
                provider,
                name,
                from,
            } => {
                self.enrolled.push(format!("{provider}/{name}"));
                Answer::Enrolled {
                    provider,
                    from,
                    done: self
                        .enrolling_current
                        .clone()
                        .map_err(|refused| refused.error()),
                }
            }
            Job::Rename {
                provider,
                label,
                to,
                from,
            } => {
                self.renamed
                    .push((format!("{provider}/{label}"), to.clone()));
                Answer::Renamed {
                    provider,
                    label,
                    to,
                    from,
                    done: self.renaming.clone().map_err(|refused| refused.error()),
                }
            }
            Job::Forget { qualified } => {
                self.forgot.push(qualified.clone());
                Answer::Forgot {
                    qualified,
                    done: self.forgetting.clone().map_err(|refused| refused.error()),
                }
            }
            Job::UpdateConfig { qualified } => {
                self.configs_updated.push(qualified);
                Answer::ConfigUpdated(
                    self.updating_config
                        .clone()
                        .map_err(|refused| refused.error()),
                )
            }
            // The account windows' records by the lane's own rules, over a file in memory.
            Job::LoadKept => Answer::Kept {
                told: self.told_before.clone(),
                preferences: (!self.preferences_unreadable).then(|| {
                    Preferences::kept(self.preferences_file.as_deref(), self.earlier.as_ref())
                }),
                windows: if self.windows_nowhere {
                    None
                } else {
                    records::load(
                        &self.windows_read(),
                        &self.windows_key,
                        self.windows_earlier.as_ref(),
                    )
                },
            },
            Job::KeepWindows { entry, write } => {
                if let Some(text) = records::kept(
                    &self.windows_read(),
                    &self.windows_key,
                    self.windows_earlier.as_ref(),
                    &entry,
                ) {
                    self.windows_file = Some(text);
                }
                self.kept_windows.push(entry);
                Answer::WindowsKept { write }
            }
            Job::CheckShared { stores } => Answer::SharedChecked {
                shared: records::shared(
                    &self.windows_read(),
                    &self.windows_key,
                    self.windows_earlier.as_ref(),
                    &stores,
                    |directory| self.directories.iter().any(|there| there == directory),
                ),
                stores,
            },
            Job::KeepPreferences { preferences } => {
                self.preferences_file = preferences.text();
                self.kept_preferences.push(preferences);
                Answer::Saved
            }
            Job::KeepTold { told } => {
                self.kept.push(told);
                Answer::Saved
            }
            Job::Post { notice } => {
                self.posted.push(notice);
                Answer::Posted
            }
            Job::SignInOver { id, enrol } => {
                self.over.push((id, enrol));
                if enrol {
                    Answer::SignInFinished {
                        id,
                        done: self.enrolling.clone().map_err(|refused| refused.error()),
                    }
                } else {
                    Answer::SignInStopped { id }
                }
            }
            Job::RepairSchedule => {
                self.repair_asks += 1;
                Answer::Repaired {
                    repaired: self.repairs.clone().map_err(|refused| refused.error()),
                    own: self.own,
                }
            }
            Job::ReadSchedule { after_change } => {
                self.schedule_reads += 1;
                Answer::ScheduleRead {
                    schedule: Some(self.scheduled.clone()),
                    own: self.own,
                    after_change,
                }
            }
            // By the lane's own rule, over a scheduler that writes nothing anywhere.
            Job::SetSchedule { on } => Answer::ScheduleSet {
                own: self.own,
                outcome: lanes::set_schedule(on, self.own, |on| {
                    if on {
                        self.installs += 1;
                    } else {
                        self.uninstalls += 1;
                    }
                    if let Some(refused) = &self.refusing {
                        return Err(refused.error());
                    }
                    self.scheduled = if on {
                        Schedule::Installed {
                            path: PLIST.into(),
                            every_seconds: 86_400,
                        }
                    } else {
                        Schedule::Absent
                    };
                    Ok(())
                }),
            },
            Job::Renew => {
                self.renew_asks += 1;
                Answer::Renewed {
                    renewals: self.renewals.clone().map_err(|refused| refused.error()),
                }
            }
            Job::Check => {
                self.doctor_asks += 1;
                Answer::Checked {
                    checks: Some(self.checks.clone()),
                }
            }
            Job::ReadLog { limit } => {
                self.log_limits.push(limit);
                let newest = self
                    .history
                    .len()
                    .saturating_sub(usize::try_from(limit).unwrap_or(usize::MAX));
                Answer::Logged {
                    changes: self.history[newest..].to_vec(),
                }
            }
            Job::FindCommandLine => {
                self.command_line_asks += 1;
                Answer::CommandLineFound {
                    found: self.command_line.clone(),
                    own: self.own,
                }
            }
        }
    }
}

/// The Pitboard directory of a test's app, as the account windows' records key it.
pub(super) const DANA: &str = "/Users/dana/.pitboard";

/// Where a scheduler keeps the renewal job, as the core names it on a Mac. Never written.
pub(super) const PLIST: &str = "/Users/x/Library/LaunchAgents/com.usepitboard.renew.plist";

/// A check as doctor reports one.
pub(super) fn check(code: &str, name: &str, level: Level, detail: &str, advice: &str) -> Check {
    Check {
        code: code.into(),
        name: name.into(),
        level,
        detail: detail.into(),
        advice: advice.into(),
    }
}

/// A change as the core's log keeps one.
pub(super) fn change(at: &str, caller: &str, verb: &str, subject: &str, outcome: &str) -> Change {
    Change {
        at: at.into(),
        caller: caller.into(),
        verb: verb.into(),
        subject: subject.into(),
        outcome: outcome.into(),
    }
}

/// A parked login renewed or not, as the core reports it.
pub(super) fn renewed(label: &str, provider: &str, outcome: &str) -> Renewed {
    Renewed {
        label: label.into(),
        provider: provider.into(),
        outcome: outcome.into(),
    }
}

/// The model's state driven by hand. Each message is handed to `State::apply` with the time
/// it arrived, and each job that says to run waits for the test to answer it: all of them in
/// turn, as the Swift model awaited each call, or one at a time in whatever order a test
/// wants.
pub(super) struct Hand {
    pub state: State,
    pub now: Now,
    /// Jobs asked for and not yet answered, oldest first.
    pending: VecDeque<Job>,
    /// Every job asked for, in order.
    pub asked: Vec<Job>,
}

impl Hand {
    pub(super) fn new() -> Hand {
        Hand {
            state: State::new(Cadence::APP),
            now: Now {
                running: Duration::ZERO,
                epoch_ms: 1_800_000_000_000,
            },
            pending: VecDeque::new(),
            asked: Vec::new(),
        }
    }

    fn took(&mut self, jobs: Vec<Job>) -> Vec<Job> {
        self.asked.extend(jobs.iter().cloned());
        self.pending.extend(jobs.iter().cloned());
        jobs
    }

    pub(super) fn send(&mut self, intent: Intent) -> Vec<Job> {
        let jobs = self.state.apply(Msg::Intent(intent), self.now);
        self.took(jobs)
    }

    /// Hands the model what a job came to.
    pub(super) fn give(&mut self, answer: Answer) -> Vec<Job> {
        let jobs = self.state.apply(Msg::Done(answer), self.now);
        self.took(jobs)
    }

    /// The actor waking for a timer.
    pub(super) fn tick(&mut self) -> Vec<Job> {
        let jobs = self.state.apply(Msg::Tick, self.now);
        self.took(jobs)
    }

    /// The change poll, for a test that must not wait for a timer.
    pub(super) fn look(&mut self) -> Vec<Job> {
        let jobs = self.state.look_now();
        self.took(jobs)
    }

    pub(super) fn later(&mut self, by: Duration) {
        self.now.running += by;
        self.now.epoch_ms += i64::try_from(by.as_millis()).expect("a short while");
    }

    /// The oldest job not yet answered, which the test now answers itself.
    pub(super) fn next(&mut self) -> Job {
        self.pending
            .pop_front()
            .expect("a job waiting for its answer")
    }

    /// The oldest job not yet answered that is `which`, which the test now answers itself
    /// while the others wait.
    pub(super) fn take(&mut self, which: fn(&Job) -> bool) -> Job {
        let at = self
            .pending
            .iter()
            .position(which)
            .expect("such a job waiting for its answer");
        self.pending.remove(at).expect("a job where it was found")
    }

    /// Answers every job waiting from `machine`, except those that are `which`, and every
    /// job those answers lead to.
    pub(super) fn run_but(&mut self, machine: &mut Machine, which: fn(&Job) -> bool) {
        let mut kept = VecDeque::new();
        while let Some(job) = self.pending.pop_front() {
            if which(&job) {
                kept.push_back(job);
                continue;
            }
            let answer = machine.answer(job);
            self.give(answer);
        }
        self.pending = kept;
    }

    /// Answers every job from `machine`, and every job those answers lead to, oldest first.
    pub(super) fn run(&mut self, machine: &mut Machine) {
        while let Some(job) = self.pending.pop_front() {
            let answer = machine.answer(job);
            self.give(answer);
        }
    }

    /// A read, as the timer's would be, and everything it leads to.
    pub(super) fn refresh(&mut self, machine: &mut Machine) {
        self.send(Intent::Refresh { asked: false });
        self.run(machine);
    }

    /// The change poll, and everything it leads to.
    pub(super) fn notice(&mut self, machine: &mut Machine) {
        self.look();
        self.run(machine);
    }

    /// What an app is shown now, its clock times read in UTC.
    pub(super) fn shown(&self) -> Snapshot {
        crate::present::present(&self.state, self.now.epoch(), &crate::present::testing::Utc)
    }

    /// How many of the jobs asked for so far are `which`.
    pub(super) fn count(&self, which: fn(&Job) -> bool) -> usize {
        self.asked.iter().filter(|job| which(job)).count()
    }

    pub(super) fn pending(&self) -> usize {
        self.pending.len()
    }
}

pub(super) fn fresh_read(job: &Job) -> bool {
    matches!(job, Job::Read { fresh: true, .. })
}

pub(super) fn any_read(job: &Job) -> bool {
    matches!(job, Job::Read { .. })
}

pub(super) fn offline_read(job: &Job) -> bool {
    matches!(job, Job::ReadOffline { .. })
}

pub(super) fn installed_ask(job: &Job) -> bool {
    matches!(job, Job::AskInstalled)
}

pub(super) fn a_look(job: &Job) -> bool {
    matches!(job, Job::Look)
}

/// A look for changes or a read of either kind: what a test leaves waiting while it answers
/// a change, to answer them in an order of its own.
pub(super) fn a_look_or_a_read(job: &Job) -> bool {
    matches!(job, Job::Look | Job::Read { .. } | Job::ReadOffline { .. })
}

pub(super) fn a_switch(job: &Job) -> bool {
    matches!(job, Job::Switch { .. })
}

pub(super) fn holding_ask(job: &Job) -> bool {
    matches!(job, Job::Holding { .. })
}

/// A machine of a test's own for the real core: a home in a scratch directory, Claude Code's
/// keychain, the vault and the scheduler in memory, and Anthropic scripted. Nothing of the
/// machine running the tests is read: no environment, no `PATH`, no login shell.
pub(super) struct World {
    root: PathBuf,
    pub host: Arc<MemoryHost>,
    pub api: Arc<ScriptedApi>,
    ctx: Context,
    /// The command line inside the app, where a test gives it one.
    helper: Option<PathBuf>,
    /// Where the app looks for the `pitboard` a terminal would run: the test's own places
    /// alone, never this machine's.
    places: Vec<PathBuf>,
    /// The login shell's `PATH`, as the app looked in it, where a test says one.
    search_path: Option<String>,
}

impl World {
    pub(super) fn new(name: &str) -> World {
        let root = std::env::temp_dir().join(format!(
            "pitboard-model-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a scratch home");
        let host = MemoryHost::new();
        let api = ScriptedApi::new();
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root.join(".pitboard"))
            .with_codex_home(root.join(".codex").to_string_lossy().into_owned())
            .with_user("tester".into())
            .with_search_path(String::new())
            .with_memory_stores(Arc::clone(&host))
            .with_scripted_api(Arc::clone(&api))
            .with_caller("app".into());
        World {
            root,
            host,
            api,
            ctx,
            helper: None,
            places: Vec::new(),
            search_path: None,
        }
    }

    /// The app's core over this machine, made from its parts as `AppCore::for_app` makes it
    /// from what the app was started with, with Claude Code's program found.
    pub(super) fn core(&self) -> Arc<AppCore> {
        let ctx = self.ctx.clone();
        let (helper, places, search_path) = (
            self.helper.clone(),
            self.places.clone(),
            self.search_path.clone(),
        );
        Arc::new(AppCore::asking(
            move || {
                let made = Made {
                    core: service::Pitboard::new(ctx.clone()),
                    found: vec![ProviderId::Claude],
                    search_path: search_path.clone(),
                    helper: helper.clone(),
                    command_line_places: places.clone(),
                };
                (made, false)
            },
            crate::ASK_AGAIN_AFTER,
        ))
    }

    /// An app on this machine with a command line inside it, in the scratch home, which a
    /// schedule runs and a link would reach, as `build-app.sh` puts one in a copy of the app.
    /// Where it is.
    #[cfg(unix)]
    pub(super) fn app_with_a_command_line(&mut self) -> PathBuf {
        self.app_with_a_command_line_in("")
    }

    /// The same, with the app in `dir` of the scratch home: under `AppTranslocation/…`, the
    /// temporary copy macOS runs an app from where it was downloaded.
    #[cfg(unix)]
    pub(super) fn app_with_a_command_line_in(&mut self, dir: &str) -> PathBuf {
        let helper = self
            .root
            .join(dir)
            .join("Pitboard.app/Contents/Helpers/pitboard");
        a_program_at(&helper);
        self.ctx = self.ctx.clone().with_schedule_program(helper.clone());
        self.helper = Some(helper.clone());
        helper
    }

    /// Where the app looks for the `pitboard` a terminal would run, after the login shell's
    /// `PATH`: `places`, and nothing of this machine's. For the test that makes an app with a
    /// command line inside it, as its own helpers are.
    #[cfg(unix)]
    pub(super) fn looks_for_pitboard_in(
        &mut self,
        places: Vec<PathBuf>,
        search_path: Option<String>,
    ) {
        self.places = places;
        self.search_path = search_path;
    }

    /// A directory of the scratch home.
    pub(super) fn dir(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    /// Another front end on the same machine, as the command line in a terminal is.
    /// What this machine's core is made with.
    pub(super) fn ctx(&self) -> &Context {
        &self.ctx
    }

    pub(super) fn elsewhere(&self) -> service::Pitboard {
        service::Pitboard::new(self.ctx.clone().with_caller("cli".into()))
    }

    /// Claude Code signed in as `who` and enrolled as `label`, with its five-hour window
    /// `percent` used, as Anthropic answers.
    pub(super) fn enrolled(&self, label: &str, who: &str, percent: f64) {
        self.signed_in(who, percent);
        self.elsewhere()
            .enroll_current(label)
            .expect("the account signed in, enrolled");
    }

    /// Claude Code signed in as `who`, with its five-hour window `percent` used, and not
    /// enrolled.
    pub(super) fn signed_in(&self, who: &str, percent: f64) {
        let login = self.claude_login(who, percent);
        self.host.live().plant(&live_service(&self.ctx), &login);
        std::fs::write(
            self.root.join(".claude.json"),
            format!(
                r#"{{"oauthAccount":{{"accountUuid":"{who}","emailAddress":"{who}@example.com","organizationUuid":"org-{who}"}}}}"#
            ),
        )
        .expect("Claude Code's config");
    }

    /// `who` signed in to Claude Code privately and enrolled as `label`, with its five-hour
    /// window `percent` used, which parks its login and leaves the account in use signed in,
    /// as `pitboard enroll <label> --sign-in` leaves it. No `claude` runs: the sign-in is
    /// planted as one that finished.
    pub(super) fn parked(&self, label: &str, who: &str, percent: f64) {
        let login = pitboard_core::testing::signed_in(
            &self.ctx,
            ProviderId::Claude,
            &self.claude_login(who, percent),
        )
        .expect("a sign-in");
        self.elsewhere()
            .enroll_signed_in(label, login)
            .expect("the account signed in privately, enrolled");
    }

    /// A switch to Claude Code's `label` that nothing can finish, as the core leaves one.
    /// Claude Code's keychain locks as the switch writes, so the switch cannot tell what it
    /// wrote and keeps its record and every copy. Claude Code then renews the login it has,
    /// which matches neither side the record kept, and its session expires, so Anthropic
    /// will not say whose it is either.
    pub(super) fn stuck_switching_to(&self, label: &str) {
        let live = live_service(&self.ctx);
        self.host
            .live()
            .fault(&live, pitboard_core::testing::Fault::LocksOnWrite);
        let locked = self.elsewhere().switch_to(label);
        self.host.live().heal_all();
        assert_eq!(
            locked.expect_err("the keychain locked").error.code(),
            "switch_unverified"
        );
        let renewed = self.claude_login_as("renewed", "access-renewed-since", 0.0);
        self.api.token_trouble(
            "access-renewed-since",
            pitboard_core::testing::Trouble::Unauthorized,
        );
        self.host.live().plant(&live, &renewed);
    }

    /// A stand-in for `claude`, the compiled one the tests start in place of every program,
    /// which this machine's core runs as `claude auth login` from now on. It writes what
    /// 2.1.289 writes before it opens the browser, then reads each line typed back as the
    /// register's `sign_in_output` and `sign_in_takes_another_code` hold: one that is not
    /// `<code>#<state>` with both halves it refuses on stderr and reads on, and the first that
    /// is it takes, and ends signed in. It stores no login: `signed_in_privately` plants the
    /// one it would have.
    ///
    /// It writes down its process id before it says anything, and every line typed back to
    /// it, each file whole: under a name of its own, then moved into place, so a test reading
    /// one as it is written never finds it half there. Written in place, its process id was
    /// read empty, as CI met on Linux (run 37437853042).
    ///
    /// The stand-in runs on every system. The tests that use it, and [`StandIn`], are still
    /// macOS's and Linux's, since whether it runs is asked of the shell's `kill -0`; W19 of
    /// the Windows work asks Windows.
    #[cfg(unix)]
    pub(super) fn claude_stand_in(&mut self) -> StandIn {
        self.a_claude_stand_in(false)
    }

    /// The same stand-in, whose first run starts a program of its own before it says
    /// anything, which outlives it with its output held open until the test lets go of it,
    /// as `codex login` does once Codex's npm launcher is killed: @openai/codex 0.149.1's
    /// `bin/codex.js` starts the program with its own output and hands on only `SIGINT`,
    /// `SIGTERM` and `SIGHUP`. Reading what the tool says then ends only once that program
    /// has, long after the tool was stopped and waited for.
    #[cfg(unix)]
    pub(super) fn claude_stand_in_holding_its_output(&mut self) -> StandIn {
        self.a_claude_stand_in(true)
    }

    #[cfg(unix)]
    fn a_claude_stand_in(&mut self, holding: bool) -> StandIn {
        use pitboard_core::testing::stand_in::{self, Script, Step};
        let bin = self.root.join("bin");
        std::fs::create_dir_all(&bin).expect("a scratch bin");
        let stand_in = StandIn {
            pid: self.root.join("claude.pid"),
            typed: self.root.join("claude.typed"),
            holder: self.root.join("holder.pid"),
            holding: self.root.join("holding"),
        };
        let mut steps = vec![Step::WritesItsPid(stand_in.pid.clone())];
        // Another of itself, which holds the output it was started with for as long as
        // `holding` is there, and whose process id it writes down whole. Only the first run
        // starts one, so each test has one to let go of.
        if holding {
            steps.push(Step::HoldsOutput {
                while_present: stand_in.holding.clone(),
                pid: stand_in.holder.clone(),
            });
        }
        steps.extend([
            Step::Says("Opening browser to sign in\u{2026}\n".into()),
            Step::Says(
                "If the browser didn't open, visit: \
                 https://claude.com/cai/oauth/authorize?code=true&state=s\n"
                    .into(),
            ),
            Step::Says("Paste code here if prompted > ".into()),
            Step::ReadsCodes {
                typed: stand_in.typed.clone(),
                takes: "Login successful.\n".into(),
                refuses: "Invalid code. Please make sure the full code was copied.\n".into(),
            },
        ]);
        let program = bin.join("claude");
        let script = Script::Plays {
            args: Some(vec!["auth".into(), "login".into()]),
            steps,
        };
        stand_in::install(&program, &script).unwrap_or_else(|e| panic!("no stand-in: {e}"));
        self.ctx = self.ctx.clone().with_claude_program(program);
        stand_in
    }

    /// `who` signed in to Claude Code privately, as `claude auth login` leaves it for Pitboard:
    /// a login stored in the keychain item Claude Code makes for Pitboard's private sign-in
    /// directory, with Anthropic scripted to say whose it is and that its five-hour window is
    /// `percent` used. `access` names its access token, so a new login of an account already
    /// here can be told from the one it has. Planted once the sign-in has started, since
    /// starting one clears whatever a sign-in before it left there.
    #[cfg(unix)]
    pub(super) fn signed_in_privately(&self, who: &str, access: &str, percent: f64) {
        let dir = self.root.join(".pitboard").join("signin");
        let service = pitboard_core::testing::service_for_dir(&dir.to_string_lossy());
        let login = self.claude_login_as(who, access, percent);
        self.host.live().plant(&service, &login);
    }

    /// Whether the login Claude Code has in use is the one whose access token is `access`.
    #[cfg(unix)]
    pub(super) fn claude_code_uses(&self, access: &str) -> bool {
        self.host
            .live()
            .peek(&live_service(&self.ctx))
            .is_some_and(|login| login.contains(&format!("\"accessToken\":\"{access}\"")))
    }

    /// A Claude Code login of `who`, in the shape Claude Code stores one, with Anthropic
    /// scripted to say whose it is and that its five-hour window is `percent` used.
    fn claude_login(&self, who: &str, percent: f64) -> String {
        self.claude_login_as(who, &format!("access-{who}"), percent)
    }

    /// The same, with its access token named `access`.
    fn claude_login_as(&self, who: &str, access: &str, percent: f64) -> String {
        let now = epoch_now();
        let access = access.to_owned();
        self.api.owned_by(
            &access,
            pitboard_core::api::Owner {
                account_uuid: who.into(),
                email: format!("{who}@example.com"),
                organization_uuid: format!("org-{who}"),
            },
        );
        self.api.using(
            &access,
            pitboard_core::usage::Snapshot {
                windows: vec![pitboard_core::usage::Window {
                    kind: "session".into(),
                    scope: None,
                    percent,
                    resets_at: Some(now + 3_600),
                    is_active: true,
                    severity: None,
                    length_seconds: Some(18_000),
                }],
                observed_at: Some(now),
                answered_at: Some(now),
                lists_every_limit: true,
                source: pitboard_core::usage::Source::Live,
            },
        );
        format!(
            r#"{{"claudeAiOauth":{{"accessToken":"{access}","refreshToken":"refresh-{who}","expiresAt":{},"refreshTokenExpiresAt":{},"scopes":["user:profile","user:inference"]}}}}"#,
            (now + 3_600) * 1_000,
            (now + 30 * 86_400) * 1_000,
        )
    }

    /// Claude Code's write lock taken, as a session writing its login holds it, until what
    /// this returns is dropped: proper-lockfile's directory, `.storage-write.lock` in Claude
    /// Code's config directory, where pitboard-core's `Claude::write_lock` and
    /// `lock::acquire` put it. A switch waits on it for about seven and a half seconds, then
    /// refuses.
    pub(super) fn claude_code_writing(&self) -> Writing {
        let held = self.root.join(".claude").join(".storage-write.lock");
        std::fs::create_dir_all(&held).expect("Claude Code's lock");
        Writing(held)
    }

    /// Codex signed in as `who` and enrolled as `label`, as `codex login` and then
    /// `pitboard enroll codex/<label>` in a terminal leave it.
    pub(super) fn codex_enrolled(&self, label: &str, who: &str) {
        let home = self.root.join(".codex");
        std::fs::create_dir_all(&home).expect("a scratch Codex home");
        self.host
            .file_at(home.join("auth.json"))
            .plant("auth.json", &self.codex_login(who));
        self.elsewhere()
            .enroll_current(&format!("codex/{label}"))
            .expect("the Codex account signed in, enrolled");
    }

    /// `who` signed in to Codex privately and enrolled as `label`, which parks its login and
    /// leaves the account in use signed in, as `pitboard enroll codex/<label> --sign-in`
    /// leaves it. No `codex` runs: the sign-in is planted as one that finished.
    pub(super) fn codex_parked(&self, label: &str, who: &str) {
        let login =
            pitboard_core::testing::signed_in(&self.ctx, ProviderId::Codex, &self.codex_login(who))
                .expect("a sign-in");
        self.elsewhere()
            .enroll_signed_in(&format!("codex/{label}"), login)
            .expect("the Codex account signed in privately, enrolled");
    }

    /// A Codex login of the ChatGPT account `who`, in the shape `codex login` writes one,
    /// with OpenAI scripted to answer for it.
    fn codex_login(&self, who: &str) -> String {
        let now = epoch_now();
        let id_token = unsigned_token(&format!(
            r#"{{"email":"{who}@example.com","exp":{},"https://api.openai.com/auth":{{"chatgpt_account_id":"{who}","chatgpt_user_id":"user-{who}","chatgpt_plan_type":"pro"}}}}"#,
            now + 3_600
        ));
        let access = unsigned_token(&format!(
            r#"{{"exp":{},"for":"refresh-{who}"}}"#,
            now + 10 * 86_400
        ));
        self.api.using(
            &access,
            pitboard_core::usage::Snapshot {
                windows: Vec::new(),
                observed_at: Some(now),
                answered_at: Some(now),
                lists_every_limit: false,
                source: pitboard_core::usage::Source::Live,
            },
        );
        format!(
            r#"{{"auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{{"id_token":"{id_token}","access_token":"{access}","refresh_token":"refresh-{who}","account_id":"{who}"}},"last_refresh":"2026-10-01T08:00:00Z"}}"#
        )
    }

    /// Pitboard's directory on this machine.
    pub(super) fn pitboard_dir(&self) -> PathBuf {
        self.root.join(".pitboard")
    }

    /// Every account was last put in use an hour ago. Enrolling the account signed in puts
    /// it in use, and nothing switches away from an account by itself for five minutes after.
    pub(super) fn in_use_an_hour_ago(&self) {
        let path = self.pitboard_dir().join("state.json");
        let mut state: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("the account index"))
                .expect("JSON");
        for account in state["accounts"].as_array_mut().expect("accounts") {
            if let Some(at) = account["last_used_at"].as_i64() {
                account["last_used_at"] = serde_json::json!(at - 3_600);
            }
        }
        std::fs::write(&path, state.to_string()).expect("the account index written");
    }

    /// Says the account index was written `seconds` later than it was. The index's time is
    /// kept to the second, so a write within the same second as the last one looks like
    /// none; a test says it came later rather than waiting a second.
    pub(super) fn index_written_later(&self, seconds: u64) {
        self.index_written(|written| written + Duration::from_secs(seconds));
    }

    /// Anthropic answers that `who`'s five-hour window is `percent` used from now on, measured
    /// now: a newer reading than the last, which the next read records.
    pub(super) fn measures(&self, who: &str, percent: f64) {
        let _ = self.claude_login(who, percent);
    }

    /// Holds the lock usage readings are written under, as a session's status line holds it
    /// while it records, until what this returns is dropped. A read with a newer reading to
    /// record waits for it, and the lane of reads with it.
    pub(super) fn recording(&self) -> std::fs::File {
        let dir = self.pitboard_dir();
        std::fs::create_dir_all(&dir).expect("Pitboard's directory");
        let lock = std::fs::File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join("usage.lock"))
            .expect("the readings' lock");
        lock.lock().expect("held");
        lock
    }

    /// Says the account index was written `seconds` earlier than it was, so that the next
    /// write moves its time, in whatever second it is made.
    pub(super) fn index_written_earlier(&self, seconds: u64) {
        self.index_written(|written| written - Duration::from_secs(seconds));
    }

    fn index_written(&self, when: impl FnOnce(std::time::SystemTime) -> std::time::SystemTime) {
        let index = self.root.join(".pitboard").join("state.json");
        let file = std::fs::File::options()
            .write(true)
            .open(&index)
            .expect("the account index");
        let written = file
            .metadata()
            .and_then(|meta| meta.modified())
            .expect("when it was written");
        file.set_modified(when(written)).expect("another time");
    }
}

/// What a test's stand-in for `claude` left: its process id, every line typed back to it,
/// and, where it holds its output open, the process id of what holds it and the file it holds
/// it for.
#[cfg(unix)]
pub(super) struct StandIn {
    pid: PathBuf,
    typed: PathBuf,
    holder: PathBuf,
    holding: PathBuf,
}

#[cfg(unix)]
impl StandIn {
    /// Every line typed back to it, in order, as it last wrote them down whole.
    pub(super) fn typed(&self) -> Vec<String> {
        std::fs::read_to_string(&self.typed)
            .map(|typed| typed.lines().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    /// The process id the last one to write one wrote, waited for until one has: a test
    /// asking before the stand-in has written it would otherwise take a stand-in that has not
    /// started yet for one that has stopped. A test that starts it again waits for the new
    /// one to say something, which it does only once it has written its own.
    fn pid(&self) -> u32 {
        written_pid(&self.pid)
    }

    /// Whether the program its first run started still holds its output open, as the shell's
    /// own `kill -0` says. Waits for its process id to have been written.
    pub(super) fn holds_output(&self) -> bool {
        running(written_pid(&self.holder))
    }

    /// Lets the program its first run started end, and its output close with it.
    pub(super) fn let_go_of_output(&self) {
        let _ = std::fs::remove_file(&self.holding);
    }

    /// Whether the last one to start is still running, as the shell's own `kill -0` says of
    /// the process id it wrote, which is never this test's or anybody else's to stop. Waits
    /// for that process id to have been written.
    pub(super) fn is_running(&self) -> bool {
        running(self.pid())
    }
}

/// Nothing a stand-in started outlives its test holding its output open.
#[cfg(unix)]
impl Drop for StandIn {
    fn drop(&mut self) {
        self.let_go_of_output();
    }
}

/// The process id a stand-in wrote whole at `path`, waited for until it has: a test asking
/// before would otherwise take a process that has not started yet for one that has stopped.
#[cfg(unix)]
fn written_pid(path: &std::path::Path) -> u32 {
    let asked = std::time::Instant::now();
    loop {
        match std::fs::read_to_string(path) {
            Ok(pid) => {
                return pid
                    .trim()
                    .parse()
                    .unwrap_or_else(|_| panic!("a process id written whole: {pid:?}"));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                assert!(
                    asked.elapsed() < Duration::from_secs(20),
                    "the stand-in never wrote its process id"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("its process id unread: {error}"),
        }
    }
}

/// Whether the process `pid` runs, as the shell's own `kill -0` says.
#[cfg(unix)]
fn running(pid: u32) -> bool {
    std::process::Command::new("/bin/sh")
        .args(["-c", &format!("kill -0 {pid} 2>/dev/null")])
        .status()
        .is_ok_and(|status| status.success())
}

/// Claude Code's write lock, held until this is dropped.
pub(super) struct Writing(PathBuf);

impl Drop for Writing {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.0);
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Asked whether it runs before it has written its process id, the stand-in is waited for,
/// and not taken for one that has stopped: this process stands in for it here, and writes
/// its own id the way the stand-in does, whole, a moment after the question.
#[test]
#[cfg(unix)]
fn whether_the_stand_in_runs_is_answered_once_it_has_written_its_process_id() {
    let world = World::new("stand-in-pid");
    let stand_in = StandIn {
        pid: world.root.join("claude.pid"),
        typed: world.root.join("claude.typed"),
        holder: world.root.join("holder.pid"),
        holding: world.root.join("holding"),
    };
    let pid = stand_in.pid.clone();
    let writing = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        let part = pid.with_extension("pid.part");
        std::fs::write(&part, format!("{}\n", std::process::id())).expect("written");
        std::fs::rename(&part, &pid).expect("moved into place");
    });
    assert!(stand_in.is_running(), "this process, once its id is there");
    writing.join().expect("the writer");
}

/// A program at `path` that anybody may run, with the directories it needs: the compiled
/// stand-in, there to be found, which refuses to do anything were it run.
#[cfg(unix)]
pub(super) fn a_program_at(path: &std::path::Path) {
    use pitboard_core::testing::stand_in::{self, Script};
    let dir = path.parent().expect("a program's directory");
    std::fs::create_dir_all(dir).expect("a scratch directory");
    stand_in::install(
        path,
        &Script::refusing("a test's program, never meant to run\n"),
    )
    .unwrap_or_else(|e| panic!("no program at {}: {e}", path.display()));
}

fn epoch_now() -> i64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("after 1970")
        .as_secs();
    i64::try_from(now).expect("before 2262")
}

/// A token in the shape a real one has, with a signature nothing checks, as Codex's login
/// carries its claims: three parts of unpadded base64url.
fn unsigned_token(payload: &str) -> String {
    [r#"{"alg":"RS256"}"#, payload, "not a real signature"]
        .map(|part| base64url(part.as_bytes()))
        .join(".")
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let held = chunk.iter().enumerate().fold(0u32, |held, (at, &byte)| {
            held | (u32::from(byte) << (16 - 8 * at))
        });
        for at in 0..(chunk.len() * 8).div_ceil(6) {
            let sextet = usize::try_from((held >> (18 - 6 * at)) & 0x3f).expect("six bits");
            out.push(char::from(ALPHABET[sextet]));
        }
    }
    out
}
