//! The core the model's lanes call: made from what the app was started with, the first time
//! a lane needs it, and kept for as long as the app runs. Not exported: an app reaches the
//! core only through the model, whose calls wait on nothing, while each call here is
//! synchronous and may block on the keychain, a lock, the network or the person's login
//! shell, so only the model's own threads make one.
//!
//! What the core answers is said in records of the bindings where a snapshot shows it, such
//! as `Status` and `Abandoned`, and in the types here where only the model reads it, such as
//! what a switch, an enrolment or a renewal came to, doctor's checks, and why something
//! failed.

use crate::{
    Abandoned, Account, FoundCommandLine, Level, Limit, Parked, Schedule, Source, Status, Tool,
    Usage, Warning, found_command_line, tool,
};
use pitboard_core::app::AppContext;
use pitboard_core::autoswitch::{Auto, Blind, Look, Skip, Threshold};
use pitboard_core::context::Environment;
use pitboard_core::provider::ProviderId;
use pitboard_core::service::{self, Changing};
use pitboard_core::{doctor, status, switch, usage, words};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Why something the core was asked to do did not happen.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PitboardError {
    /// `code` is stable, for the model to branch on; `message` names the cause and what to
    /// do. `warnings` are what was found on the way, reported even though the operation
    /// failed.
    #[error("{message}")]
    Failed {
        code: String,
        message: String,
        warnings: Vec<Warning>,
    },
}

impl From<pitboard_core::error::Error> for PitboardError {
    fn from(error: pitboard_core::error::Error) -> Self {
        PitboardError::Failed {
            code: error.code().to_string(),
            message: error.to_string(),
            warnings: Vec::new(),
        }
    }
}

impl From<service::Failed> for PitboardError {
    fn from(failed: service::Failed) -> Self {
        PitboardError::Failed {
            code: failed.error.code().to_string(),
            message: failed.error.to_string(),
            warnings: warnings(&failed.warnings),
        }
    }
}

fn warnings(found: &[service::Warning]) -> Vec<Warning> {
    found
        .iter()
        .map(|w| Warning {
            code: w.code().to_string(),
            message: w.to_string(),
            account: match w {
                service::Warning::LoginReplaced { tool, id, .. } => Some(account_id(*tool, id)),
                _ => None,
            },
        })
        .collect()
}

/// An account's `Account.id`: its tool and what the core files it under, or the tool alone
/// for a login that belongs to no account the core can name.
fn account_id(tool: ProviderId, id: &str) -> String {
    if id.is_empty() {
        format!("{}:login", tool.code())
    } else {
        format!("{}:{id}", tool.code())
    }
}

/// One change Pitboard made, as `pitboard log` shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Change {
    /// Local time, as the log records it.
    pub(crate) at: String,
    /// Which front end asked: `app`, `cli`, or `unknown` for a line written before this
    /// was recorded.
    pub(crate) caller: String,
    pub(crate) verb: String,
    pub(crate) subject: String,
    /// `ok`, or the code of whatever stopped it.
    pub(crate) outcome: String,
}

/// When a session of the tool that is already running picks a switch up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Adoption {
    /// On its own, within this many seconds.
    Follows { within_seconds: u32 },
    /// On its own, when its login is next renewed, because the file at `path` sits behind
    /// the store the switch wrote.
    Renewal { path: String },
    /// Never: `program` has to be quit and started again.
    Restart { program: String },
}

/// What makes one kind of process take a switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Remedy {
    /// Quit it and start it again.
    Restart,
    /// Quit the app the way its system quits an app, and open it again. The model may do
    /// both. `app_id` is what that system names it by, as `AppControl` takes it: on macOS
    /// its bundle id.
    ReopenApp { app_id: String, name: String },
    /// Run this command.
    Run { command: String },
    /// Do this, somewhere Pitboard cannot reach.
    Do { instruction: String },
}

/// One kind of process running a tool with a login in memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Holding {
    /// Stable, in snake case, for code to tell kinds apart by: `chatgpt_app`, `session`.
    pub(crate) kind: String,
    /// As a sentence names what is running: "the ChatGPT app", "2 `codex` sessions".
    pub(crate) phrase: String,
    pub(crate) pids: Vec<u32>,
    pub(crate) remedy: Remedy,
}

