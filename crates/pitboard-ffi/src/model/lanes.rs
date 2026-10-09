//! Where the model's jobs run, and the thread that tells an app what changed.
//!
//! Each lane is a thread of its own that takes jobs in the order they come and answers each
//! with [`Msg::Done`], as the Swift service's queues did. `reads` is the one queue of reads,
//! so a look for changes waits behind a read on the network exactly as it did there.
//! `changes` makes one change at a time, so a read answers while a switch waits on a lock or
//! on a service. `processes` asks what holds a tool's login, which lists processes and
//! nothing else, and asks the app's own code about other apps, waiting while one is given its
//! time to quit. `discovery` asks which tools are installed, which can wait on the person's
//! login shell, and is needed before the first read can say anything useful about it, so it
//! waits behind nothing. `kept` reads and writes what the model keeps: in Pitboard's
//! directory, and the account windows' records in the app's own, read again as each write
//! is made, so another copy of the app's records stay as it wrote them. A lane ends once the
//! actor has gone and its last job is done.
//!
//! Each sign-in has a thread of its own, as the Swift model's SignInCalls made one for each
//! call that waits on a person in a browser: it starts the tool's sign-in, hands on what the
//! tool says until it stops, and then enrols what it signed in to, or lets it go, as it is
//! told. Typing a code back and stopping a sign-in run on `sign_in_calls`, apart from the
//! thread reading, which they would otherwise wait behind until the browser came back. The
//! model asks for the next sign-in's thread only once the one before has let go of the core's
//! one sign-in at a time, which a stop that stopped its tool says, and its thread's last
//! answer: that comes only once the tool's output has closed too, which a program the tool
//! started can hold open long after it. Once the actor has gone, that lane stops every
//! sign-in still under way, so each thread ends, and `PitboardModel::shutdown` waits until
//! it has.

use super::advice::Told;
use super::machine::Scheduled;
use super::preferences::Preferences;
use super::state::{Answer, Cadence, Job, Msg, QuitOutcome, split};
use super::{
    AppControl, EarlierPreferences, ModelListener, Notifications, QuitQuestion, Snapshot,
    WindowsLaunch,
};
use crate::account_windows::records::{self, Records};
use crate::{AppCore, Holding, Remedy, SignInSession};
use pitboard_core::app::AppFile;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Which lane a job runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lane {
    Reads,
    Changes,
    Processes,
    Discovery,
    /// The sign-in's own thread.
    SignIn,
    /// Typing a code back to a sign-in and stopping one.
    SignInCalls,
    /// What the model keeps of its own, read and written: in Pitboard's directory, and the
    /// account windows' records in the app's own.
    Kept,
    /// Posting through the app's system's notifications.
    Notify,
}

impl Job {
    /// The lane `self` runs on, as the Swift service put each call on its queue.
    pub(crate) fn lane(&self) -> Lane {
        match self {
            // The schedule, doctor and the log are read where the Swift service read them, on
            // its queue of reads, and changes to the schedule and renewals are made on its
            // queue of changes, one at a time with every other.
            Job::Read { .. }
            | Job::ReadOffline { .. }
            | Job::Look
            | Job::AutoLook { .. }
            | Job::ReadSchedule { .. }
            | Job::Check
            | Job::ReadLog { .. }
            | Job::LookLeft => Lane::Reads,
            Job::Switch { .. }
            | Job::AutoSwitch { .. }
            | Job::Abandon
            | Job::Enrol { .. }
            | Job::Rename { .. }
            | Job::Forget { .. }
            | Job::UpdateConfig { .. }
            | Job::Stow { .. }
            | Job::RepairSchedule
            | Job::SetSchedule { .. }
            | Job::Renew => Lane::Changes,
            Job::Holding { .. } | Job::Quit { .. } | Job::Open { .. } => Lane::Processes,
            // Where the login shell's `PATH` is asked, as the Swift service asked it for
            // `searchPath`, apart from every queue of its own.
            Job::AskInstalled | Job::FindCommandLine => Lane::Discovery,
            Job::SignIn { .. } | Job::SignInOver { .. } => Lane::SignIn,
            Job::PasteCode { .. } | Job::StopSignIn { .. } => Lane::SignInCalls,
            Job::LoadKept
            | Job::KeepTold { .. }
            | Job::KeepPreferences { .. }
            | Job::KeepWindows { .. }
            | Job::CheckShared { .. } => Lane::Kept,
            Job::Post { .. } => Lane::Notify,
        }
    }
}

/// The lanes, as the actor hands them jobs. Dropped, they end.
pub(crate) struct Lanes {
    reads: Sender<Job>,
    changes: Sender<Job>,
    processes: Sender<Job>,
    discovery: Sender<Job>,
    sign_in_calls: Sender<Job>,
    kept: Sender<Job>,
    notify: Sender<Job>,
    /// What a sign-in's own thread works with and answers to.
    worker: Arc<Worker>,
    answers: Sender<Msg>,
}

impl Lanes {
    /// The sign-ins under way, for the model to wait for as it shuts down: apart from the
    /// worker, which holds the core, so that the model holding them holds nothing else.
    pub(crate) fn sign_ins(&self) -> Arc<SignIns> {
        Arc::clone(&self.worker.sign_ins)
    }
}

/// What the model keeps that the app handed it or named: its preferences as its earlier
/// store held them, and where the account windows' records are.
pub(crate) struct Keeping {
    pub(crate) earlier: Option<EarlierPreferences>,
    pub(crate) windows: Option<RecordsFile>,
}

/// The account windows' records: `windows.json` in the directory the app named, the key this
/// launch's are kept under in it, and what the app's earlier store held of them, the file's
/// stand-in until the file is there.
pub(crate) struct RecordsFile {
    pub(crate) path: PathBuf,
    pub(crate) key: String,
    pub(crate) earlier: Option<Records>,
}

impl RecordsFile {
    pub(crate) fn of(launch: WindowsLaunch) -> RecordsFile {
        RecordsFile {
            path: Path::new(&launch.directory).join(records::FILE),
            key: launch.key,
            earlier: launch.earlier.as_ref().map(Records::earlier),
        }
    }

    /// The file as it is now: its text, `None` where it is not there, and an error where it
    /// is there and cannot be read.
    fn read(&self) -> std::io::Result<Option<String>> {
        pitboard_core::app::read_file(&self.path)
    }
}

impl Lanes {
    /// Starts the lanes over `core`, `apps` and `notifications`, answering to `answers`,
    /// giving an app as long to quit as `cadence` says, with what the model keeps as
    /// `keeping` says.
    pub(crate) fn open(
        core: Arc<AppCore>,
        apps: Arc<dyn AppControl>,
        notifications: Arc<dyn Notifications>,
        keeping: Keeping,
        cadence: Cadence,
        answers: &Sender<Msg>,
    ) -> Lanes {
        let worker = Arc::new(Worker {
            core,
            apps,
            notifications,
            earlier: keeping.earlier,
            windows: keeping.windows,
            quit_within: cadence.quit_within,
            quit_checked_every: cadence.quit_checked_every,
            sign_ins: Arc::default(),
        });
        Lanes {
            reads: lane("pitboard-reads", &worker, answers, |_| {}),
            changes: lane("pitboard-changes", &worker, answers, |_| {}),
            processes: lane("pitboard-processes", &worker, answers, |_| {}),
            discovery: lane("pitboard-discovery", &worker, answers, |_| {}),
            // Once the actor has gone, nothing is left to say a sign-in is over, so every one
            // still under way is stopped.
            sign_in_calls: lane("pitboard-sign-in-calls", &worker, answers, |worker| {
                worker.sign_ins.close();
            }),
            kept: lane("pitboard-kept", &worker, answers, |_| {}),
            notify: lane("pitboard-notify", &worker, answers, |_| {}),
            worker,
            answers: answers.clone(),
        }
    }

