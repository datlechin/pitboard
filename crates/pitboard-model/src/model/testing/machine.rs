use super::platform::StandInApps;
use super::records::{
    CLAUDE, DANA, PLIST, Refusal, already_active, claude_code, claude_desktop, claude_holding,
    desktop_account, enrolled_as, refusal, status, watching,
};
use crate::account_windows::records::{self, Entry, Records};
use crate::model::EarlierPreferences;
use crate::model::advice::Told;
use crate::model::lanes;
use crate::model::preferences::Preferences;
use crate::model::state::{Answer, Cadence, Job, Msg, Now, State};
use crate::model::{AppControl, Intent, RunOutNotice, Snapshot};
use crate::{
    AutoLooked, AutoSwitched, Change, Check, Doctored, Enrolled, EnrolledAs, FoundCommandLine,
    GaveUp, Holding, OwnCommandLine, PitboardError, Schedule, Status, Switched, Uninstalled,
    WarmRefused, Warning,
};
use pitboard_core::provider::ProviderId;
use pitboard_core::state::Key;
use pitboard_core::switch::warm::Apart;
use pitboard_core::switch::{Left, Stowed};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

// Claude Desktop with work in use and home parked, Claude open where `open`.
pub(crate) fn desktop_machine(open: bool) -> Stub {
    let mut machine = Stub::reading(Ok(status(vec![
        desktop_account("work", true),
        desktop_account("home", false),
    ])));
    machine.found = vec![claude_code(), claude_desktop()];
    machine
        .held
        .insert(ProviderId::Desktop, vec![claude_holding()]);
    machine.apps = StandInApps::new(if open { &[CLAUDE] } else { &[] }, true);
    machine
}

fn warm_refused((told_apart, refused): (Apart, Refusal)) -> WarmRefused {
    WarmRefused {
        told_apart,
        error: Box::new(refused.error()),
    }
}

/// What the core answers, as a test says, the way the Swift tests' stub answered.
pub(crate) struct Stub {
    /// What a read gives.
    pub answer: Result<Status, Refusal>,
    /// What reading what is already known gives.
    pub offline: Result<Status, Refusal>,
    /// When the account index was last written, in milliseconds.
    pub changed: i64,
    /// When the usage readings were last written. A read moves it, as the core's does: what
    /// it measured is recorded.
    pub readings: i64,
    /// The tools whose program the app found.
    pub found: Vec<ProviderId>,
    /// What a switch gives.
    pub switched: Result<Switched, Refusal>,
    /// Every account switched to, as the model named it.
    pub switched_to: Vec<String>,
    /// Each account a replacement was asked for, in order, answered as a switch is.
    pub replaced_with: Vec<String>,
    /// What the core's look at whether to switch Claude Code by itself gives.
    pub look: Result<AutoLooked, Refusal>,
    /// The share each look was asked at, in order.
    pub looked_at: Vec<u8>,
    /// What switching Claude Code by itself gives.
    pub auto: Result<AutoSwitched, Refusal>,
    /// The share each switch by itself was asked at, in order.
    pub auto_at: Vec<u8>,
    /// What runs each tool with its login in memory, as the core's holder detection finds it
    /// in the process list.
    pub held: HashMap<ProviderId, Vec<Holding>>,
    /// The other apps on this machine.
    pub apps: Arc<StandInApps>,
    /// What giving up on an interrupted switch gives.
    pub abandoned: Result<GaveUp, Refusal>,
    /// What starting a sign-in's tool gives.
    pub starting: Result<(), Refusal>,
    // the account the core parked for a sign-in inside an app, as SwitchTo takes it
    pub put_back: Option<Key>,
    // whether, after an app has quit, the core still finds part of it running
    pub lingers: bool,
    // what the core says of an app's data folder: whole, and a move of its login waiting
    pub whole: bool,
    pub moving: bool,
    /// What enrolling what a sign-in signed in to gives.
    pub enrolling: Result<Enrolled, Refusal>,
    /// Every sign-in started, as `codex/work` for the account signed in to again, and `codex/`
    /// for a new one.
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
    /// What looking at the file behind Claude Code's store gives.
    pub leftover: Result<Option<Left>, Refusal>,
    /// What putting that file away gives.
    pub stowing: Result<Stowed, Refusal>,
    /// Every file put away, by what it was confirmed as.
    pub stowed_seen: Vec<String>,
    /// Every login enrolled as it is signed in now, named as `signed_in` names a sign-in.
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
    // why the app's system refuses each notification, while it does
    pub post_refused: Option<String>,
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
    /// Doctor's checks.
    pub checks: Vec<Check>,
    // what Copy Report copies
    pub report: String,
    pub doctor_asks: usize,
    /// What Pitboard has changed, oldest first, as the core's log keeps it.
    pub history: Vec<Change>,
    /// How many changes each read of the log asked for, in order.
    pub log_limits: Vec<u32>,
    /// The `pitboard` a terminal would run.
    pub command_line: FoundCommandLine,
    pub command_line_asks: usize,
    /// What the core's files say of warming up accounts, as every read gives it, and why they
    /// could not be read where `warm_refused` says.
    pub warm: Option<pitboard_core::duty::warm::Overview>,
    pub warm_refused: Option<Refusal>,
    pub warming: Result<pitboard_core::switch::warm::WarmUp, (Apart, Refusal)>,
    /// Every warm-up asked for, in order: `None` for one by itself, or the account asked
    /// for now.
    pub warmed: Vec<Option<String>>,
    /// Every change to which accounts are warmed up, in order, and the span.
    pub warm_turned: Vec<(String, bool)>,
    pub warm_ranges: Vec<(u16, u16)>,
    // what uninstalling gives, and how many times it was asked
    pub removing: Result<Uninstalled, Refusal>,
    pub removals: usize,
    // every glance written, in order
    pub glances: Vec<String>,
}

