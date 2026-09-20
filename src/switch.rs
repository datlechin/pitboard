//! Moving the signed-in identity from one enrolled account to another.

use crate::state::{Account, Generation};
use crate::{claude, configfile, lock, park, state, store, time};
use serde_json::Value;
use std::path::PathBuf;

/// Claude Code re-reads the credential store behind a 30-second cache. Measured over
/// three runs, a live session picked up a swapped credential 4.9, 13.5 and 24.3 seconds
/// after the swap, each time about 33 seconds after that process first read the store.
pub const ADOPTION_CEILING_SECONDS: u32 = 33;

pub struct Outcome {
    pub from: Option<String>,
    pub to: String,
    pub parked: Generation,
    pub config_reasserted: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Journal {
    pid: u32,
    started_at: i64,
    from: Option<String>,
    to: String,
    stage: String,
}

fn journal_path() -> PathBuf {
    state::dir().join("journal.json")
}

fn process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// Refuse while another switch is running; salvage the record of one that died.
fn claim_journal(from: Option<String>, to: &str) -> Result<(), String> {
    let path = journal_path();
    if let Ok(raw) = std::fs::read_to_string(&path) {
        let previous: Journal = serde_json::from_str(&raw)
            .map_err(|e| format!("{} is unreadable: {e}", path.display()))?;
        if process_alive(previous.pid) {
            return Err(format!(
                "another pitboard is switching accounts right now (pid {})",
                previous.pid
            ));
        }
        let salvage =
            state::dir().join(format!("journal.interrupted.{}.json", previous.started_at));
        std::fs::rename(&path, &salvage).map_err(|e| e.to_string())?;
        eprintln!(
            "note: an earlier switch to {} stopped at stage `{}`; its record is in {}",
            previous.to,
            previous.stage,
            salvage.display()
        );
    }
    std::fs::create_dir_all(state::dir()).map_err(|e| e.to_string())?;
    record(Journal {
        pid: std::process::id(),
        started_at: time::now(),
        from,
        to: to.to_string(),
        stage: "starting".into(),
    })
}

fn record(entry: Journal) -> Result<(), String> {
    let body = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
    std::fs::write(journal_path(), body).map_err(|e| e.to_string())
}

fn stage(from: Option<&str>, to: &str, name: &str) {
    let _ = record(Journal {
        pid: std::process::id(),
        started_at: time::now(),
        from: from.map(str::to_string),
        to: to.to_string(),
        stage: name.into(),
    });
}

fn oauth_of(document: &Value) -> Result<Value, String> {
    document
        .get("claudeAiOauth")
        .cloned()
        .ok_or_else(|| "the credential has no claudeAiOauth".to_string())
}

pub fn switch(label: &str) -> Result<Outcome, String> {
    let mut state = state::load()?;
    let target = state
        .get(label)
        .ok_or_else(|| format!("no account is enrolled as `{label}`"))?
        .clone();
    let generation = target
        .newest()
        .ok_or_else(|| format!("`{label}` has no parked credential"))?
        .clone();

    let service = claude::live_service();
    let live = store::read(&service, store::Owner::ClaudeCode)
        .map_err(|e| e.to_string())?
        .ok_or("nothing is signed in, so there is nothing to switch from")?;

    let mut config = claude::load_config()?;
    let outgoing = claude::identity(&config)
        .ok_or("Claude Code has not recorded who is signed in; run `claude` once first")?;
    if outgoing.account_uuid == target.account_uuid {
        return Err(format!("`{label}` is already signed in"));
    }
    // Overwriting a credential we cannot park would strand that account behind a browser
    // sign-in, so refuse rather than lose it.
    if state.by_uuid(&outgoing.account_uuid).is_none() {
        return Err(format!(
            "{} is signed in but not enrolled; run `pitboard enroll <label>` first so it can be parked",
            outgoing.email
        ));
    }

    let incoming_oauth = park::load(&generation)?;

    claim_journal(Some(outgoing.email.clone()), label)?;
    let config_path = configfile::path();
    let backup = configfile::backup(&config_path)?;

    stage(Some(&outgoing.email), label, "config");
    configfile::splice_identity(
        &mut config,
        &target.oauth_account,
        &[&outgoing.account_uuid, &outgoing.organization_uuid],
    );
    configfile::write(&config_path, &config)?;

    stage(Some(&outgoing.email), label, "credential");
    let parked = match replace_credential(&service, &live, &incoming_oauth, &outgoing.account_uuid)
    {
        Ok(p) => p,
        Err(e) => {
            let _ = std::fs::copy(&backup, &config_path);
            let _ = std::fs::remove_file(journal_path());
            return Err(e);
        }
    };

    stage(Some(&outgoing.email), label, "settling");
    let config_reasserted = reassert_identity(&config_path, &target, &outgoing.account_uuid);

    let from_label = state
        .by_uuid(&outgoing.account_uuid)
        .map(|a| a.label.clone());
    if let Some(ref name) = from_label {
        if let Some(account) = state.accounts.iter_mut().find(|a| a.label == *name) {
            account.generations.push(parked.clone());
            park::prune(account);
        }
    }
    state.active = Some(label.to_string());
    state::save(&state)?;
    let _ = std::fs::remove_file(journal_path());

    Ok(Outcome {
        from: from_label,
        to: label.to_string(),
        parked,
        config_reasserted,
    })
}

/// Park the outgoing credential and install the incoming one, under Claude Code's own
/// write lock, rolling back to the bytes read inside that lock if anything fails.
fn replace_credential(
    service: &str,
    _preflight: &Value,
    incoming_oauth: &Value,
    outgoing_uuid: &str,
) -> Result<Generation, String> {
    let storage = PathBuf::from(claude::storage_dir()).join(".storage-write");
    let _guard = lock::acquire(&storage).map_err(|e| e.to_string())?;

    let before_raw = store::read_raw(service, store::Owner::ClaudeCode)
        .map_err(|e| e.to_string())?
        .ok_or("the credential vanished between the check and the write")?;
    let before: Value =
        serde_json::from_str(&before_raw).map_err(|e| format!("credential is not JSON: {e}"))?;

    let parked = park::store_generation(outgoing_uuid, &oauth_of(&before)?)?;

    // Only claudeAiOauth moves. Anything else in the document is bound to this machine.
    let mut next = before.clone();
    next.as_object_mut()
        .ok_or("credential is not an object")?
        .insert("claudeAiOauth".into(), incoming_oauth.clone());
    let body = serde_json::to_string(&next).map_err(|e| e.to_string())?;

    if let Err(e) = store::write_raw(service, store::Owner::ClaudeCode, &body) {
        match store::write_raw(service, store::Owner::ClaudeCode, &before_raw) {
            Ok(()) => Err(format!("{e}; the previous account is still signed in")),
            Err(rollback) => Err(format!(
                "{e}; and restoring the previous credential also failed: {rollback}. \
                 The parked copy is {}",
                parked.service
            )),
        }
    } else {
        Ok(parked)
    }
}

/// Put the identity back if a running Claude Code overwrote it while we held the lock.
fn reassert_identity(path: &std::path::Path, target: &Account, outgoing_uuid: &str) -> bool {
    let Ok(mut config) = claude::load_config() else {
        return false;
    };
    let current = claude::identity(&config).map(|i| i.account_uuid);
    if current.as_deref() == Some(target.account_uuid.as_str()) {
        return true;
    }
    configfile::splice_identity(&mut config, &target.oauth_account, &[outgoing_uuid]);
    configfile::write(path, &config).is_ok()
}

/// Record the account that is signed in now, and park a copy of its credential.
///
/// Parking here rather than only at the next switch is what makes the account survive
/// the user signing in as a different one, which overwrites the live credential.
pub fn enroll_current(label: &str) -> Result<Account, String> {
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
    let existing = state.by_uuid(&identity.account_uuid).cloned();
    let mut generations = existing.map(|a| a.generations).unwrap_or_default();
    generations.push(park::store_generation(
        &identity.account_uuid,
        &oauth_of(&live)?,
    )?);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_credential_without_claude_ai_oauth_is_refused() {
        assert!(oauth_of(&serde_json::json!({"slackTag": {}})).is_err());
        assert!(oauth_of(&serde_json::json!({"claudeAiOauth": {"accessToken": "a"}})).is_ok());
    }

    #[test]
    fn this_process_is_alive_and_pid_one_is_not_us() {
        assert!(process_alive(std::process::id()));
        assert!(!process_alive(0x7FFF_FFFF));
    }
}
