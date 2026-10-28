use crate::provider;
use crate::provider::{LoginKind, Provider, ProviderId};
mod adopt;
mod auto;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod crash;
pub(crate) mod enroll;
pub(crate) mod folder;
#[cfg(test)]
mod foreign;
mod forget;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test reads the state file a home saved"
)]
mod forms;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
pub(crate) mod harness;
pub(crate) mod identify;
mod journal;
#[cfg(test)]
mod pinned;
mod refresh;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod refusals;
mod rename;
pub(crate) mod renew;
mod settle;
mod stow;
#[cfg(test)]
mod two_tools;
mod uninstall;
mod vault;
pub mod warm;

pub use crate::pending::Reclaimed;
pub use adopt::Adopted;
pub(crate) use adopt::adopt;
pub(crate) use auto::automatically;
#[cfg(any(test, feature = "test-support"))]
pub(crate) use enroll::planted;
pub use enroll::{Enrolled, Enrolment, Naming, Said, SignIn, Steps, WatchedSignIn};
pub(crate) use enroll::{Filing, enroll, enroll_filed};
#[cfg(any(test, feature = "test-support"))]
pub use enroll::{ScriptedSignIn, SignInScript};
pub(crate) use enroll::{
    one_at_a_time, ready_to_sign_in, running_already, sign_in, sign_in_watched_as,
};
pub use folder::awaited::WaitStep;
pub use folder::{Ended, GivenUp, MoveSettled};
pub(crate) use forget::forget;
pub use journal::{Abandoned, Recovered};
pub(crate) use journal::{Asking, interrupted_tool, pending as interrupted};
pub(crate) use rename::rename;
pub use renew::Renewal;
pub(crate) use renew::{Due, renew_due, renew_parked};
pub use settle::{GaveUp, abandon};
use settle::{drop_live_twins, purge};
pub(crate) use settle::{repair, settle, stuck};
pub(crate) use stow::find as left_login;
pub(crate) use stow::stow;
pub use stow::{Foreseen, Kept, Left, Stowed};
pub use uninstall::Removed;
pub(crate) use uninstall::{uninstall, uninstall_from_app};
pub use vault::Keeping;
pub(crate) use vault::keeping;

use crate::context::Context;
use crate::error::{Error, Result};
use crate::in_use::{self, InUse};
use crate::permit::Permit;
use crate::state::{Account, Key, Park, ParkKind, State};
use crate::warning::Warning;
use crate::{fault, holder, home, lock, park, state, store};
use journal::{Journal, clear_journal, write_journal};
use refresh::Refreshing;
use serde_json::Value;
use std::path::PathBuf;

/// Seconds a running Claude Code session takes to pick up a switch (`credential_cache`).
pub const ADOPTION_CEILING_SECONDS: u32 = 33;

#[derive(Debug)]
pub enum Outcome {
    Switched {
        /// Which tool's login moved.
        provider: ProviderId,
        from: String,
        to: String,
        parked: Park,
        /// When a session already running will be using the incoming login, as this tool
        /// answers it on this machine once the switch is made. Carried rather than read from
        /// a constant, because the honest answer for two of the three tools is that nothing
        /// follows until they are restarted, and for Claude Code it turns on what sits
        /// behind the keychain.
        adoption: provider::Adoption,
    },
    // Put in use where the tool held nobody's login, so no account was parked for it.
    Installed {
        provider: ProviderId,
        to: String,
        adoption: provider::Adoption,
    },
    /// Not a failure: the state the caller asked for already holds.
    AlreadyActive {
        label: String,
        /// The tool's own record named another account, and now names this one again, as a
        /// switch to it writes it.
        config_updated: bool,
    },
}

/// Pitboard's state, held exclusively, with any interrupted switch already finished. Every
/// command that changes state starts from one, so none acts on what a crash left behind,
/// and it carries the [`Permit`] it was settled with, which is what lets the change write.
#[derive(Debug)]
pub(crate) struct Settled {
    _exclusive: std::fs::File,
    state: State,
    ctx: Context,
    permit: Permit,
}

/// Makes Pitboard runs exclusive of each other. A kernel lock, unlike the directory lock
/// Claude Code's protocol requires around its own writes: the operating system releases it
/// when a process ends, so there is no staleness rule for two runs to both satisfy.
pub(crate) fn exclusive(ctx: &Context, permit: Permit) -> Result<std::fs::File> {
    let (file, path) = lock_file(ctx, permit)?;
    match file.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            fault::point("switch.lock_busy");
            file.lock()
                .map_err(|source| Error::HomeUnwritable { path, source })?;
        }
        Err(std::fs::TryLockError::Error(source)) => {
            return Err(Error::HomeUnwritable { path, source });
        }
    }
    Ok(file)
}

/// `exclusive` without waiting: `None` while another Pitboard run holds it.
fn try_exclusive(ctx: &Context, permit: Permit) -> Option<std::fs::File> {
    let (file, _) = lock_file(ctx, permit).ok()?;
    file.try_lock().ok()?;
    Some(file)
}

/// A switch's record, as a look that waits on nobody finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unfinished {
    /// Another run holds Pitboard's lock: the switch is that run's to finish or refuse over.
    UnderWay,
    /// No run holds the lock, so the switch waits for the next change.
    Interrupted,
}

// The lock kept over a second look at the record, so no run begins or ends a switch between.
pub(crate) fn unfinished(ctx: &Context) -> Option<Unfinished> {
    if !journal::pending(ctx) {
        return None;
    }
    match holder(ctx) {
        Holder::Another => Some(Unfinished::UnderWay),
        Holder::Nobody(None) => Some(Unfinished::Interrupted),
        Holder::Nobody(Some(_kept)) => journal::pending(ctx).then_some(Unfinished::Interrupted),
    }
}

pub(crate) enum Holder {
    Another,
    // Pitboard's lock, held shared until dropped, where its file is there.
    Nobody(Option<std::fs::File>),
}

// For a look that waits on nobody: the lock's file is opened to read, never made, and no file
// is no holder.
pub(crate) fn holder(ctx: &Context) -> Holder {
    let Ok(lock) = std::fs::File::open(home::dir(ctx).join("state.lock")) else {
        return Holder::Nobody(None);
    };
    match lock.try_lock_shared() {
        Err(std::fs::TryLockError::WouldBlock) => Holder::Another,
        _ => Holder::Nobody(Some(lock)),
    }
}

fn lock_file(ctx: &Context, permit: Permit) -> Result<(std::fs::File, PathBuf)> {
    let path = home::dir(ctx).join("state.lock");
    let fail = |source| Error::HomeUnwritable {
        path: path.clone(),
        source,
    };
    home::ensure(ctx, permit).map_err(fail)?;
    let file = crate::host::fs::open_private_lock(permit, &path).map_err(fail)?;
    Ok((file, path))
}

/// Which login this document holds. When this cannot be answered, nothing moves: a login
/// filed under a guessed account takes two accounts with it.
pub(super) fn identity_of_document(
    ctx: &Context,
    tool: &'static dyn Provider,
    document: &Value,
) -> Result<provider::Identity> {
    let credential = provider::Credential::new(tool.id(), document.clone());
    tool.identity(ctx, &credential)
        .map_err(|e| unidentified(ctx, tool.id(), e))
}

/// Why nobody could say whose a login of `which` is, as a change refuses over it.
fn unidentified(ctx: &Context, which: ProviderId, error: provider::ProviderError) -> Error {
    unidentified_as(ctx, which, crate::error::Unidentified::SignedIn, error)
}