impl From<pitboard_core::holder::Holding> for Holding {
    fn from(held: pitboard_core::holder::Holding) -> Holding {
        use pitboard_core::holder::Remedy as Core;
        Holding {
            kind: held.holder.kind.into(),
            phrase: held.phrase(),
            remedy: match held.holder.remedy {
                Core::Restart => Remedy::Restart,
                Core::ReopenApp { app, name } => Remedy::ReopenApp {
                    app_id: app.as_str().into(),
                    name: name.into(),
                },
                Core::Run(command) => Remedy::Run {
                    command: command.into(),
                },
                Core::Do(instruction) => Remedy::Do {
                    instruction: instruction.into(),
                },
            },
            pids: held.pids,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Switch {
    Switched {
        /// Which tool's login moved.
        provider: String,
        from: String,
        to: String,
        adoption: Adoption,
    },
    AlreadyActive {
        label: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Switched {
    pub(crate) outcome: Switch,
    pub(crate) warnings: Vec<Warning>,
}

/// What switching Claude Code by itself came to, as the model takes it. Times are epoch
/// seconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AutoSwitched {
    /// Nothing to do: no limit of `account`, the account in use, has reached the share.
    /// `used` is how much of its fullest limit still running it has used, "62% of its 5-hour
    /// limit", in the reading taken at `as_of`. Anthropic holds Pitboard off asking about it
    /// again until `held_until`, where it does.
    Watching {
        account: String,
        used: Option<String>,
        as_of: Option<i64>,
        held_until: Option<i64>,
    },
    /// `used` of a limit of `from` reached the share, and an attempt at switching away from a
    /// limit of it came to nothing a moment ago: the next is not made before `until`.
    Waiting {
        from: String,
        used: String,
        until: i64,
    },
    /// Claude Code was switched from `from` to `to`, which running sessions take as `adoption`
    /// says, with what the switch warned of, and how much the account it left had used: "96%
    /// of its 5-hour limit".
    Switched {
        from: String,
        to: String,
        adoption: Adoption,
        warnings: Vec<Warning>,
        used: String,
    },
    /// A limit of `from` reached the share, `used` of it, and Pitboard did not switch, for
    /// the reason `why` says, until `until` where the reason ends then. `key` tells it apart
    /// from another: the account, the limit, its reset and the reason's code.
    Skipped {
        key: String,
        from: String,
        used: String,
        why: String,
        until: Option<i64>,
    },
    /// Pitboard cannot judge whether to switch, for the reason `why` says, and asks again at
    /// `until` where it waits to. `key` tells it apart from another: its code, and the
    /// account or the cause it names.
    NotWatching {
        key: String,
        why: String,
        until: Option<i64>,
    },
}

/// What the core's look at whether to switch Claude Code by itself came to, from its files
/// alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AutoLooked {
    /// This stands, and nothing is to be decided under the core's lock.
    Stands(AutoSwitched),
    /// A switch may be due, or whose login Claude Code has stored is in doubt: only a
    /// decision under the core's lock can say.
    Act,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EnrolledAs {
    /// The account signed in now.
    Current,
    /// Another account, signed in privately and parked.
    SignedIn,
    /// An enrolled account's parked login, renewed.
    Renewed,
    /// The account signed in now, signed in to: its new login is the one in use now.
    /// `again` when it was enrolled already, and not when this sign-in enrolled it.
    InUse { again: bool },
}

/// What enrolling an account came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Enrolled {
    pub(crate) email: String,
    pub(crate) outcome: EnrolledAs,
    pub(crate) warnings: Vec<Warning>,
}

/// What renewing every due parked login came to, one for each. A snapshot says what the run
/// did in one sentence, `renewal_note`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Renewed {
    /// As a person types it: bare for Claude Code, `codex/work` for Codex.
    pub(crate) label: String,
    /// Which tool's login it is.
    pub(crate) provider: String,
    /// `renewed`, `renewal_deferred`, `parked_login_refused`, or the code of a failure.
    pub(crate) outcome: String,
}

/// One of doctor's checks. A snapshot shows each as a `CheckLine`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Check {
    pub(crate) code: String,
    pub(crate) name: String,
    pub(crate) level: Level,
    pub(crate) detail: String,
    /// Empty when there is nothing to do.
    pub(crate) advice: String,
}

fn enrolled(enrolled: switch::Enrolled, warnings: Vec<Warning>) -> Enrolled {
    let (email, outcome) = match enrolled {
        switch::Enrolled::Current { email } => (email, EnrolledAs::Current),
        switch::Enrolled::SignedIn { email } => (email, EnrolledAs::SignedIn),
        switch::Enrolled::Renewed { email } => (email, EnrolledAs::Renewed),
        switch::Enrolled::InUse { email, again } => (email, EnrolledAs::InUse { again }),
    };
    Enrolled {
        email,
        outcome,
        warnings,
    }
}

fn account(row: status::Row, now: i64) -> Account {
    let key = row.key();
    let unplaced = row.unplaced();
    Account {
        id: account_id(row.provider, &row.id),
        provider: row.provider.code().into(),
        qualified: key.map(|k| k.qualified()),
        unplaced,
        switchable: row.switchable(now),
        stale: row.stale.map(|s| s.code().to_string()),
        // In the row's own tool's words: a Codex row is not about Anthropic.
        stale_explanation: row.explanation().map(str::to_owned),
        parked: row.parked.map(|p| Parked {
            parked_at: p.parked_at,
            access_expires_at: p.access_expires_at,
            refresh_expires_at: p.refresh_expires_at,
        }),
        usage: row.usage.map(|u| Usage {
            source: match u.source {
                usage::Source::Live => Source::Live,
                usage::Source::Remembered => Source::Remembered,
            },
            observed_at: u.observed_at,
            lists_every_limit: u.lists_every_limit,
            windows: u
                .windows
                .into_iter()
                .map(|w| Limit {
                    length_seconds: w.length_seconds,
                    kind: w.kind,
                    scope: w.scope,
                    percent: w.percent,
                    resets_at: w.resets_at,
                    severity: w.severity,
                    is_active: w.is_active,
                })
                .collect(),
        }),
        label: row.label,
        email: row.email,
        account_id: row.id,
        signed_in: row.signed_in,
    }
}

fn status_of(done: service::Done<status::Report>) -> Status {
    let now = done.value.now;
    Status {
        now,
        accounts: done
            .value
            .rows
            .into_iter()
            .map(|row| account(row, now))
            .collect(),
        warnings: warnings(&done.warnings),
    }
}

/// When sessions already running follow a switch, as the core says it for the tool.
fn adoption_of(adoption: pitboard_core::provider::Adoption) -> Adoption {
    match adoption {
        pitboard_core::provider::Adoption::PollingWithin(seconds) => Adoption::Follows {
            within_seconds: seconds,
        },
        pitboard_core::provider::Adoption::AtRenewal { file } => Adoption::Renewal {
            path: file.display().to_string(),
        },
        pitboard_core::provider::Adoption::RestartRequired { program, .. } => Adoption::Restart {
            program: program.into(),
        },
    }
}

/// What switching Claude Code by itself at `threshold` came to, as the model takes it, with
/// what a switch warned of.
fn auto_switched(auto: Auto, warnings: Vec<Warning>, threshold: Threshold) -> AutoSwitched {
    match auto {
        Auto::Watching {
            account,
            nearest,
            as_of,
            held_until,
        } => AutoSwitched::Watching {
            account,
            used: nearest.as_ref().map(words::share_of_limit),
            as_of,
            held_until,
        },
        Auto::Waiting { from, limit, until } => AutoSwitched::Waiting {
            from,
            used: words::share_of_limit(&limit),
            until,
        },
        Auto::Switched {
            from,
            to,
            limit,
            adoption,
        } => AutoSwitched::Switched {
            from,
            to,
            adoption: adoption_of(adoption),
            warnings,
            used: words::share_of_limit(&limit),
        },
        Auto::Skipped { from, limit, why } => AutoSwitched::Skipped {
            key: skipped_key(&from, &limit, &why),
            used: words::share_of_limit(&limit),
            why: words::not_switching(&why, threshold),
            until: match why {
                Skip::Settling { until } => Some(until),
                Skip::NoRoom { .. } | Skip::AlreadyLeft | Skip::GaveUp | Skip::Overridden(_) => {
                    None
                }
            },
            from,
        },
        Auto::NotWatching { why } => AutoSwitched::NotWatching {
            key: not_watching_key(&why),
            why: words::not_watching(&why),
            until: match why {
                Blind::Unidentified { until, .. } => Some(until),
                Blind::SwitchInterrupted
                | Blind::CustomOauth
                | Blind::NothingSignedIn
                | Blind::NotEnrolled { .. }
                | Blind::NoReading { .. } => None,
            },
        },
    }
}

/// What tells one reason the automatic switch cannot judge apart from another, for saying each
/// once: its code, and the account or the cause it names. Not when it asks again, so a cause
/// that stands is said once however often it is asked about.
fn not_watching_key(why: &Blind) -> String {
    let named = match why {
        Blind::NotEnrolled { email } => Some(email),
        Blind::Unidentified { detail, .. } => Some(detail),
        Blind::NoReading { account } => Some(account),
        Blind::SwitchInterrupted | Blind::CustomOauth | Blind::NothingSignedIn => None,
    };
    match named {
        Some(named) => format!("not-watching/{}/{named}", why.code()),
        None => format!("not-watching/{}", why.code()),
    }
}

/// What tells one reason the automatic switch did not switch away from a limit apart from
/// another, for saying each once: the account, the limit, the reset the core recorded the
/// reason under, and the reason's code, as the core records it once.
fn skipped_key(from: &str, limit: &usage::Window, why: &Skip) -> String {
    format!(
        "skipped/{from}/{}/{}/{}/{}",
        limit.kind,
        limit.scope.as_deref().unwrap_or_default(),
        limit.resets_at.unwrap_or_default(),
        why.code()
    )
}

fn changed<T, R>(
    outcome: Changing<T>,
    make: impl FnOnce(T, Vec<Warning>) -> R,
) -> Result<R, PitboardError> {
    let done = outcome?;
    Ok(make(done.value, warnings(&done.warnings)))
}

/// A sign-in in progress: the tool's own, running with its output piped here because an
/// app has no terminal to hand it. Both tools open the browser themselves and finish
/// through a loopback callback. Claude Code reads stdin only for a fallback code to paste;
/// Codex prints the address to open when its browser cannot, and reads nothing.
pub(crate) struct SignInSession {
    watched: Mutex<Option<switch::WatchedSignIn>>,
    /// Apart from `watched`: reading waits on the tool, and Codex says nothing between its
    /// address and the browser coming back, so a read that held `watched` kept a cancel or
    /// a paste waiting until then.
    said: switch::Said,
    label: String,
    /// The core the sign-in started with, which enrols what it signed in to: one made again
    /// meanwhile, from a login shell that answered late, may look elsewhere for programs,
    /// but keeps its accounts in the same place.
    made: Arc<Made>,
}

impl SignInSession {
    /// The next thing the tool said, or nothing once it has stopped saying anything.
    /// Blocks until the tool says something or stops.
    pub(crate) fn next_line(&self) -> Option<String> {
        self.said.next()
    }

