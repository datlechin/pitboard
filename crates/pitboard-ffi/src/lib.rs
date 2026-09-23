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

/// Where each tool and pitboard keep things. An app started from Finder sees none of the
/// shell's environment, so it passes these itself; `None` means the tool's default.
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
    /// `CODEX_HOME`; empty means unset.
    pub codex_home: Option<String>,
    /// The `codex` that runs a sign-in, since `PATH` may not find it.
    pub codex_program: Option<String>,
    /// Where a tool's program is looked for, in `PATH`'s form, and what its sign-in is given
    /// as `PATH`, behind the program's own directory where that is not on it: the person's
    /// login shell's, which an app does not inherit. `None` is this process's own `PATH`.
    #[uniffi(default)]
    pub search_path: Option<String>,
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
        if let Some(dir) = self.codex_home {
            ctx = ctx.with_codex_home(dir);
        }
        if let Some(program) = self.codex_program {
            ctx = ctx.with_codex_program(PathBuf::from(program));
        }
        if let Some(path) = self.search_path {
            ctx = ctx.with_search_path(path);
        }
        // These bindings exist for the app, so a change made through them says so.
        ctx.with_caller("app".into())
    }
}

/// A tool pitboard handles, as the app names it to a person.
#[derive(Debug, uniffi::Record)]
pub struct Tool {
    /// What a label's prefix and every `provider` field say: `claude`, `codex`.
    pub code: String,
    /// As its own documentation names it: `Claude Code`, `Codex`.
    pub name: String,
    /// The command that runs it, which is what a person restarts.
    pub program: String,
    /// The company behind it, which is who is asked about its accounts.
    pub service: String,
}

