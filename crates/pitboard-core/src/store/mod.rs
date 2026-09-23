//! Reading and writing credentials, both Claude Code's live one and pitboard's parked ones.

use crate::provider::claude::paths as claude;
use crate::provider::claude::slot;
mod file;
#[cfg(target_os = "macos")]
mod keychain;
#[cfg(any(test, feature = "test-support"))]
pub mod memory;
#[cfg(not(target_os = "macos"))]
mod vault;

use crate::context::Context;

use serde_json::Value;
use std::path::PathBuf;

/// The only program trusted to read Claude Code's keychain item. Named here so the backend
/// that runs it and the doctor check that looks for it cannot drift apart.
pub(crate) const SECURITY: &str = "/usr/bin/security";

/// Where a credential lives. `Keychain` never occurs off macOS: the platform's backend list
/// rules it out, so callers need no platform checks of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Backend {
    Keychain,
    File,
    Absent,
}

impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Backend::Keychain => "keychain",
            Backend::File => "file",
            Backend::Absent => "absent",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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

/// What writing a document costs a store that has a ceiling, and what happens above it.
///
/// One value rather than three questions. Asking separately meant hex-encoding the same
/// login three times per switch, and meant every caller knowing that only one platform has
/// a ceiling at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cost {
    /// Bytes this document needs of the store's cheapest route.
    pub needs: usize,
    /// Bytes that route has.
    pub limit: usize,
    /// Whether the store has another route above the ceiling, and is allowed to use it.
    pub second_route: bool,
}

impl Cost {
    /// Past the cheapest route.
    pub fn over(self) -> bool {
        self.needs > self.limit
    }

    /// Past it, and written the more visible way rather than refused.
    pub fn on_the_second_route(self) -> bool {
        self.over() && self.second_route
    }

    /// Past it with nothing left to try.
    pub fn refused(self) -> bool {
        self.over() && !self.second_route
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

    /// Every name pitboard put here, where the store can be asked. `None` where it cannot,
    /// which is what a store with no way to enumerate answers rather than an empty list:
    /// nothing found and nothing askable are different, and one of them means an item can
    /// be lost track of for good.
    fn list(&self) -> Result<Option<Vec<String>>, Error> {
        Ok(None)
    }

    /// What a write would cost against this store's ceiling. `None` where there is none,
    /// which is every store but the keychain.
    fn cost(&self, _service: &str, _contents: &str) -> Option<Cost> {
        None
    }
}

/// The machine pitboard is standing on, as one value rather than a set of `cfg` branches
/// spread through the module. A host answers where the live credential may be, where
/// pitboard's own parked ones go, and how a private sign-in's credential is read and
/// discarded. It takes the context on every call because a context is built by a builder
/// and can still change after it exists.
///
/// It was called `Platform` until a second provider was on the way. The name said "which
/// operating system", the body reached into Claude Code's own slot hashing, and once
/// "provider" became a word this codebase uses, a reader meeting `Platform` could not tell
/// which of the two axes it meant. The Claude Code half is on its way out of here; what
/// stays behind this name is the machine, and only the machine.
pub(crate) trait Host: Send + Sync + std::fmt::Debug {
    /// Backends that may hold Claude Code's live credential, in the order it looks.
    fn live_chain(&self, ctx: &Context) -> Vec<Box<dyn RawStore>>;

    /// Where pitboard's own parked credentials go. Never Claude Code's fallback file.
    fn vault(&self, ctx: &Context) -> Box<dyn RawStore>;

    /// The credential Claude Code keeps for a config directory, during a private sign-in.
    fn read_signin(&self, ctx: &Context, dir: &std::path::Path) -> Result<Option<String>, Error>;

    /// Deletes an item Claude Code created, so it refuses any name that could hold a real
    /// login.
    fn discard_signin(&self, ctx: &Context, dir: &std::path::Path) -> Result<(), Error>;
}

/// macOS: the keychain, with Claude Code's plaintext file behind it.
///
/// Claude Code writes that file and deletes the keychain item when a keychain write fails
/// outright, so both can be the live one at different times.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy)]
pub(crate) struct MacOs;

#[cfg(target_os = "macos")]
impl Host for MacOs {
    fn live_chain(&self, ctx: &Context) -> Vec<Box<dyn RawStore>> {
        vec![
            Box::new(keychain::Keychain::live(ctx)),
            Box::new(file::PlainFile::live(ctx)),
        ]
    }

