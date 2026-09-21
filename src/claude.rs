//! Where Claude Code keeps things, and who it currently thinks you are.
//!
//! Read-only. Nothing in this module writes.

use crate::error::{Error, Result};
use crate::slot;
use serde_json::Value;
use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// `CLAUDE_CONFIG_DIR` as Claude Code reads it: with `||`, so an empty value is falsy
/// and means unset.
fn config_dir_env() -> Option<String> {
    std::env::var("CLAUDE_CONFIG_DIR")
        .ok()
        .filter(|v| !v.is_empty())
}

/// `CLAUDE_SECURESTORAGE_CONFIG_DIR` as Claude Code reads it: with `!== undefined`, so an
/// empty value is *set*, and pins the default slot while still selecting an empty storage
/// directory. The two variables genuinely differ here.
fn secure_storage_env() -> Option<String> {
    std::env::var("CLAUDE_SECURESTORAGE_CONFIG_DIR").ok()
}

pub fn config_dir() -> PathBuf {
    config_dir_env()
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".claude"))
}

/// Claude Code prefers a legacy `<config dir>/.config.json` when one exists, and
/// otherwise uses `<$CLAUDE_CONFIG_DIR or $HOME>/.claude.json`. Note the base differs
/// between the two branches — that asymmetry is Claude Code's, not a typo here.
pub fn config_file() -> PathBuf {
    let legacy = config_dir().join(".config.json");
    if legacy.is_file() {
        return legacy;
    }
    config_dir_env()
        .map(PathBuf::from)
        .unwrap_or_else(home)
        .join(".claude.json")
}

/// The directory whose path string selects the credential slot.
pub fn storage_dir() -> String {
    match secure_storage_env() {
        Some(v) => v,
        None => config_dir().to_string_lossy().into_owned(),
    }
}

/// Whether this process reads the default, unsuffixed credential slot.
///
/// An explicitly empty `CLAUDE_SECURESTORAGE_CONFIG_DIR` pins the default slot even
/// when `CLAUDE_CONFIG_DIR` is set; that asymmetry is deliberate in Claude Code.
pub fn is_default_slot() -> bool {
    match secure_storage_env() {
        Some(v) => v.is_empty(),
        None => config_dir_env().is_none(),
    }
}

/// The keychain service this process would read.
pub fn live_service() -> String {
    if is_default_slot() {
        slot::LIVE_SERVICE.to_string()
    } else {
        slot::service_for_dir(&storage_dir())
    }
}

pub fn load_config() -> Result<Value> {
    let path = config_file();
    let raw = std::fs::read_to_string(&path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            Error::ClaudeConfigMissing { path: path.clone() }
        } else {
            Error::ClaudeConfigUnreadable {
                path: path.clone(),
                source,
            }
        }
    })?;
    serde_json::from_str(&raw).map_err(|source| Error::ClaudeConfigNotJson { path, source })
}

/// Who Claude Code currently believes is signed in. This is a cache it maintains,
/// not the credential itself, so it can legitimately disagree with the store.
#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    pub email: String,
    pub account_uuid: String,
    pub organization_uuid: String,
    pub organization_name: Option<String>,
    pub subscription: Option<String>,
    pub rate_limit_tier: Option<String>,
}

pub fn identity(config: &Value) -> Option<Identity> {
    let o = config.get("oauthAccount")?.as_object()?;
    let s = |k: &str| o.get(k).and_then(Value::as_str).map(str::to_owned);
    Some(Identity {
        email: s("emailAddress")?,
        account_uuid: s("accountUuid")?,
        organization_uuid: s("organizationUuid").unwrap_or_default(),
        organization_name: s("organizationName"),
        subscription: s("organizationType"),
        rate_limit_tier: s("organizationRateLimitTier"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_needs_an_email_and_an_account() {
        let full = serde_json::json!({"oauthAccount": {
            "emailAddress": "a@b.c", "accountUuid": "u", "organizationUuid": "o",
            "organizationName": "Org", "organizationType": "claude_max",
            "organizationRateLimitTier": "default_claude_max_20x"}});
        let id = identity(&full).expect("should parse");
        assert_eq!(id.email, "a@b.c");
        assert_eq!(
            id.rate_limit_tier.as_deref(),
            Some("default_claude_max_20x")
        );

        assert!(identity(&serde_json::json!({})).is_none());
        assert!(identity(&serde_json::json!({"oauthAccount": {"accountUuid": "u"}})).is_none());
    }
}