    /// Types back the code the browser showed after signing in.
    pub(crate) fn paste(&self, line: String) -> Result<(), PitboardError> {
        let mut held = self.watched.lock().map_err(|_| PitboardError::Failed {
            code: "sign_in_gone".into(),
            message: "this sign-in is no longer running".into(),
            warnings: Vec::new(),
        })?;
        match held.as_mut() {
            Some(watched) => watched.paste(&line).map_err(PitboardError::from),
            None => Ok(()),
        }
    }

    /// Waits for it to finish, then enrols what it signed in to.
    pub(crate) fn finish(&self) -> Result<Enrolled, PitboardError> {
        let watched = self
            .watched
            .lock()
            .ok()
            .and_then(|mut held| held.take())
            .ok_or_else(|| PitboardError::Failed {
                code: "sign_in_gone".into(),
                message: "this sign-in is no longer running".into(),
                warnings: Vec::new(),
            })?;
        let login = watched.finish()?;
        changed(
            self.made.core.enroll_signed_in(&self.label, login),
            enrolled,
        )
    }

    /// Stops it. Whatever it wrote is discarded. Whether this stopped it: true once its tool
    /// has been stopped and waited for here and the core's one sign-in at a time let go of,
    /// false where a finish or another stop had taken it already. Held meanwhile, so a finish
    /// or a stop asked for at the same time returns only once this has.
    pub(crate) fn cancel(&self) -> bool {
        let Ok(mut held) = self.watched.lock() else {
            return false;
        };
        let Some(watched) = held.take() else {
            return false;
        };
        watched.cancel();
        true
    }
}

/// What the app's core is made of. Made once, on first use, and kept: making it can mean
/// asking the person's login shell, which takes up to five seconds.
pub(crate) struct Made {
    pub(crate) core: service::Pitboard,
    /// The tools a program was named or found for, which is what `installed` answers.
    pub(crate) found: Vec<ProviderId>,
    /// The login shell's `PATH` as far as it was looked in, where the `pitboard` a terminal
    /// runs is looked for first.
    pub(crate) search_path: Option<String>,
    /// The command line the app comes with, which the schedule runs: for an app, the one
    /// `app_command_line` finds inside it, as discovery reads it. `None` for an app with
    /// none, such as a build run from a build directory. The core schedules the program
    /// asking where none is named, and that is the app, which renews nothing.
    pub(crate) helper: Option<PathBuf>,
    /// Where each way of installing Pitboard puts `pitboard`, as the core's
    /// `command_line_places` gives them for the app's home: under it, and where the system's
    /// package managers put programs. Looked in after the login shell's `PATH` for the one a
    /// terminal would run.
    pub(crate) command_line_places: Vec<PathBuf>,
}

impl Made {
    fn from_app(found: AppContext) -> Made {
        Made {
            helper: found.context.schedule_program().map(Path::to_path_buf),
            command_line_places: found.command_line_places,
            found: found.found,
            search_path: found.search_path,
            core: service::Pitboard::new(found.context),
        }
    }
}

/// What the command line inside this copy of the app is, which decides whether daily renewal
/// can run it long after the app has quit and whether a link to it would keep working.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct OwnCommandLine {
    /// The copy has one inside it. A build run from a build directory has none.
    pub(crate) inside: bool,
    /// The copy runs from the temporary place macOS makes for an app opened where it was
    /// downloaded, which is gone once it quits, by the rule `schedule.rs` refuses it by.
    pub(crate) temporary: bool,
    /// The one inside is a program this user may run, as the core judges every program.
    pub(crate) runs: bool,
}

impl OwnCommandLine {
    /// Whether a schedule or a link would keep reaching it: there, lasting, and runnable.
    pub(crate) fn lasting(self) -> bool {
        self.inside && !self.temporary && self.runs
    }
}

/// How long after a login shell too slow to answer it is asked once more.
pub(crate) const ASK_AGAIN_AFTER: Duration = Duration::from_secs(60);

/// A `Made` made the first time it is asked for, by whichever thread asks first. Any other
/// thread asking meanwhile waits for that one rather than making a second.
///
/// One made from a login shell that answered too late is made once more when asked to, a
/// while later, and kept in its place if that answers: startup files are slowest while the
/// machine is busy logging in, which is when an app that opens at login first asks. Once
/// more and no more, because a shell that is always that slow would otherwise cost its
/// five seconds every time. A caller asking meanwhile gets what was made first rather than
/// waiting on the second ask.
struct Kept {
    make: Box<dyn Fn() -> (Made, bool) + Send + Sync>,
    again_after: Duration,
    held: Mutex<Option<Held>>,
}

struct Held {
    made: Arc<Made>,
    /// What it was made from came too late.
    late: bool,
    at: Instant,
    asked_again: bool,
}