    fn vault(&self, ctx: &Context) -> Box<dyn RawStore> {
        Box::new(keychain::Keychain::vault(ctx))
    }

    fn read_signin(&self, ctx: &Context, dir: &std::path::Path) -> Result<Option<String>, Error> {
        keychain::Keychain::live(ctx).read(&slot::service_for_dir(&dir.to_string_lossy()))
    }

    fn discard_signin(&self, ctx: &Context, dir: &std::path::Path) -> Result<(), Error> {
        let service = slot::service_for_dir(&dir.to_string_lossy());
        if service == slot::LIVE_SERVICE || service == claude::live_service(ctx) {
            return Err(Error::Write(format!("refusing to delete {service}")));
        }
        keychain::Keychain::live(ctx).delete(&service)
    }
}

/// Everywhere else: one plaintext file per storage directory, and pitboard's own file vault.
#[cfg(not(target_os = "macos"))]
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlainUnix;

#[cfg(not(target_os = "macos"))]
impl Host for PlainUnix {
    fn live_chain(&self, ctx: &Context) -> Vec<Box<dyn RawStore>> {
        vec![Box::new(file::PlainFile::live(ctx))]
    }

    fn vault(&self, ctx: &Context) -> Box<dyn RawStore> {
        Box::new(vault::FileVault::new(ctx))
    }

    fn read_signin(&self, _ctx: &Context, dir: &std::path::Path) -> Result<Option<String>, Error> {
        match std::fs::read_to_string(dir.join(slot::CRED_FILE)) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::Unreadable(e.to_string())),
        }
    }

    fn discard_signin(&self, _ctx: &Context, dir: &std::path::Path) -> Result<(), Error> {
        match std::fs::remove_file(dir.join(slot::CRED_FILE)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::Write(e.to_string())),
        }
    }
}

/// The host this build is standing on, which is what every real context uses.
pub(crate) fn host() -> std::sync::Arc<dyn Host> {
    #[cfg(target_os = "macos")]
    {
        std::sync::Arc::new(MacOs)
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::sync::Arc::new(PlainUnix)
    }
}

fn vault(ctx: &Context) -> Box<dyn RawStore> {
    ctx.host().vault(ctx)
}

/// Only "not found" means absent. A permission error or a loop in the path says nothing about
/// what is there, and reading it as empty would tell the user to sign in again.
fn exists(path: &std::path::Path) -> Result<bool, Error> {
    path.try_exists()
        .map_err(|e| Error::Unreadable(format!("cannot look for {}: {e}", path.display())))
}

pub fn credential_file(ctx: &Context) -> PathBuf {
    PathBuf::from(claude::storage_dir(ctx)).join(slot::CRED_FILE)
}