/// What the core says of warming up where nothing has been set: the default span, nobody.
pub(crate) fn no_warm() -> pitboard_core::duty::warm::Overview {
    pitboard_core::duty::warm::Overview {
        range: pitboard_core::duty::warm::Range::DEFAULT,
        accounts: Vec::new(),
        halted: None,
    }
}

impl Stub {
    pub(crate) fn reading(answer: Result<Status, Refusal>) -> Stub {
        Stub {
            answer,
            offline: Ok(status(Vec::new())),
            changed: 0,
            readings: 0,
            found: vec![claude_code()],
            switched: already_active("work", Vec::new()),
            switched_to: Vec::new(),
            replaced_with: Vec::new(),
            look: Ok(AutoLooked::Stands(Box::new(watching("work")))),
            looked_at: Vec::new(),
            auto: Ok(watching("work")),
            auto_at: Vec::new(),
            held: HashMap::new(),
            apps: StandInApps::new(&[], true),
            abandoned: Ok(GaveUp::default()),
            starting: Ok(()),
            put_back: None,
            lingers: false,
            whole: true,
            moving: false,
            enrolling: enrolled_as("travel", EnrolledAs::SignedIn, Vec::new()),
            signed_in: Vec::new(),
            pasted: Vec::new(),
            stopped: Vec::new(),
            over: Vec::new(),
            enrolling_current: enrolled_as("work", EnrolledAs::Current, Vec::new()),
            renaming: Ok(()),
            forgetting: Ok(()),
            updating_config: Ok(Vec::new()),
            leftover: Ok(None),
            stowing: Err(refusal(
                "left_login_changed",
                "nothing was left to put away",
                Vec::new(),
            )),
            stowed_seen: Vec::new(),
            enrolled: Vec::new(),
            renamed: Vec::new(),
            forgot: Vec::new(),
            configs_updated: Vec::new(),
            told_before: Told::new(),
            kept: Vec::new(),
            posted: Vec::new(),
            post_refused: None,
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
            checks: Vec::new(),
            report: String::new(),
            doctor_asks: 0,
            history: Vec::new(),
            log_limits: Vec::new(),
            command_line: FoundCommandLine::Nowhere,
            command_line_asks: 0,
            warm: None,
            warm_refused: None,
            warming: Ok(pitboard_core::switch::warm::WarmUp::Nothing),
            warmed: Vec::new(),
            warm_turned: Vec::new(),
            warm_ranges: Vec::new(),
            removing: Ok(Uninstalled::Removed),
            removals: 0,
            glances: Vec::new(),
        }
    }