impl Kept {
    fn held(&self) -> MutexGuard<'_, Option<Held>> {
        // A panic while making leaves nothing made, which the next ask makes again.
        self.held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn value(&self) -> Arc<Made> {
        let mut held = self.held();
        if let Some(held) = held.as_ref() {
            return held.made.clone();
        }
        let (made, late) = (self.make)();
        let made = Arc::new(made);
        *held = Some(Held {
            made: made.clone(),
            late,
            at: Instant::now(),
            asked_again: false,
        });
        made
    }

    /// The value, made once more first when what it was made from came too late and at
    /// least `again_after` has passed since.
    fn value_asking_again(&self) -> Arc<Made> {
        let first = self.value();
        let due = match self.held().as_mut() {
            Some(held)
                if held.late && !held.asked_again && held.at.elapsed() >= self.again_after =>
            {
                held.asked_again = true;
                true
            }
            _ => false,
        };
        if !due {
            return self.value();
        }
        let (again, late) = (self.make)();
        if late {
            return first;
        }
        let again = Arc::new(again);
        *self.held() = Some(Held {
            made: again.clone(),
            late: false,
            at: Instant::now(),
            asked_again: true,
        });
        again
    }
}

/// The app's core, which the model's lanes call: made the first time one of them needs it,
/// as `Kept` keeps it.
pub(crate) struct AppCore {
    made: Kept,
}

impl AppCore {
    /// The app's core. It reads `environment`, the one the app was started with, by the
    /// code the command line reads its own with, and the schedule runs the command line that
    /// comes with the app at `app`.
    ///
    /// The person's login shell is asked for its `PATH` when something first needs the core,
    /// never here: it can take seconds, and an app makes its model on its main thread. Each
    /// tool's program is looked for on that `PATH` and then where its installers put it.
    ///
    /// Nothing is read with it where nothing may be read here, as the command line reads
    /// nothing then ([`AppCore::readable`]): every read the model's lanes make asks first, and
    /// every change asks the gate the core's changes pass, which asks the same.
    pub(crate) fn for_app(environment: HashMap<String, String>, app: Option<String>) -> Arc<Self> {
        let environment: Environment = environment.into_iter().collect();
        let app = app.map(PathBuf::from);
        Arc::new(AppCore::asking(
            move || {
                let found = AppContext::discover(&environment, app.as_deref());
                let late = found.late;
                (Made::from_app(found), late)
            },
            ASK_AGAIN_AFTER,
        ))
    }

    /// A core made by `make` on first use, which also says whether the login shell it asked
    /// was too slow to answer.
    pub(crate) fn asking(
        make: impl Fn() -> (Made, bool) + Send + Sync + 'static,
        again_after: Duration,
    ) -> AppCore {
        AppCore {
            made: Kept {
                make: Box::new(make),
                again_after,
                held: Mutex::new(None),
            },
        }
    }

    fn core(&self) -> Arc<Made> {
        self.made.value()
    }

    /// The core, once the app may read anything with it, as the command line asks before
    /// every command. A build that may do nothing on its system, a Windows build of a release
    /// before Pitboard for Windows is released (`pitboard_core::release`), is refused with
    /// `windows_not_released` before the core is made. A home the environment names that is
    /// empty or relative is refused with `home_not_absolute`: every read under it would land
    /// in whichever folder the app was started from, `app.json` and `state.json` included.
    ///
    /// Every read the model's lanes make asks this first, and answers the refusal, or nothing
    /// where it has no way to say one. A change asks the gate the core's changes pass, which
    /// refuses the same before anything else.
    fn readable(&self) -> Result<Arc<Made>, PitboardError> {
        Self::may_read(|| self.core())
    }

    /// The same, where the core may be made once more first, as `Kept::value_asking_again`
    /// says.
    fn readable_asking_again(&self) -> Result<Arc<Made>, PitboardError> {
        Self::may_read(|| self.made.value_asking_again())
    }

    /// The core `made` makes, once the release gate lets this build do anything, and then
    /// once every home it names is a full path.
    fn may_read(made: impl FnOnce() -> Arc<Made>) -> Result<Arc<Made>, PitboardError> {
        pitboard_core::release::check()?;
        let made = made();
        made.core.check_homes()?;
        Ok(made)
    }

    /// What the app keeps in `file` in Pitboard's directory, as the model reads it: `None`
    /// where it keeps nothing there, and an error where the file is there and cannot be read,
    /// or where nothing may be read here ([`AppCore::readable`]), which the model takes alike:
    /// what may be there is kept as it is, and never written over unread.
    pub(crate) fn app_file(
        &self,
        file: pitboard_core::app::AppFile,
    ) -> std::io::Result<Option<String>> {
        self.readable()
            .map_err(std::io::Error::other)?
            .core
            .app_file(file)
    }

    /// Keeps `body` in the app's `file` in Pitboard's directory, as the core writes its own.
    pub(crate) fn keep_app_file(
        &self,
        file: pitboard_core::app::AppFile,
        body: &str,
    ) -> Result<(), PitboardError> {
        Ok(self.core().core.keep_app_file(file, body)?)
    }

    /// What the command line inside this copy of the app is. Asks the file system about that
    /// one path, as `can_run` does.
    pub(crate) fn own_command_line(&self) -> OwnCommandLine {
        let made = self.core();
        match made.helper.as_deref() {
            None => OwnCommandLine::default(),
            Some(helper) => OwnCommandLine {
                inside: true,
                temporary: pitboard_core::schedule::in_a_temporary_copy(helper),
                runs: pitboard_core::app::can_run(helper),
            },
        }
    }

    /// The first `pitboard` a terminal would run, as the core's `find_command_line` finds it:
    /// on the login shell's `PATH`, then where each way of installing Pitboard puts it, and
    /// whether it is the one inside this copy of the app. Looks along a search path, and may
    /// ask the login shell again. None is found where nothing may be read here.
    pub(crate) fn command_line(&self) -> FoundCommandLine {
        let Ok(made) = self.readable_asking_again() else {
            return FoundCommandLine::Nowhere;
        };
        found_command_line(pitboard_core::app::find_command_line(
            made.search_path.as_deref().map(std::ffi::OsStr::new),
            &made.command_line_places,
            made.helper.as_deref(),
        ))
    }

    /// The tools whose program was named or found, in the order a listing shows them. A
    /// tool missing here may still be on some `PATH`, so this narrows what is offered and
    /// never forbids anything. May ask the login shell again. None where nothing may be read
    /// here, where nothing could be enrolled either.
    pub(crate) fn installed(&self) -> Vec<Tool> {
        let Ok(made) = self.readable_asking_again() else {
            return Vec::new();
        };
        ProviderId::ALL
            .iter()
            .copied()
            .filter(|tool| made.found.contains(tool))
            .map(tool)
            .collect()
    }

