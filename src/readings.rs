//! The last usage reading pitboard took for each account.
//!
//! A parked account's access token expires about twelve hours after it was parked, and
//! pitboard will not refresh it, so after that the only honest thing to show is the last
//! number actually measured and when. Nothing here is secret; it is a disposable cache,
//! so it is written last-writer-wins without taking a lock.

use crate::usage::{Snapshot, Source};
use crate::{atomic, home};
use std::collections::HashMap;
use std::path::PathBuf;

fn path() -> PathBuf {
    home::dir().join("usage.json")
}

fn load() -> HashMap<String, Snapshot> {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// The last live reading for an account, marked as remembered rather than live.
pub fn recall(account_uuid: &str) -> Option<Snapshot> {
    load().remove(account_uuid).map(|mut s| {
        s.source = Source::Remembered;
        s
    })
}

pub fn remember(readings: &[(String, Snapshot)]) {
    if readings.is_empty() {
        return;
    }
    let mut all = load();
    for (uuid, snapshot) in readings {
        let mut stored = snapshot.clone();
        stored.account_uuid = Some(uuid.clone());
        all.insert(uuid.clone(), stored);
    }
    if home::ensure().is_ok()
        && let Ok(body) = serde_json::to_string(&all)
    {
        let _ = atomic::write(&path(), body.as_bytes(), atomic::Perms::Secret);
    }
}