    /// A copy of the app with a command line inside it that a schedule and a link would keep
    /// reaching.
    pub(crate) fn with_a_command_line_inside(mut self) -> Stub {
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
    pub(crate) fn windows_records(&self) -> Entry {
        self.windows_file
            .as_deref()
            .and_then(Records::read)
            .map(|records| records.entry(&self.windows_key))
            .unwrap_or_default()
    }

    /// What the core's files say of warming up, as a read gives it.
    fn warm_overview(&self) -> Result<pitboard_core::duty::warm::Overview, PitboardError> {
        match &self.warm_refused {
            Some(refused) => Err(refused.error()),
            None => Ok(self.warm.clone().unwrap_or_else(no_warm)),
        }
    }

    /// What the core answers `job` with.
    pub(crate) fn answer(&mut self, job: Job) -> Answer {
        match job {
            Job::Read { ticket, .. } => {
                let (readings_before, changed_before) = (self.readings, self.changed);
                self.readings += 1;
                Answer::Read {
                    ticket,
                    readings_before,
                    changed_before,
                    read: self.answer.clone().map_err(|refused| refused.error()),
                    warm: self.warm_overview(),
                }
            }
            Job::ReadOffline { why } => Answer::ReadOffline {
                why,
                read: self.offline.clone().map_err(|refused| refused.error()),
                warm: self.warm_overview(),
            },
            Job::WarmUp => {
                self.warmed.push(None);
                let mut done = self.warming.clone().map_err(warm_refused);
                // As the core's: where nothing is due it says what its files say.
                if let Ok(pitboard_core::switch::warm::WarmUp::Stands(standing)) = &mut done
                    && let Some(warm) = &self.warm
                {
                    **standing = warm.clone();
                }
                // As the core's: once a period has started, nothing is due until it resets.
                if matches!(
                    done,
                    Ok(pitboard_core::switch::warm::WarmUp::Started { .. })
                ) {
                    self.warming = Ok(pitboard_core::switch::warm::WarmUp::Stands(Box::new(
                        self.warm.clone().unwrap_or_else(no_warm),
                    )));
                }
                Answer::WarmedUp { asked: None, done }
            }
            Job::WarmNow { account } => {
                self.warmed.push(Some(account.qualified()));
                Answer::WarmedUp {
                    asked: Some(account),
                    done: self.warming.clone().map_err(warm_refused),
                }
            }
            Job::SetWarm { account, on } => {
                let qualified = account.qualified();
                self.warm_turned.push((qualified.clone(), on));
                if let Some(overview) = &mut self.warm {
                    for choice in &mut overview.accounts {
                        if choice.qualified == qualified {
                            choice.on = on;
                        }
                    }
                }
                Answer::WarmSet {
                    done: self.warm_overview(),
                }
            }
            Job::SetWarmRange { from, to } => {
                self.warm_ranges.push((from, to));
                let mut overview = self.warm.clone().unwrap_or_else(no_warm);
                if let Some(range) = pitboard_core::duty::warm::Range::new(from, to) {
                    overview.range = range;
                }
                self.warm = Some(overview.clone());
                Answer::WarmSet { done: Ok(overview) }
            }
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
            Job::Holding { account, then } => {
                let holdings = (self.held.get(&account.provider)).map_or(&[][..], Vec::as_slice);
                Answer::Held {
                    question: lanes::holder(holdings, self.apps.as_ref(), &account),
                    account,
                    then,
                }
            }
            Job::Quit { question, then } => Answer::Quit {
                outcome: lanes::quit(
                    self.apps.as_ref(),
                    &question.app_id,
                    Duration::ZERO,
                    Duration::ZERO,
                    || self.lingers,
                    pitboard_core::host::awake,
                ),
                question,
                then,
            },
            Job::Switch { account, reopen } => {
                self.switched_to.push(account.qualified());
                Answer::Switched {
                    account,
                    reopen,
                    done: self.switched.clone().map_err(|refused| refused.error()),
                }
            }
            Job::Replace { account } => {
                self.replaced_with.push(account.qualified());
                Answer::Switched {
                    account,
                    reopen: None,
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
            // By the lane's own rule, over what the test says of the app's folder.
            Job::Open {
                location,
                tool,
                front,
            } => {
                let may = if front { !self.moving } else { self.whole };
                let opened = may && self.apps.reopen(location, front).is_ok();
                Answer::Opened {
                    tool,
                    front,
                    opened,
                }
            }
            Job::Abandon => {
                Answer::Abandoned(self.abandoned.clone().map_err(|refused| refused.error()))
            }
            // A sign-in's thread answers once its tool has started, as here, then with what
            // the tool says, which a test hands on itself, and once it has stopped.
            Job::SignIn { id, tool, again } => {
                self.signed_in
                    .push(Key::new(tool, again.unwrap_or_default()).qualified());
                Answer::SignInStarted {
                    id,
                    started: self.starting.clone().map_err(|refused| refused.error()),
                    put_back: self.put_back.clone(),
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
                tool,
                again,
                from,
                reopen,
            } => {
                self.enrolled
                    .push(Key::new(tool, again.as_deref().unwrap_or_default()).qualified());
                Answer::Enrolled {
                    tool,
                    again,
                    from,
                    reopen,
                    done: self
                        .enrolling_current
                        .clone()
                        .map_err(|refused| refused.error()),
                }
            }
            Job::Rename { account, to, from } => {
                self.renamed.push((account.qualified(), to.clone()));
                Answer::Renamed {
                    account,
                    to,
                    from,
                    done: self.renaming.clone().map_err(|refused| refused.error()),
                }
            }
            Job::Forget { account } => {
                self.forgot.push(account.qualified());
                Answer::Forgot {
                    account,
                    done: self.forgetting.clone().map_err(|refused| refused.error()),
                }
            }
            Job::LookLeft => {
                Answer::LookedLeft(self.leftover.clone().map_err(|refused| refused.error()))
            }
            Job::Stow { seen, from } => {
                self.stowed_seen.push(seen);
                Answer::Stowed {
                    from,
                    done: self
                        .stowing
                        .clone()
                        .map(|stowed| (stowed, Vec::new()))
                        .map_err(|refused| refused.error()),
                }
            }
            Job::UpdateConfig { account } => {
                self.configs_updated.push(account.qualified());
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
                let title = notice.title.clone();
                self.posted.push(notice);
                Answer::Posted {
                    title,
                    refused: self.post_refused.clone(),
                }
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
            Job::Check => {
                self.doctor_asks += 1;
                Answer::Checked {
                    doctored: Some(Doctored {
                        checks: self.checks.clone(),
                        report: self.report.clone(),
                    }),
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
            Job::KeepGlance { text, reload } => {
                self.glances.push(text);
                Answer::GlanceKept { reload }
            }
            Job::Uninstall => {
                self.removals += 1;
                Answer::Uninstalled(self.removing.clone().map_err(|refused| refused.error()))
            }
        }
    }
}

/// The model's state driven by hand. Each message is handed to `State::apply` with the time
/// it arrived, and each job that says to run waits for the test to answer it: all of them in
/// turn, as the Swift model awaited each call, or one at a time in whatever order a test
/// wants.
pub(crate) struct Hand {
    pub state: State,
    pub now: Now,
    /// Jobs asked for and not yet answered, oldest first.
    pending: VecDeque<Job>,
    /// Every job asked for, in order.
    pub asked: Vec<Job>,
}

impl Hand {
    pub(crate) fn new() -> Hand {
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

    pub(crate) fn send(&mut self, intent: Intent) -> Vec<Job> {
        let jobs = self.state.apply(Msg::Intent(intent), self.now);
        self.took(jobs)
    }

    /// Hands the model what a job came to.
    pub(crate) fn give(&mut self, answer: Answer) -> Vec<Job> {
        let jobs = self.state.apply(Msg::Done(answer), self.now);
        self.took(jobs)
    }

    /// The actor waking for a timer.
    pub(crate) fn tick(&mut self) -> Vec<Job> {
        let jobs = self.state.apply(Msg::Tick, self.now);
        self.took(jobs)
    }

    /// The change poll, for a test that must not wait for a timer.
    pub(crate) fn look(&mut self) -> Vec<Job> {
        let jobs = self.state.look_now();
        self.took(jobs)
    }

    pub(crate) fn later(&mut self, by: Duration) {
        self.now.running += by;
        self.now.epoch_ms += i64::try_from(by.as_millis()).expect("a short while");
    }

    /// The oldest job not yet answered, which the test now answers itself.
    pub(crate) fn next(&mut self) -> Job {
        self.pending
            .pop_front()
            .expect("a job waiting for its answer")
    }

    /// The oldest job not yet answered that is `which`, which the test now answers itself
    /// while the others wait.
    pub(crate) fn take(&mut self, which: fn(&Job) -> bool) -> Job {
        let at = self
            .pending
            .iter()
            .position(which)
            .expect("such a job waiting for its answer");
        self.pending.remove(at).expect("a job where it was found")
    }

    /// Answers every job waiting from `machine`, except those that are `which`, and every
    /// job those answers lead to.
    pub(crate) fn run_but(&mut self, machine: &mut Stub, which: fn(&Job) -> bool) {
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
    pub(crate) fn run(&mut self, machine: &mut Stub) {
        while let Some(job) = self.pending.pop_front() {
            let answer = machine.answer(job);
            self.give(answer);
        }
    }

    /// A read, as the timer's would be, and everything it leads to.
    pub(crate) fn refresh(&mut self, machine: &mut Stub) {
        self.send(Intent::Refresh { asked: false });
        self.run(machine);
    }

    /// The change poll, and everything it leads to.
    pub(crate) fn notice(&mut self, machine: &mut Stub) {
        self.look();
        self.run(machine);
    }

    // What the actor does after each message: the glance kept where it changed, then shown.
    pub(crate) fn publish(&mut self) -> Snapshot {
        let utc = crate::present::testing::Utc;
        let text = crate::present::glance(&self.state, self.now.epoch(), &utc);
        if let Some(job) = self.state.glanced(text) {
            self.took(vec![job]);
        }
        self.shown()
    }

    /// What an app is shown now, its clock times read in UTC.
    pub(crate) fn shown(&self) -> Snapshot {
        crate::present::present(&self.state, self.now.epoch(), &crate::present::testing::Utc)
    }

    // What a Mac's app is shown now, whichever system runs the test.
    pub(crate) fn on_a_mac(&self) -> Snapshot {
        crate::present::present_on(
            pitboard_core::host::Os::MacOs,
            &self.state,
            self.now.epoch(),
            &crate::present::testing::Utc,
        )
    }

    pub(crate) fn on_windows(&self) -> Snapshot {
        crate::present::present_on(
            pitboard_core::host::Os::Windows,
            &self.state,
            self.now.epoch(),
            &crate::present::testing::Utc,
        )
    }

    /// How many of the jobs asked for so far are `which`.
    pub(crate) fn count(&self, which: fn(&Job) -> bool) -> usize {
        self.asked.iter().filter(|job| which(job)).count()
    }

    pub(crate) fn pending(&self) -> usize {
        self.pending.len()
    }

    // The code behind what the sheet that is up says went wrong, which no snapshot carries.
    pub(crate) fn sheet_failure_code(&self) -> Option<&str> {
        self.state.sheet_failure.as_ref()?.code.as_deref()
    }
}

pub(crate) fn fresh_read(job: &Job) -> bool {
    matches!(job, Job::Read { fresh: true, .. })
}

pub(crate) fn any_read(job: &Job) -> bool {
    matches!(job, Job::Read { .. })
}

pub(crate) fn offline_read(job: &Job) -> bool {
    matches!(job, Job::ReadOffline { .. })
}

pub(crate) fn installed_ask(job: &Job) -> bool {
    matches!(job, Job::AskInstalled)
}

pub(crate) fn a_look(job: &Job) -> bool {
    matches!(job, Job::Look)
}

/// A look for changes or a read of either kind: what a test leaves waiting while it answers
/// a change, to answer them in an order of its own.
pub(crate) fn a_look_or_a_read(job: &Job) -> bool {
    matches!(job, Job::Look | Job::Read { .. } | Job::ReadOffline { .. })
}

pub(crate) fn a_switch(job: &Job) -> bool {
    matches!(job, Job::Switch { .. })
}

pub(crate) fn holding_ask(job: &Job) -> bool {
    matches!(job, Job::Holding { .. })
}