/// [`unidentified`], for `login`: the one signed in, or the one a sign-in made.
pub(super) fn unidentified_as(
    ctx: &Context,
    which: ProviderId,
    login: crate::error::Unidentified,
    error: provider::ProviderError,
) -> Error {
    use crate::error::Unidentified;
    let spelling = crate::words::spelled(ctx, which);
    match error {
        provider::ProviderError::Unauthorized if login == Unidentified::SignedIn => {
            Error::SessionExpired {
                tool: which,
                refused: false,
                spelling,
            }
        }
        other @ (provider::ProviderError::ShapeUnexpected { .. }
        | provider::ProviderError::Unsupported { .. }
        | provider::ProviderError::CodexHomeMissing { .. }) => shape(which, other),
        provider::ProviderError::OnHold { link, .. } => Error::AccountOnHold {
            tool: which,
            login,
            link,
        },
        provider::ProviderError::Unnamed { error } => Error::IdentityUnverifiable {
            tool: which,
            login: if login == Unidentified::SignedIn {
                Unidentified::Unnamed
            } else {
                login
            },
            cause: crate::error::Cause::of_provider(&error),
            detail: error.to_string(),
            spelling,
        },
        other => Error::IdentityUnverifiable {
            tool: which,
            login,
            cause: crate::error::Cause::of_provider(&other),
            detail: other.to_string(),
            spelling,
        },
    }
}

/// Nothing is signed in to this tool, said the way the tool's own files explain it.
///
/// Nothing in any store Pitboard reads, and the tool's own record naming somebody as signed
/// in, are two different situations. The second means Pitboard is looking in the wrong
/// place, and writing a login there would put it where nobody reads.
pub(super) fn nothing_signed_in(ctx: &Context, tool: &'static dyn Provider) -> Error {
    match tool.own_record(ctx) {
        Some(found) => Error::LiveCredentialElsewhere { email: found.email },
        None => Error::LiveCredentialAbsent {
            tool: tool.id(),
            label: None,
            spelling: crate::words::spelled(ctx, tool.id()),
        },
    }
}

/// Where this tool's live login is, or why Pitboard cannot act on it here.
pub(super) fn live_store(
    ctx: &Context,
    tool: &'static dyn Provider,
) -> Result<provider::LiveStore> {
    tool.live(ctx).map_err(|e| shape(tool.id(), e))
}

/// The live login as it is stored, byte for byte, and as a document.
///
/// The bytes are kept because a failed write is judged by whether they changed, and a
/// document read back through a parser would compare equal to one whose bytes had moved.
fn read_live(
    ctx: &Context,
    tool: &'static dyn Provider,
    live: &provider::LiveStore,
) -> Result<(String, Value)> {
    read_stored(tool, live)?.ok_or_else(|| nothing_signed_in(ctx, tool))
}

// The bytes stored now, where they are what `going` says: the refused login, which still
// nobody can tell whose it is, so a sign-in since is never taken for it, or no account's login.
fn going_out(
    ctx: &Context,
    state: &State,
    tool: &'static dyn Provider,
    live: &provider::LiveStore,
    going: Going,
) -> Result<Option<String>> {
    match going {
        Going::Refused => {
            let (raw, refused) = read_live(ctx, tool, live)?;
            match identify::told(ctx, state, tool, &refused) {
                Ok(None) => Ok(Some(raw)),
                Ok(Some(_)) => Err(Error::SignedInAccountChanged),
                Err(error) => Err(unidentified(ctx, tool.id(), error)),
            }
        }
        Going::Nothing => match read_stored(tool, live)? {
            Some(_) => Err(Error::SignedInAccountChanged),
            None => Ok(store::read_raw(&live.chain, &live.service)?),
        },
    }
}

/// [`read_live`], with `None` where nothing is signed in: nothing stored, or a document with
/// no account's login in it, as Claude Code's `/logout` leaves.
fn read_stored(
    tool: &'static dyn Provider,
    live: &provider::LiveStore,
) -> Result<Option<(String, Value)>> {
    let Some(raw) = store::read_raw(&live.chain, &live.service)? else {
        return Ok(None);
    };
    let document = serde_json::from_str(&raw)
        .map_err(|e| Error::Store(store::Error::Malformed(e.to_string())))?;
    match tool.slice(&document) {
        Err(provider::ProviderError::NoLogin { .. }) => Ok(None),
        Err(other) => Err(shape(tool.id(), other)),
        Ok(_) => Ok(Some((raw, document))),
    }
}

pub(crate) fn switch(settled: Settled, key: &Key) -> Result<(Outcome, Vec<Warning>)> {
    switch_as(settled, key, OnRefusal::Ask)
}

/// [`switch`], once somebody has agreed to put `key`'s login in place of a login in use its
/// service refuses for good. Where that login can be told by then, it is a switch.
pub(crate) fn replace(settled: Settled, key: &Key) -> Result<(Outcome, Vec<Warning>)> {
    switch_as(settled, key, OnRefusal::Replace)
}

/// What a switch does where the login in use is one its service refuses for good.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnRefusal {
    /// Refuses, saying the switch would put the account's login in its place.
    Ask,
    Replace,
}

fn switch_as(settled: Settled, key: &Key, refused: OnRefusal) -> Result<(Outcome, Vec<Warning>)> {
    let tool = match provider::of(key.provider) {
        LoginKind::Document(tool) => tool,
        LoginKind::Folder(app) => return folder::switch(settled, key, app),
    };
    let Settled {
        _exclusive,
        mut state,
        ctx,
        permit,
    } = settled;
    let target = enrolled(&state, key)?;
    // Known before taking the tool's own lock so a round trip does not hold up its writes,
    // then confirmed under the lock. Recorded at once, so the record names the account a
    // switch moves out of when it records the one it moves to, whatever it named before.
    let outgoing = match identify::now(&ctx, permit, &mut state, tool) {
        Ok(identify::Live::Login(outgoing)) => outgoing,
        // `look` refuses where the tool's own record names somebody, whose login is elsewhere.
        Ok(identify::Live::Nothing) => {
            return put_in(state, &ctx, permit, tool, key, &target, Going::Nothing);
        }
        Err(error) => {
            let Some(detail) = refused_for_good(&error) else {
                return Err(error);
            };
            restorable(&ctx, &state, key, &target)?;
            return match refused {
                OnRefusal::Ask => Err(Error::LiveLoginRefused {
                    tool: key.provider,
                    label: state.typed(key),
                    detail,
                }),
                OnRefusal::Replace => {
                    put_in(state, &ctx, permit, tool, key, &target, Going::Refused)
                }
            };
        }
    };
    if target.owned_by(&outgoing.owner) {
        let lost = outgoing.lock_warning(key.provider);
        let (outcome, warnings) = already_active(&ctx, permit, &mut state, tool, key, &target);
        return Ok((outcome, lost.into_iter().chain(warnings).collect()));
    }
    let (switched, warnings) = switch_from(state, &ctx, permit, tool, key, outgoing)?;
    Ok((switched.into(), warnings))
}

