//! Changing the identity recorded in Claude Code's config file.
//!
//! Two rules here are not negotiable. `oauthAccount` is replaced, never removed: Claude
//! Code refuses to write its config at all when the key is missing on disk while a
//! running process still has one cached, which wedges that session's config writes. And
//! caches are dropped by shape rather than by name, because the set of account-derived
//! keys changes between releases and a copied list is wrong in both directions.

use crate::{atomic, claude, home, time};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

const BACKUPS_KEPT: usize = 10;

/// An object keyed by identifiers already partitions its contents per account or
/// organisation, so it stays across a switch. Claude Code uses bare UUIDs, `acct:<uuid>`
/// and `bi1-<16 hex>` as such keys.
fn is_partitioned(value: &Value) -> bool {
    let Some(map) = value.as_object() else {
        return false;
    };
    !map.is_empty() && map.keys().all(|k| is_identifier(k))
}

fn is_identifier(key: &str) -> bool {
    let k = key.strip_prefix("acct:").unwrap_or(key);
    if let Some(rest) = k.strip_prefix("bi1-") {
        return rest.len() == 16 && rest.bytes().all(|b| b.is_ascii_hexdigit());
    }
    k.len() == 36
        && k.split('-').map(str::len).eq([8, 4, 4, 4, 12])
        && k.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

fn mentions(value: &Value, identifiers: &[&str]) -> bool {
    let text = value.to_string();
    identifiers
        .iter()
        .any(|id| !id.is_empty() && text.contains(id))
}

/// Whether a top-level config entry belongs to the account being switched away from.
fn belongs_to(value: &Value, outgoing: &[&str]) -> bool {
    !is_partitioned(value) && mentions(value, outgoing)
}

/// Replace the recorded identity and drop what was derived from the previous one.
///
/// `profileFetchedAt` is deliberately left out so Claude Code refetches the profile
/// instead of trusting a copy we wrote.
pub fn splice_identity(config: &mut Value, oauth_account: &Value, outgoing: &[&str]) {
    let Some(root) = config.as_object_mut() else {
        return;
    };

    let stale: Vec<String> = root
        .iter()
        .filter(|(k, v)| k.as_str() != "oauthAccount" && belongs_to(v, outgoing))
        .map(|(k, _)| k.clone())
        .collect();
    for key in stale {
        root.remove(&key);
    }

    let mut incoming = oauth_account.as_object().cloned().unwrap_or_else(Map::new);
    incoming.remove("profileFetchedAt");
    root.insert("oauthAccount".into(), Value::Object(incoming));
}

fn backups_dir() -> PathBuf {
    home::dir().join("backups")
}

/// Copy the config aside before touching it, keeping our own history.
///
/// Claude Code maintains a backup ring of its own, but it churns through it in minutes
/// under normal use, so it cannot be relied on to still hold a pre-switch copy.
pub fn backup(path: &Path) -> Result<PathBuf, String> {
    let dir = backups_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let target = dir.join(format!("claude.json.{}", time::now()));
    std::fs::copy(path, &target).map_err(|e| format!("cannot back up {}: {e}", path.display()))?;

    let mut existing: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("claude.json."))
        })
        .collect();
    existing.sort();
    for old in existing.iter().rev().skip(BACKUPS_KEPT) {
        let _ = std::fs::remove_file(old);
    }
    Ok(target)
}

/// Write through a temp file in the same directory, so a reader never sees a partial config.
/// Claude Code's file, not ours, so it keeps the permissions its owner gave it.
pub fn write(path: &Path, config: &Value) -> Result<(), String> {
    let body = serde_json::to_string(config).map_err(|e| e.to_string())?;
    atomic::write(path, body.as_bytes(), atomic::Perms::MatchExisting)
        .map_err(|e| format!("cannot replace {}: {e}", path.display()))
}

