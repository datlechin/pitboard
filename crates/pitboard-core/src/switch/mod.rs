//! Moving the signed-in identity from one enrolled account to another.
//!
//! Two rules set the order of every step. Which account the outgoing login belongs to is
//! asked of Anthropic, never read from Claude Code's config, which can lag the login by a
//! day: a login filed under the wrong account takes both accounts with it. And additive
//! writes become durable before destructive ones, so a run that dies midway leaves a spare
//! copy, never a missing one.

use crate::provider::ProviderId;
use crate::provider::claude::configfile;
use crate::provider::claude::live as claude_live;
use crate::provider::claude::paths as claude;
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
mod renew;
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
use crate::state::{Account, Park, State};
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
        from: String,
        to: String,
        parked: Park,
    },
    /// Not a failure: the state the caller asked for already holds.
    AlreadyActive { label: String },
}

/// The account's whole slice of a credential document: `claudeAiOauth` and whatever else of
/// [`ACCOUNT_SCOPED`] is there.
///
/// This is what gets parked. Parking the OAuth block alone meant a switch away deleted the
/// rest of the account's keys and a switch back could not put them there, so an account
/// came back to Claude Code slightly less than it left. Whether that costs a device
/// re-verification is not something pitboard has measured, and it is not claimed anywhere;
/// what is claimed is that restoring an account restores what was there.
///
/// Measured on one real account: the slice is 524 bytes against 506 for the OAuth block
/// alone, which is nothing against the 4032-byte ceiling. An account holding a device token
/// has not been measured, and the write path handles an oversized login either way.
fn slice_of(document: &Value) -> Result<Value> {
    let object = document
        .as_object()
        .ok_or_else(|| Error::LiveCredentialShapeUnexpected {
            detail: "it is not a JSON object".into(),
        })?;
    let oauth =
        object
            .get("claudeAiOauth")
            .ok_or_else(|| Error::LiveCredentialShapeUnexpected {
                detail: "it has no claudeAiOauth block".into(),
            })?;
    let mut slice = serde_json::Map::new();
    slice.insert("claudeAiOauth".into(), oauth.clone());
    for key in ACCOUNT_SCOPED {
        if let Some(value) = object.get(key) {
            slice.insert(key.into(), value.clone());
        }
    }
    Ok(Value::Object(slice))
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
    if crate::settings::custom_oauth(ctx) {
        return Err(Error::CustomOauthEndpoint);
    }
    let _exclusive = exclusive(ctx)?;
    let mut state = state::load(ctx)?;
    journal::abandon(ctx, &mut state)
}

