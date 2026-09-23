//! pitboard's core for its native apps, as UniFFI bindings.
//!
//! Every call is synchronous and may block on the keychain, a lock or the network, so an app
//! calls it off its main thread. Timestamps are epoch seconds.

use pitboard_core::context::Context;
use pitboard_core::service::{self, Changing};
use pitboard_core::{doctor, status, switch, usage};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

uniffi::setup_scaffolding!();

/// Where Claude Code and pitboard keep things. An app started from Finder sees none of the
/// shell's environment, so it passes these itself; `None` means Claude Code's default.
#[derive(uniffi::Record)]
pub struct Settings {
    pub home: String,
    pub pitboard_home: Option<String>,
    /// `CLAUDE_CONFIG_DIR`; empty means unset.
    pub claude_config_dir: Option<String>,
    /// `CLAUDE_SECURESTORAGE_CONFIG_DIR`; empty is set, and pins the default slot.
    pub secure_storage_dir: Option<String>,
    /// The login name Claude Code files its keychain items under.
    pub user: Option<String>,
    /// The `claude` that runs a sign-in, since `PATH` may not find it.
    pub claude_program: Option<String>,
}

impl Settings {
    fn context(self) -> Context {
        let mut ctx = Context::new(PathBuf::from(self.home));
        if let Some(dir) = self.pitboard_home {
            ctx = ctx.with_pitboard_home(PathBuf::from(dir));
        }
        if let Some(dir) = self.claude_config_dir {
            ctx = ctx.with_claude_config_dir(dir);
        }
        if let Some(dir) = self.secure_storage_dir {
            ctx = ctx.with_secure_storage_dir(dir);
        }
        if let Some(user) = self.user {
            ctx = ctx.with_user(user);
        }
        if let Some(program) = self.claude_program {
            ctx = ctx.with_claude_program(PathBuf::from(program));
        }
        // These bindings exist for the app, so a change made through them says so.
        ctx.with_caller("app".into())
    }
}

/// Something to know about that did not stop the operation.
#[derive(Debug, uniffi::Record)]
pub struct Warning {
    pub code: String,
    pub message: String,
}

fn warnings(found: &[service::Warning]) -> Vec<Warning> {
    found
        .iter()
        .map(|w| Warning {
            code: w.code().to_string(),
            message: w.to_string(),
        })
        .collect()
}

/// One change pitboard made, as `pitboard log` shows them.
#[derive(Debug, uniffi::Record)]
pub struct Change {
    /// Local time, as the log records it.
    pub at: String,
    /// Which front end asked: `app`, `cli`, or `unknown` for a line written before this
    /// was recorded.
    pub caller: String,
    pub verb: String,
    pub subject: String,
    /// `ok`, or the code of whatever stopped it.
    pub outcome: String,
}

/// An interrupted switch that was given up on, keeping every login it named.
#[derive(Debug, uniffi::Record)]
pub struct Abandoned {
    pub from: String,
    pub to: String,
    /// Copies kept rather than deleted, because which one is live is now unknown.
    pub logins_kept: u32,
}

/// What renewing every due parked login came to.
#[derive(Debug, uniffi::Record)]
pub struct Renewed {
    pub label: String,
    /// `renewed`, `renewal_deferred`, `parked_login_refused`, or the code of a failure.
    pub outcome: String,
}

/// Whether anything keeps parked logins alive on this machine without a command being run.
#[derive(Debug, uniffi::Enum)]
pub enum Schedule {
    /// The platform's own scheduler runs `pitboard renew` every `every_seconds`.
    Installed { path: String, every_seconds: u32 },
    /// Nothing does. Parked logins are renewed when pitboard runs, and otherwise not.
    Absent,
    /// This platform has no scheduler pitboard knows how to write.
    Unsupported,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum PitboardError {
    /// `code` is stable, for the app to branch on; `message` names the cause and what to do.
    /// `cause` is what went wrong underneath, where Anthropic was asked, and is what decides
    /// whether another try is worth offering. `warnings` are what was found on the way,
    /// reported even though the operation failed.
    #[error("{message}")]
    Failed {
        code: String,
        cause: Option<Cause>,
        message: String,
        warnings: Vec<Warning>,
    },
}

/// Why a request to Anthropic did not produce an answer pitboard could use.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Cause {
    /// Stable, for the app to branch on.
    pub code: String,
    /// Whether the same request, later, could answer differently.
    pub worth_retrying: bool,
}

