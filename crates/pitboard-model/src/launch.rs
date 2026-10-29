//! The core the model's lanes call: made from what the app was started with, the first time
//! a lane needs it, and kept for as long as the app runs. Not exported: an app reaches the
//! core only through the model, whose calls wait on nothing, while each call here is
//! synchronous and may block on the keychain, a lock, the network or the person's login
//! shell, so only the model's own threads make one.
//!
//! What the core answers is said in the model's records, such as `Status` and `Abandoned`,
//! and in the types here, such as what a switch, an enrolment or a renewal came to, doctor's
//! checks, and why something failed. Only what `present` makes of them reaches an app.

use crate::model::state::in_its_app;
use crate::present::words::{in_app_given_up, in_app_refusal, in_app_warning, sentence, typeset};
use crate::{
    Abandoned, Account, Awaiting, FoundCommandLine, Front, Level, Parked, Schedule, Status, Usage,
    Warning, found_command_line,
};
use pitboard_core::app::AppContext;
use pitboard_core::context::Environment;
use pitboard_core::duty::autoswitch::{Auto, Blind, Look, Skip, Threshold};
use pitboard_core::duty::warm::{Overview, Range};
use pitboard_core::duty::written::Written;
use pitboard_core::provider::{self, ProviderId};
use pitboard_core::service::{self, Changing};
use pitboard_core::state::Key;
use pitboard_core::switch::warm::WarmUp;
use pitboard_core::switch::{Left, Stowed};
use pitboard_core::{doctor, status, switch, words};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Why something the core was asked to do did not happen.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PitboardError {
    // `account`: the one it is about, for its next step
    #[error("{message}")]
    Failed {
        code: String,
        message: String,
        warnings: Vec<Warning>,
        account: Option<Key>,
    },
}

impl PitboardError {
    // The core's refusal, in `front`'s words.
    fn refused(front: Front, error: &pitboard_core::error::Error) -> PitboardError {
        PitboardError::Failed {
            code: error.code().to_string(),
            message: in_app_refusal(front, error).unwrap_or_else(|| sentence(&error.to_string())),
            warnings: Vec::new(),
            account: account_of(error),
        }
    }

    // A change the core refused, and what it warned of on the way, in `front`'s words.
    fn failed(front: Front, failed: &service::Failed) -> PitboardError {
        PitboardError::Failed {
            code: failed.error.code().to_string(),
            message: in_app_refusal(front, &failed.error)
                .unwrap_or_else(|| sentence(&failed.error.to_string())),
            warnings: warnings(front, &failed.warnings),
            account: account_of(&failed.error),
        }
    }
}

// What the core answered, a refusal said in the words of the front that asked.
trait Said<T> {
    fn said(self, front: Front) -> Result<T, PitboardError>;
}

impl<T> Said<T> for Result<T, pitboard_core::error::Error> {
    fn said(self, front: Front) -> Result<T, PitboardError> {
        self.map_err(|error| PitboardError::refused(front, &error))
    }
}

impl<T> Said<T> for Result<T, service::Failed> {
    fn said(self, front: Front) -> Result<T, PitboardError> {
        self.map_err(|failed| PitboardError::failed(front, &failed))
    }
}

// The account a refusal is about, for its next step, from its label as the core typed it.
fn account_of(error: &pitboard_core::error::Error) -> Option<Key> {
    use pitboard_core::error::{Error, Unidentified};
    let of_tool = |tool: ProviderId, label: &str| Key::new(tool, Key::parse(label).label);
    match error {
        Error::NothingParked { tool, label }
        | Error::ParkedLoginRefused { tool, label }
        | Error::SignInNotKept { tool, label, .. } => Some(of_tool(*tool, label)),
        Error::ParkedLoginExpired { label }
        | Error::ParkedCredentialMissing { label }
        | Error::ParkedCredentialCorrupt { label, .. }
        | Error::ParkedLoginBelongsElsewhere { label, .. }
        | Error::AccountUnknown { label, .. } => Some(Key::parse(label)),
        Error::IdentityUnverifiable {
            tool,
            login: Unidentified::Parked { label } | Unidentified::SignIn { label },
            cause,
            ..
        } if !cause.worth_retrying() => Some(of_tool(*tool, label)),
        Error::RecoveryUndetermined { tool, to, .. }
        | Error::RecoveryElsewhere { tool, to, .. } => Some(of_tool(*tool, to)),
        Error::LiveAccountNotEnrolled { tool, .. } | Error::LiveCredentialAbsent { tool, .. } => {
            Some(Key::new(*tool, ""))
        }
        _ => None,
    }
}

#[derive(Debug)]
pub(crate) struct WarmRefused {
    pub(crate) told_apart: switch::warm::Apart,
    // boxed, as the Result it is the error of stays small
    pub(crate) error: Box<PitboardError>,
}

impl WarmRefused {
    fn of(front: Front, failed: &service::Failed) -> WarmRefused {
        WarmRefused {
            told_apart: switch::warm::told_apart(&failed.error),
            error: Box::new(PitboardError::failed(front, failed)),
        }
    }
}