/// Where parked logins live when there is no keychain to put them in: one file each, in
/// pitboard's own directory. Named here rather than in the backend so `doctor` can look at
/// what is actually on the disk without the two spellings drifting apart.
pub fn vault_dir(ctx: &Context) -> PathBuf {
    crate::home::dir(ctx).join("vault")
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
fn with_live<T>(ctx: &Context, run: impl FnOnce(&[&dyn RawStore]) -> T) -> T {
    let owned = ctx.host().live_chain(ctx);
    let chain: Vec<&dyn RawStore> = owned.iter().map(Box::as_ref).collect();
    run(&chain)
}

/// Which backend holds a credential, or `Absent`.
///
/// Settled in 2.1.278, which the earlier note here left open. Claude Code builds its live
/// chain as keychain-with-plaintext-fallback and the successor backend ("storageV5", gated
/// on `tengu_hover_rest`) does not change that: it replaces what backs the fallback half,
/// and only for a caller that hands a backend in. An ordinary `claude` hands none in, so
/// the fallback stays `<storage dir>/.credentials.json` and the keychain stays first. The
/// order below is therefore the order Claude Code reads in, not a guess.
///
/// One divergence, on purpose. Claude Code demotes to the plaintext file when a keychain
/// write fails for good, and deletes the keychain item when it does. pitboard never does:
/// see the note on `write_in`.
pub fn resolve(ctx: &Context, service: &str) -> Result<Backend, Error> {
    with_live(ctx, |chain| {
        Ok(resolve_in(chain, service)?.map_or(Backend::Absent, |b| b.kind()))
    })
}

pub fn read_raw(ctx: &Context, service: &str) -> Result<Option<String>, Error> {
    with_live(ctx, |chain| match resolve_in(chain, service)? {
        Some(backend) => backend.read(service),
        None => Ok(None),
    })
}

pub fn read(ctx: &Context, service: &str) -> Result<Option<Value>, Error> {
    match read_raw(ctx, service)? {
        None => Ok(None),
        Some(raw) => serde_json::from_str(&raw)
            .map(Some)
            .map_err(|e| Error::Malformed(e.to_string())),
    }
}

/// Write the live credential where it already lives. A failed keychain write is never
/// answered by writing the plaintext file: that demotion is Claude Code's to make, and
/// making it here would move the user's token somewhere weaker without saying so.
pub fn write_raw(ctx: &Context, service: &str, contents: &str) -> Result<(), Error> {
    with_live(ctx, |chain| write_in(chain, service, contents))
}

/// The credential Claude Code keeps for a config directory: the hashed keychain slot on
/// macOS, `.credentials.json` inside it elsewhere.
pub fn read_signin(ctx: &Context, dir: &std::path::Path) -> Result<Option<String>, Error> {
    ctx.host().read_signin(ctx, dir)
}

/// This deletes an item Claude Code created, so it refuses any name that could hold a real
/// login.
pub fn discard_signin(ctx: &Context, dir: &std::path::Path) -> Result<(), Error> {
    ctx.host().discard_signin(ctx, dir)
}

pub fn vault_read(ctx: &Context, service: &str) -> Result<Option<String>, Error> {
    vault(ctx).read(service)
}

pub fn vault_write(ctx: &Context, service: &str, contents: &str) -> Result<(), Error> {
    vault(ctx).write(service, contents)
}

pub fn vault_delete(ctx: &Context, service: &str) -> Result<(), Error> {
    vault(ctx).delete(service)
}

/// Every parked login on this machine, asked of the store rather than read out of
/// pitboard's own index. `None` where the store cannot be enumerated.
pub fn vault_list(ctx: &Context) -> Result<Option<Vec<String>>, Error> {
    vault(ctx).list()
}

/// What writing the live credential would cost, asked of the backend that would take the
/// write. A login living in the fallback file has no ceiling, and used to be told it had
/// the keychain's.
pub fn cost(ctx: &Context, service: &str, contents: &str) -> Option<Cost> {
    with_live(ctx, |chain| {
        resolve_in(chain, service)
            .ok()
            .flatten()
            .unwrap_or(chain[0])
            .cost(service, contents)
    })
}

/// A handle for comparing and logging tokens without the secret leaving this process.
pub fn fingerprint(secret: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(&Sha256::digest(secret.as_bytes())[..8])
}

#[cfg(test)]
mod tests {
    use super::memory::{Fault, MemoryStore};
    use super::*;
    use std::sync::Arc;

    /// Breaking a real keychain on demand is neither safe nor deterministic, so the chain
    /// rules are proved against stores that fail when they are told to. The same stores are
    /// what the engine's own tests drive, through `Context::with_memory_stores`.
    fn store(kind: Backend) -> Arc<MemoryStore> {
        MemoryStore::of(kind)
    }

    fn holding(kind: Backend, service: &str, value: &str) -> Arc<MemoryStore> {
        let s = store(kind);
        s.plant(service, value);
        s
    }

    #[test]
    fn a_failed_keychain_write_never_falls_through_to_the_plaintext_file() {
        let keychain = holding(Backend::Keychain, "svc", "before");
        keychain.fault("svc", Fault::FailWrite("told to".into()));
        let plaintext = store(Backend::File);
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
        let keychain = holding(Backend::Keychain, "svc", "before");
        keychain.fault("svc", Fault::CorruptWrite("something else".into()));
        let chain: [&dyn RawStore; 1] = [&keychain];
        assert!(matches!(
            write_in(&chain, "svc", "after"),
            Err(Error::NotDurable(_))
        ));
    }

    #[test]
    fn a_credential_already_in_the_file_backend_stays_there() {
        let keychain = store(Backend::Keychain);
        let plaintext = holding(Backend::File, "svc", "before");
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
        let keychain = store(Backend::Keychain);
        let plaintext = store(Backend::File);
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
        let plaintext = store(Backend::File);
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
        let ctx = Context::from_env();
        let kinds: Vec<Backend> = ctx
            .host()
            .live_chain(&ctx)
            .iter()
            .map(|b| b.kind())
            .collect();
        if cfg!(target_os = "macos") {
            assert_eq!(kinds, vec![Backend::Keychain, Backend::File]);
        } else {
            assert_eq!(kinds, vec![Backend::File]);
        }
    }
}
