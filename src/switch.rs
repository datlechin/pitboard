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

use crate::state::{Account, Generation, State};
use crate::{claude, configfile, lock, park, state, store, time};
use serde_json::Value;
use std::path::PathBuf;

/// Claude Code re-reads the credential store behind a cache anchored at each process's
/// first read. Measured over three runs on one machine: swapping at t+8, t+20 and t+28
/// seconds all took effect at t+32.3, t+33.5 and t+32.95 from process start.
pub const ADOPTION_CEILING_SECONDS: u32 = 33;

pub struct Outcome {
    pub from: String,
    pub to: String,
    pub parked: Generation,
    /// Set when the credential moved but the config could not be updated. Claude Code
    /// corrects this itself on its next call; the credential is what decides.
    pub config_warning: Option<String>,
    pub stuck_generations: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Journal {
    started_at: i64,
    from_label: String,
    from_uuid: String,
    to_label: String,
    park_service: String,
    incoming_fingerprint: String,
}

fn journal_path() -> PathBuf {
    state::dir().join("journal.json")
}

fn write_journal(entry: &Journal) -> Result<(), String> {
    std::fs::create_dir_all(state::dir()).map_err(|e| e.to_string())?;
    let body = serde_json::to_string(entry).map_err(|e| e.to_string())?;
    std::fs::write(journal_path(), body).map_err(|e| e.to_string())
}

/// Finish, in state, what an interrupted run already did in the keychain.
///
/// Called while holding pitboard's own lock, so no other run is in flight. A park item
/// that exists but is referenced by nothing would otherwise be invisible to every command,
/// leaving the account pointing at an older copy whose token has since been rotated.
fn reconcile(state: &mut State) -> Result<Option<String>, String> {
    let path = journal_path();
    let raw = match std::fs::read_to_string(&path) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let Ok(journal) = serde_json::from_str::<Journal>(&raw) else {
        let _ = std::fs::remove_file(&path);
        return Ok(Some(
            "an interrupted switch left an unreadable record; ignoring it".into(),
        ));
    };

    let mut note = format!(
        "an earlier switch from `{}` to `{}` was interrupted",
        journal.from_label, journal.to_label
    );

    if !state.references(&journal.park_service) {
        if let Ok(Some(raw)) = store::keychain_read(&journal.park_service) {
            if let Ok(oauth) = serde_json::from_str::<Value>(&raw) {
                state.attach(
                    &journal.from_label,
                    Generation {
                        service: journal.park_service.clone(),
                        parked_at: journal.started_at,
                        refresh_fingerprint: park::fingerprint_of(&oauth),
                        installed_at: None,
                    },
                );
                note.push_str("; its parked credential has been recovered");
            }
        }
    }

    // Whether the install landed is decided by the credential itself, not by how far the
    // journal got.
    let service = claude::live_service();
    if let Ok(Some(live)) = store::read(&service, store::Owner::ClaudeCode) {
        if park::fingerprint_of(&live["claudeAiOauth"]) == journal.incoming_fingerprint {
            state.active = Some(journal.to_label.clone());
            note.push_str("; the switch had in fact completed");
        }
    }

    state::save(state)?;
    let _ = std::fs::remove_file(&path);
    Ok(Some(note))
}

fn oauth_of(document: &Value) -> Result<Value, String> {
    document
        .get("claudeAiOauth")
        .cloned()
        .ok_or_else(|| "the credential has no claudeAiOauth".to_string())
}

/// The lock that makes two pitboard runs exclusive of each other.
fn exclusive() -> Result<lock::Guard, String> {
    std::fs::create_dir_all(state::dir()).map_err(|e| e.to_string())?;
    lock::acquire(&state::dir().join("state")).map_err(|e| e.to_string())
}

pub fn switch(label: &str) -> Result<Outcome, String> {
    let _exclusive = exclusive()?;
    let mut state = state::load()?;
    if let Some(note) = reconcile(&mut state)? {
        eprintln!("note: {note}");
    }

    let target = state
        .get(label)
        .cloned()
        .ok_or_else(|| format!("no account is enrolled as `{label}`"))?;
    let generation = target.restorable().cloned().ok_or_else(|| {
        format!(
            "`{label}` has no restorable parked credential; its last copy was already used \
             and Claude Code has rotated past it. Sign in as that account again."
        )
    })?;
    let incoming = park::load(&generation)?;

    let service = claude::live_service();
    let storage = PathBuf::from(claude::storage_dir()).join(".storage-write");
    let guard = lock::acquire(&storage).map_err(|e| e.to_string())?;

    let config = claude::load_config()?;
    let outgoing = claude::identity(&config)
        .ok_or("Claude Code has not recorded who is signed in; run `claude` once first")?;
    if outgoing.account_uuid == target.account_uuid {
        return Err(format!("`{label}` is already signed in"));
    }
    let outgoing_label = state
        .by_uuid(&outgoing.account_uuid)
        .map(|a| a.label.clone())
        .ok_or_else(|| {
            format!(
                "{} is signed in but not enrolled; run `pitboard enroll <label>` first so it can be parked",
                outgoing.email
            )
        })?;

    let before_raw = store::read_raw(&service, store::Owner::ClaudeCode)
        .map_err(|e| e.to_string())?
        .ok_or("nothing is signed in, so there is nothing to switch from")?;
    let before: Value =
        serde_json::from_str(&before_raw).map_err(|e| format!("credential is not JSON: {e}"))?;

    let park_service = park::reserve(&outgoing.account_uuid)?;
    write_journal(&Journal {
        started_at: time::now(),
        from_label: outgoing_label.clone(),
        from_uuid: outgoing.account_uuid.clone(),
        to_label: label.to_string(),
        park_service: park_service.clone(),
        incoming_fingerprint: park::fingerprint_of(&incoming),
    })?;

    let parked = park::store_at(&park_service, &oauth_of(&before)?)?;
    state.attach(&outgoing_label, parked.clone());
    state::save(&state)?;

    install(&service, &before, &before_raw, &incoming)?;
    state.mark_installed(label, &generation.service, time::now());
    state.active = Some(label.to_string());
    state::save(&state)?;
    drop(guard);

    let config_warning =
        update_config(&target, &outgoing.account_uuid, &outgoing.organization_uuid);

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

    Ok(Outcome {
        from: outgoing_label,
        to: label.to_string(),
        parked,
        config_warning,
        stuck_generations,
    })
}

/// Replace `claudeAiOauth` in the live document, restoring the previous bytes if anything
/// about the write does not hold. Every other key belongs to this machine and stays.
fn install(
    service: &str,
    before: &Value,
    before_raw: &str,
    incoming: &Value,
) -> Result<(), String> {
    let mut next = before.clone();
    next.as_object_mut()
        .ok_or("credential is not an object")?
        .insert("claudeAiOauth".into(), incoming.clone());
    let body = serde_json::to_string(&next).map_err(|e| e.to_string())?;

    match store::write_raw(service, store::Owner::ClaudeCode, &body) {
        Ok(()) => Ok(()),
        Err(e) => match store::write_raw(service, store::Owner::ClaudeCode, before_raw) {
            Ok(()) => Err(format!("{e}; the previous account is still signed in")),
            Err(rollback) => Err(format!(
                "{e}; and restoring the previous credential also failed: {rollback}"
            )),
        },
    }
}

/// Record the new identity. Runs after the credential is in place, so the config can never
/// claim an account the live slot does not hold.
fn update_config(target: &Account, outgoing_account: &str, outgoing_org: &str) -> Option<String> {
    let path = configfile::path();
    if let Err(e) = configfile::backup(&path) {
        return Some(format!("could not back up the config: {e}"));
    }
    let mut config = match claude::load_config() {
        Ok(c) => c,
        Err(e) => return Some(e),
    };
    configfile::splice_identity(
        &mut config,
        &target.oauth_account,
        &[outgoing_account, outgoing_org],
    );
    configfile::write(&path, &config).err()
}

/// Record the account that is signed in now, and park a copy of its credential.
///
/// Parking here rather than only at the next switch is what makes the account survive the
/// user signing in as a different one, which overwrites the live credential.
pub fn enroll_current(label: &str) -> Result<Account, String> {
    let _exclusive = exclusive()?;
    let mut state = state::load()?;
    let config = claude::load_config()?;
    let identity = claude::identity(&config)
        .ok_or("Claude Code has not recorded who is signed in; run `claude` once first")?;

    if let Some(existing) = state.by_uuid(&identity.account_uuid) {
        if existing.label != label {
            return Err(format!(
                "{} is already enrolled as `{}`",
                identity.email, existing.label
            ));
        }
    }
    if let Some(taken) = state.get(label) {
        if taken.account_uuid != identity.account_uuid {
            return Err(format!(
                "`{label}` already refers to {}; choose another label",
                taken.email
            ));
        }
    }

    let mut oauth_account = config
        .get("oauthAccount")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));
    if let Some(o) = oauth_account.as_object_mut() {
        o.remove("profileFetchedAt");
    }

    let service = claude::live_service();
    let live = store::read(&service, store::Owner::ClaudeCode)
        .map_err(|e| e.to_string())?
        .ok_or("nothing is signed in to enroll")?;

    let park_service = park::reserve(&identity.account_uuid)?;
    let generation = park::store_at(&park_service, &oauth_of(&live)?)?;

    let mut generations = state
        .by_uuid(&identity.account_uuid)
        .map(|a| a.generations.clone())
        .unwrap_or_default();
    generations.push(generation);

    let account = Account {
        label: label.to_string(),
        account_uuid: identity.account_uuid.clone(),
        email: identity.email.clone(),
        organization_uuid: identity.organization_uuid.clone(),
        oauth_account,
        generations,
    };
    state.upsert(account.clone());
    state.active = Some(label.to_string());
    state::save(&state)?;
    Ok(account)
}

pub fn forget(label: &str) -> Result<(String, Vec<String>), String> {
    let _exclusive = exclusive()?;
    let mut state = state::load()?;
    let index = state
        .accounts
        .iter()
        .position(|a| a.label == label)
        .ok_or_else(|| format!("no account is enrolled as `{label}`"))?;
    if state.active.as_deref() == Some(label) {
        return Err(format!(
            "`{label}` is signed in; switch to another account first"
        ));
    }
    let account = state.accounts.remove(index);
    state::save(&state)?;

    let stuck = account
        .generations
        .iter()
        .filter(|g| store::delete(&g.service).is_err())
        .map(|g| g.service.clone())
        .collect();
    Ok((account.email, stuck))
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