    /// Every account of every tool and what it has left, each asked of its own service.
    /// Parked logins whose access has lapsed are renewed first.
    ///
    /// `fresh` asks about every account whatever was asked moments ago. False for a poll
    /// and true when somebody asked for it: an account is otherwise only asked about again
    /// once its tightest limit could have moved by a percentage point, which is what keeps
    /// the app and the command line to one request between them.
    pub(crate) fn status(&self, fresh: bool) -> Result<Status, PitboardError> {
        Ok(status_of(self.readable()?.core.status(fresh)?))
    }

    pub(crate) fn switch_to(&self, label: String) -> Result<Switched, PitboardError> {
        changed(self.core().core.switch_to(&label), |outcome, warnings| {
            Switched {
                outcome: match outcome {
                    switch::Outcome::Switched {
                        provider,
                        from,
                        to,
                        adoption,
                        ..
                    } => Switch::Switched {
                        provider: provider.code().into(),
                        from,
                        to,
                        adoption: adoption_of(adoption),
                    },
                    switch::Outcome::AlreadyActive { label, .. } => Switch::AlreadyActive { label },
                },
                warnings,
            }
        })
    }

    /// Writes the account `label` names into its tool's config where that names another
    /// account, while it is in use, and refuses where it no longer is. Gives what it warned
    /// of.
    pub(crate) fn update_config(&self, label: String) -> Result<Vec<Warning>, PitboardError> {
        changed(self.core().core.update_config(&label), |_, warnings| {
            warnings
        })
    }

    /// Switches Claude Code by itself where a limit of the account in use has reached `at`%
    /// and another account has room, as the core decides under its lock. `at` is taken as the
    /// nearest share there can be.
    pub(crate) fn auto_switch(&self, at: u8) -> Result<AutoSwitched, PitboardError> {
        let threshold = Threshold::clamped(i64::from(at));
        changed(self.core().core.auto_switch(threshold), |auto, warnings| {
            auto_switched(auto, warnings, threshold)
        })
    }

    /// What switching Claude Code by itself at `at`% would come to, from the core's files
    /// alone: what stands, or that only a decision under its lock can say. Takes no lock,
    /// reads no keychain and asks nobody, so the model asks it after every read and every half
    /// a minute.
    pub(crate) fn auto_look(&self, at: u8) -> Result<AutoLooked, PitboardError> {
        let threshold = Threshold::clamped(i64::from(at));
        Ok(match self.readable()?.core.auto_look(threshold)? {
            Look::Stands(auto) => AutoLooked::Stands(auto_switched(*auto, Vec::new(), threshold)),
            Look::Act => AutoLooked::Act,
        })
    }

    /// Enroll the account signed in now under `label`.
    pub(crate) fn enroll_current(&self, label: String) -> Result<Enrolled, PitboardError> {
        changed(self.core().core.enroll_current(&label), enrolled)
    }

    /// Starts the tool's own sign-in for a new account, watched rather than inherited. The
    /// label may name the tool, as in `codex/work`; a bare one means Claude Code. The model
    /// hands on what the tool says, can paste a fallback code, and finishes it.
    ///
    /// May ask the login shell again first, as `installed` does: a sign-in is when a program
    /// the first, late, ask could not find is needed.
    pub(crate) fn sign_in(&self, label: String) -> Result<Arc<SignInSession>, PitboardError> {
        let made = self.made.value_asking_again();
        let watched = made.core.sign_in_watched(&label)?;
        Ok(Arc::new(SignInSession {
            said: watched.said(),
            watched: Mutex::new(Some(watched)),
            label,
            made,
        }))
    }

    pub(crate) fn forget(&self, label: String) -> Result<(), PitboardError> {
        changed(self.core().core.forget(&label), |_, _| ())
    }

    pub(crate) fn rename(&self, from: String, to: String) -> Result<(), PitboardError> {
        changed(self.core().core.rename(&from, &to), |_, _| ())
    }

    /// When Pitboard's account index last changed, in epoch seconds, or 0 when there is
    /// none, or where nothing may be read here.
    ///
    /// One stat of one file, so the model asks often. A switch typed in a terminal used to
    /// leave the menu bar naming the account the person had just stopped using, for as
    /// long as five minutes, with a button offering a switch that had already happened.
    /// The model polls this, and when it moves, reads `status_offline`: no network, and no
    /// keychain unless an interrupted switch is waiting.
    pub(crate) fn changed_at(&self) -> i64 {
        self.readable().map_or(0, |made| made.core.changed_at())
    }

    /// When Pitboard's usage readings last changed, in epoch milliseconds, or 0 when there
    /// are none, or where nothing may be read here.
    ///
    /// Every read records its answers, and every session's status line what that session has
    /// seen where it moves a limit an answer gave, so what is remembered is the newest any
    /// front end has. The model polls this beside `changed_at`, and when it moves, takes the
    /// numbers from `status_offline`: no network, and no keychain unless an interrupted
    /// switch is waiting. Only the numbers: a reading moving says nothing about who is signed
    /// in, which is `changed_at`'s to say.
    pub(crate) fn readings_changed_at(&self) -> i64 {
        self.readable()
            .map_or(0, |made| made.core.readings_changed_at())
    }

    /// The same report without asking anyone: the last numbers Pitboard measured, and whose
    /// login each tool has stored, as its service last said.
    ///
    /// What the app shows on a plane, and what it shows while a live read is still in
    /// flight, rather than an empty panel and a spinner. It warns of an interrupted switch
    /// nothing can finish as `status` does, wherever that can be told without a request.
    pub(crate) fn status_offline(&self) -> Result<Status, PitboardError> {
        Ok(status_of(self.readable()?.core.status_offline()?))
    }

    /// Give up on an interrupted switch that cannot be finished, keeping every login it
    /// names. The way out when recovery cannot reach Anthropic, which until now sent the
    /// person to a terminal.
    ///
    /// `None` when there was no interrupted switch.
    pub(crate) fn abandon_recovery(&self) -> Result<Option<Abandoned>, PitboardError> {
        Ok(self.core().core.abandon_recovery()?.map(|a| Abandoned {
            from: a.from,
            to: a.to,
            logins_kept: u32::try_from(a.kept).unwrap_or(u32::MAX),
        }))
    }

    /// What Pitboard has changed, newest last: nothing where nothing may be read here, as
    /// for a log that cannot be read.
    pub(crate) fn log(&self, limit: u32) -> Vec<Change> {
        let Ok(made) = self.readable() else {
            return Vec::new();
        };
        made.core
            .log(limit as usize)
            .into_iter()
            .map(|e| Change {
                at: e.at,
                caller: e.caller,
                verb: e.verb,
                subject: e.subject,
                outcome: e.outcome,
            })
            .collect()
    }

