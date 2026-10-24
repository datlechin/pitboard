//! Pitboard's operations, each run the way every front end must run it: a change settles any
//! interrupted switch first and is recorded in the audit log, and what went wrong on the way
//! is reported alongside the result, whether or not the change then succeeds.

use crate::context::Context;
use crate::doctor::{self, Diagnosis};
use crate::duty::autoswitch::{Auto, Look, Threshold};
use crate::error::{Error, Result};
use crate::holder;
use crate::host::{Elevation, Floor, Os};
use crate::in_use::{self, Doubt, InUse, Known};
use crate::provider::{Held, LoginKind, Provider, ProviderId};
use crate::state::{self, Account, Key};
use crate::switch::{self, Enrolled, Outcome, Recovered, Renewal, Settled, SignIn};
use crate::usage::readings;
use crate::words::capitalised;
use crate::{audit, schedule, status, statusline};
use std::fmt;

/// Proof that this process may change something on this machine, which only the one gate
/// every change passes makes ([`Pitboard::permit`]).
///
/// Everything that changes anything takes one as an argument: every write of a file through
/// `atomic`, every private file or directory made and every one removed, a write to or a
/// delete from any store of logins, the keychain among them, a change to the system's
/// scheduler, a tool's sign-in or warm-up run started, and the token exchange that renews a
/// parked login. So a change that did not ask the gate does not compile, and nothing
/// rotates a refresh token it could not then write down.
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
/// host says. A process the host says is not the person themselves
/// ([`crate::host::Host::elevation`]) changes nothing, and nor does one the host cannot place:
/// a file it wrote would be root's or the administrators', and a keychain item might be, where
/// the person's own runs might never read, replace or remove it again.
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
    /// The tool's write lock, or its refresh lock, stopped being Pitboard's while a change was
    /// under way.
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
        holding: Vec<holder::Holding>,
    },
    /// A sign-in put a new login in use in place of the old one of the same account, and
    /// sessions of a tool that never reads its login again were running with the old one.
    SessionsKeepTheOldLogin {
        label: String,
        holding: Vec<holder::Holding>,
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
    /// A sign-in was parked rather than put in use, because nobody could say whose login the
    /// tool has in use, for `why`.
    SignInParkedNotInUse {
        tool: ProviderId,
        label: String,
        why: String,
        spelling: crate::words::Spelling,
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
    /// The tool's own record names `named` since its service last named the login stored,
    /// `last`'s, as a sign-in leaves it and as another process starting on another login
    /// can: another login may be stored. Said by a read that could not ask whose it is.
    /// `asked` is false for a record brought forward from an earlier Pitboard, whose `last`
    /// is the account it last switched to, which the service never named.
    InUseUnconfirmed {
        tool: ProviderId,
        last: Stored,
        named: Stored,
        asked: bool,
    },
    /// The tool's own record names `config`, and the login stored is `in_use`'s, as its
    /// service said: what `/status` in Claude Code shows is not the account in use. Using
    /// `in_use` writes it there again.
    ConfigNamesAnother {
        tool: ProviderId,
        config: Stored,
        in_use: Stored,
    },
    // A login filed under the account the app names: one Pitboard had not seen, or one it had
    // recorded as `recorded`'s.
    TakenAsNamed {
        tool: ProviderId,
        label: String,
        recorded: Option<String>,
    },
    // A folder kept under `parks`, deleting nothing.
    SetAside {
        tool: ProviderId,
        path: std::path::PathBuf,
        label: Option<String>,
        why: state::SetAside,
    },
    // A folder kept under `parks` that holds only the keys `label` had before the app rewrote
    // them, deleting nothing.
    KeysSetAside {
        tool: ProviderId,
        path: std::path::PathBuf,
        label: Option<String>,
    },
    // The app's settings say what a Log out leaves since the account was read in use.
    LooksSignedOut {
        tool: ProviderId,
        label: String,
    },
    // A move an earlier run left was settled before this change.
    MoveSettled(switch::MoveSettled),
    // A move waits in the index: the next change to the tool settles it first.
    MoveWaiting {
        tool: ProviderId,
        label: String,
    },
    // `abandon` gave up a switch, then could not give up this move, for `why`.
    MoveNotGivenUp {
        tool: ProviderId,
        label: String,
        why: Error,
    },
    // `repair` could not settle this move, for `why`, and repaired the rest.
    MoveNotSettled {
        tool: ProviderId,
        label: String,
        why: Error,
    },
    // The app is signed out for a sign-in inside it, with `put_back` parked. Labels as typed.
    SignInAwaited {
        tool: ProviderId,
        label: String,
        put_back: String,
        since: i64,
        // a Sign In Again of `label`, not an add
        again: bool,
    },
    // An item of the login was in neither place after its move, which went on without it.
    ItemGone {
        tool: ProviderId,
        label: String,
        item: String,
    },
    /// This process may change nothing, because the host says it is not the person themselves
    /// (`Host::elevation`), or nobody could tell whether it is, so a read answers from what
    /// Pitboard last measured: it renews nothing, asks nobody and writes nothing. `why` is how it
    /// runs, as the host said it, and `None` where the host could not say.
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
            Warning::InUseUnconfirmed { .. } => "in_use_unconfirmed",
            Warning::ConfigNamesAnother { .. } => "config_names_another",
            Warning::TakenAsNamed { .. } => "login_taken_as_named",
            Warning::SetAside { .. } | Warning::KeysSetAside { .. } => "set_aside",
            Warning::LooksSignedOut { .. } => "looks_signed_out",
            Warning::MoveSettled(moved) => match moved.ended {
                switch::Ended::Undone => "interrupted_move_undone",
                switch::Ended::Finished => "interrupted_move_finished",
            },
            Warning::MoveWaiting { .. } => "move_waiting",
            Warning::MoveNotGivenUp { .. } => "move_not_given_up",
            Warning::MoveNotSettled { .. } => "move_not_settled",
            Warning::SignInAwaited { .. } => "sign_in_awaited",
            Warning::ItemGone { .. } => "login_item_gone",
            Warning::ReadOnly { .. } | Warning::ReadOnlyBelowTheFloor { .. } => "read_only",
        }
    }

    pub fn tool(&self) -> Option<ProviderId> {
        match self {
            Warning::ParkedLoginRefused { tool, .. }
            | Warning::LockCompromised { tool }
            | Warning::AuthOverridden { tool, .. }
            | Warning::FallbackLogin { tool, .. }
            | Warning::WrittenOnTheCommandLine { tool, .. }
            | Warning::SessionsUnknown { tool, .. }
            | Warning::SessionsUnknownAfterSignIn { tool, .. }
            | Warning::SignInParkedNotInUse { tool, .. }
            | Warning::LoginReplaced { tool, .. }
            | Warning::InUseUnconfirmed { tool, .. }
            | Warning::ConfigNamesAnother { tool, .. }
            | Warning::TakenAsNamed { tool, .. }
            | Warning::SetAside { tool, .. }
            | Warning::KeysSetAside { tool, .. }
            | Warning::LooksSignedOut { tool, .. }
            | Warning::MoveWaiting { tool, .. }
            | Warning::MoveNotGivenUp { tool, .. }
            | Warning::MoveNotSettled { tool, .. }
            | Warning::SignInAwaited { tool, .. }
            | Warning::ItemGone { tool, .. } => Some(*tool),
            Warning::MoveSettled(moved) => Some(moved.tool),
            Warning::Recovered(_)
            | Warning::ConfigNotUpdated(_)
            | Warning::ParksPendingRemoval(_)
            | Warning::RenewalFailed(_)
            | Warning::SessionsStillRunning { .. }
            | Warning::SessionsKeepTheOldLogin { .. }
            | Warning::SwitchStuck(_)
            | Warning::ReadOnly { .. }
            | Warning::ReadOnlyBelowTheFloor { .. } => None,
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Warning::Recovered(r) => write!(f, "{r}"),
            Warning::LockCompromised { tool } => write!(
                f,
                "{} reclaimed a credential lock while this change was under way, so it may \
                 have written or renewed a login at the same time. Pitboard read the slot back \
                 and the change stood, but check with `pitboard` that the right account is \
                 signed in. A login Pitboard parked meanwhile may need signing in to again \
                 when it is next used.",
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
                "{path} holds another {tool} login, which {reader} signs in with. No switch \
                 reaches it. While it is there, {tool} sessions already running at a switch \
                 {running}. `pitboard stow` keeps that login for its account, where Pitboard \
                 holds no other it can switch to, then deletes the file.",
                path = path.display(),
                tool = tool.name(),
                reader = crate::words::reads_only_the_file(crate::host::OS),
                running = crate::words::kept_until_renewed(),
            ),
            Warning::FallbackLogin {
                tool,
                path,
                held: Held::NoLogin,
            } => write!(
                f,
                "{path} is there with no {tool} login in it. While it is, {tool} sessions \
                 already running at a switch {running}. `pitboard stow` deletes it, which lets \
                 them follow a switch.",
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
            Warning::SignInParkedNotInUse {
                tool,
                label,
                why,
                spelling,
            } => write!(
                f,
                "{} goes on with the login it has: Pitboard could not tell whose it is \
                 ({why}), so it parked the new login for `{label}` rather than write over \
                 that one. If that login no longer works, {} and sign in to `{label}` \
                 there.",
                tool.name(),
                crate::words::sign_in_yourself(*tool, spelling.of(*tool), false)
            ),
            Warning::LoginReplaced {
                tool, label, now, ..
            } => match (tool, now) {
                (ProviderId::Desktop, Stored::Nothing) => write!(
                    f,
                    "`{label}` was signed out in Claude, which ends its login at claude.ai, and \
                     Pitboard holds no login for it. Run `pitboard enroll {label} --sign-in` to \
                     sign in to it again."
                ),
                (ProviderId::Claude | ProviderId::Codex | ProviderId::Desktop, now) => {
                    write!(
                        f,
                        "`{label}`'s login was replaced by a sign-in outside Pitboard"
                    )?;
                    let tool = tool.name();
                    match now {
                        Stored::Account(now) => {
                            write!(f, ": {tool} now has `{now}`'s login stored,")?;
                        }
                        Stored::Unenrolled(email) if email.is_empty() => write!(
                            f,
                            ": {tool} now has the login of an account Pitboard has not enrolled,"
                        )?,
                        Stored::Unenrolled(email) => {
                            write!(f, ": {tool} now has {email}'s login stored,")?;
                        }
                        Stored::Nothing => write!(f, ": {tool} now has no login stored,")?,
                        Stored::Unknown => write!(f, ",")?,
                    }
                    write!(
                        f,
                        " and Pitboard holds no login for `{label}`. Run `pitboard enroll \
                         {label} --sign-in` to sign in to it again."
                    )
                }
            },
            Warning::InUseUnconfirmed {
                tool,
                last,
                named,
                asked,
            } => match tool {
                ProviderId::Desktop => desktop_named(f, named, last),
                ProviderId::Claude | ProviderId::Codex if !asked => write!(
                    f,
                    "{name}'s config names {named}, and Pitboard has not asked {service} whose \
                     login {name} has stored since Pitboard was updated, so another login may be \
                     stored. The account it last switched to is {last}. `pitboard status` asks.",
                    name = tool.name(),
                    named = named.said("no account"),
                    service = tool.service(),
                    last = last.said("none"),
                ),
                ProviderId::Claude | ProviderId::Codex => {
                    write!(
                        f,
                        "{name}'s config has named {named} since Pitboard last asked {service} \
                         whose login {name} has stored, so another login may be stored.",
                        name = tool.name(),
                        named = named.said("no account"),
                        service = tool.service(),
                    )?;
                    match last {
                        Stored::Account(_) | Stored::Unenrolled(_) => {
                            write!(f, " The one stored then was {}'s.", last.said(""))?;
                        }
                        Stored::Nothing => write!(f, " None was stored then.")?,
                        Stored::Unknown => {}
                    }
                    write!(f, " `pitboard status` asks again.")
                }
            },
            Warning::ConfigNamesAnother {
                tool,
                config,
                in_use,
            } => match tool {
                ProviderId::Desktop => desktop_named(f, config, in_use),
                ProviderId::Claude | ProviderId::Codex => {
                    let config = config.said("no account");
                    write!(
                        f,
                        "{name}'s config names {config}, and the login {name} has stored is \
                         {}'s, as {} said. `/status` in {name} shows {config}.",
                        in_use.said("nobody"),
                        tool.service(),
                        name = tool.name(),
                    )?;
                    write!(f, " {}", in_use.how_to_name())
                }
            },
            Warning::TakenAsNamed {
                label,
                recorded: None,
                ..
            } => write!(
                f,
                "Pitboard had not seen the login Claude held, and took it as `{label}`'s because \
                 Claude names that account. If Claude showed another account, run `pitboard use \
                 {label}`, check in Claude, quit it, and enrol it under its own name."
            ),
            Warning::TakenAsNamed {
                label,
                recorded: Some(recorded),
                ..
            } => write!(
                f,
                "Pitboard had recorded the login Claude held as `{recorded}`'s, and filed it as \
                 `{label}`'s because Claude names that account. Open Claude and check which \
                 account it shows. If it is another, quit Claude with Command-Q, then run \
                 `pitboard enroll desktop/<label>` under that account's name."
            ),
            Warning::SetAside {
                path, label, why, ..
            } => {
                let path = path.display();
                match (why, label) {
                    (state::SetAside::Superseded, Some(label)) => write!(
                        f,
                        "Pitboard kept `{label}`'s earlier parked login in {path} and deleted \
                         nothing. `pitboard doctor` lists it."
                    ),
                    (state::SetAside::Superseded, None) => write!(
                        f,
                        "Pitboard kept an earlier parked login in {path} and deleted nothing. \
                         `pitboard doctor` lists it."
                    ),
                    (state::SetAside::GivenUp, Some(label)) => write!(
                        f,
                        "Pitboard moved the parts of `{label}`'s login left in Claude's folder \
                         into {path} and deleted nothing. `pitboard doctor` lists them."
                    ),
                    (state::SetAside::GivenUp, None) => write!(
                        f,
                        "Pitboard moved the parts of a login left in Claude's folder into {path} \
                         and deleted nothing. `pitboard doctor` lists them."
                    ),
                    (
                        state::SetAside::InTheWay
                        | state::SetAside::AnotherMachine
                        | state::SetAside::Orphan,
                        _,
                    ) => write!(
                        f,
                        "Pitboard moved what it found in the way into {path} and deleted nothing."
                    ),
                }
            }
            Warning::KeysSetAside { path, label, .. } => {
                let whose = label
                    .as_ref()
                    .map_or_else(|| "a login".to_string(), |label| format!("`{label}`"));
                write!(
                    f,
                    "Claude rewrote {whose}'s keys. Pitboard kept the earlier ones in {} and \
                     deleted nothing. `pitboard doctor` lists them.",
                    path.display()
                )
            }
            Warning::LooksSignedOut { label, .. } => write!(
                f,
                "Claude looks signed out since Pitboard last read `{label}` in use. The next \
                 change to a Claude Desktop account checks."
            ),
            Warning::MoveSettled(moved) => {
                let label = Key::new(moved.tool, moved.label.clone()).typed();
                f.write_str(&crate::words::move_settled(&label, moved.half, moved.ended))
            }
            Warning::MoveWaiting { label, .. } => f.write_str(&crate::words::move_waiting(label)),
            Warning::MoveNotGivenUp { label, why, .. } => {
                f.write_str(&crate::words::move_not_given_up(label, why))
            }
            Warning::MoveNotSettled { label, why, .. } => {
                f.write_str(&crate::words::move_not_settled(label, why))
            }
            Warning::SignInAwaited {
                label,
                put_back,
                again,
                ..
            } => f.write_str(&crate::words::sign_in_awaited(label, put_back, *again)),
            Warning::ItemGone { label, item, .. } => {
                f.write_str(&crate::words::item_gone(label, item))
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

/// An account as a warning or a doctor check names it: whose login a tool has stored, as
/// Pitboard last recorded it, or whom the tool's own record names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    /// An enrolled account's, by the name to type for it.
    Account(String),
    /// An account nobody enrolled, by its email.
    Unenrolled(String),
    /// None.
    Nothing,
    /// Nothing is recorded of the tool's store here.
    Unknown,
}

impl Stored {
    /// `owner`'s account of `tool`, or none.
    pub(crate) fn of(
        state: &state::State,
        tool: ProviderId,
        owner: Option<&crate::provider::Owner>,
    ) -> Stored {
        match owner {
            None => Stored::Nothing,
            Some(owner) => state.account_of(tool, owner).map_or_else(
                || Stored::Unenrolled(owner.email.clone()),
                |account| Stored::Account(state.typed(&account.key())),
            ),
        }
    }

    /// How a sentence names it: the name to type, in backticks, the email, or `nobody`.
    pub(crate) fn said(&self, nobody: &str) -> String {
        match self {
            Stored::Account(name) => format!("`{name}`"),
            Stored::Unenrolled(email) if email.is_empty() => {
                "an account Pitboard has not enrolled".into()
            }
            Stored::Unenrolled(email) => email.clone(),
            Stored::Nothing | Stored::Unknown => nobody.to_string(),
        }
    }

    /// The sentence that says what writes this account, whose login the tool has stored, into
    /// the tool's own record again: `pitboard use` where it is enrolled, and otherwise the
    /// next switch.
    pub(crate) fn how_to_name(&self) -> String {
        match self {
            Stored::Account(name) => {
                format!("`pitboard use {name}` writes `{name}` into the config.")
            }
            _ => "A switch writes the account it puts in use into the config.".into(),
        }
    }
}

/// Each account whose only login a sign-in outside Pitboard replaced, from Pitboard's own
/// record alone.
fn replaced(state: &state::State) -> Vec<Warning> {
    state
        .accounts
        .iter()
        .filter(|account| account.replaced_at.is_some() && account.parked.is_none())
        .map(|account| {
            let tool = account.provider();
            let now = state.in_use(tool).map_or(Stored::Unknown, |record| {
                Stored::of(state, tool, record.owner.as_ref())
            });
            Warning::LoginReplaced {
                tool,
                id: account.id.clone(),
                label: state.typed(&account.key()),
                now,
            }
        })
        .collect()
}

fn in_doubt(state: &state::State, tool: ProviderId, known: &Known) -> Option<Warning> {
    let named = Stored::of(state, tool, known.own_record.as_ref());
    if let Some(doubt) = known.unconfirmed() {
        return Some(Warning::InUseUnconfirmed {
            tool,
            last: Stored::of(state, tool, known.owner()),
            named,
            asked: doubt == Doubt::NamedMoved,
        });
    }
    known
        .named_another(tool)
        .map(|_| Warning::ConfigNamesAnother {
            tool,
            config: named,
            in_use: Stored::of(state, tool, known.owner()),
        })
}

fn desktop_named(f: &mut fmt::Formatter<'_>, named: &Stored, last: &Stored) -> fmt::Result {
    write!(
        f,
        "{}. Open Claude, check which account it shows, quit it with Command-Q, then run \
         `pitboard enroll desktop/<label>` under that account's name.",
        crate::words::desktop_named(named, last, |stored| stored.said("nobody"))
    )
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

// The sign-in a read says is waited for, with the account parked for it.
fn sign_in_awaited(state: &state::State, tool: ProviderId) -> Option<Warning> {
    let awaited = state.awaiting(tool)?;
    let put_back = state.by_id(tool, &awaited.put_back)?.key();
    Some(Warning::SignInAwaited {
        tool,
        label: state.typed(&Key::new(tool, awaited.label.clone())),
        put_back: state.typed(&put_back),
        since: awaited.since,
        again: awaited.account.is_some(),
    })
}

/// A change that failed, with what was found on the way: recovering an interrupted switch
/// is reported even when the change that followed it fails.
#[derive(Debug)]
pub struct Failed {
    pub error: Error,
    pub warnings: Vec<Warning>,
    // how many of `warnings`, from the first, were done before the refusal
    pub said_first: usize,
}

impl Failed {
    // A change's refusal beside what was `found` before it ran. What was done first leads: an
    // interrupted switch settled before the change, then what the change did.
    fn of(mut error: Error, found: Vec<Warning>) -> Failed {
        let (mut warnings, found): (Vec<Warning>, Vec<Warning>) = found
            .into_iter()
            .partition(|warning| matches!(warning, Warning::Recovered(_)));
        warnings.extend(error.take_said_first());
        let said_first = warnings.len();
        warnings.extend(found);
        warnings.extend(error.take_warnings());
        Failed {
            error,
            warnings,
            said_first,
        }
    }
}

pub type Changing<T> = std::result::Result<Done<T>, Failed>;

#[derive(Debug)]
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

    // None where this process may change nothing here, or another app holds it.
    pub fn hold_open(&self) -> Option<crate::app::Open> {
        crate::app::Open::hold(&self.ctx, self.permit().ok()?)
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
            said_first: 0,
        })
    }

    /// Who is signed in and what every account has left. Parked logins whose access has
    /// lapsed are renewed first, so every account is asked live, and so is a tool's stored
    /// login where only its service can tell whose it is
    /// ([`switch::identify::told_by_a_read`]).
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
    /// Pitboard that replaced the only login of an account is said from that read on. Where
    /// Claude Code's config names another account than that login's, that is said too, since
    /// `/status` in Claude Code shows the config's.
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
            let answers = status::ask(&self.ctx, permit, &state, fresh);
            (stuck.join().ok().flatten(), answers)
        });
        switch::identify::record_read(&self.ctx, permit, &answers.found, &answers.plans);
        let found = answers.found.clone();
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
        warnings.extend(self.standing(&state, &found));
        Ok(Done {
            value: report,
            warnings,
        })
    }

    /// What a read says for as long as it stands: each account whose only login a sign-in
    /// outside Pitboard replaced, and, of a tool that keeps a record of its own apart from its
    /// login, that the record has moved since its service last named the login stored, or
    /// that it names another account than that login's.
    ///
    /// `found` is whose login each tool's store holds as this read just learned it, which a
    /// read that asks nobody has none of. For every other tool, what Pitboard recorded
    /// stands, so a read that could not ask says what one that asks nobody says.
    fn standing(&self, state: &state::State, found: &[(ProviderId, InUse)]) -> Vec<Warning> {
        let mut warnings = replaced(state);
        // So a Mac with no Claude Desktop account enrolled reads nothing of Claude.
        for &tool in ProviderId::ALL
            .iter()
            .filter(|&&tool| status::shown(state, tool))
        {
            let (known, signed_out) = match crate::provider::of(tool) {
                LoginKind::Document(driver) if driver.identifies_by_itself() => continue,
                LoginKind::Document(_) => {
                    let known = match found.iter().find(|(which, _)| *which == tool) {
                        Some((_, found)) => in_use::judged(&self.ctx, tool, Some(found.clone())),
                        None => in_use::known(&self.ctx, state, tool),
                    };
                    (known, None)
                }
                LoginKind::Folder(app) => {
                    warnings.extend(sign_in_awaited(state, tool));
                    // A move under way leaves the settings between two accounts: only it is said.
                    if let Some(moving) = state.moving(tool) {
                        warnings.push(Warning::MoveWaiting {
                            tool,
                            label: state.typed(&Key::new(tool, moving.label.clone())),
                        });
                        continue;
                    }
                    // No read learns whose an app's login is, so `found` names none.
                    switch::folder::read(&self.ctx, state, app)
                }
            };
            warnings.extend(in_doubt(state, tool, &known));
            warnings.extend(signed_out.map(|key| Warning::LooksSignedOut {
                tool,
                label: state.typed(&key),
            }));
        }
        warnings
    }

    pub fn doctor(&self) -> Diagnosis {
        self.doctor_with(&crate::time::moment)
    }

    /// The same checks, each moment one names written as `moment` writes it, given the moment
    /// and when the checks are made: an app's clock, where the command line's is
    /// [`crate::time::moment`].
    pub fn doctor_with(&self, moment: &dyn Fn(i64, i64) -> String) -> Diagnosis {
        doctor::run(&self.ctx, moment)
    }

    /// The same report without asking anyone: the last numbers Pitboard measured, and whose
    /// login each tool has stored as its service last said. Nothing is renewed and nothing is
    /// asked, so it answers at once wherever there is no network. Where Claude Code's config
    /// has named another account since, as a sign-in leaves it, that is said, and a read that
    /// asks settles it.
    ///
    /// An interrupted switch the next change cannot finish is said here too, as [`status`]
    /// says it, wherever that can be told without a request: a Claude Code login renewed
    /// since the switch stopped is one only Anthropic can say whose it is, and then nothing
    /// is said. Telling reads the tool's login and the copy the switch parked, which on
    /// macOS are keychain items, and only while a switch is waiting: otherwise one look at
    /// whether its record is there is the whole of it. So nothing is said of a file behind
    /// the keychain, which only a read of the keychain can tell. What Pitboard's own record
    /// holds is said as [`status`] says it: an account whose only login was replaced outside
    /// Pitboard, and Claude Code's config naming another account than the one in use.
    ///
    /// [`status`]: Pitboard::status
    pub fn status_offline(&self) -> Result<Done<status::Report>> {
        let state = state::load(&self.ctx)?;
        let warnings = switch::stuck(&self.ctx, &state, switch::Asking::Nobody)
            .map(Warning::SwitchStuck)
            .into_iter()
            .chain(self.standing(&state, &[]))
            .collect();
        Ok(Done {
            value: status::gather_offline(&self.ctx, &state),
            warnings,
        })
    }

    /// The status line for Claude Code's session JSON. Reads only files, and writes only
    /// Pitboard's own: what the session passed, for its next run to compare with, and the
    /// usage readings, which take what moved since its last run for the account its windows
    /// prove it is on, where it moves a limit Anthropic gave that account. Where this process
    /// may change nothing it writes neither, and draws the line from the files as they are.
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

    /// What [`switch_to`] does once somebody has agreed to put `typed`'s login in place of a
    /// login in use its service refuses for good, which it refused over: parks nothing.
    /// Where the login stored can be told by then, it is a switch.
    ///
    /// [`switch_to`]: Pitboard::switch_to
    pub fn replace_with(&self, typed: &str) -> Changing<Outcome> {
        let permit = self.permitted()?;
        let key = self.named(permit, "use", typed)?;
        self.changing(permit, "use", &key.typed(), Some(key.provider), |settled| {
            switch::replace(settled, &key)
        })
    }

    /// What [`switch_to`] does for the account in use, for a front end's button drawn while
    /// `typed` was: recorded as `use`, and refused, moving nothing, where another account's
    /// login is stored by then.
    ///
    /// [`switch_to`]: Pitboard::switch_to
    pub fn update_config(&self, typed: &str) -> Changing<Outcome> {
        let permit = self.permitted()?;
        let key = self.named(permit, "use", typed)?;
        self.changing(permit, "use", &key.typed(), Some(key.provider), |settled| {
            switch::update_config(settled, &key)
        })
    }

    /// Switches Claude Code by itself where a limit of the account in use has reached
    /// `threshold` and another account has room, as [`crate::duty::autoswitch`] says. For a front
    /// end somebody asked to do that: the app with its setting on, or `pitboard watch`.
    ///
    /// Looks first from files alone ([`auto_look`]), so a look that finds nothing to do takes
    /// no lock, asks nobody and records nothing. Under the lock whose login Claude Code has
    /// stored is told, and the switch decided once, as [`switch::automatically`] says, which
    /// records in the audit log what it came to. A settle that failed first is recorded as
    /// `auto-switch` of `claude`.
    ///
    /// [`auto_look`]: Pitboard::auto_look
    pub fn auto_switch(&self, threshold: Threshold) -> Changing<Auto> {
        let permit = self.permitted()?;
        let looked = self.auto_look(threshold).map_err(|error| Failed {
            error,
            warnings: Vec::new(),
            said_first: 0,
        })?;
        if let Look::Stands(value) = looked {
            return Ok(Done {
                value: *value,
                warnings: Vec::new(),
            });
        }
        let tool = ProviderId::Claude;
        let (settled, mut warnings) = self.settled_for(permit, "auto-switch", "", Some(tool))?;
        match switch::automatically(settled, threshold) {
            Ok((value, more)) => {
                warnings.extend(more);
                Ok(Done { value, warnings })
            }
            Err(error) => Err(Failed::of(error, warnings)),
        }
    }

    /// What switching Claude Code by itself would come to now, from files alone: what
    /// stands, or that only a decision under the lock can say, which [`auto_switch`] makes.
    /// Waits on no lock, reads no keychain and asks nobody, so a front end may look as often
    /// as it likes. While another run is in the middle of a switch, only the decision can
    /// say, which waits for that run.
    ///
    /// [`auto_switch`]: Pitboard::auto_switch
    pub fn auto_look(&self, threshold: Threshold) -> Result<Look> {
        let state = state::load(&self.ctx)?;
        Ok(crate::duty::autoswitch::look(&self.ctx, &state, threshold))
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
            said_first: 0,
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
        Failed::of(error, warnings)
    }

    // An account enrolled already keeps the name it has, which a new one may no longer get:
    // 0.1.x wrote labels with a slash, and the app took labels with a space.
    fn enrolling(&self, typed: &str) -> Result<Key> {
        if let Ok(state) = state::load(&self.ctx) {
            let whole = state
                .accounts
                .iter()
                .find(|a| typed.contains(crate::label::SEPARATOR) && a.label == typed);
            let named = match crate::label::parse(typed) {
                Ok(crate::label::Spec::Bare(label)) => {
                    state.get(&Key::new(crate::label::DEFAULT, label))
                }
                Ok(crate::label::Spec::Qualified(provider, label)) => {
                    state.get(&Key::new(provider, label))
                }
                Err(_) => None,
            };
            if let Some(existing) = whole.or(named) {
                return Ok(existing.key());
            }
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

    // What the tool already runs, for a front end to print before a program sign-in, since the
    // core prints nothing. `None` off Windows, and for a tool with no holders.
    pub fn running_before_sign_in(&self, typed: &str) -> Option<String> {
        let which = crate::label::choose(typed)
            .map(|chosen| chosen.provider)
            .unwrap_or(crate::label::DEFAULT);
        switch::running_already(&self.ctx, which)
    }

    // A program's sign-in runs in a private directory and takes no lock but its own.
    pub fn sign_in(&self, typed: &str) -> std::result::Result<SignIn, Failed> {
        let permit = self.permitted()?;
        let key = self.chosen(permit, "enroll", typed)?;
        let driver = match crate::provider::of(key.provider) {
            LoginKind::Document(driver) => driver,
            // The wait is the core's, and it prints nothing: a front end reads `said`.
            LoginKind::Folder(_) => {
                return self
                    .sign_in_watched(typed)?
                    .finish()
                    .map_err(|error| self.not_started(permit, typed, error));
            }
        };
        self.ready_to_sign_in(driver)
            .and_then(|()| switch::sign_in(&self.ctx, permit, driver))
            .map_err(|error| self.not_started(permit, typed, error))
    }

    // Watched: the caller shows what is said, and types a code back where the tool reads one.
    pub fn sign_in_watched(
        &self,
        typed: &str,
    ) -> std::result::Result<switch::WatchedSignIn, Failed> {
        let permit = self.permitted()?;
        let key = self.chosen(permit, "enroll", typed)?;
        let driver = match crate::provider::of(key.provider) {
            LoginKind::Document(driver) => driver,
            // Inside the app: the account in use is parked, then a thread waits for the app
            // to quit signed in. `signin.lock` is taken before `state.lock`, as every sign-in's.
            LoginKind::Folder(app) => {
                let one_at_a_time = switch::one_at_a_time(&self.ctx, permit)
                    .map_err(|error| self.not_started(permit, typed, error))?;
                let subject = key.typed();
                let settling = self.settled_for(permit, "park", &subject, Some(key.provider))?;
                // A sign-in returns no warnings, so what settling found is said first.
                let first = settling
                    .1
                    .iter()
                    .map(|warning| format!("{warning}\n"))
                    .collect();
                return self
                    .change(permit, "park", &subject, settling, |settled| {
                        switch::folder::awaited::start(settled, &key, app, one_at_a_time, first)
                    })
                    .map(|started| started.value);
            }
        };
        self.ready_to_sign_in(driver)
            .and_then(|()| switch::sign_in_watched_as(&self.ctx, permit, driver, &key))
            .map_err(|error| self.not_started(permit, typed, error))
    }

    // Asks nobody: None for a login that does not say whose it is, as Claude Code's does not.
    pub fn signed_in_email(&self, which: ProviderId) -> Option<String> {
        let LoginKind::Document(tool) = crate::provider::of(which) else {
            return None;
        };
        if !tool.identifies_by_itself() {
            return None;
        }
        let credential = tool.read_live(&self.ctx).ok()??;
        let owner = tool.identify(&self.ctx, &credential).ok()?;
        Some(owner.email).filter(|email| !email.is_empty())
    }

    // The tools a sign-in could start here now, by the rule `ready_to_sign_in` keeps.
    pub fn installed(&self) -> Vec<ProviderId> {
        crate::provider::installed_among(&self.ctx, crate::host::proc::can_run)
    }

    /// A sign-in that did not start, or did not finish, for a reason other than its name.
    fn not_started(&self, permit: Permit, typed: &str, error: Error) -> Failed {
        audit::record(&self.ctx, permit, "enroll", typed, error.code());
        Failed {
            error,
            warnings: Vec::new(),
            said_first: 0,
        }
    }

    /// Checked before a sign-in starts, so a person does not sign in through a browser
    /// only to be told the state file belongs to another machine, that the tool is not
    /// installed, or that the account could never be switched to afterwards.
    fn ready_to_sign_in(&self, driver: &'static dyn Provider) -> Result<()> {
        let tool = driver.id();
        if tool == ProviderId::Claude && self.ctx.custom_oauth() {
            return Err(Error::CustomOauthEndpoint);
        }
        state::load(&self.ctx)?;
        // An account signed in here is one to switch to later, which needs a live store
        // Pitboard can write. Asked now rather than after a browser round trip.
        switch::live_store(&self.ctx, driver)?;
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
                program: self
                    .ctx
                    .program_for(tool)
                    .unwrap_or(std::path::Path::new(tool.program()))
                    .display()
                    .to_string(),
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

    // The way out of a switch recovery cannot finish and of a move it cannot settle.
    pub fn abandon_recovery(&self) -> Result<switch::GaveUp> {
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
        let ctx = schedule::for_its_run(&self.ctx, schedule::Job::Renew);
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
    pub fn schedule_install(&self) -> Result<schedule::Location> {
        schedule::install(&self.ctx, self.permit()?)
    }

    /// Take it away. `false` when there was nothing installed.
    pub fn schedule_uninstall(&self) -> Result<bool> {
        schedule::uninstall(&self.ctx, self.permit()?)
    }

    // Each job that runs the command line this context names, the app's own; `false` where none.
    pub fn schedule_uninstall_for_app(&self) -> Result<bool> {
        schedule::uninstall_for_app(&self.ctx, self.permit()?)
    }

    /// Point a schedule an app up to 0.3.0 wrote, which runs the app itself, or on Windows a
    /// task whose Pitboard is gone and whose settings are as Pitboard wrote them, at the
    /// command line this context names. `true` when it did; nothing changes otherwise.
    pub fn schedule_repair(&self) -> Result<bool> {
        schedule::repair(&self.ctx, self.permit()?)
    }

    /// Whether `pitboard watch` runs in the background from sign-in on, from the system's own
    /// service manager.
    pub fn watch_service(&self) -> crate::watch_service::Installed {
        crate::watch_service::status(&self.ctx)
    }

    /// Have the system's own service manager run `pitboard watch` at `at` from now on and at
    /// each sign-in. Opt-in, as `pitboard watch` in a terminal is. Refused with
    /// `schedule_not_default_home` where this is not the default home, as the renewal
    /// schedule is.
    pub fn watch_service_install(&self, at: Threshold) -> Result<schedule::Location> {
        crate::watch_service::install(&self.ctx, self.permit()?, at)
    }

    /// Stop it and take it away. `false` when there was nothing installed.
    pub fn watch_service_uninstall(&self) -> Result<bool> {
        crate::watch_service::uninstall(&self.ctx, self.permit()?)
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
        self.changing(permit, "repair", "", None, switch::repair)
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

    // Whether `which`'s app may be opened again after a switch or a refusal, from the index
    // alone. Not while a move waits, nor where it would open signed out with every account
    // parked and no sign-in waited for.
    pub fn whole(&self, which: ProviderId) -> bool {
        match crate::provider::of(which) {
            LoginKind::Document(_) => true,
            LoginKind::Folder(_) => {
                let Ok(state) = state::load(&self.ctx) else {
                    return false;
                };
                let parked = state.accounts.iter().any(|a| {
                    a.provider() == which
                        && a.parked
                            .as_ref()
                            .is_some_and(|park| park.kind == state::ParkKind::Folder)
                });
                let nobody = state
                    .in_use(which)
                    .is_some_and(|record| record.owner.is_none());
                let left_signed_out = nobody && parked && state.awaiting(which).is_none();
                state.moving(which).is_none() && !left_signed_out
            }
        }
    }

    // Whether a move of `which`'s login waits, from the index alone: asked before its app is
    // opened for a sign-in inside it, where it is meant to be signed out.
    pub fn moving(&self, which: ProviderId) -> bool {
        match crate::provider::of(which) {
            LoginKind::Document(_) => false,
            LoginKind::Folder(_) => {
                state::load(&self.ctx).map_or(true, |state| state.moving(which).is_some())
            }
        }
    }

    // Where an app's parked logins and the folders set aside are kept, for a front end to name.
    pub fn parks_folder(&self) -> std::path::PathBuf {
        crate::store::folder::parks_dir(&self.ctx)
    }

    // `PITBOARD_HOME`, or the default in the home.
    pub fn directory(&self) -> std::path::PathBuf {
        crate::home::dir(&self.ctx)
    }

    // The one the renewal schedule renews, whatever `PITBOARD_HOME` says.
    pub fn default_directory(&self) -> std::path::PathBuf {
        self.ctx.default_pitboard_home()
    }

    // As a person reads a folder: under the home it starts `~` on macOS and Linux.
    pub fn shown(&self, folder: &std::path::Path) -> String {
        let home = self.ctx.home();
        match folder.strip_prefix(home) {
            Ok(rest) if self.ctx.os != Os::Windows && !home.as_os_str().is_empty() => {
                std::path::Path::new("~").join(rest).display().to_string()
            }
            _ => folder.display().to_string(),
        }
    }

    /// When Pitboard's account index last changed, in epoch milliseconds, for a front end
    /// that wants to know whether another one has done something without asking Anthropic
    /// about it.
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
            switch::uninstall(settled).map(|mut removed| {
                let said = std::mem::take(&mut removed.warnings);
                (removed, said)
            })
        })
    }

    /// Returns the account's email.
    /// `from` may be qualified; `to` is a plain name, and stays inside whichever provider
    /// the account already belongs to. Renaming cannot move an account between tools.
    pub fn rename(&self, from: &str, to: &str) -> Changing<String> {
        let permit = self.permitted()?;
        let from = self.named(permit, "rename", from)?;
        let chosen = crate::label::choose(to)
            .map(|chosen| Key::new(chosen.provider, chosen.label))
            .map_err(|refusal| {
                self.refused(
                    permit,
                    "rename",
                    to,
                    "label_unusable",
                    Error::Usage(refusal),
                )
            })?;
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

    /// Each Claude Code account, whether Pitboard warms it up, and within which span of each
    /// day, from Pitboard's files alone: no lock, and nobody asked.
    pub fn warm_overview(&self) -> Result<crate::duty::warm::Overview> {
        let state = state::load(&self.ctx)?;
        let setting = crate::duty::warm::load(&self.ctx)?;
        Ok(crate::duty::warm::overview(&self.ctx, &state, &setting))
    }

    /// Warms up the account `typed` names now, whatever the span of the day and the attempts
    /// already made today, where its five-hour period is not running and nothing holds it back:
    /// one short request through Claude Code's own headless mode, with that account's access
    /// token. Recorded in the activity log as `warm-up` of the account.
    pub fn warm_now(&self, typed: &str) -> Changing<switch::warm::WarmUp> {
        let permit = self.permitted()?;
        let key = self.named(permit, "warm-up", typed)?;
        let settled = self.settled_to_warm(permit, "warm-up", &key.typed())?;
        Self::warming(settled, switch::warm::Target::Account(key))
    }

    /// Warms up the first account turned on that is due, as [`crate::duty::warm`] says, for a front
    /// end somebody asked to: the app, or `pitboard watch`. `Pitboard::warm_look` says
    /// whether one may be, waiting on no lock.
    pub fn warm_up(&self) -> Changing<switch::warm::WarmUp> {
        let permit = self.permitted()?;
        let looked = self.warm_look().map_err(|error| Failed {
            error,
            warnings: Vec::new(),
            said_first: 0,
        })?;
        if let crate::duty::Look::Stands(overview) = looked {
            return Ok(Done {
                value: switch::warm::WarmUp::Stands(overview),
                warnings: Vec::new(),
            });
        }
        let settled = self.settled_to_warm(permit, "warm-up", "")?;
        Self::warming(settled, switch::warm::Target::Due)
    }

    /// What warming up would come to now, from files alone: what stands, with where each
    /// account turned on stands, or that only [`Pitboard::warm_up`] can say. Waits on no lock,
    /// reads no keychain and asks nobody, so a front end may look as often as it likes.
    pub(crate) fn warm_look(&self) -> Result<crate::duty::Look<crate::duty::warm::Overview>> {
        let state = state::load(&self.ctx)?;
        crate::duty::warm::look(&self.ctx, &state)
    }

    /// Settled for a warm-up, with an interrupted switch settling finished. Without what a
    /// change of Claude Code's login warns of: a warm-up's run is given nothing of this
    /// process's environment and no file behind the keychain, and what would make it sign in
    /// another way stops it outright.
    fn settled_to_warm(
        &self,
        permit: Permit,
        verb: &str,
        subject: &str,
    ) -> std::result::Result<(Settled, Vec<Warning>), Failed> {
        let (settled, recovered) = self.settled(permit, verb, subject, Some(ProviderId::Claude))?;
        Ok((settled, recovered.into_iter().collect()))
    }

    fn warming(
        (settled, mut warnings): (Settled, Vec<Warning>),
        target: switch::warm::Target,
    ) -> Changing<switch::warm::WarmUp> {
        match switch::warm::warm_up(settled, target) {
            Ok((value, more)) => {
                warnings.extend(more);
                Ok(Done { value, warnings })
            }
            Err(error) => Err(Failed::of(error, warnings)),
        }
    }

    /// Warms up the account `typed` names, or stops, recorded in the activity log as
    /// `warm-on` or `warm-off` of that account. Only a Claude Code account is warmed up
    /// (`warm_up_unsupported`); any may be turned off. Whether that changed anything.
    pub fn warm_turn(&self, typed: &str, on: bool) -> Changing<bool> {
        let verb = if on { "warm-on" } else { "warm-off" };
        let permit = self.permitted()?;
        let key = self.named(permit, verb, typed)?;
        let subject = key.typed();
        let settled = self.settled_to_warm(permit, verb, &subject)?;
        self.change(permit, verb, &subject, settled, |settled| {
            switch::warm::turn(settled, &key, on).map(|changed| (changed, Vec::new()))
        })
    }

    /// Warms up within `range` of each day, recorded in the activity log as `warm-range` of
    /// the span.
    pub fn warm_range(
        &self,
        range: crate::duty::warm::Range,
    ) -> Changing<crate::duty::warm::Range> {
        let permit = self.permitted()?;
        let subject = format!(
            "{}-{}",
            crate::duty::warm::clock(range.from()),
            crate::duty::warm::clock(range.to())
        );
        let settled = self.settled_to_warm(permit, "warm-range", &subject)?;
        self.change(permit, "warm-range", &subject, settled, |settled| {
            switch::warm::range(settled, range).map(|range| (range, Vec::new()))
        })
    }

    /// The login left in the file behind Claude Code's store, and what putting it away would
    /// do with it, for somebody to confirm: `None` where nothing is behind that store. What
    /// `pitboard stow` and the app's sheet ask first. Takes no lock and writes nothing, and
    /// asks Anthropic whose the login is, with its own access token, only where neither its
    /// fingerprint nor its expiry already says, and then, for an enrolled account, whose the
    /// login stored is, as a read asks it, only where that changed since Anthropic last named
    /// it.
    pub fn left_login(&self) -> Result<Option<switch::Left>> {
        let state = state::load(&self.ctx)?;
        switch::left_login(&self.ctx, &state)
    }

    /// Puts away the login left in the file behind Claude Code's store, while the file still
    /// holds what `seen` was taken of, as `switch::stow` says. Recorded in the audit log as
    /// `stow`, of the account the login was found to be where it is enrolled, with what became
    /// of it, or with the error that stopped it. Settled with what a change of Claude Code's
    /// login warns of but the file, which is what it puts away.
    pub fn stow(&self, seen: &str) -> Changing<switch::Stowed> {
        let permit = self.permitted()?;
        let tool = ProviderId::Claude;
        let (settled, recovered) = self.settled(permit, "stow", "", Some(tool))?;
        let warnings = self.overridden(tool).into_iter().chain(recovered).collect();
        self.change(permit, "stow", "", (settled, warnings), |settled| {
            switch::stow(settled, seen)
        })
    }

    /// What says `tool` signs in with something else than the login Pitboard moves, read from
    /// files as well as from this process's environment, so the app, which has no shell
    /// environment at all, gets the same answer as the command line.
    fn overridden(&self, tool: ProviderId) -> Option<Warning> {
        let driver = match crate::provider::of(tool) {
            LoginKind::Document(driver) => driver,
            LoginKind::Folder(_) => return None,
        };
        let names = driver.overridden_by(&self.ctx);
        (!names.is_empty()).then_some(Warning::AuthOverridden { tool, names })
    }

    /// What a file behind `tool`'s store says, where one is there. It reads that store, so a
    /// read that asks nobody does not ask it.
    fn left_behind(&self, tool: ProviderId) -> Option<Warning> {
        let driver = match crate::provider::of(tool) {
            LoginKind::Document(driver) => driver,
            LoginKind::Folder(_) => return None,
        };
        let behind = driver.behind(&self.ctx)?;
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
        let settled = self.settled_for(permit, verb, subject, tool)?;
        self.change(permit, verb, subject, settled, run)
    }

    /// Runs a change on what is settled for it, beside what settling warned of, and records
    /// it in the audit log as `verb`: of the subject the change names where it names one, and
    /// otherwise of `subject`.
    fn change<T: Audited>(
        &self,
        permit: Permit,
        verb: &str,
        subject: &str,
        (settled, mut warnings): (Settled, Vec<Warning>),
        run: impl FnOnce(Settled) -> Result<(T, Vec<Warning>)>,
    ) -> Changing<T> {
        match run(settled) {
            Ok((value, more)) => {
                let subject = value.audit_subject().unwrap_or(subject);
                audit::record(&self.ctx, permit, verb, subject, value.audit_code());
                warnings.extend(more);
                Ok(Done { value, warnings })
            }
            Err(error) => {
                audit::record(&self.ctx, permit, verb, subject, error.code());
                Err(Failed::of(error, warnings))
            }
        }
    }

    /// Settles for a change, with what to know before it runs: another way `tool` is signed
    /// in, a file behind its store, and an interrupted switch settling finished, as
    /// [`Pitboard::settled`] records it.
    fn settled_for(
        &self,
        permit: Permit,
        verb: &str,
        subject: &str,
        tool: Option<ProviderId>,
    ) -> std::result::Result<(Settled, Vec<Warning>), Failed> {
        let (settled, recovered) = self.settled(permit, verb, subject, tool)?;
        let mut warnings = Vec::new();
        if let Some(tool) = tool {
            warnings.extend(self.overridden(tool));
            warnings.extend(self.left_behind(tool));
        }
        warnings.extend(recovered);
        Ok((settled, warnings))
    }

    /// Settles for a change, with an interrupted switch settling finished, which the audit
    /// log records as `recover`. A settle that failed is recorded as `verb` of `subject`, with
    /// its error.
    fn settled(
        &self,
        permit: Permit,
        verb: &str,
        subject: &str,
        tool: Option<ProviderId>,
    ) -> std::result::Result<(Settled, Option<Warning>), Failed> {
        let (settled, recovered) = switch::settle(&self.ctx, permit, tool).map_err(|error| {
            audit::record(&self.ctx, permit, verb, subject, error.code());
            Failed {
                error,
                warnings: Vec::new(),
                said_first: 0,
            }
        })?;
        let recovered = recovered.map(|r| {
            audit::record(&self.ctx, permit, "recover", &r.to, r.code());
            Warning::Recovered(r)
        });
        Ok((settled, recovered))
    }
}

/// How a successful change is written in the audit log.
trait Audited {
    fn audit_code(&self) -> &'static str {
        "ok"
    }

    /// The account it was of, where only running it could tell.
    fn audit_subject(&self) -> Option<&str> {
        None
    }
}

impl Audited for Outcome {
    fn audit_code(&self) -> &'static str {
        match self {
            Outcome::Switched { .. } | Outcome::Installed { .. } => "ok",
            Outcome::AlreadyActive {
                config_updated: true,
                ..
            } => "config_updated",
            Outcome::AlreadyActive { .. } => "already_active",
        }
    }
}