    /// Hands `job` to its lane, and waits for nothing.
    pub(crate) fn run(&self, job: Job) {
        let lane = match job.lane() {
            Lane::Reads => &self.reads,
            Lane::Changes => &self.changes,
            Lane::Processes => &self.processes,
            Lane::Discovery => &self.discovery,
            Lane::SignInCalls => &self.sign_in_calls,
            Lane::Kept => &self.kept,
            Lane::Notify => &self.notify,
            Lane::SignIn => return self.sign_in(job),
        };
        // A lane that has gone has nothing left to answer to.
        let _ = lane.send(job);
    }

    /// Starts a sign-in on a thread of its own, or tells one whether to enrol.
    fn sign_in(&self, job: Job) {
        match job {
            Job::SignIn { id, qualified } => self.start_sign_in(id, qualified, |run| {
                std::thread::Builder::new()
                    .name("pitboard-sign-in".into())
                    .spawn(run)
                    .map(drop)
            }),
            Job::SignInOver { id, enrol } => self.worker.sign_ins.tell(id, enrol),
            // Every other job has a lane of its own.
            _ => {}
        }
    }

    /// Starts the sign-in `id` to `qualified` on the thread `spawn` starts. One the system
    /// cannot start comes to nothing, as one that stops with a panic does: this runs on the
    /// actor's own thread, which a panic would stop, and the model with it.
    fn start_sign_in(
        &self,
        id: u64,
        qualified: String,
        spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<()>,
    ) {
        let (tell, told) = channel();
        self.worker.sign_ins.begin(id, tell);
        let worker = Arc::clone(&self.worker);
        let answers = self.answers.clone();
        let lost = Job::SignIn {
            id,
            qualified: qualified.clone(),
        };
        let started = spawn(Box::new(move || {
            signing_in(&worker, &answers, id, qualified, &told);
        }));
        if started.is_err() {
            self.worker.sign_ins.end(id);
            let _ = self.answers.send(Msg::Done(Answer::Lost(lost)));
        }
    }
}

/// A lane named `name`, which does `closing` once the actor has gone and its last job is
/// done.
fn lane(
    name: &str,
    worker: &Arc<Worker>,
    answers: &Sender<Msg>,
    closing: fn(&Worker),
) -> Sender<Job> {
    let (jobs, taken) = channel::<Job>();
    let worker = Arc::clone(worker);
    let answers = answers.clone();
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            for job in taken {
                // The core does not panic on purpose, and a panic there must not stop every
                // job after it, so it is taken as the job coming to nothing.
                let answer = catch_unwind(AssertUnwindSafe(|| worker.work(job.clone())))
                    .unwrap_or(Answer::Lost(job));
                if answers.send(Msg::Done(answer)).is_err() {
                    break;
                }
            }
            closing(&worker);
        })
        .expect("the system starts a thread for one of the model's lanes");
    jobs
}

/// A sign-in on a thread of its own, as `Job::SignIn` starts one. Starting the tool can wait
/// on the person's login shell, and reading what it says waits on a person in a browser.
///
/// Once the tool has stopped saying anything, it waits to be told whether to enrol what the
/// tool signed in to, which only the sign-in still under way is. A sign-in that is not is
/// stopped, which reaps a tool that ended by itself and stops one the model has gone from.
///
/// Its last answer, whichever way it goes, comes once nothing of it holds the core's one
/// sign-in at a time: its tool could not start, or has finished or been stopped and waited
/// for, or what it was doing came to nothing. The model starts the next sign-in then, unless
/// a stop from the lane of sign-in calls said so first: a tool stopped while reading what it
/// says goes on being read until its output closes, which a program it started can put off
/// for as long as that program runs.
fn signing_in(
    worker: &Worker,
    answers: &Sender<Msg>,
    id: u64,
    qualified: String,
    told: &Receiver<bool>,
) {
    let tell = |answer| answers.send(Msg::Done(answer)).is_ok();
    let session = match catch_unwind(AssertUnwindSafe(|| worker.core.sign_in(qualified.clone()))) {
        Ok(Ok(session)) => session,
        Ok(Err(error)) => {
            tell(Answer::SignInStarted {
                id,
                started: Err(error),
            });
            return worker.sign_ins.end(id);
        }
        Err(_) => {
            tell(Answer::Lost(Job::SignIn { id, qualified }));
            return worker.sign_ins.end(id);
        }
    };
    let enrol = worker.sign_ins.started(id, &session)
        && tell(Answer::SignInStarted {
            id,
            started: Ok(()),
        })
        && {
            // A read that stopped with a panic is taken as the tool having stopped saying
            // anything, so the sign-in is still over, one way or the other.
            let _ = catch_unwind(AssertUnwindSafe(|| {
                while let Some(text) = session.next_line() {
                    if !tell(Answer::SignInSaid { id, text }) {
                        break;
                    }
                }
            }));
            // Too late to stop it from here: what it signed in to may be being enrolled.
            worker.sign_ins.quiet(id);
            tell(Answer::SignInQuiet { id })
        }
        && told.recv().unwrap_or(false);
    if enrol {
        let answer = match catch_unwind(AssertUnwindSafe(|| session.finish())) {
            Ok(done) => Answer::SignInFinished { id, done },
            Err(_) => Answer::Lost(Job::SignInOver { id, enrol: true }),
        };
        tell(answer);
    } else {
        // A stop that panicked dropped what it held as it unwound, the core's one sign-in at
        // a time with it, and the next sign-in waits for this answer, so it is given
        // whatever the stop came to.
        let _ = catch_unwind(AssertUnwindSafe(|| session.cancel()));
        tell(Answer::SignInStopped { id });
    }
    worker.sign_ins.end(id);
}

/// The sign-ins under way, by id, which the actor's jobs and each sign-in's own thread
/// share: the session to type a code back to and to stop, from when its tool has started
/// until it stops saying anything, and the word its thread waits for after that.
#[derive(Default)]
pub(crate) struct SignIns {
    held: Mutex<Held>,
    /// Told each time a sign-in is stopped from here or its thread ends.
    changed: Condvar,
}

#[derive(Default)]
struct Held {
    /// Set once the model has gone, after which a sign-in that starts is stopped at once.
    closed: bool,
    running: HashMap<u64, Running>,
}

struct Running {
    session: Option<Arc<SignInSession>>,
    over: Option<Sender<bool>>,
    /// Its tool has been stopped and waited for from here, so nothing of it is left running
    /// but its thread, which may still be reading what the tool's output was held open by.
    stopped: bool,
}

impl SignIns {
    fn held(&self) -> MutexGuard<'_, Held> {
        // Every change here is whole whenever the lock is let go.
        self.held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The sign-in `id` is starting, and `over` is how its thread is told whether to enrol.
    pub(crate) fn begin(&self, id: u64, over: Sender<bool>) {
        self.held().running.insert(
            id,
            Running {
                session: None,
                over: Some(over),
                stopped: false,
            },
        );
    }