fn warnings(front: Front, found: &[service::Warning]) -> Vec<Warning> {
    found
        .iter()
        .map(|w| Warning {
            code: w.code().to_string(),
            message: in_app_warning(front, w).unwrap_or_else(|| sentence(&w.to_string())),
            provider: w.tool(),
            account: match w {
                service::Warning::LoginReplaced { tool, id, .. } => Some(account_id(*tool, id)),
                _ => None,
            },
            held: match w {
                service::Warning::FallbackLogin { held, .. } => Some(match held {
                    provider::Held::Login => crate::Held::Login,
                    provider::Held::NoLogin => crate::Held::NoLogin,
                    provider::Held::Unreadable => crate::Held::Unreadable,
                }),
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

// What the gate every change passes refuses in `ctx`, as a read failure says it in `front`'s
// words.
pub(crate) fn refusal(
    front: Front,
    ctx: pitboard_core::context::Context,
) -> Option<crate::ReadFailure> {
    pitboard_core::release::set_version(env!("CARGO_PKG_VERSION"));
    let refused = service::Pitboard::new(ctx).permit().err()?;
    let PitboardError::Failed { code, message, .. } = PitboardError::refused(front, &refused);
    Some(crate::ReadFailure { code, message })
}

// What giving up came to: the switch, each move given up in the app's words, and warnings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct GaveUp {
    pub(crate) switch: Option<Abandoned>,
    pub(crate) moves: Vec<String>,
    pub(crate) warnings: Vec<Warning>,
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

pub(crate) use pitboard_core::provider::Adoption;

/// What makes one kind of process take a switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Remedy {
    /// Quit it and start it again.
    Restart,
    /// Quit the app the way its system quits an app, and open it again. The model may do
    /// both, where the front names other apps. `app` is what that system names it by, as
    /// `OtherApps` takes it.
    ReopenApp {
        app: crate::model::AppId,
        name: String,
    },
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
                    app: app.into(),
                    name: name.into(),
                },
                Core::Run { arguments, .. } => Remedy::Run {
                    command: format!("{} {arguments}", held.program),
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
        provider: ProviderId,
        // the account it left, by its label, or what the tool held in place of one
        from: String,
        to: Key,
        adoption: Adoption,
    },
    // Put in use where the tool held nobody's login: no account was parked for it.
    Installed {
        provider: ProviderId,
        to: Key,
        adoption: Adoption,
    },
    AlreadyActive {
        account: Key,
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
    /// says, with what the switch warned of, and how much the account it left has used: "96%
    /// of its 5-hour limit". `from` and `to` as the core types them, and `accounts` the two
    /// they name.
    Switched {
        from: String,
        to: String,
        accounts: (Key, Key),
        adoption: Adoption,
        warnings: Vec<Warning>,
        used: String,
        // when the limit it left resets
        resets_at: Option<i64>,
    },
    /// A limit of `from` reached the share, `used` of it, and Pitboard did not switch, for
    /// the reason `why` says, until `until` where the reason ends then. `key` tells it apart
    /// from another, as the core does (`Skip::told_apart`).
    Skipped {
        key: String,
        from: String,
        used: String,
        why: String,
        // the core's for the reason, `Skip::code`
        code: &'static str,
        until: Option<i64>,
    },
    /// Pitboard cannot judge whether to switch, for the reason `why` says, and asks again at
    /// `until` where it waits to. `key` tells it apart from another, as the core does
    /// (`Blind::told_apart`).
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
    Stands(Box<AutoSwitched>),
    /// A switch may be due, or whose login Claude Code has stored is in doubt: only a
    /// decision under the core's lock can say.
    Act,
}

// What uninstalling came to, where the core did not refuse it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Uninstalled {
    // Pitboard's directory is gone
    Removed,
    // the directory stays, for `n` saved sign-ins not deleted yet
    Pending(usize),
    WarmingUp,
    // another program holds a file in the directory open
    HeldOpen,
}

impl Uninstalled {
    // In the order the command line says them.
    fn of(removed: &switch::Removed) -> Uninstalled {
        if removed.pending > 0 {
            Uninstalled::Pending(removed.pending)
        } else if removed.warming_up {
            Uninstalled::WarmingUp
        } else if !removed.home_kept_by.is_empty() || !removed.home_removed {
            Uninstalled::HeldOpen
        } else {
            Uninstalled::Removed
        }
    }
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
    // the label it is filed under, bare
    pub(crate) label: String,
    // this enrolment added it, rather than an account enrolled already
    pub(crate) added: bool,
    pub(crate) outcome: EnrolledAs,
    pub(crate) warnings: Vec<Warning>,
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
    // for a check of one account
    pub(crate) account: Option<Key>,
}

// Doctor's checks, and its report as `pitboard doctor --json` gives it, for Copy Report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Doctored {
    pub(crate) checks: Vec<Check>,
    pub(crate) report: String,
}

fn enrolled(enrolment: switch::Enrolment, warnings: Vec<Warning>) -> Enrolled {
    let (email, outcome) = match enrolment.enrolled {
        switch::Enrolled::Current { email } => (email, EnrolledAs::Current),
        switch::Enrolled::SignedIn { email } => (email, EnrolledAs::SignedIn),
        switch::Enrolled::Renewed { email } => (email, EnrolledAs::Renewed),
        switch::Enrolled::InUse { email, again } => (email, EnrolledAs::InUse { again }),
    };
    Enrolled {
        email,
        label: enrolment.key.label,
        added: enrolment.added,
        outcome,
        warnings,
    }
}

fn account(row: status::Row, now: i64) -> Account {
    let unplaced = row.unplaced();
    Account {
        id: account_id(row.provider, &row.id),
        provider: row.provider,
        key: row.key(),
        unplaced,
        standing: row.standing(now),
        stale: row.stale,
        // In the row's own tool's words: a Codex row is not about Anthropic.
        stale_explanation: crate::present::words::stale_explanation(&row)
            .map(|said| typeset(&words::capitalised(&said))),
        plan: row.plan,
        read_at: row.read_at,
        parked: row.parked.map(|p| Parked {
            refresh_expires_at: p.refresh_expires_at,
            session_expires_at: p.session_expires_at,
        }),
        usage: row.usage.map(|u| Usage {
            observed_at: u.observed_at,
            lists_every_limit: u.lists_every_limit,
            windows: u.windows,
        }),
        label: row.label,
        email: row.email,
        account_id: row.id,
        signed_in: row.signed_in,
    }
}

fn switched(front: Front, changing: Changing<switch::Outcome>) -> Result<Switched, PitboardError> {
    changed(front, changing, |outcome, warnings| Switched {
        outcome: match outcome {
            switch::Outcome::Switched {
                provider,
                from,
                to,
                adoption,
                ..
            } => Switch::Switched {
                provider,
                from: Key::parse(&from).label,
                to: Key::parse(&to),
                adoption,
            },
            switch::Outcome::Installed {
                provider,
                to,
                adoption,
            } => Switch::Installed {
                provider,
                to: Key::parse(&to),
                adoption,
            },
            switch::Outcome::AlreadyActive { label, .. } => Switch::AlreadyActive {
                account: Key::parse(&label),
            },
        },
        warnings,
    })
}

fn status_of(front: Front, done: service::Done<status::Report>) -> Status {
    let now = done.value.now;
    Status {
        now,
        awaiting: done
            .value
            .awaiting
            .into_iter()
            .map(|awaited| Awaiting {
                provider: awaited.provider,
                put_back: awaited.put_back,
                label: awaited.label.label,
                again: awaited.again,
            })
            .collect(),
        accounts: done
            .value
            .rows
            .into_iter()
            .map(|row| account(row, now))
            .collect(),
        warnings: warnings(front, &done.warnings),
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
            accounts: (Key::parse(&from), Key::parse(&to)),
            from,
            to,
            adoption,
            warnings,
            used: words::share_of_limit(&limit),
            resets_at: limit.resets_at,
        },
        Auto::Skipped { from, limit, why } => AutoSwitched::Skipped {
            key: why.told_apart(&from, &limit),
            used: words::share_of_limit(&limit),
            why: words::not_switching(&why, threshold),
            code: why.code(),
            until: match why {
                Skip::Settling { until } => Some(until),
                _ => None,
            },
            from,
        },
        Auto::NotWatching { why } => AutoSwitched::NotWatching {
            key: why.told_apart(),
            why: crate::present::words::not_watching(&why),
            until: match why {
                Blind::Unidentified { until, .. } => Some(until),
                _ => None,
            },
        },
    }
}

fn changed<T, R>(
    front: Front,
    outcome: Changing<T>,
    make: impl FnOnce(T, Vec<Warning>) -> R,
) -> Result<R, PitboardError> {
    let done = outcome.said(front)?;
    Ok(make(done.value, warnings(front, &done.warnings)))
}