/// What [`switch`] does for the account in use, for a button drawn while it was: writes it
/// into its tool's own record where that names another account. Refused, moving nothing,
/// where another account's login is stored by then, which [`switch`] would switch back from.
pub(crate) fn update_config(settled: Settled, key: &Key) -> Result<(Outcome, Vec<Warning>)> {
    let tool = match provider::of(key.provider) {
        LoginKind::Document(tool) => tool,
        LoginKind::Folder(app) => return folder::confirm(settled, key, app),
    };
    let Settled {
        _exclusive,
        mut state,
        ctx,
        permit,
    } = settled;
    let target = enrolled(&state, key)?;
    match identify::now(&ctx, permit, &mut state, tool)? {
        identify::Live::Login(stored) if target.owned_by(&stored.owner) => {
            let lost = stored.lock_warning(key.provider);
            let (outcome, warnings) = already_active(&ctx, permit, &mut state, tool, key, &target);
            Ok((outcome, lost.into_iter().chain(warnings).collect()))
        }
        _ => Err(Error::AccountNotInUse {
            tool: key.provider,
            label: state.typed(key),
        }),
    }
}

/// Why the login in use no longer works, where its service refused it for good or it has none.
// For good is `invalid_grant` alone, as Claude Code takes it (the register's `token_refusals`): a
// login its service will not name may work, and one on hold comes back.
fn refused_for_good(error: &Error) -> Option<String> {
    matches!(error, Error::SessionExpired { refused: true, .. }).then(|| identify::untold(error))
}

/// `target`'s parked login, where a switch to it could be made now.
fn restorable(ctx: &Context, state: &State, key: &Key, target: &Account) -> Result<Park> {
    let held = target.parked.clone().ok_or_else(|| Error::NothingParked {
        tool: key.provider,
        label: state.typed(key),
    })?;
    if !held.restorable_at(ctx.now()) {
        return Err(Error::ParkedLoginExpired {
            label: state.typed(key),
        });
    }
    Ok(held)
}

// What a login goes in place of where nothing is parked going out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Going {
    // A login whose refresh token its service refuses for good, which no longer works.
    Refused,
    // No account's login, with the tool's own record naming nobody: a tool never signed in.
    Nothing,
}

// What is stored is read again under the tool's lock, and a login that turned up or changed
// meanwhile, as a sign-in leaves it, is not written over.
fn put_in(
    mut state: State,
    ctx: &Context,
    permit: Permit,
    tool: &'static dyn Provider,
    key: &Key,
    target: &Account,
    going: Going,
) -> Result<(Outcome, Vec<Warning>)> {
    let which = key.provider;
    let to = state.typed(key);
    let from = match going {
        Going::Refused => format!("{}'s refused login", which.name()),
        Going::Nothing => format!("{} signed in to nobody", which.name()),
    };
    let held = restorable(ctx, &state, key, target)?;
    let live = live_store(ctx, tool)?;
    let gone = going_out(ctx, &state, tool, &live, going)?;
    let incoming = park::load(ctx, tool, key, &held)?;
    kept(ctx, tool, &to, &incoming, &target.owner())?;
    let (held, incoming) =
        prove_incoming(ctx, permit, &mut state, tool, key, target, held, incoming)?;

    let refreshing = Refreshing::take(ctx, permit, tool)?;
    let guard = write_lock(ctx, permit, tool)?;
    if store::read_raw(&live.chain, &live.service)? != gone {
        return Err(Error::SignedInAccountChanged);
    }
    let before = match &gone {
        Some(raw) => serde_json::from_str(raw)
            .map_err(|e| Error::Store(store::Error::Malformed(e.to_string())))?,
        None => Value::Object(serde_json::Map::new()),
    };
    // Empty where nothing was stored, which a rollback deletes.
    let before_raw = gone.unwrap_or_default();
    let next = to_body(
        &tool
            .splice(&before, &incoming)
            .map_err(|e| shape(which, e))?,
    );
    let on_the_command_line = priced(which, &live, &next, &to)?;
    // While it waits, nothing renews the park going in, which a session may have renewed
    // in the slot by then, and the next change finishes or undoes it as a switch's.
    write_journal(
        ctx,
        permit,
        &Journal {
            provider: which,
            started_at: ctx.now(),
            from_label: String::new(),
            from_id: String::new(),
            to_label: key.label.to_string(),
            to_id: target.id.clone(),
            park_service: String::new(),
            incoming_service: held.service.clone(),
            from_fingerprint: tool.fingerprint(&before),
            to_fingerprint: held.login.clone(),
            slot: Some(tool.slot(ctx)),
        },
    )?;
    fault::point("replace.journal_written");
    // The park going in is kept until the slot is seen to hold it, so a write that did not
    // land loses nothing; only a slot nobody can read keeps the record, for the next change.
    let failed = |detail: String| match going {
        Going::Refused => Error::ReplaceFailed {
            tool: which,
            label: to.clone(),
            detail,
        },
        Going::Nothing => Error::InstallFailed {
            tool: which,
            label: to.clone(),
            detail,
        },
    };
    if let Err(e) = install_with(
        ctx,
        which,
        |body| match body {
            "" => store::delete_raw(&live.chain, permit, &live.service),
            body => store::write_raw(&live.chain, permit, &live.service, body),
        },
        || store::read_raw(&live.chain, &live.service),
        &next,
        &before_raw,
        &from,
        &to,
    ) {
        return Err(match e {
            Error::SwitchUnverified { .. } => e,
            Error::SwitchRolledBack { detail, .. } | Error::SwitchCorrupted { detail, .. } => {
                clear_journal(ctx, permit);
                failed(detail)
            }
            other => {
                clear_journal(ctx, permit);
                failed(other.to_string())
            }
        });
    }
    fault::point("replace.installed");
    let lock_lost = guard.as_ref().is_some_and(lock::Guard::compromised);
    match holds(tool, &live) {
        Ok(true) => {}
        Ok(false) => {
            clear_journal(ctx, permit);
            return Err(failed(
                "the login was gone again before Pitboard finished".into(),
            ));
        }
        Err(unreadable) => {
            return Err(Error::SwitchUnverified {
                tool: which,
                from,
                to,
                detail: unreadable.to_string(),
            });
        }
    }

    state.discard(&held.service, ParkKind::Vault);
    let named = tool.own_record(ctx);
    state.identified(
        which,
        InUse {
            owner: Some(target.owner()),
            login: tool.fingerprint(&incoming),
            known_at: ctx.now(),
            named: named
                .as_ref()
                .and_then(|named| in_use::naming(which, named)),
        },
        ctx.now(),
    );
    state::save(ctx, permit, &state)?;
    drop(guard);
    let lock_lost = lock_lost | Refreshing::let_go(refreshing);
    let cache_warning = write_config(
        ctx,
        permit,
        &mut state,
        tool,
        target,
        &named.unwrap_or_else(|| target.owner()),
    );
    let parks_pending = purge(ctx, permit, &mut state);
    clear_journal(ctx, permit);
    // A session that started with no login keeps none, which no warning about a login says.
    let still_running = match (going, still_holding(ctx, which)) {
        (Going::Nothing, _) | (_, StillHolding::Nothing) => None,
        (Going::Refused, StillHolding::These(holding)) => Some(Warning::SessionsStillRunning {
            from: from.clone(),
            holding,
        }),
        (Going::Refused, StillHolding::Unknown) => Some(Warning::SessionsUnknown {
            tool: which,
            from: from.clone(),
        }),
    };
    let warnings = on_the_command_line
        .into_iter()
        .chain(still_running)
        .chain(lock_lost.then_some(Warning::LockCompromised { tool: which }))
        .chain(cache_warning)
        .chain((parks_pending > 0).then_some(Warning::ParksPendingRemoval(parks_pending)))
        .collect();
    Ok((
        Outcome::Installed {
            provider: which,
            to,
            adoption: tool.adoption(tool.behind(ctx).as_ref()),
        },
        warnings,
    ))
}

