//! Reading and writing credentials, both a tool's live one and Pitboard's parked ones.
//!
//! Which store holds what is decided by the tool and by the machine: a tool's own module
//! builds its live chain, and [`crate::host::Host`] says what this machine has. What is
//! here holds on every machine: the chain rules, and the stores that are only files.

mod file;
#[cfg(any(test, feature = "test-support"))]
pub mod memory;
// macOS parks in the login keychain, so there nothing outside the tests opens a vault of
// files; Linux parks in one. Windows parks nowhere until W20 seals each park to the person,
// in files of its own kind, so nothing opens this vault there either.
#[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
pub(crate) mod vault;

pub(crate) use file::PlainFile;

use crate::context::Context;
use crate::service::Permit;

use serde_json::Value;
use std::path::PathBuf;

/// Where a credential lives. `Keychain` occurs only where the machine has one: its host
/// offers no other, so callers need no checks of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Backend {
    Keychain,
    File,
    Absent,
    /// A store nobody can say anything of yet, since the part of Pitboard that would read it
    /// is not written: on Windows, until each of its stores is. Kept, with its code, once
    /// they are.
    Unknown,
}

impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Backend::Keychain => "keychain",
            Backend::File => "file",
            Backend::Absent => "absent",
            Backend::Unknown => "unknown",
        }
    }
}

/// A store the part of Pitboard that would read or write it is not written for yet. Every
/// call says it cannot be read, and why, which no caller takes for a store with nothing in
/// it: an empty answer would say a login is gone.
pub(crate) struct Unbuilt {
    kind: Backend,
    why: &'static str,
}

impl Unbuilt {
    /// One of `kind`, every call to which says `why`.
    pub(crate) fn new(kind: Backend, why: &'static str) -> Unbuilt {
        Unbuilt { kind, why }
    }

    fn refused<T>(&self) -> Result<T, Error> {
        Err(Error::Unreadable(self.why.into()))
    }
}

impl RawStore for Unbuilt {
    fn kind(&self) -> Backend {
        self.kind
    }

    fn contains(&self, _service: &str) -> Result<bool, Error> {
        self.refused()
    }

    fn read(&self, _service: &str) -> Result<Option<String>, Error> {
        self.refused()
    }

    fn write(&self, _: Permit, _service: &str, _contents: &str) -> Result<(), Error> {
        self.refused()
    }

    fn delete(&self, _: Permit, _service: &str) -> Result<(), Error> {
        self.refused()
    }

    fn list(&self) -> Result<Option<Vec<String>>, Error> {
        self.refused()
    }
}

/// Why a locked keychain stops Pitboard, and what to do, wherever a failure says why.
pub(crate) const LOCKED: &str = "the keychain is locked and cannot ask to be unlocked from \
                                 here; unlock it with `security unlock-keychain`, or run \
                                 Pitboard from a desktop session";

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The store could not be interrogated. Never treat this as "no credential".
    #[error("the credential store could not be read: {0}")]
    Unreadable(String),
    /// The keychain is locked and cannot ask to be unlocked from where this runs, as over
    /// SSH. Never "no credential" either, and nothing is wrong with what it holds.
    #[error("the credential store could not be read: {LOCKED}")]
    Locked,
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
            Error::Locked => "credential_store_locked",
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
/// holds exactly what was written. Both changes take the [`Permit`] only the one gate
/// every change passes makes, so no login is written or deleted, in the keychain or in a
/// file, by a run that did not ask whether it may.
pub(crate) trait RawStore: Send + Sync {
    fn kind(&self) -> Backend;
    fn contains(&self, service: &str) -> Result<bool, Error>;
    fn read(&self, service: &str) -> Result<Option<String>, Error>;
    fn write(&self, permit: Permit, service: &str, contents: &str) -> Result<(), Error>;
    fn delete(&self, permit: Permit, service: &str) -> Result<(), Error>;

