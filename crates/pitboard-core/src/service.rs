//! pitboard's operations, each run the way every front end must run it: a change settles any
//! interrupted switch first and is recorded in the audit log, and what went wrong on the way
//! is reported alongside the result, whether or not the change then succeeds.

use crate::context::Context;
use crate::doctor::{self, Diagnosis};
use crate::error::{Error, Result};
use crate::state::{self, Account};
use crate::switch::{self, Enrolled, Outcome, Recovered, Renewal, Settled, SignIn};
use crate::{audit, claude, status, statusline};
use std::fmt;

/// Something to know about that did not stop the operation.
#[derive(Debug)]
pub enum Warning {
    /// An earlier switch had been interrupted; this run found what it did and recorded it.
    Recovered(Recovered),
    /// The login moved, but Claude Code's config still names the previous account.
    ConfigNotUpdated(Error),
    /// Parked logins no longer in use that could not be deleted yet.
    ParksPendingRemoval(usize),
    /// Anthropic refuses a parked login for good, so it was dropped.
    ParkedLoginRefused {
        label: String,
    },
    RenewalFailed(Error),
    /// The environment authenticates Claude Code some other way, so the login pitboard
    /// moved is not the one a session will use.
    AuthOverridden {
        names: Vec<String>,
    },
    /// The login was too large for `security`'s stdin, so it went on the argument line.
    WrittenOnTheCommandLine {
        bytes: usize,
        limit: usize,
    },
}

impl Warning {
    /// Stable, for a program to branch on.
    pub fn code(&self) -> &'static str {
        match self {
            Warning::Recovered(r) => r.code(),
            Warning::ConfigNotUpdated(e) | Warning::RenewalFailed(e) => e.code(),
            Warning::ParksPendingRemoval(_) => "parks_pending_removal",
            Warning::ParkedLoginRefused { .. } => "parked_login_refused",
            Warning::AuthOverridden { .. } => "auth_overridden",
            Warning::WrittenOnTheCommandLine { .. } => "written_on_the_command_line",
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Warning::Recovered(r) => write!(f, "{r}"),
            Warning::ConfigNotUpdated(e) | Warning::RenewalFailed(e) => write!(f, "{e}"),
            Warning::ParksPendingRemoval(count) => write!(
                f,
                "{count} parked login(s) no longer in use could not be removed yet; pitboard \
                 tries again on its next change"
            ),
            Warning::ParkedLoginRefused { label } => write!(
                f,
                "Anthropic no longer accepts the parked login for `{label}`. Run `pitboard \
                 enroll {label} --sign-in` to sign in to it again."
            ),
            Warning::WrittenOnTheCommandLine { bytes, limit } => write!(
                f,
                "this login needs {bytes} bytes and `security` reads {limit} from stdin, so \
                 it was written on the argument line, where a process running as you could \
                 have read it while the call lasted. Claude Code writes this same login the \
                 same way whenever it refreshes the token."
            ),
            Warning::AuthOverridden { names } => write!(
                f,
                "{} is set, so Claude Code signs in with it and not with the login pitboard \
                 moved. Unset it for the switch to take effect.",
                names.join(" and ")
            ),
        }
    }
}

#[derive(Debug)]
pub struct Done<T> {
    pub value: T,
    pub warnings: Vec<Warning>,
}

/// A change that failed, with what was found on the way: recovering an interrupted switch
/// is reported even when the change that followed it fails.
#[derive(Debug)]
pub struct Failed {
    pub error: Error,
    pub warnings: Vec<Warning>,
}

pub type Changing<T> = std::result::Result<Done<T>, Failed>;

pub struct Pitboard {
    ctx: Context,
}

impl Pitboard {
    pub fn new(ctx: Context) -> Pitboard {
        Pitboard { ctx }
    }

    /// Who is signed in and what every account has left. Parked logins whose access has
    /// lapsed are renewed first, so every account is asked live.
    pub fn status(&self) -> Result<Done<status::Report>> {
        let mut warnings = Vec::new();
        for (label, outcome) in switch::renew_parked(&self.ctx) {
            audit::record(&self.ctx, "renew", &label, outcome.code());
            match outcome {
                Renewal::Refused => warnings.push(Warning::ParkedLoginRefused { label }),
                Renewal::Failed(e) => warnings.push(Warning::RenewalFailed(e)),
                Renewal::Renewed | Renewal::Deferred => {}
            }
        }
        // Unreadable is not the same as empty: reporting it as empty would say the enrolled
        // logins are gone.
        let state = state::load(&self.ctx)?;
        Ok(Done {
            value: status::gather(&self.ctx, &state),
            warnings,
        })
    }

    pub fn doctor(&self) -> Diagnosis {
        doctor::run(&self.ctx)
    }

    /// The status line for Claude Code's session JSON. Reads only files.
    /// The same report without asking anyone: the last numbers pitboard measured, and who
    /// Claude Code's config says is signed in. Nothing is renewed and nothing is asked, so
    /// it answers at once wherever there is no network.
    pub fn status_offline(&self) -> Result<Done<status::Report>> {
        let state = state::load(&self.ctx)?;
        Ok(Done {
            value: status::gather_offline(&self.ctx, &state),
            warnings: Vec::new(),
        })
    }

    pub fn statusline(&self, session: &str) -> statusline::StatusLine {
        statusline::read(&self.ctx, session)
    }