/// The account enrolled under `key`, or a refusal that lists the labels that are.
fn enrolled(state: &State, key: &Key) -> Result<Account> {
    state
        .get(key)
        .cloned()
        .ok_or_else(|| Error::AccountUnknown {
            label: key.typed(),
            enrolled: state.labels(key.provider),
        })
}

/// A switch made: which tool's login moved, between which accounts, the outgoing login's new
/// park, and when sessions already running take it. What [`Outcome::Switched`] says.
struct Switched {
    provider: ProviderId,
    from: String,
    to: String,
    parked: Park,
    adoption: provider::Adoption,
}

impl From<Switched> for Outcome {
    fn from(switched: Switched) -> Outcome {
        let Switched {
            provider,
            from,
            to,
            parked,
            adoption,
        } = switched;
        Outcome::Switched {
            provider,
            from,
            to,
            parked,
            adoption,
        }
    }
}

/// [`switch`] to `key`'s account from `outgoing`, the login its tool has stored, which the
/// caller told whose it is and recorded under Pitboard's lock, and which is not the target's.
/// The automatic switch decides from that same telling, so nothing comes between what it
/// decided and the login it moves.
fn switch_from(
    mut state: State,
    ctx: &Context,
    permit: Permit,
    tool: &'static dyn Provider,
    key: &Key,
    outgoing: identify::Login,
) -> Result<(Switched, Vec<Warning>)> {
    let label = &key.label;
    let target = enrolled(&state, key)?;
    let identify::Login {
        store: live,
        document: first,
        owner: outgoing,
        lock_lost: renewal_lock_lost,
    } = outgoing;
    let (outgoing_key, outgoing_id) = state
        .account_of(key.provider, &outgoing)
        .map(|account| (account.key(), account.id.clone()))
        .ok_or_else(|| Error::LiveAccountNotEnrolled {
            tool: key.provider,
            who: crate::words::login(
                key.provider,
                &outgoing.email,
                state
                    .accounts
                    .iter()
                    .any(|a| a.provider() == key.provider && a.email == outgoing.email),
            ),
        })?;
    let (from, to) = (state.typed(&outgoing_key), state.typed(key));
    let held = target.parked.clone().ok_or_else(|| Error::NothingParked {
        tool: key.provider,
        label: to.clone(),
    })?;
    if !held.restorable_at(ctx.now()) {
        return Err(Error::ParkedLoginExpired { label: to });
    }
    let incoming = park::load(ctx, tool, key, &held)?;
    kept(ctx, tool, &to, &incoming, &target.owner())?;
    // Asked before the tool's lock is taken, like the outgoing question, so the round trip
    // does not hold up its writes.
    let (held, incoming) =
        prove_incoming(ctx, permit, &mut state, tool, key, &target, held, incoming)?;

    // A renewal by a running session spends the refresh token it sends, and the tool's write
    // lock does not keep one from starting. So the lock a renewal takes first is held from
    // before the outgoing login is read for the last time until the login installed in its
    // place is recorded: the copy parked is then never one a session has spent.
    let refreshing = Refreshing::take(ctx, permit, tool)?;
    let guard = write_lock(ctx, permit, tool)?;
    let Readied {
        before_raw,
        before,
        next,
        on_the_command_line,
    } = ready(ctx, tool, &live, &first, &outgoing, &incoming, &to)?;

    // The outgoing login's park has a ceiling of its own on a machine whose vault is the
    // keychain, and it is asked about now, while refusing still changes nothing. The name
    // is the one `reserve` is about to make, give or take the millisecond, which is all
    // the price depends on.
    let parking = park::price(
        ctx,
        key.provider,
        &from,
        &park::service_name(&outgoing_id, ctx.now_millis()),
        &tool.slice(&before).map_err(|e| shape(key.provider, e))?,
    )?;
    let park_service = park::reserve(ctx, permit, &outgoing_id)?;
    write_journal(
        ctx,
        permit,
        &Journal {
            provider: key.provider,
            started_at: ctx.now(),
            from_label: outgoing_key.label.clone(),
            from_id: outgoing_id,
            to_label: label.to_string(),
            to_id: target.id.clone(),
            park_service: park_service.clone(),
            incoming_service: held.service.clone(),
            // Which side the live credential came from, answerable without asking anyone.
            from_fingerprint: tool.fingerprint(&before),
            to_fingerprint: held.login.clone(),
            slot: Some(tool.slot(ctx)),
        },
    )?;
    fault::point("switch.journal_written");

    // Until the incoming login is installed there is nothing for a later run to finish, so
    // a failure here takes the record of intent away with it. A copy that was written but
    // could not be recorded is deleted: nothing that survives would name it.
    let slice = tool.slice(&before).map_err(|e| shape(key.provider, e))?;
    let parked = match park::store_at(ctx, permit, tool, &park_service, &slice) {
        Ok(parked) => parked,
        Err(e) => {
            clear_journal(ctx, permit);
            return Err(e);
        }
    };
    fault::point("switch.park_stored");
    state.park(&outgoing_key, parked.clone());
    if let Err(e) = state::save(ctx, permit, &state) {
        let _ = store::vault_delete(ctx, permit, &parked.service);
        clear_journal(ctx, permit);
        return Err(e);
    }
    fault::point("switch.park_recorded");

    // For a tool whose own sign-out revokes whatever it finds stored, there must never be
    // two usable copies of one account's login at rest. The copy is read back before
    // anything overwrites the original, so a park that did not survive the write is found
    // here, while the login it copies is still where it was.
    if tool.park_semantics() == provider::ParkSemantics::MoveOnly
        && store::vault_read(ctx, &parked.service)?.is_none()
    {
        state.release(&parked.service);
        state::save(ctx, permit, &state)?;
        clear_journal(ctx, permit);
        return Err(Error::ParkedCredentialMissing { label: from });
    }

    // The live login is read once more before it is replaced. A tool that takes no write
    // lock, which is Codex, can have rewritten it since it was read under nothing at all:
    // a session still running from before the switch refreshing its token, which spends
    // the chain just parked and leaves the only live copy of it in the file this write is
    // about to replace. Nothing is installed over a login that moved. Where the one there
    // now is still the outgoing account's, the park is a spent copy of it and is dropped;
    // where it is anybody else's, or cannot be told, the park may be the outgoing account's
    // only login and is kept. The window left is the rename itself.
    let now = store::read_raw(&live.chain, &live.service);
    if !matches!(&now, Ok(Some(now)) if *now == before_raw) {
        let still_outgoing = now
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .and_then(|document| identity_of_document(ctx, tool, &document).ok())
            .is_some_and(|found| found == outgoing.identity());
        if still_outgoing {
            state.discard(&parked.service, ParkKind::Vault);
        }
        state::save(ctx, permit, &state)?;
        clear_journal(ctx, permit);
        purge(ctx, permit, &mut state);
        return Err(Error::SignedInAccountChanged);
    }

    if let Err(e) = install_with(
        ctx,
        key.provider,
        |body| store::write_raw(&live.chain, permit, &live.service, body),
        || store::read_raw(&live.chain, &live.service),
        &next,
        &before_raw,
        &from,
        &to,
    ) {
        // Nobody could say what the slot holds. Keep every copy, and keep the record of
        // intent, so the next run with a store that answers finishes this or undoes it.
        // Everything below deletes something or forgets something, and neither is a thing
        // to do without knowing.
        if matches!(e, Error::SwitchUnverified { .. }) {
            return Err(e);
        }
        if !only_copy_left(&e) {
            state.discard(&parked.service, ParkKind::Vault);
        }
        state::save(ctx, permit, &state)?;
        clear_journal(ctx, permit);
        purge(ctx, permit, &mut state);
        return Err(e);
    }
    fault::point("switch.installed");

    // The write landed. That is not the same as it having held: a `/logout` that has given
    // up waiting deletes the credential with no lock held (the register's
    // `logout_skips_the_lock`), and a lock that aged out while the machine slept lets the tool
    // reclaim it and write underneath. Both cost one read to notice here, and a browser
    // sign-in to discover later.
    //
    // Checked before the incoming copy is discarded, so finding it did not hold leaves both
    // logins parked rather than neither.
    let lock_lost = guard.as_ref().is_some_and(lock::Guard::compromised);
    match holds(tool, &live) {
        Ok(true) => {}
        Ok(false) => {
            clear_journal(ctx, permit);
            return Err(Error::SwitchDidNotHold {
                tool: key.provider,
                from,
                to,
                spelling: crate::words::spelled(ctx, key.provider),
            });
        }
        Err(unreadable) => {
            return Err(Error::SwitchUnverified {
                tool: key.provider,
                from,
                to,
                detail: unreadable.to_string(),
            });
        }
    }

    state.discard(&held.service, ParkKind::Vault);
    let installed = InUse {
        owner: Some(target.owner()),
        login: tool.fingerprint(&incoming),
        known_at: ctx.now(),
        // What the tool's own record named when the store was read, until `write_config`
        // writes `target` there.
        named: state
            .in_use(key.provider)
            .and_then(|known| known.named.clone()),
    };
    state.identified(key.provider, installed, ctx.now());
    state::save(ctx, permit, &state)?;
    fault::point("switch.recorded");
    drop(guard);
    let lock_lost = lock_lost | Refreshing::let_go(refreshing) | renewal_lock_lost;

    let cache_warning = write_config(ctx, permit, &mut state, tool, &target, &outgoing);
    fault::point("switch.config_updated");
    let parks_pending = purge(ctx, permit, &mut state);
    clear_journal(ctx, permit);

    // A tool that never follows a switch on its own goes on using the outgoing account in
    // everything of it already running. Said with what is running, because "restart it"
    // means nothing to somebody who does not know one is open, with what makes each kind
    // take the switch, and with the one thing not to do in any of them. Where nobody could
    // tell what is running, that is said, with the same thing not to do.
    let still_running = match still_holding(ctx, key.provider) {
        StillHolding::Nothing => None,
        StillHolding::These(holding) => Some(Warning::SessionsStillRunning {
            from: from.clone(),
            holding,
        }),
        StillHolding::Unknown => Some(Warning::SessionsUnknown {
            tool: key.provider,
            from: from.clone(),
        }),
    };
    let warnings = on_the_command_line
        .into_iter()
        .chain(parking)
        .chain(still_running)
        .chain(lock_lost.then_some(Warning::LockCompromised { tool: key.provider }))
        .chain(cache_warning)
        .chain((parks_pending > 0).then_some(Warning::ParksPendingRemoval(parks_pending)))
        .collect();
    Ok((
        Switched {
            provider: key.provider,
            adoption: tool.adoption(tool.behind(ctx).as_ref()),
            from,
            to,
            parked,
        },
        warnings,
    ))
}

