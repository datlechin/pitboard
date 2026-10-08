//! Moving the signed-in identity from one enrolled account to another.
//!
//! Two rules set the order of every step. Which account the outgoing login belongs to is
//! asked of Anthropic, never read from Claude Code's config, which can lag the login by a
//! day: a login filed under the wrong account takes both accounts with it. And additive
//! writes become durable before destructive ones, so a run that dies midway leaves a spare
//! copy, never a missing one.

use crate::provider;
use crate::provider::ProviderId;
mod adopt;
mod auto;
#[cfg(test)]
mod crash;
mod enroll;
#[cfg(test)]
mod foreign;
mod forget;
#[cfg(test)]
pub(crate) mod harness;
mod journal;
#[cfg(test)]
mod refusals;
mod rename;
pub(crate) mod renew;
#[cfg(test)]
mod two_tools;
mod uninstall;

pub use crate::pending::Reclaimed;
pub use adopt::{Adopted, adopt};
pub(crate) use auto::automatically;
#[cfg(feature = "test-support")]
pub(crate) use enroll::planted;
pub(crate) use enroll::sign_in_watched_as;
pub use enroll::{Enrolled, Said, SignIn, WatchedSignIn, enroll, sign_in, sign_in_watched};
#[cfg(any(test, feature = "test-support"))]
pub use enroll::{ScriptedSignIn, SignInScript};
pub use forget::forget;
pub use journal::{Abandoned, Recovered, pending as interrupted};
pub(crate) use journal::{Asking, interrupted_tool};
pub use rename::rename;
pub use renew::{Due, Renewal, renew_due, renew_parked};
pub use uninstall::{Removed, uninstall};

use crate::context::Context;
use crate::error::{Error, Result};
use crate::in_use::{self, InUse};
use crate::service::{Permit, Warning};
use crate::state::{Account, Key, Park, State};
use crate::{api, fault, holder, home, lock, park, pending, state, store};
use journal::{Journal, clear_journal, reconcile, write_journal};
use serde_json::Value;
use std::path::PathBuf;

/// Claude Code serves the credential from a 30 second cache, so a session already running
/// picks up a swap within about 33 seconds, wherever no `.credentials.json` sits behind the
/// keychain (the register's `credential_cache`). Measured over three runs on one machine
/// with 2.1.278: swapping at t+8, t+20 and t+28 seconds took effect at t+32.3, t+33.5 and
/// t+32.95 from process start.
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
    /// Not a failure: the state the caller asked for already holds.
    AlreadyActive { label: String },
}

/// Pitboard's state, held exclusively, with any interrupted switch already finished. Every
/// command that changes state starts from one, so none acts on what a crash left behind,
/// and it carries the [`Permit`] it was settled with, which is what lets the change write.
pub struct Settled {
    _exclusive: std::fs::File,
    state: State,
    ctx: Context,
    permit: Permit,
}

/// Throws away a record of an interrupted switch that cannot be finished, keeping every
/// copy it names. Takes Pitboard's own lock but never Claude Code's: it installs nothing.
///
/// Under a custom Claude Code endpoint it gives up on another tool's switch, as a change to
/// that tool settles one there, and refuses a Claude Code switch, which is settled nowhere.
/// It refused every switch there, so a Codex switch that read as stuck could be neither
/// finished nor given up on.
pub fn abandon(ctx: &Context, permit: Permit) -> Result<Option<Abandoned>> {
    refuse_custom_oauth(ctx, journal::interrupted_tool(ctx))?;
    let _exclusive = exclusive(ctx, permit)?;
    let mut state = state::load(ctx)?;
    journal::abandon(ctx, permit, &mut state)
}

/// Under a custom OAuth endpoint Claude Code's live login is in "Claude Code-custom-oauth-
/// credentials", not the item Pitboard reads. Acting on it would park nothing and restore
/// into an item nobody reads, so Pitboard does not act on Claude Code at all: not a change
/// to one of its accounts, not a change that could touch every tool's, and not the
/// recovery of an interrupted Claude Code switch. A change to another tool's account goes
/// ahead; its login is somewhere this setting does not move.
fn refuse_custom_oauth(ctx: &Context, tool: Option<ProviderId>) -> Result<()> {
    if !crate::settings::custom_oauth(ctx) {
        return Ok(());
    }
    let claude = Some(ProviderId::Claude);
    if tool.is_none() || tool == claude || journal::interrupted_tool(ctx) == claude {
        return Err(Error::CustomOauthEndpoint);
    }
    Ok(())
}

