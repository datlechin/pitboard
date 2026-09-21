//! Where an account's login waits while another is signed in. Generations are append-only
//! and no failure path deletes one.

use crate::error::{Error, Result};
use crate::state::{Account, Generation, retained};
use crate::{store, time};
use serde_json::Value;

pub fn service_name(account_uuid: &str, at_millis: i64) -> String {
    format!("pitboard-park-{account_uuid}-{at_millis}")
}

/// Claim a free name before writing to it, so the caller can record it first and recovery
/// can find a park left by a run that died. Reusing a name would destroy the generation
/// already there.
pub fn reserve(account_uuid: &str) -> Result<String> {
    let start = time::now_millis();
    for offset in 0..1_000 {
        let candidate = service_name(account_uuid, start + offset);
        if store::vault_read(&candidate)?.is_none() {
            return Ok(candidate);
        }
    }
    Err(Error::ParkSlotExhausted)
}

/// Write a credential into a reserved name and prove it reads back.
pub fn store_at(service: &str, oauth: &Value) -> Result<Generation> {
    let refresh_fingerprint = fingerprint_of(oauth);
    if refresh_fingerprint.is_empty() {
        return Err(Error::LiveCredentialShapeUnexpected {
            detail: "it has no refresh token, so it could never be restored".into(),
        });
    }
    let body = serde_json::to_string(oauth).expect("an oauth block is always serialisable");
    store::vault_write(service, &body)?;
    Ok(Generation {
        service: service.to_string(),
        parked_at: time::now(),
        refresh_fingerprint,
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

/// Takes the label so a failure names the account, not an item the user has never seen.
pub fn load(label: &str, generation: &Generation) -> Result<Value> {
    let raw =
        store::vault_read(&generation.service)?.ok_or_else(|| Error::ParkedCredentialMissing {
            label: label.to_string(),
        })?;
    let value: Value = serde_json::from_str(&raw).map_err(|e| Error::ParkedCredentialCorrupt {
        label: label.to_string(),
        detail: e.to_string(),
    })?;
    if generation.refresh_fingerprint.is_empty()
        || fingerprint_of(&value) != generation.refresh_fingerprint
    {
        return Err(Error::ParkedCredentialCorrupt {
            label: label.to_string(),
            detail: "it does not match the fingerprint pitboard recorded".into(),
        });
    }
    Ok(value)
}

/// Drop generations past retention from the account and return their items, deleting
/// nothing. The caller saves state first, so no durable state ever refers to a deleted
/// item; a crash in between leaves only an item nothing refers to.
pub fn retire(account: &mut Account) -> Vec<String> {
    let keep: Vec<String> = retained(&account.generations)
        .iter()
        .map(|g| g.service.clone())
        .collect();
    let (kept, retired): (Vec<Generation>, Vec<Generation>) = account
        .generations
        .drain(..)
        .partition(|g| keep.contains(&g.service));
    account.generations = kept;
    retired.into_iter().map(|g| g.service).collect()
}

/// Delete retired items, returning the ones that resisted.
pub fn delete(services: &[String]) -> Vec<String> {
    services
        .iter()
        .filter(|s| store::vault_delete(s).is_err())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retiring_removes_old_generations_from_state_without_touching_any_item() {
        let generation = |at: i64| Generation {
            service: format!("pitboard-park-retire-{at}"),
            parked_at: at,
            refresh_fingerprint: "f".into(),
            installed_at: None,
        };
        let mut account = Account {
            label: "a".into(),
            account_uuid: "u".into(),
            email: "a@b.c".into(),
            organization_uuid: "o".into(),
            oauth_account: serde_json::json!({}),
            generations: (1..=8).map(generation).collect(),
        };
        let retired = retire(&mut account);
        assert_eq!(account.generations.len(), 5, "the five newest stay");
        assert_eq!(retired.len(), 3);
        assert!(
            retired.iter().all(|s| !account.references(s)),
            "a retired item must no longer be referenced"
        );
    }

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

    /// Its fingerprint would be the empty string, which every later check would match.
    #[test]
    fn a_credential_with_no_refresh_token_is_refused_rather_than_parked() {
        let refused = store_at(
            "pitboard-park-test-no-refresh",
            &serde_json::json!({"accessToken": "a"}),
        );
        assert!(refused.is_err());
    }

    #[test]
    fn a_credential_with_no_refresh_token_fingerprints_to_nothing_rather_than_panicking() {
        assert_eq!(fingerprint_of(&serde_json::json!({"accessToken": "a"})), "");
    }
}