    /// The enrolled account under `label`, if any, read without taking the lock.
    pub fn account(&self, label: &str) -> Option<Account> {
        state::load(&self.ctx).ok()?.get(label).cloned()
    }

    pub fn switch_to(&self, label: &str) -> Changing<Outcome> {
        self.changing("use", label, |settled| switch::switch(settled, label))
    }

    pub fn enroll_current(&self, label: &str) -> Changing<Enrolled> {
        self.changing("enroll", label, |settled| {
            switch::enroll(settled, label, None).map(|e| (e, Vec::new()))
        })
    }

    /// Claude Code's own sign-in in a private directory. It takes no lock but its own, so a
    /// person taking their time in a browser never holds up a switch.
    pub fn sign_in(&self, label: &str) -> Result<SignIn> {
        self.before_signing_in()
            .and_then(|()| switch::sign_in(&self.ctx))
            .inspect_err(|e| audit::record(&self.ctx, "enroll", label, e.code()))
    }

    /// The same sign-in with its output piped, for a front end that has no terminal to
    /// hand over. The caller shows what Claude Code says and can type a code back.
    pub fn sign_in_watched(&self) -> Result<switch::WatchedSignIn> {
        self.before_signing_in()?;
        switch::sign_in_watched(&self.ctx)
    }

    /// Everything that can refuse an enrolment and is knowable before the new login exists.
    /// Checked first, so a person does not sign in through a browser only to be told the
    /// state file belongs to another machine or that `claude` is not installed.
    fn before_signing_in(&self) -> Result<()> {
        if self.ctx.custom_oauth() {
            return Err(Error::CustomOauthEndpoint);
        }
        state::load(&self.ctx)?;
        if claude::program(&self.ctx).is_none() {
            return Err(Error::ClaudeProgramMissing {
                program: self.ctx.claude_program().display().to_string(),
            });
        }
        Ok(())
    }

    pub fn enroll_signed_in(&self, label: &str, login: SignIn) -> Changing<Enrolled> {
        self.changing("enroll", label, |settled| {
            switch::enroll(settled, label, Some(login)).map(|e| (e, Vec::new()))
        })
    }

    /// Returns the account's email.
    pub fn forget(&self, label: &str) -> Changing<String> {
        self.changing("forget", label, |settled| switch::forget(settled, label))
    }

    /// Throws away a record of an interrupted switch that cannot be finished, keeping
    /// every login it names. The way out when recovery cannot reach Anthropic.
    pub fn abandon_recovery(&self) -> Result<Option<switch::Abandoned>> {
        let outcome = switch::abandon(&self.ctx);
        audit::record(
            &self.ctx,
            "abandon",
            "",
            match &outcome {
                Ok(_) => "ok",
                Err(e) => e.code(),
            },
        );
        outcome
    }

    /// The changes pitboard has made, newest last.
    pub fn log(&self, limit: usize) -> Vec<audit::Entry> {
        audit::read(&self.ctx, limit)
    }

    /// Deletes every parked login and pitboard's own directory. Claude Code's login is
    /// left alone: whoever is signed in stays signed in.
    pub fn uninstall(&self) -> Changing<switch::Removed> {
        self.changing("uninstall", "", |settled| {
            switch::uninstall(settled).map(|r| (r, Vec::new()))
        })
    }

    /// Returns the account's email.
    pub fn rename(&self, from: &str, to: &str) -> Changing<String> {
        self.changing("rename", &format!("{from} -> {to}"), |settled| {
            switch::rename(settled, from, to).map(|email| (email, Vec::new()))
        })
    }

    /// Settles, runs the change, and records it in the audit log.
    fn changing<T: Audited>(
        &self,
        verb: &str,
        subject: &str,
        run: impl FnOnce(Settled) -> Result<(T, Vec<Warning>)>,
    ) -> Changing<T> {
        let (settled, recovered) = switch::settle(&self.ctx).map_err(|error| {
            audit::record(&self.ctx, verb, subject, error.code());
            Failed {
                error,
                warnings: Vec::new(),
            }
        })?;
        let mut warnings = Vec::new();
        if !self.ctx.overriding_auth().is_empty() {
            warnings.push(Warning::AuthOverridden {
                names: self.ctx.overriding_auth().to_vec(),
            });
        }
        if let Some(r) = recovered {
            audit::record(&self.ctx, "recover", &r.to, r.code());
            warnings.push(Warning::Recovered(r));
        }
        match run(settled) {
            Ok((value, more)) => {
                audit::record(&self.ctx, verb, subject, value.audit_code());
                warnings.extend(more);
                Ok(Done { value, warnings })
            }
            Err(error) => {
                audit::record(&self.ctx, verb, subject, error.code());
                Err(Failed { error, warnings })
            }
        }
    }
}

/// How a successful change is written in the audit log.
trait Audited {
    fn audit_code(&self) -> &'static str {
        "ok"
    }
}

impl Audited for Outcome {
    fn audit_code(&self) -> &'static str {
        match self {
            Outcome::Switched { .. } => "ok",
            Outcome::AlreadyActive { .. } => "already_active",
        }
    }
}

impl Audited for Enrolled {}
impl Audited for String {}

impl Audited for switch::Removed {
    fn audit_code(&self) -> &'static str {
        if self.pending > 0 {
            "parks_pending_removal"
        } else {
            "ok"
        }
    }
}