/// What recovery found is returned apart from the `Settled`, so it can be reported whether
/// or not the command that follows succeeds.
pub fn settle(ctx: &Context) -> Result<(Settled, Option<Recovered>)> {
    // Under a custom OAuth endpoint the live login is in "Claude Code-custom-oauth-
    // credentials", not the item pitboard reads. Acting would park nothing and restore
    // into an item nobody reads, so pitboard does not act at all.
    if crate::settings::custom_oauth(ctx) {
        return Err(Error::CustomOauthEndpoint);
    }
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

/// Who a live access token belongs to. When this cannot be answered, nothing moves: a login
/// filed under a guessed account takes two accounts with it.
fn identify(ctx: &Context, access_token: &str) -> Result<api::Owner> {
    api::owner(ctx, access_token).map_err(|e| match e {
        api::ApiError::Unauthorized => Error::SessionExpired,
        other => Error::IdentityUnverifiable {
            cause: crate::error::Cause::of(&other),
            detail: other.to_string(),
        },
    })
}

fn access_token(document: &Value) -> Result<String> {
    document["claudeAiOauth"]["accessToken"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::LiveCredentialShapeUnexpected {
            detail: "it has no access token".into(),
        })
}

pub fn switch(settled: Settled, label: &str) -> Result<(Outcome, Vec<Warning>)> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
    } = settled;
    let ctx = &ctx;
    let target = state
        .get(label)
        .cloned()
        .ok_or_else(|| Error::AccountUnknown {
            label: label.to_string(),
            enrolled: state.labels(),
        })?;

    // Asked before taking Claude Code's lock so the round trip does not hold up its writes,
    // then confirmed under the lock.
    let service = claude::live_service(ctx);
    let live = store::read(&claude_live::chain(ctx), &service)?
        .ok_or_else(|| claude::nothing_signed_in(ctx))?;
    let identified_with = access_token(&live)?;
    let outgoing = identify(ctx, &identified_with)?;

    if outgoing.account_uuid == target.account_uuid {
        if state.active_for(ProviderId::Claude) != Some(label) {
            state.set_active(ProviderId::Claude, Some(label.to_string()));
            if let Some(account) = state.accounts.iter_mut().find(|a| a.label == label) {
                account.last_used_at = Some(ctx.now());
            }
            state::save(ctx, &state)?;
        }
        return Ok((
            Outcome::AlreadyActive {
                label: label.to_string(),
            },
            Vec::new(),
        ));
    }
    let outgoing_label = state
        .by_uuid(&outgoing.account_uuid)
        .map(|a| a.label.clone())
        .ok_or_else(|| Error::LiveAccountNotEnrolled {
            email: outgoing.email.clone(),
        })?;
    let held = target.parked.clone().ok_or_else(|| Error::NothingParked {
        label: label.to_string(),
    })?;
    if !held.restorable_at(ctx.now()) {
        return Err(Error::ParkedLoginExpired {
            label: label.to_string(),
        });
    }
    let incoming = park::load(ctx, label, &held)?;
    // Asked before Claude Code's lock is taken, like the outgoing question, so the round
    // trip does not hold up its writes.
    let (held, incoming) = prove_incoming(ctx, &mut state, label, &target, held, incoming)?;

    let storage = PathBuf::from(claude::storage_dir(ctx)).join(".storage-write");
    let guard = lock::acquire(&storage)?;

    let before_raw = store::read_raw(&claude_live::chain(ctx), &service)?
        .ok_or_else(|| claude::nothing_signed_in(ctx))?;
    let before: Value =
        serde_json::from_str(&before_raw).map_err(|e| Error::LiveCredentialShapeUnexpected {
            detail: e.to_string(),
        })?;
    // A refresh keeps the account, so an unchanged access token needs no second round trip.
    // A sign-in between the two reads would not keep it.
    let now_token = access_token(&before)?;
    if now_token != identified_with
        && identify(ctx, &now_token)?.account_uuid != outgoing.account_uuid
    {
        return Err(Error::SignedInAccountChanged);
    }

    // Checked before anything is parked, so a switch that could never be written changes
    // nothing.
    let next = splice(&before, &incoming)?;
    // Asked once. The answer is about the backend that would take this write, so a login
    // living in the fallback file is not told it has the keychain's ceiling.
    let price = store::cost(&claude_live::chain(ctx), &service, &next);
    if price.is_some_and(store::Cost::refused) {
        let price = price.expect("refused implies a ceiling");
        return Err(Error::CredentialTooLarge {
            label: label.to_string(),
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
                bytes: p.needs,
                limit: p.limit,
            });

    let park_service = park::reserve(ctx, &outgoing.account_uuid)?;
    write_journal(
        ctx,
        &Journal {
            started_at: ctx.now(),
            from_label: outgoing_label.clone(),
            from_uuid: outgoing.account_uuid.clone(),
            to_label: label.to_string(),
            to_uuid: target.account_uuid.clone(),
            park_service: park_service.clone(),
            incoming_service: held.service.clone(),
            // Which side the live credential came from, answerable without asking anyone.
            from_fingerprint: park::fingerprint_of(&before["claudeAiOauth"]),
            to_fingerprint: held.refresh_fingerprint.clone(),
        },
    )?;
    fault::point("switch.journal_written");

    // Until the incoming login is installed there is nothing for a later run to finish, so
    // a failure here takes the record of intent away with it. A copy that was written but
    // could not be recorded is deleted: nothing that survives would name it.
    let parked = match park::store_at(ctx, &park_service, &slice_of(&before)?) {
        Ok(parked) => parked,
        Err(e) => {
            clear_journal(ctx);
            return Err(e);
        }
    };
    fault::point("switch.park_stored");
    state.park(&outgoing_label, parked.clone());
    if let Err(e) = state::save(ctx, &state) {
        let _ = store::vault_delete(ctx, &parked.service);
        clear_journal(ctx);
        return Err(e);
    }
    fault::point("switch.park_recorded");

    if let Err(e) = install(ctx, &service, &next, &before_raw, &outgoing_label, label) {
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

    // The write landed. That is not the same as it having held. Measured in 2.1.278, a
    // `/logout` that has given up waiting deletes the credential with no lock held at all,
    // which is the one Claude Code write this lock does not exclude; and a lock that aged
    // out while the machine slept lets Claude Code reclaim it and write underneath. Both
    // cost one keychain read to notice here, at 0.016 seconds against the two network round
    // trips this command has already made, and cost a browser sign-in to discover later.
    //
    // Checked before the incoming copy is discarded, so finding it did not hold leaves both
    // logins parked rather than neither.
    let lock_lost = guard.compromised();
    match store::read_raw(&claude_live::chain(ctx), &service) {
        Ok(Some(now)) if now.contains("\"claudeAiOauth\"") => {
            // Claude Code may have rotated the token it was just given, which keeps the
            // account and changes the bytes. A login being there at all is the fact.
            let _ = now;
        }
        Ok(_) => {
            clear_journal(ctx);
            return Err(Error::SwitchDidNotHold {
                from: outgoing_label,
                to: label.to_string(),
            });
        }
        Err(unreadable) => {
            return Err(Error::SwitchUnverified {
                from: outgoing_label,
                to: label.to_string(),
                detail: unreadable.to_string(),
            });
        }
    }

    state.discard(&held.service);
    state.set_active(ProviderId::Claude, Some(label.to_string()));
    if let Some(account) = state.accounts.iter_mut().find(|a| a.label == label) {
        account.last_used_at = Some(ctx.now());
    }
    state::save(ctx, &state)?;
    fault::point("switch.recorded");
    drop(guard);

    // Claude Code does not correct a stale config on its own; the next switch rewrites it.
    let config_warning = update_config(
        ctx,
        &target,
        &outgoing.account_uuid,
        &outgoing.organization_uuid,
    )
    .err()
    .map(Warning::ConfigNotUpdated);
    fault::point("switch.config_updated");
    let parks_pending = purge(ctx, &mut state);
    clear_journal(ctx);

    let warnings = on_the_command_line
        .into_iter()
        .chain(lock_lost.then_some(Warning::LockCompromised))
        .chain(config_warning)
        .chain((parks_pending > 0).then_some(Warning::ParksPendingRemoval(parks_pending)))
        .collect();
    Ok((
        Outcome::Switched {
            from: outgoing_label,
            to: label.to_string(),
            parked,
        },
        warnings,
    ))
}

/// Ask Anthropic about the login going in, not only about the one coming out.
///
/// A switch used to ask twice about the login it was throwing away and never once about
/// the one it was installing. If that account's refresh chain had been revoked, signed out
/// elsewhere or refused by Anthropic, the switch installed it, read it back, found a login
/// there and reported success; the person discovered they were signed out the next time
/// they ran `claude`, with no login to go back to on either side.
///
/// A lapsed park is renewed rather than refused, which is the whole point of parking one.
/// That is a write, so it happens here, before anything is parked, while both copies are
/// still where they were.
fn prove_incoming(
    ctx: &Context,
    state: &mut State,
    label: &str,
    target: &Account,
    held: Park,
    incoming: Value,
) -> Result<(Park, Value)> {
    let usable = held.askable_at(ctx.now());
    if usable {
        let token = park::oauth_in(&incoming)["accessToken"]
            .as_str()
            .ok_or_else(|| Error::ParkedCredentialCorrupt {
                label: label.to_string(),
                detail: "it has no access token".into(),
            })?;
        match api::owner(ctx, token) {
            Ok(owner) if owner.account_uuid == target.account_uuid => return Ok((held, incoming)),
            Ok(other) => {
                return Err(Error::ParkedLoginBelongsElsewhere {
                    label: label.to_string(),
                    email: other.email,
                });
            }
            // The access token has lapsed earlier than the recorded expiry said it
            // would. Renewing settles it either way.
            Err(api::ApiError::Unauthorized) => {}
            Err(e) => {
                return Err(Error::IdentityUnverifiable {
                    cause: crate::error::Cause::of(&e),
                    detail: e.to_string(),
                });
            }
        }
    }

    // Lapsed, or refused as lapsed. Renew it and switch to what comes back.
    let Some(fresh) = renew::renew_one(ctx, state, label, &held)? else {
        return Err(Error::IdentityUnverifiable {
            cause: crate::error::Cause::Unreachable,
            detail: format!("`{label}`'s parked login needs renewing and Anthropic did not answer"),
        });
    };
    let document = park::load(ctx, label, &fresh)?;
    Ok((fresh, document))
}

/// After a failed install, whether the copy just parked is the outgoing account's only login.
/// Once its old login is back in place, the copy is a second holder that Claude Code will
/// rotate past; if it could not be put back, the copy is all that is left of it.
fn only_copy_left(failure: &Error) -> bool {
    matches!(failure, Error::SwitchCorrupted { .. })
}

/// Keys that belong to the account rather than to the machine. Claude Code deletes all of
/// them along with the login on logout, so leaving one behind would hand the incoming
/// account the outgoing account's device token or its second OAuth block. Measured in
/// 2.1.278: `delete i.claudeAiOauth, delete i.organizationUuid, delete i.trustedDeviceToken,
/// delete i.enterpriseGateway, delete i.designOauth`.
const ACCOUNT_SCOPED: [&str; 4] = [
    "organizationUuid",
    "trustedDeviceToken",
    "enterpriseGateway",
    "designOauth",
];

/// The live document with the incoming login in place of the outgoing one, and nothing of
/// the outgoing account left behind. Claude Code makes these keys again as it needs them,
/// which is the state a logout and a fresh login would leave.
fn splice(before: &Value, incoming: &Value) -> Result<String> {
    let mut next = before.clone();
    let document = next
        .as_object_mut()
        .ok_or_else(|| Error::LiveCredentialShapeUnexpected {
            detail: "it is not a JSON object".into(),
        })?;
    document.insert("claudeAiOauth".into(), park::oauth_in(incoming).clone());
    // The outgoing account's keys go, and the incoming account's take their place where the
    // park holds them. A park from a version that kept only the OAuth block holds none, and
    // then this is exactly what it always did.
    for key in ACCOUNT_SCOPED {
        match incoming.get(key) {
            Some(value) => document.insert(key.into(), value.clone()),
            None => document.remove(key),
        };
    }
    Ok(serde_json::to_string(&next).expect("a credential document stays serialisable"))
}

fn install(
    ctx: &Context,
    service: &str,
    next: &str,
    before_raw: &str,
    from: &str,
    to: &str,
) -> Result<()> {
    install_with(
        |body| store::write_raw(&claude_live::chain(ctx), service, body),
        || store::read_raw(&claude_live::chain(ctx), service),
        next,
        before_raw,
        from,
        to,
    )
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
            from: from.to_string(),
            to: to.to_string(),
            detail: format!("{failure}; {rollback}"),
        }),
    }
}