impl From<pitboard_core::error::Error> for PitboardError {
    fn from(error: pitboard_core::error::Error) -> Self {
        PitboardError::Failed {
            code: error.code().to_string(),
            cause: cause(&error),
            message: error.to_string(),
            warnings: Vec::new(),
        }
    }
}

impl From<service::Failed> for PitboardError {
    fn from(failed: service::Failed) -> Self {
        PitboardError::Failed {
            code: failed.error.code().to_string(),
            cause: cause(&failed.error),
            message: failed.error.to_string(),
            warnings: warnings(&failed.warnings),
        }
    }
}

/// What went wrong underneath, where Anthropic was asked. The app decides whether to offer
/// another try from this rather than from the wording of a message.
fn cause(error: &pitboard_core::error::Error) -> Option<Cause> {
    error.cause().map(|c| Cause {
        code: c.code().to_string(),
        worth_retrying: c.worth_retrying(),
    })
}

#[derive(uniffi::Enum)]
pub enum Source {
    Live,
    ClaudeCodeCache,
    Remembered,
}

#[derive(uniffi::Record)]
pub struct Window {
    /// Anthropic's kind, such as `session`, `weekly_all` or `weekly_scoped`.
    pub kind: String,
    /// The model a scoped limit applies to.
    pub scope: Option<String>,
    /// Share already used; past 100 once exceeded.
    pub percent: f64,
    pub resets_at: Option<i64>,
    /// How Anthropic grades this row, when it grades it.
    pub severity: Option<String>,
    /// Whether this limit is one the account is working against now.
    pub is_active: bool,
}

#[derive(uniffi::Record)]
pub struct Usage {
    pub source: Source,
    pub observed_at: Option<i64>,
    pub windows: Vec<Window>,
}

#[derive(uniffi::Record)]
pub struct Parked {
    pub parked_at: i64,
    pub access_expires_at: Option<i64>,
    pub refresh_expires_at: Option<i64>,
}

#[derive(uniffi::Record)]
pub struct Account {
    /// `None` for an account signed in but not enrolled.
    pub label: Option<String>,
    pub email: String,
    pub account_uuid: String,
    pub signed_in: bool,
    /// Whether switching to it would work now.
    pub switchable: bool,
    pub parked: Option<Parked>,
    pub usage: Option<Usage>,
    /// Why the usage is not live, when it is not.
    pub stale: Option<String>,
    /// What to tell a person about `stale`, when it is worth a word.
    pub stale_explanation: Option<String>,
    /// How long this account lasts, in seconds: until its tightest limit fills at the rate
    /// it has been filling, or until that limit resets, whichever comes first.
    ///
    /// `None` until there is enough to go on. A wrong runway tells somebody to switch when
    /// they need not, which is worse than none.
    pub lasts_seconds: Option<i64>,
    /// Whether `lasts_seconds` is a limit filling or a limit resetting, which is the
    /// difference between "about an hour left" and "whole again in an hour".
    pub lasts_burning: bool,
}

#[derive(uniffi::Record)]
pub struct Status {
    pub now: i64,
    /// The signed-in account first.
    pub accounts: Vec<Account>,
    pub warnings: Vec<Warning>,
}

#[derive(uniffi::Enum)]
pub enum Switch {
    /// Running Claude Code sessions follow within `adoption_ceiling_seconds`.
    Switched {
        from: String,
        to: String,
        adoption_ceiling_seconds: u32,
    },
    AlreadyActive {
        label: String,
    },
}

