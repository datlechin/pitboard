//! pitboard's operations, each run the way every front end must run it: a change settles any
//! interrupted switch first and is recorded in the audit log, and what went wrong on the way
//! is reported alongside the result, whether or not the change then succeeds.

use crate::context::Context;
use crate::doctor::{self, Diagnosis};
use crate::error::{Error, Result};
use crate::provider::ProviderId;
use crate::state::{self, Account, Key};
use crate::switch::{self, Enrolled, Outcome, Recovered, Renewal, Settled, SignIn};
use crate::{audit, schedule, status, statusline};
use std::fmt;

/// Something to know about that did not stop the operation.
#[derive(Debug)]
#[non_exhaustive]
pub enum Warning {
    /// An earlier switch had been interrupted; this run found what it did and recorded it.
    Recovered(Recovered),
    /// The login moved, but what the tool caches about who is signed in still names the
    /// previous account.
    ConfigNotUpdated(Error),
    /// Parked logins no longer in use that could not be deleted yet.
    ParksPendingRemoval(usize),
    /// The service refuses a parked login for good, so it was dropped.
    ParkedLoginRefused {
        tool: ProviderId,
        label: String,
    },
    RenewalFailed(Error),
    /// The tool's write lock stopped being pitboard's while a change was under way.
    LockCompromised {
        tool: ProviderId,
    },
    /// The environment authenticates the tool some other way, so the login pitboard moved
    /// is not the one a session will use.
    AuthOverridden {
        tool: ProviderId,
        names: Vec<String>,
    },
    /// The login was too large for `security`'s stdin, so it went on the argument line.
    WrittenOnTheCommandLine {
        tool: ProviderId,
        bytes: usize,
        limit: usize,
    },
    /// Sessions of a tool that never follows a switch on its own were running when it
    /// happened, and go on using the account they started with until they are restarted.
    SessionsStillRunning {
        program: &'static str,
        count: usize,
        from: String,
    },
}

impl Warning {
    /// Stable, for a program to branch on.
    pub fn code(&self) -> &'static str {
        match self {
            Warning::Recovered(r) => r.code(),
            Warning::LockCompromised { .. } => "lock_compromised",
            Warning::ConfigNotUpdated(e) | Warning::RenewalFailed(e) => e.code(),
            Warning::ParksPendingRemoval(_) => "parks_pending_removal",
            Warning::ParkedLoginRefused { .. } => "parked_login_refused",
            Warning::AuthOverridden { .. } => "auth_overridden",
            Warning::WrittenOnTheCommandLine { .. } => "written_on_the_command_line",
            Warning::SessionsStillRunning { .. } => "sessions_still_running",
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Warning::Recovered(r) => write!(f, "{r}"),
            Warning::LockCompromised { tool } => write!(
                f,
                "{} reclaimed the credential write lock while this change was under way, so \
                 it may have written the login at the same time. pitboard read the slot back \
                 and the change stood, but check with `pitboard` that the right account is \
                 signed in.",
                tool.name()
            ),
            Warning::ConfigNotUpdated(e) | Warning::RenewalFailed(e) => write!(f, "{e}"),
            Warning::ParksPendingRemoval(count) => write!(
                f,
                "{count} parked login(s) no longer in use could not be removed yet; pitboard \
                 tries again on its next change"
            ),
            Warning::ParkedLoginRefused { tool, label } => write!(
                f,
                "{} no longer accepts the parked login for `{label}`. Run `pitboard enroll \
                 {label} --sign-in` to sign in to it again.",
                tool.service()
            ),
            Warning::WrittenOnTheCommandLine { tool, bytes, limit } => {
                write!(
                    f,
                    "this login needs {bytes} bytes and `security` reads {limit} from stdin, \
                     so it was written on the argument line, where a process running as you \
                     could have read it while the call lasted."
                )?;
                if *tool == ProviderId::Claude {
                    write!(
                        f,
                        " Claude Code writes this same login the same way whenever it \
                         refreshes the token."
                    )?;
                }
                Ok(())
            }
            Warning::AuthOverridden { tool, names } => write!(
                f,
                "{} is set, so {} signs in with it and not with the login pitboard moved. \
                 Unset it for the switch to take effect.",
                names.join(" and "),
                tool.name()
            ),
            Warning::SessionsStillRunning {
                program,
                count,
                from,
            } => write!(
                f,
                "{count} `{program}` session{} started before this switch {} still running and \
                 still using `{from}`. Quit {} and start again to use the new account. Quit \
                 rather than signing out inside one: signing out there revokes `{from}`'s \
                 login, which pitboard has just parked.",
                if *count == 1 { "" } else { "s" },
                if *count == 1 { "is" } else { "are" },
                if *count == 1 { "it" } else { "them" },
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
        let renewed = switch::renew_parked(&self.ctx);
        // Unreadable is not the same as empty: reporting it as empty would say the enrolled
        // logins are gone.
        let state = state::load(&self.ctx)?;
        for (key, outcome) in renewed {
            audit::record(&self.ctx, "renew", &key.typed(), outcome.code());
            match outcome {
                Renewal::Refused => warnings.push(Warning::ParkedLoginRefused {
                    tool: key.provider,
                    label: state.typed(&key),
                }),
                Renewal::Failed(e) => warnings.push(Warning::RenewalFailed(e)),
                Renewal::Renewed | Renewal::Deferred => {}
            }
        }
        Ok(Done {
            value: status::gather(&self.ctx, &state, fresh),
            warnings,
        })
    }

    pub fn doctor(&self) -> Diagnosis {
        doctor::run(&self.ctx)
    }

    /// The same report without asking anyone: the last numbers pitboard measured, and who
    /// each tool's own files say is signed in. Nothing is renewed and nothing is asked, so
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
        let key = self.named("use", typed)?;
        self.changing("use", &key.typed(), Some(key.provider), |settled| {
            switch::switch(settled, &key)
        })
    }