/// What recovery found is returned apart from the `Settled`, so it can be reported whether
/// or not the command that follows succeeds.
///
/// `tool` is the tool the change that follows is about, where it is about one.
pub fn settle(
    ctx: &Context,
    permit: Permit,
    tool: Option<ProviderId>,
) -> Result<(Settled, Option<Recovered>)> {
    refuse_custom_oauth(ctx, tool)?;
    let exclusive = exclusive(ctx, permit)?;
    let mut state = state::load(ctx)?;
    let recovered = reconcile(ctx, permit, &mut state)?;
    // After the journal has had its say, so a switch's own park is already accounted for.
    pending::sweep(ctx, permit, &mut state)?;
    drop_live_twins(ctx, permit, &mut state)?;
    purge(ctx, permit, &mut state);
    Ok((
        Settled {
            _exclusive: exclusive,
            state,
            ctx: ctx.clone(),
            permit,
        },
        recovered,
    ))
}

/// Why the next change would stop at an interrupted switch it cannot finish: the refusal
/// that change would make, in its words. `None` where no switch is waiting, where the next
/// change finishes or undoes it by itself, and where telling which would need the service
/// and `asking` says not to ask it.
///
/// Worked out the way [`settle`] works it out, from the same reads and the same decision,
/// and only read: no lock is taken, nothing is written, and the record stays for whatever
/// finishes it or gives up on it. A switch under way in another run has a record too, and
/// what it leaves at every step is what the next change could settle, ordinarily from the
/// fingerprints alone, so it does not read as stuck.
pub(crate) fn stuck(ctx: &Context, state: &State, asking: Asking) -> Option<Error> {
    if !journal::pending(ctx) {
        return None;
    }
    // Under a custom Claude Code endpoint no change comes to a Claude Code switch: each is
    // refused over the endpoint first, as settling it and giving up on it are. A Codex
    // switch is settled there, and given up on, as anywhere.
    refuse_custom_oauth(ctx, journal::interrupted_tool(ctx)).ok()?;
    journal::refusal(ctx, state, asking)
}

/// Ask the store itself what parked logins are on this machine, and resolve every one the
/// state does not name. `settle` already does this from Pitboard's own list of names on
/// every change; this is the thorough version, for a machine whose list was lost with its
/// state file, or written by a version that kept no list.
pub fn repair(settled: Settled) -> Result<pending::Reclaimed> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
        permit,
    } = settled;
    let reclaimed = pending::reclaim(&ctx, permit, &mut state)?;
    purge(&ctx, permit, &mut state);
    Ok(reclaimed)
}

/// Drop every park that is a copy of the login its tool has in use now, and say so in the
/// state before anything deletes it. The journal records the one copy a switch makes on
/// purpose; this finds the one nothing records, which a new login leaves when it was put in
/// use and could not be read back, and was parked as well.
fn drop_live_twins(ctx: &Context, permit: Permit, state: &mut State) -> Result<()> {
    let twins = park::live_twins(ctx, state);
    if twins.is_empty() {
        return Ok(());
    }
    for service in &twins {
        state.discard(service);
    }
    state::save(ctx, permit, state)
}

/// Delete what no account refers to any more. A failed save only leaves deleted names
/// listed, and deleting a missing item succeeds, so a later run clears them.
fn purge(ctx: &Context, permit: Permit, state: &mut State) -> usize {
    let listed = state.discarded.len();
    let remaining = park::purge(ctx, permit, state);
    if remaining != listed {
        let _ = state::save(ctx, permit, state);
    }
    remaining
}

/// Makes Pitboard runs exclusive of each other. A kernel lock, unlike the directory lock
/// Claude Code's protocol requires around its own writes: the operating system releases it
/// when a process ends, so there is no staleness rule for two runs to both satisfy.
fn exclusive(ctx: &Context, permit: Permit) -> Result<std::fs::File> {
    let (file, path) = lock_file(ctx, permit)?;
    file.lock()
        .map_err(|source| Error::HomeUnwritable { path, source })?;
    Ok(file)
}

