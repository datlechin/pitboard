//! Credential stores that live in memory and fail when they are told to.
//!
//! The interesting half of pitboard is what happens when a step fails partway through, and
//! against a real keychain none of it can be produced: a write cannot be made to fail, an
//! item cannot be made to disappear between two calls, and a read cannot be made to say it
//! could not tell. So the tests that mattered most were the ones that could not be written.
//!
//! This is not a simulation of the keychain. It keeps the one rule every real backend has
//! to keep, that a write reads its own result back and reports anything else, and otherwise
//! does exactly what it is told.

use super::{Backend, Error, Platform, RawStore};
use crate::context::Context;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// What a store does instead of working.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// The write fails outright and nothing is stored, the way a refused keychain write does.
    FailWrite(String),
    /// The write lands, but the store holds something else afterwards. A backend that lies
    /// about what it wrote is the failure `NotDurable` exists for.
    CorruptWrite(String),
    /// Every read says it could not tell. Never the same answer as "nothing is there":
    /// reading a locked keychain as empty is what would tell someone to sign in again.
    Unreadable(String),
    /// The item is gone from this call onwards, as if something else had deleted it.
    Vanish,
}

/// One store. Plant, peek and enumerate without going through the store's own rules, so a
/// test can set up a machine state and then assert on it.
#[derive(Debug)]
pub struct MemoryStore {
    kind: Backend,
    items: Mutex<HashMap<String, String>>,
    faults: Mutex<HashMap<String, Fault>>,
    blanket: Mutex<Option<Fault>>,
}

impl MemoryStore {
    /// An empty store that reports itself as `kind`.
    pub fn of(kind: Backend) -> Arc<MemoryStore> {
        Arc::new(MemoryStore {
            kind,
            items: Mutex::new(HashMap::new()),
            faults: Mutex::new(HashMap::new()),
            blanket: Mutex::new(None),
        })
    }

    /// Put something there without a write, to set a machine up.
    pub fn plant(&self, service: &str, contents: &str) {
        self.items
            .lock()
            .expect("a poisoned test store is a failed test")
            .insert(service.into(), contents.into());
    }

    /// What is there, ignoring any fault. This is the assertion a test makes.
    pub fn peek(&self, service: &str) -> Option<String> {
        self.items
            .lock()
            .expect("a poisoned test store is a failed test")
            .get(service)
            .cloned()
    }

    /// Every service this store holds, which no real vault on macOS can answer.
    pub fn services(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .items
            .lock()
            .expect("a poisoned test store is a failed test")
            .keys()
            .cloned()
            .collect();
        names.sort();
        names
    }

    /// From now on, this service misbehaves in this way.
    pub fn fault(&self, service: &str, fault: Fault) {
        self.faults
            .lock()
            .expect("a poisoned test store is a failed test")
            .insert(service.into(), fault);
    }

    /// From now on, every service misbehaves in this way, including ones that do not
    /// exist yet. A name reserved at the moment of the write cannot be named in advance.
    pub fn fault_all(&self, fault: Fault) {
        *self
            .blanket
            .lock()
            .expect("a poisoned test store is a failed test") = Some(fault);
    }

    /// Stop misbehaving.
    pub fn heal(&self, service: &str) {
        self.faults
            .lock()
            .expect("a poisoned test store is a failed test")
            .remove(service);
    }

    fn fault_for(&self, service: &str) -> Option<Fault> {
        self.faults
            .lock()
            .expect("a poisoned test store is a failed test")
            .get(service)
            .cloned()
            .or_else(|| {
                self.blanket
                    .lock()
                    .expect("a poisoned test store is a failed test")
                    .clone()
            })
    }
}

impl RawStore for Arc<MemoryStore> {
    fn kind(&self) -> Backend {
        self.kind
    }

    fn contains(&self, service: &str) -> Result<bool, Error> {
        Ok(self.read(service)?.is_some())
    }

    fn read(&self, service: &str) -> Result<Option<String>, Error> {
        match self.fault_for(service) {
            Some(Fault::Unreadable(why)) => Err(Error::Unreadable(why)),
            Some(Fault::Vanish) => Ok(None),
            _ => Ok(self.peek(service)),
        }
    }

    fn write(&self, service: &str, contents: &str) -> Result<(), Error> {
        match self.fault_for(service) {
            Some(Fault::FailWrite(why)) => return Err(Error::Write(why)),
            Some(Fault::Unreadable(why)) => return Err(Error::Unreadable(why)),
            Some(Fault::CorruptWrite(instead)) => self.plant(service, &instead),
            _ => self.plant(service, contents),
        }
        // The rule every backend keeps: believe the store, not the call that wrote to it.
        match self.peek(service) {
            Some(back) if back == contents => Ok(()),
            _ => Err(Error::NotDurable(format!(
                "{service} holds different bytes"
            ))),
        }
    }

