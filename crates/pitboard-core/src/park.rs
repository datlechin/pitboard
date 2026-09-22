//! Where an account's login waits while another is signed in. Nothing here decides what to
//! delete: a park no account refers to is listed in `State::discarded` and purged from there.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::state::{Park, State};
use crate::{api, store};
use serde_json::{Value, json};

pub fn service_name(account_uuid: &str, at_millis: i64) -> String {
    format!("pitboard-park-{account_uuid}-{at_millis}")
}

/// Claim a free name before writing to it, so the caller can record it first and recovery
/// can find a park left by a run that died. Reusing a name would destroy the park there.
pub fn reserve(ctx: &Context, account_uuid: &str) -> Result<String> {
    let start = ctx.now_millis();
    for offset in 0..1_000 {
        let candidate = service_name(account_uuid, start + offset);
        if store::vault_read(ctx, &candidate)?.is_none() {
            return Ok(candidate);
        }
    }
    Err(Error::ParkSlotExhausted)
}

/// Write a login into a reserved name and prove it reads back.
pub fn store_at(ctx: &Context, service: &str, oauth: &Value) -> Result<Park> {
    let park = describe(service, ctx.now(), oauth);
    if park.refresh_fingerprint.is_empty() {
        return Err(Error::LiveCredentialShapeUnexpected {
            detail: "it has no refresh token, so it could never be restored".into(),
        });
    }
    let body = serde_json::to_string(oauth).expect("an oauth block is always serialisable");
    store::vault_write(ctx, service, &body)?;
    Ok(park)
}

/// What the account index records about a login: nothing secret.
pub fn describe(service: &str, parked_at: i64, oauth: &Value) -> Park {
    // Claude Code records both expiries in epoch milliseconds.
    let expiry = |key: &str| oauth.get(key).and_then(Value::as_i64).map(|ms| ms / 1000);
    Park {
        service: service.to_string(),
        parked_at,
        refresh_fingerprint: fingerprint_of(oauth),
        access_expires_at: expiry("expiresAt"),
        refresh_expires_at: expiry("refreshTokenExpiresAt"),
    }
}

/// The parked login with fresh tokens, stored as Claude Code stores its own after renewing,
/// so it reads the same to Claude Code once restored. With no refresh-token lifetime in the
/// answer Claude Code keeps the date it already had (`refreshTokenExpiresAt ?? previous`,
/// measured in 2.1.278); dropping it instead would make a lapsed park look immortal, and
/// pitboard would keep offering and renewing it forever.
pub fn renewed(oauth: &Value, fresh: &api::Renewed, now_millis: i64) -> Value {
    let mut next = oauth.clone();
    let Some(fields) = next.as_object_mut() else {
        return next;
    };
    fields.insert("accessToken".into(), json!(fresh.access_token));
    if let Some(refresh) = &fresh.refresh_token {
        fields.insert("refreshToken".into(), json!(refresh));
    }
    fields.insert(
        "expiresAt".into(),
        json!(now_millis + fresh.expires_in * 1000),
    );
    if let Some(seconds) = fresh.refresh_token_expires_in {
        fields.insert(
            "refreshTokenExpiresAt".into(),
            json!(now_millis + seconds * 1000),
        );
    }
    if let Some(scopes) = &fresh.scopes {
        fields.insert("scopes".into(), json!(scopes));
    }
    next
}

pub fn fingerprint_of(oauth: &Value) -> String {
    oauth
        .get("refreshToken")
        .and_then(Value::as_str)
        .map(store::fingerprint)
        .unwrap_or_default()
}

/// Takes the label so a failure names the account, not an item the user has never seen.
pub fn load(ctx: &Context, label: &str, park: &Park) -> Result<Value> {
    let raw =
        store::vault_read(ctx, &park.service)?.ok_or_else(|| Error::ParkedCredentialMissing {
            label: label.to_string(),
        })?;
    let value: Value = serde_json::from_str(&raw).map_err(|e| Error::ParkedCredentialCorrupt {
        label: label.to_string(),
        detail: e.to_string(),
    })?;
    if park.refresh_fingerprint.is_empty() || fingerprint_of(&value) != park.refresh_fingerprint {
        return Err(Error::ParkedCredentialCorrupt {
            label: label.to_string(),
            detail: "it does not match the fingerprint pitboard recorded".into(),
        });
    }
    Ok(value)
}

/// Delete every discarded item, keeping listed only those that resisted. Returns how many
/// remain.
pub fn purge(ctx: &Context, state: &mut State) -> usize {
    state
        .discarded
        .retain(|service| store::vault_delete(ctx, service).is_err());
    state.discarded.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_park_records_when_its_login_stops_working() {
        let park = describe(
            "pitboard-park-x-1",
            50,
            &serde_json::json!({
                "refreshToken": "r",
                "expiresAt": 1_790_000_000_123i64,
                "refreshTokenExpiresAt": 1_792_000_000_999i64
            }),
        );
        assert_eq!(park.access_expires_at, Some(1_790_000_000));
        assert_eq!(park.refresh_expires_at, Some(1_792_000_000));
        assert_eq!(
            park.refresh_fingerprint,
            fingerprint_of(&serde_json::json!({"refreshToken": "r"}))
        );
    }

    #[test]
    fn a_renewed_login_is_stored_as_claude_code_stores_its_own() {
        let parked = json!({
            "accessToken": "a1", "refreshToken": "r1", "expiresAt": 1,
            "refreshTokenExpiresAt": 2, "scopes": ["user:inference"],
            "subscriptionType": "max", "rateLimitTier": "default_claude_max_20x"
        });
        let fresh = api::Renewed {
            access_token: "a2".into(),
            refresh_token: Some("r2".into()),
            expires_in: 60,
            refresh_token_expires_in: Some(120),
            scopes: None,
        };
        let next = renewed(&parked, &fresh, 1_000_000);
        assert_eq!(next["accessToken"], "a2");
        assert_eq!(next["refreshToken"], "r2");
        assert_eq!(next["expiresAt"], 1_060_000);
        assert_eq!(next["refreshTokenExpiresAt"], 1_120_000);
        assert_eq!(
            next["scopes"],
            json!(["user:inference"]),
            "kept when not answered"
        );
        assert_eq!(
            next["subscriptionType"], "max",
            "what renewal does not touch stays"
        );

        let kept = api::Renewed {
            refresh_token: None,
            refresh_token_expires_in: None,
            ..fresh
        };
        let next = renewed(&parked, &kept, 1_000_000);
        assert_eq!(
            next["refreshToken"], "r1",
            "the server kept the refresh token"
        );
        // Claude Code keeps the date it had. Dropping it would make a park that is about to
        // lapse look as though it never expires.
        assert_eq!(next["refreshTokenExpiresAt"], 2);
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
            &Context::from_env(),
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
