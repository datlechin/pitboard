//! Reading and writing Claude Code's credential store.

mod file;
mod keychain;

use crate::{claude, hex, slot};
use serde_json::Value;
use std::path::PathBuf;

/// Whose item is being read. Claude Code treats several `security` exit codes as
/// "absent"; for an item we are about to overwrite, only 44 may mean that. Mistaking
/// "could not tell" for "nothing there" would destroy a credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    ClaudeCode,
    Pitboard,
}

pub enum Presence {
    Present(String),
    Absent,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Keychain,
    File,
    Absent,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The store could not be interrogated. Never treat this as "no credential".
    #[error("the credential store could not be read: {0}")]
    Unreadable(String),
    #[error("the stored credential is not valid JSON: {0}")]
    Malformed(String),
    #[error("writing the credential failed: {0}")]
    Write(String),
    /// A read-back after writing did not return what was written.
    #[error("the credential did not survive the write: {0}")]
    NotDurable(String),
}

pub fn credential_file() -> PathBuf {
    PathBuf::from(claude::storage_dir()).join(slot::CRED_FILE)
}

/// Where the credential is right now.
///
/// Resolved on every call, never cached: Claude Code migrates between backends when a
/// keychain write fails, so a remembered answer goes wrong without warning.
pub fn resolve(service: &str) -> Result<Backend, Error> {
    match keychain::attributes(service) {
        Presence::Present(_) => Ok(Backend::Keychain),
        Presence::Failed(m) => Err(Error::Unreadable(m)),
        Presence::Absent => Ok(if credential_file().is_file() {
            Backend::File
        } else {
            Backend::Absent
        }),
    }
}

pub fn read_raw(service: &str, owner: Owner) -> Result<Option<String>, Error> {
    match resolve(service)? {
        Backend::Absent => Ok(None),
        Backend::Keychain => match keychain::read(service, owner) {
            Presence::Present(s) => Ok(Some(s)),
            Presence::Absent => Ok(None),
            Presence::Failed(m) => Err(Error::Unreadable(m)),
        },
        Backend::File => file::read(&credential_file()).map_err(Error::Unreadable),
    }
}

pub fn read(service: &str, owner: Owner) -> Result<Option<Value>, Error> {
    match read_raw(service, owner)? {
        None => Ok(None),
        Some(raw) => serde_json::from_str(&raw)
            .map(Some)
            .map_err(|e| Error::Malformed(e.to_string())),
    }
}

/// Write, then read back and compare.
///
/// The backend is whichever one currently holds the credential. A failed keychain write
/// is never answered by writing the plaintext file: that is Claude Code's own demotion
/// to perform, and doing it here would quietly downgrade where the user's token lives.
pub fn write_raw(service: &str, owner: Owner, contents: &str) -> Result<(), Error> {
    let backend = match resolve(service)? {
        Backend::File => Backend::File,
        _ => Backend::Keychain,
    };
    match backend {
        Backend::File => file::write(&credential_file(), contents).map_err(Error::Write)?,
        _ => keychain::write(service, contents).map_err(Error::Write)?,
    }
    match read_raw(service, owner)? {
        Some(back) if back == contents => Ok(()),
        Some(_) => Err(Error::NotDurable(format!(
            "{service} holds different bytes than were just written"
        ))),
        None => Err(Error::NotDurable(format!("{service} reads back empty"))),
    }
}

/// Write to a named keychain item, bypassing backend resolution.
///
/// Park generations are always keychain items: they are ours, and they must never
/// follow Claude Code's fallback to a plaintext file.
pub fn keychain_write(service: &str, contents: &str) -> Result<(), String> {
    keychain::write(service, contents)
}

pub fn keychain_read(service: &str) -> Result<Option<String>, Error> {
    match keychain::read(service, Owner::Pitboard) {
        Presence::Present(s) => Ok(Some(s)),
        Presence::Absent => Ok(None),
        Presence::Failed(m) => Err(Error::Unreadable(m)),
    }
}

pub fn delete(service: &str) -> Result<(), Error> {
    keychain::delete(service).map_err(Error::Write)
}

pub fn too_large(service: &str, contents: &str) -> bool {
    keychain::too_large(service, contents)
}

/// A stable handle for a token, so credentials can be compared and logged without the
/// secret leaving this process.
pub fn fingerprint(secret: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(&Sha256::digest(secret.as_bytes())[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_are_short_stable_and_not_the_secret() {
        let fp = fingerprint("sk-ant-example");
        assert_eq!(fp.len(), 16);
        assert_eq!(fp, fingerprint("sk-ant-example"));
        assert_ne!(fp, fingerprint("sk-ant-example2"));
        assert!(!fp.contains("sk-ant"));
    }
}
