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
#[cfg(test)]
mod crash;
mod enroll;
mod forget;
#[cfg(test)]
mod harness;
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
pub use enroll::{Enrolled, SignIn, WatchedSignIn, enroll, sign_in, sign_in_watched};
pub use forget::forget;
pub use journal::{Abandoned, Recovered, pending as interrupted};
pub use rename::rename;
pub use renew::{Due, Renewal, renew_due, renew_parked};
pub use uninstall::{Removed, uninstall};

use crate::context::Context;
use crate::error::{Error, Result};
use crate::service::Warning;
use crate::state::{Account, Key, Park, State};
use crate::{api, fault, home, lock, park, pending, state, store};
use journal::{Journal, clear_journal, reconcile, write_journal};
use serde_json::Value;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

/// Claude Code serves the credential from a 30 second cache whose clock restarts on every
/// read or write, so a session picks up a swap within about 30 seconds of its last read
/// rather than of its start. Measured over three runs on one machine: swapping at t+8, t+20
/// and t+28 seconds took effect at t+32.3, t+33.5 and t+32.95 from process start.
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
        /// answers it. Carried rather than read from a constant, because the honest answer
        /// for two of the three tools is that nothing follows until they are restarted.
        adoption: provider::Adoption,
    },
    /// Not a failure: the state the caller asked for already holds.
    AlreadyActive { label: String },
}

/// pitboard's state, held exclusively, with any interrupted switch already finished. Every
/// command that changes state starts from one, so none acts on what a crash left behind.
pub struct Settled {
    _exclusive: std::fs::File,
    state: State,
    ctx: Context,
}

/// Throws away a record of an interrupted switch that cannot be finished, keeping every
/// copy it names. Takes pitboard's own lock but never Claude Code's: it installs nothing.
pub fn abandon(ctx: &Context) -> Result<Option<Abandoned>> {
    refuse_custom_oauth(ctx, None)?;
    let _exclusive = exclusive(ctx)?;
    let mut state = state::load(ctx)?;
    journal::abandon(ctx, &mut state)
}

/// Under a custom OAuth endpoint Claude Code's live login is in "Claude Code-custom-oauth-
/// credentials", not the item pitboard reads. Acting on it would park nothing and restore
/// into an item nobody reads, so pitboard does not act on Claude Code at all: not a change
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
pub fn settle(ctx: &Context, tool: Option<ProviderId>) -> Result<(Settled, Option<Recovered>)> {
    refuse_custom_oauth(ctx, tool)?;
    let exclusive = exclusive(ctx)?;
    let mut state = state::load(ctx)?;
    let recovered = reconcile(ctx, &mut state)?;
    // After the journal has had its say, so a switch's own park is already accounted for.
    pending::sweep(ctx, &mut state)?;
    purge(ctx, &mut state);
    Ok((
        Settled {
            _exclusive: exclusive,
            state,
            ctx: ctx.clone(),
        },
        recovered,
    ))
}

/// Ask the store itself what parked logins are on this machine, and resolve every one the
/// state does not name. `settle` already does this from pitboard's own list of names on
/// every change; this is the thorough version, for a machine whose list was lost with its
/// state file, or written by a version that kept no list.
pub fn repair(settled: Settled) -> Result<pending::Reclaimed> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
    } = settled;
    let reclaimed = pending::reclaim(&ctx, &mut state)?;
    purge(&ctx, &mut state);
    Ok(reclaimed)
}

/// Delete what no account refers to any more. A failed save only leaves deleted names
/// listed, and deleting a missing item succeeds, so a later run clears them.
fn purge(ctx: &Context, state: &mut State) -> usize {
    let listed = state.discarded.len();
    let remaining = park::purge(ctx, state);
    if remaining != listed {
        let _ = state::save(ctx, state);
    }
    remaining
}

/// Makes pitboard runs exclusive of each other. A kernel lock, unlike the directory lock
/// Claude Code's protocol requires around its own writes: the operating system releases it
/// when a process ends, so there is no staleness rule for two runs to both satisfy.
fn exclusive(ctx: &Context) -> Result<std::fs::File> {
    let (file, path) = lock_file(ctx)?;
    file.lock()
        .map_err(|source| Error::HomeUnwritable { path, source })?;
    Ok(file)
}