/// `use` of `target`, the account under `key`, whose login its tool has stored: writes it
/// into the tool's own record where that names another account, as a switch to it does.
fn already_active(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    tool: &'static dyn Provider,
    key: &Key,
    target: &Account,
) -> (Outcome, Vec<Warning>) {
    let which = key.provider;
    let naming = in_use::naming(which, &target.owner());
    let named = tool
        .own_record(ctx)
        .filter(|named| Some(state::new_id(which, named)) != naming);
    let (config_updated, warnings) = match named {
        None => (false, Vec::new()),
        Some(named) => match write_config(ctx, permit, state, tool, target, &named) {
            None => (true, Vec::new()),
            Some(warning) => (false, vec![warning]),
        },
    };
    (
        Outcome::AlreadyActive {
            label: state.typed(key),
            config_updated,
        },
        warnings,
    )
}

/// Writes `target`, whose login `tool` has stored and is recorded as in use, into the
/// tool's own record over `named`, and records that it names `target` once the write lands.
/// A write that fails is the warning returned, and leaves the record naming what the tool's
/// own record named when the store was read, so every read says it names another account.
fn write_config(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    tool: &'static dyn Provider,
    target: &Account,
    named: &provider::Owner,
) -> Option<Warning> {
    let which = tool.id();
    if let Err(error) = tool.after_switch(ctx, permit, target, named) {
        return Some(Warning::ConfigNotUpdated(error));
    }
    if let Some(record) = state.in_use(which).cloned() {
        let naming = InUse {
            named: in_use::naming(which, &target.owner()),
            ..record
        };
        // Unsaved, the next read finds the tool's own record naming `target` and records it.
        if state.identified(which, naming, ctx.now()).changed {
            let _ = state::save(ctx, permit, state);
        }
    }
    None
}

/// A write to a tool's live login, made ready under the tool's own lock.
struct Readied {
    /// The login there now, byte for byte and as a document.
    before_raw: String,
    before: Value,
    /// What goes in its place.
    next: String,
    on_the_command_line: Option<Warning>,
}

/// The lock `tool` takes around its own writes to its live login, taken the way it
/// takes it, where it takes one. The caller holds it from before it readies a write until it
/// has recorded what it wrote.
fn write_lock(
    ctx: &Context,
    permit: Permit,
    tool: &'static dyn Provider,
) -> Result<Option<lock::Guard>> {
    Ok(tool
        .write_lock(ctx)
        .map(|dir| lock::acquire(permit, &dir, ctx.locks.write))
        .transpose()?)
}

/// Readies `incoming` to go in place of the login read earlier as `first`, which was
/// `signed_in`'s, with the tool's write lock already held. `label` is the account
/// `incoming` is.
///
/// Refuses, having written nothing, when somebody else is signed in by now, or when what
/// would be written could never be.
fn ready(
    ctx: &Context,
    tool: &'static dyn Provider,
    live: &provider::LiveStore,
    first: &Value,
    signed_in: &provider::Owner,
    incoming: &Value,
    label: &str,
) -> Result<Readied> {
    let which = tool.id();
    let (before_raw, before) = read_live(ctx, tool, live)?;
    // A refresh keeps the account, so an unchanged share of the document needs no second
    // question. A sign-in between the two reads would not keep it.
    if tool.slice(&before).ok() != tool.slice(first).ok()
        && identity_of_document(ctx, tool, &before)? != signed_in.identity()
    {
        return Err(Error::SignedInAccountChanged);
    }

    // Checked before anything is parked or written, so a change that could never be written
    // changes nothing.
    let next = to_body(
        &tool
            .splice(&before, incoming)
            .map_err(|e| shape(which, e))?,
    );
    let on_the_command_line = priced(which, live, &next, label)?;
    Ok(Readied {
        before_raw,
        before,
        next,
        on_the_command_line,
    })
}