/// Record the new identity in Claude Code's config. Runs after the login is in place, so
/// the config never names an account before its login is live.
fn update_config(
    ctx: &Context,
    target: &Account,
    outgoing_account: &str,
    outgoing_org: &str,
) -> Result<()> {
    let path = claude::config_file(ctx);
    configfile::backup(ctx, &path)?;
    configfile::update(ctx, &path, |config| {
        configfile::splice_identity(
            config,
            target.claude().map_or(&Value::Null, |c| c.oauth_account),
            &[outgoing_account, outgoing_org],
        )
    })
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Claude Code deletes these along with the login on logout, so they belong to the
    /// account. Left behind, the incoming account would present the outgoing account's
    /// device token, and hold its second OAuth block.
    /// What gets parked is the account's whole slice, so a switch back restores what was
    /// there rather than the OAuth block and a set of missing keys.
    #[test]
    fn what_is_parked_is_everything_that_belongs_to_the_account() {
        let live = serde_json::json!({
            "claudeAiOauth": {"refreshToken": "a"},
            "organizationUuid": "org-a",
            "trustedDeviceToken": "device-of-a",
            "enterpriseGateway": {"url": "https://gateway.example"},
            "designOauth": {"refreshToken": "design-of-a"},
            "mcpOAuth": {"a-server": "token"},
            "somethingOfThisMachine": true,
        });
        let slice = slice_of(&live).expect("it has an oauth block");
        assert_eq!(
            slice,
            serde_json::json!({
                "claudeAiOauth": {"refreshToken": "a"},
                "organizationUuid": "org-a",
                "trustedDeviceToken": "device-of-a",
                "enterpriseGateway": {"url": "https://gateway.example"},
                "designOauth": {"refreshToken": "design-of-a"},
            }),
            "everything the account owns, and nothing the machine or another server owns"
        );
    }

    /// Restoring puts the incoming account's keys where the outgoing account's were, and
    /// takes away any the incoming account does not have.
    #[test]
    fn restoring_a_slice_replaces_the_outgoing_accounts_keys_rather_than_only_removing_them() {
        let before = serde_json::json!({
            "claudeAiOauth": {"refreshToken": "a"},
            "organizationUuid": "org-a",
            "trustedDeviceToken": "device-of-a",
            "designOauth": {"refreshToken": "design-of-a"},
            "mcpOAuth": {"a-server": "token"},
        });
        let incoming = serde_json::json!({
            "claudeAiOauth": {"refreshToken": "b"},
            "organizationUuid": "org-b",
            "trustedDeviceToken": "device-of-b",
        });
        let after: Value =
            serde_json::from_str(&splice(&before, &incoming).expect("spliced")).expect("json");

        assert_eq!(after["claudeAiOauth"]["refreshToken"], "b");
        assert_eq!(after["organizationUuid"], "org-b");
        assert_eq!(after["trustedDeviceToken"], "device-of-b");
        assert!(
            after.get("designOauth").is_none(),
            "a key the incoming account does not have must not be left holding the \
             outgoing account's value"
        );
        assert_eq!(
            after["mcpOAuth"]["a-server"], "token",
            "and what belongs to neither account stays"
        );
    }

    /// A park written by a version that kept only the OAuth block still restores, and still
    /// clears the outgoing account's keys, which is exactly what it always did.
    #[test]
    fn a_park_from_before_the_slice_still_restores() {
        let before = serde_json::json!({
            "claudeAiOauth": {"refreshToken": "a"},
            "organizationUuid": "org-a",
            "trustedDeviceToken": "device-of-a",
        });
        let legacy = serde_json::json!({"refreshToken": "b", "accessToken": "b-access"});
        let after: Value =
            serde_json::from_str(&splice(&before, &legacy).expect("spliced")).expect("json");

        assert_eq!(after["claudeAiOauth"]["refreshToken"], "b");
        assert!(after.get("organizationUuid").is_none());
        assert!(after.get("trustedDeviceToken").is_none());
    }

    #[test]
    fn a_switch_leaves_nothing_of_the_outgoing_account() {
        let before = serde_json::json!({
            "claudeAiOauth": {"refreshToken": "old"},
            "organizationUuid": "org-a",
            "trustedDeviceToken": "device-of-a",
            "enterpriseGateway": {"url": "https://gateway.example"},
            "designOauth": {"refreshToken": "design-of-a"},
            "somethingOfThisMachine": true,
        });
        let after: Value = serde_json::from_str(
            &splice(&before, &serde_json::json!({"refreshToken": "new"})).expect("spliced"),
        )
        .expect("valid JSON");
        assert_eq!(after["claudeAiOauth"]["refreshToken"], "new");
        assert_eq!(after["somethingOfThisMachine"], true);
        for key in ACCOUNT_SCOPED {
            assert!(after.get(key).is_none(), "{key} was left behind");
        }
    }

    use std::cell::RefCell;

    fn failing(message: &str) -> store::Error {
        store::Error::Write(message.into())
    }

    #[test]
    fn a_successful_write_needs_no_rollback() {
        let written = RefCell::new(Vec::new());
        let result = install_with(
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
        assert!(only_copy_left(&Error::SwitchCorrupted { from, to, detail }));
    }

    #[test]
    fn a_credential_without_claude_ai_oauth_is_refused() {
        assert!(slice_of(&serde_json::json!({"slackTag": {}})).is_err());
        assert!(slice_of(&serde_json::json!({"claudeAiOauth": {"accessToken": "a"}})).is_ok());
    }
}