    /// Which account somebody meant, as the key the engine looks accounts up by.
    ///
    /// Resolving here rather than deeper down means every command takes `codex/work` and
    /// a bare `work` on the same terms, and the one place that decides what an ambiguous
    /// bare label does is the one place that knows every provider's accounts.
    fn named(&self, verb: &str, typed: &str) -> std::result::Result<Key, Failed> {
        let state = state::load(&self.ctx).map_err(|error| Failed {
            error,
            warnings: Vec::new(),
        })?;
        crate::label::resolve(&state, typed)
            .map(Account::key)
            .map_err(|error| {
                audit::record(&self.ctx, verb, typed, error.code());
                Failed {
                    error,
                    warnings: Vec::new(),
                }
            })
    }

    /// `typed` may name a tool, as in `claude/work`. A bare name means the default tool.
    pub fn enroll_current(&self, typed: &str) -> Changing<Enrolled> {
        let key = self.chosen("enroll", typed)?;
        self.changing("enroll", &key.typed(), Some(key.provider), |settled| {
            switch::enroll(settled, &key, None)
        })
    }

    /// Which tool a new account is for, and what it is called there.
    ///
    /// Split here rather than deeper down so nothing below ever sees a name with a tool
    /// still stuck to the front of it, which would enrol an account literally called
    /// `claude/work`.
    fn chosen(&self, verb: &str, typed: &str) -> std::result::Result<Key, Failed> {
        self.enrolling(typed).map_err(|error| {
            audit::record(&self.ctx, verb, typed, "label_unusable");
            Failed {
                error,
                warnings: Vec::new(),
            }
        })
    }

    /// The account `pitboard enroll <typed>` is about: the one already enrolled under that
    /// exact name, or a new one of the tool the name says.
    ///
    /// A label written by 0.1.x could contain a slash, which a new name cannot, and signing
    /// in to such an account again is exactly what every message about a lapsed park tells
    /// somebody to do. So an existing account is found by its whole name first.
    fn enrolling(&self, typed: &str) -> Result<Key> {
        if typed.contains(crate::label::SEPARATOR)
            && let Ok(state) = state::load(&self.ctx)
            && let Some(existing) = state.accounts.iter().find(|a| a.label == typed)
        {
            return Ok(existing.key());
        }
        crate::label::choose(typed)
            .map(|chosen| Key::new(chosen.provider, chosen.label))
            .map_err(Error::Usage)
    }

    /// The enrolled account `pitboard enroll <typed>` would sign in to again, if it names
    /// one, for saying whose login to sign in with before a browser opens.
    pub fn account_to_enroll(&self, typed: &str) -> Option<Account> {
        let key = self.enrolling(typed).ok()?;
        state::load(&self.ctx).ok()?.get(&key).cloned()
    }

    /// What to type to name the account `pitboard enroll <typed>` is about, on this
    /// machine: bare where that names it alone, qualified where another tool shares it.
    pub fn name_to_type(&self, typed: &str) -> String {
        let Ok(key) = self.enrolling(typed) else {
            return typed.to_string();
        };
        state::load(&self.ctx).map_or_else(|_| key.typed(), |state| state.typed(&key))
    }

    /// The tool's own sign-in in a private directory, for the tool `typed` names. It takes
    /// no lock but its own, so a person taking their time in a browser never holds up a
    /// switch.
    pub fn sign_in(&self, typed: &str) -> Result<SignIn> {
        self.signing_in(typed)
            .and_then(|tool| switch::sign_in(&self.ctx, tool))
            .inspect_err(|e| audit::record(&self.ctx, "enroll", typed, e.code()))
    }

    /// The same sign-in with its output piped, for a front end that has no terminal to
    /// hand over. The caller shows what the tool says and can type a code back.
    pub fn sign_in_watched(&self, typed: &str) -> Result<switch::WatchedSignIn> {
        self.signing_in(typed)
            .and_then(|tool| switch::sign_in_watched(&self.ctx, tool))
            .inspect_err(|e| audit::record(&self.ctx, "enroll", typed, e.code()))
    }