    /// Renew every parked login that is due, and nothing else. Refused, as every change is,
    /// where this process may change nothing.
    pub(crate) fn renew(&self) -> Result<Vec<Renewed>, PitboardError> {
        Ok(self
            .core()
            .core
            .renew()?
            .into_iter()
            .map(|(key, outcome)| Renewed {
                label: key.typed(),
                provider: key.provider.code().into(),
                outcome: outcome.code().to_string(),
            })
            .collect())
    }

    /// The proof the one gate every change passes hands out, where this process may change
    /// anything, for a file the app keeps of its own outside Pitboard's directory.
    pub(crate) fn permit(&self) -> Result<service::Permit, PitboardError> {
        Ok(self.core().core.permit()?)
    }

    /// Whether anything keeps parked logins alive without a command being run. `None` where
    /// nothing may be read here, which is not a schedule that is absent.
    pub(crate) fn schedule(&self) -> Option<Schedule> {
        Some(match self.readable().ok()?.core.schedule() {
            pitboard_core::schedule::Installed::Yes {
                path,
                every_seconds,
            } => Schedule::Installed {
                path: path.to_string_lossy().into_owned(),
                every_seconds,
            },
            pitboard_core::schedule::Installed::No => Schedule::Absent,
            pitboard_core::schedule::Installed::Unsupported => Schedule::Unsupported,
        })
    }

    /// Ask this computer's own scheduler to renew parked logins daily. Opt-in, and the
    /// settings say what it does before offering it. Refused where the app named no command
    /// line to run, and first, as every change is, where this process may change nothing.
    pub(crate) fn schedule_install(&self) -> Result<String, PitboardError> {
        let made = self.core();
        made.core.permit()?;
        if made.helper.is_none() {
            return Err(pitboard_core::error::Error::ScheduleProgramUnnamed.into());
        }
        Ok(made.core.schedule_install()?.to_string_lossy().into_owned())
    }

    /// Take it away. `false` when there was nothing installed.
    pub(crate) fn schedule_uninstall(&self) -> Result<bool, PitboardError> {
        Ok(self.core().core.schedule_uninstall()?)
    }

    /// Point a schedule an app up to 0.3.0 wrote at the command line this app comes with.
    /// That app scheduled itself, so launchd has been starting a second app every day and
    /// renewing nothing. The model asks once a launch: `true` when it repaired one, and
    /// nothing changes where the schedule already runs a command line or there is none.
    pub(crate) fn schedule_repair(&self) -> Result<bool, PitboardError> {
        Ok(self.core().core.schedule_repair()?)
    }

