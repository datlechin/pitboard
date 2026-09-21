//! Bringing an account under pitboard's care.
//!
//! A parked copy is only safe if the live slot is replaced the moment it is taken; otherwise
//! Claude Code keeps rotating the same token and the copy goes stale. So the account signed
//! in now is recorded but not parked — its first switch parks it at exactly that moment —
//! and any other account is signed in inside a private directory, where the live slot is
//! never touched and the vault is the new login's only holder.

use super::{Error, Result, Settled, access_token, identify, oauth_of, purge};
use crate::api::Owner;
use crate::context::Context;
use crate::state::{Account, Park, State};
use crate::{claude, home, park, state, store};
use serde_json::{Value, json};
use std::fs::{File, OpenOptions, TryLockError};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::Command;

pub enum Enrolled {
    /// The account signed in now, recorded without parking: its first switch parks it.
    Current { email: String },
    /// Another account, signed in privately and parked.
    SignedIn { email: String },
    /// An enrolled account signed in to again: its parked login is now the new one.
    Renewed { email: String },
}

/// A login Claude Code stored for pitboard in a private directory, not yet enrolled. Dropping
/// it deletes that directory and the credential Claude Code kept for it.
pub struct SignIn {
    dir: PathBuf,
    document: Value,
    ctx: Context,
    _one_at_a_time: File,
}

impl Drop for SignIn {
    fn drop(&mut self) {
        let _ = store::discard_signin(&self.ctx, &self.dir);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Run Claude Code's own sign-in in a private directory, where the live login is never
/// touched. It waits on a person in a browser, so it takes no lock but its own: a switch
/// meanwhile goes ahead, and a second sign-in is refused rather than queued.
pub fn sign_in(ctx: &Context) -> Result<SignIn> {
    let home = home::ensure(ctx).map_err(|source| Error::HomeUnwritable {
        path: home::dir(ctx),
        source,
    })?;
    let lock_path = home.join("signin.lock");
    let one_at_a_time = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(&lock_path)
        .map_err(|source| Error::HomeUnwritable {
            path: lock_path.clone(),
            source,
        })?;
    match one_at_a_time.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Err(Error::SignInInProgress),
        Err(TryLockError::Error(source)) => {
            return Err(Error::HomeUnwritable {
                path: lock_path,
                source,
            });
        }
    }

    let dir = home.join("signin");
    let _ = std::fs::remove_dir_all(&dir);
    home::create_private(&dir).map_err(|source| Error::HomeUnwritable {
        path: dir.clone(),
        source,
    })?;
    let mut pending = SignIn {
        dir,
        document: Value::Null,
        ctx: ctx.clone(),
        _one_at_a_time: one_at_a_time,
    };

    // pitboard never sees the sign-in; it reads the login Claude Code stores once it is done.
    // What Claude Code prints goes to stderr, so `--json` output stays one JSON line.
    let finished = Command::new(&ctx.claude_program)
        .args(["auth", "login"])
        .env("CLAUDE_CONFIG_DIR", &pending.dir)
        .env_remove("CLAUDE_SECURESTORAGE_CONFIG_DIR")
        .stdout(std::io::stderr())
        .status()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::ClaudeNotFound,
            _ => Error::SignInIncomplete,
        })?
        .success();
    if !finished {
        return Err(Error::SignInIncomplete);
    }
    let raw = store::read_signin(ctx, &pending.dir)?.ok_or(Error::SignInIncomplete)?;
    pending.document =
        serde_json::from_str(&raw).map_err(|e| Error::LiveCredentialShapeUnexpected {
            detail: e.to_string(),
        })?;
    Ok(pending)
}

/// Enroll the account signed in now, or with `signed_in`, the one a sign-in just produced.
pub fn enroll(settled: Settled, label: &str, signed_in: Option<SignIn>) -> Result<Enrolled> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
    } = settled;
    match signed_in {
        Some(login) => park_signed_in(&ctx, label, &mut state, &login),
        None => record_current(&ctx, label, &mut state),
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

fn record_current(ctx: &Context, label: &str, state: &mut State) -> Result<Enrolled> {
    let live = store::read(ctx, &claude::live_service(ctx))?.ok_or(Error::LiveCredentialAbsent)?;
    let owner = identify(ctx, &access_token(&live)?)?;
    claim(state, label, &owner)?;
    let parked = state.get(label).and_then(|a| a.parked.clone());
    state.upsert(account(label, &owner, parked));
    state.active = Some(label.to_string());
    state::save(ctx, state)?;
    Ok(Enrolled::Current { email: owner.email })
}

fn park_signed_in(
    ctx: &Context,
    label: &str,
    state: &mut State,
    login: &SignIn,
) -> Result<Enrolled> {
    let owner = identify(ctx, &access_token(&login.document)?)?;
    claim(state, label, &owner)?;
    let service = park::reserve(ctx, &owner.account_uuid)?;
    let fresh = park::store_at(ctx, &service, &oauth_of(&login.document)?)?;
    let previous = state.get(label).and_then(|a| a.parked.clone());
    let renewed = state.get(label).is_some();
    state.upsert(account(label, &owner, previous));
    state.park(label, fresh);
    // Unrecorded, the new login would be an item nothing refers to, never deleted.
    state::save(ctx, state).inspect_err(|_| {
        let _ = store::vault_delete(ctx, &service);
    })?;
    purge(ctx, state);
    Ok(if renewed {
        Enrolled::Renewed { email: owner.email }
    } else {
        Enrolled::SignedIn { email: owner.email }
    })
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
