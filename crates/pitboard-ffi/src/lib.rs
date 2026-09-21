//! pitboard's core for its native apps, as UniFFI bindings.
//!
//! Every call is synchronous and may block on the keychain, a lock or the network, so an app
//! calls it off its main thread. Timestamps are epoch seconds.

use pitboard_core::context::Context;
use pitboard_core::service::{self, Changing};
use pitboard_core::{doctor, status, switch, usage};
use std::path::PathBuf;
use std::sync::Arc;

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

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum PitboardError {
    /// `code` is stable, for the app to branch on; `message` names the cause and what to do.
    /// `warnings` are what was found on the way, reported even though the operation failed.
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
    pub fn status(&self) -> Result<Status, PitboardError> {
        let done = self.core.status()?;
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
                switch::Outcome::Switched { from, to, .. } => Switch::Switched {
                    from,
                    to,
                    adoption_ceiling_seconds: switch::ADOPTION_CEILING_SECONDS,
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
