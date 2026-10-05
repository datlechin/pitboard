//! The shapes Claude Desktop's support passes between its parts: what a cookie jar holds,
//! whose a data folder is, and whether live usage may read Claude's key.
//!
//! Declared here, ahead of the code that fills them, so the parts that read the jar, move
//! the folder and ask claude.ai can be written against one set of types.

// Filled and read by the tree engine and live usage, which this file is the boundary for.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// What Pitboard reads of Chromium's `Cookies` database: its schema version and the rows
/// for claude.ai that say who is signed in.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CookieTable {
    /// `meta.version`. 24 on the build the register names.
    pub meta_version: u32,
    pub rows: Vec<CookieRow>,
}

/// One cookie as it is stored: still encrypted, never decrypted to be stored elsewhere.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CookieRow {
    pub host_key: String,
    pub name: String,
    /// The session's ciphertext, which is printed nowhere, not even by `Debug`.
    pub encrypted_value: Vec<u8>,
    /// Microseconds since 1601, Chromium's own epoch.
    pub expires_utc: i64,
}

/// Says how long the value is and nothing of what it is, so a failed assertion or a debug
/// log never carries a session's ciphertext.
impl std::fmt::Debug for CookieRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CookieRow")
            .field("host_key", &self.host_key)
            .field("name", &self.name)
            .field(
                "encrypted_value",
                &format_args!("<{} bytes>", self.encrypted_value.len()),
            )
            .field("expires_utc", &self.expires_utc)
            .finish()
    }
}

/// Whose login a data folder holds, and a handle on the session that proves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TreeIdentity {
    /// `lastKnownAccountUuid` from the folder's `config.json`.
    pub account_uuid: String,
    /// The SHA-256 of the session cookie's ciphertext, in hex. Never the cookie itself.
    pub fingerprint: String,
    /// When the session cookie expires, in epoch seconds.
    pub expires_at: Option<i64>,
}

/// The keychain's own stamp on Claude's key: when the item was made and last changed. Read
/// without the password, so reading it never asks anybody anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStamp {
    pub cdat: String,
    pub mdat: String,
}

/// Whether macOS lets Pitboard read Claude's key.
// `NeedsApproval` is the name the design and its wire code `needs_approval` share.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Approval {
    Unknown,
    Granted,
    NeedsApproval,
}

/// Whether Claude Desktop's usage is asked of claude.ai, which needs Claude's key, or read
/// only from what the app itself wrote down. Off until somebody turns it on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveUsage {
    pub enabled: bool,
    pub approval: Approval,
    /// Why it is not working, as a stable code: `no_gui`, `denied`, `auth_failed`,
    /// `timed_out`, `item_changed`, `item_missing`, `key_does_not_decrypt`,
    /// `no_session`, `other`.
    pub reason: Option<String>,
    pub changed_at: Option<i64>,
    pub last_ok_at: Option<i64>,
    /// The key's stamp when it was granted, so a key made again is asked about again.
    pub stamp: Option<ItemStamp>,
}

impl Default for LiveUsage {
    fn default() -> LiveUsage {
        LiveUsage {
            enabled: false,
            approval: Approval::Unknown,
            reason: None,
            changed_at: None,
            last_ok_at: None,
            stamp: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cookie's ciphertext is a secret's, and printing a row never shows it.
    #[test]
    fn a_cookie_row_never_prints_its_value() {
        let row = CookieRow {
            host_key: ".claude.ai".into(),
            name: "sessionKey".into(),
            encrypted_value: b"v10SECRETCIPHERTEXT".to_vec(),
            expires_utc: 1,
        };
        let table = CookieTable {
            meta_version: 24,
            rows: vec![row.clone()],
        };
        for printed in [
            format!("{row:?}"),
            format!("{table:?}"),
            format!("{row:#?}"),
        ] {
            assert!(printed.contains("sessionKey"), "{printed}");
            assert!(!printed.contains("SECRET"), "{printed}");
            // Nor as the numbers of its bytes, which is how a Vec<u8> prints.
            assert!(!printed.contains("83, 69, 67"), "{printed}");
            assert!(printed.contains("19 bytes"), "{printed}");
        }
    }

    /// Live usage starts off, and says so in the codes a front end branches on.
    #[test]
    fn live_usage_starts_off() {
        let off = LiveUsage::default();
        assert!(!off.enabled);
        assert_eq!(off.approval, Approval::Unknown);
        let json = serde_json::to_value(&off).unwrap();
        assert_eq!(json["approval"], "unknown");
        assert_eq!(
            serde_json::to_value(Approval::NeedsApproval).unwrap(),
            "needs_approval"
        );
    }
}
