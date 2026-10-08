//! Pitboard's operations, each run the way every front end must run it: a change settles any
//! interrupted switch first and is recorded in the audit log, and what went wrong on the way
//! is reported alongside the result, whether or not the change then succeeds.

use crate::context::Context;
use crate::doctor::{self, Diagnosis};
use crate::error::{Error, Result};
use crate::holder::{self, capitalised};
use crate::host::{Elevation, Floor};
use crate::provider::{Held, ProviderId};
use crate::state::{self, Account, Key};
use crate::switch::{self, Enrolled, Outcome, Recovered, Renewal, Settled, SignIn};
use crate::{audit, readings, schedule, status, statusline};
use std::fmt;

/// Proof that this process may change something on this machine, which only the one gate
/// every change passes makes ([`Pitboard::permit`]).
///
/// Everything that changes anything takes one as an argument: the durable write of a file,
/// every private file or directory made and every one removed, a write to or a delete from
/// any store of logins, the keychain among them, a change to the system's scheduler, a
/// tool's sign-in started, and the token exchange that renews a parked login. So a change
/// that did not ask the gate does not compile, and nothing rotates a refresh token it could
/// not then write down.
#[derive(Debug, Clone, Copy)]
pub struct Permit {
    _only_the_gate_makes_one: (),
}

#[cfg(test)]
impl Permit {
    /// One for a unit test of what takes it, below the gate. A test of the gate itself, or
    /// of a change through [`Pitboard`], asks the gate.
    pub(crate) fn for_a_test() -> Permit {
        Permit {
            _only_the_gate_makes_one: (),
        }
    }
}

/// The one gate every change passes: whether this process may change anything here, as the
/// host says. A process that runs as root or under sudo, or elevated on Windows, changes
/// nothing, and nor does one the host cannot place: a file it wrote would be root's or the
/// administrators', and a keychain item might be, where the person's own runs might never
/// read, replace or remove it again.
///
/// Nor does one whose environment names a home that is empty or relative
/// ([`crate::home::check_absolute`]): what it wrote would land under whichever folder it
/// was run from. Asked after elevation, so `sudo` is what a run under it is told first.
///
/// Before those, a build that may do nothing at all changes nothing: a Windows build of a
/// release made before Pitboard for Windows is released ([`crate::release`]). So no writer
/// and no token exchange runs there, even for a caller that forgot to ask. Then a Windows
/// older than 11 24H2 ([`Floor`]): asked before elevation, which cannot change it.
pub(crate) fn gate(ctx: &Context) -> Result<Permit> {
    crate::release::check()?;
    match ctx.host().floor() {
        Floor::Met => {}
        Floor::Below { build } => return Err(Error::SystemTooOld { build: Some(build) }),
        Floor::Unknown => return Err(Error::SystemTooOld { build: None }),
    }
    match ctx.host().elevation(ctx) {
        Elevation::Normal => {}
        Elevation::Elevated { why } => return Err(Error::Elevated { why: Some(why) }),
        Elevation::Unknown => return Err(Error::Elevated { why: None }),
    }
    crate::home::check_absolute(ctx)?;
    Ok(Permit {
        _only_the_gate_makes_one: (),
    })
}

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
    /// A file of the tool's sits behind the store in use. While it is there a session
    /// already running keeps its account after a switch until its login is next renewed.
    /// Where it holds a login, a session that cannot read that store signs in with it, and
    /// no switch reaches it.
    FallbackLogin {
        tool: ProviderId,
        path: std::path::PathBuf,
        held: Held,
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
    /// A switch of a tool that never follows one on its own, where the process list could
    /// not be read: nobody can say whether anything of the tool started before it is still
    /// running, and still using `from`. Not the same as nothing running, which says nothing.
    SessionsUnknown {
        tool: ProviderId,
        from: String,
    },
    /// A sign-in that put a new login of `label` in use, of a tool that never reads its login
    /// again, where the process list could not be read: nobody can say whether anything of
    /// the tool is still running with the old login.
    SessionsUnknownAfterSignIn {
        tool: ProviderId,
        label: String,
    },
    /// A sign-in to the account Pitboard last recorded in use was parked rather than put in
    /// use, because nobody could say whose login the tool has in use, for `why`.
    SignInParkedNotInUse {
        tool: ProviderId,
        label: String,
        why: String,
    },
    /// An interrupted switch is waiting that the next change cannot finish, and every change
    /// stops at it until it can be finished or is given up on. The refusal that change would
    /// make, said by a read so that nobody has to make a change to find out.
    SwitchStuck(Error),
    /// A sign-in outside Pitboard replaced the only login of `label`, and Pitboard holds none
    /// for it: said by every read until it is signed in again, put in use again or forgotten.
    /// `now` is whose login the tool has stored instead, as last recorded, which every switch
    /// changes. `id` is the account's, which stays while the words do not.
    LoginReplaced {
        tool: ProviderId,
        id: String,
        label: String,
        now: Stored,
    },
    /// This process may change nothing, because it runs as root or under sudo, or elevated on
    /// Windows, or nobody could tell whether it does, so a read answers from what Pitboard
    /// last measured: it renews nothing, asks nobody and writes nothing. `why` is how it runs,
    /// as the host said it, and `None` where the host could not say.
    ReadOnly {
        why: Option<&'static str>,
    },
    ReadOnlyBelowTheFloor {
        build: Option<u32>,
    },
}

impl Warning {
    fn read_only(refused: &Error) -> Option<Warning> {
        match *refused {
            Error::Elevated { why } => Some(Warning::ReadOnly { why }),
            Error::SystemTooOld { build } => Some(Warning::ReadOnlyBelowTheFloor { build }),
            _ => None,
        }
    }

    /// Stable, for a program to branch on.
    pub fn code(&self) -> &'static str {
        match self {
            Warning::Recovered(r) => r.code(),
            Warning::LockCompromised { .. } => "lock_compromised",
            Warning::ConfigNotUpdated(e) | Warning::RenewalFailed(e) | Warning::SwitchStuck(e) => {
                e.code()
            }
            Warning::ParksPendingRemoval(_) => "parks_pending_removal",
            Warning::ParkedLoginRefused { .. } => "parked_login_refused",
            Warning::AuthOverridden { .. } => "auth_overridden",
            Warning::FallbackLogin { .. } => "fallback_login",
            Warning::WrittenOnTheCommandLine { .. } => "written_on_the_command_line",
            Warning::SessionsStillRunning { .. } => "sessions_still_running",
            Warning::SessionsKeepTheOldLogin { .. } => "sessions_keep_old_login",
            Warning::SessionsUnknown { .. } | Warning::SessionsUnknownAfterSignIn { .. } => {
                "sessions_unknown"
            }
            Warning::SignInParkedNotInUse { .. } => "sign_in_parked_not_in_use",
            Warning::LoginReplaced { .. } => "login_replaced",
            Warning::ReadOnly { .. } | Warning::ReadOnlyBelowTheFloor { .. } => "read_only",
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
            Warning::ConfigNotUpdated(e) | Warning::RenewalFailed(e) | Warning::SwitchStuck(e) => {
                write!(f, "{e}")
            }
            Warning::ParksPendingRemoval(count) => write!(
                f,
                "{count} parked login(s) no longer in use could not be removed yet; Pitboard \
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
                "{} is set, so {} signs in with it and not with the login Pitboard moved. \
                 Unset it for the switch to take effect.",
                names.join(" and "),
                tool.name()
            ),
            Warning::FallbackLogin {
                tool,
                path,
                held: Held::Login,
            } => write!(
                f,
                "{path} holds another {tool} login, which a session that cannot read the \
                 keychain, such as one started over SSH, signs in with. No switch reaches it. \
                 While it is there, {tool} sessions already running at a switch {running}. \
                 `pitboard doctor` says what to do.",
                path = path.display(),
                tool = tool.name(),
                running = crate::words::kept_until_renewed(),
            ),
            Warning::FallbackLogin {
                tool,
                path,
                held: Held::NoLogin,
            } => write!(
                f,
                "{path} is there with no {tool} login in it. While it is, {tool} sessions \
                 already running at a switch {running}. Deleting it lets them follow a switch: \
                 `rm {path}`.",
                path = path.display(),
                tool = tool.name(),
                running = crate::words::kept_until_renewed(),
            ),
            Warning::FallbackLogin {
                tool,
                path,
                held: Held::Unreadable,
            } => write!(
                f,
                "{path} is there, and Pitboard could not read it. While it is, {tool} sessions \
                 already running at a switch {running}. `pitboard doctor` says why, and what to \
                 do.",
                path = path.display(),
                tool = tool.name(),
                running = crate::words::kept_until_renewed(),
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
            Warning::SessionsUnknown { tool, from } => write!(
                f,
                "Pitboard could not tell whether {} sessions started before this switch are \
                 still running, because it could not read the list of processes. Do not sign \
                 out in one that is: signing out there revokes `{from}`'s login, which \
                 Pitboard has just parked.",
                tool.name()
            ),
            Warning::SessionsUnknownAfterSignIn { tool, label } => write!(
                f,
                "Pitboard could not tell whether {} sessions started before this sign-in are \
                 still running, because it could not read the list of processes. Quit any \
                 that are still using `{label}`'s old login and start them again. Otherwise \
                 one of them can put the old login back in place of the new one when it \
                 refreshes its token.",
                tool.name()
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
            Warning::LoginReplaced {
                tool, label, now, ..
            } => {
                write!(
                    f,
                    "`{label}`'s login was replaced by a sign-in outside Pitboard"
                )?;
                let tool = tool.name();
                match now {
                    Stored::Account(now) => write!(f, ": {tool} now has `{now}`'s login stored,")?,
                    Stored::Unenrolled(email) => {
                        write!(f, ": {tool} now has {email}'s login stored,")?;
                    }
                    Stored::Nothing => write!(f, ": {tool} now has no login stored,")?,
                    Stored::Unknown => write!(f, ",")?,
                }
                write!(
                    f,
                    " and Pitboard holds no login for `{label}`. Run `pitboard enroll {label} \
                     --sign-in` to sign in to it again."
                )
            }
            Warning::ReadOnly { why } => {
                read_only(f, &crate::words::elevated(crate::host::OS, *why))
            }
            Warning::ReadOnlyBelowTheFloor { build } => {
                read_only(f, &crate::words::too_old(*build))
            }
        }
    }
}

/// Whose login a tool has stored, as Pitboard last recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    /// An enrolled account's, by the name to type for it.
    Account(String),
    /// The login of an account nobody enrolled, by its email.
    Unenrolled(String),
    /// None.
    Nothing,
    /// Nothing is recorded of the tool's store here.
    Unknown,
}

