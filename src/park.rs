//! Where an account's credential waits while another one is signed in.
//!
//! Generations are append-only. No failure path deletes one: if a write goes wrong the
//! previous generation is still there. Pruning runs only after the state that stopped
//! referencing a generation is durable.

use crate::state::{Account, Generation, retained};
use crate::{store, time};
use serde_json::Value;

pub fn service_name(account_uuid: &str, at_millis: i64) -> String {
    format!("pitboard-park-{account_uuid}-{at_millis}")
}

/// Claim a name no generation occupies, before anything is written to it.
///
/// Reserving separately from writing lets the caller record its intent first, so a run
/// that dies mid-park leaves a name that recovery can go looking for. Two parks of one
/// account can land in the same millisecond, and reusing a name would destroy the
/// generation already there.
pub fn reserve(account_uuid: &str) -> Result<String, String> {
    let start = time::now_millis();
    for offset in 0..1_000 {
        let candidate = service_name(account_uuid, start + offset);
        match store::keychain_read(&candidate) {
            Ok(None) => return Ok(candidate),
            Ok(Some(_)) => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err(format!("cannot find a free park slot for {account_uuid}"))
}

/// Write a credential into a reserved name and prove it reads back.
pub fn store_at(service: &str, oauth: &Value) -> Result<Generation, String> {
    let body = serde_json::to_string(oauth).map_err(|e| e.to_string())?;
    if store::too_large(service, &body) {
        return Err(format!("credential is too large to park in {service}"));
    }
    store::keychain_write(service, &body)?;
    match store::keychain_read(service).map_err(|e| e.to_string())? {
        Some(back) if back == body => {}
        _ => return Err(format!("{service} did not read back as written")),
    }
    Ok(Generation {
        service: service.to_string(),
        parked_at: time::now(),
        refresh_fingerprint: fingerprint_of(oauth),
        installed_at: None,
    })
}

pub fn fingerprint_of(oauth: &Value) -> String {
    oauth
        .get("refreshToken")
        .and_then(Value::as_str)
        .map(store::fingerprint)
        .unwrap_or_default()
}

pub fn load(generation: &Generation) -> Result<Value, String> {
    let raw = store::keychain_read(&generation.service)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            format!(
                "the parked credential {} is gone from the keychain",
                generation.service
            )
        })?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|e| format!("{} is not valid JSON: {e}", generation.service))?;

    if fingerprint_of(&value) != generation.refresh_fingerprint {
        return Err(format!(
            "{} holds a different credential than the one parked there",
            generation.service
        ));
    }
    Ok(value)
}

/// Delete generations past the retention window, reporting the ones that resisted.
pub fn prune(account: &mut Account) -> Vec<String> {
    let keep: Vec<String> = retained(&account.generations)
        .iter()
        .map(|g| g.service.clone())
        .collect();
    let mut stuck = Vec::new();
    account.generations.retain(|g| {
        if keep.contains(&g.service) {
            return true;
        }
        match store::delete(&g.service) {
            Ok(()) => false,
            Err(_) => {
                stuck.push(g.service.clone());
                true
            }
        }
    });
    stuck
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_names_carry_the_account_and_the_moment() {
        let s = service_name("9aeb9c89-316c-4344-84c5-603d71dc5c9a", 1789935600123);
        assert_eq!(
            s,
            "pitboard-park-9aeb9c89-316c-4344-84c5-603d71dc5c9a-1789935600123"
        );
        assert!(
            s.starts_with("pitboard-park-"),
            "must never collide with a Claude Code item"
        );
    }

    #[test]
    fn a_credential_with_no_refresh_token_fingerprints_to_nothing_rather_than_panicking() {
        assert_eq!(fingerprint_of(&serde_json::json!({"accessToken": "a"})), "");
    }
}
