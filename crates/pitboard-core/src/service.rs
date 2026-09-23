//! pitboard's operations, each run the way every front end must run it: a change settles any
//! interrupted switch first and is recorded in the audit log, and what went wrong on the way
//! is reported alongside the result, whether or not the change then succeeds.

use crate::context::Context;
use crate::doctor::{self, Diagnosis};
use crate::error::{Error, Result};
use crate::provider::claude::paths as claude;
use crate::state::{self, Account};
use crate::switch::{self, Enrolled, Outcome, Recovered, Renewal, Settled, SignIn};
use crate::{audit, schedule, status, statusline};
use std::fmt;

/// Something to know about that did not stop the operation.
#[derive(Debug)]
#[non_exhaustive]
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
    /// Claude Code's write lock stopped being pitboard's while a change was under way.
    LockCompromised,
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
            Warning::LockCompromised => "lock_compromised",
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
            Warning::LockCompromised => write!(
                f,
                "Claude Code reclaimed the credential write lock while this change was \
                 under way, so it may have written the login at the same time. pitboard \
                 read the slot back and the change stood, but check with `pitboard` that \
                 the right account is signed in."
            ),
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
    ///
    /// `fresh` asks Anthropic about every account whatever was asked recently. Ordinarily
    /// false: a number is only asked for again once the tightest limit it describes could
    /// have moved by a percentage point, which collapses several front ends on one machine
    /// to one request per account per few minutes.
    pub fn status(&self, fresh: bool) -> Result<Done<status::Report>> {
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
            value: status::gather(&self.ctx, &state, fresh),
            warnings,
        })
    }

    pub fn doctor(&self) -> Diagnosis {
        doctor::run(&self.ctx)
    }

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

    /// The status line for Claude Code's session JSON. Reads only files.
    pub fn statusline(&self, session: &str) -> statusline::StatusLine {
        statusline::read(&self.ctx, session)
    }

    /// The enrolled account under `typed`, if any, read without taking the lock.
    pub fn account(&self, typed: &str) -> Option<Account> {
        let state = state::load(&self.ctx).ok()?;
        crate::label::resolve(&state, typed).ok().cloned()
    }

    pub fn switch_to(&self, typed: &str) -> Changing<Outcome> {
        let label = self.named(typed)?;
        self.changing("use", &label, |settled| switch::switch(settled, &label))
    }

    /// Which account somebody meant, as a plain label the engine can look up.
    ///
    /// Resolving here rather than deeper down means every command takes `codex/work` and
    /// a bare `work` on the same terms, and the one place that decides what an ambiguous
    /// bare label does is the one place that knows every provider's accounts.
    fn named(&self, typed: &str) -> std::result::Result<String, Failed> {
        let state = state::load(&self.ctx).map_err(|error| Failed {
            error,
            warnings: Vec::new(),
        })?;
        crate::label::resolve(&state, typed)
            .map(|account| account.label.clone())
            .map_err(|error| {
                audit::record(&self.ctx, "use", typed, error.code());
                Failed {
                    error,
                    warnings: Vec::new(),
                }
            })
    }

    /// `typed` may name a tool, as in `claude/work`. A bare name means the default tool.
    pub fn enroll_current(&self, typed: &str) -> Changing<Enrolled> {
        let chosen = self.chosen(typed)?;
        self.changing("enroll", &chosen.label, |settled| {
            switch::enroll(settled, chosen.provider, &chosen.label, None).map(|e| (e, Vec::new()))
        })
    }

    /// Which tool a new account is for, and what it is called there.
    ///
    /// Split here rather than deeper down so nothing below ever sees a name with a tool
    /// still stuck to the front of it, which would enrol an account literally called
    /// `claude/work`.
    fn chosen(&self, typed: &str) -> std::result::Result<crate::label::Chosen, Failed> {
        crate::label::choose(typed).map_err(|detail| {
            audit::record(&self.ctx, "enroll", typed, "label_unusable");
            Failed {
                error: Error::Usage(detail),
                warnings: Vec::new(),
            }
        })
    }

    /// Claude Code's own sign-in in a private directory. It takes no lock but its own, so a
    /// person taking their time in a browser never holds up a switch.
    pub fn sign_in(&self, typed: &str) -> Result<SignIn> {
        self.before_signing_in()
            .and_then(|()| switch::sign_in(&self.ctx))
            .inspect_err(|e| audit::record(&self.ctx, "enroll", typed, e.code()))
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
        // A private sign-in works by pointing the tool's own login at a scratch directory
        // through its home variable. Where that does not really isolate it, running one
        // would write over the login somebody is using. Claude Code is always isolated;
        // the check is here so the tool that is not gets refused rather than special-cased.
        if let crate::provider::Isolation::NotIsolated { reason } =
            crate::provider::of(crate::provider::ProviderId::Claude)
                .private_signin_isolation(&self.ctx)
        {
            return Err(Error::SignInNotIsolated { reason });
        }
        if claude::program(&self.ctx).is_none() {
            return Err(Error::ClaudeProgramMissing {
                program: self.ctx.claude_program().display().to_string(),
            });
        }
        Ok(())
    }

    pub fn enroll_signed_in(&self, typed: &str, login: SignIn) -> Changing<Enrolled> {
        let chosen = self.chosen(typed)?;
        self.changing("enroll", &chosen.label, |settled| {
            switch::enroll(settled, chosen.provider, &chosen.label, Some(login))
                .map(|e| (e, Vec::new()))
        })
    }

    /// Returns the account's email.
    pub fn forget(&self, typed: &str) -> Changing<String> {
        let label = self.named(typed)?;
        self.changing("forget", &label, |settled| switch::forget(settled, &label))
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

    /// Renew every parked login that is due, and nothing else. No switch, no usage, and
    /// no request but the token exchange. This is what the schedule runs.
    pub fn renew(&self) -> Vec<(String, Renewal)> {
        let outcomes = switch::renew_due(&self.ctx, switch::Due::ToStayAlive);
        for (label, outcome) in &outcomes {
            audit::record(&self.ctx, "renew", label, outcome.code());
        }
        outcomes
    }

    /// Whether anything is keeping parked logins alive on this machine without somebody
    /// running a command.
    pub fn schedule(&self) -> schedule::Installed {
        schedule::status(&self.ctx)
    }

    /// Ask the platform's own scheduler to run `renew` daily. Opt-in, and stays opt-in.
    pub fn schedule_install(&self) -> Result<std::path::PathBuf> {
        schedule::install(&self.ctx)
    }

    /// Take it away. `false` when there was nothing installed.
    pub fn schedule_uninstall(&self) -> Result<bool> {
        schedule::uninstall(&self.ctx)
    }

    /// Take over a pitboard directory another machine wrote: keep the accounts, drop the
    /// logins that came with them. `None` when the directory was already this machine's.
    ///
    /// The one change that does not settle first, because a stamp from elsewhere is what
    /// stops settling. Everything after it settles normally.
    pub fn adopt(&self) -> Result<Option<switch::Adopted>> {
        switch::adopt(&self.ctx)
    }

    /// Ask the credential store what parked logins are on this machine, and give back or
    /// delete every one pitboard's own records do not name. Ordinarily there is nothing to
    /// do: every change resolves the names it wrote down. This is for a machine whose state
    /// file was lost or restored from a backup, where the store is the only record left.
    pub fn repair(&self) -> Changing<switch::Reclaimed> {
        self.changing("repair", "", |settled| {
            switch::repair(settled).map(|r| (r, Vec::new()))
        })
    }

    /// When pitboard's account index last changed, for a front end that wants to know
    /// whether another one has done something without asking Anthropic about it.
    pub fn changed_at(&self) -> i64 {
        state::changed_at(&self.ctx)
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
    /// `from` may be qualified; `to` is a plain name, and stays inside whichever provider
    /// the account already belongs to. Renaming cannot move an account between tools.
    pub fn rename(&self, from: &str, to: &str) -> Changing<String> {
        let from = self.named(from)?;
        let to = self.chosen(to)?.label;
        self.changing("rename", &format!("{from} -> {to}"), |settled| {
            switch::rename(settled, &from, &to).map(|email| (email, Vec::new()))
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
        // Read from files as well as from this process's environment, so the app, which
        // has no shell environment at all, gets the same answer as the command line.
        let overridden = crate::settings::overrides(&self.ctx);
        if !overridden.is_empty() {
            warnings.push(Warning::AuthOverridden {
                names: overridden.iter().map(ToString::to_string).collect(),
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

impl Audited for switch::Reclaimed {}
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
