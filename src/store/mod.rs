//! Reading and writing credentials, both Claude Code's live one and pitboard's parked ones.

mod file;
#[cfg(target_os = "macos")]
mod keychain;
#[cfg(not(target_os = "macos"))]
mod vault;

use crate::{claude, hex, slot};
use serde_json::Value;
use std::path::PathBuf;

/// Where a credential lives. `Keychain` never occurs off macOS: the platform's backend list
/// rules it out, so callers need no platform checks of their own.
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
    #[error("the credential did not survive the write: {0}")]
    NotDurable(String),
}

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Error::Unreadable(_) => "credential_store_unreadable",
            Error::Malformed(_) => "credential_not_json",
            Error::Write(_) => "credential_write_failed",
            Error::NotDurable(_) => "credential_not_durable",
        }
    }

    /// A credential that is present but unreadable as JSON means Claude Code's format
    /// moved under us, which is a different answer from a write that simply failed.
    pub fn exit_code(&self) -> u8 {
        match self {
            Error::Malformed(_) => 3,
            _ => 1,
        }
    }
}

/// One credential store. `write` must read its result back and return `Ok` only if it
/// holds exactly what was written.
pub(crate) trait RawStore: Send + Sync {
    fn kind(&self) -> Backend;
    fn contains(&self, service: &str) -> Result<bool, Error>;
    fn read(&self, service: &str) -> Result<Option<String>, Error>;
    fn write(&self, service: &str, contents: &str) -> Result<(), Error>;
    fn delete(&self, service: &str) -> Result<(), Error>;
    /// Only the keychain has a hard ceiling.
    fn too_large(&self, _service: &str, _contents: &str) -> bool {
        false
    }
}

/// Backends that may hold Claude Code's live credential, in the order it looks.
///
/// On macOS it writes the plaintext file and deletes the keychain item when a keychain
/// write fails outright, so both can be the live one at different times.
#[cfg(target_os = "macos")]
fn live_chain() -> [&'static dyn RawStore; 2] {
    [&keychain::LIVE, &file::LIVE]
}
#[cfg(not(target_os = "macos"))]
fn live_chain() -> [&'static dyn RawStore; 1] {
    [&file::LIVE]
}

/// Where pitboard's own parked credentials go. Never Claude Code's fallback file.
#[cfg(target_os = "macos")]
fn vault() -> &'static dyn RawStore {
    &keychain::VAULT
}
#[cfg(not(target_os = "macos"))]
fn vault() -> &'static dyn RawStore {
    &vault::FILE
}

pub fn credential_file() -> PathBuf {
    PathBuf::from(claude::storage_dir()).join(slot::CRED_FILE)
}

/// Which backend in `chain` holds `service`. The chain is a parameter so tests can pass
/// backends that fail on demand.
fn resolve_in<'a>(
    chain: &[&'a dyn RawStore],
    service: &str,
) -> Result<Option<&'a dyn RawStore>, Error> {
    for backend in chain {
        if backend.contains(service)? {
            return Ok(Some(*backend));
        }
    }
    Ok(None)
}

fn write_in(chain: &[&dyn RawStore], service: &str, contents: &str) -> Result<(), Error> {
    let backend = resolve_in(chain, service)?.unwrap_or(chain[0]);
    backend.write(service, contents)
}

/// Resolved on every call, never cached: Claude Code moves the credential between backends
/// when a keychain write fails, so a remembered answer goes wrong without warning.
fn resolve_backend(service: &str) -> Result<Option<&'static dyn RawStore>, Error> {
    resolve_in(&live_chain(), service)
}

pub fn resolve(service: &str) -> Result<Backend, Error> {
    Ok(resolve_backend(service)?.map_or(Backend::Absent, |b| b.kind()))
}

pub fn read_raw(service: &str) -> Result<Option<String>, Error> {
    match resolve_backend(service)? {
        Some(backend) => backend.read(service),
        None => Ok(None),
    }
}

