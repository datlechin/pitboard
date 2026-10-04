//! Pitboard's operations, each run the way every front end must run it: a change settles any
//! interrupted switch first and is recorded in the audit log, and what went wrong on the way
//! is reported alongside the result, whether or not the change then succeeds.

use crate::context::Context;
use crate::doctor::{self, Diagnosis};
use crate::error::{Error, Result};
use crate::holder::{self, capitalised};
use crate::provider::ProviderId;
use crate::state::{self, Account, Key};
use crate::switch::{self, Enrolled, Outcome, Recovered, Renewal, Settled, SignIn};
use crate::{audit, readings, schedule, status, statusline};
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
    /// The tool's write lock stopped being Pitboard's while a change was under way.
    LockCompromised {
        tool: ProviderId,
    },
    /// The environment authenticates the tool some other way, so the login Pitboard moved
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
    /// `holding` is what was running, by kind, never empty.
    SessionsStillRunning {
        from: String,
        holding: Vec<crate::holder::Holding>,
    },
    /// A sign-in put a new login in use in place of the old one of the same account, and
    /// sessions of a tool that never reads its login again were running with the old one.
    SessionsKeepTheOldLogin {
        label: String,
        holding: Vec<crate::holder::Holding>,
    },
    /// A sign-in to the account Pitboard last recorded in use was parked rather than put in
    /// use, because nobody could say whose login the tool has in use, for `why`.
    SignInParkedNotInUse {
        tool: ProviderId,
        label: String,
        why: String,
    },
    /// An earlier switch of a tool kept in a folder was interrupted, and the app is open, so
    /// it could not be finished on a run that only reads.
    RecoveryWaiting {
        tool: ProviderId,
        from: String,
        to: String,
    },
    /// A switch of a tool kept in a folder failed partway and left its record, so half of
    /// each account may be where the other's should be until the next change finishes or
    /// undoes it. The app must stay closed until then.
    SwitchUnfinished {
        tool: ProviderId,
        from: String,
        to: String,
    },
    /// A parked Claude Desktop login cannot be renewed, and its session runs out at
    /// `expires_at`, in epoch seconds.
    ParkExpiresSoon {
        label: String,
        expires_at: i64,
    },
    /// Items a switch found in the data folder that belong to neither account were set aside
    /// in `path` rather than deleted.
    StraysKept {
        path: std::path::PathBuf,
        count: usize,
    },
    /// Somebody signed the tool in to another account outside Pitboard, and the account
    /// recorded in use had no park, so it is no longer recorded in use.
    ReplacedOutsidePitboard {
        tool: ProviderId,
        label: String,
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
            Warning::SessionsKeepTheOldLogin { .. } => "sessions_keep_old_login",
            Warning::SignInParkedNotInUse { .. } => "sign_in_parked_not_in_use",
            Warning::RecoveryWaiting { .. } => "recovery_waiting",
            Warning::SwitchUnfinished { .. } => "switch_unfinished",
            Warning::ParkExpiresSoon { .. } => "park_expires_soon",
            Warning::StraysKept { .. } => "strays_kept",
            Warning::ReplacedOutsidePitboard { .. } => "replaced_outside_pitboard",
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
                 it may have written the login at the same time. Pitboard read the slot back \
                 and the change stood, but check with `pitboard` that the right account is \
                 signed in.",
                tool.name()
            ),
            Warning::ConfigNotUpdated(e) | Warning::RenewalFailed(e) => write!(f, "{e}"),
            Warning::ParksPendingRemoval(count) => write!(
                f,
                "{count} parked login(s) no longer in use could not be removed yet; Pitboard \
                 tries again on its next change"
            ),
            Warning::ParkedLoginRefused { tool, label } => write!(
                f,
                "{} no longer accepts the parked login for `{label}`. {}",
                tool.service(),
                crate::error::sign_in_again(label, "sign in to it again")
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
                "{} is set, so {} signs in with it and not with the login Pitboard moved. \
                 Unset it for the switch to take effect.",
                names.join(" and "),
                tool.name()
            ),
            Warning::SessionsStillRunning { from, holding } => write!(
                f,
                "{} started before this switch {} still running and still using `{from}`. {} \
                 Do not sign out in {}: signing out there revokes `{from}`'s login, which \
                 Pitboard has just parked.",
                capitalised(&holder::described(holding)),
                if holder::plural(holding) { "are" } else { "is" },
                holder::remedies(holding, "to use the new account"),
                if holder::plural(holding) {
                    "any of them"
                } else {
                    "it"
                },
            ),
            Warning::SessionsKeepTheOldLogin { label, holding } => write!(
                f,
                "{} started before this sign-in {} still running and still using `{label}`'s \
                 old login. {} Otherwise one of them can put the old login back in place of \
                 the new one when it refreshes its token.",
                capitalised(&holder::described(holding)),
                if holder::plural(holding) { "are" } else { "is" },
                holder::remedies(holding, "to use the new one"),
            ),
            Warning::SignInParkedNotInUse { tool, label, why } => write!(
                f,
                "{} goes on with the login it has: Pitboard could not tell whose it is \
                 ({why}), so it parked the new login for `{label}` rather than write over \
                 that one. If that login no longer works, run `{}` and sign in to `{label}` \
                 there.",
                tool.name(),
                tool.login_command()
            ),
            Warning::RecoveryWaiting { tool, from, to } => write!(
                f,
                "an earlier switch of {} from `{from}` to `{to}` was interrupted, and {} is \
                 open, so Pitboard cannot finish it yet. Quit {}, and the next change \
                 finishes it.",
                tool.name(),
                tool.program(),
                tool.program()
            ),
            Warning::SwitchUnfinished { tool, from, to } => write!(
                f,
                "the switch of {} from `{from}` to `{to}` stopped partway, and Pitboard kept \
                 its record. Keep {} closed: the next change finishes or undoes it first.",
                tool.name(),
                tool.program()
            ),
            Warning::ParkExpiresSoon { label, expires_at } => write!(
                f,
                "the parked login for `{label}` runs out on {} and Pitboard cannot renew it. \
                 Switch to `{label}` before then to keep it.",
                crate::time::local(*expires_at, "%b %-d %H:%M")
            ),
            Warning::StraysKept { path, count } => write!(
                f,
                "{count} item(s) in Claude's data folder belonged to neither account, so \
                 Pitboard set them aside in {} rather than delete them.",
                path.display()
            ),
            Warning::ReplacedOutsidePitboard { tool, label } => write!(
                f,
                "{} was signed in to another account outside Pitboard, and `{label}` had no \
                 parked login to keep, so it is no longer recorded in use. Enroll it again to \
                 switch back to it.",
                tool.name()
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
                Renewal::Renewed | Renewal::Deferred | Renewal::NotRenewable { .. } => {}
            }
        }
        warnings.extend(switch::tree_waiting(&self.ctx, &state));
        Ok(Done {
            value: status::gather(&self.ctx, &state, fresh),
            warnings,
        })
    }

    pub fn doctor(&self) -> Diagnosis {
        doctor::run(&self.ctx)
    }

    /// The same report without asking anyone: the last numbers Pitboard measured, and who
    /// each tool's own files say is signed in. Nothing is renewed and nothing is asked, so
    /// it answers at once wherever there is no network.
    pub fn status_offline(&self) -> Result<Done<status::Report>> {
        let state = state::load(&self.ctx)?;
        Ok(Done {
            value: status::gather_offline(&self.ctx, &state),
            warnings: switch::tree_waiting(&self.ctx, &state)
                .into_iter()
                .collect(),
        })
    }

    /// The status line for Claude Code's session JSON. Reads only files, and writes only
    /// Pitboard's own: what the session passed, for its next run to compare with, and the
    /// usage readings, which keep what moved since its last run where it is newer.
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

    /// Sign `which`'s app out, parking the account in it, so somebody can open the app, sign
    /// in to another account and enrol that one. Only an app whose login is a folder signs
    /// out this way: a tool with a sign-in of its own adds an account with `--sign-in`.
    pub fn switch_to_signed_out(&self, which: ProviderId) -> Changing<Outcome> {
        let subject = format!("{which} -> signed out");
        if crate::provider::of(which).tree().is_none() {
            let error = Error::Usage(format!(
                "{} is not signed out by Pitboard. Add another account with `pitboard enroll \
                 {which}/<label> --sign-in`.",
                which.name()
            ));
            return Err(self.refused("use", &subject, Some(which), error.code(), error));
        }
        self.changing("use", &subject, Some(which), |settled| {
            switch::sign_out(settled, which)
        })
    }

    /// The sign-out Pitboard made that no enrolment has followed yet, where there is one.
    pub fn awaiting_sign_in(&self) -> Option<switch::Awaiting> {
        switch::awaiting_sign_in(&self.ctx)
    }

    /// Whether Claude Desktop's usage is asked of claude.ai, and whether macOS lets Pitboard
    /// read the key that needs. Reads Pitboard's own file and nothing else.
    pub fn live_usage(&self) -> status::LiveUsage {
        crate::provider::desktop::live_usage::load(&self.ctx)
    }

    /// Turn live usage on, which reads Claude's key once and may put macOS's question in
    /// front of somebody. Nothing else in Pitboard ever asks for it.
    pub fn enable_live_usage(&self) -> Result<status::LiveUsage> {
        let turned = crate::provider::desktop::live_usage::enable(&self.ctx);
        self.audit_live_usage("enable", &turned);
        turned
    }

    /// Turn live usage off and forget the key. Reads nothing.
    pub fn disable_live_usage(&self) -> Result<status::LiveUsage> {
        let turned = crate::provider::desktop::live_usage::disable(&self.ctx);
        self.audit_live_usage("disable", &turned);
        turned
    }

    fn audit_live_usage(&self, subject: &str, turned: &Result<status::LiveUsage>) {
        let outcome = match turned {
            Ok(_) => "ok",
            Err(e) => e.code(),
        };
        audit::record(&self.ctx, "live_usage", subject, outcome);
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
            .map_err(|error| self.refused(verb, typed, named_tool(typed), error.code(), error))
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
        self.enrolling(typed)
            .map_err(|error| self.refused(verb, typed, chosen_tool(typed), "label_unusable", error))
    }

    /// A change refused over the name it was given, which happens before it settles.
    ///
    /// A mistyped name takes no lock and writes nothing but its line in the audit log. But
    /// every change settles an interrupted switch first, and one refused here would leave
    /// that switch for whatever runs next and say nothing about it. So where a switch was
    /// interrupted, this settles for the tool that switch was of, which a custom OAuth
    /// endpoint allows or refuses exactly as it would a change to that tool, and reports what
    /// it found beside the refusal. Only as far as it can: a recovery that cannot finish is
    /// the next change's to report, and what this one reports is why it was refused.
    ///
    /// A folder login's interrupted run is settled only where the change would have settled
    /// it: one about its own tool, or about no tool the name says. `tool` is the tool the
    /// refused change was about, where its name says one. A change to Claude Code or Codex
    /// never moves anything of the Claude app's, even one refused for a typo.
    fn refused(
        &self,
        verb: &str,
        subject: &str,
        tool: Option<ProviderId>,
        code: &str,
        error: Error,
    ) -> Failed {
        let interrupted = switch::interrupted_tool(&self.ctx);
        let settle_for = if switch::interrupted(&self.ctx) {
            Some(interrupted)
        } else {
            // Only a folder login's run is left, and `interrupted` is its tool.
            interrupted
                .filter(|&folder| tool.is_none_or(|tool| tool == folder))
                .map(|_| tool)
        };
        let recovered = match settle_for.map(|which| switch::settle(&self.ctx, which)) {
            Some(Ok((_, recovered))) => recovered,
            // The app is open, which is what a change about it would have said.
            Some(Err(Error::RecoveryWaiting { tool, from, to })) => {
                vec![Warning::RecoveryWaiting { tool, from, to }]
            }
            Some(Err(_)) | None => Vec::new(),
        };
        let warnings = self.recovered(recovered);
        audit::record(&self.ctx, verb, subject, code);
        Failed { error, warnings }
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
    pub fn sign_in(&self, typed: &str) -> std::result::Result<SignIn, Failed> {
        let tool = self.signing_in(typed)?;
        switch::sign_in(&self.ctx, tool).map_err(|error| self.not_started(typed, error))
    }

    /// The same sign-in with its output piped, for a front end that has no terminal to
    /// hand over. The caller shows what the tool says and can type a code back.
    pub fn sign_in_watched(
        &self,
        typed: &str,
    ) -> std::result::Result<switch::WatchedSignIn, Failed> {
        let tool = self.signing_in(typed)?;
        switch::sign_in_watched(&self.ctx, tool).map_err(|error| self.not_started(typed, error))
    }

    /// Which tool a sign-in is for, once everything that could refuse it has been asked.
    ///
    /// A name no account could have is refused the way every change refuses one, settling
    /// an interrupted switch on the way; anything else refused here is recorded and nothing
    /// more, since nothing was about to change.
    fn signing_in(&self, typed: &str) -> std::result::Result<crate::provider::ProviderId, Failed> {
        let tool = self.chosen("enroll", typed)?.provider;
        self.ready_to_sign_in(tool)
            .map_err(|error| self.not_started(typed, error))?;
        Ok(tool)
    }

    /// A sign-in that did not start, or did not finish, for a reason other than its name.
    fn not_started(&self, typed: &str, error: Error) -> Failed {
        audit::record(&self.ctx, "enroll", typed, error.code());
        Failed {
            error,
            warnings: Vec::new(),
        }
    }

    /// Checked before a sign-in starts, so a person does not sign in through a browser
    /// only to be told the state file belongs to another machine, that the tool is not
    /// installed, or that the account could never be switched to afterwards.
    fn ready_to_sign_in(&self, tool: crate::provider::ProviderId) -> Result<()> {
        // A tool whose login is a folder is signed in to inside its own app, with the data
        // folder it always uses, so there is no private sign-in to run. Asked first, so
        // nothing below can ever start the app.
        if crate::provider::of(tool).tree().is_some() {
            return Err(Error::SignInUnsupported { tool });
        }
        if tool == crate::provider::ProviderId::Claude && self.ctx.custom_oauth() {
            return Err(Error::CustomOauthEndpoint);
        }
        state::load(&self.ctx)?;
        let driver = crate::provider::of(tool);
        // An account signed in here is one to switch to later, which needs a live store
        // Pitboard can write. Asked now rather than after a browser round trip.
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
        Ok(())
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
    ///
    /// A park that only its own app can renew is listed too, with when its session runs out,
    /// so a schedule or a person reading the output knows it is kept as it is rather than
    /// forgotten. It is not written in the audit log: nothing was done to it.
    pub fn renew(&self) -> Vec<(Key, Renewal)> {
        let mut outcomes = switch::renew_due(&self.ctx, switch::Due::ToStayAlive);
        for (key, outcome) in &outcomes {
            audit::record(&self.ctx, "renew", &key.typed(), outcome.code());
        }
        if let Ok(state) = state::load(&self.ctx) {
            outcomes.extend(
                state
                    .accounts
                    .iter()
                    .filter(|a| crate::provider::of(a.provider()).tree().is_some())
                    .filter_map(|a| {
                        let park = a.parked.as_ref()?;
                        let key = a.key();
                        let label = state.typed(&key);
                        Some((
                            key,
                            Renewal::NotRenewable {
                                label,
                                expires_at: park.refresh_expires_at,
                            },
                        ))
                    }),
            );
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

    /// Point a schedule an app up to 0.3.0 wrote, which runs the app itself, at the command
    /// line this context names. `true` when it did; nothing changes otherwise.
    pub fn schedule_repair(&self) -> Result<bool> {
        schedule::repair(&self.ctx)
    }

    /// Take over a Pitboard directory another machine wrote: keep the accounts, drop the
    /// logins that came with them. `None` when the directory was already this machine's.
    ///
    /// The one change that does not settle first, because a stamp from elsewhere is what
    /// stops settling. Everything after it settles normally.
    pub fn adopt(&self) -> Result<Option<switch::Adopted>> {
        switch::adopt(&self.ctx)
    }

    /// Ask the credential store what parked logins are on this machine, and give back or
    /// delete every one Pitboard's own records do not name. Ordinarily there is nothing to
    /// do: every change resolves the names it wrote down. This is for a machine whose state
    /// file was lost or restored from a backup, where the store is the only record left.
    pub fn repair(&self) -> Changing<switch::Reclaimed> {
        self.changing("repair", "", None, |settled| {
            switch::repair(settled).map(|r| (r, Vec::new()))
        })
    }

    /// What is running `which`'s tool with a login in memory that a switch would leave it
    /// on, by kind: for a front end to say so, or to offer to quit an app, before switching.
    /// Empty where nothing is, where the tool follows a switch by itself, or where nobody
    /// could tell. Reads the process list and nothing else.
    ///
    /// An app whose login is a folder is found by its own bundle: it is asked to quit before
    /// a switch rather than told afterwards, so what runs it matters before, not after.
    pub fn holding(&self, which: ProviderId) -> Vec<holder::Holding> {
        match crate::provider::of(which).tree() {
            Some(tree) => holder::find_within(&self.ctx, tree)
                .unwrap_or_default()
                .into_iter()
                .filter(|holding| !holding.pids.is_empty())
                .collect(),
            None => switch::still_holding(&self.ctx, which).unwrap_or_default(),
        }
    }

    /// When Pitboard's account index last changed, for a front end that wants to know
    /// whether another one has done something without asking Anthropic about it.
    pub fn changed_at(&self) -> i64 {
        state::changed_at(&self.ctx)
    }

    /// When Pitboard's usage readings last changed, in epoch milliseconds, for a front end
    /// that shows them to follow what the others record without asking anyone.
    pub fn readings_changed_at(&self) -> i64 {
        readings::changed_at(&self.ctx)
    }

    /// The changes Pitboard has made, newest last.
    pub fn log(&self, limit: usize) -> Vec<audit::Entry> {
        audit::read(&self.ctx, limit)
    }

    /// Takes away the daily renewal schedule, deletes every parked login this Pitboard
    /// wrote, and removes Pitboard's own directory. Each tool's login is left alone: whoever
    /// is signed in stays signed in.
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
            return Err(self.refused(
                "rename",
                &from.typed(),
                Some(from.provider),
                error.code(),
                error,
            ));
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
        warnings.extend(self.recovered(recovered));
        // Settled, so a record still here is one settling left alone, not this run's.
        let unfinished_before = switch::tree_interrupted(&self.ctx).is_some();
        match run(settled) {
            Ok((value, more)) => {
                audit::record(&self.ctx, verb, subject, value.audit_code());
                warnings.extend(more);
                Ok(Done { value, warnings })
            }
            Err(mut error) => {
                audit::record(&self.ctx, verb, subject, error.code());
                // A refusal that says a run is waiting for the app to quit says all the
                // warning would.
                if matches!(error, Error::RecoveryWaiting { .. }) {
                    warnings.retain(|w| !matches!(w, Warning::RecoveryWaiting { .. }));
                }
                warnings.extend(error.take_warnings());
                // A run that failed after its record was written left half of a switch,
                // which a refusal for the app opening midway already says.
                if !unfinished_before && !matches!(error, Error::AppStillOpen { midway: true, .. })
                {
                    warnings.extend(switch::tree_unfinished(&self.ctx));
                }
                Err(Failed { error, warnings })
            }
        }
    }

    /// What settling found, with each recovery written in the audit log.
    fn recovered(&self, found: Vec<Warning>) -> Vec<Warning> {
        for warning in &found {
            if let Warning::Recovered(r) = warning {
                audit::record(&self.ctx, "recover", &r.subject(), r.code());
            }
        }
        found
    }
}

/// The tool a name for an existing account says, where it says one. A bare name could be
/// any tool's.
fn named_tool(typed: &str) -> Option<ProviderId> {
    match crate::label::parse(typed) {
        Ok(crate::label::Spec::Qualified(tool, _)) => Some(tool),
        _ => None,
    }
}

/// The tool a name for a new account says: its prefix where that is a tool, and the
/// default tool where it is bare.
fn chosen_tool(typed: &str) -> Option<ProviderId> {
    match typed.split_once(crate::label::SEPARATOR) {
        None => Some(crate::label::DEFAULT),
        Some((prefix, _)) => ProviderId::parse(prefix),
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
            Outcome::Switched { .. } | Outcome::SignedOut { .. } | Outcome::Installed { .. } => {
                "ok"
            }
            Outcome::AlreadyActive { .. } => "already_active",
            Outcome::AlreadySignedOut { .. } => "already_signed_out",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::switch::harness::{Machine, codex_machine, hold, machine};
    use std::collections::BTreeMap;

    type Make = fn(&str) -> Machine;

    /// What a switch of Claude Desktop can warn about goes under the codes a front end
    /// branches on, and names the app, not Claude Code.
    #[test]
    fn claude_desktops_warnings_have_their_codes() {
        let warnings = [
            (
                Warning::RecoveryWaiting {
                    tool: ProviderId::Desktop,
                    from: "home".into(),
                    to: "work".into(),
                },
                "recovery_waiting",
            ),
            (
                Warning::ParkExpiresSoon {
                    label: "desktop/work".into(),
                    expires_at: 1_790_000_000,
                },
                "park_expires_soon",
            ),
            (
                Warning::StraysKept {
                    path: "/tmp/strays".into(),
                    count: 2,
                },
                "strays_kept",
            ),
            (
                Warning::ReplacedOutsidePitboard {
                    tool: ProviderId::Desktop,
                    label: "desktop/home".into(),
                },
                "replaced_outside_pitboard",
            ),
        ];
        for (warning, code) in warnings {
            assert_eq!(warning.code(), code);
            let said = warning.to_string();
            assert!(!said.contains("Claude Code"), "{said}");
            assert!(!said.contains('\u{2014}'), "{said}");
        }
    }

    /// Claude Desktop is signed in to inside the app, with the data folder it always uses,
    /// so Pitboard has no sign-in of its own to run. It is refused before anything is
    /// started or reserved, and the app is never opened from a sign-in.
    #[test]
    fn a_claude_desktop_sign_in_is_refused_before_anything_starts() {
        let m = machine("desktop-sign-in");
        let ctx = m
            .ctx
            .clone()
            .with_desktop_dir(m.ctx_home().join("claude-desktop").to_string_lossy().into())
            .with_desktop_app(m.ctx_home().join("Claude.app").to_string_lossy().into());
        let before = files(&m);

        let failed = Pitboard::new(ctx.clone())
            .sign_in("desktop/work")
            .err()
            .expect("there is no sign-in to run");
        assert_eq!(
            failed.error.code(),
            "sign_in_unsupported",
            "{}",
            failed.error
        );
        let watched = Pitboard::new(ctx)
            .sign_in_watched("desktop/work")
            .err()
            .expect("nor one to watch");
        assert_eq!(watched.error.code(), "sign_in_unsupported");

        assert_eq!(files(&m), before, "no sign-in is reserved");
        assert_eq!(
            last_audited(&m).last(),
            Some(&("enroll".to_string(), "sign_in_unsupported".to_string()))
        );
        assert!(!m.ctx_home().join("claude-desktop").exists());
    }

    /// A name is refused the same way whichever tool it is for.
    const MACHINES: [(&str, Make); 2] = [("claude", machine), ("codex", codex_machine)];

    type Refuse = fn(&Pitboard, ProviderId) -> Option<Failed>;

    /// Every way a change is refused over the name it was given, with the verb the audit
    /// log records it under, the code it is refused with and the one the log records: a
    /// name nobody enrolled, a name no account could have, whether the account signed in
    /// now is being enrolled or one is being signed in to, and a new name that would move
    /// an account to another tool.
    const REFUSALS: [(&str, &str, &str, &str, Refuse); 4] = [
        (
            "use",
            "use",
            "account_unknown",
            "account_unknown",
            |p, _| p.switch_to("nobody").err(),
        ),
        ("enroll", "enroll", "usage", "label_unusable", |p, _| {
            p.enroll_current("codx/work").err()
        }),
        ("sign-in", "enroll", "usage", "label_unusable", |p, _| {
            p.sign_in("codx/work").err()
        }),
        ("rename", "rename", "usage", "usage", |p, tool| {
            let other = if tool == ProviderId::Claude {
                ProviderId::Codex
            } else {
                ProviderId::Claude
            };
            p.rename(
                &Key::new(tool, "here").qualified(),
                &Key::new(other, "moved").qualified(),
            )
            .err()
        }),
    ];

    /// A switch from `here` to `there` killed after parking `here` and before installing
    /// `there`, so its record is all that says it happened.
    fn interrupted(make: Make, name: &str) -> Machine {
        let m = make(name);
        let settled = switch::settle(&m.ctx, None)
            .expect("nothing to recover yet")
            .0;
        let died = crate::fault::killing("switch.park_recorded", || {
            switch::switch(settled, &m.key("there"))
        });
        assert_eq!(died.unwrap_err(), "switch.park_recorded");
        assert!(switch::interrupted(&m.ctx));
        m
    }

    /// Every file in Pitboard's own directory but the audit log.
    fn files(m: &Machine) -> BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(crate::home::dir(&m.ctx))
            .expect("a Pitboard home")
            .map(|entry| entry.expect("an entry").path())
            .filter(|path| path.file_name() != Some("audit.log".as_ref()))
            .map(|path| {
                let body = std::fs::read(&path).unwrap_or_default();
                (path.display().to_string(), body)
            })
            .collect()
    }

    /// The last two lines of the audit log, as verb and outcome.
    fn last_audited(m: &Machine) -> Vec<(String, String)> {
        audit::read(&m.ctx, 2)
            .into_iter()
            .map(|entry| (entry.verb, entry.outcome))
            .collect()
    }

    /// Every other change settles an interrupted switch before anything else, and one
    /// refused over its name did not, so the switch stayed unrecovered and the refusal was
    /// all anybody was told. It is settled now, recorded the way any recovery is, and
    /// reported beside the refusal.
    #[test]
    fn a_change_refused_over_its_name_still_recovers_an_interrupted_switch() {
        for (tool, make) in MACHINES {
            for (change, verb, code, audited, refuse) in REFUSALS {
                let at = format!("{tool}, {change}");
                let m = interrupted(make, &format!("refused-{tool}-{change}"));

                let failed = refuse(&Pitboard::new(m.ctx.clone()), m.which)
                    .unwrap_or_else(|| panic!("{at}: the name must still be refused"));

                assert_eq!(failed.error.code(), code, "{at}: {}", failed.error);
                let said: Vec<&str> = failed.warnings.iter().map(Warning::code).collect();
                assert_eq!(said, ["interrupted_switch_undone"], "{at}");
                assert!(!switch::interrupted(&m.ctx), "{at}: the record is resolved");
                hold(&m, &at);
                assert_eq!(
                    last_audited(&m),
                    [
                        (
                            "recover".to_string(),
                            "interrupted_switch_undone".to_string()
                        ),
                        (verb.to_string(), audited.to_string()),
                    ],
                    "{at}"
                );
            }
        }
    }

    /// A mistyped name with nothing to recover takes no lock and writes nothing but its
    /// line in the audit log.
    #[test]
    fn a_change_refused_over_its_name_with_nothing_interrupted_changes_nothing() {
        for (tool, make) in MACHINES {
            for (change, verb, code, audited, refuse) in REFUSALS {
                let at = format!("{tool}, {change}");
                let m = make(&format!("refused-quietly-{tool}-{change}"));
                let (before, parked, live) = (files(&m), m.mem.vault().services(), m.live());

                let failed = refuse(&Pitboard::new(m.ctx.clone()), m.which)
                    .unwrap_or_else(|| panic!("{at}: the name must still be refused"));

                assert_eq!(failed.error.code(), code, "{at}: {}", failed.error);
                assert!(failed.warnings.is_empty(), "{at}: {:?}", failed.warnings);
                assert_eq!(files(&m), before, "{at}: not even the lock file is made");
                assert_eq!(m.mem.vault().services(), parked, "{at}");
                assert_eq!(m.live(), live, "{at}");
                assert_eq!(
                    last_audited(&m).last(),
                    Some(&(verb.to_string(), audited.to_string())),
                    "{at}"
                );
            }
        }
    }

    /// A new login that did not hold after it was written is parked rather than lost, and
    /// parking a login too big for `security`'s standard input puts it on the argument line.
    /// The change then fails, and that is still said beside the failure.
    #[test]
    fn a_new_login_parked_after_it_did_not_hold_says_how_it_was_parked() {
        for (tool, make) in MACHINES {
            let m = make(&format!("not-installed-said-{tool}"));
            m.mem.vault().takes_on_stdin(64);
            let login = crate::switch::harness::signed_in(&m, "here", "here-refresh-2");
            m.fault_live(crate::store::memory::Fault::DeletedAfterWrite);

            let failed = Pitboard::new(m.ctx.clone())
                .enroll_signed_in(&m.key("here").typed(), login)
                .expect_err("it did not hold");

            assert_eq!(failed.error.code(), "sign_in_not_installed", "{tool}");
            let said: Vec<&str> = failed.warnings.iter().map(Warning::code).collect();
            assert_eq!(said, ["written_on_the_command_line"], "{tool}");
        }
    }

    /// The recovery settles for the tool whose switch was interrupted, not for no tool in
    /// particular, so a custom Claude Code endpoint stops exactly what it stops for any
    /// change: the recovery of a Claude Code switch, and not of a Codex one. What it stops
    /// is left for a later run, and the refusal is reported as it always was.
    #[test]
    fn a_custom_claude_endpoint_stops_only_the_recovery_of_a_claude_code_switch() {
        let codex = interrupted(codex_machine, "refused-custom-codex");
        let mut ctx = codex.ctx.clone();
        ctx.custom_oauth = true;
        let failed = Pitboard::new(ctx).switch_to("nobody").expect_err("refused");
        assert_eq!(failed.error.code(), "account_unknown");
        let said: Vec<&str> = failed.warnings.iter().map(Warning::code).collect();
        assert_eq!(said, ["interrupted_switch_undone"]);
        assert!(!switch::interrupted(&codex.ctx));

        let claude = interrupted(machine, "refused-custom-claude");
        let mut ctx = claude.ctx.clone();
        ctx.custom_oauth = true;
        let failed = Pitboard::new(ctx).switch_to("nobody").expect_err("refused");
        assert_eq!(
            failed.error.code(),
            "account_unknown",
            "the refusal, not the recovery that could not run"
        );
        assert!(failed.warnings.is_empty(), "{:?}", failed.warnings);
        assert!(
            switch::interrupted(&claude.ctx),
            "the record is kept for a run that can finish it"
        );
    }

    /// An unfinished Claude Desktop switch, while Claude is open, is said to be waiting by a
    /// change about no tool that goes ahead, and stops one that cannot, said once.
    #[test]
    fn a_desktop_switch_waiting_for_claude_is_reported() {
        use crate::switch::harness::{APP_PATH, desktop_machine};
        let m = desktop_machine("service-waiting");
        assert_eq!(
            m.crash_at("tree.item_parked").unwrap_err(),
            "tree.item_parked"
        );
        m.mem.runs_within(APP_PATH);
        let pitboard = Pitboard::new(m.ctx.clone());

        let done = pitboard.repair().expect("goes ahead");
        let codes: Vec<_> = done.warnings.iter().map(Warning::code).collect();
        assert_eq!(codes, ["recovery_waiting"]);

        let failed = pitboard.uninstall().err().expect("refused");
        assert_eq!(failed.error.code(), "recovery_waiting");
        assert!(
            failed.warnings.is_empty(),
            "the refusal says it: {:?}",
            failed.warnings
        );
    }

    /// A Claude Desktop switch that fails partway keeps its record, and the failure says so:
    /// whoever quit Claude for it must keep Claude closed until the next change finishes or
    /// undoes it, rather than open it on half of each account. A refusal that already says
    /// it stopped partway is not said twice.
    #[test]
    fn a_desktop_switch_that_fails_partway_says_it_is_unfinished() {
        use crate::switch::harness::{APP_PATH, desktop_machine};
        use std::os::unix::fs::PermissionsExt;
        let m = desktop_machine("service-unfinished");
        let pitboard = Pitboard::new(m.ctx.clone());
        let support = m.support();
        let locked = support.clone();
        let failed = crate::fault::meanwhile(
            "tree.item_parked",
            move || {
                std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500))
                    .expect("lock the data folder");
            },
            || pitboard.switch_to("desktop/there"),
        )
        .expect_err("the next move fails");
        std::fs::set_permissions(&support, std::fs::Permissions::from_mode(0o700))
            .expect("unlock the data folder");
        assert!(
            switch::tree_interrupted(&m.ctx).is_some(),
            "the record is kept"
        );
        let unfinished: Vec<_> = failed
            .warnings
            .iter()
            .filter(|w| w.code() == "switch_unfinished")
            .collect();
        assert_eq!(unfinished.len(), 1, "{:?}", failed.warnings);
        let said = unfinished[0].to_string();
        assert!(
            said.contains("desktop/here") && said.contains("desktop/there"),
            "{said}"
        );
        assert!(said.contains("Claude"), "{said}");

        // Claude opening partway says the same in its own refusal.
        m.recover().expect("undone").expect("found");
        let mem = std::sync::Arc::clone(&m.mem);
        let pitboard = Pitboard::new(m.ctx.clone());
        let opened = crate::fault::meanwhile(
            "tree.item_parked",
            move || {
                mem.runs_within(APP_PATH);
            },
            || pitboard.switch_to("desktop/there"),
        )
        .expect_err("the app opened partway");
        assert_eq!(opened.error.code(), "app_opened_midway");
        assert!(
            opened
                .warnings
                .iter()
                .all(|w| w.code() != "switch_unfinished"),
            "{:?}",
            opened.warnings
        );
    }

    /// Status reads and never moves, so a Claude Desktop switch a crash left while Claude is
    /// open is said to be waiting, by both reports, and its record is kept for a change.
    #[test]
    fn status_says_a_desktop_switch_waits_while_claude_is_open() {
        use crate::switch::harness::APP_PATH;
        let (m, before) = interrupted_desktop("status-waiting");
        m.mem.runs_within(APP_PATH);
        let ctx = m
            .ctx
            .clone()
            .with_scripted_api(crate::api::scripted::ScriptedApi::new())
            .with_scripted_safe_storage(crate::api::scripted::ScriptedSafeStorage::forbidding());
        let pitboard = Pitboard::new(ctx);

        let reports = [
            ("status", pitboard.status(false).expect("a report")),
            ("offline", pitboard.status_offline().expect("a report")),
        ];
        for (how, done) in reports {
            assert!(
                matches!(
                    done.warnings.as_slice(),
                    [Warning::RecoveryWaiting { tool: ProviderId::Desktop, from, to }]
                        if from == "desktop/here" && to == "desktop/there"
                ),
                "{how}: {:?}",
                done.warnings
            );
        }
        assert_eq!(m.inodes(), before, "nothing moved");
        assert_eq!(
            switch::tree_interrupted(&m.ctx),
            Some(("desktop/here".to_string(), "desktop/there".to_string())),
            "the record is kept"
        );
    }

    /// A Claude Desktop machine whose switch to `there` died with `here`'s first item parked,
    /// and every file it holds by inode.
    fn interrupted_desktop(
        name: &str,
    ) -> (
        crate::switch::harness::DesktopMachine,
        BTreeMap<std::path::PathBuf, u64>,
    ) {
        let m = crate::switch::harness::desktop_machine(name);
        assert_eq!(
            m.crash_at("tree.item_parked").unwrap_err(),
            "tree.item_parked"
        );
        assert!(switch::tree_interrupted(&m.ctx).is_some());
        let inodes = m.inodes();
        (m, inodes)
    }

    /// A change to Claude Code or Codex refused over its name leaves an interrupted Claude
    /// Desktop switch alone, as the change itself would have: nothing in the data folder or
    /// a park moves, and the record stays for a change to the app.
    #[test]
    fn a_change_to_another_tool_refused_over_its_name_leaves_a_desktop_switch_alone() {
        type Attempt = fn(&Pitboard) -> Option<Failed>;
        let attempts: [(&str, Attempt); 6] = [
            ("use codex", |p| p.switch_to("codex/typo").err()),
            ("use claude", |p| p.switch_to("claude/typo").err()),
            ("forget codex", |p| p.forget("codex/typo").err()),
            ("enroll codex", |p| p.enroll_current("codex/").err()),
            // A bare new name is Claude Code's.
            ("enroll bare", |p| p.enroll_current("").err()),
            ("rename codex", |p| p.rename("codex/typo", "other").err()),
        ];
        for (at, attempt) in attempts {
            let (m, before) = interrupted_desktop(&format!("refused-other-{}", at.len()));
            let failed = attempt(&Pitboard::new(m.ctx.clone()))
                .unwrap_or_else(|| panic!("{at}: the name must still be refused"));
            assert!(failed.warnings.is_empty(), "{at}: {:?}", failed.warnings);
            assert_eq!(m.inodes(), before, "{at}: nothing moved");
            assert!(
                switch::tree_interrupted(&m.ctx).is_some(),
                "{at}: the record is kept for a change to the app"
            );
        }
    }

    /// A change to Claude Desktop refused over its name settles the app's interrupted switch
    /// the way the change would have, and so does one whose name says no tool.
    #[test]
    fn a_change_to_the_app_refused_over_its_name_settles_its_switch() {
        for typed in ["desktop/typo", "typo"] {
            let (m, _) = interrupted_desktop(&format!("refused-desktop-{}", typed.len()));
            let failed = Pitboard::new(m.ctx.clone())
                .switch_to(typed)
                .expect_err("refused");
            assert_eq!(failed.error.code(), "account_unknown", "{typed}");
            let said: Vec<&str> = failed.warnings.iter().map(Warning::code).collect();
            assert_eq!(said, ["interrupted_switch_undone"], "{typed}");
            assert!(switch::tree_interrupted(&m.ctx).is_none(), "{typed}");
        }
    }

    /// While Claude is open, a change to it refused over its name moves nothing and says the
    /// interrupted switch waits, rather than saying nothing.
    #[test]
    fn a_change_to_the_app_refused_while_it_is_open_says_its_switch_waits() {
        use crate::switch::harness::APP_PATH;
        for typed in ["desktop/typo", "typo"] {
            let (m, before) = interrupted_desktop(&format!("refused-open-{}", typed.len()));
            m.mem.runs_within(APP_PATH);
            let failed = Pitboard::new(m.ctx.clone())
                .switch_to(typed)
                .expect_err("refused");
            assert_eq!(failed.error.code(), "account_unknown", "{typed}");
            let said: Vec<&str> = failed.warnings.iter().map(Warning::code).collect();
            assert_eq!(said, ["recovery_waiting"], "{typed}");
            assert_eq!(m.inodes(), before, "{typed}: nothing moved");
            assert!(switch::tree_interrupted(&m.ctx).is_some(), "{typed}");
        }
    }

    /// The audit log names the account an interrupted sign-out was signing out.
    #[test]
    fn a_recovered_sign_out_is_logged_with_its_account() {
        use crate::switch::harness::desktop_machine;
        let m = desktop_machine("service-sign-out");
        let killed = crate::fault::killing("tree.config_spliced", || {
            let (settled, _) = switch::settle(&m.ctx, Some(ProviderId::Desktop))?;
            switch::sign_out(settled, ProviderId::Desktop)
        });
        assert_eq!(killed.unwrap_err(), "tree.config_spliced");

        let done = Pitboard::new(m.ctx.clone()).repair().expect("repaired");
        assert!(
            matches!(done.warnings.as_slice(), [Warning::Recovered(r)] if r.signed_out),
            "{:?}",
            done.warnings
        );
        let recovered = audit::read(&m.ctx, 10)
            .into_iter()
            .find(|e| e.verb == "recover")
            .expect("logged");
        assert_eq!(recovered.subject, "desktop/here -> signed out");
        assert_eq!(recovered.outcome, "interrupted_switch_finished");
    }

    /// A Claude Desktop machine whose keychain panics at any read: nothing here may ask for
    /// Claude's key.
    fn quiet_desktop(name: &str) -> (crate::switch::harness::DesktopMachine, Pitboard) {
        let m = crate::switch::harness::desktop_machine(name);
        let ctx = m
            .ctx
            .clone()
            .with_scripted_safe_storage(crate::api::scripted::ScriptedSafeStorage::forbidding());
        (m, Pitboard::new(ctx))
    }

    /// Adding a second account to the app starts by signing it out: the account in it is
    /// parked, and Pitboard remembers that it waits for somebody to sign in and enrol.
    #[test]
    fn signing_claude_desktop_out_parks_its_account_and_waits_for_a_sign_in() {
        let (m, pitboard) = quiet_desktop("service-signed-out");
        let done = pitboard
            .switch_to_signed_out(ProviderId::Desktop)
            .expect("signed out");
        assert!(
            matches!(&done.value, Outcome::SignedOut { from, .. } if from == "desktop/here"),
            "{:?}",
            done.value
        );
        let awaiting = pitboard.awaiting_sign_in().expect("waiting for a sign-in");
        assert_eq!(awaiting.from_label, "here");
        let logged = audit::read(&m.ctx, 1).pop().expect("logged");
        assert_eq!(
            (
                logged.verb.as_str(),
                logged.subject.as_str(),
                logged.outcome.as_str()
            ),
            ("use", "desktop -> signed out", "ok")
        );
    }

    /// Only an app can sign itself out this way; a tool whose login is one document is
    /// refused before anything is settled or moved.
    #[test]
    fn only_a_folder_login_is_signed_out() {
        let m = machine("service-signed-out-claude");
        let Err(failed) = Pitboard::new(m.ctx.clone()).switch_to_signed_out(ProviderId::Claude)
        else {
            panic!("refused");
        };
        assert_eq!(failed.error.code(), "usage", "{}", failed.error);
    }

    /// A Claude Desktop park is renewed by nobody but the app, so `renew` says so for each,
    /// with when its session runs out, and changes nothing.
    #[test]
    fn renew_says_each_claude_desktop_park_is_not_renewable() {
        let (m, pitboard) = quiet_desktop("service-renew");
        let state_file = crate::home::dir(&m.ctx).join("state.json");
        let before = std::fs::read(&state_file).expect("accounts");
        let expiry = crate::state::load(&m.ctx)
            .expect("accounts")
            .get(&m.key("there"))
            .and_then(|a| a.parked.as_ref())
            .and_then(|p| p.refresh_expires_at);
        assert!(expiry.is_some());
        let outcomes = pitboard.renew();
        assert_eq!(outcomes.len(), 1, "{outcomes:?}");
        let (key, outcome) = &outcomes[0];
        assert_eq!(key, &m.key("there"));
        assert!(
            matches!(outcome, Renewal::NotRenewable { label, expires_at }
                if label == "desktop/there" && *expires_at == expiry),
            "{outcome:?}"
        );
        assert_eq!(std::fs::read(&state_file).expect("accounts"), before);
    }

    /// Live usage is off until somebody turns it on, and turning it off reads nothing.
    #[test]
    fn live_usage_is_off_until_turned_on_and_turning_it_off_reads_nothing() {
        let (_m, pitboard) = quiet_desktop("service-live-usage");
        assert!(!pitboard.live_usage().enabled);
        let off = pitboard.disable_live_usage().expect("turned off");
        assert!(!off.enabled);
        assert_eq!(pitboard.live_usage(), off);
    }

    /// What is running Claude Desktop is found by the app's own bundle, so a front end can
    /// ask for it to be quit before a switch rather than after.
    #[test]
    fn whatever_runs_claude_desktop_is_found_before_a_switch() {
        use crate::switch::harness::APP_PATH;
        let (m, pitboard) = quiet_desktop("service-holding");
        assert!(pitboard.holding(ProviderId::Desktop).is_empty());
        let pid = m.mem.runs_within(APP_PATH);
        let holding = pitboard.holding(ProviderId::Desktop);
        assert_eq!(
            holding
                .iter()
                .flat_map(|h| h.pids.clone())
                .collect::<Vec<_>>(),
            [pid]
        );
    }
}