    /// Every name Pitboard put here, where the store can be asked. `None` where it cannot,
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
/// Pitboard's own directory. Named here rather than in the backend so `doctor` can look at
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

fn write_in(
    chain: &[&dyn RawStore],
    permit: Permit,
    service: &str,
    contents: &str,
) -> Result<(), Error> {
    let backend = resolve_in(chain, service)?.unwrap_or(chain[0]);
    backend.write(permit, service, contents)
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
/// write fails for good, and deletes the keychain item when it does; from 2.1.281 a locked
/// keychain whose item the process has seen is not failing for good. Pitboard never
/// demotes: see the note on `write_in`.
pub fn resolve(live: &Live, service: &str) -> Result<Backend, Error> {
    with_live(live, |chain| {
        Ok(resolve_in(chain, service)?.map_or(Backend::Absent, |b| b.kind()))
    })
}

/// The nearest backend after the one in `chain` holding `service` that holds it too, by its
/// own look. The chain is a parameter so tests can pass backends that fail on demand.
fn behind_in<'a>(
    chain: &[&'a dyn RawStore],
    service: &str,
) -> Result<Option<&'a dyn RawStore>, Error> {
    let mut in_use = false;
    for backend in chain {
        if !in_use {
            in_use = backend.contains(service)?;
        } else if backend.contains(service)? {
            return Ok(Some(*backend));
        }
    }
    Ok(None)
}

/// The backend after the one holding the login in use that holds something under `service`
/// too, a login or not: what a reader that cannot reach that backend signs in with instead,
/// and what no write to it reaches. A backend is behind wherever its own look says it is
/// there, so a file is behind holding `{}`, nothing at all, or what cannot be read, as a file
/// that is not text or that this user may not read: reading it then gives the error. `None`
/// where the login in use is in the last backend, or nowhere.
pub fn behind<'a>(live: &'a Live, service: &str) -> Result<Option<&'a dyn RawStore>, Error> {
    behind_in(&live.refs(), service)
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
pub fn write_raw(live: &Live, permit: Permit, service: &str, contents: &str) -> Result<(), Error> {
    with_live(live, |chain| write_in(chain, permit, service, contents))
}

pub fn vault_read(ctx: &Context, service: &str) -> Result<Option<String>, Error> {
    vault(ctx).read(service)
}

pub fn vault_write(
    ctx: &Context,
    permit: Permit,
    service: &str,
    contents: &str,
) -> Result<(), Error> {
    vault(ctx).write(permit, service, contents)
}

/// What writing `contents` into the vault would cost against its ceiling, where it has one.
pub fn vault_cost(ctx: &Context, service: &str, contents: &str) -> Option<Cost> {
    vault(ctx).cost(service, contents)
}

pub fn vault_delete(ctx: &Context, permit: Permit, service: &str) -> Result<(), Error> {
    vault(ctx).delete(permit, service)
}

/// Every parked login on this machine, asked of the store rather than read out of
/// Pitboard's own index. `None` where the store cannot be enumerated.
pub fn vault_list(ctx: &Context) -> Result<Option<Vec<String>>, Error> {
    vault(ctx).list()
}

