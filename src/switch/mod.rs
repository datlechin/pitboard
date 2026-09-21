//! Moving the signed-in identity from one enrolled account to another.
//!
//! The ordering here is the whole design. Two rules produce it.
//!
//! Everything that decides *which account the outgoing credential is filed under* is read
//! under Claude Code's write lock, before the config is touched. Reading the identity
//! earlier means filing a credential against a config that has since moved, which files
//! one account's token as another's and destroys both.
//!
//! Additive writes are made durable before destructive ones. Attaching a parked generation
//! cannot lose anything, so it is saved first; installing a credential is irreversible, so
//! it is last. A run that dies in between leaves a superfluous copy, never a missing one.

mod enroll;
mod forget;
mod journal;

pub use enroll::{Enrolled, enroll};
pub use forget::forget;

use crate::error::{Error, Result};
use crate::state::{Account, Generation};
use crate::{api, claude, configfile, home, lock, park, state, store, time};
use journal::{Journal, journal_path, reconcile, write_journal};
use serde_json::Value;
use std::path::PathBuf;

/// Claude Code re-reads the credential store behind a cache anchored at each process's
/// first read. Measured over three runs on one machine: swapping at t+8, t+20 and t+28
/// seconds all took effect at t+32.3, t+33.5 and t+32.95 from process start.
pub const ADOPTION_CEILING_SECONDS: u32 = 33;

pub enum Outcome {
    Switched {
        from: String,
        to: String,
        parked: Generation,
        /// The credential moved but the config did not. Claude Code repairs this on its
        /// next call, so it is reported rather than treated as a failure.
        config_warning: Option<Error>,
        stuck_generations: Vec<String>,
    },
    /// Asking for the account that is already signed in is not a failure: the state the
    /// caller wanted already holds.
    AlreadyActive { label: String },
}

pub(super) fn oauth_of(document: &Value) -> Result<Value> {
    document
        .get("claudeAiOauth")
        .cloned()
        .ok_or_else(|| Error::LiveCredentialShapeUnexpected {
            detail: "it has no claudeAiOauth block".into(),
        })
}

/// The lock that makes two pitboard runs exclusive of each other.
pub(super) fn exclusive() -> Result<lock::Guard> {
    home::ensure().map_err(|source| Error::RecoveryFailed {
        path: home::dir(),
        source,
    })?;
    Ok(lock::acquire(&home::dir().join("state"))?)
}

/// Who a live access token belongs to. Refusing when this cannot be answered is the point:
/// filing a credential under a guessed account is how two accounts are destroyed at once.
pub(super) fn identify(access_token: &str) -> Result<api::Owner> {
    api::owner(access_token).map_err(|e| match e {
        api::ApiError::Unauthorized => Error::SessionExpired,
        other => Error::IdentityUnverifiable {
            detail: other.to_string(),
        },
    })
}

pub(super) fn access_token(document: &Value) -> Result<String> {
    document["claudeAiOauth"]["accessToken"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::LiveCredentialShapeUnexpected {
            detail: "it has no access token".into(),
        })
}