pub(crate) struct SignInSession {
    watched: Mutex<Option<switch::WatchedSignIn>>,
    // where the core's wait for an app to quit signed in is, apart from `watched` as `said` is
    steps: switch::Steps,
    // the account the core parked for it, as `SwitchTo` takes it
    pub(crate) put_back: Option<Key>,
    /// Apart from `watched`: reading waits on the tool, and Codex says nothing between its
    /// address and the browser coming back, so a read that held `watched` kept a cancel or
    /// a paste waiting until then.
    said: switch::Said,
    naming: switch::Naming,
    /// The core the sign-in started with, which enrols what it signed in to: one made again
    /// meanwhile, from a login shell that answered late, may look elsewhere for programs,
    /// but keeps its accounts in the same place.
    made: Arc<Made>,
    front: Front,
}

impl SignInSession {
    /// The next thing the tool said, or nothing once it has stopped saying anything.
    /// Blocks until the tool says something or stops.
    pub(crate) fn next_line(&self) -> Option<String> {
        self.said.next()
    }

    // Where the wait for an app to quit signed in is; None for a program's sign-in.
    pub(crate) fn step(&self) -> Option<switch::WaitStep> {
        self.steps.now()
    }

    /// Types back the code the browser showed after signing in.
    pub(crate) fn paste(&self, line: &str) -> Result<(), PitboardError> {
        let mut held = self.watched.lock().map_err(|_| PitboardError::Failed {
            code: "sign_in_gone".into(),
            message: "This sign-in is no longer running.".into(),
            warnings: Vec::new(),
            account: None,
        })?;
        match held.as_mut() {
            Some(watched) => watched.paste(line).said(self.front),
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
                message: "This sign-in is no longer running.".into(),
                warnings: Vec::new(),
                account: None,
            })?;
        let login = watched.finish().said(self.front)?;
        changed(
            self.front,
            self.made
                .core
                .enroll_signed_in_as(login, self.naming.clone()),
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

// A late shell is asked once more, and only once: its startup files are slowest at login.
struct Kept {
    make: Box<dyn Fn() -> (Made, bool) + Send + Sync>,
    again_after: Duration,
    held: Mutex<Option<Held>>,
    #[cfg(test)]
    waiting: std::sync::OnceLock<std::sync::mpsc::Sender<()>>,
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
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn value(&self) -> Arc<Made> {
        #[cfg(test)]
        self.say_if_waiting();
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

    // Tells a listening test this call found the value being made, before it waits on the lock.
    #[cfg(test)]
    fn say_if_waiting(&self) {
        if let Some(waiting) = self.waiting.get() {
            let busy = matches!(
                self.held.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            );
            if busy {
                let _ = waiting.send(());
            }
        }
    }
}

/// The app's core, which the model's lanes call: made the first time one of them needs it,
/// as `Kept` keeps it.
pub(crate) struct AppCore {
    made: Kept,
    // whose words a refusal is said in
    front: Front,
    // Held until the app quits or uninstalls, so `pitboard uninstall` refuses meanwhile.
    open: std::sync::OnceLock<Mutex<Option<pitboard_core::app::Open>>>,
}

impl AppCore {
    /// The app's core. It reads `environment`, the one the app was started with, by the
    /// code the command line reads its own with, and the schedule runs the command line that
    /// comes with the app at `app`. What it refuses is said in `front`'s words.
    ///
    /// The person's login shell is asked for its `PATH` when something first needs the core,
    /// never here: it can take seconds, and an app makes its model on its main thread. Each
    /// tool's program is looked for on that `PATH` and then where its installers put it.
    ///
    /// Nothing is read with it where nothing may be read here, as the command line reads
    /// nothing then ([`AppCore::readable`]): every read the model's lanes make asks first, and
    /// every change asks the gate the core's changes pass, which asks the same.
    pub(crate) fn for_app(
        environment: HashMap<String, String>,
        app: Option<String>,
        front: Front,
    ) -> Arc<Self> {
        let environment: Environment = environment.into_iter().collect();
        let app = app.map(PathBuf::from);
        Arc::new(AppCore::asking(
            front,
            move || {
                let found = AppContext::discover(&environment, app.as_deref());
                let late = found.late;
                (Made::from_app(found), late)
            },
            ASK_AGAIN_AFTER,
        ))
    }

    // `make` also says whether the login shell it asked answered too late.
    pub(crate) fn asking(
        front: Front,
        make: impl Fn() -> (Made, bool) + Send + Sync + 'static,
        again_after: Duration,
    ) -> AppCore {
        pitboard_core::release::set_version(env!("CARGO_PKG_VERSION"));
        AppCore {
            front,
            made: Kept {
                make: Box::new(make),
                again_after,
                held: Mutex::new(None),
                #[cfg(test)]
                waiting: std::sync::OnceLock::new(),
            },
            open: std::sync::OnceLock::new(),
        }
    }

    fn core(&self) -> Arc<Made> {
        self.held(self.made.value())
    }

    fn held(&self, made: Arc<Made>) -> Arc<Made> {
        self.open.get_or_init(|| Mutex::new(made.core.hold_open()));
        made
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
        self.may_read(|| self.core())
    }

    /// The same, where the core may be made once more first, as `Kept::value_asking_again`
    /// says.
    fn readable_asking_again(&self) -> Result<Arc<Made>, PitboardError> {
        self.may_read(|| self.held(self.made.value_asking_again()))
    }

    /// The core `made` makes, once the release gate lets this build do anything, and then
    /// once every home it names is a full path.
    fn may_read(&self, made: impl FnOnce() -> Arc<Made>) -> Result<Arc<Made>, PitboardError> {
        pitboard_core::release::check().said(self.front)?;
        let made = made();
        made.core.check_homes().said(self.front)?;
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
        self.core().core.keep_app_file(file, body).said(self.front)
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

    // Looked for each time it is asked, so a tool installed since the app started is found.
    // None where nothing may be read here, where nothing could be enrolled either.
    pub(crate) fn installed(&self) -> Vec<ProviderId> {
        let Ok(made) = self.readable_asking_again() else {
            return Vec::new();
        };
        let here = made.core.installed();
        crate::listed(self.front.os())
            .filter(|tool| here.contains(tool))
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
        let done = self.readable()?.core.status(fresh).said(self.front)?;
        Ok(status_of(self.front, done))
    }

    pub(crate) fn switch_to(&self, account: &Key) -> Result<Switched, PitboardError> {
        switched(self.front, self.core().core.switch_to(&account.qualified()))
    }

    /// What `switch_to` does once somebody has agreed to put `account`'s login in place of a
    /// login in use its service refuses for good.
    pub(crate) fn replace_with(&self, account: &Key) -> Result<Switched, PitboardError> {
        switched(
            self.front,
            self.core().core.replace_with(&account.qualified()),
        )
    }

    /// Writes `account` into its tool's config where that names another account, while it is
    /// in use, and refuses where it no longer is. Gives what it warned of.
    pub(crate) fn update_config(&self, account: &Key) -> Result<Vec<Warning>, PitboardError> {
        changed(
            self.front,
            self.core().core.update_config(&account.qualified()),
            |_, warnings| warnings,
        )
    }

    /// What is left in the file behind Claude Code's store, and what putting it away would do
    /// with it, as `pitboard stow` asks first: `None` where nothing is. Takes no lock and writes
    /// nothing, and may ask Anthropic whose the login in it is, and whose the login stored is.
    pub(crate) fn left_login(&self) -> Result<Option<Left>, PitboardError> {
        self.readable()?.core.left_login().said(self.front)
    }

    /// Puts away the file behind Claude Code's store, while it holds what `seen` was taken of,
    /// as `pitboard stow` does. Gives what it did, and what it warned of.
    pub(crate) fn stow(&self, seen: &str) -> Result<(Stowed, Vec<Warning>), PitboardError> {
        changed(
            self.front,
            self.core().core.stow(seen),
            |stowed, warnings| (stowed, warnings),
        )
    }

    /// Switches Claude Code by itself where a limit of the account in use has reached `at`%
    /// and another account has room, as the core decides under its lock. `at` is taken as the
    /// nearest share there can be.
    pub(crate) fn auto_switch(&self, at: u8) -> Result<AutoSwitched, PitboardError> {
        let threshold = Threshold::clamped(i64::from(at));
        changed(
            self.front,
            self.core().core.auto_switch(threshold),
            |auto, warnings| auto_switched(auto, warnings, threshold),
        )
    }

    /// What switching Claude Code by itself at `at`% would come to, from the core's files
    /// alone: what stands, or that only a decision under its lock can say. Takes no lock,
    /// reads no keychain and asks nobody, so the model asks it after every read and every half
    /// a minute.
    pub(crate) fn auto_look(&self, at: u8) -> Result<AutoLooked, PitboardError> {
        let threshold = Threshold::clamped(i64::from(at));
        Ok(
            match self
                .readable()?
                .core
                .auto_look(threshold)
                .said(self.front)?
            {
                Look::Stands(auto) => {
                    AutoLooked::Stands(Box::new(auto_switched(*auto, Vec::new(), threshold)))
                }
                Look::Act => AutoLooked::Act,
            },
        )
    }

    /// Each Claude Code account, whether Pitboard warms it up and where it stands, and the
    /// span of the day, from the core's files alone, so a read asks it every time.
    pub(crate) fn warm_overview(&self) -> Result<Overview, PitboardError> {
        self.readable()?.core.warm_overview().said(self.front)
    }

    /// Warms up the first account turned on that is due, as the core decides: looking at its
    /// files first, so where none is due it takes no lock and says where each one stands.
    pub(crate) fn warm_up(&self) -> Result<WarmUp, WarmRefused> {
        self.core()
            .core
            .warm_up()
            .map(|done| done.value)
            .map_err(|failed| WarmRefused::of(self.front, &failed))
    }

    /// Warms up `account` now, whatever the span of the day.
    pub(crate) fn warm_now(&self, account: &Key) -> Result<WarmUp, WarmRefused> {
        self.core()
            .core
            .warm_now(&account.qualified())
            .map(|done| done.value)
            .map_err(|failed| WarmRefused::of(self.front, &failed))
    }

    /// Warms up `account` from now on, or stops.
    pub(crate) fn warm_turn(&self, account: &Key, on: bool) -> Result<(), PitboardError> {
        changed(
            self.front,
            self.core().core.warm_turn(&account.qualified(), on),
            |_, _| (),
        )
    }

    /// Warms up from `from` to `to`, in minutes of the day, `to` up to 24:00. A span that
    /// starts and ends at once is refused as the command line refuses it.
    pub(crate) fn warm_range(&self, from: u16, to: u16) -> Result<(), PitboardError> {
        let range = Range::new(from, to).ok_or_else(|| {
            PitboardError::refused(
                self.front,
                &pitboard_core::error::Error::Usage(
                    "the span starts and ends at the same time".into(),
                ),
            )
        })?;
        changed(self.front, self.core().core.warm_range(range), |_, _| ())
    }

    // With the app's own hold on Pitboard's directory, which goes with the directory.
    pub(crate) fn uninstall(&self) -> Result<Uninstalled, PitboardError> {
        let made = self.core();
        let mut own = self
            .open
            .get_or_init(|| Mutex::new(made.core.hold_open()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        changed(
            self.front,
            made.core.uninstall_from_app(&mut own),
            |removed, _| Uninstalled::of(&removed),
        )
    }

    /// Enroll the account `tool` has signed in now, under `naming`.
    pub(crate) fn enroll_current(
        &self,
        tool: ProviderId,
        naming: switch::Naming,
    ) -> Result<Enrolled, PitboardError> {
        changed(
            self.front,
            self.core().core.enroll_current_as(tool, naming),
            enrolled,
        )
    }

    /// Starts `tool`'s own sign-in, watched rather than inherited, again for the account
    /// `again` names or for a new one. The model hands on what the tool says, can paste a
    /// fallback code, and finishes it.
    ///
    /// May ask the login shell again first, as `installed` does: a sign-in is when a program
    /// the first, late, ask could not find is needed.
    pub(crate) fn sign_in(
        &self,
        tool: ProviderId,
        again: Option<String>,
    ) -> Result<Arc<SignInSession>, PitboardError> {
        let made = self.made.value_asking_again();
        let (watched, naming) = match again {
            Some(label) => (
                made.core
                    .sign_in_watched(&Key::new(tool, &label).qualified())
                    .said(self.front)?,
                switch::Naming::As(label),
            ),
            None => (
                made.core.sign_in_watched_for(tool).said(self.front)?,
                switch::Naming::FromEmail,
            ),
        };
        Ok(Arc::new(SignInSession {
            said: watched.said(),
            steps: watched.steps(),
            put_back: watched.put_back(),
            watched: Mutex::new(Some(watched)),
            naming,
            made,
            front: self.front,
        }))
    }

    pub(crate) fn forget(&self, account: &Key) -> Result<(), PitboardError> {
        changed(
            self.front,
            self.core().core.forget(&account.qualified()),
            |_, _| (),
        )
    }

    pub(crate) fn rename(&self, account: &Key, to: &str) -> Result<(), PitboardError> {
        changed(
            self.front,
            self.core().core.rename(&account.qualified(), to),
            |_, _| (),
        )
    }

    /// When Pitboard's account index last changed, in epoch milliseconds, or 0 when there is
    /// none, or where nothing may be read here.
    ///
    /// One stat of one file. A switch typed in a terminal used to leave the menu bar naming
    /// the account the person had just stopped using, for as long as five minutes, with a
    /// button offering a switch that had already happened. The model looks at this as the
    /// watch on Pitboard's directory hears a write ([`AppCore::written`]), and when it moves,
    /// reads `status_offline`: no network, and no keychain unless an interrupted switch is
    /// waiting.
    pub(crate) fn changed_at(&self) -> i64 {
        self.readable().map_or(0, |made| made.core.changed_at())
    }

    /// When Pitboard's usage readings last changed, in epoch milliseconds, or 0 when there
    /// are none, or where nothing may be read here.
    ///
    /// Every read records its answers, and every session's status line what that session has
    /// seen where it moves a limit an answer gave, so what is remembered is the newest any
    /// front end has. The model looks at this beside `changed_at`, and when it moves, takes the
    /// numbers from `status_offline`: no network, and no keychain unless an interrupted
    /// switch is waiting. Only the numbers: a reading moving says nothing about who is signed
    /// in, which is `changed_at`'s to say.
    pub(crate) fn readings_changed_at(&self) -> i64 {
        self.readable()
            .map_or(0, |made| made.core.readings_changed_at())
    }

    /// Pitboard's directory watched for the next write to the account index or the readings,
    /// or where nothing may be read here ([`AppCore::readable`]), the refusal.
    pub(crate) fn written(&self) -> Result<Written, PitboardError> {
        Ok(self.readable()?.core.written())
    }

    /// The same report without asking anyone: the last numbers Pitboard measured, and whose
    /// login each tool has stored, as its service last said.
    ///
    /// What the app shows on a plane, and what it shows while a live read is still in
    /// flight, rather than an empty panel and a spinner. It warns of an interrupted switch
    /// nothing can finish as `status` does, wherever that can be told without a request.
    pub(crate) fn status_offline(&self) -> Result<Status, PitboardError> {
        let done = self.readable()?.core.status_offline().said(self.front)?;
        Ok(status_of(self.front, done))
    }

    pub(crate) fn abandon_recovery(&self) -> Result<GaveUp, PitboardError> {
        let gave_up = self.core().core.abandon_recovery().said(self.front)?;
        Ok(GaveUp {
            switch: gave_up.switch.map(|a| Abandoned {
                from: a.from,
                to: a.to,
                logins_kept: u32::try_from(a.kept).unwrap_or(u32::MAX),
            }),
            moves: gave_up.moves.iter().map(in_app_given_up).collect(),
            warnings: warnings(self.front, &gave_up.warnings),
        })
    }

    // An app whose login is its files stays closed on an index nobody can read.
    pub(crate) fn whole(&self, tool: ProviderId) -> bool {
        if !in_its_app(tool) {
            return true;
        }
        self.readable_asking_again()
            .is_ok_and(|made| made.core.whole(tool))
    }

    pub(crate) fn moving(&self, tool: ProviderId) -> bool {
        if !in_its_app(tool) {
            return false;
        }
        self.readable_asking_again()
            .map_or(true, |made| made.core.moving(tool))
    }

    // Only what quitting that app puts right: a `codex` in a terminal is not waited for.
    pub(crate) fn holds_app(&self, tool: ProviderId, app: &crate::model::AppId) -> bool {
        self.holding(tool)
            .iter()
            .any(|held| matches!(&held.remedy, Remedy::ReopenApp { app: its, .. } if its == app))
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

    /// The proof the one gate every change passes hands out, where this process may change
    /// anything, for a file the app keeps of its own outside Pitboard's directory.
    pub(crate) fn permit(&self) -> Result<service::Permit, PitboardError> {
        self.core().core.permit().said(self.front)
    }

    /// Whether anything keeps parked logins alive without a command being run. `None` where
    /// nothing may be read here, which is not a schedule that is absent.
    pub(crate) fn schedule(&self) -> Option<Schedule> {
        Some(match self.readable().ok()?.core.schedule() {
            pitboard_core::schedule::Installed::Yes {
                location,
                every_seconds,
            } => Schedule::Installed {
                path: location.to_string(),
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
        made.core.permit().said(self.front)?;
        if made.helper.is_none() {
            return Err(PitboardError::refused(
                self.front,
                &pitboard_core::error::Error::ScheduleProgramUnnamed,
            ));
        }
        Ok(made.core.schedule_install().said(self.front)?.to_string())
    }

    /// Take it away. `false` when there was nothing installed.
    pub(crate) fn schedule_uninstall(&self) -> Result<bool, PitboardError> {
        self.core().core.schedule_uninstall().said(self.front)
    }

    /// Point a schedule an app up to 0.3.0 wrote at the command line this app comes with.
    /// That app scheduled itself, so launchd has been starting a second app every day and
    /// renewing nothing. The model asks once a launch: `true` when it repaired one, and
    /// nothing changes where the schedule already runs a command line or there is none.
    pub(crate) fn schedule_repair(&self) -> Result<bool, PitboardError> {
        self.core().core.schedule_repair().said(self.front)
    }

    /// What is running `tool` with a login a switch would leave it on, by kind, for the model
    /// to say so or to offer to quit an app first. Empty where nothing is, for a tool that
    /// follows a switch by itself, and where nothing may be read here, where no switch is made
    /// either. Reads the process list and nothing else, so it answers at once.
    pub(crate) fn holding(&self, tool: ProviderId) -> Vec<Holding> {
        let Ok(made) = self.readable() else {
            return Vec::new();
        };
        made.core
            .holding(tool)
            .into_iter()
            .map(Holding::from)
            .collect()
    }

    /// Every check `pitboard doctor` makes, each moment one names on the person's clock as
    /// `local` says it. `None` where this build may do nothing on its system
    /// (`pitboard_core::release`), which makes no check, as `pitboard doctor` there makes
    /// none. A home that is not a full path is doctor's own to say, as its `homes` check,
    /// failed, and nothing else.
    pub(crate) fn doctor(&self, local: &dyn crate::LocalTime) -> Option<Doctored> {
        pitboard_core::release::check().ok()?;
        let diagnosis = self
            .core()
            .core
            .doctor_with(&|at, now| crate::present::clock_of(local, at, now));
        let report = serde_json::to_string_pretty(&diagnosis.report()).unwrap_or_default();
        let checks = diagnosis
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
                account: c.account.as_deref().map(Key::parse),
            })
            .collect();
        Some(Doctored { checks, report })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pitboard_core::context::Context;

    // A core over a home that is no folder on macOS and Linux, and one nothing makes on Windows,
    // which finds no program.
    fn made(helper: Option<&Path>) -> Made {
        let mut ctx = Context::new(nowhere())
            .with_search_path(String::new())
            .with_caller("app".into());
        if let Some(helper) = helper {
            ctx = ctx.with_schedule_program(helper.to_path_buf());
        }
        Made {
            search_path: None,
            helper: helper.map(Path::to_path_buf),
            command_line_places: pitboard_core::app::command_line_places(&at_nowhere(), None),
            core: service::Pitboard::new(ctx),
        }
    }

    // A core made from a login shell that answered `path`.
    fn answering(path: &str) -> Made {
        Made {
            search_path: Some(path.to_owned()),
            ..made(None)
        }
    }

    // What the login shell said to the core in use, once something asked what is installed.
    fn shell_said(core: &AppCore) -> Option<String> {
        core.installed();
        core.core().search_path.clone()
    }

    // `/dev/null` is no folder, and Windows has none like it, so there a folder nothing makes.
    fn nowhere() -> PathBuf {
        use pitboard_core::host::{OS, Os};
        match OS {
            Os::MacOs | Os::Linux => PathBuf::from("/dev/null"),
            Os::Windows => std::env::temp_dir().join("pitboard-app-nowhere"),
        }
    }

    fn at_nowhere() -> Environment {
        let home = nowhere().to_string_lossy().into_owned();
        [("HOME", home.as_str()), ("USERPROFILE", home.as_str())]
            .into_iter()
            .collect()
    }

    fn installed(core: &AppCore) -> Vec<&'static str> {
        core.installed().into_iter().map(ProviderId::code).collect()
    }

    /// The environment of an app whose home is `home`, whose login shell cannot be run, so
    /// no test starts the person's own, and with `more` beside.
    fn environment(home: &str, more: &[(&str, &str)]) -> HashMap<String, String> {
        [
            ("HOME", home),
            ("USERPROFILE", home),
            ("SHELL", "/nowhere/at/all/sh"),
        ]
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

    // Scheduling this process would start a second app every day and renew nothing. The home
    // is no folder on macOS and Linux and one nothing makes on Windows, so a regression here
    // reaches no scheduler.
    #[test]
    fn the_app_schedules_nothing_without_a_command_line_to_run() {
        let core = AppCore::for_app(
            environment(&nowhere().to_string_lossy(), &[]),
            None,
            Front::MacMenuBar,
        );
        let Err(PitboardError::Failed { code, message, .. }) = core.schedule_install() else {
            panic!("the app scheduled itself");
        };
        assert_eq!(code, "schedule_program_unnamed");
        assert!(message.contains("command line"), "{message}");
    }

    // "the credential store could not be read: ..." stood in an error InfoBar.
    #[test]
    fn a_core_failure_without_an_app_sentence_starts_with_a_capital_and_ends_with_a_period() {
        use pitboard_core::error::Error;
        let said = |error: Error| {
            let PitboardError::Failed { message, .. } =
                PitboardError::refused(Front::MacMenuBar, &error);
            message
        };
        let refused = || Error::ScheduleRefused {
            detail: "access denied".into(),
        };
        assert_eq!(said(refused()), sentence(&refused().to_string()));
        assert_eq!(
            said(Error::SignedInAccountChanged),
            sentence(&Error::SignedInAccountChanged.to_string())
        );
        let overridden = || service::Warning::AuthOverridden {
            tool: ProviderId::Claude,
            names: vec!["ANTHROPIC_API_KEY".into()],
        };
        let PitboardError::Failed {
            message, warnings, ..
        } = PitboardError::failed(
            Front::MacMenuBar,
            &service::Failed {
                error: Error::SignedInAccountChanged,
                warnings: vec![overridden()],
                said_first: 0,
            },
        );
        assert_eq!(
            message,
            sentence(&Error::SignedInAccountChanged.to_string())
        );
        assert_eq!(warnings[0].message, sentence(&overridden().to_string()));
    }

    // Plan.md section 3: no refusal or warning names a command, or how Pitboard works inside.
    #[test]
    fn no_sentence_the_apps_show_names_a_command() {
        use pitboard_core::error::{Cause, Enrolled, Error, Unidentified};
        use pitboard_core::service::Stored;
        use pitboard_core::words::Spelling;
        let (claude, codex) = (ProviderId::Claude, ProviderId::Codex);
        let path = || PathBuf::from("/Users/dana/.pitboard/state.json");
        let label = || "work".to_owned();
        let detail = || "it went wrong".to_owned();
        let parked = |cause| Error::IdentityUnverifiable {
            tool: claude,
            login: Unidentified::Parked { label: label() },
            cause,
            detail: detail(),
            spelling: Spelling::Bare,
        };
        let errors = vec![
            Error::StateWrongMachine { path: path() },
            Error::StateVersionUnknown {
                path: path(),
                found: 9,
            },
            Error::LiveCredentialShapeUnexpected {
                tool: claude,
                detail: detail(),
            },
            Error::AccountUnknown {
                label: label(),
                enrolled: Enrolled(vec!["home".into()]),
            },
            Error::NothingParked {
                tool: codex,
                label: "codex/work".into(),
            },
            Error::ParkedLoginExpired { label: label() },
            Error::LiveAccountNotEnrolled {
                tool: claude,
                who: "dana@example.com".into(),
            },
            Error::AlreadyEnrolled {
                tool: codex,
                email: "dana@example.com".into(),
                label: "codex/work".into(),
                asked: "codex/home".into(),
                waited: false,
            },
            Error::ParkSlotExhausted,
            Error::ParkedCredentialMissing { label: label() },
            Error::ParkedCredentialCorrupt {
                label: label(),
                detail: detail(),
            },
            Error::ConfigWriteFailed {
                path: path(),
                detail: detail(),
            },
            parked(Cause::Refused),
            parked(Cause::Unreachable),
            Error::ParkedLoginRefused {
                tool: claude,
                label: label(),
            },
            Error::ParkedLoginBelongsElsewhere {
                label: label(),
                who: "home@example.com".into(),
            },
            Error::SwitchUnverified {
                tool: claude,
                from: "home".into(),
                to: label(),
                detail: detail(),
            },
            Error::SwitchCorrupted {
                tool: claude,
                from: "home".into(),
                to: label(),
                detail: detail(),
                spelling: Spelling::Bare,
            },
            Error::SignInNotKept {
                tool: codex,
                label: "codex/work".into(),
                detail: detail(),
            },
            Error::RenewalFailed {
                label: label(),
                cause: None,
                detail: detail(),
            },
            Error::LeftLoginNotEnrolled {
                path: path(),
                email: "dana@example.com".into(),
                organization: String::new(),
            },
            Error::LeftLoginUnidentified {
                path: path(),
                cause: Cause::Refused,
                detail: detail(),
            },
            Error::SignInInProgress,
            Error::SignInSettledElsewhere { label: label() },
            Error::SignInNotInstalled {
                tool: codex,
                label: "codex/work".into(),
                detail: detail(),
                parked: true,
                spelling: Spelling::Bare,
                warnings: Vec::new(),
            },
            Error::SwitchDidNotHold {
                tool: codex,
                from: "codex/home".into(),
                to: "codex/work".into(),
                spelling: Spelling::Bare,
            },
            Error::LiveLoginRefused {
                tool: claude,
                label: label(),
                detail: detail(),
            },
            Error::UninstallWouldStrand {
                label: Some("desktop/home".into()),
                set_aside: Vec::new(),
            },
            Error::UninstallWouldStrand {
                label: None,
                set_aside: vec![path()],
            },
        ];
        let found = [
            service::Warning::Recovered(switch::Recovered {
                from: "home".into(),
                to: label(),
                finished: true,
            }),
            service::Warning::MoveSettled(switch::MoveSettled {
                tool: ProviderId::Desktop,
                label: "desktop/home".into(),
                half: pitboard_core::state::Half::Leave,
                ended: switch::Ended::Finished,
            }),
            service::Warning::ParkedLoginRefused {
                tool: codex,
                label: "codex/work".into(),
            },
            service::Warning::LockCompromised { tool: claude },
            service::Warning::FallbackLogin {
                tool: claude,
                path: path(),
                held: provider::Held::Login,
            },
            service::Warning::FallbackLogin {
                tool: claude,
                path: path(),
                held: provider::Held::Unreadable,
            },
            service::Warning::LoginReplaced {
                tool: claude,
                id: "a".into(),
                label: label(),
                now: Stored::Nothing,
            },
            service::Warning::InUseUnconfirmed {
                tool: claude,
                last: Stored::Account(label()),
                named: Stored::Nothing,
                asked: false,
            },
            service::Warning::ConfigNamesAnother {
                tool: claude,
                config: Stored::Account("home".into()),
                in_use: Stored::Account(label()),
            },
            service::Warning::RenewalFailed(Error::RenewalFailed {
                label: label(),
                cause: None,
                detail: detail(),
            }),
            service::Warning::SwitchStuck(Error::RecoveryUndetermined {
                tool: claude,
                from: "home".into(),
                to: label(),
                detail: detail(),
                spelling: Spelling::Bare,
            }),
        ];
        let said: Vec<String> = errors
            .into_iter()
            .map(|error| {
                let PitboardError::Failed { message, .. } =
                    PitboardError::refused(Front::MacMenuBar, &error);
                message
            })
            .chain(
                warnings(Front::MacMenuBar, &found)
                    .into_iter()
                    .map(|warning| warning.message),
            )
            .collect();
        for message in said {
            assert!(!message.is_empty());
            assert!(!message.contains("pitboard "), "{message}");
            assert!(!message.contains('`'), "{message}");
            let found = crate::present::testing::mechanism_words(&message);
            assert!(found.is_empty(), "{found:?} in {message}");
        }
    }

    #[test]
    fn the_apps_core_names_the_apps_version_to_the_release_gate() {
        AppCore::for_app(
            environment(&nowhere().to_string_lossy(), &[]),
            None,
            Front::MacMenuBar,
        );
        assert_eq!(
            pitboard_core::release::version(),
            Some(env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn the_app_is_refused_at_launch_where_every_change_is() {
        use pitboard_core::host::{Elevation, Floor};
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let refused = |set: &dyn Fn(&pitboard_core::testing::MemoryHost), pitboard: PathBuf| {
            let host = pitboard_core::testing::MemoryHost::new();
            set(&host);
            refusal(
                Front::WindowsNotificationArea,
                Context::new(home.clone())
                    .with_pitboard_home(pitboard)
                    .with_caller("app".into())
                    .with_memory_stores(host),
            )
        };
        let full = home.join(".pitboard");
        let code = |set: &dyn Fn(&pitboard_core::testing::MemoryHost), pitboard: &PathBuf| {
            refused(set, pitboard.clone()).map(|failure| failure.code)
        };
        assert_eq!(code(&|_| {}, &full), None);
        let elevated = |host: &pitboard_core::testing::MemoryHost| {
            host.runs_with(Elevation::Elevated {
                why: "as administrator",
            });
        };
        assert_eq!(code(&elevated, &full).as_deref(), Some("elevated"));
        let unknown = |host: &pitboard_core::testing::MemoryHost| {
            host.runs_with(Elevation::Unknown);
        };
        assert_eq!(code(&unknown, &full).as_deref(), Some("elevated"));
        let below = |host: &pitboard_core::testing::MemoryHost| {
            host.runs_on(Floor::Below { build: 22631 });
        };
        assert_eq!(
            refused(&below, full.clone()).map(|failure| failure.message),
            Some(sentence(
                &pitboard_core::error::Error::SystemTooOld { build: Some(22631) }.to_string()
            ))
        );
        let floor_unknown = |host: &pitboard_core::testing::MemoryHost| {
            host.runs_on(Floor::Unknown);
        };
        assert_eq!(
            code(&floor_unknown, &full).as_deref(),
            Some("system_too_old")
        );
        assert_eq!(
            code(&|_| {}, &PathBuf::from("pitboard")).as_deref(),
            Some("home_not_absolute")
        );
        assert!(!home.exists(), "nothing is made");
    }

    /// Where Pitboard may change nothing, as root or under sudo, the app's schedule is
    /// refused for that before anything else, as every change is: not for a command line
    /// the app does not name, which would send the person to install the app again.
    #[test]
    fn the_app_schedules_nothing_where_pitboard_may_change_nothing() {
        let host = pitboard_core::testing::MemoryHost::new();
        host.runs_with(pitboard_core::host::Elevation::Elevated { why: "as root" });
        let core = AppCore::asking(
            Front::MacMenuBar,
            move || {
                let mut made = made(None);
                let ctx = Context::new(nowhere())
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
    fn the_app_schedules_nothing_where_pitboard_home_is_another_directory() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let helper = home.join("Applications/Pitboard.app/Contents/Helpers/pitboard");
        std::fs::create_dir_all(helper.parent().expect("its folder")).expect("made");
        std::fs::write(&helper, "").expect("a command line");
        let app = |pitboard: PathBuf| {
            let (home, helper) = (home.clone(), helper.clone());
            let host = pitboard_core::testing::MemoryHost::new();
            AppCore::asking(
                Front::MacMenuBar,
                move || {
                    let mut made = made(Some(&helper));
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

        let default = app(service::Pitboard::new(Context::new(home.clone())).default_directory());
        default.schedule_install().expect("installed");
        assert!(matches!(
            default.schedule(),
            Some(Schedule::Installed { .. })
        ));
        assert_eq!(elsewhere.schedule_uninstall().ok(), Some(true));
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
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let full = scratch.path().join("home");
        assert!(!full.exists(), "{} is there already", full.display());
        for (home, pitboard, variable) in [
            (
                "relative".into(),
                full.join(".pitboard"),
                pitboard_core::host::OS.home_variable(),
            ),
            (full.clone(), "pitboard".into(), "PITBOARD_HOME"),
            (full.clone(), PathBuf::new(), "PITBOARD_HOME"),
        ] {
            let host = pitboard_core::testing::MemoryHost::new();
            let given = pitboard.clone();
            let core = AppCore::asking(
                Front::MacMenuBar,
                move || {
                    let mut made = made(None);
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
            assert!(core.holding(ProviderId::Codex).is_empty());
            assert!(
                !core.whole(ProviderId::Desktop) && core.moving(ProviderId::Desktop),
                "{variable}"
            );
            assert!(core.whole(ProviderId::Codex) && !core.moving(ProviderId::Codex));
            assert!(installed(&core).is_empty(), "{variable}");
            assert_eq!(core.command_line(), FoundCommandLine::Nowhere);
            let Doctored { checks, report } = core
                .doctor(&crate::present::testing::Utc)
                .expect("doctor's own to say");
            let report: serde_json::Value = serde_json::from_str(&report).expect("JSON");
            assert_eq!(
                report["checks"].as_array().map(Vec::len),
                Some(checks.len()),
                "what `pitboard doctor --json` gives"
            );
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
    fn the_app_repairs_nothing_where_no_schedule_runs_it() {
        let launched = AppCore::for_app(
            environment(&nowhere().to_string_lossy(), &[]),
            None,
            Front::MacMenuBar,
        );
        assert!(!launched.schedule_repair().expect("nothing to repair"));
        let there = std::env::current_exe().expect("this test's own program");
        let named = AppCore::asking(
            Front::MacMenuBar,
            move || (made(Some(&there)), false),
            ASK_AGAIN_AFTER,
        );
        assert!(!named.schedule_repair().expect("nothing to repair"));
    }

    #[test]
    fn the_core_is_made_once_when_it_is_first_needed() {
        let asks = Asks::default();
        let (making, made) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel::<()>();
        let released = Mutex::new(released);
        let core = Arc::new(AppCore::asking(
            Front::MacMenuBar,
            {
                let asks = asks.clone();
                move || {
                    asks.note();
                    let _ = making.send(());
                    let _ = released.lock().expect("a test's own lock").recv();
                    (answering("/answered"), false)
                }
            },
            ASK_AGAIN_AFTER,
        ));
        let listed = match pitboard_core::host::OS {
            pitboard_core::host::Os::MacOs => 3,
            pitboard_core::host::Os::Linux | pitboard_core::host::Os::Windows => 2,
        };
        assert_eq!(
            crate::tools_on(pitboard_core::host::OS).len(),
            listed,
            "listing the tools asks nothing"
        );
        assert_eq!(asks.count(), 0, "and nor does making the core");
        assert!(core.whole(ProviderId::Codex) && !core.moving(ProviderId::Codex));
        assert_eq!(
            asks.count(),
            0,
            "nor opening an app whose login is a document"
        );
        let (waiting, waits) = std::sync::mpsc::channel();
        core.made.waiting.set(waiting).expect("listened to once");
        let call = |which| {
            let core = core.clone();
            std::thread::spawn(move || match which {
                0 => drop(core.installed()),
                1 => drop(core.changed_at()),
                _ => drop(core.command_line()),
            })
        };
        let first = call(0);
        made.recv_timeout(Duration::from_secs(20))
            .expect("the core being made");
        let others = [call(1), call(2)];
        for _ in &others {
            waits
                .recv_timeout(Duration::from_secs(20))
                .expect("a call waiting for it");
        }
        drop(release);
        for call in std::iter::once(first).chain(others) {
            call.join().expect("a call that answers");
        }
        assert_eq!(shell_said(&core).as_deref(), Some("/answered"));
        assert_eq!(asks.count(), 1);
    }

    /// A login shell too slow to answer, which startup files are while the machine is busy
    /// logging in, is asked once more when something next asks what is installed, a while
    /// later, and what it answers then is what is used. Once more and no more: a shell that
    /// is always that slow would otherwise cost its patience on every ask.
    #[test]
    fn a_login_shell_too_slow_to_answer_is_asked_once_more_later() {
        let asks = Asks::default();
        let mut core = AppCore::asking(
            Front::MacMenuBar,
            {
                let asks = asks.clone();
                move || match asks.note() {
                    1 => (made(None), true),
                    _ => (answering("/answered"), false),
                }
            },
            Duration::from_secs(3600),
        );
        assert_eq!(shell_said(&core), None, "what the first, late, ask found");
        assert_eq!(
            shell_said(&core),
            None,
            "and nothing more until the while is up"
        );
        assert_eq!(asks.count(), 1);
        // The while is up.
        core.made.again_after = Duration::ZERO;
        assert_eq!(shell_said(&core).as_deref(), Some("/answered"));
        assert_eq!(asks.count(), 2);
        assert_eq!(shell_said(&core).as_deref(), Some("/answered"));
        assert_eq!(asks.count(), 2, "asked once more, and no more");
    }

    /// Not before the while is up, and never when the shell answered or could not be asked.
    /// A second ask that is late as well leaves what the first made in its place.
    #[test]
    fn a_login_shell_is_not_asked_again_sooner_or_for_nothing() {
        let soon = Asks::default();
        let early = AppCore::asking(
            Front::MacMenuBar,
            {
                let soon = soon.clone();
                move || {
                    soon.note();
                    (made(None), true)
                }
            },
            Duration::from_secs(3600),
        );
        installed(&early);
        installed(&early);
        assert_eq!(soon.count(), 1);

        let answered = Asks::default();
        let core = AppCore::asking(
            Front::MacMenuBar,
            {
                let answered = answered.clone();
                move || {
                    answered.note();
                    (made(None), false)
                }
            },
            Duration::ZERO,
        );
        installed(&core);
        installed(&core);
        assert_eq!(answered.count(), 1);

        let late = Asks::default();
        let again = AppCore::asking(
            Front::MacMenuBar,
            {
                let late = late.clone();
                move || match late.note() {
                    1 => (made(None), true),
                    _ => (answering("/late"), true),
                }
            },
            Duration::ZERO,
        );
        assert_eq!(shell_said(&again), None);
        assert_eq!(shell_said(&again), None, "the second ask was late too");
        assert_eq!(shell_said(&again), None);
        assert_eq!(late.count(), 2);
    }

    /// The app's core reads the environment it was started with as the command line reads its
    /// own, and asks the login shell that environment names. A shell that cannot be run is no
    /// answer, so nothing is said to be on its `PATH`. A custom OAuth endpoint refuses a change to
    /// a Claude Code account, and an app outside a bundle has no command line to schedule.
    #[test]
    fn the_apps_core_is_made_from_the_environment_it_was_started_with() {
        // A home of its own, so Claude Code's slot is hashed from it and nothing real is read.
        let scratch = tempfile::tempdir().expect("a scratch home");
        let home = scratch.path();
        let at = |name: &str| home.join(name).to_string_lossy().into_owned();
        let (pitboard, claude, codex) = (at("pitboard"), at("claude"), at("codex"));
        let core = AppCore::for_app(
            environment(
                &at(""),
                &[
                    ("PITBOARD_HOME", &pitboard),
                    ("CLAUDE_CONFIG_DIR", &claude),
                    ("CODEX_HOME", &codex),
                    ("CLAUDE_CODE_CUSTOM_OAUTH_URL", "https://oauth.example"),
                ],
            ),
            None,
            Front::MacMenuBar,
        );
        assert_eq!(core.core().search_path, None, "the shell could not be run");
        let Err(PitboardError::Failed { code, .. }) =
            core.enroll_current(ProviderId::Claude, switch::Naming::FromEmail)
        else {
            panic!("a Claude Code account was enrolled under a custom OAuth endpoint");
        };
        assert_eq!(code, "custom_oauth_endpoint");
        let Err(PitboardError::Failed { code, .. }) = core.schedule_install() else {
            panic!("an app outside a bundle scheduled itself");
        };
        assert_eq!(code, "schedule_program_unnamed");
    }
}
