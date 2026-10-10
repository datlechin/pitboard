//! Renewing a login of Claude Code's where Claude Code keeps it, as Claude Code renews one
//! (the register's `refresh_lock`): under its refresh lock, the login read again, its refresh
//! token sent, and the answer saved through its write lock where the store still holds the
//! token sent. `pitboard stow` renews the login left in the file behind the keychain this
//! way, and a change or a read renews the login stored this way where it must tell whose
//! that is and its access token has lapsed.
//!
//! The exchange spends the refresh token it sends. So what could refuse the save is asked
//! before the token is sent, and from Anthropic's answer until it is saved, a copy of the
//! renewed login waits in Pitboard's vault, written down first in `state.json`'s `renewing`.
//! A run that stops in between, or whose save fails, leaves the copy for the next to save
//! ([`finish`]), and nothing is sent for a slot while a copy waits for it.

use super::{shape, to_body, write_lock};
use crate::context::Context;
use crate::error::Error;
use crate::provider::claude::live;
use crate::provider::{self, ProviderError, ProviderId};
use crate::service::Permit;
use crate::state::{self, Renewing, State};
use crate::store::{self, RawStore};
use crate::{fault, lock, park};
use serde_json::Value;

/// How often Claude Code's write lock is asked for to save a renewed login, each for as long
/// as a change waits: 30 seconds in all, twice the lock's staleness.
const SAVE_ROUNDS: u32 = 4;

/// What the vault copy of a renewed login is named after, in place of an account's id.
const COPY: &str = "renewing";

/// The locks `which`'s tool takes before it renews its login, held: its own, then the one
/// beside it, taken as a renewal takes them (Claude Code's are the register's `refresh_lock`),
/// so no session renews the login meanwhile. The second is gone without where it cannot be
/// made, as the tool goes without it, and let go of first, as the tool lets go of it.
pub(super) struct Refreshing {
    legacy: Option<lock::Guard>,
    own: lock::Guard,
}

impl Refreshing {
    /// `None` for a tool that takes no such lock. Taken before the tool's write lock, as the
    /// tool takes them.
    pub(super) fn take(
        ctx: &Context,
        permit: Permit,
        which: ProviderId,
    ) -> crate::error::Result<Option<Refreshing>> {
        let Some((own, legacy)) = provider::of(which).refresh_lock(ctx) else {
            return Ok(None);
        };
        let own = lock::acquire(permit, &own, lock::REFRESH)?;
        let legacy = match lock::acquire(permit, &legacy, lock::REFRESH) {
            Ok(legacy) => Some(legacy),
            Err(lock::LockError::Io(_)) => None,
            Err(busy) => return Err(busy.into()),
        };
        Ok(Some(Refreshing { legacy, own }))
    }

    /// Lets go of it, saying whether it stopped being Pitboard's meanwhile.
    pub(super) fn let_go(held: Option<Refreshing>) -> bool {
        held.is_some_and(|held| {
            held.own.compromised() || held.legacy.as_ref().is_some_and(lock::Guard::compromised)
        })
    }
}

/// Where a renewed login is kept: the store itself, read and written directly rather than
/// through the stores in front of it, and the login's name there.
#[derive(Clone, Copy)]
pub(super) struct Place<'a> {
    pub(super) store: &'a dyn RawStore,
    pub(super) service: &'a str,
}

/// How far a renewal went, for what stops it to say.
#[derive(Debug, Default)]
pub(super) struct Progress {
    /// Anthropic answered, which spent the refresh token sent.
    pub(super) spent: bool,
    /// The answer was saved where the login is kept.
    pub(super) saved: bool,
    /// A lock of Claude Code's stopped being Pitboard's while it held it, or the answer was
    /// saved without the write lock.
    pub(super) lock_lost: bool,
    /// The answer could not be saved, and waits in the vault for [`finish`].
    pub(super) kept: bool,
}

pub(super) enum Renewed {
    /// What the place holds now, byte for byte, with the renewed login in it.
    Saved(String),
    /// Anthropic refuses the refresh token for good. Nothing was written.
    Refused,
}

pub(super) enum Stop {
    /// The place no longer holds the login renewed: found before the refresh token was sent,
    /// which changes nothing, or after, when the renewed login is saved over no other login,
    /// as Claude Code saves none.
    Changed,
    /// Anthropic could not renew it this time.
    Unrenewed(ProviderError),
    Failed(Error),
}

