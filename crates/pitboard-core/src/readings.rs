//! The newest usage reading pitboard knows for each account.
//!
//! When an account cannot be asked, because Anthropic is unreachable or its parked login
//! could not be renewed, the only honest thing to show is the last number actually measured,
//! and when.
//!
//! Every front end records here and shows what is here: a status line in each open session,
//! the command line and the menu bar. A session knows only what its own last response said,
//! so when each showed its own numbers and whoever wrote last won, sessions on one account
//! disagreed with each other and with the menu bar. A reading here only moves forward (see
//! [`crate::usage::merge`]), so it is the newest thing any of them has seen. Nothing here is
//! secret.

use crate::context::Context;
use crate::usage::{Snapshot, Source};
use crate::{atomic, home};
use std::collections::HashMap;
use std::os::unix::fs::OpenOptionsExt;
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

/// When the readings last changed, in epoch milliseconds, or 0 when there are none.
///
/// For a front end to follow what the others record without asking anyone. Milliseconds
/// rather than seconds, because sessions record moments apart, and the second of two
/// changes within one second would otherwise go unseen until the next.
pub fn changed_at(ctx: &Context) -> i64 {
    std::fs::metadata(path(ctx))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

/// Drop what was remembered for an account that is no longer enrolled. Nothing here is
/// secret, but an account someone has dropped should leave no trace behind either.
pub fn forget(ctx: &Context, account_uuid: &str) {
    if !load(ctx).contains_key(account_uuid) {
        return;
    }
    let Some(_held) = exclusive(ctx) else {
        return;
    };
    let mut all = load(ctx);
    if all.remove(account_uuid).is_none() {
        return;
    }
    if let Ok(body) = serde_json::to_string(&all) {
        let _ = atomic::write(&path(ctx), body.as_bytes(), atomic::Perms::Secret);
    }
}

/// Fold readings into what is remembered, account by account and limit by limit, keeping
/// the newer of each. However stale what is offered, and whoever writes last, a reading
/// never moves backwards.
///
/// Written only when that changes something: a status line runs in every open session, as
/// often as every second, and mostly offers what is already here. A change is made under a
/// lock, to what is here once the lock is held, so two front ends writing at once cannot
/// each write back the other's older copy.
pub fn remember(ctx: &Context, readings: &[(String, Snapshot)]) {
    if readings.is_empty() || !fold(&mut load(ctx), readings, ctx.now()) {
        return;
    }
    let Some(_held) = exclusive(ctx) else {
        return;
    };
    let mut all = load(ctx);
    if fold(&mut all, readings, ctx.now())
        && let Ok(body) = serde_json::to_string(&all)
    {
        let _ = atomic::write(&path(ctx), body.as_bytes(), atomic::Perms::Secret);
    }
}

/// `readings` folded into `all`, and whether that changed anything.
fn fold(all: &mut HashMap<String, Snapshot>, readings: &[(String, Snapshot)], now: i64) -> bool {
    let mut changed = false;
    for (uuid, offered) in readings {
        let Some(mut next) = crate::usage::merge(all.get(uuid), Some(offered), now) else {
            continue;
        };
        // What it is once stored, as `load` says of everything here.
        next.account_uuid = Some(uuid.clone());
        next.source = Source::Remembered;
        if all.get(uuid) != Some(&next) {
            all.insert(uuid.clone(), next);
            changed = true;
        }
    }
    changed
}

/// Held while a change is written. A kernel lock, which the system lets go of when the
/// process ends, and apart from the one around switches: a status line must never wait on
/// a switch.
fn exclusive(ctx: &Context) -> Option<std::fs::File> {
    home::ensure(ctx).ok()?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(home::dir(ctx).join("usage.lock"))
        .ok()?;
    file.lock().ok()?;
    Some(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{Clock, FixedClock};
    use crate::usage::Window;
    use std::sync::Arc;

    const NOW: i64 = 1_789_935_000;
    const RESETS: i64 = NOW + 3_600;

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn machine(name: &str) -> (Context, Scratch) {
        let root = std::env::temp_dir().join(format!(
            "pitboard-readings-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root.clone())
            .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn Clock>);
        home::ensure(&ctx).expect("a home");
        (ctx, Scratch(root))
    }

    fn reading(kind: &str, percent: f64, observed_at: Option<i64>) -> Snapshot {
        Snapshot {
            windows: vec![Window {
                kind: kind.into(),
                scope: None,
                percent,
                resets_at: Some(RESETS),
                is_active: true,
                severity: None,
                length_seconds: Some(5 * 3_600),
            }],
            observed_at,
            account_uuid: None,
            source: Source::Live,
        }
    }

    fn five_hour(ctx: &Context, uuid: &str) -> Option<f64> {
        Some(load(ctx).get(uuid)?.windows.first()?.percent)
    }

    /// The owner's panes: a busy one had recorded 22% when an idle one, still holding the
    /// 20% of its last response, ran after it. Whoever writes last, the 22% stands.
    #[test]
    fn a_reading_never_moves_backwards_whoever_writes_last() {
        let (ctx, _scratch) = machine("backwards");
        remember(
            &ctx,
            &[("work".into(), reading("session", 22.0, Some(NOW - 60)))],
        );
        remember(&ctx, &[("work".into(), reading("five_hour", 20.0, None))]);
        assert_eq!(five_hour(&ctx, "work"), Some(22.0));
        assert_eq!(load(&ctx)["work"].observed_at, Some(NOW - 60));

        remember(&ctx, &[("work".into(), reading("five_hour", 25.0, None))]);
        assert_eq!(
            five_hour(&ctx, "work"),
            Some(25.0),
            "and forwards is forwards"
        );
        assert_eq!(load(&ctx)["work"].observed_at, Some(NOW));
    }

    /// A status line runs in every open session, as often as every second, and what it
    /// offers is mostly what is already here.
    #[test]
    fn a_reading_that_changes_nothing_leaves_the_file_alone() {
        let (ctx, _scratch) = machine("unchanged");
        remember(
            &ctx,
            &[
                ("work".into(), reading("session", 22.0, Some(NOW - 60))),
                ("personal".into(), reading("session", 3.0, Some(NOW - 60))),
            ],
        );
        // Laid out as nothing pitboard writes would lay it out, so a rewrite shows.
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path(&ctx)).unwrap()).unwrap();
        let laid_out = serde_json::to_string_pretty(&written).unwrap();
        std::fs::write(path(&ctx), &laid_out).unwrap();

        remember(&ctx, &[("work".into(), reading("five_hour", 20.0, None))]);
        remember(&ctx, &[("work".into(), reading("five_hour", 22.0, None))]);
        assert_eq!(std::fs::read_to_string(path(&ctx)).unwrap(), laid_out);
    }
}
