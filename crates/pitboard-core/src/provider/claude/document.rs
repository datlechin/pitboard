//! How Claude Code lays out its credential document, and which parts of it belong to the
//! account signed in rather than to the machine.
//!
//! Read out of 2.1.278 and dated in [`super::assumptions`]. The shared switch and park code
//! reach this only through the provider boundary: nothing outside Claude Code's own module
//! knows that its login sits under `claudeAiOauth`, beside keys that belong to nobody's
//! account.

use crate::api;
use crate::store;
use serde_json::{Value, json};

/// Keys that belong to the account rather than to the machine. Claude Code deletes all of
/// them along with the login on logout, so leaving one behind would hand the incoming
/// account the outgoing account's device token or its second OAuth block. Measured in
/// 2.1.278: `delete i.claudeAiOauth, delete i.organizationUuid, delete i.trustedDeviceToken,
/// delete i.enterpriseGateway, delete i.designOauth`.
pub(crate) const ACCOUNT_SCOPED: [&str; 4] = [
    "organizationUuid",
    "trustedDeviceToken",
    "enterpriseGateway",
    "designOauth",
];

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
pub(crate) fn slice(document: &Value) -> Result<Value, String> {
    let object = document
        .as_object()
        .ok_or_else(|| "it is not a JSON object".to_string())?;
    let oauth = object
        .get("claudeAiOauth")
        .ok_or_else(|| "it has no claudeAiOauth block".to_string())?;
    let mut slice = serde_json::Map::new();
    slice.insert("claudeAiOauth".into(), oauth.clone());
    for key in ACCOUNT_SCOPED {
        if let Some(value) = object.get(key) {
            slice.insert(key.into(), value.clone());
        }
    }
    Ok(Value::Object(slice))
}

/// The live document with the incoming login in place of the outgoing one, and nothing of
/// the outgoing account left behind. Claude Code makes these keys again as it needs them,
/// which is the state a logout and a fresh login would leave.
pub(crate) fn splice(before: &Value, incoming: &Value) -> Result<Value, String> {
    let mut next = before.clone();
    let document = next
        .as_object_mut()
        .ok_or_else(|| "it is not a JSON object".to_string())?;
    document.insert("claudeAiOauth".into(), oauth_in(incoming).clone());
    // The outgoing account's keys go, and the incoming account's take their place where the
    // park holds them. A park from a version that kept only the OAuth block holds none, and
    // then this is exactly what it always did.
    for key in ACCOUNT_SCOPED {
        match incoming.get(key) {
            Some(value) => document.insert(key.into(), value.clone()),
            None => document.remove(key),
        };
    }
    Ok(next)
}

/// The OAuth block inside a parked login.
///
/// A park holds the account's whole slice of Claude Code's credential document, which is
/// `claudeAiOauth` plus whatever else of [`ACCOUNT_SCOPED`] was there. A park written
/// before that held the OAuth block alone, so a document with no `claudeAiOauth` key is one
/// of those and is the block itself. Reading either shape is what lets a park from an older
/// pitboard still be restored.
pub(crate) fn oauth_in(document: &Value) -> &Value {
    document.get("claudeAiOauth").unwrap_or(document)
}

/// A non-secret handle on the refresh token, or nothing where there is none.
pub(crate) fn fingerprint_of(document: &Value) -> String {
    oauth_in(document)
        .get("refreshToken")
        .and_then(Value::as_str)
        .map(store::fingerprint)
        .unwrap_or_default()
}

/// The parked login with fresh tokens, stored as Claude Code stores its own after renewing,
/// so it reads the same to Claude Code once restored. With no refresh-token lifetime in the
/// answer Claude Code keeps the date it already had (`refreshTokenExpiresAt ?? previous`,
/// measured in 2.1.278); dropping it instead would make a lapsed park look immortal, and
/// pitboard would keep offering and renewing it forever.
pub(crate) fn renewed(document: &Value, fresh: &api::Renewed, now_millis: i64) -> Value {
    let mut next = document.clone();
    // Whatever else the slice holds is kept; only the tokens move.
    let oauth = match next.get_mut("claudeAiOauth") {
        Some(block) => block,
        None => &mut next,
    };
    let Some(fields) = oauth.as_object_mut() else {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// What gets parked is the account's whole slice, so a switch back restores what was
    /// there rather than the OAuth block and a set of missing keys.
    #[test]
    fn what_is_parked_is_everything_that_belongs_to_the_account() {
        let live = json!({
            "claudeAiOauth": {"refreshToken": "a"},
            "organizationUuid": "org-a",
            "trustedDeviceToken": "device-of-a",
            "enterpriseGateway": {"url": "https://gateway.example"},
            "designOauth": {"refreshToken": "design-of-a"},
            "mcpOAuth": {"a-server": "token"},
            "somethingOfThisMachine": true,
        });
        assert_eq!(
            slice(&live).expect("it has an oauth block"),
            json!({
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
        let before = json!({
            "claudeAiOauth": {"refreshToken": "a"},
            "organizationUuid": "org-a",
            "trustedDeviceToken": "device-of-a",
            "designOauth": {"refreshToken": "design-of-a"},
            "mcpOAuth": {"a-server": "token"},
        });
        let incoming = json!({
            "claudeAiOauth": {"refreshToken": "b"},
            "organizationUuid": "org-b",
            "trustedDeviceToken": "device-of-b",
        });
        let after = splice(&before, &incoming).expect("spliced");

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
        let before = json!({
            "claudeAiOauth": {"refreshToken": "a"},
            "organizationUuid": "org-a",
            "trustedDeviceToken": "device-of-a",
        });
        let legacy = json!({"refreshToken": "b", "accessToken": "b-access"});
        let after = splice(&before, &legacy).expect("spliced");

        assert_eq!(after["claudeAiOauth"]["refreshToken"], "b");
        assert!(after.get("organizationUuid").is_none());
        assert!(after.get("trustedDeviceToken").is_none());
    }

    #[test]
    fn a_switch_leaves_nothing_of_the_outgoing_account() {
        let before = json!({
            "claudeAiOauth": {"refreshToken": "old"},
            "organizationUuid": "org-a",
            "trustedDeviceToken": "device-of-a",
            "enterpriseGateway": {"url": "https://gateway.example"},
            "designOauth": {"refreshToken": "design-of-a"},
            "somethingOfThisMachine": true,
        });
        let after = splice(&before, &json!({"refreshToken": "new"})).expect("spliced");
        assert_eq!(after["claudeAiOauth"]["refreshToken"], "new");
        assert_eq!(after["somethingOfThisMachine"], true);
        for key in ACCOUNT_SCOPED {
            assert!(after.get(key).is_none(), "{key} was left behind");
        }
    }

    #[test]
    fn a_credential_without_claude_ai_oauth_is_refused() {
        assert!(slice(&json!({"slackTag": {}})).is_err());
        assert!(slice(&json!({"claudeAiOauth": {"accessToken": "a"}})).is_ok());
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
            at: None,
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
    fn a_credential_with_no_refresh_token_fingerprints_to_nothing_rather_than_panicking() {
        assert_eq!(fingerprint_of(&json!({"accessToken": "a"})), "");
    }
}