    /// What is running `provider`'s tool with a login a switch would leave it on, by kind,
    /// for the model to say so or to offer to quit an app first. Empty where nothing is, for
    /// a tool that follows a switch by itself, for a provider code nobody knows, and where
    /// nothing may be read here, where no switch is made either. Reads the process list and
    /// nothing else, so it answers at once.
    pub(crate) fn holding(&self, provider: String) -> Vec<Holding> {
        let Ok(made) = self.readable() else {
            return Vec::new();
        };
        pitboard_core::provider::ProviderId::parse(&provider)
            .map(|which| {
                made.core
                    .holding(which)
                    .into_iter()
                    .map(Holding::from)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every check `pitboard doctor` makes. `None` where this build may do nothing on its
    /// system (`pitboard_core::release`), which makes no check, as `pitboard doctor` there
    /// makes none. A home that is not a full path is doctor's own to say, as its `homes`
    /// check, failed, and nothing else.
    pub(crate) fn doctor(&self) -> Option<Vec<Check>> {
        pitboard_core::release::check().ok()?;
        let checks = self
            .core()
            .core
            .doctor()
            .checks
            .into_iter()
            .map(|c| Check {
                code: c.code.to_string(),
                name: c.name,
                level: match c.level {
                    doctor::Level::Ok => Level::Ok,
                    doctor::Level::Warn => Level::Warn,
                    doctor::Level::Fail => Level::Fail,
                },
                detail: c.detail,
                advice: c.advice,
            })
            .collect();
        Some(checks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools;
    use pitboard_core::context::Context;

    /// A core over a home nothing can be written under, with a `codex` named where one is
    /// given, and scheduling the command line at `helper` where one is.
    fn made(codex: Option<&str>, helper: Option<&Path>) -> Made {
        let mut ctx = Context::new(PathBuf::from("/dev/null")).with_caller("app".into());
        if let Some(codex) = codex {
            ctx = ctx.with_codex_program(PathBuf::from(codex));
        }
        if let Some(helper) = helper {
            ctx = ctx.with_schedule_program(helper.to_path_buf());
        }
        Made {
            found: codex.map(|_| ProviderId::Codex).into_iter().collect(),
            search_path: None,
            helper: helper.map(Path::to_path_buf),
            command_line_places: pitboard_core::app::command_line_places(Path::new("/dev/null")),
            core: service::Pitboard::new(ctx),
        }
    }

    fn installed(core: &AppCore) -> Vec<String> {
        core.installed().into_iter().map(|tool| tool.code).collect()
    }

    /// The environment of an app whose home is `home`, whose login shell cannot be run, so
    /// no test starts the person's own, and with `more` beside.
    fn environment(home: &str, more: &[(&str, &str)]) -> HashMap<String, String> {
        [("HOME", home), ("SHELL", "/nowhere/at/all/sh")]
            .iter()
            .chain(more)
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    /// Counts how often the app's core was made.
    #[derive(Clone, Default)]
    struct Asks(Arc<std::sync::atomic::AtomicUsize>);

    impl Asks {
        /// One more ask, and how many there have been with it.
        fn note(&self) -> usize {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
        }

        fn count(&self) -> usize {
            self.0.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    /// The app's core never schedules itself: where it names no command line to run,
    /// scheduling this process would start a second app every day and renew nothing. The
    /// home is one nothing can be written under, so even a regression here reaches no
    /// scheduler.
    #[test]
    #[cfg_attr(windows, ignore = "W14: Pitboard's own folder on Windows")]
    fn the_app_schedules_nothing_without_a_command_line_to_run() {
        let core = AppCore::for_app(environment("/dev/null", &[]), None);
        let Err(PitboardError::Failed { code, message, .. }) = core.schedule_install() else {
            panic!("the app scheduled itself");
        };
        assert_eq!(code, "schedule_program_unnamed");
        assert!(message.contains("command line"), "{message}");
    }

    /// Where Pitboard may change nothing, as root or under sudo, the app's schedule is
    /// refused for that before anything else, as every change is: not for a command line
    /// the app does not name, which would send the person to install the app again.
    #[test]
    fn the_app_schedules_nothing_where_pitboard_may_change_nothing() {
        let host = pitboard_core::host::memory::MemoryHost::new();
        host.runs_with(pitboard_core::host::Elevation::Elevated { why: "as root" });
        let core = AppCore::asking(
            move || {
                let mut made = made(None, None);
                let ctx = Context::new(PathBuf::from("/dev/null"))
                    .with_caller("app".into())
                    .with_memory_stores(Arc::clone(&host));
                made.core = service::Pitboard::new(ctx);
                (made, false)
            },
            ASK_AGAIN_AFTER,
        );
        let Err(PitboardError::Failed { code, .. }) = core.schedule_install() else {
            panic!("the app scheduled itself as root");
        };
        assert_eq!(code, "elevated");
    }

    /// The app turns on daily renewal through the core's own install, so it is refused as
    /// the command line's is while `PITBOARD_HOME` names a directory other than the default
    /// home, which is all the schedule renews, and the pretend scheduler is left with nothing
    /// installed. The same app with its default home installs it.
    #[test]
    #[cfg_attr(windows, ignore = "W25: Task Scheduler")]
    fn the_app_schedules_nothing_where_pitboard_home_is_another_directory() {
        let home = std::env::temp_dir().join(format!(
            "pitboard-app-schedule-home-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        let helper = home.join("Applications/Pitboard.app/Contents/Helpers/pitboard");
        std::fs::create_dir_all(helper.parent().expect("its folder")).expect("made");
        std::fs::write(&helper, "").expect("a command line");
        let app = |pitboard: PathBuf| {
            let (home, helper) = (home.clone(), helper.clone());
            let host = pitboard_core::host::memory::MemoryHost::new();
            AppCore::asking(
                move || {
                    let mut made = made(None, Some(&helper));
                    let ctx = Context::new(home.clone())
                        .with_pitboard_home(pitboard.clone())
                        .with_caller("app".into())
                        .with_schedule_program(helper.clone())
                        .with_memory_stores(Arc::clone(&host));
                    made.core = service::Pitboard::new(ctx);
                    (made, false)
                },
                ASK_AGAIN_AFTER,
            )
        };

        let elsewhere = app(home.join("elsewhere"));
        let Err(PitboardError::Failed { code, message, .. }) = elsewhere.schedule_install() else {
            panic!("the app scheduled the default home's renewal from another home");
        };
        assert_eq!(code, "schedule_not_default_home");
        assert!(message.contains("unset PITBOARD_HOME"), "{message}");
        assert_eq!(elsewhere.schedule(), Some(Schedule::Absent));

        let default = app(home.join(".pitboard"));
        default.schedule_install().expect("installed");
        assert!(matches!(
            default.schedule(),
            Some(Schedule::Installed { .. })
        ));
        assert_eq!(elsewhere.schedule_uninstall().ok(), Some(true));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Where a home the environment names is empty or relative, the app's core reads nothing
    /// under it, as the command line reads nothing: every read the model's lanes make answers
    /// the refusal, or nothing where it has no way to say one, and never what lies under
    /// whichever folder the app was started from. What the app keeps there reads as a file
    /// that cannot be read, which the model never writes over. Doctor says it as its `homes`
    /// check, and checks nothing else.
    #[test]
    fn the_apps_core_reads_nothing_under_a_home_that_is_not_a_full_path() {
        // `/Users/x` is no full path on Windows; this one is everywhere, and is never made.
        let full = std::env::temp_dir().join(format!(
            "pitboard-app-no-full-home-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        assert!(!full.exists(), "{} is there already", full.display());
        for (home, pitboard, variable) in [
            ("relative".into(), full.join(".pitboard"), "HOME"),
            (full.clone(), "pitboard".into(), "PITBOARD_HOME"),
            (full.clone(), PathBuf::new(), "PITBOARD_HOME"),
        ] {
            let host = pitboard_core::host::memory::MemoryHost::new();
            let given = pitboard.clone();
            let core = AppCore::asking(
                move || {
                    let mut made = made(Some("/nowhere/codex"), None);
                    let ctx = Context::new(PathBuf::clone(&home))
                        .with_pitboard_home(given.clone())
                        .with_caller("app".into())
                        .with_memory_stores(Arc::clone(&host));
                    made.core = service::Pitboard::new(ctx);
                    (made, false)
                },
                ASK_AGAIN_AFTER,
            );
            let refused = |read: Result<Status, PitboardError>| match read {
                Err(PitboardError::Failed { code, message, .. }) => {
                    assert_eq!(code, "home_not_absolute", "{variable}");
                    assert!(message.starts_with(variable), "{message}");
                }
                Ok(_) => panic!("read under {variable} {pitboard:?}"),
            };
            refused(core.status(false));
            refused(core.status_offline());
            for file in [
                pitboard_core::app::AppFile::Told,
                pitboard_core::app::AppFile::Preferences,
            ] {
                let unread = core.app_file(file).expect_err("not read");
                assert!(unread.to_string().starts_with(variable), "{unread}");
            }
            assert_eq!((core.changed_at(), core.readings_changed_at()), (0, 0));
            assert!(core.log(500).is_empty());
            assert_eq!(core.schedule(), None, "not a schedule that is absent");
            assert!(core.holding("codex".into()).is_empty());
            assert!(installed(&core).is_empty(), "{variable}");
            assert_eq!(core.command_line(), FoundCommandLine::Nowhere);
            let checks = core.doctor().expect("doctor's own to say");
            let homes = checks
                .iter()
                .find(|check| check.code == "homes")
                .expect("the homes check");
            assert_eq!(homes.level, Level::Fail);
            assert!(homes.detail.starts_with(variable), "{}", homes.detail);
            assert!(
                checks
                    .iter()
                    .all(|check| ["system_too_old", "elevated", "homes"]
                        .contains(&check.code.as_str())),
                "nothing else is checked"
            );
        }
        assert!(!full.exists(), "{} was written", full.display());
    }

    /// Repairing at launch is a no-op wherever there is nothing to repair, and never
    /// reaches a scheduler to find that out: in a core made as the app launches, which
    /// outside an app names no command line, and in one that names this test's own program,
    /// which stands in for a command line that is there.
    #[test]
    #[cfg_attr(windows, ignore = "W14: Pitboard's own folder on Windows")]
    fn the_app_repairs_nothing_where_no_schedule_runs_it() {
        let launched = AppCore::for_app(environment("/dev/null", &[]), None);
        assert!(!launched.schedule_repair().expect("nothing to repair"));
        let there = std::env::current_exe().expect("this test's own program");
        let named = AppCore::asking(move || (made(None, Some(&there)), false), ASK_AGAIN_AFTER);
        assert!(!named.schedule_repair().expect("nothing to repair"));
    }

    /// Where the tools are can take asking the person's login shell, so the core is made when
    /// something first needs it, and not when the app makes its model, which it does on its
    /// main thread; and once, however many calls arrive at the same time.
    #[test]
    #[cfg_attr(windows, ignore = "W14: Pitboard's own folder on Windows")]
    fn the_core_is_made_once_when_it_is_first_needed() {
        let asks = Asks::default();
        let core = Arc::new(AppCore::asking(
            {
                let asks = asks.clone();
                move || {
                    asks.note();
                    std::thread::sleep(Duration::from_millis(100));
                    (made(Some("/nowhere/codex"), None), false)
                }
            },
            ASK_AGAIN_AFTER,
        ));
        assert_eq!(tools().len(), 2, "listing the tools asks nothing");
        assert_eq!(asks.count(), 0, "and nor does making the core");
        let calls: Vec<_> = (0..3)
            .map(|which| {
                let core = core.clone();
                std::thread::spawn(move || match which {
                    0 => drop(core.installed()),
                    1 => drop(core.changed_at()),
                    _ => drop(core.command_line()),
                })
            })
            .collect();
        for call in calls {
            call.join().expect("a call that answers");
        }
        assert_eq!(installed(&core), ["codex"]);
        assert_eq!(asks.count(), 1);
    }

    /// A login shell too slow to answer, which startup files are while the machine is busy
    /// logging in, is asked once more when something next asks what is installed, a while
    /// later, and what it answers then is what is used. Once more and no more: a shell that
    /// is always that slow would otherwise cost its patience on every ask.
    #[test]
    #[cfg_attr(windows, ignore = "W14: Pitboard's own folder on Windows")]
    fn a_login_shell_too_slow_to_answer_is_asked_once_more_later() {
        let asks = Asks::default();
        let core = AppCore::asking(
            {
                let asks = asks.clone();
                move || match asks.note() {
                    1 => (made(None, None), true),
                    _ => (made(Some("/nowhere/codex"), None), false),
                }
            },
            Duration::from_millis(200),
        );
        assert!(
            installed(&core).is_empty(),
            "what the first, late, ask found"
        );
        assert!(
            installed(&core).is_empty(),
            "and nothing more until the while is up"
        );
        assert_eq!(asks.count(), 1);
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(installed(&core), ["codex"]);
        assert_eq!(asks.count(), 2);
        assert_eq!(installed(&core), ["codex"]);
        assert_eq!(asks.count(), 2, "asked once more, and no more");
    }

    /// Not before the while is up, and never when the shell answered or could not be asked.
    /// A second ask that is late as well leaves what the first made in its place.
    #[test]
    fn a_login_shell_is_not_asked_again_sooner_or_for_nothing() {
        let soon = Asks::default();
        let early = AppCore::asking(
            {
                let soon = soon.clone();
                move || {
                    soon.note();
                    (made(None, None), true)
                }
            },
            Duration::from_secs(3600),
        );
        installed(&early);
        installed(&early);
        assert_eq!(soon.count(), 1);

        let answered = Asks::default();
        let core = AppCore::asking(
            {
                let answered = answered.clone();
                move || {
                    answered.note();
                    (made(None, None), false)
                }
            },
            Duration::ZERO,
        );
        installed(&core);
        installed(&core);
        assert_eq!(answered.count(), 1);

        let late = Asks::default();
        let again = AppCore::asking(
            {
                let late = late.clone();
                move || match late.note() {
                    1 => (made(None, None), true),
                    _ => (made(Some("/nowhere/codex"), None), true),
                }
            },
            Duration::ZERO,
        );
        assert!(installed(&again).is_empty());
        assert!(installed(&again).is_empty(), "the second ask was late too");
        assert!(installed(&again).is_empty());
        assert_eq!(late.count(), 2);
    }

    /// The app's core reads the environment it was started with as the command line reads its
    /// own, and asks the login shell that environment names. A shell that cannot be run is no
    /// answer, so nothing is said to be on its `PATH`, and a program named outright counts as
    /// found. A custom OAuth endpoint refuses a change to a Claude Code account, and an app
    /// outside a bundle has no command line to schedule.
    #[test]
    fn the_apps_core_is_made_from_the_environment_it_was_started_with() {
        // A home of its own, so Claude Code's slot is hashed from it and nothing real is read.
        let home = std::env::temp_dir().join(format!("pitboard-ffi-app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("a scratch home");
        let at = |name: &str| home.join(name).to_string_lossy().into_owned();
        let (pitboard, claude, codex) = (at("pitboard"), at("claude"), at("codex"));
        let core = AppCore::for_app(
            environment(
                &at(""),
                &[
                    ("PITBOARD_HOME", &pitboard),
                    ("CLAUDE_CONFIG_DIR", &claude),
                    ("CODEX_HOME", &codex),
                    ("PITBOARD_CODEX", "/nowhere/codex"),
                    ("CLAUDE_CODE_CUSTOM_OAUTH_URL", "https://oauth.example"),
                ],
            ),
            None,
        );
        assert!(installed(&core).contains(&"codex".to_owned()));
        assert_eq!(core.core().search_path, None, "the shell could not be run");
        let Err(PitboardError::Failed { code, .. }) = core.enroll_current("work".into()) else {
            panic!("a Claude Code account was enrolled under a custom OAuth endpoint");
        };
        assert_eq!(code, "custom_oauth_endpoint");
        let Err(PitboardError::Failed { code, .. }) = core.schedule_install() else {
            panic!("an app outside a bundle scheduled itself");
        };
        assert_eq!(code, "schedule_program_unnamed");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A reason not to switch away from a limit is said once for that limit and its reset, as
    /// the core records it: the limit's next window, another limit and another reason are each
    /// another.
    #[test]
    fn a_reason_not_to_switch_is_told_apart_by_its_limit_its_reset_and_its_code() {
        let limit = |kind: &str, scope: Option<&str>, resets_at: i64| usage::Window {
            kind: kind.into(),
            scope: scope.map(str::to_owned),
            percent: 97.0,
            resets_at: Some(resets_at),
            is_active: true,
            severity: None,
            length_seconds: None,
        };
        let session = limit("session", None, 9_000);
        assert_eq!(
            skipped_key("work", &session, &Skip::GaveUp),
            "skipped/work/session//9000/attempts_spent"
        );
        let keys = [
            skipped_key("work", &session, &Skip::GaveUp),
            skipped_key("work", &limit("session", None, 27_000), &Skip::GaveUp),
            skipped_key(
                "work",
                &limit("weekly_scoped", Some("Opus"), 9_000),
                &Skip::GaveUp,
            ),
            skipped_key("work", &session, &Skip::AlreadyLeft),
            skipped_key("home", &session, &Skip::GaveUp),
        ];
        let told_apart: std::collections::BTreeSet<&String> = keys.iter().collect();
        assert_eq!(told_apart.len(), keys.len(), "{keys:?}");
    }
}
