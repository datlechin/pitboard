//! The last usage reading pitboard took for each account.
//!
//! When an account cannot be asked, because Anthropic is unreachable or its parked login
//! could not be renewed, the only honest thing to show is the last number actually measured,
//! and when. Nothing here is secret; it is a disposable cache, written last-writer-wins
//! without a lock.

use crate::usage::{Snapshot, Source};
use crate::{atomic, home};
use std::collections::HashMap;
use std::path::PathBuf;

fn path() -> PathBuf {
    home::dir().join("usage.json")
}

pub fn load() -> HashMap<String, Snapshot> {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|raw| serde_json::from_str::<HashMap<String, Snapshot>>(&raw).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|(uuid, mut snapshot)| {
            snapshot.source = Source::Remembered;
            (uuid, snapshot)
        })
        .collect()
}

/// Keep live readings for when their account can no longer be asked.
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