pub fn read(service: &str) -> Result<Option<Value>, Error> {
    match read_raw(service)? {
        None => Ok(None),
        Some(raw) => serde_json::from_str(&raw)
            .map(Some)
            .map_err(|e| Error::Malformed(e.to_string())),
    }
}

/// Write the live credential where it already lives. A failed keychain write is never
/// answered by writing the plaintext file: that demotion is Claude Code's to make, and
/// making it here would move the user's token somewhere weaker without saying so.
pub fn write_raw(service: &str, contents: &str) -> Result<(), Error> {
    write_in(&live_chain(), service, contents)
}

/// The credential Claude Code keeps for a config directory: the hashed keychain slot on
/// macOS, `.credentials.json` inside it elsewhere.
pub fn read_signin(dir: &std::path::Path) -> Result<Option<String>, Error> {
    #[cfg(target_os = "macos")]
    {
        keychain::LIVE.read(&slot::service_for_dir(&dir.to_string_lossy()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        match std::fs::read_to_string(dir.join(slot::CRED_FILE)) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::Unreadable(e.to_string())),
        }
    }
}

/// This deletes an item Claude Code created, so it refuses any name that could hold a real
/// login.
pub fn discard_signin(dir: &std::path::Path) -> Result<(), Error> {
    #[cfg(target_os = "macos")]
    {
        let service = slot::service_for_dir(&dir.to_string_lossy());
        if service == slot::LIVE_SERVICE || service == claude::live_service() {
            return Err(Error::Write(format!("refusing to delete {service}")));
        }
        keychain::LIVE.delete(&service)
    }
    #[cfg(not(target_os = "macos"))]
    {
        match std::fs::remove_file(dir.join(slot::CRED_FILE)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::Write(e.to_string())),
        }
    }
}

pub fn vault_read(service: &str) -> Result<Option<String>, Error> {
    vault().read(service)
}

pub fn vault_write(service: &str, contents: &str) -> Result<(), Error> {
    vault().write(service, contents)
}

pub fn vault_delete(service: &str) -> Result<(), Error> {
    vault().delete(service)
}

pub fn too_large(service: &str, contents: &str) -> bool {
    vault().too_large(service, contents)
}