    /// The tool of the sign-in `id` has started as `session`, which a code can be typed back
    /// to and which can be stopped from now on. False once the model has gone, when it is to
    /// be stopped at once.
    pub(crate) fn started(&self, id: u64, session: &Arc<SignInSession>) -> bool {
        let mut held = self.held();
        if held.closed {
            return false;
        }
        match held.running.get_mut(&id) {
            Some(running) => {
                running.session = Some(Arc::clone(session));
                true
            }
            None => false,
        }
    }

    /// The tool of the sign-in `id` has stopped saying anything, and is no longer typed to or
    /// stopped from outside.
    pub(crate) fn quiet(&self, id: u64) {
        if let Some(running) = self.held().running.get_mut(&id) {
            running.session = None;
        }
    }

    /// The session of the sign-in `id`, while a code can be typed back to it.
    pub(crate) fn session(&self, id: u64) -> Option<Arc<SignInSession>> {
        self.held()
            .running
            .get(&id)
            .and_then(|running| running.session.clone())
    }

    /// Tells the thread of the sign-in `id` whether to enrol what its tool signed in to.
    pub(crate) fn tell(&self, id: u64, enrol: bool) {
        let over = self
            .held()
            .running
            .get_mut(&id)
            .and_then(|running| running.over.take());
        if let Some(over) = over {
            let _ = over.send(enrol);
        }
    }

    /// The thread of the sign-in `id` has ended.
    pub(crate) fn end(&self, id: u64) {
        self.held().running.remove(&id);
        self.changed.notify_all();
    }

    /// The model has gone: every sign-in still under way is stopped, and every thread
    /// waiting to be told whether to enrol is told not to, by its word going. Returns once
    /// each tool it stops has stopped and been waited for, and says so of each.
    pub(crate) fn close(&self) {
        let stopping: Vec<(u64, Arc<SignInSession>)> = {
            let mut held = self.held();
            held.closed = true;
            held.running
                .iter_mut()
                .filter_map(|(&id, running)| {
                    running.over = None;
                    running.session.take().map(|session| (id, session))
                })
                .collect()
        };
        for (id, session) in stopping {
            session.cancel();
            if let Some(running) = self.held().running.get_mut(&id) {
                running.stopped = true;
            }
            self.changed.notify_all();
        }
    }

    /// Waits, once closed, for no longer than `within`, until nothing a sign-in started may
    /// still be running: each sign-in's tool has been stopped from here, or its thread has
    /// ended. That covers one still starting, which stops its tool as it starts, one whose
    /// thread was waiting to be told whether to enrol, which stops its tool once told not
    /// to, and one enrolling what it signed in to. Whether every one did.
    pub(crate) fn stopped_within(&self, within: Duration) -> bool {
        let held = self.held();
        let (held, waited) = self
            .changed
            .wait_timeout_while(held, within, |held| {
                held.running.values().any(|running| !running.stopped)
            })
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        drop(held);
        !waited.timed_out()
    }
}

/// What every lane works with.
pub(crate) struct Worker {
    pub(crate) core: Arc<AppCore>,
    pub(crate) apps: Arc<dyn AppControl>,
    pub(crate) notifications: Arc<dyn Notifications>,
    pub(crate) earlier: Option<EarlierPreferences>,
    pub(crate) windows: Option<RecordsFile>,
    pub(crate) quit_within: Duration,
    pub(crate) quit_checked_every: Duration,
    pub(crate) sign_ins: Arc<SignIns>,
}

impl Worker {
    /// Does `job` with the core or the app's own code, either of which may block: the core
    /// on the keychain, a lock, the network or the login shell, and an app given its time to
    /// quit for as long as `quit_within`.
    pub(crate) fn work(&self, job: Job) -> Answer {
        let core = &self.core;
        match job {
            Job::Read { ticket, fresh } => {
                // Before the read: a session can record newer numbers, and a terminal can
                // switch, while the read waits on a service. One job, where the Swift awaited
                // each of the three calls on its queue of reads, so no look runs between
                // them.
                let readings_before = core.readings_changed_at();
                let changed_before = core.changed_at();
                Answer::Read {
                    ticket,
                    readings_before,
                    changed_before,
                    read: core.status(fresh),
                }
            }
            Job::ReadOffline { why } => Answer::ReadOffline {
                why,
                read: core.status_offline(),
            },
            Job::Look => Answer::Looked {
                changed: core.changed_at(),
                measured: core.readings_changed_at(),
            },
            Job::AskInstalled => Answer::Installed {
                tools: core.installed(),
            },
            Job::Holding { qualified } => {
                let holdings = core.holding(split(&qualified).0.to_owned());
                Answer::Held {
                    question: holder(&holdings, self.apps.as_ref(), &qualified),
                    qualified,
                }
            }
            Job::Quit { question } => Answer::Quit {
                outcome: quit(
                    self.apps.as_ref(),
                    &question.app_id,
                    self.quit_within,
                    self.quit_checked_every,
                ),
                question,
            },
            Job::Switch { qualified, reopen } => Answer::Switched {
                done: core.switch_to(qualified.clone()),
                qualified,
                reopen,
            },
            Job::AutoLook { at } => Answer::AutoLooked {
                looked: core.auto_look(at),
            },
            Job::AutoSwitch { at } => Answer::AutoSwitched {
                done: core.auto_switch(at),
            },
            Job::Open { location } => {
                // An app that cannot be opened again is the person's to open: nothing here
                // can mend it, and the switch is made either way.
                let _ = self.apps.reopen(location);
                Answer::Opened
            }
            Job::Abandon => Answer::Abandoned(core.abandon_recovery()),
            // Named with their tool, as AppModel.swift's `qualified(_:for:)` named them, so a
            // Codex login is enrolled as Codex's and not as a Claude Code account.
            Job::Enrol {
                provider,
                name,
                from,
            } => Answer::Enrolled {
                done: core.enroll_current(format!("{provider}/{name}")),
                provider,
                from,
            },
            Job::Rename {
                provider,
                label,
                to,
                from,
            } => Answer::Renamed {
                done: core.rename(format!("{provider}/{label}"), to.clone()),
                provider,
                label,
                to,
                from,
            },
            Job::Forget { qualified } => Answer::Forgot {
                done: core.forget(qualified.clone()),
                qualified,
            },
            Job::UpdateConfig { qualified } => Answer::ConfigUpdated(core.update_config(qualified)),
            Job::LookLeft => Answer::LookedLeft(core.left_login()),
            Job::Stow { seen, from } => Answer::Stowed {
                done: core.stow(&seen),
                from,
            },
            // Nothing kept, a record that is there and cannot be read, and one that does not
            // read as a record of what was told, are each nothing told. The record is written
            // whole the next time something is told, over one that could not be read too: at
            // worst a run-out is notified once more. Preferences that are there and cannot be
            // read are not none: they are left as they are, and never written over unread.
            // Nor are the account windows' records that are there and cannot be read as
            // records, whatever reads them.
            Job::LoadKept => Answer::Kept {
                told: core
                    .app_file(AppFile::Told)
                    .ok()
                    .flatten()
                    .and_then(|text| serde_json::from_str::<Told>(&text).ok())
                    .unwrap_or_default(),
                preferences: core
                    .app_file(AppFile::Preferences)
                    .ok()
                    .map(|text| Preferences::kept(text.as_deref(), self.earlier.as_ref())),
                windows: self
                    .windows
                    .as_ref()
                    .and_then(|file| records::load(&file.read(), &file.key, file.earlier.as_ref())),
            },
            // Read again as it is written, so what another copy of the app wrote of its own
            // directory's since is kept, and never written over where it cannot be read as
            // records.
            Job::KeepWindows { entry, write } => {
                if let Some(file) = &self.windows
                    && let Some(text) =
                        records::kept(&file.read(), &file.key, file.earlier.as_ref(), &entry)
                    && let Ok(permit) = core.permit()
                {
                    let _ = pitboard_core::app::write_file(permit, &file.path, &text);
                }
                Answer::WindowsKept { write }
            }
            Job::CheckShared { stores } => Answer::SharedChecked {
                shared: self.windows.as_ref().and_then(|file| {
                    records::shared(
                        &file.read(),
                        &file.key,
                        file.earlier.as_ref(),
                        &stores,
                        |directory| Path::new(directory).exists(),
                    )
                }),
                stores,
            },
            Job::KeepPreferences { preferences } => {
                if let Some(body) = preferences.text() {
                    let _ = core.keep_app_file(AppFile::Preferences, &body);
                }
                Answer::Saved
            }
            Job::KeepTold { told } => {
                if let Ok(body) = serde_json::to_string(&told) {
                    let _ = core.keep_app_file(AppFile::Told, &body);
                }
                Answer::Saved
            }
            Job::Post { notice } => {
                let _ = self.notifications.post(notice);
                Answer::Posted
            }
            Job::RepairSchedule => Answer::Repaired {
                repaired: core.schedule_repair(),
                own: core.own_command_line(),
            },
            Job::ReadSchedule { after_change } => Answer::ScheduleRead {
                schedule: core.schedule(),
                own: core.own_command_line(),
                after_change,
            },
            Job::SetSchedule { on } => {
                let own = core.own_command_line();
                Answer::ScheduleSet {
                    outcome: set_schedule(on, own, |on| {
                        if on {
                            core.schedule_install().map(drop)
                        } else {
                            core.schedule_uninstall().map(drop)
                        }
                    }),
                    own,
                }
            }
            Job::Renew => Answer::Renewed {
                renewals: core.renew(),
            },
            Job::Check => Answer::Checked {
                checks: core.doctor(),
            },
            Job::ReadLog { limit } => Answer::Logged {
                changes: core.log(limit),
            },
            Job::FindCommandLine => Answer::CommandLineFound {
                found: core.command_line(),
                own: core.own_command_line(),
            },
            // Typed back as the Swift model typed it, whatever comes of it: the tool says
            // itself whether it took the code.
            Job::PasteCode { id, code } => {
                if let Some(session) = self.sign_ins.session(id) {
                    let _ = session.paste(code);
                }
                Answer::Pasted
            }
            // A stop that stopped the tool has waited for it and let go of the core's one
            // sign-in at a time, so it says so, and the next sign-in starts without waiting
            // for the sign-in's own thread: that answers only once the tool's output has
            // closed, which a program the tool started can hold open long after the tool was
            // stopped, as `codex login` does once Codex's npm launcher is killed.
            Job::StopSignIn { id } => {
                if self
                    .sign_ins
                    .session(id)
                    .is_some_and(|session| session.cancel())
                {
                    Answer::SignInStopped { id }
                } else {
                    Answer::Stopped
                }
            }
            // A sign-in runs on a thread of its own, and never here.
            Job::SignIn { .. } | Job::SignInOver { .. } => Answer::Lost(job),
        }
    }
}

