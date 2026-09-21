//! Reading and writing credentials, both Claude Code's live one and pitboard's parked ones.

mod file;
#[cfg(target_os = "macos")]
mod keychain;
#[cfg(not(target_os = "macos"))]
mod vault;

use crate::{claude, hex, slot};
use serde_json::Value;
use std::path::PathBuf;

/// Where a credential actually lives. Reported for display; which values can occur is
/// decided by the platform's backend list, not by a runtime check in the caller.
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

/// One credential store.
///
/// `write` must verify its own result: doing it here rather than in the caller is what
/// keeps a write to one resolve plus one read-back, instead of resolving the backend a
/// second time just to check itself.
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

/// Which backend holds the live credential right now.
///
/// Resolved on every call, never cached: Claude Code migrates between backends when a
/// keychain write fails, so a remembered answer goes wrong without warning.
/// Which backend in `chain` holds `service`.
///
/// Taking the chain as an argument is the whole test seam: the shipped code passes the
/// platform's list, and a test passes fakes. Nothing else changes.
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

/// Write the live credential where it already lives.
///
/// A failed keychain write is never answered by writing the plaintext file: that demotion
/// is Claude Code's to perform, and doing it here would quietly downgrade where the user's
/// token is kept.
/// Write the live credential where it already lives.
///
/// A failed keychain write is never answered by writing the plaintext file: that demotion
/// is Claude Code's to perform, and doing it here would quietly downgrade where the user's
/// token is kept.
pub fn write_raw(service: &str, contents: &str) -> Result<(), Error> {
    write_in(&live_chain(), service, contents)
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

/// A stable handle for a token, so credentials can be compared and logged without the
/// secret leaving this process.
pub fn fingerprint(secret: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(&Sha256::digest(secret.as_bytes())[..8])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// A backend that can be told to fail, so the policies around a write can be tested
    /// without breaking a real keychain — which is neither safe nor deterministic.
    struct Fake {
        kind: Backend,
        stored: RefCell<HashMap<String, String>>,
        fail_write: bool,
        /// What a read returns after a write, when it should differ from what was written.
        corrupt_readback: Option<String>,
    }

    // The fakes are only ever used from one test thread at a time.
    unsafe impl Sync for Fake {}

    impl Fake {
        fn empty(kind: Backend) -> Fake {
            Fake {
                kind,
                stored: RefCell::new(HashMap::new()),
                fail_write: false,
                corrupt_readback: None,
            }
        }
        fn holding(kind: Backend, service: &str, value: &str) -> Fake {
            let f = Fake::empty(kind);
            f.stored.borrow_mut().insert(service.into(), value.into());
            f
        }
    }

    impl RawStore for Fake {
        fn kind(&self) -> Backend {
            self.kind
        }
        fn contains(&self, service: &str) -> Result<bool, Error> {
            Ok(self.stored.borrow().contains_key(service))
        }
        fn read(&self, service: &str) -> Result<Option<String>, Error> {
            Ok(self.stored.borrow().get(service).cloned())
        }
        fn write(&self, service: &str, contents: &str) -> Result<(), Error> {
            if self.fail_write {
                return Err(Error::Write("the fake was told to fail".into()));
            }
            let stored = self
                .corrupt_readback
                .clone()
                .unwrap_or_else(|| contents.to_string());
            self.stored.borrow_mut().insert(service.into(), stored);
            match self.read(service)? {
                Some(back) if back == contents => Ok(()),
                _ => Err(Error::NotDurable(format!(
                    "{service} holds different bytes"
                ))),
            }
        }
        fn delete(&self, service: &str) -> Result<(), Error> {
            self.stored.borrow_mut().remove(service);
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