impl From<Error> for Stop {
    fn from(error: Error) -> Stop {
        Stop::Failed(error)
    }
}

impl From<store::Error> for Stop {
    fn from(error: store::Error) -> Stop {
        Stop::Failed(error.into())
    }
}

/// Renews `login`, the login `place` holds as `seen` byte for byte, with Claude Code's
/// refresh lock held by the caller, and saves the answer there. A store that could not take
/// the answer, and Claude Code's write lock, are asked before the refresh token is sent:
/// the lock is taken once and `place` read again under it, so a writer in the way stops it
/// with nothing sent. Once Anthropic has answered, the answer is copied
/// into the vault, the write lock asked for again for 30 seconds and the answer then saved
/// without it, as Claude Code writes on once its own lock is lost. The copy is let go of
/// once the answer is saved or `place` holds another login, and kept for [`finish`] where
/// the save failed. `progress` says how far it went, whatever it returns.
#[allow(clippy::too_many_arguments)]
pub(super) fn renew(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    which: ProviderId,
    place: Place<'_>,
    seen: &str,
    login: &Value,
    progress: &mut Progress,
) -> Result<Renewed, Stop> {
    let tool = provider::of(which);
    // A renewed login is this one with new tokens of the same length.
    if let Some(price) = place
        .store
        .cost(place.service, seen)
        .filter(|p| p.refused())
    {
        return Err(Stop::Failed(Error::CredentialTooLarge {
            tool: which,
            label: which.name().to_owned(),
            bytes: price.needs,
            limit: price.limit,
        }));
    }
    let writing = write_lock(ctx, permit, which)?;
    if place.store.read(place.service)?.as_deref() != Some(seen) {
        return Err(Stop::Changed);
    }
    drop(writing);
    let fresh = match tool.renew(
        ctx,
        permit,
        &provider::Credential::new(which, login.clone()),
    ) {
        Ok(fresh) => fresh.raw,
        Err(ProviderError::InvalidGrant { .. }) => return Ok(Renewed::Refused),
        Err(error) => return Err(Stop::Unrenewed(error)),
    };
    progress.spent = true;
    let sent = tool.fingerprint(login);
    let copy = copy(ctx, permit, state, which, place, &sent, &fresh);
    fault::point("refresh.exchanged");
    let writing = write_lock_to_save(ctx, permit, which, progress);
    let saved = save(permit, which, place, &sent, &fresh);
    progress.lock_lost |= writing.as_ref().is_some_and(lock::Guard::compromised);
    drop(writing);
    if saved.is_ok() {
        progress.saved = true;
        fault::point("refresh.saved");
    }
    match (&saved, copy) {
        (Ok(_) | Err(Stop::Changed), Some(copy)) => let_go(ctx, permit, state, &copy),
        (Err(_), Some(_)) => progress.kept = true,
        (_, None) => {}
    }
    saved.map(Renewed::Saved)
}

/// Saves each renewed login a run that stopped left in the vault for the slot its tool keeps
/// its login in now, where the store there still holds the login renewed, and lets go of the
/// copy, as it does where that store holds the renewed login already or another one. Under
/// Pitboard's lock, as every change and a read that renews start, and before either takes a
/// lock of Claude Code's. A copy left for another slot waits for a run that reads that slot.
/// One whose save cannot be told, as with a store that cannot be read or a lock held, is
/// kept for the next run, and why is the error: the store still holds the refresh token its
/// renewal spent, so nothing may be sent for that slot meanwhile.
pub(super) fn finish(ctx: &Context, permit: Permit, state: &mut State) -> crate::error::Result<()> {
    let mut finished = Ok(());
    for left in state.renewing.clone() {
        finished = finished.and(save_left(ctx, permit, state, &left));
    }
    finished
}