pub fn path() -> PathBuf {
    claude::config_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD_ACCOUNT: &str = "8499da91-744e-452b-ab4d-cca7cc448a36";
    const OLD_ORG: &str = "b8b291b2-2645-4cf3-a69f-d3256e996a9b";
    const NEW_ACCOUNT: &str = "9aeb9c89-316c-4344-84c5-603d71dc5c9a";

    fn config() -> Value {
        serde_json::json!({
            "oauthAccount": {"accountUuid": OLD_ACCOUNT, "emailAddress": "old@x.com",
                             "profileFetchedAt": 1789871209282i64},
            "groveConfigCache": {OLD_ACCOUNT: {"a": 1}, NEW_ACCOUNT: {"a": 2}},
            "passesEligibilityCache": {OLD_ORG: {"eligible": true}},
            "clientDataCacheSlots": {"bi1-0123456789abcdef": {"x": 1}},
            "cachedArtifactRoster": {"org": OLD_ORG, "items": []},
            "cachedUsageUtilization": {"accountUuid": OLD_ACCOUNT, "utilization": {}},
            "numStartups": 412,
            "autoUpdates": true,
            "projects": {"/Users/x/code": {"allowedTools": []}}
        })
    }

    #[test]
    fn partitioned_caches_survive_a_switch() {
        let mut c = config();
        splice_identity(
            &mut c,
            &serde_json::json!({"accountUuid": NEW_ACCOUNT}),
            &[OLD_ACCOUNT, OLD_ORG],
        );
        // Keyed by account or organisation, so both accounts' entries coexist safely.
        assert!(c.get("groveConfigCache").is_some());
        assert!(c.get("passesEligibilityCache").is_some());
        assert!(c.get("clientDataCacheSlots").is_some());
    }

    #[test]
    fn unpartitioned_caches_of_the_outgoing_account_are_dropped() {
        let mut c = config();
        splice_identity(
            &mut c,
            &serde_json::json!({"accountUuid": NEW_ACCOUNT}),
            &[OLD_ACCOUNT, OLD_ORG],
        );
        assert!(c.get("cachedArtifactRoster").is_none());
        assert!(c.get("cachedUsageUtilization").is_none());
    }

    #[test]
    fn machine_and_preference_state_is_left_alone() {
        let mut c = config();
        splice_identity(
            &mut c,
            &serde_json::json!({"accountUuid": NEW_ACCOUNT}),
            &[OLD_ACCOUNT, OLD_ORG],
        );
        assert_eq!(c["numStartups"], 412);
        assert_eq!(c["autoUpdates"], true);
        assert!(c["projects"]["/Users/x/code"].is_object());
    }

    #[test]
    fn oauth_account_is_replaced_never_removed() {
        let mut c = config();
        splice_identity(
            &mut c,
            &serde_json::json!({"accountUuid": NEW_ACCOUNT, "emailAddress": "new@x.com"}),
            &[OLD_ACCOUNT, OLD_ORG],
        );
        assert_eq!(c["oauthAccount"]["accountUuid"], NEW_ACCOUNT);
        assert_eq!(c["oauthAccount"]["emailAddress"], "new@x.com");
    }

    #[test]
    fn profile_fetched_at_is_stripped_so_claude_code_refetches() {
        let mut c = config();
        let incoming = serde_json::json!({"accountUuid": NEW_ACCOUNT, "profileFetchedAt": 999});
        splice_identity(&mut c, &incoming, &[OLD_ACCOUNT, OLD_ORG]);
        assert!(c["oauthAccount"].get("profileFetchedAt").is_none());
    }

    #[test]
    fn identifier_shapes_match_what_claude_code_actually_uses() {
        assert!(is_identifier(OLD_ACCOUNT));
        assert!(is_identifier(&format!("acct:{OLD_ACCOUNT}")));
        assert!(is_identifier("bi1-0123456789abcdef"));
        for not in [
            "numStartups",
            "bi1-short",
            "bi1-0123456789abcdeZ",
            "",
            "a-b-c-d-e",
        ] {
            assert!(
                !is_identifier(not),
                "{not:?} should not read as an identifier"
            );
        }
    }

    #[test]
    fn an_empty_object_is_not_treated_as_partitioned() {
        assert!(!is_partitioned(&serde_json::json!({})));
    }

    #[test]
    fn a_cache_naming_neither_uuid_is_kept() {
        let mut c = config();
        c["somethingNew"] = serde_json::json!({"unrelated": "value"});
        splice_identity(&mut c, &serde_json::json!({}), &[OLD_ACCOUNT, OLD_ORG]);
        assert!(c.get("somethingNew").is_some());
    }
}