/// `exclusive` without waiting: `None` while another pitboard run holds it.
fn try_exclusive(ctx: &Context) -> Option<std::fs::File> {
    let (file, _) = lock_file(ctx).ok()?;
    file.try_lock().ok()?;
    Some(file)
}

fn lock_file(ctx: &Context) -> Result<(std::fs::File, PathBuf)> {
    let path = home::dir(ctx).join("state.lock");
    let fail = |source| Error::HomeUnwritable {
        path: path.clone(),
        source,
    };
    home::ensure(ctx).map_err(fail)?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(&path)
        .map_err(fail)?;
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
        .map(|found| api::Owner {
            account_uuid: found.account_id,
            email: found.email,
            organization_uuid: found.group.unwrap_or_default(),
        })
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
/// Nothing in any store pitboard reads, and the tool's own record naming somebody as signed
/// in, are two different situations. The second means pitboard is looking in the wrong
/// place, and writing a login there would put it where nobody reads.
pub(super) fn nothing_signed_in(ctx: &Context, which: ProviderId) -> Error {
    match provider::of(which).recorded_identity(ctx) {
        Some(found) => Error::LiveCredentialElsewhere { email: found.email },
        None => Error::LiveCredentialAbsent { tool: which },
    }
}

/// Where this tool's live login is, or why pitboard cannot act on it here.
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
        mut state,
        ctx,
    } = settled;
    let ctx = &ctx;
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
    let (_, first) = read_live(ctx, key.provider, &live)?;
    let outgoing = identify_document(ctx, key.provider, &first)?;

    if outgoing.account_uuid == target.account_uuid {
        if state.active_for(key.provider) != Some(label.as_str()) {
            state.set_active(key.provider, Some(label.to_string()));
            state.used(key, ctx.now());
            state::save(ctx, &state)?;
        }
        return Ok((Outcome::AlreadyActive { label: key.typed() }, Vec::new()));
    }
    let outgoing_key = state
        .by_uuid(key.provider, &outgoing.account_uuid)
        .map(Account::key)
        .ok_or_else(|| Error::LiveAccountNotEnrolled {
            email: outgoing.email.clone(),
        })?;
    let (from, to) = (outgoing_key.typed(), key.typed());
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
    let (held, incoming) = prove_incoming(ctx, &mut state, key, &target, held, incoming)?;

    let guard = tool
        .write_lock(ctx)
        .map(|dir| lock::acquire(&dir))
        .transpose()?;
    let (before_raw, before) = read_live(ctx, key.provider, &live)?;
    // A refresh keeps the account, so an unchanged share of the document needs no second
    // question. A sign-in between the two reads would not keep it.
    if tool.slice(&before).ok() != tool.slice(&first).ok()
        && identify_document(ctx, key.provider, &before)?.account_uuid != outgoing.account_uuid
    {
        return Err(Error::SignedInAccountChanged);
    }

    // Checked before anything is parked, so a switch that could never be written changes
    // nothing.
    let next = to_body(
        tool.splice(&before, &incoming)
            .map_err(|e| shape(key.provider, e))?,
    );
    // Asked once. The answer is about the backend that would take this write, so a login
    // living in a fallback file is not told it has the keychain's ceiling.
    let price = store::cost(&live.chain, &live.service, &next);
    if price.is_some_and(store::Cost::refused) {
        let price = price.expect("refused implies a ceiling");
        return Err(Error::CredentialTooLarge {
            tool: key.provider,
            label: to,
            bytes: price.needs,
            limit: price.limit,
        });
    }
    // Said once per switch rather than hidden: the same bytes are visible to `ps` for the
    // length of one `security` call, which is the only way to write a login this size.
    let on_the_command_line =
        price
            .filter(|p| p.on_the_second_route())
            .map(|p| Warning::WrittenOnTheCommandLine {
                tool: key.provider,
                bytes: p.needs,
                limit: p.limit,
            });

    // The outgoing login's park has a ceiling of its own on a machine whose vault is the
    // keychain, and it is asked about now, while refusing still changes nothing. The name
    // is the one `reserve` is about to make, give or take the millisecond, which is all
    // the price depends on.
    let parking = park::price(
        ctx,
        key.provider,
        &from,
        &park::service_name(&outgoing.account_uuid, ctx.now_millis()),
        &tool.slice(&before).map_err(|e| shape(key.provider, e))?,
    )?;
    let park_service = park::reserve(ctx, &outgoing.account_uuid)?;
    write_journal(
        ctx,
        &Journal {
            provider: key.provider,
            started_at: ctx.now(),
            from_label: outgoing_key.label.clone(),
            from_uuid: outgoing.account_uuid.clone(),
            to_label: label.to_string(),
            to_uuid: target.account_uuid.clone(),
            park_service: park_service.clone(),
            incoming_service: held.service.clone(),
            // Which side the live credential came from, answerable without asking anyone.
            from_fingerprint: tool.fingerprint(&before),
            to_fingerprint: held.refresh_fingerprint.clone(),
        },
    )?;
    fault::point("switch.journal_written");

    // Until the incoming login is installed there is nothing for a later run to finish, so
    // a failure here takes the record of intent away with it. A copy that was written but
    // could not be recorded is deleted: nothing that survives would name it.
    let slice = tool.slice(&before).map_err(|e| shape(key.provider, e))?;
    let parked = match park::store_at(ctx, key.provider, &park_service, &slice) {
        Ok(parked) => parked,
        Err(e) => {
            clear_journal(ctx);
            return Err(e);
        }
    };
    fault::point("switch.park_stored");
    state.park(&outgoing_key, parked.clone());
    if let Err(e) = state::save(ctx, &state) {
        let _ = store::vault_delete(ctx, &parked.service);
        clear_journal(ctx);
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
        state.discard(&parked.service);
        state::save(ctx, &state)?;
        clear_journal(ctx);
        return Err(Error::ParkedCredentialMissing { label: from });
    }

    if let Err(e) = install_with(
        key.provider,
        |body| store::write_raw(&live.chain, &live.service, body),
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
        state::save(ctx, &state)?;
        clear_journal(ctx);
        purge(ctx, &mut state);
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
    // logins parked rather than neither. The tool may have rotated the token it was just
    // given, which keeps the account and changes the bytes: a login of its shape being
    // there at all is the fact.
    let lock_lost = guard.as_ref().is_some_and(lock::Guard::compromised);
    match store::read_raw(&live.chain, &live.service) {
        Ok(Some(now))
            if serde_json::from_str::<Value>(&now)
                .is_ok_and(|document| tool.slice(&document).is_ok()) => {}
        Ok(_) => {
            clear_journal(ctx);
            return Err(Error::SwitchDidNotHold {
                tool: key.provider,
                from,
                to,
            });
        }
        Err(unreadable) => {
            return Err(Error::SwitchUnverified {
                from,
                to,
                detail: unreadable.to_string(),
            });
        }
    }

    state.discard(&held.service);
    state.set_active(key.provider, Some(label.to_string()));
    state.used(key, ctx.now());
    state::save(ctx, &state)?;
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
        .after_switch(ctx, &target, &outgoing_identity)
        .err()
        .map(Warning::ConfigNotUpdated);
    fault::point("switch.config_updated");
    let parks_pending = purge(ctx, &mut state);
    clear_journal(ctx);

    // A tool that never follows a switch on its own goes on using the outgoing account in
    // every session already running. Said with a count, because "restart it" means nothing
    // to somebody who does not know one is open, and with the one thing not to do in it.
    let still_running = match tool.adoption() {
        provider::Adoption::RestartRequired { program } => ctx
            .host()
            .running(program)
            .filter(|count| *count > 0)
            .map(|count| Warning::SessionsStillRunning {
                program,
                count,
                from: from.clone(),
            }),
        provider::Adoption::PollingWithin(_) => None,
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
            adoption: tool.adoption(),
            from,
            to,
            parked,
        },
        warnings,
    ))
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
    state: &mut State,
    key: &Key,
    target: &Account,
    held: Park,
    incoming: Value,
) -> Result<(Park, Value)> {
    let label = key.typed();
    if held.askable_at(ctx.now()) {
        let credential = provider::Credential::new(key.provider, incoming.clone());
        match provider::of(key.provider).verify(ctx, &credential) {
            Ok(found) if found.account_id == target.account_uuid => return Ok((held, incoming)),
            Ok(other) => {
                return Err(Error::ParkedLoginBelongsElsewhere {
                    label,
                    email: other.email,
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
    let Some(fresh) = renew::renew_one(ctx, state, key, &held)? else {
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
/// told pitboard could not put their login back and they should sign in again. Nothing had
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
    /// message pitboard has. A locked keychain fails the write, fails the read-back, and
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