/// Refuses `next` where the backend that would take it cannot, and says where it goes on the
/// command line. Asked once: the answer is about that backend, so a login living in a
/// fallback file is not told it has the keychain's ceiling. Said rather than hidden, since the
/// same bytes are visible to `ps` for the length of one `security` call.
fn priced(
    which: ProviderId,
    live: &provider::LiveStore,
    next: &str,
    label: &str,
) -> Result<Option<Warning>> {
    let price = store::cost(&live.chain, &live.service, next);
    if let Some(price) = price.filter(|price| price.refused()) {
        return Err(Error::CredentialTooLarge {
            tool: which,
            label: label.to_string(),
            bytes: price.needs,
            limit: price.limit,
            store: price.store,
        });
    }
    Ok(price
        .filter(|p| p.on_the_second_route())
        .map(|p| Warning::WrittenOnTheCommandLine {
            tool: which,
            bytes: p.needs,
            limit: p.limit,
        }))
}

/// Whether a login of the tool's shape is in its live slot, read back after a write.
///
/// The tool may have rotated the token it was just given, which keeps the account and
/// changes the bytes: a login of its shape being there at all is the fact.
fn holds(
    tool: &'static dyn Provider,
    live: &provider::LiveStore,
) -> std::result::Result<bool, store::Error> {
    store::read_raw(&live.chain, &live.service).map(|now| {
        now.and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .is_some_and(|document| tool.slice(&document).is_ok())
    })
}

/// What is running a tool whose running sessions keep the login they started with, which a
/// change of that login leaves on the login they had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StillHolding {
    /// Nothing is: no process runs the tool, or its sessions follow a change by themselves.
    Nothing,
    /// These are, by kind. Never empty.
    These(Vec<holder::Holding>),
    /// The process list could not be read, so nobody can say whether anything is. Said as
    /// such, because taken for nothing it would tell somebody with sessions open that none
    /// are.
    Unknown,
}

/// What is running `which`'s tool with the login it started with, by kind, as the process
/// list says.
pub(crate) fn still_holding(ctx: &Context, which: ProviderId) -> StillHolding {
    // What sits behind a store only delays sessions that follow by themselves, which hold
    // nothing here, so it is not read.
    let tool = match provider::of(which) {
        LoginKind::Document(tool) => tool,
        LoginKind::Folder(app) => {
            return match folder::open(ctx, app) {
                folder::Open::No => StillHolding::Nothing,
                folder::Open::Yes(listed) => StillHolding::These(vec![holder::Holding {
                    holder: *app.holder(),
                    pids: listed.iter().map(|one| one.pid).collect(),
                    program: which.program(),
                }]),
                folder::Open::Unknown(_) => StillHolding::Unknown,
            };
        }
    };
    let provider::Adoption::RestartRequired { program, holders } = tool.adoption(None) else {
        return StillHolding::Nothing;
    };
    match holder::find(ctx, which, program, holders) {
        None => StillHolding::Unknown,
        Some(holding) if holding.is_empty() => StillHolding::Nothing,
        Some(holding) => StillHolding::These(holding),
    }
}

/// Ask the service about the login going in, not only about the one coming out.
///
/// A switch used to ask twice about the login it was throwing away and never once about
/// the one it was installing. If that account's refresh chain had been revoked, signed out
/// elsewhere or refused by the service, the switch installed it, read it back, found a login
/// there and reported success; the person discovered they were signed out the next time
/// they ran the tool, with no login to go back to on either side.
///
/// A lapsed park is renewed rather than refused, which is the whole point of parking one.
/// That is a write, so it happens here, before anything is parked, while both copies are
/// still where they were.
#[allow(clippy::too_many_arguments)]
fn prove_incoming(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    tool: &'static dyn Provider,
    key: &Key,
    target: &Account,
    held: Park,
    incoming: Value,
) -> Result<(Park, Value)> {
    let label = state.typed(key);
    if held.askable_at(ctx.now()) {
        let credential = provider::Credential::new(key.provider, incoming.clone());
        match tool.verify(ctx, &credential) {
            Ok(found) if target.holds(&found) => return Ok((held, incoming)),
            Ok(other) => {
                let email = state
                    .named(key.provider, &other)
                    .map(|owner| owner.email)
                    .or_else(|| tool.name(ctx, &credential, &other).ok());
                return Err(Error::ParkedLoginBelongsElsewhere {
                    label,
                    who: email.map_or_else(
                        || "another account".to_owned(),
                        |email| crate::words::login(key.provider, &email, email == target.email),
                    ),
                });
            }
            // The access token has lapsed earlier than the recorded expiry said it would.
            // Renewing settles it either way.
            Err(provider::ProviderError::Unauthorized) => {}
            Err(provider::ProviderError::ShapeUnexpected { detail, .. }) => {
                return Err(Error::ParkedCredentialCorrupt { label, detail });
            }
            Err(provider::ProviderError::OnHold { link, .. }) => {
                return Err(Error::AccountOnHold {
                    tool: key.provider,
                    login: crate::error::Unidentified::Parked { label },
                    link,
                });
            }
            Err(e) => {
                return Err(Error::IdentityUnverifiable {
                    tool: key.provider,
                    login: crate::error::Unidentified::Parked {
                        label: label.clone(),
                    },
                    cause: crate::error::Cause::of_provider(&e),
                    detail: e.to_string(),
                    spelling: crate::words::spelled(ctx, key.provider),
                });
            }
        }
    }

    // Lapsed, or refused as lapsed. Renew it and switch to what comes back.
    let Some(fresh) = renew::renew_one(ctx, permit, state, tool, key, &held)? else {
        return Err(Error::IdentityUnverifiable {
            tool: key.provider,
            login: crate::error::Unidentified::Parked {
                label: label.clone(),
            },
            cause: crate::error::Cause::Unreachable,
            detail: format!(
                "it needs renewing and {} did not answer",
                key.provider.service()
            ),
            spelling: crate::words::spelled(ctx, key.provider),
        });
    };
    let document = park::load(ctx, tool, key, &fresh)?;
    Ok((fresh, document))
}

// Refused before anything moves where the tool would sign `document` out or leave it unused.
pub(super) fn kept(
    ctx: &Context,
    tool: &'static dyn Provider,
    label: &str,
    document: &Value,
    owner: &provider::Owner,
) -> Result<()> {
    tool.refuses(ctx, document, owner).map_or(Ok(()), |reason| {
        Err(Error::LoginRestricted {
            tool: tool.id(),
            label: label.to_owned(),
            reason,
        })
    })
}

/// After a failed install, whether the copy just parked is the outgoing account's only login.
/// Once its old login is back in place, the copy is a second holder that Claude Code will
/// rotate past; if it could not be put back, the copy is all that is left of it.
fn only_copy_left(failure: &Error) -> bool {
    matches!(failure, Error::SwitchCorrupted { .. })
}

/// A provider saying a login is not the shape it keeps.
///
/// Only the shape errors reach the switch this way. Anything about the network keeps the
/// code the step it happened in gives it, because "could not reach Anthropic while proving
/// who the incoming login belongs to" and "could not reach Anthropic for usage" are the
/// same failure and not the same problem.
pub(super) fn shape(tool: ProviderId, error: provider::ProviderError) -> Error {
    match error {
        provider::ProviderError::ShapeUnexpected { detail, .. } => {
            Error::LiveCredentialShapeUnexpected { tool, detail }
        }
        provider::ProviderError::Unsupported { reason, .. } => {
            Error::LiveStoreUnsupported { tool, reason }
        }
        provider::ProviderError::CodexHomeMissing { path } => Error::CodexHomeMissing { path },
        other => Error::LiveCredentialShapeUnexpected {
            tool,
            detail: other.to_string(),
        },
    }
}

