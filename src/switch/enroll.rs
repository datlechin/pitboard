//! Bringing an account under pitboard's care.
//!
//! A parked copy is only safe if the live slot is replaced the moment it is taken; otherwise
//! Claude Code keeps rotating the same token and the copy goes stale. So the account signed
//! in now is recorded but not parked — its first switch parks it at exactly that moment —
//! and any other account is signed in inside a private directory, where the live slot is
//! never touched and the vault is the new login's only holder.

use super::{Error, Result, Settled, access_token, identify, oauth_of, purge};
use crate::api::Owner;
use crate::state::{Account, Park, State};
use crate::{claude, home, park, state, store};
use serde_json::{Value, json};
use std::process::Command;

pub enum Enrolled {
    /// The account signed in now, recorded without parking: its first switch parks it.
    Current { email: String },
    /// Another account, signed in privately and parked.
    SignedIn { email: String },
    /// An enrolled account signed in to again: its parked login is now the new one.
    Renewed { email: String },
}

pub fn enroll(settled: Settled, label: &str, sign_in: bool) -> Result<Enrolled> {
    let Settled {
        _exclusive,
        mut state,
    } = settled;
    if sign_in {
        sign_in_new(label, &mut state)
    } else {
        record_current(label, &mut state)
    }
}

/// A label names one account for good: its own, or one not enrolled under another label.
fn claim(state: &State, label: &str, owner: &Owner) -> Result<()> {
    if let Some(taken) = state.get(label)
        && taken.account_uuid != owner.account_uuid
    {
        return Err(Error::LabelTaken {
            label: label.to_string(),
            email: taken.email.clone(),
        });
    }
    if let Some(existing) = state.by_uuid(&owner.account_uuid)
        && existing.label != label
    {
        return Err(Error::AlreadyEnrolled {
            email: owner.email.clone(),
            label: existing.label.clone(),
        });
    }
    Ok(())
}

fn record_current(label: &str, state: &mut State) -> Result<Enrolled> {
    let live = store::read(&claude::live_service())?.ok_or(Error::LiveCredentialAbsent)?;
    let owner = identify(&access_token(&live)?)?;
    claim(state, label, &owner)?;
    let parked = state.get(label).and_then(|a| a.parked.clone());
    state.upsert(account(label, &owner, parked));
    state.active = Some(label.to_string());
    state::save(state)?;
    Ok(Enrolled::Current { email: owner.email })
}

fn sign_in_new(label: &str, state: &mut State) -> Result<Enrolled> {
    let dir = home::dir().join("signin");
    let _ = std::fs::remove_dir_all(&dir);
    home::create_private(&dir).map_err(|source| Error::HomeUnwritable {
        path: dir.clone(),
        source,
    })?;

    // Anthropic's own sign-in, run in a private directory. pitboard never sees the sign-in;
    // it only reads the login Claude Code stores once it is done.
    let finished = Command::new("claude")
        .args(["auth", "login"])
        .env("CLAUDE_CONFIG_DIR", &dir)
        .env_remove("CLAUDE_SECURESTORAGE_CONFIG_DIR")
        .status()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::ClaudeNotFound,
            _ => Error::SignInIncomplete,
        })?
        .success();

    let result = (|| {
        if !finished {
            return Err(Error::SignInIncomplete);
        }
        let raw = store::read_signin(&dir)?.ok_or(Error::SignInIncomplete)?;
        let document: Value =
            serde_json::from_str(&raw).map_err(|e| Error::LiveCredentialShapeUnexpected {
                detail: e.to_string(),
            })?;
        let owner = identify(&access_token(&document)?)?;
        claim(state, label, &owner)?;
        let service = park::reserve(&owner.account_uuid)?;
        let fresh = park::store_at(&service, &oauth_of(&document)?)?;
        let previous = state.get(label).and_then(|a| a.parked.clone());
        let renewed = state.get(label).is_some();
        state.upsert(account(label, &owner, previous));
        state.park(label, fresh);
        // Unrecorded, the new login would be an item nothing refers to, never deleted.
        state::save(state).inspect_err(|_| {
            let _ = store::vault_delete(&service);
        })?;
        purge(state);
        Ok(if renewed {
            Enrolled::Renewed { email: owner.email }
        } else {
            Enrolled::SignedIn { email: owner.email }
        })
    })();

    let _ = store::discard_signin(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Only what Anthropic just confirmed. Leaving the rest out makes Claude Code fetch its own
/// profile after a switch rather than trust a copy pitboard wrote.
fn account(label: &str, owner: &Owner, parked: Option<Park>) -> Account {
    Account {
        label: label.to_string(),
        account_uuid: owner.account_uuid.clone(),
        email: owner.email.clone(),
        organization_uuid: owner.organization_uuid.clone(),
        oauth_account: json!({
            "accountUuid": owner.account_uuid,
            "emailAddress": owner.email,
            "organizationUuid": owner.organization_uuid,
        }),
        parked,
    }
}