    fn delete(&self, service: &str) -> Result<(), Error> {
        self.items
            .lock()
            .expect("a poisoned test store is a failed test")
            .remove(service);
        Ok(())
    }

    fn list(&self) -> Result<Option<Vec<String>>, Error> {
        Ok(Some(
            self.services()
                .into_iter()
                .filter(|s| crate::park::is_park_name(s))
                .collect(),
        ))
    }
}

/// A machine with no keychain and no files: a live chain, a vault, and whatever Claude Code
/// would have left behind for a private sign-in.
#[derive(Debug)]
pub struct MemoryPlatform {
    live: Arc<MemoryStore>,
    vault: Arc<MemoryStore>,
    signins: Mutex<HashMap<PathBuf, String>>,
}

impl Default for MemoryPlatform {
    fn default() -> MemoryPlatform {
        MemoryPlatform {
            // Keychain, because that is the chain the interesting rules are written for.
            live: MemoryStore::of(Backend::Keychain),
            vault: MemoryStore::of(Backend::Keychain),
            signins: Mutex::new(HashMap::new()),
        }
    }
}

impl MemoryPlatform {
    pub fn new() -> Arc<MemoryPlatform> {
        Arc::new(MemoryPlatform::default())
    }

    /// Where Claude Code's live credential is.
    pub fn live(&self) -> &Arc<MemoryStore> {
        &self.live
    }

    /// Where pitboard's parked logins are.
    pub fn vault(&self) -> &Arc<MemoryStore> {
        &self.vault
    }

    /// What a private sign-in left in `dir`, as if `claude` had run there.
    pub fn plant_signin(&self, dir: &Path, contents: &str) {
        self.signins
            .lock()
            .expect("a poisoned test platform is a failed test")
            .insert(dir.to_path_buf(), contents.into());
    }
}

impl Platform for MemoryPlatform {
    fn live_chain(&self, _ctx: &Context) -> Vec<Box<dyn RawStore>> {
        vec![Box::new(Arc::clone(&self.live))]
    }

    fn vault(&self, _ctx: &Context) -> Box<dyn RawStore> {
        Box::new(Arc::clone(&self.vault))
    }

    fn read_signin(&self, _ctx: &Context, dir: &Path) -> Result<Option<String>, Error> {
        Ok(self
            .signins
            .lock()
            .expect("a poisoned test platform is a failed test")
            .get(dir)
            .cloned())
    }

    fn discard_signin(&self, _ctx: &Context, dir: &Path) -> Result<(), Error> {
        self.signins
            .lock()
            .expect("a poisoned test platform is a failed test")
            .remove(dir);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Arc<MemoryStore> {
        MemoryStore::of(Backend::Keychain)
    }

    #[test]
    fn a_write_that_does_not_read_back_is_reported_rather_than_believed() {
        let s = store();
        s.fault("svc", Fault::CorruptWrite("something else".into()));
        assert!(matches!(s.write("svc", "after"), Err(Error::NotDurable(_))));
        assert_eq!(s.peek("svc").as_deref(), Some("something else"));
    }

    #[test]
    fn a_refused_write_leaves_what_was_there() {
        let s = store();
        s.plant("svc", "before");
        s.fault("svc", Fault::FailWrite("told to".into()));
        assert!(matches!(s.write("svc", "after"), Err(Error::Write(_))));
        assert_eq!(s.peek("svc").as_deref(), Some("before"));
    }

    #[test]
    fn an_unreadable_store_never_answers_absent() {
        let s = store();
        s.plant("svc", "before");
        s.fault("svc", Fault::Unreadable("the keychain is locked".into()));
        assert!(matches!(s.read("svc"), Err(Error::Unreadable(_))));
        assert!(matches!(s.contains("svc"), Err(Error::Unreadable(_))));
    }

    #[test]
    fn something_that_vanished_reads_as_gone_but_is_still_there_to_assert_on() {
        let s = store();
        s.plant("svc", "before");
        s.fault("svc", Fault::Vanish);
        assert_eq!(s.read("svc").expect("vanishing is not a failure"), None);
        assert_eq!(s.peek("svc").as_deref(), Some("before"));
        s.heal("svc");
        assert_eq!(s.read("svc").expect("healed"), Some("before".into()));
    }

    #[test]
    fn a_vault_can_be_enumerated_which_no_real_one_on_macos_can() {
        let s = store();
        s.plant("pitboard-park-b", "2");
        s.plant("pitboard-park-a", "1");
        assert_eq!(s.services(), vec!["pitboard-park-a", "pitboard-park-b"]);
    }
}