/// Turns daily renewal on or off through `change`, which asks the scheduler, as
/// MachineModel.swift's `setSchedule` did. Turning it on is refused before the scheduler is
/// asked where the command line inside this copy, `own`, is not one a schedule would keep
/// reaching, so nothing writes a schedule that fails every day without telling anyone.
/// Turning it off never is: that is how such a schedule is taken away.
pub(crate) fn set_schedule(
    on: bool,
    own: crate::OwnCommandLine,
    change: impl FnOnce(bool) -> Result<(), crate::PitboardError>,
) -> Scheduled {
    if on && !own.lasting() {
        return Scheduled::CannotSchedule;
    }
    match change(on) {
        Ok(()) => Scheduled::Done,
        Err(error) => Scheduled::Refused(error),
    }
}

/// An app on this machine running the tool `qualified` is of with its login in memory, that
/// Pitboard may quit and open again, as the question to ask about it. Asked of the core,
/// which read the process list into `holdings`, and only taken where the app is running:
/// what is left of one that has gone cannot be quit. An app whose own code cannot say
/// whether it runs is asked about, since the process list says it does.
pub(crate) fn holder(
    holdings: &[Holding],
    apps: &dyn AppControl,
    qualified: &str,
) -> Option<QuitQuestion> {
    holdings.iter().find_map(|held| match &held.remedy {
        Remedy::ReopenApp { app_id, name } => {
            let running = !matches!(apps.running(app_id.clone()), Ok(None));
            running.then(|| QuitQuestion {
                qualified: qualified.to_owned(),
                app_id: app_id.clone(),
                name: name.clone(),
            })
        }
        Remedy::Restart | Remedy::Run { .. } | Remedy::Do { .. } => None,
    })
}

/// Asks the app `app` to quit and waits until it has, for as long as `within`, asking every
/// `every` whether it still runs. One not running is not asked. An app whose own code fails
/// to answer is taken as still running, and one it fails to ask is still running, so nothing
/// is switched under it.
pub(crate) fn quit(
    apps: &dyn AppControl,
    app: &str,
    within: Duration,
    every: Duration,
) -> QuitOutcome {
    let copy = match apps.running(app.to_owned()) {
        Ok(Some(copy)) => copy,
        Ok(None) => return QuitOutcome::NotRunning,
        Err(_) => return QuitOutcome::StillRunning,
    };
    if apps.request_quit(app.to_owned()).is_err() {
        return QuitOutcome::StillRunning;
    }
    let deadline = Instant::now() + within;
    while !matches!(apps.running(app.to_owned()), Ok(None)) {
        let now = Instant::now();
        if now >= deadline {
            return QuitOutcome::StillRunning;
        }
        std::thread::sleep(every.min(deadline - now));
    }
    QuitOutcome::Quit { copy }
}

/// Starts the one thread that calls `listener`, with each snapshot sent to the returned
/// sender, in the order they were sent. One thread, so revisions arrive in the order they
/// were made, which several threads calling at once would not keep.
///
/// Where several are waiting, only the newest is told: an app shows the newest and drops
/// the rest. Once `stopped` is set nothing more is told. The thread ends once the sender has
/// gone, and drops the listener then.
pub(crate) fn notifier(
    listener: Arc<dyn ModelListener>,
    stopped: Arc<AtomicBool>,
) -> Sender<Snapshot> {
    let (tell, told) = channel::<Snapshot>();
    std::thread::Builder::new()
        .name("pitboard-notifier".into())
        .spawn(move || notify(listener.as_ref(), &told, &stopped))
        .expect("the system starts a thread for the model's listener");
    tell
}