    /// Which tool a sign-in is for, once everything that could refuse it has been asked.
    ///
    /// Checked first, so a person does not sign in through a browser only to be told the
    /// state file belongs to another machine, that the tool is not installed, or that the
    /// account could never be switched to afterwards.
    fn signing_in(&self, typed: &str) -> Result<crate::provider::ProviderId> {
        let tool = self.enrolling(typed)?.provider;
        if tool == crate::provider::ProviderId::Claude && self.ctx.custom_oauth() {
            return Err(Error::CustomOauthEndpoint);
        }
        state::load(&self.ctx)?;
        let driver = crate::provider::of(tool);
        // An account signed in here is one to switch to later, which needs a live store
        // pitboard can write. Asked now rather than after a browser round trip.
        switch::live_store(&self.ctx, tool)?;
        // A private sign-in works by pointing the tool's own login at a scratch directory
        // through its home variable. Where that does not really isolate it, running one
        // would write over the login somebody is using, and there is no override: forcing
        // past "this could touch your live login" is what the rule exists to prevent.
        if let crate::provider::Isolation::NotIsolated { reason } =
            driver.private_signin_isolation(&self.ctx)
        {
            return Err(Error::SignInNotIsolated { reason });
        }
        if driver.program(&self.ctx).is_none() {
            return Err(Error::ProgramMissing {
                tool,
                program: self.ctx.program_for(tool).display().to_string(),
            });
        }
        Ok(tool)
    }

    pub fn enroll_signed_in(&self, typed: &str, login: SignIn) -> Changing<Enrolled> {
        let key = self.chosen("enroll", typed)?;
        self.changing("enroll", &key.typed(), Some(key.provider), |settled| {
            switch::enroll(settled, &key, Some(login))
        })
    }

    /// Returns the account's email.
    pub fn forget(&self, typed: &str) -> Changing<String> {
        let key = self.named("forget", typed)?;
        self.changing("forget", &key.typed(), Some(key.provider), |settled| {
            switch::forget(settled, &key)
        })
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
    pub fn renew(&self) -> Vec<(Key, Renewal)> {
        let outcomes = switch::renew_due(&self.ctx, switch::Due::ToStayAlive);
        for (key, outcome) in &outcomes {
            audit::record(&self.ctx, "renew", &key.typed(), outcome.code());
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
        self.changing("repair", "", None, |settled| {
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
        self.changing("uninstall", "", None, |settled| {
            switch::uninstall(settled).map(|r| (r, Vec::new()))
        })
    }

    /// Returns the account's email.
    /// `from` may be qualified; `to` is a plain name, and stays inside whichever provider
    /// the account already belongs to. Renaming cannot move an account between tools.
    pub fn rename(&self, from: &str, to: &str) -> Changing<String> {
        let from = self.named("rename", from)?;
        let chosen = self.chosen("rename", to)?;
        // Only a prefix somebody actually typed can disagree: a bare new name stays inside
        // the account's own tool whatever tool a bare name would mean for a new account.
        if to.contains(crate::label::SEPARATOR) && chosen.provider != from.provider {
            let error = Error::Usage(format!(
                "`{from}` is a {} account, and a rename cannot move it to {}. Sign in to that \
                 tool and enrol the account there instead.",
                from.provider, chosen.provider
            ));
            audit::record(&self.ctx, "rename", &from.typed(), error.code());
            return Err(Failed {
                error,
                warnings: Vec::new(),
            });
        }
        let to = chosen.label;
        self.changing(
            "rename",
            &format!("{from} -> {to}"),
            Some(from.provider),
            |settled| switch::rename(settled, &from, &to).map(|email| (email, Vec::new())),
        )
    }

    /// Settles, runs the change, and records it in the audit log.
    ///
    /// `tool` is the tool whose login the change is about, where it is about one: its own
    /// ways of being signed in by something else are what is worth warning about, and a
    /// refusal that is one tool's business does not stop a change to another's.
    fn changing<T: Audited>(
        &self,
        verb: &str,
        subject: &str,
        tool: Option<ProviderId>,
        run: impl FnOnce(Settled) -> Result<(T, Vec<Warning>)>,
    ) -> Changing<T> {
        let (settled, recovered) = switch::settle(&self.ctx, tool).map_err(|error| {
            audit::record(&self.ctx, verb, subject, error.code());
            Failed {
                error,
                warnings: Vec::new(),
            }
        })?;
        let mut warnings = Vec::new();
        // Read from files as well as from this process's environment, so the app, which
        // has no shell environment at all, gets the same answer as the command line.
        if let Some(tool) = tool {
            let names = crate::provider::of(tool).overridden_by(&self.ctx);
            if !names.is_empty() {
                warnings.push(Warning::AuthOverridden { tool, names });
            }
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
