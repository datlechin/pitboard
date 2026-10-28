//! What a duty tried and said, kept in Pitboard's directory between runs and shared by every
//! front end that acts by itself. Written only under the lock every change takes, so two of
//! them never act on one thing twice, nor record one reason twice. Nothing kept is secret:
//! account ids, names of limits, times, counts and codes.

use super::cadence::{ATTEMPTS, retry_after};
use crate::permit::Permit;
use crate::{atomic, error};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The attempts at one thing, and the reasons said about it, for as long as its duty keeps
/// it: for the automatic switch, one limit of one account until it resets.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Tries {
    /// Attempts begun, but for those that failed for a reason trying again may mend.
    pub(crate) attempts: u32,
    /// Attempts in a row that failed for a reason trying again may mend, which each make the
    /// wait before the next longer.
    pub(crate) waits: u32,
    /// When the last began.
    pub(crate) at: i64,
    /// The reasons said and recorded, by code, with when each was.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) said: BTreeMap<String, i64>,
}

impl Tries {
    /// An attempt begins at `now`.
    pub(crate) fn attempt(&mut self, now: i64) {
        self.attempts += 1;
        self.at = now;
    }

    /// The last attempt failed for a reason trying again may mend: not one of the attempts,
    /// and the next waits longer.
    pub(crate) fn waited(&mut self) {
        self.attempts = self.attempts.saturating_sub(1);
        self.waits = self.waits.saturating_add(1);
    }

    /// Whether as many attempts as are ever made came to nothing.
    pub(crate) fn spent(&self) -> bool {
        self.attempts >= ATTEMPTS
    }

    /// Until when the next attempt waits after the last.
    pub(crate) fn waits_until(&self) -> i64 {
        self.at + retry_after(self.waits)
    }

    /// Whether the reason `code` was said already.
    pub(crate) fn said(&self, code: &str) -> bool {
        self.said.contains_key(code)
    }

    /// The reason `code` is said at `now`. Whether it was not said before, and so is to be
    /// recorded.
    pub(crate) fn say(&mut self, code: &str, now: i64) -> bool {
        if self.said(code) {
            return false;
        }
        self.said.insert(code.to_owned(), now);
        true
    }

    /// When anything was last kept of it.
    pub(crate) fn last(&self) -> i64 {
        self.said.values().copied().fold(self.at, i64::max)
    }
}

/// What is kept at `path`. Nothing kept, and a record that cannot be read, are nothing
/// tried: the record is written whole the next time anything is tried, and at worst one
/// thing is tried once more.
pub(crate) fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// `kept`, written whole at `path`.
pub(crate) fn save<T: Serialize>(permit: Permit, path: &Path, kept: &T) -> error::Result<()> {
    let failed = |source| error::Error::StateWriteFailed {
        path: path.to_path_buf(),
        source,
    };
    let body = serde_json::to_vec(kept).map_err(|e| failed(std::io::Error::other(e)))?;
    atomic::write(permit, path, &body, atomic::Perms::Secret).map_err(failed)
}

/// Nothing kept at `path` any more: no file, rather than an empty record.
pub(crate) fn forget(permit: Permit, path: &Path) -> error::Result<()> {
    match crate::host::fs::remove_file(permit, path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(error::Error::StateWriteFailed {
            path: path.to_path_buf(),
            source,
        }),
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;

    #[test]
    fn an_attempt_that_waiting_may_mend_is_not_counted_and_the_next_waits_longer() {
        let mut tries = Tries::default();
        tries.attempt(100);
        tries.waited();
        assert_eq!((tries.attempts, tries.waits), (0, 1));
        assert_eq!(tries.waits_until(), 160);
        tries.attempt(160);
        tries.waited();
        assert_eq!(tries.waits_until(), 280);
        for at in 0..i64::from(ATTEMPTS) {
            assert!(!tries.spent());
            tries.attempt(300 + at);
        }
        assert!(tries.spent());
    }

    #[test]
    fn a_reason_is_said_once_and_counts_as_kept() {
        let mut tries = Tries::default();
        tries.attempt(10);
        assert!(tries.say("no_room", 50));
        assert!(!tries.say("no_room", 60));
        assert!(tries.said("no_room"));
        assert!(!tries.said("settling"));
        assert_eq!(tries.last(), 50);
    }

    #[test]
    fn nothing_said_is_left_out_of_the_record() {
        let tries = Tries {
            attempts: 1,
            waits: 0,
            at: 10,
            said: BTreeMap::new(),
        };
        let text = serde_json::to_string(&tries).expect("serialises");
        assert_eq!(text, r#"{"attempts":1,"waits":0,"at":10}"#);
        let back: Tries = serde_json::from_str(r#"{"at":10}"#).expect("a missing field is none");
        assert_eq!(back.at, 10);
    }

    #[test]
    fn a_record_that_cannot_be_read_is_nothing_kept() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path().to_path_buf();
        let path = dir.join("kept.json");
        std::fs::write(&path, "not json").expect("write");
        let kept: BTreeMap<String, Tries> = load(&path);
        assert!(kept.is_empty());
    }
}