fn notify(listener: &dyn ModelListener, told: &Receiver<Snapshot>, stopped: &AtomicBool) {
    while let Ok(mut snapshot) = told.recv() {
        while let Ok(newer) = told.try_recv() {
            snapshot = newer;
        }
        if stopped.load(Ordering::SeqCst) {
            return;
        }
        // What the app could not take in is the app's to say; the next snapshot comes all
        // the same.
        let _ = listener.changed(snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Intent;
    use crate::model::state::{Cadence, Now, State};
    use crate::model::testing::{CHATGPT_CODEX, Posted, StandInApps, World, chatgpt_holding};
    #[cfg(unix)]
    use pitboard_core::testing::fs as files;

    const CHATGPT: &str = crate::model::testing::CHATGPT;

    fn worker(core: Arc<AppCore>, apps: Arc<StandInApps>) -> Worker {
        Worker {
            core,
            apps,
            notifications: Arc::new(Posted::default()),
            earlier: None,
            windows: None,
            quit_within: Duration::from_millis(50),
            quit_checked_every: Duration::from_millis(5),
            sign_ins: Arc::default(),
        }
    }

    /// The read the state asks for first, once what is installed is in.
    fn a_read(worker: &Worker) -> Job {
        let mut state = State::new(Cadence::APP);
        let now = Now {
            running: Duration::ZERO,
            epoch_ms: 0,
        };
        state.apply(Msg::Intent(Intent::Refresh { asked: false }), now);
        let answer = worker.work(Job::AskInstalled);
        let mut jobs = state.apply(Msg::Done(answer), now);
        jobs.pop().expect("a read")
    }

    /// A read takes when the readings and the index were last written as they stood before
    /// it, though the read itself records what it measured: taken after, a session's newer
    /// numbers recorded while the read waited counted as seen though nothing showed them.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_read_takes_the_stamps_as_they_stood_before_it() {
        let world = World::new("stamps");
        world.enrolled("work", "here", 10.0);
        let core = world.core();
        let worker = worker(Arc::clone(&core), StandInApps::new(&[], true));
        let job = a_read(&worker);
        let (readings, changed) = (core.readings_changed_at(), core.changed_at());
        let Answer::Read {
            readings_before,
            changed_before,
            read,
            ..
        } = worker.work(job)
        else {
            panic!("a read answers as one");
        };
        assert!(read.is_ok());
        assert_eq!((readings_before, changed_before), (readings, changed));
        assert!(
            core.readings_changed_at() > readings_before,
            "the read recorded what it measured"
        );
    }

    /// Reads answer while a switch runs, as the Swift service's queue of reads answered
    /// while its queue of changes waited: a switch and giving up are changes, made one at a
    /// time on a lane of their own, and what holds a login and quitting an app are asked on
    /// another, which lists processes and waits on apps and never on the network.
    #[test]
    fn a_switch_runs_on_a_lane_of_its_own() {
        let question = QuitQuestion {
            qualified: "codex/work".into(),
            app_id: CHATGPT.into(),
            name: "ChatGPT".into(),
        };
        let switch = Job::Switch {
            qualified: "codex/work".into(),
            reopen: None,
        };
        assert_eq!(switch.lane(), Lane::Changes);
        assert_eq!(Job::Abandon.lane(), Lane::Changes);
        for change in [
            Job::Enrol {
                provider: "codex".into(),
                name: "job".into(),
                from: None,
            },
            Job::Rename {
                provider: "claude".into(),
                label: "work".into(),
                to: "office".into(),
                from: None,
            },
            Job::Forget {
                qualified: "claude/spare".into(),
            },
            Job::UpdateConfig {
                qualified: "claude/work".into(),
            },
        ] {
            assert_eq!(change.lane(), Lane::Changes, "{change:?}");
        }
        for job in [
            Job::Holding {
                qualified: "codex/work".into(),
            },
            Job::Quit { question },
            Job::Open {
                location: "/Applications/ChatGPT.app".into(),
            },
        ] {
            assert_eq!(job.lane(), Lane::Processes, "{job:?}");
        }
        assert_eq!(Job::Look.lane(), Lane::Reads);
        assert_eq!(Job::AskInstalled.lane(), Lane::Discovery);
    }

    /// The look at whether to switch Claude Code by itself reads files and claims nothing, so
    /// it answers on the lane of reads while a switch waits. The switch it may lead to is a
    /// change, made one at a time with every other.
    #[test]
    fn a_look_whether_to_switch_by_itself_runs_with_the_reads() {
        assert_eq!(Job::AutoLook { at: 95 }.lane(), Lane::Reads);
        assert_eq!(Job::AutoSwitch { at: 95 }.lane(), Lane::Changes);
    }

    /// AppModelTests.swift's onlyARunningAppHoldingTheToolIsAskedAbout once more, on the real
    /// core, whose holder detection reads the process list: `switching.rs` has it over the
    /// holdings a test says. What is left of an app that has gone cannot be quit, and a tool
    /// nothing holds is switched at once: ChatGPT holds Codex's login, not Claude Code's.
    #[test]
    fn the_core_says_which_running_app_holds_a_login() {
        let world = World::new("holder");
        world.host.runs_at("codex", &[CHATGPT_CODEX, "codex"]);
        let apps = StandInApps::new(&[CHATGPT], true);
        let worker = worker(world.core(), Arc::clone(&apps));

        let held = |qualified: &str| match worker.work(Job::Holding {
            qualified: qualified.into(),
        }) {
            Answer::Held { question, .. } => question,
            other => panic!("not what holds the login: {other:?}"),
        };
        assert_eq!(
            held("codex/spare"),
            Some(QuitQuestion {
                qualified: "codex/spare".into(),
                app_id: CHATGPT.into(),
                name: "ChatGPT".into(),
            })
        );
        assert_eq!(held("claude/personal"), None, "not Claude Code's login");
        assert_eq!(held("personal"), None, "a bare label is Claude Code's");

        apps.set_running(&[]);
        assert_eq!(held("codex/spare"), None, "gone, so nothing to quit");
        assert!(apps.asked().is_empty(), "and nothing was asked to quit");
    }

    /// A tool whose sessions are started again by a person, or by a command, has nothing
    /// for Pitboard to quit.
    #[test]
    fn only_an_app_pitboard_may_reopen_is_asked_about() {
        let apps = StandInApps::new(&[CHATGPT], true);
        let sessions = Holding {
            kind: "session".into(),
            phrase: "2 `codex` sessions".into(),
            pids: vec![1, 2],
            remedy: Remedy::Restart,
        };
        let daemon = Holding {
            kind: "app_server_daemon".into(),
            phrase: "Codex's background app server".into(),
            pids: vec![3],
            remedy: Remedy::Run {
                command: "codex app-server daemon restart".into(),
            },
        };
        assert_eq!(holder(&[sessions, daemon], apps.as_ref(), "codex/w"), None);
        let found = holder(&[chatgpt_holding()], apps.as_ref(), "codex/w");
        assert_eq!(found.map(|q| q.name), Some("ChatGPT".into()));
    }

    /// An app whose own code cannot say whether it runs is asked about: the core's process
    /// list says it does, and switched under it, it would go on with the account left.
    #[test]
    fn an_app_nobody_can_see_is_taken_as_running() {
        let apps = StandInApps::new(&[], true);
        apps.fail_running(true);
        let found = holder(&[chatgpt_holding()], apps.as_ref(), "codex/w");
        assert_eq!(found.map(|q| q.app_id), Some(CHATGPT.into()));
    }

    /// AppModelTests.swift's anAppIsGivenItsTimeToQuitAndNoMore. An app that quits when
    /// asked is waited for, and the copy that ran is the one to open again; one not running
    /// is not asked; one that does not quit is given its time and no more, and is left
    /// running.
    #[test]
    fn an_app_is_given_its_time_to_quit_and_no_more() {
        let apps = StandInApps::new(&[CHATGPT], true);
        let within = Duration::from_millis(50);
        let every = Duration::from_millis(5);
        assert_eq!(
            quit(apps.as_ref(), CHATGPT, within, every),
            QuitOutcome::Quit {
                copy: StandInApps::copy_of(CHATGPT)
            }
        );
        assert_eq!(
            quit(apps.as_ref(), CHATGPT, within, every),
            QuitOutcome::NotRunning
        );
        assert_eq!(
            apps.asked(),
            [format!("quit {CHATGPT}")],
            "one not running is not asked"
        );

        let busy = StandInApps::new(&[CHATGPT], false);
        let within = Duration::from_millis(100);
        let started = Instant::now();
        assert_eq!(
            quit(busy.as_ref(), CHATGPT, within, every),
            QuitOutcome::StillRunning
        );
        assert!(started.elapsed() >= within);
        assert!(busy.is_running(CHATGPT));
    }

    /// While it waits it asks whether the app still runs every so often, and not more:
    /// thirty seconds at every 200 milliseconds is 150 questions, not a busy loop.
    #[test]
    fn an_app_given_its_time_is_asked_after_it_every_so_often() {
        assert_eq!(Cadence::APP.quit_within, Duration::from_secs(30));
        assert_eq!(Cadence::APP.quit_checked_every, Duration::from_millis(200));
        let busy = StandInApps::new(&[CHATGPT], false);
        let outcome = quit(
            busy.as_ref(),
            CHATGPT,
            Duration::from_millis(100),
            Duration::from_millis(20),
        );
        assert_eq!(outcome, QuitOutcome::StillRunning);
        // Once before asking it to quit, then at 0, 20, 40, 60, 80 and 100 ms, give or take
        // a slow machine's sleeps, which only ever ask less: never fewer than once before
        // and once at the end.
        let asked = busy.running_calls();
        assert!((2..=7).contains(&asked), "asked {asked} times");
    }

    /// An app whose own code fails is still running as far as Pitboard can tell, so nothing
    /// is switched under it: failing to say where it runs, failing to ask it, and failing to
    /// say it has gone all end as an app still open.
    #[test]
    fn an_app_nobody_can_ask_or_see_go_is_still_running() {
        let every = Duration::from_millis(5);
        let within = Duration::from_millis(30);
        let unseen = StandInApps::new(&[CHATGPT], true);
        unseen.fail_running(true);
        assert_eq!(
            quit(unseen.as_ref(), CHATGPT, within, every),
            QuitOutcome::StillRunning
        );
        assert!(
            unseen.asked().is_empty(),
            "nothing asked of an app not seen"
        );

        let unasked = StandInApps::new(&[CHATGPT], true);
        unasked.fail_quit(true);
        assert_eq!(
            quit(unasked.as_ref(), CHATGPT, within, every),
            QuitOutcome::StillRunning
        );
        assert!(unasked.is_running(CHATGPT));
    }

    /// A sign-in runs on a thread of its own, which is told whether to enrol, and a code typed
    /// back and a stop run on the lane of sign-in calls, apart from it and from every other
    /// lane: the thread waits on a browser, and the Swift model's SignInCalls ran each of
    /// those calls apart from the one reading.
    #[test]
    fn a_sign_in_runs_on_a_thread_of_its_own() {
        for job in [
            Job::SignIn {
                id: 1,
                qualified: "claude/work".into(),
            },
            Job::SignInOver { id: 1, enrol: true },
        ] {
            assert_eq!(job.lane(), Lane::SignIn, "{job:?}");
        }
        for job in [
            Job::PasteCode {
                id: 1,
                code: "c#s".into(),
            },
            Job::StopSignIn { id: 1 },
        ] {
            assert_eq!(job.lane(), Lane::SignInCalls, "{job:?}");
        }
    }

    /// A sign-in's thread is told once whether to enrol. Once the model has gone, its word
    /// goes, which tells it not to, and a sign-in whose tool starts after that is stopped as
    /// it starts: nothing is left to say it is over.
    #[test]
    #[cfg(unix)]
    fn a_sign_in_is_told_once_and_let_go_once_the_model_has_gone() {
        let sign_ins = SignIns::default();
        let (tell, told) = channel();
        sign_ins.begin(1, tell);
        sign_ins.tell(1, true);
        sign_ins.tell(1, false);
        assert_eq!(told.try_recv(), Ok(true));
        assert!(told.try_recv().is_err(), "told once");

        let mut world = World::new("let-go");
        let claude = world.claude_stand_in();
        let core = world.core();
        let (tell, told) = channel();
        sign_ins.begin(2, tell);
        let session = core
            .sign_in("claude/travel".into())
            .expect("the stand-in starts");
        // Once it has written its process id, so that it stopping below is not taken from
        // one stopped before it wrote it.
        assert!(claude.is_running(), "the stand-in started");
        assert!(sign_ins.started(2, &session));
        assert!(sign_ins.session(2).is_some());
        sign_ins.quiet(2);
        assert!(
            sign_ins.session(2).is_none(),
            "nothing typed to or stopped once quiet"
        );

        let (tell_late, told_late) = channel();
        sign_ins.begin(3, tell_late);
        sign_ins.close();
        assert!(told.recv().is_err(), "let go");
        assert!(told_late.recv().is_err(), "let go before it started");
        assert!(!sign_ins.started(3, &session), "stopped as it starts");
        session.cancel();
        let stopped = Instant::now();
        while claude.is_running() {
            assert!(
                stopped.elapsed() < Duration::from_secs(20),
                "the stand-in stopped"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Closing stops each sign-in whose tool has started, and has stopped it and waited for
    /// it by the time it returns. The sign-ins have stopped once every one's tool has been
    /// stopped from here or its thread has ended: one still starting holds them up until its
    /// thread ends, for no longer than asked, and one whose tool was stopped does not, though
    /// its thread may still be reading what the tool said.
    #[test]
    #[cfg(unix)]
    fn the_sign_ins_have_stopped_once_each_tool_has_or_its_thread_has_ended() {
        let sign_ins = Arc::new(SignIns::default());
        let mut world = World::new("stopped-within");
        let claude = world.claude_stand_in();
        let core = world.core();
        let (tell, _told) = channel();
        sign_ins.begin(1, tell);
        let session = core
            .sign_in("claude/travel".into())
            .expect("the stand-in starts");
        assert!(sign_ins.started(1, &session));
        let (tell, _told_starting) = channel();
        sign_ins.begin(2, tell);
        // It writes its process id before it says anything.
        assert!(session.next_line().is_some(), "the stand-in says something");
        assert!(claude.is_running());

        sign_ins.close();
        assert!(!claude.is_running(), "stopped and waited for as it closes");
        assert!(
            !sign_ins.stopped_within(Duration::from_millis(50)),
            "one still starting"
        );
        let ending = {
            let sign_ins = Arc::clone(&sign_ins);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                sign_ins.end(2);
            })
        };
        assert!(sign_ins.stopped_within(Duration::from_secs(20)));
        ending.join().expect("the thread that ends it");
    }

    /// A sign-in whose thread the system cannot start comes to nothing, as one whose thread
    /// stops with a panic does, which the model says as a failure. It starts on the actor's
    /// own thread, where a panic stopped the model, which answered no intent after it, and
    /// nothing is left waiting to be told whether to enrol.
    #[test]
    fn a_sign_in_whose_thread_cannot_start_comes_to_nothing() {
        let world = World::new("no-thread");
        let (answers, answered) = channel();
        let lanes = Lanes::open(
            world.core(),
            StandInApps::new(&[], true),
            Arc::new(Posted::default()),
            Keeping {
                earlier: None,
                windows: None,
            },
            Cadence::APP,
            &answers,
        );
        let refused = catch_unwind(AssertUnwindSafe(|| {
            lanes.start_sign_in(1, "claude/travel".into(), |_| {
                Err(std::io::Error::other("no thread left"))
            });
        }));
        assert!(refused.is_ok(), "the actor's thread goes on");
        let answer = answered
            .recv_timeout(Duration::from_secs(20))
            .expect("an answer");
        assert!(
            matches!(
                &answer,
                Msg::Done(Answer::Lost(Job::SignIn { id: 1, qualified }))
                    if qualified == "claude/travel"
            ),
            "{answer:?}"
        );
        assert!(
            lanes.worker.sign_ins.held().running.is_empty(),
            "nothing left waiting"
        );
    }

    /// A code typed back and a stop reach a sign-in only while its tool has started and is
    /// saying something: before, there is nothing to reach, and after, what it signed in to
    /// may be being enrolled. A stop that stops the tool says so by the sign-in's id, since by
    /// then the tool has been waited for and the core's one sign-in at a time let go of, and
    /// one that reaches nothing says nothing of it.
    #[test]
    #[cfg(unix)]
    fn a_code_and_a_stop_reach_only_a_tool_that_has_started_and_still_speaks() {
        let mut world = World::new("reach");
        let claude = world.claude_stand_in();
        let core = world.core();
        let worker = worker(Arc::clone(&core), StandInApps::new(&[], true));
        let (tell, _told) = channel();
        worker.sign_ins.begin(1, tell);
        let paste = Job::PasteCode {
            id: 1,
            code: "the-code#the-state".into(),
        };
        assert!(matches!(worker.work(paste.clone()), Answer::Pasted));

        let session = core
            .sign_in("claude/travel".into())
            .expect("the stand-in starts");
        assert!(worker.sign_ins.started(1, &session));
        let mut said = String::new();
        while !said.contains("Paste code") {
            said.push_str(&session.next_line().expect("the stand-in's prompt"));
        }
        assert!(matches!(worker.work(paste), Answer::Pasted));
        let typed = Instant::now();
        while claude.typed().is_empty() {
            assert!(
                typed.elapsed() < Duration::from_secs(20),
                "typed to the tool"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            claude.typed(),
            ["the-code#the-state"],
            "the one typed once it started"
        );
        // Stops the stand-in, which ends once it has taken the code, and reaps it.
        session.cancel();

        // In the same home, at once: the cancel has let go of the core's one sign-in at a
        // time by the time it returns, whatever processes other tests start meanwhile.
        let session = core
            .sign_in("claude/again".into())
            .expect("the stand-in starts again");
        assert!(worker.sign_ins.started(1, &session));
        // It writes its process id before it says anything.
        assert!(session.next_line().is_some(), "the stand-in says something");
        let stopped = worker.work(Job::StopSignIn { id: 1 });
        assert!(
            matches!(stopped, Answer::SignInStopped { id: 1 }),
            "{stopped:?}"
        );
        assert!(!claude.is_running(), "stopped and waited for as it answers");
        assert!(
            core.sign_in("claude/after".into())
                .expect("the one sign-in at a time let go of")
                .cancel(),
            "the next stopped too"
        );
        assert!(matches!(
            worker.work(Job::StopSignIn { id: 1 }),
            Answer::Stopped
        ));
        worker.sign_ins.quiet(1);
        assert!(matches!(
            worker.work(Job::StopSignIn { id: 1 }),
            Answer::Stopped
        ));
    }

    /// What is about the machine runs where the Swift service ran it: the schedule, doctor and
    /// the log on the lane of reads, so they answer while a switch waits; the repair, a
    /// change to the schedule and a renewal on the lane of changes, one at a time with every
    /// other change; and the command line where the login shell's `PATH` is asked, apart from
    /// both.
    #[test]
    fn what_is_about_the_machine_runs_where_the_swift_service_ran_it() {
        for job in [
            Job::ReadSchedule {
                after_change: false,
            },
            Job::Check,
            Job::ReadLog { limit: 500 },
        ] {
            assert_eq!(job.lane(), Lane::Reads, "{job:?}");
        }
        for job in [
            Job::RepairSchedule,
            Job::SetSchedule { on: true },
            Job::Renew,
        ] {
            assert_eq!(job.lane(), Lane::Changes, "{job:?}");
        }
        assert_eq!(Job::FindCommandLine.lane(), Lane::Discovery);
    }

    /// The `pitboard` a terminal runs: the first on the login shell's `PATH`, then where each
    /// way of installing Pitboard puts one, and whether it is this app's own once every link
    /// on the way to it is followed. Looked for over real files in the scratch home, and in
    /// none of this machine's places.
    ///
    /// MachineModelTests.swift's theCommandLineIsLookedForWhereATerminalWouldFindIt.
    #[test]
    #[cfg(unix)]
    fn the_command_line_is_looked_for_where_a_terminal_would_find_it() {
        use crate::FoundCommandLine;
        let mut world = World::new("command-line");
        let helper = world.app_with_a_command_line();
        let (bin, cargo) = (world.dir("bin"), world.dir("cargo/bin"));
        std::fs::create_dir_all(&bin).expect("a bin");
        world.looks_for_pitboard_in(
            vec![bin.clone()],
            Some(format!("/nowhere/bin:{}", cargo.display())),
        );
        let worker = worker(world.core(), StandInApps::new(&[], true));
        let found = |worker: &Worker| match worker.work(Job::FindCommandLine) {
            Answer::CommandLineFound { found, own } => {
                assert!(own.lasting(), "{own:?}");
                found
            }
            other => panic!("not the command line: {other:?}"),
        };
        assert_eq!(found(&worker), FoundCommandLine::Nowhere);

        files::link(&helper, &bin.join("pitboard")).expect("a link");
        assert_eq!(
            found(&worker),
            FoundCommandLine::Bundled {
                path: bin.join("pitboard").to_string_lossy().into_owned()
            }
        );

        crate::model::testing::a_program_at(&cargo.join("pitboard"));
        assert_eq!(
            found(&worker),
            FoundCommandLine::Another {
                path: cargo.join("pitboard").to_string_lossy().into_owned()
            },
            "the login shell's PATH first"
        );
    }

    /// The `pitboard` a terminal runs is looked for in a core made from its parts, as a
    /// fixture makes its own, and in one made as the app makes its own, by
    /// `AppCore::for_app` from the environment the app was started with. It is looked for on
    /// the login shell's `PATH` ahead of what cargo installed, and then where each way of
    /// installing Pitboard puts it under the app's home, which is all there is where the shell
    /// could not be asked. Only a copy of the app has a command line inside it for a link to
    /// lead back to: a core not run from an app has none, and nor does a build directory.
    /// Each look finds a copy in the scratch home before any place of this machine's. The
    /// core made from its parts names no tool's program and is given an empty search path
    /// for one, needing neither; the one made from the environment names each tool's
    /// program outright, and the login shell it names cannot be run. So nothing of this
    /// machine is looked in.
    ///
    /// AppModelTests.swift's theSettingsLookForTheCommandLineWhereTheLoginShellSays, but for
    /// linking nothing, since linking is the app's own.
    #[test]
    #[cfg(unix)]
    fn the_settings_look_for_the_command_line_where_the_login_shell_says() {
        use crate::{FoundCommandLine, Made, OwnCommandLine};
        use pitboard_core::context::Context;
        use pitboard_core::host::{OS, Os};
        use std::path::Path;
        let world = World::new("installs");
        let app = world.dir("Pitboard.app");
        let helper = app.join("Contents/Helpers/pitboard");
        let (bin, home) = (world.dir("bin"), world.dir("home"));
        let cargo = home.join(".cargo/bin/pitboard");
        crate::model::testing::a_program_at(&helper);
        crate::model::testing::a_program_at(&cargo);
        std::fs::create_dir_all(&bin).expect("a bin");
        let linked = bin.join("pitboard");
        files::link(&helper, &linked).expect("a link");
        let text = |path: &Path| path.to_string_lossy().into_owned();
        let bundled = |path: &Path| FoundCommandLine::Bundled { path: text(path) };
        let another = |path: &Path| FoundCommandLine::Another { path: text(path) };
        let found = |core: Arc<AppCore>| match worker(core, StandInApps::new(&[], true))
            .work(Job::FindCommandLine)
        {
            Answer::CommandLineFound { found, own } => (found, own),
            other => panic!("not the command line: {other:?}"),
        };

        let told = |search_path: Option<String>, helper: Option<&Path>| {
            let (home, helper) = (home.clone(), helper.map(Path::to_path_buf));
            Arc::new(AppCore::asking(
                move || {
                    let made = Made {
                        core: pitboard_core::service::Pitboard::new(
                            Context::new(home.clone())
                                .with_pitboard_home(home.join(".pitboard"))
                                .with_search_path(String::new())
                                .with_caller("app".into()),
                        ),
                        found: Vec::new(),
                        search_path: search_path.clone(),
                        helper: helper.clone(),
                        command_line_places: pitboard_core::app::command_line_places(&home),
                    };
                    (made, false)
                },
                crate::ASK_AGAIN_AFTER,
            ))
        };
        let login = Some(format!("/nowhere/bin:{}", bin.display()));
        assert_eq!(
            found(told(login.clone(), Some(&helper))).0,
            bundled(&linked),
            "ahead of the one cargo installed"
        );
        assert_eq!(
            found(told(login, None)).0,
            another(&linked),
            "not run from an app"
        );
        assert_eq!(
            found(told(None, Some(&helper))).0,
            another(&cargo),
            "the login shell could not be asked"
        );

        let environment: HashMap<String, String> = [
            ("HOME", text(&home)),
            ("SHELL", "/nowhere/at/all/sh".into()),
            ("PITBOARD_HOME", text(&home.join(".pitboard"))),
            ("CLAUDE_CONFIG_DIR", text(&home.join(".claude"))),
            ("CODEX_HOME", text(&home.join(".codex"))),
            ("PITBOARD_CLAUDE", "/nowhere/claude".into()),
            ("PITBOARD_CODEX", "/nowhere/codex".into()),
        ]
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect();
        let started_at = |at: &Path| AppCore::for_app(environment.clone(), Some(text(at)));
        let (from_the_app, own) = found(started_at(&app));
        assert_eq!(
            from_the_app,
            another(&cargo),
            "the login shell could not be run"
        );
        // No app runs on Linux, so nothing there has a command line inside it, and the
        // Windows app does not say where its own is yet.
        let inside = match OS {
            Os::MacOs => true,
            Os::Linux | Os::Windows => false,
        };
        assert_eq!(own.lasting(), inside, "{own:?}");
        let (from_a_build, own) = found(started_at(&world.dir(".build/debug")));
        assert_eq!(from_a_build, another(&cargo));
        assert_eq!(
            own,
            OwnCommandLine::default(),
            "a build directory has no command line inside it"
        );

        std::fs::remove_file(&cargo).expect("cargo's copy taken away");
        files::link(&helper, &cargo).expect("a link where cargo installs");
        assert_eq!(
            found(started_at(&app)).0,
            if inside {
                bundled(&cargo)
            } else {
                another(&cargo)
            },
            "the app's own, linked to where cargo installs"
        );
    }

    /// Whether this copy has a command line a schedule or a link would keep reaching is read
    /// from the file inside it each time: none at all, one nobody may run, and one that runs.
    /// A copy with none, or one that cannot run, is refused before the scheduler is asked.
    ///
    /// MachineModelTests.swift's anAppWithoutACommandLineItCanRunCannotLinkOrSchedule, over a
    /// real file.
    #[test]
    #[cfg(unix)]
    fn the_command_line_inside_is_read_from_the_file_each_time() {
        let read = |worker: &Worker| match worker.work(Job::ReadSchedule {
            after_change: false,
        }) {
            Answer::ScheduleRead { own, .. } => own,
            other => panic!("not the schedule: {other:?}"),
        };
        let none = World::new("no-command-line");
        let without = worker(none.core(), StandInApps::new(&[], true));
        assert_eq!(read(&without), crate::OwnCommandLine::default());
        assert!(matches!(
            without.work(Job::SetSchedule { on: true }),
            Answer::ScheduleSet {
                outcome: Scheduled::CannotSchedule,
                ..
            }
        ));

        let mut world = World::new("unrunnable");
        let helper = world.app_with_a_command_line();
        let worker = worker(world.core(), StandInApps::new(&[], true));
        files::deny_running(&helper).expect("a mode nobody runs");
        let own = read(&worker);
        assert!(own.inside && !own.temporary && !own.runs, "{own:?}");
        assert!(matches!(
            worker.work(Job::SetSchedule { on: true }),
            Answer::ScheduleSet {
                outcome: Scheduled::CannotSchedule,
                ..
            }
        ));
        files::make_runnable(&helper).expect("a mode that runs");
        assert!(read(&worker).lasting());

        let mut temporary = World::new("translocated");
        temporary.app_with_a_command_line_in("AppTranslocation/0A1B2C/d");
        let gone_on_quit = self::worker(temporary.core(), StandInApps::new(&[], true));
        let own = read(&gone_on_quit);
        assert!(own.inside && own.temporary && own.runs, "{own:?}");
        assert!(matches!(
            gone_on_quit.work(Job::SetSchedule { on: true }),
            Answer::ScheduleSet {
                outcome: Scheduled::CannotSchedule,
                ..
            }
        ));
    }

    /// An app is opened again through the app's own code, where `running` said it was, and
    /// one that cannot be opened is not the switch's failure.
    #[test]
    fn opening_again_goes_through_the_apps_own_code() {
        let apps = StandInApps::new(&[], true);
        let world = World::new("open");
        let worker = worker(world.core(), Arc::clone(&apps));
        let answer = worker.work(Job::Open {
            location: StandInApps::copy_of(CHATGPT),
        });
        assert!(matches!(answer, Answer::Opened));
        assert_eq!(apps.asked(), [format!("open {CHATGPT}")]);

        apps.fail_open(true);
        let answer = worker.work(Job::Open {
            location: StandInApps::copy_of(CHATGPT),
        });
        assert!(matches!(answer, Answer::Opened));
    }
}