/// A spliced document as bytes to write.
fn to_body(document: &Value) -> String {
    serde_json::to_string(document).expect("a credential document stays serialisable")
}

/// Write the new login, and if that fails, leave the old one in place.
///
/// A failed write often changes nothing, so the slot is read back before deciding a
/// rollback is needed. That read-back has three answers, not two. It used to have two, and
/// the missing one is the likeliest failure there is: on a machine whose keychain is
/// locked, the write fails, the read-back fails too, "could not read" was taken to mean
/// "the slot changed", a rollback was attempted, that failed as well, and the person was
/// told Pitboard could not put their login back and they should sign in again. Nothing had
/// been written and their login had never moved.
#[allow(clippy::too_many_arguments)]
fn install_with(
    ctx: &Context,
    tool: ProviderId,
    write: impl Fn(&str) -> std::result::Result<(), store::Error>,
    read: impl Fn() -> std::result::Result<Option<String>, store::Error>,
    next: &str,
    before_raw: &str,
    from: &str,
    to: &str,
) -> Result<()> {
    let Err(failure) = write(next) else {
        return Ok(());
    };
    let rolled_back = |detail: String| Error::SwitchRolledBack {
        from: from.to_string(),
        to: to.to_string(),
        detail,
    };
    match read() {
        // Unchanged. The write never landed and there is nothing to undo.
        Ok(Some(now)) if now == before_raw => return Err(rolled_back(failure.to_string())),
        // Written partway, which only a whole write ends. Put back what was there.
        Err(store::Error::Unfinished(_)) => {}
        // Could not tell. Change nothing further and keep every copy: the caller leaves its
        // record of intent in place so a later run, with a store that answers, decides.
        Err(unreadable) => {
            return Err(Error::SwitchUnverified {
                tool,
                from: from.to_string(),
                to: to.to_string(),
                detail: format!("{failure}; {unreadable}"),
            });
        }
        // Changed, or gone. Put back what was there.
        _ => {}
    }
    match write(before_raw) {
        Ok(()) => Err(rolled_back(failure.to_string())),
        Err(rollback) => Err(Error::SwitchCorrupted {
            tool,
            from: from.to_string(),
            to: to.to_string(),
            detail: format!("{failure}; {rollback}"),
            spelling: crate::words::spelled(ctx, tool),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::RefCell;

    #[test]
    fn a_tools_home_that_names_no_folder_refuses_with_its_own_code() {
        let path = PathBuf::from("/nowhere/codex");
        let missing = || provider::ProviderError::CodexHomeMissing { path: path.clone() };
        let codex = ProviderId::Codex;
        let ctx = Context::for_unit_test();
        for refused in [
            shape(codex, missing()),
            unidentified(&ctx, codex, missing()),
        ] {
            assert_eq!(refused.code(), "codex_home_missing");
            assert_eq!(refused.exit_code(), 1);
            let said = refused.to_string();
            assert!(said.starts_with("CODEX_HOME is /nowhere/codex, which is no folder"));
        }
    }

    // Windows PowerShell 5.1 refuses npm's `claude.ps1` and runs the `.cmd` (2026-10-18).
    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "a test lays out an npm prefix as `npm install -g` leaves one"
    )]
    fn nothing_signed_in_names_the_cmd_pitboard_found_on_windows() {
        use crate::host::Os;
        let scratch = tempfile::tempdir().expect("a scratch home");
        let npm = scratch.path().join("npm");
        let native = scratch.path().join("bin");
        let programs = [
            (&npm, "claude"),
            (&npm, "claude.cmd"),
            (&npm, "claude.ps1"),
            (&npm, "codex"),
            (&npm, "codex.cmd"),
            (&npm, "codex.ps1"),
            (&native, "claude.exe"),
            (&native, "codex.exe"),
        ];
        for (folder, name) in programs {
            std::fs::create_dir_all(folder).expect("a folder");
            std::fs::write(folder.join(name), "").expect("a program");
            crate::host::fs::testing::make_runnable(&folder.join(name)).expect("runnable");
        }
        let home = scratch.path().join("home");
        for (os, folder, claude, codex) in [
            (Os::MacOs, &npm, "claude", "codex"),
            (Os::Linux, &npm, "claude", "codex"),
            (Os::Windows, &npm, "claude.cmd", "codex.cmd"),
            (Os::Windows, &native, "claude", "codex"),
        ] {
            let env = crate::context::Environment::on(
                os,
                [
                    ("HOME", home.as_os_str()),
                    ("USERPROFILE", home.as_os_str()),
                    ("PITBOARD_HOME", home.join("pitboard").as_os_str()),
                    ("CLAUDE_CONFIG_DIR", home.join("claude").as_os_str()),
                    ("CODEX_HOME", home.join("codex").as_os_str()),
                    ("PATH", folder.as_os_str()),
                ],
            );
            let ctx = Context::for_command_line(&env);
            let said = |tool| nothing_signed_in(&ctx, tool).to_string();
            assert_eq!(
                said(provider::claude_code()),
                format!(
                    "nothing is signed in to Claude Code right now. Run `{claude}`, sign in, then \
                     try again."
                ),
                "{os:?} {}",
                folder.display()
            );
            assert_eq!(
                said(provider::codex()),
                format!(
                    "nothing is signed in to Codex right now. Run `{codex} login`, sign in, then \
                     try again."
                ),
                "{os:?} {}",
                folder.display()
            );
        }
    }

    // Another run's lock, held in another process: this run waits on nobody and gets nothing,
    // and the look that waits on nobody finds the switch under way. A lock file that is not
    // there is nobody's.
    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "a test leaves a switch's record as an interrupted run would"
    )]
    fn a_lock_another_process_holds_keeps_a_run_out_and_a_missing_one_holds_nobody() {
        use crate::helper::{Helper, Job};
        let m = harness::machine("held-elsewhere");
        let permit = Permit::for_a_test();
        let lock = home::dir(&m.ctx).join("state.lock");
        let _ = std::fs::remove_file(&lock);
        std::fs::write(home::dir(&m.ctx).join("journal.json"), "{}").expect("a switch's record");
        assert_eq!(unfinished(&m.ctx), Some(Unfinished::Interrupted));
        assert!(!lock.exists(), "looking makes no lock");

        let other = Helper::start(Job::Locks, &lock);
        assert!(try_exclusive(&m.ctx, permit).is_none());
        assert_eq!(unfinished(&m.ctx), Some(Unfinished::UnderWay));
        other.stop();
        assert_eq!(unfinished(&m.ctx), Some(Unfinished::Interrupted));
        assert!(try_exclusive(&m.ctx, permit).is_some());
    }

    fn failing(message: &str) -> store::Error {
        store::Error::Write(message.into())
    }

    /// After a sign-in outside Pitboard the record can still name the account a switch goes
    /// to. The switch moves the store from the login it found there, so the account it goes
    /// to comes to be in use then, and the login it found is the one parked.
    #[test]
    fn a_switch_after_a_sign_in_outside_puts_the_account_it_goes_to_in_use() {
        let m = harness::machine("records-both-logins");
        let mut state = state::load(&m.ctx).expect("state");
        let before = InUse::of(
            state.get(&m.key("there")).expect("enrolled"),
            "signed-in-over",
            harness::NOW - 3600,
        );
        state
            .in_use
            .insert(ProviderId::Claude.code().into(), before);
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");

        let settled = settle(&m.ctx, Permit::for_a_test(), None)
            .expect("nothing to recover")
            .0;
        switch(settled, &m.key("there")).expect("switched");

        let state = state::load(&m.ctx).expect("state");
        let there = state.get(&m.key("there")).expect("enrolled");
        assert_eq!(
            there.last_used_at,
            Some(harness::NOW),
            "in use from this switch"
        );
        assert_eq!(
            state.in_use(ProviderId::Claude).map(|r| r.login.as_str()),
            Some(
                provider::document(ProviderId::Claude)
                    .fingerprint(&harness::oauth("there-refresh", 30))
                    .as_str()
            )
        );
        assert!(
            state
                .get(&m.key("here"))
                .expect("enrolled")
                .parked
                .is_some()
        );
    }

    /// A session renewing Claude Code's login takes the refresh lock before it sends the
    /// refresh token, and saves the answer only where the login stored still holds the token
    /// it sent (the register's `refresh_lock`). A switch holds that lock from before it reads
    /// the outgoing login until it has recorded the one it installed, so a session renewing
    /// meanwhile waits: the login parked for the account switched from is never one a session
    /// spent, with its renewal lost at a save that found another login stored.
    #[test]
    fn a_session_renewing_the_outgoing_login_during_a_switch_waits_for_it() {
        use harness::{Session, lock_dir, renews_meanwhile, saves};
        let refreshing =
            |m: &harness::Rig| lock_dir(&provider::claude::paths::refresh_lock(&m.ctx));
        for point in ["switch.journal_written", "switch.park_recorded"] {
            let m = harness::machine(&format!("refresh-lock-{point}"));
            let keychain = std::sync::Arc::clone(m.mem.live());
            let session = Session::new();
            let settled = settle(&m.ctx, Permit::for_a_test(), None)
                .expect("nothing to recover")
                .0;

            let done = fault::meanwhile(
                point,
                renews_meanwhile(&m, std::sync::Arc::clone(&keychain), &session),
                || switch(settled, &m.key("there")),
            );
            saves(&m, &keychain, &session);

            assert!(
                session.waited.get(),
                "{point}: a session spent the refresh token Pitboard parked"
            );
            done.expect("switched");
            let state = state::load(&m.ctx).expect("state");
            let parked = state
                .get(&m.key("here"))
                .and_then(|account| account.parked.clone())
                .expect("the outgoing login is parked");
            assert_eq!(parked.login, store::fingerprint("here-refresh"), "{point}");
            assert!(
                !refreshing(&m).exists(),
                "{point}: the refresh lock is let go of"
            );
        }
    }

    #[test]
    fn a_successful_write_needs_no_rollback() {
        let written = RefCell::new(Vec::new());
        let result = install_with(
            &Context::for_unit_test(),
            ProviderId::Claude,
            |b| {
                written.borrow_mut().push(b.to_string());
                Ok(())
            },
            || unreachable!(),
            "new",
            "old",
            "a",
            "b",
        );
        assert!(result.is_ok());
        assert_eq!(*written.borrow(), vec!["new"]);
    }

    #[test]
    fn a_failed_write_that_changed_nothing_is_not_reported_as_a_lost_login() {
        let result = install_with(
            &Context::for_unit_test(),
            ProviderId::Claude,
            |_| Err(failing("keychain locked")),
            || Ok(Some("old".into())),
            "new",
            "old",
            "a",
            "b",
        );
        assert!(
            matches!(result, Err(Error::SwitchRolledBack { .. })),
            "the old login never left, so the user must not be told to sign in again"
        );
    }

    #[test]
    fn a_half_write_is_rolled_back() {
        let slot = RefCell::new("old".to_string());
        let result = install_with(
            &Context::for_unit_test(),
            ProviderId::Claude,
            |b| {
                if b == "new" {
                    *slot.borrow_mut() = "garbled".into();
                    Err(failing("interrupted"))
                } else {
                    *slot.borrow_mut() = b.to_string();
                    Ok(())
                }
            },
            || Ok(Some(slot.borrow().clone())),
            "new",
            "old",
            "a",
            "b",
        );
        assert!(matches!(result, Err(Error::SwitchRolledBack { .. })));
        assert_eq!(
            *slot.borrow(),
            "old",
            "the previous login must be back in place"
        );
    }

    /// The likeliest failure of all, and the one that used to produce the most alarming
    /// message Pitboard has. A locked keychain fails the write, fails the read-back, and
    /// would have failed the rollback too; "could not read" was taken to mean "the slot
    /// changed", so the person was told their login could not be put back. Nothing had been
    /// written and it had never moved.
    #[test]
    fn a_store_that_cannot_be_read_back_is_not_a_lost_login() {
        let writes = RefCell::new(0);
        let result = install_with(
            &Context::for_unit_test(),
            ProviderId::Claude,
            |_| {
                *writes.borrow_mut() += 1;
                Err(failing("the keychain is locked"))
            },
            || Err(store::Error::Unreadable("the keychain is locked".into())),
            "new",
            "old",
            "a",
            "b",
        );
        assert!(
            matches!(result, Err(Error::SwitchUnverified { .. })),
            "not knowing is its own answer, and must not read as a lost login"
        );
        assert_eq!(
            *writes.borrow(),
            1,
            "and nothing further is written into a store that cannot be read"
        );
    }

    // Credential Manager's pieces, a write into which stopped partway: nobody reads them until a
    // whole login is written over them, which the write of the one that was there is.
    #[test]
    fn a_write_left_unfinished_is_rolled_back_by_writing_the_login_that_was_there() {
        let slot = RefCell::new(Err("left unfinished".to_string()));
        let result = install_with(
            &Context::for_unit_test(),
            ProviderId::Claude,
            |b| {
                if b == "new" {
                    return Err(failing("Element not found. (os error 1168)"));
                }
                *slot.borrow_mut() = Ok(b.to_string());
                Ok(())
            },
            || {
                slot.borrow()
                    .clone()
                    .map(Some)
                    .map_err(store::Error::Unfinished)
            },
            "new",
            "old",
            "a",
            "b",
        );
        assert!(
            matches!(result, Err(Error::SwitchRolledBack { .. })),
            "{result:?}"
        );
        assert_eq!(*slot.borrow(), Ok("old".to_string()));
    }

    #[test]
    fn only_a_failed_rollback_after_a_change_is_reported_as_corruption() {
        let result = install_with(
            &Context::for_unit_test(),
            ProviderId::Claude,
            |_| Err(failing("disk full")),
            || Ok(Some("garbled".into())),
            "new",
            "old",
            "a",
            "b",
        );
        assert!(matches!(result, Err(Error::SwitchCorrupted { .. })));
    }

    #[test]
    fn a_copy_is_kept_after_a_failed_install_only_when_it_is_all_that_is_left() {
        let (from, to, detail) = ("a".to_string(), "b".to_string(), String::new());
        assert!(!only_copy_left(&Error::SwitchRolledBack {
            from: from.clone(),
            to: to.clone(),
            detail: detail.clone(),
        }));
        assert!(only_copy_left(&Error::SwitchCorrupted {
            tool: ProviderId::Claude,
            from,
            to,
            detail,
            spelling: crate::words::Spelling::Bare,
        }));
    }
}