impl Audited for switch::Reclaimed {}
impl Audited for switch::WatchedSignIn {}
impl Audited for Enrolled {}
impl Audited for String {}
impl Audited for bool {}
impl Audited for crate::duty::warm::Range {}

impl Audited for switch::Stowed {
    fn audit_code(&self) -> &'static str {
        self.kept.code()
    }

    fn audit_subject(&self) -> Option<&str> {
        self.kept.account()
    }
}

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
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;
    use crate::switch::harness::{
        Rig, Tool, audit_lines, codex_machine, hold, machine, signed_in_outside, state_file,
    };
    use std::collections::BTreeMap;
    use std::sync::Arc;

    type Make = fn(&str) -> Rig;

    // Codex's login says whose it is, so reading it asks nobody; Claude Code's does not say.
    #[test]
    fn whose_login_a_tool_stores_is_read_from_the_login_alone() {
        let m = codex_machine("signed-in-email");
        let pitboard = Pitboard::new(m.ctx.clone());
        let calls = m.api.calls();
        assert_eq!(
            pitboard.signed_in_email(ProviderId::Codex).as_deref(),
            Some("here@example.com")
        );
        assert_eq!(m.api.calls(), calls, "nobody was asked");
        assert_eq!(pitboard.signed_in_email(ProviderId::Claude), None);
        assert_eq!(pitboard.signed_in_email(ProviderId::Desktop), None);

        m.sign_in(&serde_json::json!({"auth_mode": "apikey", "OPENAI_API_KEY": "sk-test"}));
        assert_eq!(
            pitboard.signed_in_email(ProviderId::Codex),
            None,
            "no account's"
        );
        m.fault_live(crate::store::memory::Fault::Unreadable("locked".into()));
        assert_eq!(
            pitboard.signed_in_email(ProviderId::Codex),
            None,
            "unreadable"
        );
    }

    /// A name is refused the same way whichever tool it is for.
    const MACHINES: [(&str, Make); 2] = [("claude", machine), ("codex", codex_machine)];

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
    )]
    fn a_read_opens_no_jar_and_lists_no_process() {
        let m = switch::harness::desktop_machine("reads");
        let pitboard = Pitboard::new(m.ctx.clone());
        let (jars, listings) = (m.mem.jars_asked().len(), m.mem.listings());
        // Claude Code's own record, of nobody, is this first read's to make.
        pitboard.status(true).expect("read");
        let before = state_file(&m);
        let online = pitboard.status(true).expect("read");
        let offline = pitboard.status_offline().expect("read");
        let doctor = pitboard.doctor();
        pitboard.statusline("{}");
        assert_eq!(m.mem.jars_asked().len(), jars, "no jar was opened");
        assert_eq!(m.mem.listings(), listings, "no process was listed");
        assert_eq!(state_file(&m), before, "nothing was recorded");

        // And in the rows a change would open the jar for: 10, 15 and 6.
        let data = m.ctx_home().join("claude/data");
        let rows: [(&str, &dyn Fn()); 3] = [
            ("no session", &|| {
                crate::desktop_testing::write_stand_in_jar(&data.join("Cookies"), 24, &[]);
            }),
            ("another name", &|| {
                let settings = data.join("config.json");
                let written = std::fs::read_to_string(&settings).expect("settings");
                let renamed = written.replace(
                    switch::harness::WORK.uuid,
                    "0b7e2c44-2222-4d1f-8a35-91c0de77aa01",
                );
                std::fs::write(&settings, renamed).expect("renamed");
            }),
            ("no jar", &|| {
                std::fs::remove_file(data.join("Cookies")).expect("the jar taken away");
            }),
        ];
        for (row, change) in rows {
            change();
            pitboard.status(true).expect("read");
            pitboard.status_offline().expect("read");
            pitboard.doctor();
            pitboard.statusline("{}");
            assert_eq!(m.mem.jars_asked().len(), jars, "{row}: no jar was opened");
            assert_eq!(m.mem.listings(), listings, "{row}: no process was listed");
            assert_eq!(state_file(&m), before, "{row}: nothing was recorded");
        }

        for report in [&online.value, &offline.value] {
            let row = report
                .rows
                .iter()
                .find(|row| row.provider == ProviderId::Desktop)
                .expect("a row");
            assert!(row.signed_in);
            assert_eq!(row.read_at, Some(switch::harness::NOW));
        }
        for warnings in [&online.warnings, &offline.warnings] {
            assert!(
                warnings
                    .iter()
                    .all(|w| !w.to_string().contains("Claude Desktop")
                        && w.code() != "in_use_unconfirmed"),
                "{warnings:?}"
            );
        }
        let codes: Vec<&str> = doctor.checks.iter().map(|c| c.code).collect();
        for code in [
            "desktop_data_folder",
            "desktop_settings",
            "desktop_layout",
            "desktop_login",
            "desktop_parked_login",
            "desktop_build",
        ] {
            assert!(codes.contains(&code), "{code}: {codes:?}");
        }
        assert!(
            doctor
                .checks
                .iter()
                .filter(|c| c.code.starts_with("desktop_"))
                .all(|c| c.level == doctor::Level::Ok),
            "{:?}",
            doctor
                .checks
                .iter()
                .map(|c| (c.code, &c.detail))
                .collect::<Vec<_>>()
        );
    }

    // Claude's settings there and unreadable, which a read of them would trip over.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
    )]
    fn a_mac_with_no_claude_desktop_account_reads_nothing_of_claude() {
        let plain = machine("no-desktop-plain");
        let m = machine("no-desktop");
        let scratch = m.ctx_home().join("claude");
        std::fs::create_dir_all(scratch.join("data")).expect("a data folder");
        std::fs::write(scratch.join("data/config.json"), "{").expect("settings");
        crate::host::fs::testing::deny_reading(&scratch.join("data/config.json")).expect("000");
        let read = |m: &Rig, ctx: Context| {
            let pitboard = Pitboard::new(ctx);
            let codes = |done: Done<status::Report>| {
                let warnings: Vec<&str> = done.warnings.iter().map(Warning::code).collect();
                (done.value.rows.len(), format!("{warnings:?}"))
            };
            let doctor: Vec<&str> = pitboard.doctor().checks.iter().map(|c| c.code).collect();
            assert_eq!(m.mem.listings(), 0);
            assert!(m.mem.jars_asked().is_empty());
            (
                codes(pitboard.status(true).expect("read")),
                codes(pitboard.status_offline().expect("read")),
                format!("{doctor:?}"),
            )
        };
        let with_claude = crate::fault::killing("settings.read", || {
            read(&m, m.ctx.clone().with_scratch_claude(scratch))
        })
        .expect("no read opened Claude's settings");
        assert_eq!(
            with_claude,
            read(&plain, plain.ctx.clone()),
            "as with no Claude at all"
        );
        assert!(!with_claude.2.contains("desktop_"), "{}", with_claude.2);
    }

    // Forgetting the one account leaves a record of nobody in use, which opts nothing in.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
    )]
    fn forgetting_the_last_claude_desktop_account_stops_every_read_of_claude() {
        let m = switch::harness::desktop_machine("forgot-last");
        let claude = m.claude();
        claude.opens();
        claude.logs_out();
        claude.quits();
        let pitboard = Pitboard::new(m.ctx.clone());
        pitboard.forget("desktop/work").expect("forgotten");
        claude.signed_in_as(&switch::harness::STRANGER);
        let state = state::load(&m.ctx).expect("state");
        assert!(state.accounts.is_empty());
        assert!(state.in_use(ProviderId::Desktop).is_some(), "{state:?}");

        let (listings, jars) = (m.mem.listings(), m.mem.jars_asked().len());
        let said = crate::fault::killing("settings.read", || {
            let codes = |done: Done<status::Report>| -> Vec<&'static str> {
                done.warnings.iter().map(Warning::code).collect()
            };
            let doctor: Vec<&str> = pitboard
                .doctor()
                .checks
                .iter()
                .map(|c| c.code)
                .filter(|code| code.starts_with("desktop_"))
                .collect();
            (
                codes(pitboard.status(true).expect("read")),
                codes(pitboard.status_offline().expect("read")),
                doctor,
            )
        })
        .expect("no read opened Claude's settings");
        assert_eq!(said, (Vec::new(), Vec::new(), Vec::new()));
        assert_eq!(m.mem.listings(), listings);
        assert_eq!(m.mem.jars_asked().len(), jars);
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
    )]
    fn what_claudes_settings_say_since_is_a_warning_of_every_read() {
        let m = switch::harness::desktop_machine("standing");
        let pitboard = Pitboard::new(m.ctx.clone());
        let settings = m.ctx_home().join("claude/data/config.json");
        let said = |pitboard: &Pitboard| -> Vec<String> {
            pitboard
                .status_offline()
                .expect("read")
                .warnings
                .iter()
                .map(|w| format!("{}: {w}", w.code()))
                .collect()
        };
        assert!(said(&pitboard).is_empty());

        let written = std::fs::read_to_string(&settings).expect("settings");
        std::fs::write(
            &settings,
            written.replace(
                switch::harness::WORK.uuid,
                "0b7e2c44-2222-4d1f-8a35-91c0de77aa01",
            ),
        )
        .expect("renamed");
        let warned = said(&pitboard);
        assert_eq!(warned.len(), 1, "{warned:?}");
        assert!(
            warned[0].starts_with(
                "in_use_unconfirmed: Claude's settings have named an account Pitboard has not \
                 enrolled since Pitboard last read `desktop/work` in use, so Claude may hold \
                 another login. Open Claude,"
            ),
            "{warned:?}"
        );

        let emptied: String = written
            .lines()
            .map(|line| match line.split_once(": ") {
                Some((key, _)) if key.contains("oauth:tokenCache") => {
                    format!("{key}: {},\n", crate::desktop_testing::EMPTIED)
                }
                _ => format!("{line}\n"),
            })
            .collect();
        std::fs::write(&settings, emptied).expect("emptied");
        let warned = said(&pitboard);
        assert_eq!(
            warned,
            [
                "looks_signed_out: Claude looks signed out since Pitboard last read `desktop/work` \
              in use. The next change to a Claude Desktop account checks."
            ]
        );
        let mut kept: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&settings).expect("settings"))
                .expect("JSON");
        kept.insert(
            "custom3pMcpOAuth".into(),
            serde_json::json!({"grant": "kept"}),
        );
        std::fs::write(
            &settings,
            serde_json::to_string_pretty(&kept).expect("JSON"),
        )
        .expect("grants");
        assert_eq!(
            said(&pitboard),
            warned,
            "a Log out leaves the connector grants"
        );
        assert_eq!(m.mem.listings(), 1, "the enrolment's look, and no read's");
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
    )]
    fn what_stands_is_judged_from_one_read_of_claudes_settings() {
        let m = switch::harness::desktop_machine("one-read");
        let pitboard = Pitboard::new(m.ctx.clone());
        let state = state::load(&m.ctx).expect("state");
        let (warnings, reads) =
            crate::fault::counting("settings.read", || pitboard.standing(&state, &[]));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(reads, 1);
        let (known, reads) = crate::fault::counting("settings.read", || {
            in_use::known(&m.ctx, &state, ProviderId::Desktop)
        });
        assert_eq!((known.doubt, reads), (None, 1));
    }

    #[test]
    fn a_folder_set_aside_is_said_with_its_account_or_with_no_name() {
        use state::SetAside;
        let said = |why, label: Option<&str>| {
            Warning::SetAside {
                tool: ProviderId::Desktop,
                path: "/p/parks/x".into(),
                label: label.map(str::to_owned),
                why,
            }
            .to_string()
        };
        assert_eq!(
            said(SetAside::Superseded, Some("desktop/home")),
            "Pitboard kept `desktop/home`'s earlier parked login in /p/parks/x and deleted \
             nothing. `pitboard doctor` lists it."
        );
        assert_eq!(
            said(SetAside::Superseded, None),
            "Pitboard kept an earlier parked login in /p/parks/x and deleted nothing. `pitboard \
             doctor` lists it."
        );
        assert_eq!(
            said(SetAside::GivenUp, Some("desktop/work")),
            "Pitboard moved the parts of `desktop/work`'s login left in Claude's folder into \
             /p/parks/x and deleted nothing. `pitboard doctor` lists them."
        );
        assert_eq!(
            said(SetAside::GivenUp, None),
            "Pitboard moved the parts of a login left in Claude's folder into /p/parks/x and \
             deleted nothing. `pitboard doctor` lists them."
        );
        for why in [
            SetAside::InTheWay,
            SetAside::AnotherMachine,
            SetAside::Orphan,
        ] {
            for label in [None, Some("desktop/work")] {
                assert_eq!(
                    said(why, label),
                    "Pitboard moved what it found in the way into /p/parks/x and deleted nothing."
                );
            }
        }
        let keys = |label: Option<&str>| Warning::KeysSetAside {
            tool: ProviderId::Desktop,
            path: "/p/parks/x".into(),
            label: label.map(str::to_owned),
        };
        assert_eq!(keys(None).code(), "set_aside");
        assert_eq!(
            keys(Some("desktop/home")).to_string(),
            "Claude rewrote `desktop/home`'s keys. Pitboard kept the earlier ones in /p/parks/x \
             and deleted nothing. `pitboard doctor` lists them."
        );
        assert_eq!(
            keys(None).to_string(),
            "Claude rewrote a login's keys. Pitboard kept the earlier ones in /p/parks/x and \
             deleted nothing. `pitboard doctor` lists them."
        );
    }

    // What a change did before it refused leads, with a journal recovery, which ran before
    // it; what it only found follows.
    #[test]
    fn a_refused_change_counts_what_it_did_first() {
        let found = || Warning::LooksSignedOut {
            tool: ProviderId::Claude,
            label: "work".into(),
        };
        let refusal = || Error::AccountNotInUse {
            tool: ProviderId::Desktop,
            label: "desktop/work".into(),
        };
        let waiting = || Warning::MoveWaiting {
            tool: ProviderId::Desktop,
            label: "desktop/work".into(),
        };
        let recovered = || {
            Warning::Recovered(Recovered {
                from: "home".into(),
                to: "work".into(),
                finished: true,
            })
        };
        let codes = |failed: &Failed| -> Vec<&'static str> {
            failed.warnings.iter().map(Warning::code).collect()
        };
        let failed = Failed::of(refusal().after(vec![waiting()]), vec![found()]);
        assert_eq!(codes(&failed), ["move_waiting", "looks_signed_out"]);
        assert_eq!(failed.said_first, 1);
        assert_eq!(failed.error.code(), "account_not_in_use");
        let failed = Failed::of(refusal(), vec![found()]);
        assert_eq!(codes(&failed), ["looks_signed_out"]);
        assert_eq!(failed.said_first, 0);
        let failed = Failed::of(refusal(), vec![found(), recovered()]);
        assert_eq!(
            codes(&failed),
            ["interrupted_switch_finished", "looks_signed_out"]
        );
        assert_eq!(failed.said_first, 1);
        let failed = Failed::of(refusal().after(vec![waiting()]), vec![found(), recovered()]);
        assert_eq!(
            codes(&failed),
            [
                "interrupted_switch_finished",
                "move_waiting",
                "looks_signed_out"
            ]
        );
        assert_eq!(failed.said_first, 2);
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
    )]
    fn what_holds_claude_desktops_login_is_said() {
        let m = switch::harness::desktop_machine("holding");
        let pitboard = Pitboard::new(m.ctx.clone());
        let claude = m.claude();
        assert!(pitboard.holding(ProviderId::Desktop).is_empty());
        claude.opens();
        let holding = pitboard.holding(ProviderId::Desktop);
        assert_eq!(holding.len(), 1, "{holding:?}");
        assert_eq!(holding[0].holder.kind, "claude_desktop");
        assert_eq!(holding[0].pids.len(), 2, "its own program and a helper");
        claude.crashes();
        assert!(pitboard.holding(ProviderId::Desktop).is_empty());
        m.mem.without_a_process_list();
        assert!(matches!(
            switch::still_holding(&m.ctx, ProviderId::Desktop),
            switch::StillHolding::Unknown
        ));
        assert!(pitboard.holding(ProviderId::Desktop).is_empty());
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
    )]
    fn a_desktop_row_says_no_usage_and_asks_nobody() {
        use crate::status::Stale;
        let m = switch::harness::desktop_machine("rows");
        let now = switch::harness::NOW;
        let mut state = state::load(&m.ctx).expect("state");
        let another = |label: &str, parked, replaced_at| Account {
            label: label.into(),
            id: format!("desktop_{label}"),
            account_uuid: label.into(),
            email: String::new(),
            parked,
            last_used_at: None,
            replaced_at,
            plan: None,
            detail: state::Detail::Desktop {},
        };
        let park = state::Park {
            service: "pitboard-park-desktop_home-1".into(),
            kind: state::ParkKind::Folder,
            parked_at: now,
            login: "0a1b2c3d4e5f6071".into(),
            access_expires_at: None,
            refresh_expires_at: None,
            session_expires_at: Some(now + 20 * 86_400),
        };
        state.accounts.push(another("home", Some(park), None));
        state.accounts.push(another("gone", None, Some(now - 5)));
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");

        let pitboard = Pitboard::new(m.ctx.clone());
        for report in [
            pitboard.status(true).expect("read").value,
            pitboard.status_offline().expect("read").value,
        ] {
            let row = |label: &str| {
                report
                    .rows
                    .iter()
                    .find(|row| {
                        row.provider == ProviderId::Desktop && row.label.as_deref() == Some(label)
                    })
                    .unwrap_or_else(|| panic!("a row for {label}"))
            };
            let (work, home, gone) = (row("work"), row("home"), row("gone"));
            assert!(work.signed_in && !home.signed_in && !gone.signed_in);
            assert_eq!(work.read_at, Some(now));
            assert_eq!((home.read_at, gone.read_at), (None, None));
            for row in [work, home] {
                assert_eq!(row.stale, Some(Stale::NoUsage));
                assert!(row.usage.is_none() && row.plan.is_none() && row.lasts(now).is_none());
                assert_eq!(row.email, "");
                assert_eq!(row.organization_uuid, None);
            }
            assert!(home.switchable(now), "no deadline is known");
            assert_eq!(gone.stale, Some(Stale::LoginReplaced));
            assert_eq!(
                gone.explanation().as_deref(),
                Some("its login was signed out or replaced in Claude; sign in again")
            );
            assert_eq!(
                Stale::NoUsage
                    .explanation_for(ProviderId::Desktop, "Claude")
                    .as_deref(),
                Some("Pitboard reads no usage for Claude Desktop")
            );
            assert_eq!(
                Stale::Unreachable.explanation_for(ProviderId::Desktop, "Claude"),
                None
            );
        }
        assert!(m.api.asked().is_empty(), "{:?}", m.api.asked());
        assert!(
            m.mem
                .trail()
                .iter()
                .all(|touch| !touch.service.starts_with("pitboard-park-desktop_")),
            "{:?}",
            m.mem.trail()
        );
        assert!(
            m.mem.jars_asked().len() == 1 && m.mem.listings() == 1,
            "the enrolment's own"
        );
    }

    // The app took names with a space before a new name had to be one word.
    #[test]
    fn an_account_named_before_names_were_one_word_keeps_its_name() {
        let m = machine("one-word");
        let mut state = state::load(&m.ctx).expect("state");
        state
            .accounts
            .iter_mut()
            .find(|a| a.label == "there")
            .expect("there")
            .label = "over there".into();
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        let pitboard = Pitboard::new(m.ctx.clone());

        assert_eq!(
            pitboard.account_to_enroll("over there").map(|a| a.label),
            Some("over there".to_owned())
        );
        let refused = pitboard
            .enroll_current("new one")
            .expect_err("a new name of two words");
        assert_eq!(refused.error.code(), "usage", "{}", refused.error);
        assert_eq!(
            audit_lines(&m, "enroll"),
            [("new one".to_owned(), "label_unusable".to_owned())]
        );
    }

    // An existing account's name keeps it, and a rename gives a new one by the rules all the same.
    #[test]
    fn a_rename_to_a_name_another_tool_has_follows_the_rules_for_a_new_one() {
        let m = codex_machine("rename-one-word");
        let mut state = state::load(&m.ctx).expect("state");
        state
            .accounts
            .push(switch::harness::account("my work", "elsewhere", None));
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        let pitboard = Pitboard::new(m.ctx.clone());

        let refused = pitboard
            .rename("codex/there", "my work")
            .expect_err("a name of two words");
        assert_eq!(refused.error.code(), "usage", "{}", refused.error);
        assert!(
            state::load(&m.ctx)
                .expect("state")
                .get(&m.key("there"))
                .is_some(),
            "the Codex account keeps its name"
        );
    }

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
    fn interrupted(make: Make, name: &str) -> Rig {
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
    fn stuck(make: Make, name: &str) -> Rig {
        let m = interrupted(make, name);
        m.fault_live(crate::store::memory::Fault::Vanish);
        m
    }

    /// The switch interrupted on a Claude Code machine, which the next change cannot finish
    /// either, though only Anthropic could tell: Claude Code has since renewed the login, so
    /// it matches neither side the record kept, and its session has expired, so Anthropic
    /// will not say whose it is. An access token the scripted service does not know is one
    /// it refuses.
    fn stuck_unless_asked(name: &str) -> Rig {
        let m = interrupted(machine, name);
        m.sign_in(&switch::harness::document("renewed-since"));
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

    fn untouched(m: &Rig) -> Untouched {
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
    fn a_switch_says_a_login_is_left_where_it_does_not_reach() {
        let m = machine("switch-fallback-login");
        let left = plant_behind(
            &m,
            &switch::harness::document("left-by-a-sign-in").to_string(),
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
    fn plant_behind(m: &Rig, contents: &str) -> std::path::PathBuf {
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
    fn a_read_says_a_file_is_behind_the_keychain() {
        let m = machine("read-fallback-login");
        let left = plant_behind(
            &m,
            &switch::harness::document("left-by-a-sign-in").to_string(),
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
        assert!(
            said[0].ends_with(
                "`pitboard stow` keeps that login for its account, where Pitboard holds no \
                 other it can switch to, then deletes the file."
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
                     `pitboard stow` deletes it, which lets them follow a switch.",
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

    #[test]
    fn giving_up_on_a_switch_nothing_can_finish_happens_once() {
        for (tool, make) in MACHINES {
            let m = stuck(make, &format!("give-up-once-{tool}"));
            let pitboard = Pitboard::new(m.ctx.clone());

            let gave = pitboard.abandon_recovery().expect("given up");
            let abandoned = gave.switch.expect("the switch given up");
            assert_eq!(
                (abandoned.from, abandoned.to),
                (m.key("here").typed(), m.key("there").typed())
            );
            let again = pitboard.abandon_recovery().expect("nothing to give up");
            assert!(
                again.switch.is_none() && again.moves.is_empty(),
                "{tool}: {again:?}"
            );

            let read = pitboard.status(false).expect("a read");
            assert_eq!(said(&read), [], "{tool}: nothing waits");
            let changed = pitboard.switch_to(&m.key("there").typed());
            assert!(
                changed
                    .as_ref()
                    .err()
                    .is_none_or(|refused| refused.error.code() != "recovery_undetermined"),
                "{tool}: {:?}",
                changed.err().map(|refused| refused.error)
            );
        }
    }

    #[test]
    fn each_change_is_logged_newest_last_as_the_command_line_types_it() {
        for (tool, make) in MACHINES {
            let m = make(&format!("logged-{tool}"));
            let pitboard = Pitboard::new(m.ctx.clone().with_caller("app".into()));

            pitboard
                .switch_to(&m.key("there").typed())
                .expect("a switch");
            pitboard
                .rename(&m.key("here").typed(), "office")
                .expect("renamed");

            let log = pitboard.log(2);
            let (here, there) = (m.key("here"), m.key("there"));
            assert_eq!(
                log.iter()
                    .map(|change| (
                        change.caller.as_str(),
                        change.verb.as_str(),
                        change.subject.clone(),
                        change.outcome.as_str()
                    ))
                    .collect::<Vec<_>>(),
                [
                    ("app", "use", there.typed(), "ok"),
                    ("app", "rename", format!("{here} -> office"), "ok"),
                ],
                "{tool}"
            );
            assert_eq!(pitboard.log(1), log[1..], "{tool}");
        }
    }

    /// Where only the service can say whose the live login is, a read asks it, as the next
    /// change would.
    #[test]
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
        // Nothing tried to renew the login here, so nothing says it cannot be renewed.
        let said = refused.error.to_string();
        assert!(said.contains("session has expired"), "{said}");
        assert!(!said.contains("sign in"), "{said}");
    }

    /// A switch the next change finishes by itself is not one anybody has to do anything
    /// about. The rows already say who is signed in, from the tool's login and not from
    /// Pitboard's record, and the change that finishes it says what it found.
    #[test]
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
            let expected: &[&str] = match Tool::of(m.which) {
                Tool::Claude => &[],
                Tool::Codex => &["recovery_undetermined"],
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
            match Tool::of(m.which) {
                Tool::Claude => assert_eq!(
                    gave_up.expect_err("refused").code(),
                    "custom_oauth_endpoint",
                    "{tool}"
                ),
                Tool::Codex => {
                    assert!(gave_up.expect("given up").switch.is_some(), "{tool}");
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
        assert_eq!(audit_lines(&m, "in-use"), noticed);

        let recorded = state_file(&m);
        let again = pitboard.status(false).expect("a second read");
        assert_eq!(said(&again), [("login_replaced", replaced.to_string())]);
        assert_eq!(audit_lines(&m, "in-use"), noticed, "recorded once");
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
            replaced(state).iter().map(ToString::to_string).collect()
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
            switch::identify::nobody(None, 1).expect("a store holding no login"),
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

        state.accounts[0].parked = Some(state::Park {
            service: "pitboard-park-here-1".into(),
            parked_at: 1,
            kind: state::ParkKind::Vault,
            login: "f".into(),
            access_expires_at: None,
            refresh_expires_at: None,
            session_expires_at: None,
        });
        assert!(
            said(&state).is_empty(),
            "a login parked for it is a way back"
        );
    }

    /// A read that finds what the record already says writes nothing, so several front ends
    /// reading one machine never take turns rewriting its account index.
    #[test]
    fn a_read_that_finds_nothing_new_writes_nothing() {
        let m = machine("read-nothing-new");
        let pitboard = Pitboard::new(m.ctx.clone());
        let before = state_file(&m);

        pitboard.status(false).expect("a read");
        pitboard.status(false).expect("another read");

        assert_eq!(state_file(&m), before);
        assert!(audit_lines(&m, "in-use").is_empty());
    }

    /// Each `CLAUDE_CONFIG_DIR` has a login of its own stored, and Pitboard keeps whose for
    /// each. A read under another one leaves this one's record, so a sign-in outside Pitboard
    /// here is still found by the next read here, and reads taking turns under the two write
    /// nothing once each has its record.
    #[test]
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
            audit_lines(&m, "in-use"),
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
            audit_lines(&m, "in-use"),
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
            audit_lines(&m, "in-use")[1..],
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
    fn a_read_that_cannot_find_the_login_claude_code_names_records_nothing() {
        let m = machine("read-login-elsewhere");
        m.mem.live().delete_everything();
        let before = state_file(&m);

        let read = Pitboard::new(m.ctx.clone()).status(false).expect("a read");

        assert_eq!(state_file(&m), before);
        assert!(audit_lines(&m, "in-use").is_empty());
        let said = said(&read);
        assert!(
            !said.iter().any(|(code, _)| *code == "login_replaced"),
            "{said:?}"
        );
    }

    /// The labels of the rows a read has in use.
    fn in_use(read: &Done<status::Report>) -> Vec<&str> {
        read.value
            .rows
            .iter()
            .filter(|row| row.signed_in)
            .filter_map(|row| row.label.as_deref())
            .collect()
    }

    /// Another Claude Code process that starts or signs in on another login can write that
    /// login's account into Claude Code's config, and leaves the keychain's as it was. A read
    /// that asks nobody takes the account in use from what Anthropic last said of the login
    /// stored, and says the config has moved since. It took the config's account.
    #[test]
    fn an_offline_read_names_the_account_anthropic_named_whatever_the_config_says() {
        let m = machine("offline-config-moved");
        switch::harness::config_names(&m, "there");

        let read = Pitboard::new(m.ctx.clone())
            .status_offline()
            .expect("a read");

        assert_eq!(in_use(&read), ["here"]);
        assert_eq!(
            said(&read),
            [(
                "in_use_unconfirmed",
                "Claude Code's config has named `there` since Pitboard last asked Anthropic \
                 whose login Claude Code has stored, so another login may be stored. The one \
                 stored then was `here`'s. `pitboard status` asks again."
                    .to_string()
            )]
        );
        assert_eq!(m.api.calls(), 0);
    }

    /// A record brought forward from 0.9.0 names the account it last switched to, which
    /// Anthropic never named. Where Claude Code's config names another account, a read that
    /// asks nobody says the account in use is not confirmed, as doctor and the status line do.
    #[test]
    fn an_offline_read_of_a_record_never_asked_about_says_the_config_names_another() {
        let m = machine("offline-never-asked");
        let mut state = state::load(&m.ctx).expect("state");
        let here = state.get(&m.key("here")).expect("here").owner();
        state.in_use.insert(
            ProviderId::Claude.code().into(),
            InUse {
                owner: Some(here),
                login: String::new(),
                known_at: 0,
                named: None,
            },
        );
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        switch::harness::config_names(&m, "there");

        let read = Pitboard::new(m.ctx.clone())
            .status_offline()
            .expect("a read");

        assert_eq!(in_use(&read), ["here"]);
        assert_eq!(
            said(&read),
            [(
                "in_use_unconfirmed",
                "Claude Code's config names `there`, and Pitboard has not asked Anthropic whose \
                 login Claude Code has stored since Pitboard was updated, so another login may \
                 be stored. The account it last switched to is `here`. `pitboard status` asks."
                    .to_string()
            )]
        );
        assert_eq!(m.api.calls(), 0);
    }

    // As on nearly every machine updated from 0.9.0 until its first read that asks.
    #[test]
    fn an_offline_read_of_a_record_never_asked_about_is_not_confirmed_where_the_config_agrees() {
        let m = machine("offline-never-asked-agrees");
        let mut state = state::load(&m.ctx).expect("state");
        let here = state.get(&m.key("here")).expect("here").owner();
        state.in_use.insert(
            ProviderId::Claude.code().into(),
            InUse {
                owner: Some(here),
                login: String::new(),
                known_at: 0,
                named: None,
            },
        );
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        switch::harness::config_names(&m, "here");

        let read = Pitboard::new(m.ctx.clone())
            .status_offline()
            .expect("a read");

        assert_eq!(in_use(&read), ["here"]);
        assert_eq!(
            said(&read),
            [(
                "in_use_unconfirmed",
                "Claude Code's config names `here`, and Pitboard has not asked Anthropic whose \
                 login Claude Code has stored since Pitboard was updated, so another login may \
                 be stored. The account it last switched to is `here`. `pitboard status` asks."
                    .to_string()
            )]
        );
        assert_eq!(m.api.calls(), 0);
    }

    /// A read that asks settles it: the login stored is the one Anthropic named, known by its
    /// fingerprint, so nobody is asked whose it is, and what stands is that the config names
    /// another account, which `/status` in Claude Code shows. Said by every read after it,
    /// the one that asks nobody too, until something writes the config again.
    #[test]
    fn a_read_settles_it_and_says_the_config_names_another() {
        use crate::api::scripted::Asked;
        let m = machine("read-config-names-another");
        switch::harness::config_names(&m, "there");
        let pitboard = Pitboard::new(m.ctx.clone());
        let names_another = "Claude Code's config names `there`, and the login Claude Code has \
                             stored is `here`'s, as Anthropic said. `/status` in Claude Code \
                             shows `there`. `pitboard use here` writes `here` into the config.";

        let read = pitboard.status(false).expect("a read");

        assert!(
            !m.api
                .asked()
                .contains(&Asked::Owner("access-here-refresh".into())),
            "{:?}",
            m.api.asked()
        );
        assert_eq!(in_use(&read), ["here"]);
        assert_eq!(
            said(&read),
            [("config_names_another", names_another.to_string())]
        );

        let offline = pitboard.status_offline().expect("a read of what is known");
        assert_eq!(in_use(&offline), ["here"]);
        assert_eq!(
            said(&offline),
            [("config_names_another", names_another.to_string())]
        );
    }

    /// A tool whose login names its own account is identified from the login, which asks
    /// nobody, so a read that sends no request can still tell whether the next change
    /// finishes its switch. Here Codex has renewed its login since the switch stopped, so the
    /// login matches neither side the record kept, and the copy the switch parked cannot be
    /// read. `doctor` and the read that asks nobody said nothing of it, as though only OpenAI
    /// could say whose the login is.
    #[test]
    fn whose_a_codex_login_is_is_told_without_asking_anyone() {
        let m = interrupted(codex_machine, "codex-renewed-since");
        m.sign_in(&switch::harness::codex_login("here", "renewed-since"));
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

    /// A Codex store that this machine's `requirements.toml` pins is refused before a sign-in
    /// starts, naming that file, as it is for a switch: no line of the person's own changes
    /// it, and the `-c` Pitboard's sign-ins give is under it, so the login would be kept where
    /// Pitboard cannot read it back. Pitboard used to read only `$CODEX_HOME/config.toml`, and
    /// started the sign-in.
    #[test]
    fn a_codex_store_a_requirement_pins_is_refused_before_a_sign_in() {
        let m = codex_machine("codex-pinned");
        // Named where nothing is, so a sign-in that got past the refusal would find no
        // program to run rather than this machine's own `codex`.
        let ctx = m.ctx.clone().with_codex_program("/nowhere/codex".into());
        let required = crate::provider::codex::layers::system_file(&ctx, "requirements.toml");
        m.mem.administers(
            required.clone(),
            "cli_auth_credentials_store = \"keyring\"\n",
        );
        let pitboard = Pitboard::new(ctx);
        for refused in [
            pitboard.sign_in("codex/new").expect_err("refused"),
            pitboard.sign_in_watched("codex/new").expect_err("refused"),
        ] {
            assert_eq!(refused.error.code(), "live_store_unsupported");
            let said = refused.error.to_string();
            assert!(
                said.contains(&format!("pinned to `keyring` by {}", required.display())),
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
    fn reserved(m: &Rig) -> String {
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
    fn files(m: &Rig) -> BTreeMap<String, Vec<u8>> {
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
    fn last_audited(m: &Rig) -> Vec<(String, String)> {
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
            let login = switch::harness::signed_in(&m, "here", "here-refresh-2");
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

    #[derive(Debug, Clone, Copy)]
    enum Refused {
        As(Elevation),
        On(Floor),
    }

    const AS_ROOT: Refused = Refused::As(Elevation::Elevated { why: "as root" });
    const WITH_SUDO: Refused = Refused::As(Elevation::Elevated { why: "with sudo" });
    const SERVICE_ACCOUNT: Refused = Refused::As(Elevation::Elevated {
        why: crate::host::token::AS_A_SERVICE_ACCOUNT,
    });
    const ELEVATION_UNKNOWN: Refused = Refused::As(Elevation::Unknown);
    const BELOW_THE_FLOOR: Refused = Refused::On(Floor::Below { build: 22631 });
    const FLOOR_UNKNOWN: Refused = Refused::On(Floor::Unknown);

    const REFUSED: [Refused; 6] = [
        AS_ROOT,
        WITH_SUDO,
        SERVICE_ACCOUNT,
        ELEVATION_UNKNOWN,
        BELOW_THE_FLOOR,
        FLOOR_UNKNOWN,
    ];

    impl Refused {
        fn said_by(self, m: &Rig) {
            match self {
                Refused::As(elevation) => m.mem.runs_with(elevation),
                Refused::On(floor) => m.mem.runs_on(floor),
            }
        }

        fn lifted(m: &Rig) {
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
            said_first: 0,
        })
    }

    type Change = fn(&Pitboard, &Rig) -> Option<Failed>;

    /// Every change a front end can ask for, by its name. `enroll_signed_in` is asked apart,
    /// since it takes a sign-in made before.
    const CHANGES: [(&str, Change); 19] = [
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
        ("schedule_uninstall_for_app", |p, _| {
            failed(p.schedule_uninstall_for_app())
        }),
        ("schedule_repair", |p, _| failed(p.schedule_repair())),
        ("watch_service_install", |p, _| {
            failed(p.watch_service_install(Threshold::DEFAULT))
        }),
        ("watch_service_uninstall", |p, _| {
            failed(p.watch_service_uninstall())
        }),
        ("adopt", |p, _| failed(p.adopt())),
        ("repair", |p, _| p.repair().err()),
        ("keep_app_file", |p, _| {
            failed(p.keep_app_file(crate::app::AppFile::Preferences, "{}"))
        }),
        ("uninstall", |p, _| p.uninstall().err()),
        ("auto_switch", |p, _| {
            p.auto_switch(Threshold::DEFAULT).err()
        }),
    ];

    /// Everything on a machine a change could leave different: every directory and file
    /// under its home, the audit log among them and the scheduler's files with them, every
    /// item in its keychain, every login parked and the one in use. The private directory
    /// a sign-in made before is not counted: dropping that sign-in takes it away again. The
    /// lock that sign-in holds counts by its length: Windows lets no other handle read it
    /// while it is held, and dropping the sign-in lets it go.
    type Everything = (
        BTreeMap<String, Option<Vec<u8>>>,
        Vec<(String, Option<String>)>,
        Vec<(String, Option<String>)>,
        Option<serde_json::Value>,
    );

    fn everything(m: &Rig) -> Everything {
        fn walk(dir: &std::path::Path, into: &mut BTreeMap<String, Option<Vec<u8>>>) {
            for entry in std::fs::read_dir(dir).expect("a directory to read") {
                let path = entry.expect("an entry").path();
                if path.ends_with(".pitboard/signin") {
                    continue;
                }
                if path.is_dir() {
                    into.insert(path.display().to_string(), None);
                    walk(&path, into);
                } else if path.ends_with(".pitboard/signin.lock") {
                    let len = std::fs::metadata(&path).expect("the lock's length").len();
                    into.insert(path.display().to_string(), Some(len.to_le_bytes().to_vec()));
                } else {
                    let body = std::fs::read(&path)
                        .unwrap_or_else(|e| panic!("{} to read: {e}", path.display()));
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
    /// change that settles would otherwise finish, and asks nobody. One test for each tool
    /// and each way Pitboard may change nothing, named `<tool>_<way>`.
    mod every_change_is_refused_where_pitboard_may_change_nothing_and_changes_nothing {
        use super::*;

        macro_rules! cases {
            ($($case:ident: $make:ident, $refused:ident;)*) => {
                const _: () =
                    assert!([$(stringify!($case)),*].len() == MACHINES.len() * REFUSED.len());
                $(
                    #[test]
                    fn $case() {
                        every_change_is_refused(stringify!($case), $make, $refused);
                    }
                )*
            };
        }

        cases! {
            claude_as_root: machine, AS_ROOT;
            claude_with_sudo: machine, WITH_SUDO;
            claude_as_a_service_account: machine, SERVICE_ACCOUNT;
            claude_elevation_unknown: machine, ELEVATION_UNKNOWN;
            claude_below_the_floor: machine, BELOW_THE_FLOOR;
            claude_floor_unknown: machine, FLOOR_UNKNOWN;
            codex_as_root: codex_machine, AS_ROOT;
            codex_with_sudo: codex_machine, WITH_SUDO;
            codex_as_a_service_account: codex_machine, SERVICE_ACCOUNT;
            codex_elevation_unknown: codex_machine, ELEVATION_UNKNOWN;
            codex_below_the_floor: codex_machine, BELOW_THE_FLOOR;
            codex_floor_unknown: codex_machine, FLOOR_UNKNOWN;
        }
    }

    fn every_change_is_refused(case: &str, make: Make, refused: Refused) {
        for (change, run) in CHANGES {
            let at = format!("{case}, {change}");
            let m = interrupted(make, &format!("gate-{case}-{change}"));
            refused.said_by(&m);
            let (before, asked) = (everything(&m), m.api.calls());

            let failed =
                run(&Pitboard::new(m.ctx.clone()), &m).unwrap_or_else(|| panic!("{at}: refused"));

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

        let at = format!("{case}, enroll_signed_in");
        let m = interrupted(make, &format!("gate-{case}-enroll-signed-in"));
        let login = switch::harness::signed_in(&m, "new", "new-refresh");
        refused.said_by(&m);
        let before = everything(&m);
        let failed = Pitboard::new(m.ctx.clone())
            .enroll_signed_in(&m.key("new").typed(), login)
            .expect_err("refused");
        assert_eq!(failed.error.code(), refused.code(), "{at}");
        assert!(everything(&m) == before, "{at}: nothing changes");
    }

    #[test]
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
                    assert!(
                        error
                            .to_string()
                            .starts_with(&format!("{} is ", crate::host::OS.home_variable())),
                        "{at}: {error}"
                    );
                };

                refused(pitboard.permit().expect_err("the gate refuses"));
                refused(pitboard.status_offline().expect_err("refused"));
                refused(pitboard.status(false).expect_err("refused"));
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
    fn a_read_where_pitboard_may_change_nothing_asks_nobody_and_says_why() {
        for (tool, make) in MACHINES {
            for refused in REFUSED {
                let at = format!("{tool}, {refused:?}");
                let m = make(&format!("gate-read-{tool}"));
                let later = m
                    .ctx
                    .clone()
                    .with_clock(Arc::new(crate::time::FixedClock::at(
                        switch::harness::NOW + 11 * 86_400,
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
    fn the_status_line_where_pitboard_may_change_nothing_draws_the_same_line_and_writes_nothing() {
        let session = serde_json::json!({
            "session_id": "a-session",
            "rate_limits": {
                "five_hour": {"used_percentage": 40.0, "resets_at": switch::harness::NOW + 3_600},
                "seven_day": {"used_percentage": 20.0, "resets_at": switch::harness::NOW + 86_400},
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
    fn a_renewal_where_pitboard_may_change_nothing_is_refused_and_asks_nobody() {
        for (tool, make) in MACHINES {
            for refused in REFUSED {
                let at = format!("{tool}, {refused:?}");
                let m = make(&format!("gate-renew-{tool}"));
                switch::harness::renews(&m, "there-refresh", "there-refresh-2");
                let later = m
                    .ctx
                    .clone()
                    .with_clock(Arc::new(crate::time::FixedClock::at(
                        switch::harness::NOW + 11 * 86_400,
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