/// `exclusive` without waiting: `None` while another Pitboard run holds it.
fn try_exclusive(ctx: &Context, permit: Permit) -> Option<std::fs::File> {
    let (file, _) = lock_file(ctx, permit).ok()?;
    file.try_lock().ok()?;
    Some(file)
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

/// Whose login this document holds. When this cannot be answered, nothing moves: a login
/// filed under a guessed account takes two accounts with it.
///
/// Through the provider, because the answer costs a network round trip for Claude Code and
/// nothing at all for Codex, whose login carries a signed token naming the account. The
/// caller wants the answer and should not have to know which.
pub(super) fn identify_document(
    ctx: &Context,
    which: ProviderId,
    document: &Value,
) -> Result<api::Owner> {
    let credential = provider::Credential::new(which, document.clone());
    provider::of(which)
        .identify(ctx, &credential)
        .map(api::Owner::from)
        .map_err(|e| match e {
            provider::ProviderError::Unauthorized => Error::SessionExpired { tool: which },
            other @ (provider::ProviderError::ShapeUnexpected { .. }
            | provider::ProviderError::Unsupported { .. }) => shape(which, other),
            other => Error::IdentityUnverifiable {
                tool: which,
                cause: crate::error::Cause::of_provider(&other),
                detail: other.to_string(),
            },
        })
}

/// Nothing is signed in to this tool, said the way the tool's own files explain it.
///
/// Nothing in any store Pitboard reads, and the tool's own record naming somebody as signed
/// in, are two different situations. The second means Pitboard is looking in the wrong
/// place, and writing a login there would put it where nobody reads.
pub(super) fn nothing_signed_in(ctx: &Context, which: ProviderId) -> Error {
    match provider::of(which).recorded_identity(ctx) {
        Some(found) => Error::LiveCredentialElsewhere { email: found.email },
        None => Error::LiveCredentialAbsent { tool: which },
    }
}

/// Where this tool's live login is, or why Pitboard cannot act on it here.
pub(super) fn live_store(ctx: &Context, which: ProviderId) -> Result<provider::LiveStore> {
    provider::of(which).live(ctx).map_err(|e| shape(which, e))
}

/// The live login as it is stored, byte for byte, and as a document.
///
/// The bytes are kept because a failed write is judged by whether they changed, and a
/// document read back through a parser would compare equal to one whose bytes had moved.
fn read_live(
    ctx: &Context,
    which: ProviderId,
    live: &provider::LiveStore,
) -> Result<(String, Value)> {
    let raw = store::read_raw(&live.chain, &live.service)?
        .ok_or_else(|| nothing_signed_in(ctx, which))?;
    let document = serde_json::from_str(&raw)
        .map_err(|e| Error::Store(store::Error::Malformed(e.to_string())))?;
    match provider::of(which).slice(&document) {
        Err(provider::ProviderError::NoLogin { .. }) => Err(nothing_signed_in(ctx, which)),
        Err(other) => Err(shape(which, other)),
        Ok(_) => Ok((raw, document)),
    }
}

pub fn switch(settled: Settled, key: &Key) -> Result<(Outcome, Vec<Warning>)> {
    let Settled {
        _exclusive,
        state,
        ctx,
        permit,
    } = settled;
    switch_held(state, &ctx, permit, key, None)
}

/// [`switch`], for a caller that holds Pitboard's lock itself. Where `expected` names an
/// account by its id, the switch is made only away from that account: one decided from what
/// was known before the lock was held is refused, with nothing changed, once somebody else
/// has switched since.
fn switch_held(
    mut state: State,
    ctx: &Context,
    permit: Permit,
    key: &Key,
    expected: Option<&str>,
) -> Result<(Outcome, Vec<Warning>)> {
    let label = &key.label;
    let tool = provider::of(key.provider);
    let target = state
        .get(key)
        .cloned()
        .ok_or_else(|| Error::AccountUnknown {
            label: key.typed(),
            enrolled: state.labels(key.provider),
        })?;
    let live = live_store(ctx, key.provider)?;

    // Asked before taking the tool's own lock so a round trip does not hold up its writes,
    // then confirmed under the lock.
    let named = in_use::named(ctx, key.provider);
    let (_, first) = read_live(ctx, key.provider, &live)?;
    let outgoing = identify_document(ctx, key.provider, &first)?;
    // Whose login the store holds, as its service just said. Kept with whatever is saved
    // next, so the record names the account a switch moves out of when it records the one
    // it moves to, whatever it named before.
    let found = InUse {
        owner: Some(outgoing.clone()),
        login: tool.fingerprint(&first),
        known_at: ctx.now(),
        named,
    };
    let identified = state.identified(key.provider, found, ctx.now());

    if target.owned_by(&outgoing) {
        if identified.changed {
            state::save(ctx, permit, &state)?;
        }
        return Ok((
            Outcome::AlreadyActive {
                label: state.typed(key),
            },
            Vec::new(),
        ));
    }
    if expected.is_some_and(|expected| expected != state.id_of(key.provider, &outgoing)) {
        return Err(Error::SwitchOvertaken);
    }
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
        return Err(Error::ParkedLoginExpired { label: to.clone() });
    }
    let incoming = park::load(ctx, key, &held)?;
    // Asked before the tool's lock is taken, like the outgoing question, so the round trip
    // does not hold up its writes.
    let (held, incoming) = prove_incoming(ctx, permit, &mut state, key, &target, held, incoming)?;

    let guard = write_lock(ctx, permit, key.provider)?;
    let Readied {
        before_raw,
        before,
        next,
        on_the_command_line,
    } = ready(ctx, key.provider, &live, &first, &outgoing, &incoming, &to)?;

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
            to_fingerprint: held.refresh_fingerprint.clone(),
            slot: Some(tool.slot(ctx)),
        },
    )?;
    fault::point("switch.journal_written");

    // Until the incoming login is installed there is nothing for a later run to finish, so
    // a failure here takes the record of intent away with it. A copy that was written but
    // could not be recorded is deleted: nothing that survives would name it.
    let slice = tool.slice(&before).map_err(|e| shape(key.provider, e))?;
    let parked = match park::store_at(ctx, permit, key.provider, &park_service, &slice) {
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
            .and_then(|document| identify_document(ctx, key.provider, &document).ok())
            .is_some_and(|found| found.same_login(&outgoing));
        if still_outgoing {
            state.discard(&parked.service);
        }
        state::save(ctx, permit, &state)?;
        clear_journal(ctx, permit);
        purge(ctx, permit, &mut state);
        return Err(Error::SignedInAccountChanged);
    }

    if let Err(e) = install_with(
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
            state.discard(&parked.service);
        }
        state::save(ctx, permit, &state)?;
        clear_journal(ctx, permit);
        purge(ctx, permit, &mut state);
        return Err(e);
    }
    fault::point("switch.installed");

    // The write landed. That is not the same as it having held. Measured in Claude Code
    // 2.1.278, a `/logout` that has given up waiting deletes the credential with no lock
    // held at all, which is the one write its lock does not exclude; and a lock that aged
    // out while the machine slept lets the tool reclaim it and write underneath. Both cost
    // one read to notice here, and cost a browser sign-in to discover later.
    //
    // Checked before the incoming copy is discarded, so finding it did not hold leaves both
    // logins parked rather than neither.
    let lock_lost = guard.as_ref().is_some_and(lock::Guard::compromised);
    match holds(key.provider, &live) {
        Ok(true) => {}
        Ok(false) => {
            clear_journal(ctx, permit);
            return Err(Error::SwitchDidNotHold {
                tool: key.provider,
                from,
                to,
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

    state.discard(&held.service);
    let installed = InUse {
        owner: Some(target.owner()),
        login: tool.fingerprint(&incoming),
        known_at: ctx.now(),
        named: in_use::naming(key.provider, &target.owner()),
    };
    state.identified(key.provider, installed, ctx.now());
    state::save(ctx, permit, &state)?;
    fault::point("switch.recorded");
    drop(guard);

    // The tool does not correct what it caches about who is signed in on its own; the next
    // switch rewrites it.
    let outgoing_identity = provider::Identity {
        account_id: outgoing.account_uuid.clone(),
        email: outgoing.email.clone(),
        group: Some(outgoing.organization_uuid.clone()).filter(|g| !g.is_empty()),
    };
    let cache_warning = tool
        .after_switch(ctx, permit, &target, &outgoing_identity)
        .err()
        .map(Warning::ConfigNotUpdated);
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
        Outcome::Switched {
            provider: key.provider,
            adoption: tool.adoption(tool.behind(ctx).as_ref()),
            from,
            to,
            parked,
        },
        warnings,
    ))
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

/// The lock `which`'s tool takes around its own writes to its live login, taken the way it
/// takes it, where it takes one. The caller holds it from before it readies a write until it
/// has recorded what it wrote.
fn write_lock(ctx: &Context, permit: Permit, which: ProviderId) -> Result<Option<lock::Guard>> {
    Ok(provider::of(which)
        .write_lock(ctx)
        .map(|dir| lock::acquire(permit, &dir))
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
    which: ProviderId,
    live: &provider::LiveStore,
    first: &Value,
    signed_in: &api::Owner,
    incoming: &Value,
    label: &str,
) -> Result<Readied> {
    let tool = provider::of(which);
    let (before_raw, before) = read_live(ctx, which, live)?;
    // A refresh keeps the account, so an unchanged share of the document needs no second
    // question. A sign-in between the two reads would not keep it.
    if tool.slice(&before).ok() != tool.slice(first).ok()
        && !identify_document(ctx, which, &before)?.same_login(signed_in)
    {
        return Err(Error::SignedInAccountChanged);
    }

    // Checked before anything is parked or written, so a change that could never be written
    // changes nothing.
    let next = to_body(
        tool.splice(&before, incoming)
            .map_err(|e| shape(which, e))?,
    );
    // Asked once. The answer is about the backend that would take this write, so a login
    // living in a fallback file is not told it has the keychain's ceiling.
    let price = store::cost(&live.chain, &live.service, &next);
    if price.is_some_and(store::Cost::refused) {
        let price = price.expect("refused implies a ceiling");
        return Err(Error::CredentialTooLarge {
            tool: which,
            label: label.to_string(),
            bytes: price.needs,
            limit: price.limit,
        });
    }
    // Said once per write rather than hidden: the same bytes are visible to `ps` for the
    // length of one `security` call, which is the only way to write a login this size.
    let on_the_command_line =
        price
            .filter(|p| p.on_the_second_route())
            .map(|p| Warning::WrittenOnTheCommandLine {
                tool: which,
                bytes: p.needs,
                limit: p.limit,
            });
    Ok(Readied {
        before_raw,
        before,
        next,
        on_the_command_line,
    })
}

/// Whether a login of the tool's shape is in its live slot, read back after a write.
///
/// The tool may have rotated the token it was just given, which keeps the account and
/// changes the bytes: a login of its shape being there at all is the fact.
fn holds(which: ProviderId, live: &provider::LiveStore) -> std::result::Result<bool, store::Error> {
    let tool = provider::of(which);
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
    match provider::of(which).adoption(None) {
        provider::Adoption::RestartRequired { program, holders } => {
            match holder::find(ctx, program, holders) {
                None => StillHolding::Unknown,
                Some(holding) if holding.is_empty() => StillHolding::Nothing,
                Some(holding) => StillHolding::These(holding),
            }
        }
        provider::Adoption::PollingWithin(_) | provider::Adoption::AtRenewal { .. } => {
            StillHolding::Nothing
        }
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
fn prove_incoming(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    key: &Key,
    target: &Account,
    held: Park,
    incoming: Value,
) -> Result<(Park, Value)> {
    let label = state.typed(key);
    if held.askable_at(ctx.now()) {
        let credential = provider::Credential::new(key.provider, incoming.clone());
        match provider::of(key.provider)
            .verify(ctx, &credential)
            .map(api::Owner::from)
        {
            Ok(found) if target.owned_by(&found) => return Ok((held, incoming)),
            Ok(other) => {
                return Err(Error::ParkedLoginBelongsElsewhere {
                    label,
                    who: crate::words::login(
                        key.provider,
                        &other.email,
                        other.email == target.email,
                    ),
                });
            }
            // The access token has lapsed earlier than the recorded expiry said it would.
            // Renewing settles it either way.
            Err(provider::ProviderError::Unauthorized) => {}
            Err(provider::ProviderError::ShapeUnexpected { detail, .. }) => {
                return Err(Error::ParkedCredentialCorrupt { label, detail });
            }
            Err(e) => {
                return Err(Error::IdentityUnverifiable {
                    tool: key.provider,
                    cause: crate::error::Cause::of_provider(&e),
                    detail: e.to_string(),
                });
            }
        }
    }

    // Lapsed, or refused as lapsed. Renew it and switch to what comes back.
    let Some(fresh) = renew::renew_one(ctx, permit, state, key, &held)? else {
        return Err(Error::IdentityUnverifiable {
            tool: key.provider,
            cause: crate::error::Cause::Unreachable,
            detail: format!(
                "`{label}`'s parked login needs renewing and {} did not answer",
                key.provider.service()
            ),
        });
    };
    let document = park::load(ctx, key, &fresh)?;
    Ok((fresh, document))
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
        other => Error::LiveCredentialShapeUnexpected {
            tool,
            detail: other.to_string(),
        },
    }
}

/// A spliced document as bytes to write.
fn to_body(document: Value) -> String {
    serde_json::to_string(&document).expect("a credential document stays serialisable")
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
fn install_with(
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
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::RefCell;

    fn failing(message: &str) -> store::Error {
        store::Error::Write(message.into())
    }

    /// After a sign-in outside Pitboard the record can still name the account a switch goes
    /// to. The switch moves the store from the login it found there, so the account it goes
    /// to comes to be in use then, and the login it found is the one parked.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W23: Claude Code's Credential Manager store, which a machine in memory plays"
    )]
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
                provider::of(ProviderId::Claude)
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

    #[test]
    fn a_successful_write_needs_no_rollback() {
        let written = RefCell::new(Vec::new());
        let result = install_with(
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

    #[test]
    fn only_a_failed_rollback_after_a_change_is_reported_as_corruption() {
        let result = install_with(
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
            detail
        }));
    }
}