pub fn switch(label: &str) -> Result<Outcome> {
    let _exclusive = exclusive()?;
    let mut state = state::load()?;
    if let Some(note) = reconcile(&mut state)? {
        eprintln!("note: {note}");
    }

    let target = state
        .get(label)
        .cloned()
        .ok_or_else(|| Error::AccountUnknown {
            label: label.to_string(),
        })?;

    // Identified before taking Claude Code's lock, so the round trip does not hold up its
    // writes; confirmed again under the lock below.
    let service = claude::live_service();
    let live = store::read(&service)?.ok_or(Error::LiveCredentialAbsent)?;
    let identified_with = access_token(&live)?;
    let outgoing = identify(&identified_with)?;

    if outgoing.account_uuid == target.account_uuid {
        return Ok(Outcome::AlreadyActive {
            label: label.to_string(),
        });
    }
    let outgoing_label = state
        .by_uuid(&outgoing.account_uuid)
        .map(|a| a.label.clone())
        .ok_or_else(|| Error::LiveAccountNotEnrolled {
            email: outgoing.email.clone(),
        })?;
    let generation = target
        .restorable()
        .cloned()
        .ok_or_else(|| Error::AccountNotRestorable {
            label: label.to_string(),
        })?;
    let incoming = park::load(label, &generation)?;

    let storage = PathBuf::from(claude::storage_dir()).join(".storage-write");
    let guard = lock::acquire(&storage)?;

    let before_raw = store::read_raw(&service)?.ok_or(Error::LiveCredentialAbsent)?;
    let before: Value =
        serde_json::from_str(&before_raw).map_err(|e| Error::LiveCredentialShapeUnexpected {
            detail: e.to_string(),
        })?;
    // A refresh keeps the account, so an unchanged access token needs no second round trip.
    // A sign-in between the two reads would not keep it.
    let now_token = access_token(&before)?;
    if now_token != identified_with && identify(&now_token)?.account_uuid != outgoing.account_uuid {
        return Err(Error::SignedInAccountChanged);
    }

    let park_service = park::reserve(&outgoing.account_uuid)?;
    write_journal(&Journal {
        started_at: time::now(),
        from_label: outgoing_label.clone(),
        from_uuid: outgoing.account_uuid.clone(),
        to_label: label.to_string(),
        to_uuid: target.account_uuid.clone(),
        park_service: park_service.clone(),
        incoming_service: generation.service.clone(),
    })?;

    let parked = park::store_at(&park_service, &oauth_of(&before)?)?;
    state.attach(&outgoing_label, parked.clone());
    state::save(&state)?;

    if let Err(e) = install(
        &service,
        &before,
        &before_raw,
        &incoming,
        &outgoing_label,
        label,
    ) {
        // The outgoing account is still signed in, so Claude Code keeps rotating the token
        // this copy was taken from. It would go stale, and restoring a stale copy zeroes the
        // login, so it is retired now rather than left to be picked later.
        state.mark_installed(&outgoing_label, &parked.service, time::now());
        state::save(&state)?;
        let _ = std::fs::remove_file(journal_path());
        return Err(e);
    }
    state.mark_installed(label, &generation.service, time::now());
    state.active = Some(label.to_string());
    state::save(&state)?;
    drop(guard);

    let config_warning =
        update_config(&target, &outgoing.account_uuid, &outgoing.organization_uuid).err();

    let mut stuck_generations = Vec::new();
    if let Some(account) = state
        .accounts
        .iter_mut()
        .find(|a| a.label == outgoing_label)
    {
        stuck_generations = park::prune(account);
    }
    state::save(&state)?;
    let _ = std::fs::remove_file(journal_path());

    Ok(Outcome::Switched {
        from: outgoing_label,
        to: label.to_string(),
        parked,
        config_warning,
        stuck_generations,
    })
}

/// Replace `claudeAiOauth` in the live document, restoring the previous bytes if anything
/// about the write does not hold. Every other key belongs to this machine and stays.
/// Replace `claudeAiOauth` in the live document, putting the previous bytes back if
/// anything about the write does not hold. Every other key belongs to this machine.
fn install(
    service: &str,
    before: &Value,
    before_raw: &str,
    incoming: &Value,
    from: &str,
    to: &str,
) -> Result<()> {
    let mut next = before.clone();
    next.as_object_mut()
        .ok_or_else(|| Error::LiveCredentialShapeUnexpected {
            detail: "it is not a JSON object".into(),
        })?
        .insert("claudeAiOauth".into(), incoming.clone());
    let body = serde_json::to_string(&next).expect("a credential document stays serialisable");

    match store::write_raw(service, &body) {
        Ok(()) => Ok(()),
        Err(failure) => match store::write_raw(service, before_raw) {
            Ok(()) => Err(Error::SwitchRolledBack {
                from: from.to_string(),
                to: to.to_string(),
                detail: failure.to_string(),
            }),
            Err(rollback) => Err(Error::SwitchCorrupted {
                from: from.to_string(),
                to: to.to_string(),
                detail: format!("{failure}; {rollback}"),
            }),
        },
    }
}

/// Record the new identity. Runs after the credential is in place, so the config can never
/// claim an account the live slot does not hold.
/// Record the new identity. Runs after the credential is in place, so the config can
/// never claim an account the live slot does not hold.
fn update_config(target: &Account, outgoing_account: &str, outgoing_org: &str) -> Result<()> {
    let path = configfile::path();
    configfile::backup(&path)?;
    let mut config = claude::load_config()?;
    configfile::splice_identity(
        &mut config,
        &target.oauth_account,
        &[outgoing_account, outgoing_org],
    );
    configfile::write(&path, &config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_credential_without_claude_ai_oauth_is_refused() {
        assert!(oauth_of(&serde_json::json!({"slackTag": {}})).is_err());
        assert!(oauth_of(&serde_json::json!({"claudeAiOauth": {"accessToken": "a"}})).is_ok());
    }
}
