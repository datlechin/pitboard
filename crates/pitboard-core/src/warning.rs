use crate::error::Error;
use crate::holder;
use crate::in_use::Stored;
use crate::provider::{Held, ProviderId};
use crate::state::{self, Key};
use crate::switch::{self, Recovered};
use crate::words::capitalised;
use std::fmt;

/// Something to know about that did not stop the operation.
#[derive(Debug)]
#[non_exhaustive]
pub enum Warning {
    /// An interrupted switch this run found and finished or undid.
    Recovered(Recovered),
    /// The login moved, and the tool's own record of who is signed in still names the last.
    ConfigNotUpdated(Error),
    ParksPendingRemoval(usize),
    /// The service refuses a parked login for good, so it was dropped.
    ParkedLoginRefused {
        tool: ProviderId,
        label: String,
    },
    RenewalFailed(Error),
    /// The tool's write or refresh lock stopped being Pitboard's during the change.
    LockCompromised {
        tool: ProviderId,
    },
    /// The tool signs in some other way than with the login Pitboard moves.
    AuthOverridden {
        tool: ProviderId,
        names: Vec<String>,
    },
    /// A file sits behind the tool's store, which keeps running sessions on their account.
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
    /// What still runs the tool on `from`, which a switch does not reach.
    SessionsStillRunning {
        from: String,
        holding: Vec<holder::Holding>,
    },
    /// What still runs the tool on the login a sign-in replaced.
    SessionsKeepTheOldLogin {
        label: String,
        holding: Vec<holder::Holding>,
    },
    /// The process list could not be read after a switch from `from`.
    SessionsUnknown {
        tool: ProviderId,
        from: String,
    },
    /// The process list could not be read after a sign-in replaced `label`'s login.
    SessionsUnknownAfterSignIn {
        tool: ProviderId,
        label: String,
    },
    /// A sign-in was parked rather than put in use, since whose login is in use was not known.
    SignInParkedNotInUse {
        tool: ProviderId,
        label: String,
        why: String,
        spelling: crate::words::Spelling,
    },
    /// The refusal the next change would make over an interrupted switch.
    SwitchStuck(Error),
    /// A sign-in outside Pitboard replaced the only login of `label`; `now` is whose is stored.
    LoginReplaced {
        tool: ProviderId,
        id: String,
        label: String,
        now: Stored,
    },
    /// The tool's record names `named` since its service named `last`'s login; `asked` is
    /// false where the service was never asked since an update.
    InUseUnconfirmed {
        tool: ProviderId,
        last: Stored,
        named: Stored,
        asked: bool,
    },
    /// The tool's record names `config`, and the login stored is `in_use`'s.
    ConfigNamesAnother {
        tool: ProviderId,
        config: Stored,
        in_use: Stored,
    },
    /// A login filed under the account the app names; `recorded` is whose Pitboard had it as.
    TakenAsNamed {
        tool: ProviderId,
        label: String,
        recorded: Option<String>,
    },
    /// A folder kept under `parks`, deleting nothing.
    SetAside {
        tool: ProviderId,
        path: std::path::PathBuf,
        label: Option<String>,
        why: state::SetAside,
    },
    /// A folder kept under `parks` with the keys `label` had before the app rewrote them.
    KeysSetAside {
        tool: ProviderId,
        path: std::path::PathBuf,
        label: Option<String>,
    },
    /// The app's settings look signed out since the account was read in use.
    LooksSignedOut {
        tool: ProviderId,
        label: String,
    },
    /// A move an earlier run left, settled before this change.
    MoveSettled(switch::MoveSettled),
    /// A move waits: the next change to the tool settles it first.
    MoveWaiting {
        tool: ProviderId,
        label: String,
    },
    /// `abandon` gave up a switch and could not give up this move.
    MoveNotGivenUp {
        tool: ProviderId,
        label: String,
        why: Error,
    },
    /// `repair` could not settle this move, and repaired the rest.
    MoveNotSettled {
        tool: ProviderId,
        label: String,
        why: Error,
    },
    /// The app is signed out for a sign-in inside it, with `put_back` parked.
    SignInAwaited {
        tool: ProviderId,
        label: String,
        put_back: String,
        since: i64,
        /// A Sign In Again of `label`, not an add.
        again: bool,
    },
    /// An item of the login was in neither place after its move.
    ItemGone {
        tool: ProviderId,
        label: String,
        item: String,
    },
    /// A read that changes nothing, since this process is not the person themselves.
    ReadOnly {
        why: Option<&'static str>,
    },
    /// A read that changes nothing, since Windows is older than Pitboard runs on.
    ReadOnlyBelowTheFloor {
        build: Option<u32>,
    },
}

impl Warning {
    pub(crate) fn read_only(refused: &Error) -> Option<Warning> {
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