/// Whether another `PITBOARD_HOME` could have parked a login where this one parks its own.
pub fn vault_is_shared(ctx: &Context) -> bool {
    ctx.host().vault_is_shared()
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

    /// What the next backend after the one in use holds is what a reader that cannot reach
    /// that one gets instead. Nothing is behind a login that is in the last backend, or in
    /// none.
    #[test]
    fn what_is_behind_the_login_in_use_is_the_next_backend_that_holds_it() {
        let keychain = store(Backend::Keychain);
        let between = store(Backend::File);
        let plaintext = store(Backend::File);
        let chain: [&dyn RawStore; 3] = [&keychain, &between, &plaintext];
        let held = |chain: &[&dyn RawStore]| -> Option<String> {
            behind_in(chain, "svc")
                .unwrap()
                .map(|behind| behind.read("svc").unwrap().expect("there"))
        };
        assert_eq!(held(&chain), None);

        plaintext.plant("svc", "left");
        assert_eq!(
            held(&chain),
            None,
            "the file is the login when the keychain holds none"
        );

        keychain.plant("svc", "in use");
        assert_eq!(held(&chain).as_deref(), Some("left"));
        between.plant("svc", "nearer");
        assert_eq!(held(&chain).as_deref(), Some("nearer"));

        keychain.fault("svc", Fault::Locked);
        assert!(
            matches!(behind_in(&chain, "svc"), Err(Error::Locked)),
            "which backend is in use cannot be told, so neither can what is behind it"
        );
    }

    /// Claude Code tells a file behind the keychain by a look at it, not by reading it
    /// (`fallback_file_pins_session_login`), so neither does this. A file that is there and
    /// cannot be read is behind, with why it cannot be read, and not a chain that cannot be
    /// told. A real file, since what a look and a read each say of one is the point.
    #[test]
    fn a_file_that_cannot_be_read_is_still_behind() {
        let dir = std::env::temp_dir().join(format!(
            "pitboard-store-behind-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".credentials.json");
        std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        let keychain = holding(Backend::Keychain, "svc", "in use");
        let file = PlainFile::at(path);
        let chain: [&dyn RawStore; 2] = [&keychain, &file];

        let behind = behind_in(&chain, "svc")
            .expect("which backend is in use is told")
            .expect("the file is behind")
            .read("svc");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(matches!(behind, Err(Error::Unreadable(_))), "{behind:?}");
    }

    #[test]
    fn a_failed_keychain_write_never_falls_through_to_the_plaintext_file() {
        let keychain = holding(Backend::Keychain, "svc", "before");
        keychain.fault("svc", Fault::FailWrite("told to".into()));
        let plaintext = store(Backend::File);
        let chain: [&dyn RawStore; 2] = [&keychain, &plaintext];

        let result = write_in(&chain, Permit::for_a_test(), "svc", "after");

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
            write_in(&chain, Permit::for_a_test(), "svc", "after"),
            Err(Error::NotDurable(_))
        ));
    }

    #[test]
    fn a_credential_already_in_the_file_backend_stays_there() {
        let keychain = store(Backend::Keychain);
        let plaintext = holding(Backend::File, "svc", "before");
        let chain: [&dyn RawStore; 2] = [&keychain, &plaintext];

        write_in(&chain, Permit::for_a_test(), "svc", "after").unwrap();

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

        write_in(&chain, Permit::for_a_test(), "svc", "fresh").unwrap();

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
            fn write(&self, _: Permit, _: &str, _: &str) -> Result<(), Error> {
                unreachable!()
            }
            fn delete(&self, _: Permit, _: &str) -> Result<(), Error> {
                unreachable!()
            }
        }
        let plaintext = store(Backend::File);
        let chain: [&dyn RawStore; 2] = [&Broken, &plaintext];

        assert!(matches!(
            write_in(&chain, Permit::for_a_test(), "svc", "x"),
            Err(Error::Unreadable(_))
        ));
        assert_eq!(
            plaintext.read("svc").unwrap(),
            None,
            "could-not-tell must never be read as nothing-there"
        );
    }

    /// A store the part of Pitboard that would read it is not written for answers every call
    /// as one that cannot be read, saying why, and never as one with nothing in it; a chain
    /// of it alone resolves to that, never to absent.
    #[test]
    fn a_store_not_built_yet_cannot_be_read() {
        let store = Unbuilt::new(Backend::Unknown, "not built yet");
        let unreadable = |answer: Result<String, Error>| match answer {
            Err(Error::Unreadable(why)) => assert_eq!(why, "not built yet"),
            other => panic!("{other:?}"),
        };
        let permit = Permit::for_a_test();
        assert_eq!(store.kind(), Backend::Unknown);
        unreadable(store.contains("svc").map(|c| c.to_string()));
        unreadable(store.read("svc").map(|r| format!("{r:?}")));
        unreadable(store.write(permit, "svc", "x").map(|()| String::new()));
        unreadable(store.delete(permit, "svc").map(|()| String::new()));
        unreadable(store.list().map(|l| format!("{l:?}")));
        assert_eq!(store.cost("svc", "x"), None);
        let live = Live::of(vec![Box::new(Unbuilt::new(
            Backend::Unknown,
            "not built yet",
        ))]);
        unreadable(resolve(&live, "svc").map(|b| b.name().to_string()));
    }

    #[test]
    fn fingerprints_are_short_stable_and_not_the_secret() {
        let fp = fingerprint("sk-ant-example");
        assert_eq!(fp.len(), 16);
        assert_eq!(fp, fingerprint("sk-ant-example"));
        assert_ne!(fp, fingerprint("sk-ant-example2"));
        assert!(!fp.contains("sk-ant"));
    }
}
