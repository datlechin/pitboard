//! Where Claude Code keeps things, and who it currently thinks is signed in. Read-only.

use crate::error::{Error, Result};
use crate::slot;
use serde_json::Value;
use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// Read with `||`: an empty value means unset.
fn config_dir_env() -> Option<String> {
    std::env::var("CLAUDE_CONFIG_DIR")
        .ok()
        .filter(|v| !v.is_empty())
}

/// Read with `!== undefined`: an empty value is set.
fn secure_storage_env() -> Option<String> {
    std::env::var("CLAUDE_SECURESTORAGE_CONFIG_DIR").ok()
}

pub fn config_dir() -> PathBuf {
    config_dir_env()
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".claude"))
}

/// A legacy `<config dir>/.config.json` wins when present; otherwise
/// `<$CLAUDE_CONFIG_DIR or $HOME>/.claude.json`. The differing base is Claude Code's.
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
    storage_dir_from(
        secure_storage_env(),
        &home(),
        &config_dir().to_string_lossy(),
    )
}

fn storage_dir_from(secure: Option<String>, home: &std::path::Path, config_dir: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    match secure {
        Some(v) if v.is_empty() => home.join(".claude").to_string_lossy().nfc().collect(),
        Some(v) => v.nfc().collect(),
        None => config_dir.to_string(),
    }
}

/// Whether this process reads the unsuffixed slot. An empty
/// `CLAUDE_SECURESTORAGE_CONFIG_DIR` pins it even when `CLAUDE_CONFIG_DIR` is set.
pub fn is_default_slot() -> bool {
    is_default_slot_from(secure_storage_env().as_deref(), config_dir_env().as_deref())
}

fn is_default_slot_from(secure: Option<&str>, config_dir: Option<&str>) -> bool {
    match secure {
        Some(v) => v.is_empty(),
        None => config_dir.is_none(),
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

/// Who Claude Code's config says is signed in. A cache, refreshed about once a day, so it
/// can disagree with the credential; ask `api::owner` when it matters.
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
    fn an_empty_storage_dir_pins_the_default_slot_even_with_a_config_dir() {
        assert!(is_default_slot_from(None, None));
        assert!(!is_default_slot_from(None, Some("/cfg")));
        assert!(is_default_slot_from(Some(""), Some("/cfg")));
        assert!(!is_default_slot_from(Some("/elsewhere"), None));
    }

    /// Claude Code: `if (n !== undefined) return (n || join(homedir(), ".claude"))`. The
    /// empty string would make every credential and lock path relative to the working
    /// directory.
    #[test]
    fn an_empty_storage_dir_means_the_default_directory_not_the_current_one() {
        let home = std::path::Path::new("/home/x");
        assert_eq!(
            storage_dir_from(Some(String::new()), home, "/cfg"),
            "/home/x/.claude"
        );
        assert_eq!(
            storage_dir_from(Some("/elsewhere".into()), home, "/cfg"),
            "/elsewhere"
        );
        assert_eq!(storage_dir_from(None, home, "/cfg"), "/cfg");
    }

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