#[derive(uniffi::Record)]
pub struct Switched {
    pub outcome: Switch,
    pub warnings: Vec<Warning>,
}

#[derive(uniffi::Enum)]
pub enum EnrolledAs {
    /// The account signed in now.
    Current,
    /// Another account, signed in privately and parked.
    SignedIn,
    /// An enrolled account's parked login, renewed.
    Renewed,
}

#[derive(uniffi::Record)]
pub struct Enrolled {
    pub email: String,
    pub enrolled: EnrolledAs,
    pub warnings: Vec<Warning>,
}

/// The email of the account a change was made to.
#[derive(uniffi::Record)]
pub struct Changed {
    pub email: String,
    pub warnings: Vec<Warning>,
}

#[derive(uniffi::Enum)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

#[derive(uniffi::Record)]
pub struct Check {
    pub code: String,
    pub name: String,
    pub level: Level,
    pub detail: String,
    /// Empty when there is nothing to do.
    pub advice: String,
}

#[derive(uniffi::Record)]
pub struct Diagnosis {
    pub checks: Vec<Check>,
    /// No check failed.
    pub healthy: bool,
}

fn account(row: status::Row, now: i64) -> Account {
    Account {
        switchable: row.switchable(now),
        lasts_seconds: row.runway.seconds(),
        lasts_burning: matches!(row.runway, pitboard_core::history::Runway::Burning(_)),
        stale: row.stale.map(|s| s.code().to_string()),
        stale_explanation: row.stale.and_then(|s| s.explanation()).map(str::to_owned),
        parked: row.parked.map(|p| Parked {
            parked_at: p.parked_at,
            access_expires_at: p.access_expires_at,
            refresh_expires_at: p.refresh_expires_at,
        }),
        usage: row.usage.map(|u| Usage {
            source: match u.source {
                usage::Source::Live => Source::Live,
                usage::Source::ClaudeCodeCache => Source::ClaudeCodeCache,
                usage::Source::Remembered => Source::Remembered,
            },
            observed_at: u.observed_at,
            windows: u
                .windows
                .into_iter()
                .map(|w| Window {
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
        account_uuid: row.account_uuid,
        signed_in: row.signed_in,
    }
}

fn changed<T, R>(
    outcome: Changing<T>,
    make: impl FnOnce(T, Vec<Warning>) -> R,
) -> Result<R, PitboardError> {
    let done = outcome?;
    Ok(make(done.value, warnings(&done.warnings)))
}

/// A sign-in in progress: Claude Code's own, running with its output piped here because an
/// app has no terminal to hand it. Measured in 2.1.278: it opens the browser itself and
/// finishes through a loopback callback, reading stdin only for the fallback code.
#[derive(uniffi::Object)]
pub struct SignIn {
    watched: Mutex<Option<switch::WatchedSignIn>>,
    label: String,
    core: Arc<Pitboard>,
}

#[uniffi::export]
impl SignIn {
    /// The next thing Claude Code said, or nothing once it has stopped saying anything.
    /// Blocks, so call it off the main thread.
    pub fn next_line(&self) -> Option<String> {
        let held = self.watched.lock().ok()?;
        held.as_ref()?.next_line()
    }

    /// Types the code back, for when the browser could not reach the callback.
    pub fn paste(&self, line: String) -> Result<(), PitboardError> {
        let mut held = self.watched.lock().map_err(|_| PitboardError::Failed {
            code: "sign_in_gone".into(),
            cause: None,
            message: "this sign-in is no longer running".into(),
            warnings: Vec::new(),
        })?;
        match held.as_mut() {
            Some(watched) => watched.paste(&line).map_err(PitboardError::from),
            None => Ok(()),
        }
    }

    /// Waits for it to finish, then enrols what it signed in to.
    pub fn finish(&self) -> Result<Enrolled, PitboardError> {
        let watched = self
            .watched
            .lock()
            .ok()
            .and_then(|mut held| held.take())
            .ok_or_else(|| PitboardError::Failed {
                code: "sign_in_gone".into(),
                cause: None,
                message: "this sign-in is no longer running".into(),
                warnings: Vec::new(),
            })?;
        let login = watched.finish()?;
        changed(
            self.core.core.enroll_signed_in(&self.label, login),
            |enrolled, warnings| {
                let (email, enrolled) = match enrolled {
                    switch::Enrolled::Current { email } => (email, EnrolledAs::Current),
                    switch::Enrolled::SignedIn { email } => (email, EnrolledAs::SignedIn),
                    switch::Enrolled::Renewed { email } => (email, EnrolledAs::Renewed),
                };
                Enrolled {
                    email,
                    enrolled,
                    warnings,
                }
            },
        )
    }

    /// Stops it. Whatever it wrote is discarded.
    pub fn cancel(&self) {
        if let Ok(mut held) = self.watched.lock()
            && let Some(watched) = held.take()
        {
            watched.cancel();
        }
    }
}

#[derive(uniffi::Object)]
pub struct Pitboard {
    core: service::Pitboard,
}

#[uniffi::export]
impl Pitboard {
    #[uniffi::constructor]
    pub fn new(settings: Settings) -> Arc<Self> {
        Arc::new(Pitboard {
            core: service::Pitboard::new(settings.context()),
        })
    }

    /// Every account and what it has left, asked of Anthropic. Parked logins whose access
    /// has lapsed are renewed first.
    ///
    /// `fresh` asks about every account whatever was asked moments ago. Pass false for a
    /// poll and true when somebody asked for it: an account is otherwise only asked about
    /// again once its tightest limit could have moved by a percentage point, which is what
    /// keeps the app and the command line to one request between them.
    pub fn status(&self, fresh: bool) -> Result<Status, PitboardError> {
        let done = self.core.status(fresh)?;
        let now = done.value.now;
        Ok(Status {
            now,
            accounts: done
                .value
                .rows
                .into_iter()
                .map(|row| account(row, now))
                .collect(),
            warnings: warnings(&done.warnings),
        })
    }

    pub fn switch_to(&self, label: String) -> Result<Switched, PitboardError> {
        changed(self.core.switch_to(&label), |outcome, warnings| Switched {
            outcome: match outcome {
                switch::Outcome::Switched {
                    from, to, adoption, ..
                } => Switch::Switched {
                    from,
                    to,
                    // Zero where nothing follows on its own, which the app renders as an
                    // instruction rather than as a countdown that would never end.
                    adoption_ceiling_seconds: match adoption {
                        pitboard_core::provider::Adoption::PollingWithin(seconds) => seconds,
                        pitboard_core::provider::Adoption::RestartRequired { .. } => 0,
                    },
                },
                switch::Outcome::AlreadyActive { label } => Switch::AlreadyActive { label },
            },
            warnings,
        })
    }

    /// Enroll the account signed in now under `label`.
    pub fn enroll_current(&self, label: String) -> Result<Enrolled, PitboardError> {
        changed(self.core.enroll_current(&label), |enrolled, warnings| {
            let (email, enrolled) = match enrolled {
                switch::Enrolled::Current { email } => (email, EnrolledAs::Current),
                switch::Enrolled::SignedIn { email } => (email, EnrolledAs::SignedIn),
                switch::Enrolled::Renewed { email } => (email, EnrolledAs::Renewed),
            };
            Enrolled {
                email,
                enrolled,
                warnings,
            }
        })
    }

    /// Starts the tool's own sign-in for a new account, watched rather than inherited. The
    /// label may name the tool, as in `codex/work`; a bare one means Claude Code. The caller
    /// shows what the tool says, can paste a fallback code, and finishes it.
    pub fn sign_in(self: Arc<Self>, label: String) -> Result<Arc<SignIn>, PitboardError> {
        let watched = self.core.sign_in_watched(&label)?;
        Ok(Arc::new(SignIn {
            watched: Mutex::new(Some(watched)),
            label,
            core: self,
        }))
    }

    pub fn forget(&self, label: String) -> Result<Changed, PitboardError> {
        changed(self.core.forget(&label), |email, warnings| Changed {
            email,
            warnings,
        })
    }

    pub fn rename(&self, from: String, to: String) -> Result<Changed, PitboardError> {
        changed(self.core.rename(&from, &to), |email, warnings| Changed {
            email,
            warnings,
        })
    }

    /// When pitboard's account index last changed, in epoch seconds, or 0 when there is
    /// none.
    ///
    /// One stat of one file, so an app can ask often. A switch typed in a terminal used to
    /// leave the menu bar naming the account the person had just stopped using, for as
    /// long as five minutes, with a button offering a switch that had already happened.
    /// Poll this, and when it moves, read `status_offline`: no network and no keychain.
    pub fn changed_at(&self) -> i64 {
        self.core.changed_at()
    }

    /// The same report without asking anyone: the last numbers pitboard measured, and who
    /// Claude Code's config says is signed in.
    ///
    /// What the app shows on a plane, and what it shows while a live read is still in
    /// flight, rather than an empty panel and a spinner.
    pub fn status_offline(&self) -> Result<Status, PitboardError> {
        let done = self.core.status_offline()?;
        let now = done.value.now;
        Ok(Status {
            now,
            accounts: done
                .value
                .rows
                .into_iter()
                .map(|row| account(row, now))
                .collect(),
            warnings: warnings(&done.warnings),
        })
    }

    /// Give up on an interrupted switch that cannot be finished, keeping every login it
    /// names. The way out when recovery cannot reach Anthropic, which until now sent the
    /// person to a terminal.
    ///
    /// `None` when there was no interrupted switch.
    pub fn abandon_recovery(&self) -> Result<Option<Abandoned>, PitboardError> {
        Ok(self.core.abandon_recovery()?.map(|a| Abandoned {
            from: a.from,
            to: a.to,
            logins_kept: u32::try_from(a.kept).unwrap_or(u32::MAX),
        }))
    }

    /// What pitboard has changed, newest last.
    pub fn log(&self, limit: u32) -> Vec<Change> {
        self.core
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

    /// Renew every parked login that is due, and nothing else.
    pub fn renew(&self) -> Vec<Renewed> {
        self.core
            .renew()
            .into_iter()
            .map(|(key, outcome)| Renewed {
                label: key.typed(),
                outcome: outcome.code().to_string(),
            })
            .collect()
    }

    /// Whether anything keeps parked logins alive without a command being run.
    pub fn schedule(&self) -> Schedule {
        match self.core.schedule() {
            pitboard_core::schedule::Installed::Yes {
                path,
                every_seconds,
            } => Schedule::Installed {
                path: path.to_string_lossy().into_owned(),
                every_seconds,
            },
            pitboard_core::schedule::Installed::No => Schedule::Absent,
            pitboard_core::schedule::Installed::Unsupported => Schedule::Unsupported,
        }
    }

    /// Ask this computer's own scheduler to renew parked logins daily. Opt-in, and the
    /// caller is expected to say what it does before offering it.
    pub fn schedule_install(&self) -> Result<String, PitboardError> {
        Ok(self.core.schedule_install()?.to_string_lossy().into_owned())
    }

    /// Take it away. `false` when there was nothing installed.
    pub fn schedule_uninstall(&self) -> Result<bool, PitboardError> {
        Ok(self.core.schedule_uninstall()?)
    }

    pub fn doctor(&self) -> Diagnosis {
        let diagnosis = self.core.doctor();
        Diagnosis {
            healthy: doctor::healthy(&diagnosis.checks),
            checks: diagnosis
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
                .collect(),
        }
    }
}