/// A handle for comparing and logging tokens without the secret leaving this process.
pub fn fingerprint(secret: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(&Sha256::digest(secret.as_bytes())[..8])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Breaking a real keychain on demand is neither safe nor deterministic.
    struct Fake {
        kind: Backend,
        stored: Mutex<HashMap<String, String>>,
        fail_write: bool,
        corrupt_readback: Option<String>,
    }

    impl Fake {
        fn empty(kind: Backend) -> Fake {
            Fake {
                kind,
                stored: Mutex::new(HashMap::new()),
                fail_write: false,
                corrupt_readback: None,
            }
        }
        fn holding(kind: Backend, service: &str, value: &str) -> Fake {
            let f = Fake::empty(kind);
            f.stored
                .lock()
                .unwrap()
                .insert(service.into(), value.into());
            f
        }
    }

    impl RawStore for Fake {
        fn kind(&self) -> Backend {
            self.kind
        }
        fn contains(&self, service: &str) -> Result<bool, Error> {
            Ok(self.stored.lock().unwrap().contains_key(service))
        }
        fn read(&self, service: &str) -> Result<Option<String>, Error> {
            Ok(self.stored.lock().unwrap().get(service).cloned())
        }
        fn write(&self, service: &str, contents: &str) -> Result<(), Error> {
            if self.fail_write {
                return Err(Error::Write("the fake was told to fail".into()));
            }
            let stored = self
                .corrupt_readback
                .clone()
                .unwrap_or_else(|| contents.to_string());
            self.stored.lock().unwrap().insert(service.into(), stored);
            match self.read(service)? {
                Some(back) if back == contents => Ok(()),
                _ => Err(Error::NotDurable(format!(
                    "{service} holds different bytes"
                ))),
            }
        }
        fn delete(&self, service: &str) -> Result<(), Error> {
            self.stored.lock().unwrap().remove(service);
            Ok(())
        }
    }

    #[test]
    fn a_failed_keychain_write_never_falls_through_to_the_plaintext_file() {
        let keychain = Fake {
            fail_write: true,
            ..Fake::holding(Backend::Keychain, "svc", "before")
        };
        let plaintext = Fake::empty(Backend::File);
        let chain: [&dyn RawStore; 2] = [&keychain, &plaintext];

        let result = write_in(&chain, "svc", "after");

        assert!(matches!(result, Err(Error::Write(_))));
        assert_eq!(
            plaintext.read("svc").unwrap(),
            None,
            "demoting the credential to a plaintext file is Claude Code's decision, not ours"
        );
        assert_eq!(keychain.read("svc").unwrap().as_deref(), Some("before"));
    }

    #[test]
    fn a_write_that_does_not_read_back_is_reported_rather_than_believed() {
        let keychain = Fake {
            corrupt_readback: Some("something else".into()),
            ..Fake::holding(Backend::Keychain, "svc", "before")
        };
        let chain: [&dyn RawStore; 1] = [&keychain];
        assert!(matches!(
            write_in(&chain, "svc", "after"),
            Err(Error::NotDurable(_))
        ));
    }

    #[test]
    fn a_credential_already_in_the_file_backend_stays_there() {
        let keychain = Fake::empty(Backend::Keychain);
        let plaintext = Fake::holding(Backend::File, "svc", "before");
        let chain: [&dyn RawStore; 2] = [&keychain, &plaintext];

        write_in(&chain, "svc", "after").unwrap();

        assert_eq!(plaintext.read("svc").unwrap().as_deref(), Some("after"));
        assert_eq!(
            keychain.read("svc").unwrap(),
            None,
            "a credential must not be promoted behind Claude Code's back either"
        );
    }

    #[test]
    fn an_absent_credential_is_written_to_the_preferred_backend() {
        let keychain = Fake::empty(Backend::Keychain);
        let plaintext = Fake::empty(Backend::File);
        let chain: [&dyn RawStore; 2] = [&keychain, &plaintext];

        write_in(&chain, "svc", "fresh").unwrap();

        assert_eq!(keychain.read("svc").unwrap().as_deref(), Some("fresh"));
        assert_eq!(plaintext.read("svc").unwrap(), None);
    }

    #[test]
    fn an_unreadable_backend_aborts_instead_of_looking_further_down_the_chain() {
        struct Broken;
        impl RawStore for Broken {
            fn kind(&self) -> Backend {
                Backend::Keychain
            }
            fn contains(&self, _: &str) -> Result<bool, Error> {
                Err(Error::Unreadable("security exited 1".into()))
            }
            fn read(&self, _: &str) -> Result<Option<String>, Error> {
                unreachable!()
            }
            fn write(&self, _: &str, _: &str) -> Result<(), Error> {
                unreachable!()
            }
            fn delete(&self, _: &str) -> Result<(), Error> {
                unreachable!()
            }
        }
        let plaintext = Fake::empty(Backend::File);
        let chain: [&dyn RawStore; 2] = [&Broken, &plaintext];

        assert!(matches!(
            write_in(&chain, "svc", "x"),
            Err(Error::Unreadable(_))
        ));
        assert_eq!(
            plaintext.read("svc").unwrap(),
            None,
            "could-not-tell must never be read as nothing-there"
        );
    }

    #[test]
    fn fingerprints_are_short_stable_and_not_the_secret() {
        let fp = fingerprint("sk-ant-example");
        assert_eq!(fp.len(), 16);
        assert_eq!(fp, fingerprint("sk-ant-example"));
        assert_ne!(fp, fingerprint("sk-ant-example2"));
        assert!(!fp.contains("sk-ant"));
    }

    #[test]
    fn the_live_chain_never_offers_a_keychain_off_macos() {
        let kinds: Vec<Backend> = live_chain().iter().map(|b| b.kind()).collect();
        if cfg!(target_os = "macos") {
            assert_eq!(kinds, vec![Backend::Keychain, Backend::File]);
        } else {
            assert_eq!(kinds, vec![Backend::File]);
        }
    }
}
