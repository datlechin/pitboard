//! The last usage reading pitboard took for each account.
//!
//! When an account cannot be asked, because Anthropic is unreachable or its parked login
//! could not be renewed, the only honest thing to show is the last number actually measured,
//! and when. Nothing here is secret; it is a disposable cache, written last-writer-wins
//! without a lock.

use crate::context::Context;
use crate::usage::{Snapshot, Source};
use crate::{atomic, home};
use std::collections::HashMap;
use std::path::PathBuf;

fn path(ctx: &Context) -> PathBuf {
    home::dir(ctx).join("usage.json")
}

pub fn load(ctx: &Context) -> HashMap<String, Snapshot> {
    std::fs::read_to_string(path(ctx))
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

/// Drop what was remembered for an account that is no longer enrolled. Nothing here is
/// secret, but an account someone has dropped should leave no trace behind either.
pub fn forget(ctx: &Context, account_uuid: &str) {
    let mut all = load(ctx);
    if all.remove(account_uuid).is_none() {
        return;
    }
    if let Ok(body) = serde_json::to_string(&all) {
        let _ = atomic::write(&path(ctx), body.as_bytes(), atomic::Perms::Secret);
    }
}

/// Keep live readings for when their account can no longer be asked.
pub fn remember(ctx: &Context, readings: &[(String, Snapshot)]) {
    if readings.is_empty() {
        return;
    }
    let mut all = load(ctx);
    for (uuid, snapshot) in readings {
        let mut stored = snapshot.clone();
        stored.account_uuid = Some(uuid.clone());
        all.insert(uuid.clone(), stored);
    }
    if home::ensure(ctx).is_ok()
        && let Ok(body) = serde_json::to_string(&all)
    {
        let _ = atomic::write(&path(ctx), body.as_bytes(), atomic::Perms::Secret);
    }
}
