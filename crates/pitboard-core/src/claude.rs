//! Where Claude Code keeps things, and who it currently thinks is signed in. Read-only.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::slot;
use serde_json::Value;
use std::path::PathBuf;

pub fn config_dir(ctx: &Context) -> PathBuf {
    ctx.claude_config_dir
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.home.join(".claude"))
}

/// A legacy `<config dir>/.config.json` wins when present; otherwise
/// `<$CLAUDE_CONFIG_DIR or $HOME>/.claude.json`. The differing base is Claude Code's.
pub fn config_file(ctx: &Context) -> PathBuf {
    let legacy = config_dir(ctx).join(".config.json");
    if legacy.is_file() {
        return legacy;
    }
    ctx.claude_config_dir
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.home.clone())
        .join(".claude.json")
}

/// Where the `claude` that signs someone in actually is, if it is anywhere. A bare name is
/// looked up in PATH the way a shell would.
pub fn program(ctx: &Context) -> Option<PathBuf> {
    let named = &ctx.claude_program;
    if named.components().count() > 1 {
        return std::fs::metadata(named).is_ok().then(|| named.clone());
    }
    std::env::var_os("PATH")?
        .to_string_lossy()
        .split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| PathBuf::from(dir).join(named))
        .find(|candidate| std::fs::metadata(candidate).is_ok())
}

/// The directory whose path string selects the credential slot.
pub fn storage_dir(ctx: &Context) -> String {
    storage_dir_from(
        ctx.secure_storage_dir.as_deref(),
        &ctx.home,
        &config_dir(ctx).to_string_lossy(),
    )
}

fn storage_dir_from(secure: Option<&str>, home: &std::path::Path, config_dir: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    match secure {
        Some("") => home.join(".claude").to_string_lossy().nfc().collect(),
        Some(v) => v.nfc().collect(),
        None => config_dir.to_string(),
    }
}

/// Whether this process reads the unsuffixed slot. An empty
/// `CLAUDE_SECURESTORAGE_CONFIG_DIR` pins it even when `CLAUDE_CONFIG_DIR` is set.
pub fn is_default_slot(ctx: &Context) -> bool {
    is_default_slot_from(
        ctx.secure_storage_dir.as_deref(),
        ctx.claude_config_dir.as_deref(),
    )
}

fn is_default_slot_from(secure: Option<&str>, config_dir: Option<&str>) -> bool {
    match secure {
        Some(v) => v.is_empty(),
        None => config_dir.is_none(),
    }
}

/// The keychain service this process would read.
pub fn live_service(ctx: &Context) -> String {
    if is_default_slot(ctx) {
        slot::LIVE_SERVICE.to_string()
    } else {
        slot::service_for_dir(&storage_dir(ctx))
    }
}

pub fn load_config(ctx: &Context) -> Result<Value> {
    let path = config_file(ctx);
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

/// The three organization fields are written by separate fetches, not by the profile save,
/// so a config that has never made them is normal and they stay `None`.
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

    /// A bare name is looked up the way a shell looks it up, so pitboard and the person's
    /// own shell disagree about whether Claude Code is installed only if PATH differs.
    #[test]
    fn a_program_is_found_on_path_and_a_missing_one_is_not() {
        let ctx = Context::new(std::path::PathBuf::from("/home/x"));
        let found = program(&ctx.clone().with_claude_program("ls".into()))
            .expect("every machine that runs these tests has ls on PATH");
        assert!(found.ends_with("ls"), "{found:?}");
        assert!(found.is_absolute(), "a shell would get an absolute path");
        assert!(std::fs::metadata(&found).is_ok());
        assert_eq!(
            program(
                &ctx.clone()
                    .with_claude_program("no-such-program-anywhere".into())
            ),
            None
        );
        assert_eq!(
            program(&ctx.with_claude_program("/nowhere/at/all/claude".into())),
            None
        );
    }

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
        assert_eq!(storage_dir_from(Some(""), home, "/cfg"), "/home/x/.claude");
        assert_eq!(
            storage_dir_from(Some("/elsewhere"), home, "/cfg"),
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

    /// What 2.1.278 actually writes when it saves a profile: the organization fields come
    /// from other fetches and are often absent. An account is still identified.
    #[test]
    fn identity_survives_a_config_with_no_organization_fields() {
        let written = serde_json::json!({"oauthAccount": {
            "accountUuid": "u", "emailAddress": "a@b.c", "organizationUuid": "o",
            "billingType": "subscription", "seatTier": "max_20x"}});
        let id = identity(&written).expect("should parse");
        assert_eq!(id.organization_uuid, "o");
        assert_eq!(id.organization_name, None);
        assert_eq!(id.subscription, None);
    }
}
