//! Reading Claude Code's credential store, the way Claude Code reads it.
//!
//! On macOS every call execs `/usr/bin/security`. That is not a shortcut around a
//! native API — it is the only correct choice. The keychain item's Decrypt ACL trusts
//! exactly one application, `/usr/bin/security`, and a foreign in-process read through
//! the Security framework permanently appends the caller to that ACL and its code
//! hash to the item's partition list. A poisoned partition list makes every subsequent
//! `/usr/bin/security` read of that item cost 1-3 seconds instead of 0.02, silently,
//! which Claude Code then pays on every credential re-read. Measured, not assumed.
//!
//! Read-only for now: there is no write path in this module yet.

use crate::{claude, slot};
use std::path::PathBuf;
use std::process::Command;

const SECURITY: &str = "/usr/bin/security";

/// Where the credential actually lives right now.
///
/// This is resolved on every call and never cached: Claude Code silently migrates
/// between backends when a keychain write fails, so a cached answer goes wrong without
/// warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Keychain,
    File,
    Absent,
}

#[derive(Debug)]
pub enum StoreError {
    /// The store could not be interrogated. Never treat this as "no credential".
    Unreadable(String),
    Malformed(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Unreadable(m) => write!(f, "credential store unreadable: {m}"),
            StoreError::Malformed(m) => write!(f, "credential is not valid JSON: {m}"),
        }
    }
}

/// How to read an exit status from `security`.
///
/// Claude Code's own item and items this tool owns are classified differently on
/// purpose: for our items, only 44 means absent, and anything else means stop. We must
/// never mistake "could not tell" for "nothing there" and overwrite something real.
#[derive(Clone, Copy, PartialEq)]
pub enum Owner {
    ClaudeCode,
    /// Items this tool owns. No write path exists yet, so only the tests exercise it;
    /// the classifier is written now because getting it wrong later destroys credentials.
    #[allow(dead_code)]
    Pitboard,
}

pub enum Presence {
    Present(String),
    Absent,
    Failed(String),
}

fn classify(owner: Owner, code: Option<i32>, stdout: String, stderr: String) -> Presence {
    match code {
        Some(0) if !stdout.trim().is_empty() => Presence::Present(stdout.trim_end().to_string()),
        Some(0) if owner == Owner::ClaudeCode => Presence::Absent,
        Some(44) => Presence::Absent,
        Some(36) if owner == Owner::ClaudeCode => Presence::Absent,
        other => Presence::Failed(format!(
            "security exited {}: {}",
            other
                .map(|c| c.to_string())
                .unwrap_or_else(|| "on a signal".into()),
            stderr.trim()
        )),
    }
}

fn security(args: &[&str], owner: Owner) -> Presence {
    match Command::new(SECURITY).args(args).output() {
        Err(e) => Presence::Failed(format!("cannot exec {SECURITY}: {e}")),
        Ok(out) => classify(
            owner,
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ),
    }
}

/// Item attributes only — never the secret. Safe to call freely.
pub fn keychain_attributes(service: &str) -> Presence {
    let account = slot::account_name();
    security(
        &["find-generic-password", "-a", &account, "-s", service],
        Owner::ClaudeCode,
    )
}

/// The credential document itself.
pub fn keychain_document(service: &str, owner: Owner) -> Presence {
    let account = slot::account_name();
    security(
        &["find-generic-password", "-a", &account, "-s", service, "-w"],
        owner,
    )
}

pub fn credential_file() -> PathBuf {
    PathBuf::from(claude::storage_dir()).join(slot::CRED_FILE)
}

/// Resolve which backend currently holds the credential, without reading the secret.
pub fn resolve_backend(service: &str) -> Result<Backend, StoreError> {
    match keychain_attributes(service) {
        Presence::Present(_) => Ok(Backend::Keychain),
        Presence::Failed(m) => Err(StoreError::Unreadable(m)),
        Presence::Absent => Ok(if credential_file().is_file() {
            Backend::File
        } else {
            Backend::Absent
        }),
    }
}

/// Read the whole credential document, from wherever it actually is.
pub fn read_document(service: &str, owner: Owner) -> Result<Option<serde_json::Value>, StoreError> {
    let raw = match resolve_backend(service)? {
        Backend::Absent => return Ok(None),
        Backend::Keychain => match keychain_document(service, owner) {
            Presence::Present(s) => s,
            Presence::Absent => return Ok(None),
            Presence::Failed(m) => return Err(StoreError::Unreadable(m)),
        },
        Backend::File => std::fs::read_to_string(credential_file())
            .map_err(|e| StoreError::Unreadable(e.to_string()))?,
    };
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|e| StoreError::Malformed(e.to_string()))
}

/// A stable, non-reversible handle for a token, so two credentials can be compared
/// and logged without any secret ever leaving this process.
pub fn fingerprint(secret: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(secret.as_bytes())
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_codes_absent_codes_are_absent_and_the_rest_abort() {
        for code in [0, 44, 36] {
            assert!(
                matches!(
                    classify(Owner::ClaudeCode, Some(code), String::new(), String::new()),
                    Presence::Absent
                ),
                "code {code} should read as absent for Claude Code's item"
            );
        }
        for code in [1, 37, 50, 128] {
            assert!(
                matches!(
                    classify(Owner::ClaudeCode, Some(code), String::new(), String::new()),
                    Presence::Failed(_)
                ),
                "code {code} must abort, not be mistaken for absent"
            );
        }
    }

    #[test]
    fn our_own_items_only_accept_44_as_absent() {
        assert!(matches!(
            classify(Owner::Pitboard, Some(44), String::new(), String::new()),
            Presence::Absent
        ));
        // 0-with-no-output and 36 are absence signals for Claude Code's item but not ours:
        // for an item we are about to overwrite, "could not tell" must never mean "empty".
        for code in [0, 36, 37, 50] {
            assert!(
                matches!(
                    classify(Owner::Pitboard, Some(code), String::new(), String::new()),
                    Presence::Failed(_)
                ),
                "code {code} must abort for a pitboard-owned item"
            );
        }
    }

    #[test]
    fn a_signal_is_a_failure_not_an_absence() {
        assert!(matches!(
            classify(Owner::ClaudeCode, None, String::new(), String::new()),
            Presence::Failed(_)
        ));
    }

    #[test]
    fn present_requires_actual_output() {
        assert!(matches!(
            classify(Owner::ClaudeCode, Some(0), "{}".into(), String::new()),
            Presence::Present(_)
        ));
    }

    #[test]
    fn fingerprints_are_stable_short_and_not_the_secret() {
        let fp = fingerprint("sk-ant-example");
        assert_eq!(fp.len(), 16);
        assert_eq!(fp, fingerprint("sk-ant-example"));
        assert_ne!(fp, fingerprint("sk-ant-example2"));
        assert!(!fp.contains("sk-ant"));
    }
}