/// [`finish`] for `left`.
fn save_left(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    left: &Renewing,
) -> crate::error::Result<()> {
    let fresh = store::vault_read(ctx, &left.service)?
        .and_then(|held| serde_json::from_str::<Value>(&held).ok());
    let Some(fresh) = fresh else {
        let_go(ctx, permit, state, &left.service);
        return Ok(());
    };
    if left.tool == ProviderId::Claude && crate::settings::custom_oauth(ctx) {
        return Err(Error::CustomOauthEndpoint);
    }
    let live = super::live_store(ctx, left.tool)?;
    if live.service != left.slot {
        return Ok(());
    }
    let kept = store::of_kind(&live.chain, &left.store).ok_or_else(|| {
        Error::Store(store::Error::Unreadable(format!(
            "{} keeps no login in a {} here",
            left.tool.name(),
            left.store
        )))
    })?;
    let place = Place {
        store: kept,
        service: &live.service,
    };
    let refreshing = Refreshing::take(ctx, permit, left.tool)?;
    let saved = write_lock(ctx, permit, left.tool)
        .map_err(Stop::from)
        .and_then(|_writing| save(permit, left.tool, place, &left.sent, &fresh));
    Refreshing::let_go(refreshing);
    match saved {
        Ok(_) | Err(Stop::Changed) => {
            let_go(ctx, permit, state, &left.service);
            Ok(())
        }
        Err(Stop::Failed(error)) => Err(error),
        Err(Stop::Unrenewed(error)) => Err(super::unidentified(left.tool, error)),
    }
}

/// Claude Code's write lock, to save a renewed login under once the refresh token it was
/// renewed with is spent. Giving up as a change does would drop that login, so it is asked
/// for again until a lock nobody touches has gone stale twice over. `None` where it is still
/// not had, which `progress` says: the login is then saved without it.
fn write_lock_to_save(
    ctx: &Context,
    permit: Permit,
    which: ProviderId,
    progress: &mut Progress,
) -> Option<lock::Guard> {
    for _ in 0..SAVE_ROUNDS {
        match write_lock(ctx, permit, which) {
            Ok(writing) => return writing,
            Err(Error::Lock(lock::LockError::Busy)) => {}
            Err(_) => break,
        }
    }
    progress.lock_lost = true;
    None
}

/// Writes `fresh`, the login renewed from the refresh token `sent` fingerprints, into
/// `place` as Claude Code saves a renewal: over the login it holds now, where that still has
/// that refresh token, keeping whatever else it has come to hold. What it holds then.
fn save(
    permit: Permit,
    which: ProviderId,
    place: Place<'_>,
    sent: &str,
    fresh: &Value,
) -> Result<String, Stop> {
    let tool = provider::of(which);
    let now = place
        .store
        .read(place.service)?
        .map(|now| live::document_in(&now))
        .filter(|now| tool.fingerprint(now) == sent)
        .ok_or(Stop::Changed)?;
    let renewed = to_body(tool.splice(&now, fresh).map_err(|e| shape(which, e))?);
    match place.store.write(permit, place.service, &renewed) {
        // Nothing landed, so once more: nothing else holds the renewed login.
        Err(store::Error::Write(_)) => place.store.write(permit, place.service, &renewed)?,
        written => written?,
    }
    Ok(renewed)
}

/// Copies `fresh` into the vault until it is saved, written down in `state` before the copy
/// is. The copy's name, or `None` where none was made: one that would go on the argument
/// line is not made, and the save is then the only copy, as Claude Code's own renewal has.
fn copy(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    which: ProviderId,
    place: Place<'_>,
    sent: &str,
    fresh: &Value,
) -> Option<String> {
    let body = to_body(fresh.clone());
    let service = park::free_name(ctx, COPY).ok()?;
    if store::vault_cost(ctx, &service, &body).is_some_and(store::Cost::over) {
        return None;
    }
    state.renewing.push(Renewing {
        tool: which,
        slot: place.service.to_owned(),
        store: place.store.kind().name().to_owned(),
        sent: sent.to_owned(),
        service: service.clone(),
    });
    let copied = state::save(ctx, permit, state)
        .and_then(|()| Ok(store::vault_write(ctx, permit, &service, &body)?));
    if copied.is_err() {
        state.renewing.retain(|left| left.service != service);
        let _ = state::save(ctx, permit, state);
        return None;
    }
    Some(service)
}

/// Lets go of the copy [`copy`] made, once the renewed login is saved or has nowhere to go.
fn let_go(ctx: &Context, permit: Permit, state: &mut State, service: &str) {
    state.renewing.retain(|left| left.service != service);
    state.discard(service);
    if state::save(ctx, permit, state).is_ok() {
        super::purge(ctx, permit, state);
    }
}