/// What a read says for as long as it stands, from Pitboard's own record alone: each account
/// whose only login a sign-in outside Pitboard replaced.
fn standing(state: &state::State) -> Vec<Warning> {
    state
        .accounts
        .iter()
        .filter(|account| account.replaced_at.is_some() && account.parked.is_none())
        .map(|account| {
            let tool = account.provider();
            let now = match state.in_use(tool) {
                None => Stored::Unknown,
                Some(record) => match (&record.owner, state.account_in_use(tool)) {
                    (None, _) => Stored::Nothing,
                    (Some(_), Some(now)) => Stored::Account(state.typed(&now.key())),
                    (Some(owner), None) => Stored::Unenrolled(owner.email.clone()),
                },
            };
            Warning::LoginReplaced {
                tool,
                id: account.id.clone(),
                label: state.typed(&account.key()),
                now,
            }
        })
        .collect()
}

fn read_only(f: &mut fmt::Formatter<'_>, said: &crate::words::ChangesNothing) -> fmt::Result {
    write!(
        f,
        "{}, so it changes nothing: these are the numbers it last measured, and it renews no \
         parked login and asks nobody. {}",
        said.because, said.to_ask_again
    )
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

    /// Whether this process may change anything here, as the one gate every change passes
    /// says, with the proof it hands out where it may. Every change below asks it first,
    /// before it reads, locks or records anything, so a change refused here leaves the
    /// Pitboard directory, the stores of logins and the scheduler exactly as they were, the
    /// audit log included. A front end asks it too before it writes a file of its own
    /// through [`crate::app::write_file`], or before it asks a question whose answer could
    /// only be refused.
    pub fn permit(&self) -> Result<Permit> {
        gate(&self.ctx)
    }

    /// Whether every home the environment names is a full path, as the gate and every read
    /// of the accounts ask. A front end asks it before anything else that reads under one,
    /// such as the audit log, the schedule's file or the status line's session records, so
    /// a home that is empty or relative is refused there too, never read as the folder the
    /// front end runs in.
    pub fn check_homes(&self) -> Result<()> {
        crate::home::check_absolute(&self.ctx)
    }

    /// The gate's answer as the refusal of a change, which has found nothing on the way.
    fn permitted(&self) -> std::result::Result<Permit, Failed> {
        self.permit().map_err(|error| Failed {
            error,
            warnings: Vec::new(),
        })
    }

    /// Who is signed in and what every account has left. Parked logins whose access has
    /// lapsed are renewed first, so every account is asked live.
    ///
    /// `fresh` asks Anthropic about every account whatever was asked recently. Ordinarily
    /// false: a number is only asked for again once the tightest limit it describes could
    /// have moved by a percentage point, which collapses several front ends on one machine
    /// to one request per account per few minutes.
    ///
    /// An interrupted switch the next change cannot finish is said as that change would
    /// refuse over it, and nothing is done about it: every change stops at it until it can
    /// be finished or is given up on, and a front end that waited for a change to be
    /// refused could not offer to give up on it until then. One the next change finishes by
    /// itself is not said. Nobody has anything to do about it: the rows already say who is
    /// signed in from each tool's login rather than from Pitboard's record, and the change
    /// that finishes it says what it found.
    ///
    /// A file behind a tool's store is said as a change says it, since while it is there
    /// every switch reaches running sessions only at their login's next renewal, and a read
    /// is where somebody looks before they switch.
    ///
    /// Whose login each tool has stored is recorded where the read found it changed, as
    /// [`switch::identify::record_read`] says, before the report is made, so a sign-in outside
    /// Pitboard that replaced the only login of an account is said from that read on.
    ///
    /// Where this process may change nothing, it answers what [`status_offline`] answers,
    /// with a `read_only` warning that says why: renewing a parked login rotates its refresh
    /// token, a live read records what it measured, and neither may happen then.
    ///
    /// [`status_offline`]: Pitboard::status_offline
    pub fn status(&self, fresh: bool) -> Result<Done<status::Report>> {
        let permit = match self.permit() {
            Ok(permit) => permit,
            Err(refused) => {
                let Some(warning) = Warning::read_only(&refused) else {
                    return Err(refused);
                };
                let mut read = self.status_offline()?;
                read.warnings.push(warning);
                return Ok(read);
            }
        };
        let renewed = switch::renew_parked(&self.ctx, permit);
        // Unreadable is not the same as empty: reporting it as empty would say the enrolled
        // logins are gone.
        let state = state::load(&self.ctx)?;
        // Alongside the answers rather than before them. Where only the service can say
        // whose the live login is, both ask it, and asked one after the other, a service that
        // did not answer held the read for its timeout twice.
        let (stuck, answers) = std::thread::scope(|scope| {
            let stuck = scope.spawn(|| switch::stuck(&self.ctx, &state, switch::Asking::Service));
            let answers = status::ask(&self.ctx, &state, fresh);
            (stuck.join().ok().flatten(), answers)
        });
        switch::identify::record_read(&self.ctx, permit, &answers.found);
        // Again, so the rows and what stands are of whose login each tool has stored as it
        // was just recorded.
        let state = state::load(&self.ctx)?;
        let report = status::report(&self.ctx, permit, &state, answers);
        let mut warnings: Vec<Warning> = stuck.map(Warning::SwitchStuck).into_iter().collect();
        for (key, outcome) in renewed {
            audit::record(&self.ctx, permit, "renew", &key.typed(), outcome.code());
            match outcome {
                Renewal::Refused => warnings.push(Warning::ParkedLoginRefused {
                    tool: key.provider,
                    label: state.typed(&key),
                }),
                Renewal::Failed(e) => warnings.push(Warning::RenewalFailed(e)),
                Renewal::Renewed | Renewal::Deferred => {}
            }
        }
        warnings.extend(
            ProviderId::ALL
                .iter()
                .filter_map(|&tool| self.left_behind(tool)),
        );
        warnings.extend(standing(&state));
        Ok(Done {
            value: report,
            warnings,
        })
    }

    pub fn doctor(&self) -> Diagnosis {
        doctor::run(&self.ctx)
    }

    /// The same report without asking anyone: the last numbers Pitboard measured, and who
    /// each tool's own files say is signed in. Nothing is renewed and nothing is asked, so
    /// it answers at once wherever there is no network.
    ///
    /// An interrupted switch the next change cannot finish is said here too, as [`status`]
    /// says it, wherever that can be told without a request: a Claude Code login renewed
    /// since the switch stopped is one only Anthropic can say whose it is, and then nothing
    /// is said. Telling reads the tool's login and the copy the switch parked, which on
    /// macOS are keychain items, and only while a switch is waiting: otherwise one look at
    /// whether its record is there is the whole of it. So nothing is said of a file behind
    /// the keychain, which only a read of the keychain can tell. What Pitboard's own record
    /// holds is said as [`status`] says it: an account whose only login was replaced outside
    /// Pitboard.
    ///
    /// [`status`]: Pitboard::status
    pub fn status_offline(&self) -> Result<Done<status::Report>> {
        let state = state::load(&self.ctx)?;
        let warnings = switch::stuck(&self.ctx, &state, switch::Asking::Nobody)
            .map(Warning::SwitchStuck)
            .into_iter()
            .chain(standing(&state))
            .collect();
        Ok(Done {
            value: status::gather_offline(&self.ctx, &state),
            warnings,
        })
    }

    /// The status line for Claude Code's session JSON. Reads only files, and writes only
    /// Pitboard's own: what the session passed, for its next run to compare with, and the
    /// usage readings, which take what moved since its last run where it moves a limit
    /// Anthropic gave that account. Where this process may change nothing it writes neither,
    /// and draws the line from the files as they are.
    pub fn statusline(&self, session: &str) -> statusline::StatusLine {
        statusline::read(&self.ctx, self.permit().ok(), session)
    }

    /// The enrolled account under `typed`, if any, read without taking the lock.
    pub fn account(&self, typed: &str) -> Option<Account> {
        let state = state::load(&self.ctx).ok()?;
        crate::label::resolve(&state, typed).ok().cloned()
    }

    pub fn switch_to(&self, typed: &str) -> Changing<Outcome> {
        let permit = self.permitted()?;
        let key = self.named(permit, "use", typed)?;
        self.changing(permit, "use", &key.typed(), Some(key.provider), |settled| {
            switch::switch(settled, &key)
        })
    }

    /// Switches Claude Code by itself where a limit of the account in use has reached
    /// `threshold` and another account has room, as [`crate::autoswitch`] says. For a front
    /// end somebody asked to do that: the app with its setting on, or `pitboard watch`.
    ///
    /// Looks first from files alone, so a look that finds nothing to do takes no lock, asks
    /// nobody and records nothing. A switch is decided again under the lock, and is recorded
    /// in the audit log as `auto-switch`, with what it came to; nothing else here is.
    pub fn auto_switch(
        &self,
        threshold: crate::autoswitch::Threshold,
    ) -> Changing<crate::autoswitch::Auto> {
        let permit = self.permitted()?;
        let state = state::load(&self.ctx).map_err(|error| Failed {
            error,
            warnings: Vec::new(),
        })?;
        let plan = match crate::autoswitch::look(&self.ctx, &state, threshold) {
            crate::autoswitch::Next::Say(auto) => {
                return Ok(Done {
                    value: auto,
                    warnings: Vec::new(),
                });
            }
            crate::autoswitch::Next::Switch(plan) => plan,
        };
        let subject = state.typed(&plan.to);
        self.changing(
            permit,
            "auto-switch",
            &subject,
            Some(ProviderId::Claude),
            |settled| switch::automatically(settled, &plan, threshold),
        )
    }

    /// Which account somebody meant, as the key the engine looks accounts up by.
    ///
    /// Resolving here rather than deeper down means every command takes `codex/work` and
    /// a bare `work` on the same terms, and the one place that decides what an ambiguous
    /// bare label does is the one place that knows every provider's accounts.
    fn named(&self, permit: Permit, verb: &str, typed: &str) -> std::result::Result<Key, Failed> {
        let state = state::load(&self.ctx).map_err(|error| Failed {
            error,
            warnings: Vec::new(),
        })?;
        crate::label::resolve(&state, typed)
            .map(Account::key)
            .map_err(|error| self.refused(permit, verb, typed, error.code(), error))
    }

    /// `typed` may name a tool, as in `claude/work`. A bare name means the default tool.
    pub fn enroll_current(&self, typed: &str) -> Changing<Enrolled> {
        let permit = self.permitted()?;
        let key = self.chosen(permit, "enroll", typed)?;
        self.changing(
            permit,
            "enroll",
            &key.typed(),
            Some(key.provider),
            |settled| switch::enroll(settled, &key, None),
        )
    }

    /// Which tool a new account is for, and what it is called there.
    ///
    /// Split here rather than deeper down so nothing below ever sees a name with a tool
    /// still stuck to the front of it, which would enrol an account literally called
    /// `claude/work`.
    fn chosen(&self, permit: Permit, verb: &str, typed: &str) -> std::result::Result<Key, Failed> {
        self.enrolling(typed)
            .map_err(|error| self.refused(permit, verb, typed, "label_unusable", error))
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
    fn refused(
        &self,
        permit: Permit,
        verb: &str,
        subject: &str,
        code: &str,
        error: Error,
    ) -> Failed {
        let recovered = if switch::interrupted(&self.ctx) {
            switch::settle(&self.ctx, permit, switch::interrupted_tool(&self.ctx))
                .ok()
                .and_then(|(_, recovered)| recovered)
        } else {
            None
        };
        let mut warnings = Vec::new();
        if let Some(r) = recovered {
            audit::record(&self.ctx, permit, "recover", &r.to, r.code());
            warnings.push(Warning::Recovered(r));
        }
        audit::record(&self.ctx, permit, verb, subject, code);
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
        let permit = self.permitted()?;
        let key = self.signing_in(permit, typed)?;
        switch::sign_in(&self.ctx, permit, key.provider)
            .map_err(|error| self.not_started(permit, typed, error))
    }

    /// The same sign-in with its output piped, for a front end that has no terminal to
    /// hand over. The caller shows what the tool says and can type a code back.
    pub fn sign_in_watched(
        &self,
        typed: &str,
    ) -> std::result::Result<switch::WatchedSignIn, Failed> {
        let permit = self.permitted()?;
        let key = self.signing_in(permit, typed)?;
        switch::sign_in_watched_as(&self.ctx, permit, &key)
            .map_err(|error| self.not_started(permit, typed, error))
    }

    /// Which account a sign-in is for, once everything that could refuse it has been asked.
    ///
    /// A name no account could have is refused the way every change refuses one, settling
    /// an interrupted switch on the way; anything else refused here is recorded and nothing
    /// more, since nothing was about to change.
    fn signing_in(&self, permit: Permit, typed: &str) -> std::result::Result<Key, Failed> {
        let key = self.chosen(permit, "enroll", typed)?;
        self.ready_to_sign_in(key.provider)
            .map_err(|error| self.not_started(permit, typed, error))?;
        Ok(key)
    }

    /// A sign-in that did not start, or did not finish, for a reason other than its name.
    fn not_started(&self, permit: Permit, typed: &str, error: Error) -> Failed {
        audit::record(&self.ctx, permit, "enroll", typed, error.code());
        Failed {
            error,
            warnings: Vec::new(),
        }
    }

    /// Checked before a sign-in starts, so a person does not sign in through a browser
    /// only to be told the state file belongs to another machine, that the tool is not
    /// installed, or that the account could never be switched to afterwards.
    fn ready_to_sign_in(&self, tool: crate::provider::ProviderId) -> Result<()> {
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
        let permit = self.permitted()?;
        let key = self.chosen(permit, "enroll", typed)?;
        self.changing(
            permit,
            "enroll",
            &key.typed(),
            Some(key.provider),
            |settled| switch::enroll(settled, &key, Some(login)),
        )
    }

    /// Returns the account's email.
    pub fn forget(&self, typed: &str) -> Changing<String> {
        let permit = self.permitted()?;
        let key = self.named(permit, "forget", typed)?;
        self.changing(
            permit,
            "forget",
            &key.typed(),
            Some(key.provider),
            |settled| switch::forget(settled, &key),
        )
    }

    /// Throws away a record of an interrupted switch that cannot be finished, keeping
    /// every login it names. The way out when recovery cannot reach Anthropic.
    pub fn abandon_recovery(&self) -> Result<Option<switch::Abandoned>> {
        let permit = self.permit()?;
        let outcome = switch::abandon(&self.ctx, permit);
        audit::record(
            &self.ctx,
            permit,
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
    /// Refused where this process may change nothing, as every change is, so a run that
    /// renewed nothing for that reason says so rather than reading as one where nothing was
    /// due.
    ///
    /// A run the schedule started renews the default home, whatever `PITBOARD_HOME` says
    /// ([`schedule::for_its_run`]): the schedule is that home's alone.
    pub fn renew(&self) -> Result<Vec<(Key, Renewal)>> {
        let ctx = schedule::for_its_run(&self.ctx);
        let permit = gate(&ctx)?;
        let outcomes = switch::renew_due(&ctx, permit, switch::Due::ToStayAlive);
        for (key, outcome) in &outcomes {
            audit::record(&ctx, permit, "renew", &key.typed(), outcome.code());
        }
        Ok(outcomes)
    }

    /// Whether anything is keeping parked logins alive on this machine without somebody
    /// running a command.
    pub fn schedule(&self) -> schedule::Installed {
        schedule::status(&self.ctx)
    }

    /// Whether the schedule renews this Pitboard's own parked logins: whether its home is
    /// the default home, `~/.pitboard`, compared as a path. The schedule renews that home
    /// alone, and `schedule_install` is refused from any other.
    pub fn schedule_renews_this_home(&self) -> bool {
        schedule::serves(&self.ctx)
    }

    /// Ask the platform's own scheduler to run `renew` daily. Opt-in, and stays opt-in.
    /// Refused with `schedule_not_default_home` where this is not the default home
    /// ([`Pitboard::schedule_renews_this_home`]).
    pub fn schedule_install(&self) -> Result<std::path::PathBuf> {
        schedule::install(&self.ctx, self.permit()?)
    }

    /// Take it away. `false` when there was nothing installed.
    pub fn schedule_uninstall(&self) -> Result<bool> {
        schedule::uninstall(&self.ctx, self.permit()?)
    }

    /// Point a schedule an app up to 0.3.0 wrote, which runs the app itself, at the command
    /// line this context names. `true` when it did; nothing changes otherwise.
    pub fn schedule_repair(&self) -> Result<bool> {
        schedule::repair(&self.ctx, self.permit()?)
    }

    /// Take over a Pitboard directory another machine wrote: keep the accounts, drop the
    /// logins that came with them. `None` when the directory was already this machine's.
    ///
    /// The one change that does not settle first, because a stamp from elsewhere is what
    /// stops settling. Everything after it settles normally.
    pub fn adopt(&self) -> Result<Option<switch::Adopted>> {
        switch::adopt(&self.ctx, self.permit()?)
    }

    /// Ask the credential store what parked logins are on this machine, and give back or
    /// delete every one Pitboard's own records do not name. Ordinarily there is nothing to
    /// do: every change resolves the names it wrote down. This is for a machine whose state
    /// file was lost or restored from a backup, where the store is the only record left.
    pub fn repair(&self) -> Changing<switch::Reclaimed> {
        let permit = self.permitted()?;
        self.changing(permit, "repair", "", None, |settled| {
            switch::repair(settled).map(|r| (r, Vec::new()))
        })
    }

    /// What is running `which`'s tool with a login in memory that a switch would leave it
    /// on, by kind: for a front end to say so, or to offer to quit an app, before switching.
    /// Empty where nothing is, where the tool follows a switch by itself, or where nobody
    /// could tell. Reads the process list and nothing else.
    pub fn holding(&self, which: ProviderId) -> Vec<holder::Holding> {
        match switch::still_holding(&self.ctx, which) {
            switch::StillHolding::These(holding) => holding,
            switch::StillHolding::Nothing | switch::StillHolding::Unknown => Vec::new(),
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

    /// What the app keeps in `file` in Pitboard's directory, or `None` where it keeps nothing
    /// there. One that is there and cannot be read is an error.
    pub fn app_file(&self, file: crate::app::AppFile) -> std::io::Result<Option<String>> {
        crate::app::read_app_file(&self.ctx, file)
    }

    /// Keeps `body` in the app's `file` in Pitboard's directory, private and whole, as the
    /// core writes its own files.
    pub fn keep_app_file(&self, file: crate::app::AppFile, body: &str) -> Result<()> {
        crate::app::write_app_file(&self.ctx, self.permit()?, file, body)
    }

    /// The changes Pitboard has made, newest last.
    pub fn log(&self, limit: usize) -> Vec<audit::Entry> {
        audit::read(&self.ctx, limit)
    }

    /// Takes away the daily renewal schedule, deletes every parked login this Pitboard
    /// wrote, and removes Pitboard's own directory. Each tool's login is left alone: whoever
    /// is signed in stays signed in.
    pub fn uninstall(&self) -> Changing<switch::Removed> {
        let permit = self.permitted()?;
        self.changing(permit, "uninstall", "", None, |settled| {
            switch::uninstall(settled).map(|r| (r, Vec::new()))
        })
    }

    /// Returns the account's email.
    /// `from` may be qualified; `to` is a plain name, and stays inside whichever provider
    /// the account already belongs to. Renaming cannot move an account between tools.
    pub fn rename(&self, from: &str, to: &str) -> Changing<String> {
        let permit = self.permitted()?;
        let from = self.named(permit, "rename", from)?;
        let chosen = self.chosen(permit, "rename", to)?;
        // Only a prefix somebody actually typed can disagree: a bare new name stays inside
        // the account's own tool whatever tool a bare name would mean for a new account.
        if to.contains(crate::label::SEPARATOR) && chosen.provider != from.provider {
            let error = Error::Usage(format!(
                "`{from}` is a {} account, and a rename cannot move it to {}. Sign in to that \
                 tool and enrol the account there instead.",
                from.provider, chosen.provider
            ));
            return Err(self.refused(permit, "rename", &from.typed(), error.code(), error));
        }
        let to = chosen.label;
        self.changing(
            permit,
            "rename",
            &format!("{from} -> {to}"),
            Some(from.provider),
            |settled| switch::rename(settled, &from, &to).map(|email| (email, Vec::new())),
        )
    }

    /// What a file behind `tool`'s store says, where one is there. It reads that store, so a
    /// read that asks nobody does not ask it.
    fn left_behind(&self, tool: ProviderId) -> Option<Warning> {
        let behind = crate::provider::of(tool).behind(&self.ctx)?;
        Some(Warning::FallbackLogin {
            tool,
            path: behind.path,
            held: behind.held,
        })
    }

    /// Settles, runs the change, and records it in the audit log.
    ///
    /// `tool` is the tool whose login the change is about, where it is about one: its own
    /// ways of being signed in by something else are what is worth warning about, and a
    /// refusal that is one tool's business does not stop a change to another's.
    fn changing<T: Audited>(
        &self,
        permit: Permit,
        verb: &str,
        subject: &str,
        tool: Option<ProviderId>,
        run: impl FnOnce(Settled) -> Result<(T, Vec<Warning>)>,
    ) -> Changing<T> {
        let (settled, recovered) = switch::settle(&self.ctx, permit, tool).map_err(|error| {
            audit::record(&self.ctx, permit, verb, subject, error.code());
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
            warnings.extend(self.left_behind(tool));
        }
        if let Some(r) = recovered {
            audit::record(&self.ctx, permit, "recover", &r.to, r.code());
            warnings.push(Warning::Recovered(r));
        }
        match run(settled) {
            Ok((value, more)) => {
                if value.recorded() {
                    audit::record(&self.ctx, permit, verb, subject, value.audit_code());
                }
                warnings.extend(more);
                Ok(Done { value, warnings })
            }
            Err(mut error) => {
                audit::record(&self.ctx, permit, verb, subject, error.code());
                warnings.extend(error.take_warnings());
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

    /// Whether it changed anything worth a line. A change that found, once it held the lock,
    /// that there was nothing to do, did nothing.
    fn recorded(&self) -> bool {
        true
    }
}

impl Audited for crate::autoswitch::Auto {
    fn recorded(&self) -> bool {
        matches!(self, crate::autoswitch::Auto::Switched { .. })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::switch::harness::{
        Machine, codex_machine, hold, in_use_lines, machine, signed_in_outside, state_file,
    };
    use std::collections::BTreeMap;
    use std::sync::Arc;

    type Make = fn(&str) -> Machine;

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
        let settled = switch::settle(&m.ctx, Permit::for_a_test(), None)
            .expect("nothing to recover yet")
            .0;
        let died = crate::fault::killing("switch.park_recorded", || {
            switch::switch(settled, &m.key("there"))
        });
        assert_eq!(died.unwrap_err(), "switch.park_recorded");
        assert!(switch::interrupted(&m.ctx));
        m
    }

    /// The same switch, which the next change cannot finish: nothing is signed in to the
    /// tool any more, so nothing says which side of the switch won. Told apart without
    /// asking anyone, for either tool.
    fn stuck(make: Make, name: &str) -> Machine {
        let m = interrupted(make, name);
        m.fault_live(crate::store::memory::Fault::Vanish);
        m
    }

    /// The switch interrupted on a Claude Code machine, which the next change cannot finish
    /// either, though only Anthropic could tell: Claude Code has since renewed the login, so
    /// it matches neither side the record kept, and its session has expired, so Anthropic
    /// will not say whose it is. An access token the scripted service does not know is one
    /// it refuses.
    fn stuck_unless_asked(name: &str) -> Machine {
        let m = interrupted(machine, name);
        m.sign_in(&crate::switch::harness::document("renewed-since"));
        m
    }

    /// The warnings a read gives, by code and words.
    fn said(read: &Done<status::Report>) -> Vec<(&'static str, String)> {
        read.warnings
            .iter()
            .map(|w| (w.code(), w.to_string()))
            .collect()
    }

    /// Everything a read must leave as it found it: Pitboard's files but the records a
    /// live read keeps by design (what it measured and when to ask again), every parked
    /// login and the live one.
    type Untouched = (
        BTreeMap<String, Vec<u8>>,
        Vec<(String, Option<String>)>,
        Option<serde_json::Value>,
    );

    fn untouched(m: &Machine) -> Untouched {
        let kept_by_a_read = ["usage.json", "usage.lock", "asking.json"];
        let mut files = files(m);
        files.retain(|path, _| !kept_by_a_read.iter().any(|kept| path.ends_with(kept)));
        let vault = m.mem.vault();
        let parked = vault
            .services()
            .into_iter()
            .map(|service| {
                let held = vault.peek(&service);
                (service, held)
            })
            .collect();
        (files, parked, m.live())
    }

    /// A switch says when Claude Code keeps another login in its fallback file, which a
    /// session that cannot read the keychain signs in with and no switch reaches. Codex has
    /// no store behind the one in use, so nothing is said of it.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_switch_says_a_login_is_left_where_it_does_not_reach() {
        let m = machine("switch-fallback-login");
        let left = plant_behind(
            &m,
            &crate::switch::harness::document("left-by-a-sign-in").to_string(),
        );
        let switched = Pitboard::new(m.ctx.clone())
            .switch_to("there")
            .expect("a switch");
        let said: Vec<String> = switched
            .warnings
            .iter()
            .filter(|w| w.code() == "fallback_login")
            .map(ToString::to_string)
            .collect();
        assert_eq!(said.len(), 1, "{:?}", switched.warnings);
        assert!(
            said[0].starts_with(&left.display().to_string()),
            "{}",
            said[0]
        );

        for (tool, make) in MACHINES {
            let m = make(&format!("switch-nothing-left-{tool}"));
            let switched = Pitboard::new(m.ctx.clone())
                .switch_to(&m.key("there").typed())
                .expect("a switch");
            assert!(
                switched
                    .warnings
                    .iter()
                    .all(|w| w.code() != "fallback_login"),
                "{tool}: {:?}",
                switched.warnings
            );
        }
    }

    /// Plants `contents` in Claude Code's `.credentials.json` behind the keychain, as a
    /// sign-in where the keychain could not be read leaves it, and gives back its path.
    fn plant_behind(m: &Machine, contents: &str) -> std::path::PathBuf {
        let left = crate::provider::claude::live::credential_file(&m.ctx);
        m.mem.file_at(left.clone()).plant(
            &crate::provider::claude::paths::live_service(&m.ctx),
            contents,
        );
        left
    }

    /// While a file sits behind the keychain, running sessions keep their account after a
    /// switch until their login is next renewed, so a read says it is there, as a change
    /// does, and not only somebody about to switch hears of it. A read that asks nobody
    /// reads no keychain, and says nothing of what is behind one. Codex has no store behind
    /// its own.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_read_says_a_file_is_behind_the_keychain() {
        let m = machine("read-fallback-login");
        let left = plant_behind(
            &m,
            &crate::switch::harness::document("left-by-a-sign-in").to_string(),
        );
        let pitboard = Pitboard::new(m.ctx.clone());

        let read = pitboard.status(false).expect("a read");
        let said: Vec<String> = read
            .warnings
            .iter()
            .filter(|w| w.code() == "fallback_login")
            .map(ToString::to_string)
            .collect();
        assert_eq!(said.len(), 1, "{:?}", read.warnings);
        assert!(
            said[0].starts_with(&format!(
                "{} holds another Claude Code login",
                left.display()
            )),
            "{}",
            said[0]
        );
        assert!(
            said[0].contains(
                "sessions already running at a switch keep the account they are on until \
                 their login is next renewed, or until they are started again"
            ),
            "{}",
            said[0]
        );
        let offline = pitboard.status_offline().expect("a read of what is known");
        assert!(
            offline
                .warnings
                .iter()
                .all(|w| w.code() != "fallback_login"),
            "{:?}",
            offline.warnings
        );

        for (tool, make) in MACHINES {
            let m = make(&format!("read-nothing-left-{tool}"));
            let read = Pitboard::new(m.ctx.clone()).status(false).expect("a read");
            assert!(
                read.warnings.iter().all(|w| w.code() != "fallback_login"),
                "{tool}: {:?}",
                read.warnings
            );
        }
    }

    /// A running session watches the file by its being there, whatever it holds, so one
    /// with no login in it holds sessions to their account all the same, and is said too:
    /// in words that say so, and with what lets them follow.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn an_empty_file_behind_the_keychain_is_said_too() {
        let m = machine("read-empty-fallback");
        let left = plant_behind(&m, "{}");
        let pitboard = Pitboard::new(m.ctx.clone());

        for warnings in [
            pitboard.status(false).expect("a read").warnings,
            pitboard.switch_to("there").expect("a switch").warnings,
        ] {
            let said: Vec<String> = warnings
                .iter()
                .filter(|w| w.code() == "fallback_login")
                .map(ToString::to_string)
                .collect();
            assert_eq!(
                said,
                [format!(
                    "{path} is there with no Claude Code login in it. While it is, Claude \
                     Code sessions already running at a switch keep the account they are on \
                     until their login is next renewed, or until they are started again. \
                     Deleting it lets them follow a switch: `rm {path}`.",
                    path = left.display()
                )],
                "{warnings:?}"
            );
        }
    }

    /// A running session looks at the file and never reads it to decide, so one Pitboard
    /// cannot read, as one that is not text or that this user may not read, holds sessions
    /// to their account all the same. It is said, in words that do not guess what it holds.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_file_behind_the_keychain_that_cannot_be_read_is_said_too() {
        let m = machine("read-unreadable-fallback");
        let left = plant_behind(&m, "{}");
        m.mem.file_at(left.clone()).fault(
            &crate::provider::claude::paths::live_service(&m.ctx),
            crate::store::memory::Fault::UnreadableContents(
                "stream did not contain valid UTF-8".into(),
            ),
        );
        let pitboard = Pitboard::new(m.ctx.clone());

        for warnings in [
            pitboard.status(false).expect("a read").warnings,
            pitboard.switch_to("there").expect("a switch").warnings,
        ] {
            let said: Vec<String> = warnings
                .iter()
                .filter(|w| w.code() == "fallback_login")
                .map(ToString::to_string)
                .collect();
            assert_eq!(
                said,
                [format!(
                    "{} is there, and Pitboard could not read it. While it is, Claude Code \
                     sessions already running at a switch keep the account they are on until \
                     their login is next renewed, or until they are started again. `pitboard \
                     doctor` says why, and what to do.",
                    left.display()
                )],
                "{warnings:?}"
            );
        }
    }

    /// A switch nothing can finish was said only by a change refused over it. The app looks
    /// for it in what a read says, so it never offered to give up on one, and `pitboard
    /// status` never said it at all. A read says it now, in the words of the refusal, and
    /// changes nothing: the record is kept for whatever finishes it or gives up on it.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_read_says_an_interrupted_switch_it_cannot_finish_is_waiting() {
        for (tool, make) in MACHINES {
            let m = stuck(make, &format!("read-stuck-{tool}"));
            let pitboard = Pitboard::new(m.ctx.clone());
            let before = untouched(&m);

            let read = pitboard
                .status(false)
                .expect("a read is not refused over it");

            assert_eq!(untouched(&m), before, "{tool}: a read changes nothing");
            assert!(switch::interrupted(&m.ctx), "{tool}: the record is kept");
            let refused = pitboard
                .switch_to(&m.key("there").typed())
                .expect_err("every change stops at it");
            assert_eq!(refused.error.code(), "recovery_undetermined", "{tool}");
            assert_eq!(
                said(&read),
                [("recovery_undetermined", refused.error.to_string())],
                "{tool}: said in the refusal's own words"
            );
        }
    }

    /// Where only the service can say whose the live login is, a read asks it, as the next
    /// change would.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_read_asks_whose_the_login_is_where_only_the_service_can_say() {
        let m = stuck_unless_asked("read-stuck-asked");
        let pitboard = Pitboard::new(m.ctx.clone());
        let before = untouched(&m);

        let read = pitboard.status(false).expect("a read");

        assert_eq!(untouched(&m), before, "a read changes nothing");
        let refused = pitboard.switch_to("there").expect_err("it stops at it");
        assert_eq!(
            said(&read),
            [("recovery_undetermined", refused.error.to_string())]
        );
        assert!(
            refused.error.to_string().contains("has expired"),
            "{}",
            refused.error
        );
    }

    /// A switch the next change finishes by itself is not one anybody has to do anything
    /// about. The rows already say who is signed in, from the tool's login and not from
    /// Pitboard's record, and the change that finishes it says what it found.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_read_says_nothing_of_a_switch_the_next_change_finishes() {
        for (tool, make) in MACHINES {
            let m = interrupted(make, &format!("read-settles-{tool}"));
            let before = untouched(&m);

            let read = Pitboard::new(m.ctx.clone()).status(false).expect("a read");

            assert_eq!(said(&read), [], "{tool}");
            assert_eq!(untouched(&m), before, "{tool}: a read finishes nothing");
            assert!(switch::interrupted(&m.ctx), "{tool}");
        }
    }

    /// The read never says a switch is waiting that the next change would not stop at for
    /// that reason, and never offers to give up on one that giving up cannot reach. Under a
    /// custom Claude Code endpoint every change is refused over the endpoint before it comes
    /// to a Claude Code switch, and giving up on one is refused too. A Codex switch is
    /// settled under one as anywhere else, and given up on as anywhere else: giving up
    /// refused it, so the read offered a way out that could not be taken.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_read_says_a_switch_is_waiting_only_where_a_change_would_stop_at_it() {
        for (tool, make) in MACHINES {
            let m = stuck(make, &format!("read-stuck-custom-{tool}"));
            let mut ctx = m.ctx.clone();
            ctx.custom_oauth = true;
            let pitboard = Pitboard::new(ctx);

            let read = pitboard.status(false).expect("a read");
            let known = pitboard.status_offline().expect("a read");
            let refused = pitboard
                .switch_to(&m.key("there").typed())
                .expect_err("refused");

            let codes: Vec<&str> = read.warnings.iter().map(Warning::code).collect();
            let expected: &[&str] = match m.which {
                ProviderId::Claude => &[],
                ProviderId::Codex => &["recovery_undetermined"],
            };
            assert_eq!(codes, expected, "{tool}: {}", refused.error);
            let codes: Vec<&str> = known.warnings.iter().map(Warning::code).collect();
            assert_eq!(codes, expected, "{tool}: the read that asks nobody too");
            assert_eq!(
                refused.error.code() == "recovery_undetermined",
                !expected.is_empty(),
                "{tool}"
            );

            let gave_up = pitboard.abandon_recovery();
            match m.which {
                ProviderId::Claude => assert_eq!(
                    gave_up.expect_err("refused").code(),
                    "custom_oauth_endpoint",
                    "{tool}"
                ),
                ProviderId::Codex => {
                    assert!(gave_up.expect("given up").is_some(), "{tool}");
                    assert!(!switch::interrupted(&m.ctx), "{tool}: nothing waits");
                }
            }
        }
    }

    /// Every read says it, the one that asks nobody too: what an app reads each time the
    /// account index moves, and what `pitboard status --offline` prints. It sends no request,
    /// so it says it wherever the record and the logins tell it without one, in the words of
    /// the refusal, and changes nothing. Where only Anthropic could say whose the login is,
    /// it does not say what it cannot know.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_read_that_asks_nobody_says_a_switch_is_waiting_wherever_it_can_tell() {
        for (tool, make) in MACHINES {
            let m = stuck(make, &format!("known-stuck-{tool}"));
            let pitboard = Pitboard::new(m.ctx.clone());
            let (before, asked) = (untouched(&m), m.api.calls());

            let read = pitboard.status_offline().expect("a read");

            assert_eq!(untouched(&m), before, "{tool}: a read changes nothing");
            assert_eq!(m.api.calls(), asked, "{tool}: and asks nobody");
            assert!(switch::interrupted(&m.ctx), "{tool}: the record is kept");
            let refused = pitboard
                .switch_to(&m.key("there").typed())
                .expect_err("every change stops at it");
            assert_eq!(
                said(&read),
                [("recovery_undetermined", refused.error.to_string())],
                "{tool}"
            );
        }

        let m = stuck_unless_asked("known-stuck-unasked");
        let asked = m.api.calls();
        let read = Pitboard::new(m.ctx.clone())
            .status_offline()
            .expect("a read");
        assert_eq!(m.api.calls(), asked, "asks nobody");
        assert_eq!(said(&read), [], "only Anthropic could tell");
    }

    /// Claude Code 2.1.294 stamps its usage cache with the account its config names, and
    /// asks with the login its session holds, which can still be the account switched away
    /// from. Four of five caches captured on one machine were another login's numbers, so
    /// no read takes a reading from it, whether or not Anthropic answered this time.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn what_claude_code_cached_is_never_an_accounts_reading() {
        use crate::switch::harness::{NOW, cache_usage, usage_answer};
        let m = machine("cached-usage");
        readings::answered(
            &m.ctx,
            Permit::for_a_test(),
            &[(
                "here".into(),
                crate::usage::from_usage_object(&usage_answer(10.0), NOW),
            )],
        );
        cache_usage(&m, usage_answer(100.0));
        let pitboard = Pitboard::new(m.ctx.clone());
        for (how, read) in [
            ("offline", pitboard.status_offline()),
            ("online", pitboard.status(false)),
        ] {
            let read = read.expect("a read");
            let here = read
                .value
                .rows
                .iter()
                .find(|row| row.label.as_deref() == Some("here"))
                .expect("here's row");
            let shares: Vec<f64> = here
                .usage
                .iter()
                .flat_map(|usage| &usage.windows)
                .map(|window| window.percent)
                .collect();
            assert_eq!(shares, [10.0], "{how}");
        }
    }

    /// A login Anthropic has named is known by its refresh token's fingerprint from then on,
    /// so a read asks whose it is only once the login has changed. It asked on every read,
    /// one round trip more each time for an answer it already had.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_login_already_identified_is_not_asked_about_again() {
        use crate::api::scripted::Asked;
        let m = machine("read-known-login");

        Pitboard::new(m.ctx.clone()).status(false).expect("a read");

        let asked = m.api.asked();
        assert!(
            !asked.contains(&Asked::Owner("access-here-refresh".into())),
            "{asked:?}"
        );
        assert!(
            asked.contains(&Asked::Usage("access-here-refresh".into())),
            "what it has left is still asked: {asked:?}"
        );
    }

    /// A sign-in outside Pitboard replaced the only login of the account in use. The first
    /// read records it and the activity log says so once; every read after, the one that
    /// asks nobody too, says it until that account is signed in again or forgotten, and its
    /// row says why it has nothing parked. It read as `nothing parked`, and nothing said what
    /// had happened.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_sign_in_outside_pitboard_is_said_recorded_and_shown_on_its_row() {
        use crate::switch::harness::NOW;
        let m = signed_in_outside("read-signed-in-outside");
        let pitboard = Pitboard::new(m.ctx.clone());
        let replaced = "`here`'s login was replaced by a sign-in outside Pitboard: Claude Code \
                        now has `elsewhere`'s login stored, and Pitboard holds no login for \
                        `here`. Run `pitboard enroll here --sign-in` to sign in to it again.";
        let row = |read: &Done<status::Report>, label: &str| {
            read.value
                .rows
                .iter()
                .find(|row| row.label.as_deref() == Some(label))
                .map(|row| (row.signed_in, row.stale))
        };

        let read = pitboard.status(false).expect("a read");

        let state = state::load(&m.ctx).expect("state");
        assert_eq!(
            state
                .account_in_use(ProviderId::Claude)
                .map(|account| account.label.as_str()),
            Some("elsewhere")
        );
        let account = |label: &str| state.get(&m.key(label)).expect("enrolled");
        assert_eq!(account("here").replaced_at, Some(NOW));
        assert_eq!(account("elsewhere").last_used_at, Some(NOW));
        assert!(account("there").parked.is_some(), "`there` keeps its park");
        assert_eq!(
            row(&read, "elsewhere").map(|(signed_in, _)| signed_in),
            Some(true)
        );
        assert_eq!(
            row(&read, "here"),
            Some((false, Some(status::Stale::LoginReplaced)))
        );
        assert_eq!(said(&read), [("login_replaced", replaced.to_string())]);
        let noticed = [
            ("elsewhere".to_string(), "signed_in_outside".to_string()),
            ("here".to_string(), "login_replaced".to_string()),
        ];
        assert_eq!(in_use_lines(&m), noticed);

        let recorded = state_file(&m);
        let again = pitboard.status(false).expect("a second read");
        assert_eq!(said(&again), [("login_replaced", replaced.to_string())]);
        assert_eq!(in_use_lines(&m), noticed, "recorded once");
        assert_eq!(state_file(&m), recorded, "and written once");

        let offline = pitboard.status_offline().expect("a read of what is known");
        assert_eq!(said(&offline), [("login_replaced", replaced.to_string())]);
    }

    /// What a replaced login is said to have been replaced with is what the record holds of
    /// the store: an account nobody enrolled, by its email, no login at all, or nothing where
    /// nothing is recorded of the store.
    #[test]
    fn a_replaced_login_says_what_the_store_holds_instead_as_far_as_it_is_known() {
        use crate::in_use::InUse;
        use crate::switch::harness::{account, owner};
        let mut state = state::State::default();
        state.accounts.push(account("here", "here", None));
        state.accounts[0].replaced_at = Some(1);
        let said = |state: &state::State| -> Vec<String> {
            standing(state).iter().map(ToString::to_string).collect()
        };
        let then = "and Pitboard holds no login for `here`. Run `pitboard enroll here \
                    --sign-in` to sign in to it again.";

        let somebody = InUse {
            owner: Some(owner("new")),
            login: "new-login".into(),
            known_at: 1,
            named: None,
        };
        state
            .in_use
            .insert(ProviderId::Claude.code().into(), somebody);
        assert_eq!(
            said(&state),
            [format!(
                "`here`'s login was replaced by a sign-in outside Pitboard: Claude Code now has \
                 new@example.com's login stored, {then}"
            )]
        );

        state.in_use.insert(
            ProviderId::Claude.code().into(),
            crate::switch::identify::nobody(None, 1).expect("a store holding no login"),
        );
        assert_eq!(
            said(&state),
            [format!(
                "`here`'s login was replaced by a sign-in outside Pitboard: Claude Code now has \
                 no login stored, {then}"
            )]
        );

        state.in_use.clear();
        assert_eq!(
            said(&state),
            [format!(
                "`here`'s login was replaced by a sign-in outside Pitboard, {then}"
            )]
        );

        state.accounts[0].parked = Some(crate::state::Park {
            service: "pitboard-park-here-1".into(),
            parked_at: 1,
            refresh_fingerprint: "f".into(),
            access_expires_at: None,
            refresh_expires_at: None,
        });
        assert!(
            said(&state).is_empty(),
            "a login parked for it is a way back"
        );
    }

    /// A read that finds what the record already says writes nothing, so several front ends
    /// reading one machine never take turns rewriting its account index.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_read_that_finds_nothing_new_writes_nothing() {
        let m = machine("read-nothing-new");
        let pitboard = Pitboard::new(m.ctx.clone());
        let before = state_file(&m);

        pitboard.status(false).expect("a read");
        pitboard.status(false).expect("another read");

        assert_eq!(state_file(&m), before);
        assert!(in_use_lines(&m).is_empty());
    }

    /// Each `CLAUDE_CONFIG_DIR` has a login of its own stored, and Pitboard keeps whose for
    /// each. A read under another one leaves this one's record, so a sign-in outside Pitboard
    /// here is still found by the next read here, and reads taking turns under the two write
    /// nothing once each has its record.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_read_under_another_claude_config_dir_keeps_this_ones_record() {
        use crate::switch::harness::{NOW, account, document, owner};
        let m = machine("read-two-slots");
        m.api
            .owned_by("access-elsewhere-refresh", owner("elsewhere"));
        m.api.owned_by("access-there-other", owner("there"));
        let mut state = state::load(&m.ctx).expect("state");
        state.accounts.push(account("elsewhere", "elsewhere", None));
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        let other = m
            .ctx
            .clone()
            .with_claude_config_dir(m.ctx_home().join("other").to_string_lossy().into_owned());
        m.mem.live().plant(
            &crate::provider::claude::paths::live_service(&other),
            &document("there-other").to_string(),
        );

        Pitboard::new(other.clone())
            .status(false)
            .expect("a read under the other");
        m.sign_in(&document("elsewhere-refresh"));
        Pitboard::new(m.ctx.clone())
            .status(false)
            .expect("a read here");

        let state = state::load(&m.ctx).expect("state");
        assert_eq!(
            state.get(&m.key("here")).expect("enrolled").replaced_at,
            Some(NOW)
        );
        assert_eq!(
            in_use_lines(&m),
            [
                ("elsewhere".to_string(), "signed_in_outside".to_string()),
                ("here".to_string(), "login_replaced".to_string()),
            ]
        );

        let recorded = state_file(&m);
        for ctx in [&other, &m.ctx, &other] {
            Pitboard::new(ctx.clone()).status(false).expect("a read");
        }
        assert_eq!(state_file(&m), recorded, "reads taking turns write nothing");
    }

    /// `here` is signed in under two `CLAUDE_CONFIG_DIR`s, and a sign-in outside Pitboard
    /// replaces its login under one. Its login under the other is still stored, so no read
    /// says it was replaced, least of all one under the slot where it is in use. Once the
    /// other slot's is replaced too, it is.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_login_still_stored_under_another_claude_config_dir_is_not_said_replaced() {
        use crate::switch::harness::{NOW, account, document, owner};
        let m = machine("read-two-slots-one-account");
        m.api.owned_by("access-here-other", owner("here"));
        m.api
            .owned_by("access-elsewhere-refresh", owner("elsewhere"));
        let mut state = state::load(&m.ctx).expect("state");
        state.accounts.push(account("elsewhere", "elsewhere", None));
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        let other = m
            .ctx
            .clone()
            .with_claude_config_dir(m.ctx_home().join("other").to_string_lossy().into_owned());
        let plant_there = |login: &str| {
            m.mem.live().plant(
                &crate::provider::claude::paths::live_service(&other),
                &document(login).to_string(),
            );
        };
        let replaced = |read: &Done<status::Report>| {
            said(read)
                .into_iter()
                .filter(|(code, _)| *code == "login_replaced")
                .count()
        };

        plant_there("here-other");
        Pitboard::new(other.clone())
            .status(false)
            .expect("a read under the other");
        plant_there("elsewhere-refresh");
        let there = Pitboard::new(other.clone())
            .status(false)
            .expect("a read under the other after a sign-in there");
        let here = Pitboard::new(m.ctx.clone())
            .status(false)
            .expect("a read here");

        assert_eq!((replaced(&there), replaced(&here)), (0, 0));
        assert_eq!(
            state::load(&m.ctx)
                .expect("state")
                .get(&m.key("here"))
                .expect("enrolled")
                .replaced_at,
            None
        );
        assert_eq!(
            in_use_lines(&m),
            [("elsewhere".to_string(), "signed_in_outside".to_string())]
        );

        m.sign_in(&document("elsewhere-refresh"));
        let gone = Pitboard::new(m.ctx.clone())
            .status(false)
            .expect("a read here after a sign-in here");

        assert_eq!(replaced(&gone), 1);
        assert_eq!(
            state::load(&m.ctx)
                .expect("state")
                .get(&m.key("here"))
                .expect("enrolled")
                .replaced_at,
            Some(NOW)
        );
        assert_eq!(
            in_use_lines(&m)[1..],
            [
                ("elsewhere".to_string(), "signed_in_outside".to_string()),
                ("here".to_string(), "login_replaced".to_string()),
            ]
        );
    }

    /// Claude Code's config names `here` and Pitboard finds no login where it reads: the
    /// login is somewhere Pitboard does not look, which is not a sign-out. A read records
    /// nothing, and says of no account that its login was replaced.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn a_read_that_cannot_find_the_login_claude_code_names_records_nothing() {
        let m = machine("read-login-elsewhere");
        m.mem.live().delete_everything();
        let before = state_file(&m);

        let read = Pitboard::new(m.ctx.clone()).status(false).expect("a read");

        assert_eq!(state_file(&m), before);
        assert!(in_use_lines(&m).is_empty());
        let said = said(&read);
        assert!(
            !said.iter().any(|(code, _)| *code == "login_replaced"),
            "{said:?}"
        );
    }

    /// A tool whose login names its own account is identified from the login, which asks
    /// nobody, so a read that sends no request can still tell whether the next change
    /// finishes its switch. Here Codex has renewed its login since the switch stopped, so the
    /// login matches neither side the record kept, and the copy the switch parked cannot be
    /// read. `doctor` and the read that asks nobody said nothing of it, as though only OpenAI
    /// could say whose the login is.
    #[test]
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
    fn whose_a_codex_login_is_is_told_without_asking_anyone() {
        let m = interrupted(codex_machine, "codex-renewed-since");
        m.sign_in(&crate::switch::harness::codex_login(
            "here",
            "renewed-since",
        ));
        m.mem.vault().fault(
            &reserved(&m),
            crate::store::memory::Fault::Unreadable("locked".into()),
        );
        let pitboard = Pitboard::new(m.ctx.clone());
        let asked = m.api.calls();

        let read = pitboard.status_offline().expect("a read");
        let checks = pitboard.doctor().checks;

        assert_eq!(m.api.calls(), asked, "asks nobody");
        let refused = pitboard
            .switch_to("codex/there")
            .expect_err("it stops at it");
        assert_eq!(refused.error.code(), "recovery_undetermined");
        assert_eq!(
            said(&read),
            [("recovery_undetermined", refused.error.to_string())]
        );
        let check = checks
            .iter()
            .find(|c| c.code == "interrupted_switch")
            .expect("said");
        assert_eq!(check.detail, refused.error.to_string());
    }

    /// A Codex store that `/etc/codex/requirements.toml` pins is refused before a sign-in
    /// starts, naming that file, as it is for a switch: no line of the person's own changes
    /// it, and the `-c` Pitboard's sign-ins give is under it, so the login would be kept where
    /// Pitboard cannot read it back. Pitboard used to read only `$CODEX_HOME/config.toml`, and
    /// started the sign-in.
    #[test]
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
    fn a_codex_store_a_requirement_pins_is_refused_before_a_sign_in() {
        let m = codex_machine("codex-pinned");
        // Named where nothing is, so a sign-in that got past the refusal would find no
        // program to run rather than this machine's own `codex`.
        let ctx = m.ctx.clone().with_codex_program("/nowhere/codex".into());
        m.mem.administers(
            "/etc/codex/requirements.toml",
            "cli_auth_credentials_store = \"keyring\"\n",
        );
        let pitboard = Pitboard::new(ctx);
        for refused in [
            pitboard.sign_in("codex/new").err().expect("refused"),
            pitboard
                .sign_in_watched("codex/new")
                .err()
                .expect("refused"),
        ] {
            assert_eq!(refused.error.code(), "live_store_unsupported");
            let said = refused.error.to_string();
            assert!(
                said.contains("pinned to `keyring` by /etc/codex/requirements.toml"),
                "{said}"
            );
            assert!(
                said.contains("No line in your own config.toml can change it"),
                "{said}"
            );
        }
        let switched = pitboard.switch_to("codex/there").expect_err("refused too");
        assert_eq!(switched.error.code(), "live_store_unsupported");
        let file = m
            .mem
            .file_at(crate::provider::codex::paths::auth_file(&m.ctx));
        let kept = crate::store::RawStore::read(&file, "auth.json")
            .expect("readable")
            .expect("there");
        let kept: serde_json::Value = serde_json::from_str(&kept).expect("JSON");
        assert_eq!(
            kept["tokens"]["refresh_token"], "here-refresh",
            "the auth.json Codex does not use is left as it was"
        );
    }

    /// The park the interrupted switch reserved, as its record names it.
    fn reserved(m: &Machine) -> String {
        let raw = std::fs::read_to_string(crate::home::dir(&m.ctx).join("journal.json"))
            .expect("a record");
        serde_json::from_str::<serde_json::Value>(&raw).expect("a record is JSON")["park_service"]
            .as_str()
            .expect("a park")
            .to_owned()
    }

    /// `doctor` sends no request, and says what a read says wherever it can tell without
    /// one: the refusal, and the way out. Where only the service could tell, it says what
    /// it always said, that a switch did not finish.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn doctor_says_a_switch_is_stuck_where_it_can_tell_without_asking() {
        for (tool, make) in MACHINES {
            let m = stuck(make, &format!("doctor-stuck-{tool}"));
            let pitboard = Pitboard::new(m.ctx.clone());
            let before = untouched(&m);

            let checks = pitboard.doctor().checks;

            assert_eq!(untouched(&m), before, "{tool}: doctor changes nothing");
            let refused = pitboard
                .switch_to(&m.key("there").typed())
                .expect_err("refused");
            let check = checks
                .iter()
                .find(|c| c.code == "interrupted_switch")
                .expect("said");
            assert_eq!(check.level, doctor::Level::Warn, "{tool}");
            assert_eq!(check.detail, refused.error.to_string(), "{tool}");
            assert!(check.advice.contains("pitboard abandon"), "{tool}");
        }

        let m = stuck_unless_asked("doctor-stuck-unasked");
        let asked = m.api.calls();
        let checks = Pitboard::new(m.ctx.clone()).doctor().checks;
        assert_eq!(m.api.calls(), asked, "doctor asks nobody");
        let check = checks
            .iter()
            .find(|c| c.code == "interrupted_switch")
            .expect("said");
        assert_eq!(check.detail, "a switch did not finish");
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
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
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
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
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
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
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
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
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

    #[derive(Debug, Clone, Copy)]
    enum Refused {
        As(Elevation),
        On(Floor),
    }

    const REFUSED: [Refused; 6] = [
        Refused::As(Elevation::Elevated { why: "as root" }),
        Refused::As(Elevation::Elevated { why: "with sudo" }),
        Refused::As(Elevation::Elevated {
            why: crate::host::token::AS_A_SERVICE_ACCOUNT,
        }),
        Refused::As(Elevation::Unknown),
        Refused::On(Floor::Below { build: 22631 }),
        Refused::On(Floor::Unknown),
    ];

    impl Refused {
        fn said_by(self, m: &Machine) {
            match self {
                Refused::As(elevation) => m.mem.runs_with(elevation),
                Refused::On(floor) => m.mem.runs_on(floor),
            }
        }

        fn lifted(m: &Machine) {
            m.mem.runs_with(Elevation::Normal);
            m.mem.runs_on(Floor::Met);
        }

        fn code(self) -> &'static str {
            match self {
                Refused::As(_) => "elevated",
                Refused::On(_) => "system_too_old",
            }
        }

        fn words(self) -> crate::words::ChangesNothing {
            match self {
                Refused::As(Elevation::Elevated { why }) => {
                    crate::words::elevated(crate::host::OS, Some(why))
                }
                Refused::As(_) => crate::words::elevated(crate::host::OS, None),
                Refused::On(Floor::Below { build }) => crate::words::too_old(Some(build)),
                Refused::On(_) => crate::words::too_old(None),
            }
        }
    }

    /// What one change came to, as a change refused at the gate is reported: its error,
    /// and the warnings found on the way, of which there must be none.
    fn failed<T>(outcome: Result<T>) -> Option<Failed> {
        outcome.err().map(|error| Failed {
            error,
            warnings: Vec::new(),
        })
    }

    type Change = fn(&Pitboard, &Machine) -> Option<Failed>;

    /// Every change a front end can ask for, by its name. `enroll_signed_in` is asked apart,
    /// since it takes a sign-in made before.
    const CHANGES: [(&str, Change); 16] = [
        ("switch_to", |p, m| {
            p.switch_to(&m.key("there").typed()).err()
        }),
        ("enroll_current", |p, m| {
            p.enroll_current(&m.key("here").typed()).err()
        }),
        ("sign_in", |p, m| p.sign_in(&m.key("new").typed()).err()),
        ("sign_in_watched", |p, m| {
            p.sign_in_watched(&m.key("new").typed()).err()
        }),
        ("forget", |p, m| p.forget(&m.key("there").typed()).err()),
        ("rename", |p, m| {
            p.rename(&m.key("there").typed(), "elsewhere").err()
        }),
        ("abandon_recovery", |p, _| failed(p.abandon_recovery())),
        ("renew", |p, _| failed(p.renew())),
        ("schedule_install", |p, _| failed(p.schedule_install())),
        ("schedule_uninstall", |p, _| failed(p.schedule_uninstall())),
        ("schedule_repair", |p, _| failed(p.schedule_repair())),
        ("adopt", |p, _| failed(p.adopt())),
        ("repair", |p, _| p.repair().err()),
        ("keep_app_file", |p, _| {
            failed(p.keep_app_file(crate::app::AppFile::Preferences, "{}"))
        }),
        ("uninstall", |p, _| p.uninstall().err()),
        ("auto_switch", |p, _| {
            p.auto_switch(crate::autoswitch::Threshold::DEFAULT).err()
        }),
    ];

    /// Everything on a machine a change could leave different: every directory and file
    /// under its home, the audit log among them and the scheduler's files with them, every
    /// item in its keychain, every login parked and the one in use. The private directory
    /// a sign-in made before is not counted: dropping that sign-in takes it away again.
    type Everything = (
        BTreeMap<String, Option<Vec<u8>>>,
        Vec<(String, Option<String>)>,
        Vec<(String, Option<String>)>,
        Option<serde_json::Value>,
    );

    fn everything(m: &Machine) -> Everything {
        fn walk(dir: &std::path::Path, into: &mut BTreeMap<String, Option<Vec<u8>>>) {
            for entry in std::fs::read_dir(dir).expect("a directory to read") {
                let path = entry.expect("an entry").path();
                if path.ends_with(".pitboard/signin") {
                    continue;
                }
                if path.is_dir() {
                    into.insert(path.display().to_string(), None);
                    walk(&path, into);
                } else {
                    let body = std::fs::read(&path).expect("a file to read");
                    into.insert(path.display().to_string(), Some(body));
                }
            }
        }
        let mut files = BTreeMap::new();
        walk(&m.ctx_home(), &mut files);
        let items = |store: &crate::store::memory::MemoryStore| {
            store
                .services()
                .into_iter()
                .map(|service| {
                    let held = store.peek(&service);
                    (service, held)
                })
                .collect()
        };
        (files, items(m.mem.live()), items(m.mem.vault()), m.live())
    }

    /// Running as root or under sudo, Pitboard changed things as root: a state file, a
    /// schedule or a lock it made was root's, and a park might be, where the person's own
    /// runs might never read or replace it again. Every change is refused now, at one gate,
    /// before it reads, locks or records anything, and leaves the machine byte for byte as it
    /// was, the audit log included. The machine has an interrupted switch waiting, which every
    /// change that settles would otherwise finish, and asks nobody.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
    fn every_change_is_refused_where_pitboard_may_change_nothing_and_changes_nothing() {
        for (tool, make) in MACHINES {
            for refused in REFUSED {
                for (change, run) in CHANGES {
                    let at = format!("{tool}, {change}, {refused:?}");
                    let m = interrupted(make, &format!("gate-{tool}-{change}"));
                    refused.said_by(&m);
                    let (before, asked) = (everything(&m), m.api.calls());

                    let failed = run(&Pitboard::new(m.ctx.clone()), &m)
                        .unwrap_or_else(|| panic!("{at}: refused"));

                    assert_eq!(
                        failed.error.code(),
                        refused.code(),
                        "{at}: {}",
                        failed.error
                    );
                    assert!(failed.warnings.is_empty(), "{at}: {:?}", failed.warnings);
                    assert!(everything(&m) == before, "{at}: nothing changes");
                    assert_eq!(m.api.calls(), asked, "{at}: nobody is asked");
                }

                let at = format!("{tool}, enroll_signed_in, {refused:?}");
                let m = interrupted(make, &format!("gate-{tool}-enroll-signed-in"));
                let login = crate::switch::harness::signed_in(&m, "new", "new-refresh");
                refused.said_by(&m);
                let before = everything(&m);
                let failed = Pitboard::new(m.ctx.clone())
                    .enroll_signed_in(&m.key("new").typed(), login)
                    .expect_err("refused");
                assert_eq!(failed.error.code(), refused.code(), "{at}");
                assert!(everything(&m) == before, "{at}: nothing changes");
            }
        }
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn the_gate_says_why_it_refuses_and_lets_a_person_through() {
        let m = machine("gate-words");
        let pitboard = Pitboard::new(m.ctx.clone());
        assert!(pitboard.permit().is_ok(), "as the person");
        for refused in REFUSED {
            refused.said_by(&m);
            let refusal = pitboard.permit().expect_err("refused");
            assert_eq!(refusal.code(), refused.code(), "{refused:?}");
            assert_eq!(refusal.exit_code(), 1, "{refused:?}");
            assert_eq!(refusal.to_string(), refused.words().refusal, "{refused:?}");
            Refused::lifted(&m);
            assert!(pitboard.permit().is_ok(), "{refused:?}: lifted");
        }
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn the_gate_lets_windows_11_24h2_through_and_says_an_older_one_first() {
        let m = machine("gate-floor");
        let pitboard = Pitboard::new(m.ctx.clone());
        for build in [26100, 26200] {
            m.mem
                .runs_on(crate::host::windows_floor(Some((10, 0, build))));
            assert!(pitboard.permit().is_ok(), "{build}");
        }
        m.mem
            .runs_on(crate::host::windows_floor(Some((10, 0, 22631))));
        let refused = pitboard.permit().expect_err("refused");
        assert_eq!(refused.code(), "system_too_old");
        assert_eq!(
            refused.to_string(),
            crate::words::too_old(Some(22631)).refusal
        );
        m.mem.runs_with(Elevation::Elevated { why: "as root" });
        let first = pitboard.permit().expect_err("refused");
        assert_eq!(
            first.code(),
            "system_too_old",
            "the system before how it runs"
        );
    }

    /// An empty or relative `HOME` made every path under it lead into the folder Pitboard
    /// was run from: its own files where `PITBOARD_HOME` was not set, and the scheduler's
    /// and each tool's default folder whatever was. The gate refuses it now, and so does
    /// every read of the accounts, with `home_not_absolute`, and nothing changes. Asked of
    /// the gate and the reads first, which change nothing whatever they answer.
    #[test]
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
    fn every_change_and_read_is_refused_where_the_home_is_not_a_full_path() {
        for home in ["", "relative"] {
            for (tool, make) in MACHINES {
                let at = format!("{tool}, HOME={home:?}");
                let m = make(&format!("unplaced-{tool}-{}", home.len()));
                let mut ctx = m.ctx.clone();
                ctx.home = std::path::PathBuf::from(home);
                let pitboard = Pitboard::new(ctx);
                let refused = |error: Error| {
                    assert_eq!(error.code(), "home_not_absolute", "{at}: {error}");
                    assert!(error.to_string().starts_with("HOME is "), "{at}: {error}");
                };

                refused(pitboard.permit().expect_err("the gate refuses"));
                refused(pitboard.status_offline().err().expect("refused"));
                refused(pitboard.status(false).err().expect("refused"));
                let before = (everything(&m), m.api.calls());
                for (change, run) in CHANGES {
                    let failed = run(&pitboard, &m).unwrap_or_else(|| panic!("{at}: {change}"));
                    refused(failed.error);
                    assert!(failed.warnings.is_empty(), "{at}, {change}");
                }
                assert!(
                    (everything(&m), m.api.calls()) == before,
                    "{at}: nothing changes and nobody is asked"
                );
            }
        }
    }

    /// What a read says, account by account: enough to tell a live read from one of what
    /// was last measured.
    type Read = Vec<(
        ProviderId,
        Option<String>,
        bool,
        Option<crate::usage::Snapshot>,
        Option<status::Stale>,
    )>;

    fn rows(report: &status::Report) -> Read {
        report
            .rows
            .iter()
            .map(|row| {
                (
                    row.provider,
                    row.label.clone(),
                    row.signed_in,
                    row.usage.clone(),
                    row.stale,
                )
            })
            .collect()
    }

    /// A read renewed every parked login whose access had lapsed, asked each service about
    /// every account and wrote what it measured down, as root too. Where this process may
    /// change nothing it answers what the read that asks nobody answers, and says why: it
    /// renews nothing, asks nobody and writes nothing. A park whose access has lapsed is
    /// there to be renewed, so a read that renewed would show it.
    #[test]
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
    fn a_read_where_pitboard_may_change_nothing_asks_nobody_and_says_why() {
        for (tool, make) in MACHINES {
            for refused in REFUSED {
                let at = format!("{tool}, {refused:?}");
                let m = make(&format!("gate-read-{tool}"));
                let later = m
                    .ctx
                    .clone()
                    .with_clock(Arc::new(crate::time::FixedClock::at(
                        crate::switch::harness::NOW + 11 * 86_400,
                    )));
                let pitboard = Pitboard::new(later);
                refused.said_by(&m);
                let (before, asked) = (everything(&m), m.api.calls());

                let read = pitboard.status(false).expect("a read");
                let known = pitboard.status_offline().expect("a read");

                assert!(everything(&m) == before, "{at}: nothing is written");
                assert_eq!(m.api.calls(), asked, "{at}: nobody is asked");
                assert_eq!(rows(&read.value), rows(&known.value), "{at}");
                let codes: Vec<&str> = read.warnings.iter().map(Warning::code).collect();
                assert_eq!(codes.last(), Some(&"read_only"), "{at}");
                assert_eq!(read.warnings.len(), known.warnings.len() + 1, "{at}");
                let said = read.warnings.last().expect("said").to_string();
                let words = refused.words();
                assert!(
                    said.starts_with(&format!("{}, so it changes nothing", words.because)),
                    "{at}: {said}"
                );
                assert!(said.ends_with(words.to_ask_again), "{at}: {said}");

                Refused::lifted(&m);
                let live = pitboard.status(false).expect("a read");
                assert!(m.api.calls() > asked, "{at}: as the person, it asks");
                assert!(
                    !live.warnings.iter().any(|w| w.code() == "read_only"),
                    "{at}"
                );
            }
        }
    }

    /// The status line kept what each session passed it and the readings it moved, as root
    /// too. Where this process may change nothing it draws the same line from the files as
    /// they are, and writes neither.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn the_status_line_where_pitboard_may_change_nothing_draws_the_same_line_and_writes_nothing() {
        let session = serde_json::json!({
            "session_id": "a-session",
            "rate_limits": {
                "five_hour": {"used_percentage": 40.0, "resets_at": crate::switch::harness::NOW + 3_600},
                "seven_day": {"used_percentage": 20.0, "resets_at": crate::switch::harness::NOW + 86_400},
            },
        })
        .to_string();
        for refused in REFUSED {
            let m = machine("gate-statusline");
            let pitboard = Pitboard::new(m.ctx.clone());
            refused.said_by(&m);
            let before = everything(&m);

            let drawn = pitboard.statusline(&session);

            assert!(everything(&m) == before, "{refused:?}: nothing is written");
            Refused::lifted(&m);
            assert_eq!(pitboard.statusline(&session), drawn, "{refused:?}");
            assert!(
                crate::home::dir(&m.ctx).join("sessions.json").exists(),
                "{refused:?}: as the person, the session is kept"
            );
        }
    }

    /// `renew` returned an empty list when it renewed nothing, whatever the reason, so a
    /// run that could not renew read as one where nothing was due. Refused, it says so, and
    /// asks nobody, with a park that is due to be renewed.
    #[test]
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
    fn a_renewal_where_pitboard_may_change_nothing_is_refused_and_asks_nobody() {
        for (tool, make) in MACHINES {
            for refused in REFUSED {
                let at = format!("{tool}, {refused:?}");
                let m = make(&format!("gate-renew-{tool}"));
                crate::switch::harness::renews(&m, "there-refresh", "there-refresh-2");
                let later = m
                    .ctx
                    .clone()
                    .with_clock(Arc::new(crate::time::FixedClock::at(
                        crate::switch::harness::NOW + 11 * 86_400,
                    )));
                let pitboard = Pitboard::new(later);
                refused.said_by(&m);
                let (before, asked) = (everything(&m), m.api.calls());

                let refusal = pitboard.renew().expect_err("refused");

                assert_eq!(refusal.code(), refused.code(), "{at}");
                assert!(everything(&m) == before, "{at}: nothing changes");
                assert_eq!(m.api.calls(), asked, "{at}: nobody is asked");

                Refused::lifted(&m);
                let renewed = pitboard.renew().expect("as the person, it renews");
                let codes: Vec<&str> = renewed.iter().map(|(_, r)| r.code()).collect();
                assert_eq!(codes, ["renewed"], "{at}");
            }
        }
    }

    /// `doctor` says why every change is refused, first, as a check that fails, and says
    /// nothing of it where Pitboard runs as the person.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn doctor_says_why_pitboard_changes_nothing() {
        let m = machine("gate-doctor");
        let pitboard = Pitboard::new(m.ctx.clone());
        let refusing = |checks: &[doctor::Check]| -> Vec<&'static str> {
            checks
                .iter()
                .map(|c| c.code)
                .filter(|code| ["elevated", "system_too_old"].contains(code))
                .collect()
        };
        assert!(
            refusing(&pitboard.doctor().checks).is_empty(),
            "as the person"
        );
        for refused in REFUSED {
            refused.said_by(&m);
            let checks = pitboard.doctor().checks;
            let check = checks.first().expect("checks");
            let words = refused.words();
            assert_eq!(check.code, refused.code(), "{refused:?}");
            assert_eq!(check.level, doctor::Level::Fail, "{refused:?}");
            assert_eq!(check.detail, words.because, "{refused:?}");
            assert!(
                check.advice.starts_with("Pitboard changes nothing ")
                    && check.advice.ends_with(words.way_out),
                "{refused:?}: {}",
                check.advice
            );
            Refused::lifted(&m);
        }
        m.mem.runs_on(Floor::Below { build: 22631 });
        m.mem.runs_with(Elevation::Unknown);
        assert_eq!(
            refusing(&pitboard.doctor().checks),
            ["system_too_old", "elevated"]
        );
    }
}
