//! Reading and writing credentials, both Claude Code's live one and pitboard's parked ones.

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
    /// Keychain items another program owns, kept under `account`. `None` on a machine with
    /// no keychain, where a tool keeps its login in a file instead.
    ///
    /// Which items and which account is the other program's business, so both are handed
    /// in. Deriving them here is how Claude Code's slot hashing came to live inside what
    /// claimed to be an operating-system abstraction.
    fn foreign_keychain(&self, ctx: &Context, account: &str) -> Option<Box<dyn RawStore>>;

    /// The single file at `path`, as a store.
    fn file(&self, path: PathBuf) -> Box<dyn RawStore>;

    /// Where pitboard's own parked logins go: the keychain where there is one, a private
    /// directory of files where there is not. This one really is a fact about the machine.
    fn vault(&self, ctx: &Context) -> Box<dyn RawStore>;

    /// How many processes are running `program` on this machine, where that can be told.
    fn running(&self, program: &str) -> Option<usize> {
        crate::process::running(program)
    }
}

/// The keychain account pitboard stores its own items under.
///
/// It is Claude Code's derivation, and it stays Claude Code's derivation, because every
/// park already on every machine is filed under whatever this returned the day it was
/// written. Changing it would not move those items; it would make them unfindable, which
/// is the same as deleting every parked login on upgrade.
///
/// macOS only: a keychain item is filed under an account, and a file is not.
#[cfg(target_os = "macos")]
pub(crate) fn vault_account(ctx: &Context) -> String {
    crate::provider::claude::slot::account_name(ctx)
}

/// macOS: a keychain, and files.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy)]
pub(crate) struct MacOs;

#[cfg(target_os = "macos")]
impl Host for MacOs {
    fn foreign_keychain(&self, ctx: &Context, account: &str) -> Option<Box<dyn RawStore>> {
        Some(Box::new(keychain::Keychain::foreign(
            ctx,
            account.to_string(),
        )))
    }

    fn file(&self, path: PathBuf) -> Box<dyn RawStore> {
        Box::new(file::PlainFile::at(path))
    }

    fn vault(&self, ctx: &Context) -> Box<dyn RawStore> {
        Box::new(keychain::Keychain::vault(ctx))
    }
}

/// Everywhere else: files, and pitboard's own file vault.
#[cfg(not(target_os = "macos"))]
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlainUnix;

#[cfg(not(target_os = "macos"))]
impl Host for PlainUnix {
    fn foreign_keychain(&self, _ctx: &Context, _account: &str) -> Option<Box<dyn RawStore>> {
        None
    }

    fn file(&self, path: PathBuf) -> Box<dyn RawStore> {
        Box::new(file::PlainFile::at(path))
    }

    fn vault(&self, ctx: &Context) -> Box<dyn RawStore> {
        Box::new(vault::FileVault::new(ctx))
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

/// Where parked logins live when there is no keychain to put them in: one file each, in
/// pitboard's own directory. Named here rather than in the backend so `doctor` can look at
/// what is actually on the disk without the two spellings drifting apart.
pub fn vault_dir(ctx: &Context) -> PathBuf {
    crate::home::dir(ctx).join("vault")
}

/// One tool's live credential chain, in the order that tool reads it.
///
/// Built by that tool's own module. Which backends can hold a login, and in what order, is
/// a fact about the tool: Claude Code looks in the keychain and then in a plaintext file it
/// demotes to, while Codex and Gemini have one file and nothing behind it.
pub struct Live(Vec<Box<dyn RawStore>>);

impl Live {
    pub(crate) fn of(backends: Vec<Box<dyn RawStore>>) -> Live {
        assert!(
            !backends.is_empty(),
            "a live chain with no backends can hold nothing"
        );
        Live(backends)
    }

    fn refs(&self) -> Vec<&dyn RawStore> {
        self.0.iter().map(Box::as_ref).collect()
    }
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

fn with_live<T>(live: &Live, run: impl FnOnce(&[&dyn RawStore]) -> T) -> T {
    run(&live.refs())
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
pub fn resolve(live: &Live, service: &str) -> Result<Backend, Error> {
    with_live(live, |chain| {
        Ok(resolve_in(chain, service)?.map_or(Backend::Absent, |b| b.kind()))
    })
}

pub fn read_raw(live: &Live, service: &str) -> Result<Option<String>, Error> {
    with_live(live, |chain| match resolve_in(chain, service)? {
        Some(backend) => backend.read(service),
        None => Ok(None),
    })
}

pub fn read(live: &Live, service: &str) -> Result<Option<Value>, Error> {
    match read_raw(live, service)? {
        None => Ok(None),
        Some(raw) => serde_json::from_str(&raw)
            .map(Some)
            .map_err(|e| Error::Malformed(e.to_string())),
    }
}

/// Write the live credential where it already lives. A failed keychain write is never
/// answered by writing the plaintext file: that demotion is Claude Code's to make, and
/// making it here would move the user's token somewhere weaker without saying so.
pub fn write_raw(live: &Live, service: &str, contents: &str) -> Result<(), Error> {
    with_live(live, |chain| write_in(chain, service, contents))
}

pub fn vault_read(ctx: &Context, service: &str) -> Result<Option<String>, Error> {
    vault(ctx).read(service)
}

pub fn vault_write(ctx: &Context, service: &str, contents: &str) -> Result<(), Error> {
    vault(ctx).write(service, contents)
}

/// What writing `contents` into the vault would cost against its ceiling, where it has one.
pub fn vault_cost(ctx: &Context, service: &str, contents: &str) -> Option<Cost> {
    vault(ctx).cost(service, contents)
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
pub fn cost(live: &Live, service: &str, contents: &str) -> Option<Cost> {
    with_live(live, |chain| {
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

    /// A machine without a keychain must say so rather than hand back something that
    /// behaves like one. A tool's own module builds its chain out of this answer, so a
    /// host that always offered a keychain would build a chain that cannot work.
    #[test]
    fn a_keychain_is_offered_only_where_there_is_one() {
        let ctx = Context::from_env();
        let offered = ctx.host().foreign_keychain(&ctx, "someone");
        assert_eq!(
            offered.map(|k| k.kind()),
            cfg!(target_os = "macos").then_some(Backend::Keychain)
        );
        assert_eq!(
            ctx.host().file(PathBuf::from("/nowhere/at/all")).kind(),
            Backend::File
        );
    }
}
