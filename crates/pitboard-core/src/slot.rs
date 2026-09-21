//! Credential slot naming, transcribed from Claude Code: it chooses the keychain service a
//! process sees by hashing a directory path. If Claude Code changes that, only this file
//! changes.

use crate::context::Context;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

/// The service name used when no slot is selected.
pub const LIVE_SERVICE: &str = "Claude Code-credentials";

/// The credential file used when the keychain is unavailable, and always on Linux and Windows.
pub const CRED_FILE: &str = ".credentials.json";

/// Claude Code's fallback when `$USER` is unset or contains anything unexpected.
const FALLBACK_ACCOUNT: &str = "claude-code-user";

/// The keychain account name Claude Code stores under.
pub fn account_name(ctx: &Context) -> String {
    match &ctx.user {
        Some(u) if is_accepted_account(u) => u.clone(),
        _ => FALLBACK_ACCOUNT.to_string(),
    }
}

/// Claude Code accepts `^[a-zA-Z0-9._-]+$` and falls back to a literal otherwise.
fn is_accepted_account(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// The first 8 hex characters of the SHA-256 of the NFC-normalised path string.
///
/// The path is hashed verbatim: no tilde expansion, no symlink resolution, no
/// trailing-slash normalisation. A path that differs by one byte is a different slot.
pub fn dir_hash(dir: &str) -> String {
    let normalised: String = dir.nfc().collect();
    hex::encode(&Sha256::digest(normalised.as_bytes())[..4])
}

/// The keychain service name for the slot selected by `dir`.
pub fn service_for_dir(dir: &str) -> String {
    format!("{LIVE_SERVICE}-{}", dir_hash(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These three vectors were measured on a real machine on 2026-09-21: each path was
    /// given to Claude Code, and the keychain item it created carried exactly this suffix.
    /// They are the regression test for the whole derivation.
    #[test]
    fn matches_measured_vectors() {
        let cases = [
            ("/Users/ngoquocdat/.claude", "e80beed8"),
            (
                "/private/tmp/claude-501/-Users-ngoquocdat/32ac806a-a8b6-4258-8ba1-548995f8d827/scratchpad/m0/s1",
                "a4bc5512",
            ),
            (
                "/private/tmp/claude-501/-Users-ngoquocdat/32ac806a-a8b6-4258-8ba1-548995f8d827/scratchpad/m0/s2",
                "79d45fb9",
            ),
        ];
        for (path, expected) in cases {
            assert_eq!(dir_hash(path), expected, "derivation drifted for {path}");
        }
    }

    #[test]
    fn builds_the_full_service_name() {
        assert_eq!(
            service_for_dir("/Users/ngoquocdat/.claude"),
            "Claude Code-credentials-e80beed8"
        );
    }

    #[test]
    fn a_trailing_slash_is_a_different_slot() {
        assert_ne!(dir_hash("/Users/x/.claude"), dir_hash("/Users/x/.claude/"));
    }

    #[test]
    fn account_names_are_screened_like_claude_code_screens_them() {
        for good in ["ngoquocdat", "a.b_c-1", "X"] {
            assert!(is_accepted_account(good), "{good} should be accepted");
        }
        for bad in [
            "",
            "has space",
            "quote\"",
            "sl/ash",
            "semi;colon",
            "dollar$",
        ] {
            assert!(!is_accepted_account(bad), "{bad:?} should be rejected");
        }
    }
}