/// Every tool pitboard handles, in the order a listing shows them.
#[uniffi::export]
pub fn tools() -> Vec<Tool> {
    pitboard_core::provider::ProviderId::ALL
        .iter()
        .map(|tool| Tool {
            code: tool.code().into(),
            name: tool.name().into(),
            program: tool.program().into(),
            service: tool.service().into(),
        })
        .collect()
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
    /// As a person types it: bare for Claude Code, `codex/work` for Codex.
    pub label: String,
    /// Which tool's login it is.
    pub provider: String,
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
    /// The service's own name for it: Anthropic's `session`, `weekly_all` or
    /// `weekly_scoped`, or one named after its length for OpenAI.
    pub kind: String,
    /// How long the window runs, where that is known. The way to name a window to a person
    /// whatever its service called it.
    pub length_seconds: Option<i64>,
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
    /// Unique among the accounts of one status, and stable between two: the tool and the
    /// account, or the tool alone for a login that belongs to no account pitboard can name.
    /// Two tools' accounts can share a label, so a label cannot be an identity.
    pub id: String,
    /// Which tool the account is for, as a `Tool`'s `code`.
    pub provider: String,
    /// `None` for an account signed in but not enrolled.
    pub label: Option<String>,
    /// The label with its tool, `claude/work` or `codex/work`: what to pass back to switch
    /// to, forget or rename it, which names exactly one account whatever else is enrolled.
    /// `None` exactly when `label` is.
    pub qualified: Option<String>,
    /// A login of this tool is there and belongs to no account pitboard can name: one it
    /// could not read, or one it cannot switch, such as an API key. Not an account to enrol.
    pub unplaced: bool,
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

/// When a session of the tool that is already running picks a switch up.
#[derive(uniffi::Enum)]
pub enum Adoption {
    /// On its own, within this many seconds.
    Follows { within_seconds: u32 },
    /// Never: `program` has to be quit and started again.
    Restart { program: String },
}

#[derive(uniffi::Enum)]
pub enum Switch {
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
    /// The account signed in now, signed in to: its new login is the one in use now.
    /// `again` when it was enrolled already, and not when this sign-in enrolled it.
    InUse { again: bool },
}

#[derive(uniffi::Record)]
pub struct Enrolled {
    pub email: String,
    pub enrolled: EnrolledAs,
    pub warnings: Vec<Warning>,
}

fn enrolled(enrolled: switch::Enrolled, warnings: Vec<Warning>) -> Enrolled {
    let (email, enrolled) = match enrolled {
        switch::Enrolled::Current { email } => (email, EnrolledAs::Current),
        switch::Enrolled::SignedIn { email } => (email, EnrolledAs::SignedIn),
        switch::Enrolled::Renewed { email } => (email, EnrolledAs::Renewed),
        switch::Enrolled::InUse { email, again } => (email, EnrolledAs::InUse { again }),
    };
    Enrolled {
        email,
        enrolled,
        warnings,
    }
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
    let key = row.key();
    let unplaced = row.unplaced();
    Account {
        id: if row.account_uuid.is_empty() {
            format!("{}:login", row.provider.code())
        } else {
            format!("{}:{}", row.provider.code(), row.account_uuid)
        },
        provider: row.provider.code().into(),
        qualified: key.map(|k| k.qualified()),
        unplaced,
        switchable: row.switchable(now),
        lasts_seconds: row.runway.seconds(),
        lasts_burning: matches!(row.runway, pitboard_core::history::Runway::Burning(_)),
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
                usage::Source::ClaudeCodeCache => Source::ClaudeCodeCache,
                usage::Source::Remembered => Source::Remembered,
            },
            observed_at: u.observed_at,
            windows: u
                .windows
                .into_iter()
                .map(|w| Window {
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

/// A sign-in in progress: the tool's own, running with its output piped here because an
/// app has no terminal to hand it. Both tools open the browser themselves and finish
/// through a loopback callback. Claude Code reads stdin only for a fallback code to paste;
/// Codex prints the address to open when its browser cannot, and reads nothing.
#[derive(uniffi::Object)]
pub struct SignIn {
    watched: Mutex<Option<switch::WatchedSignIn>>,
    /// Apart from `watched`: reading waits on the tool, and Codex says nothing between its
    /// address and the browser coming back, so a read that held `watched` kept a cancel or
    /// a paste waiting until then, and the app's main thread with it.
    said: switch::Said,
    label: String,
    provider: pitboard_core::provider::ProviderId,
    core: Arc<Pitboard>,
}

#[uniffi::export]
impl SignIn {
    /// Which tool's sign-in this is, as a `Tool`'s `code`.
    pub fn provider(&self) -> String {
        self.provider.code().into()
    }

    /// Whether this tool's sign-in can take a code typed back, for when the browser cannot
    /// reach its callback. Claude Code's can; Codex's prints an address instead.
    pub fn takes_a_code(&self) -> bool {
        self.provider == pitboard_core::provider::ProviderId::Claude
    }

    /// The next thing the tool said, or nothing once it has stopped saying anything.
    /// Blocks, so call it off the main thread.
    pub fn next_line(&self) -> Option<String> {
        self.said.next()
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
            enrolled,
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

    /// Every account of every tool and what it has left, each asked of its own service.
    /// Parked logins whose access has lapsed are renewed first.
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
                    provider,
                    from,
                    to,
                    adoption,
                    ..
                } => Switch::Switched {
                    provider: provider.code().into(),
                    from,
                    to,
                    adoption: match adoption {
                        pitboard_core::provider::Adoption::PollingWithin(seconds) => {
                            Adoption::Follows {
                                within_seconds: seconds,
                            }
                        }
                        pitboard_core::provider::Adoption::RestartRequired { program } => {
                            Adoption::Restart {
                                program: program.into(),
                            }
                        }
                    },
                },
                switch::Outcome::AlreadyActive { label } => Switch::AlreadyActive { label },
            },
            warnings,
        })
    }

    /// Enroll the account signed in now under `label`.
    pub fn enroll_current(&self, label: String) -> Result<Enrolled, PitboardError> {
        changed(self.core.enroll_current(&label), enrolled)
    }

    /// Starts the tool's own sign-in for a new account, watched rather than inherited. The
    /// label may name the tool, as in `codex/work`; a bare one means Claude Code. The caller
    /// shows what the tool says, can paste a fallback code, and finishes it.
    pub fn sign_in(self: Arc<Self>, label: String) -> Result<Arc<SignIn>, PitboardError> {
        let watched = self.core.sign_in_watched(&label)?;
        let provider = watched.provider();
        Ok(Arc::new(SignIn {
            said: watched.said(),
            watched: Mutex::new(Some(watched)),
            label,
            provider,
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
                provider: key.provider.code().into(),
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
