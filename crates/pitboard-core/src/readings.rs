//! The newest usage reading Pitboard knows for each account.
//!
//! When an account cannot be asked, because Anthropic is unreachable or its parked login
//! could not be renewed, the only honest thing to show is the last number actually measured,
//! and when.
//!
//! Every front end records here and shows what is here: a status line in each open session,
//! the command line and the menu bar. A session knows only what its own last response said,
//! so when each showed its own numbers and whoever wrote last won, sessions on one account
//! disagreed with each other and with the menu bar. An answer from the service sets an
//! account's reading, and a session's numbers only move a limit an answer gave (see
//! [`crate::usage::answered`] and [`crate::usage::moved`]), so a reading is the newest thing
//! any of them has seen of that account. Nothing here is secret.

use crate::context::Context;
use crate::service::Permit;
use crate::usage::{Snapshot, Source, Window};
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
pub fn forget(ctx: &Context, permit: Permit, id: &str) {
    change(ctx, permit, |all| all.remove(id).is_some());
}

/// Answers about accounts, each recorded as that account's reading as
/// [`crate::usage::answered`] folds it in.
pub fn answered(ctx: &Context, permit: Permit, answers: &[(String, Snapshot)]) {
    let now = ctx.now();
    change(ctx, permit, |all| {
        let mut changed = false;
        for (id, answer) in answers {
            let mut next = crate::usage::answered(all.get(id), answer, now);
            // What it is once stored, as `load` says of everything here.
            next.source = Source::Remembered;
            if all.get(id) != Some(&next) {
                all.insert(id.clone(), next);
                changed = true;
            }
        }
        changed
    });
}

/// What a session on account `id` passed its status line, recorded where it moves a limit
/// of that account's reading, as [`crate::usage::moved`] says. An account with no reading
/// gets none from it.
pub fn moved(ctx: &Context, permit: Permit, id: &str, passed: &[Window]) {
    let now = ctx.now();
    change(ctx, permit, |all| {
        let Some(next) = all
            .get(id)
            .and_then(|known| crate::usage::moved(known, passed, now))
        else {
            return false;
        };
        all.insert(id.to_owned(), next);
        true
    });
}

/// The readings changed by `fold`, which says whether it changed anything.
///
/// Written only when it did: a status line runs in every open session, as often as every
/// second, and mostly passes what is already here. A change is made under a lock, to what
/// is here once the lock is held, so two front ends writing at once cannot each write back
/// the other's older copy.
fn change(ctx: &Context, permit: Permit, fold: impl Fn(&mut HashMap<String, Snapshot>) -> bool) {
    if !fold(&mut load(ctx)) {
        return;
    }
    let Some(_held) = exclusive(ctx, permit) else {
        return;
    };
    let mut all = load(ctx);
    if fold(&mut all)
        && let Ok(body) = serde_json::to_string(&all)
    {
        let _ = atomic::write(permit, &path(ctx), body.as_bytes(), atomic::Perms::Secret);
    }
}

/// Held while a change is written, here or to what sessions passed their status line. A
/// kernel lock, which the system lets go of when the process ends, and apart from the one
/// around switches: a status line must never wait on a switch.
pub(crate) fn exclusive(ctx: &Context, permit: Permit) -> Option<std::fs::File> {
    home::ensure(ctx, permit).ok()?;
    let file =
        crate::host::fs::open_private_lock(permit, &home::dir(ctx).join("usage.lock")).ok()?;
    file.lock().ok()?;
    Some(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{Clock, FixedClock};
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
        home::ensure(&ctx, Permit::for_a_test()).expect("a home");
        (ctx, Scratch(root))
    }

    fn window(kind: &str, percent: f64) -> Window {
        Window {
            kind: kind.into(),
            scope: None,
            percent,
            resets_at: Some(RESETS),
            is_active: true,
            severity: None,
            length_seconds: crate::usage::anthropic_window_length(kind),
        }
    }

    /// Anthropic's answer about an account with one limit, taken at `at`.
    fn answer(kind: &str, percent: f64, at: i64) -> Snapshot {
        Snapshot {
            windows: vec![window(kind, percent)],
            observed_at: Some(at),
            answered_at: Some(at),
            lists_every_limit: true,
            source: Source::Live,
        }
    }

    /// A session on `work` passing its status line `percent` of the limit Claude Code calls
    /// `kind`.
    fn passes(ctx: &Context, kind: &str, percent: f64) {
        moved(ctx, Permit::for_a_test(), "work", &[window(kind, percent)]);
    }

    fn five_hour(ctx: &Context, uuid: &str) -> Option<f64> {
        Some(load(ctx).get(uuid)?.windows.first()?.percent)
    }

    /// The readings as they are, laid out as nothing Pitboard writes would lay them out, so
    /// a rewrite shows.
    fn laid_out(ctx: &Context) -> String {
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path(ctx)).unwrap()).unwrap();
        let laid_out = serde_json::to_string_pretty(&written).unwrap();
        std::fs::write(path(ctx), &laid_out).unwrap();
        laid_out
    }

    /// 0.9.0 filed a session's numbers and Claude Code's usage cache under the account its
    /// config named, which could be another login's, and kept a limit until its reset
    /// whatever an answer said. The account's first answer is all that is left of it.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_reading_0_9_0_wrote_under_the_wrong_account_is_gone_at_the_first_answer() {
        let (ctx, _scratch) = machine("from-0-9-0");
        std::fs::write(
            path(&ctx),
            serde_json::json!({"work": {
                "windows": [
                    {"kind": "session", "scope": null, "percent": 40.0, "resets_at": RESETS,
                     "is_active": true, "severity": null, "length_seconds": 18000},
                    {"kind": "weekly_all", "scope": null, "percent": 100.0,
                     "resets_at": NOW + 5 * 86_400, "is_active": false, "severity": null,
                     "length_seconds": 604800}
                ],
                "observed_at": NOW - 600,
                "account_uuid": "work",
                "source": "remembered"
            }})
            .to_string(),
        )
        .unwrap();
        assert_eq!(load(&ctx)["work"].answered_at, None, "as 0.9.0 wrote it");

        answered(
            &ctx,
            Permit::for_a_test(),
            &[("work".into(), answer("session", 12.0, NOW))],
        );
        let work = &load(&ctx)["work"];
        let kinds: Vec<&str> = work.windows.iter().map(|w| w.kind.as_str()).collect();
        assert_eq!(kinds, ["session"]);
        assert_eq!(work.answered_at, Some(NOW));
    }

    /// One file holds both tools' readings, and each answer is recorded by what it is:
    /// Anthropic's lists every limit, and a window OpenAI's leaves out stands while it runs.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn each_answer_is_recorded_by_whether_it_lists_every_limit() {
        let (ctx, _scratch) = machine("by-service");
        let given = |kinds: &[&str], at: i64, lists_every_limit: bool| Snapshot {
            windows: kinds.iter().map(|kind| window(kind, 30.0)).collect(),
            observed_at: Some(at),
            answered_at: Some(at),
            lists_every_limit,
            source: Source::Live,
        };
        for (at, kinds) in [
            (NOW - 60, &["five_hour", "seven_day"][..]),
            (NOW, &["seven_day"]),
        ] {
            answered(
                &ctx,
                Permit::for_a_test(),
                &[
                    ("claude".into(), given(kinds, at, true)),
                    ("codex".into(), given(kinds, at, false)),
                ],
            );
        }
        let kinds = |id: &str| -> Vec<String> {
            load(&ctx)[id]
                .windows
                .iter()
                .map(|w| w.kind.clone())
                .collect()
        };
        assert_eq!(kinds("claude"), ["seven_day"]);
        assert_eq!(kinds("codex"), ["five_hour", "seven_day"]);
        assert!(
            load(&ctx)["claude"].lists_every_limit,
            "and kept as it was said"
        );
    }

    /// The owner's panes: a busy one had recorded 22% when an idle one, still holding the
    /// 20% of its last response, ran after it. Whoever writes last, the 22% stands.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_session_never_moves_a_reading_back_whoever_writes_last() {
        let (ctx, _scratch) = machine("backwards");
        answered(
            &ctx,
            Permit::for_a_test(),
            &[("work".into(), answer("session", 22.0, NOW - 60))],
        );
        passes(&ctx, "five_hour", 20.0);
        assert_eq!(five_hour(&ctx, "work"), Some(22.0));
        assert_eq!(load(&ctx)["work"].observed_at, Some(NOW - 60));

        passes(&ctx, "five_hour", 25.0);
        assert_eq!(
            five_hour(&ctx, "work"),
            Some(25.0),
            "and forwards is forwards"
        );
        let work = &load(&ctx)["work"];
        assert_eq!(work.observed_at, Some(NOW));
        assert_eq!(work.answered_at, Some(NOW - 60), "and no answer since");
    }

    /// A session's numbers do not say whose they are, and an account Pitboard has never had
    /// an answer about has no window to know them by.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_session_founds_no_reading() {
        let (ctx, _scratch) = machine("founds");
        passes(&ctx, "five_hour", 22.0);
        assert!(load(&ctx).is_empty());
        assert!(!path(&ctx).exists());
    }

    /// A status line runs in every open session, as often as every second, and what it
    /// passes is mostly what is already here.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_reading_that_changes_nothing_leaves_the_file_alone() {
        let (ctx, _scratch) = machine("unchanged");
        let answers = [
            ("work".into(), answer("session", 22.0, NOW - 60)),
            ("personal".into(), answer("session", 3.0, NOW - 60)),
        ];
        answered(&ctx, Permit::for_a_test(), &answers);
        let laid_out = laid_out(&ctx);

        passes(&ctx, "five_hour", 20.0);
        passes(&ctx, "five_hour", 22.0);
        answered(&ctx, Permit::for_a_test(), &answers);
        assert_eq!(std::fs::read_to_string(path(&ctx)).unwrap(), laid_out);
    }

    /// After a banked reset on claude.ai, this machine's sessions passed 14% of a weekly limit
    /// recorded at 100%, with the same reset. A session cannot say its number is the newer,
    /// so the 100% stands until Pitboard's next answer from Anthropic, which can. From there
    /// sessions move the lower share forward again.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_banked_reset_is_recorded_by_the_next_answer_and_followed_by_sessions() {
        let (ctx, _scratch) = machine("banked");
        let weekly = |ctx: &Context| load(ctx)["work"].windows[0].percent;
        answered(
            &ctx,
            Permit::for_a_test(),
            &[("work".into(), answer("weekly_all", 100.0, NOW - 3_600))],
        );

        passes(&ctx, "seven_day", 14.0);
        assert_eq!(weekly(&ctx), 100.0, "a session says no time");

        answered(
            &ctx,
            Permit::for_a_test(),
            &[("work".into(), answer("weekly_all", 14.0, NOW))],
        );
        assert_eq!(weekly(&ctx), 14.0);

        passes(&ctx, "seven_day", 15.0);
        assert_eq!(weekly(&ctx), 15.0);
    }

    /// A parked account that ran out, asked about once its window has reset: the answer that
    /// finds nothing used is recorded once, and every read after it that finds the same
    /// leaves the file alone.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_window_past_its_reset_is_recorded_reset_once() {
        let (ctx, _scratch) = machine("reset");
        let mut full = answer("session", 100.0, NOW - 7_200);
        full.windows[0].resets_at = Some(NOW - 3_600);
        answered(&ctx, Permit::for_a_test(), &[("parked".into(), full)]);

        let mut idle = answer("session", 0.0, NOW);
        idle.windows[0].resets_at = None;
        answered(
            &ctx,
            Permit::for_a_test(),
            &[("parked".into(), idle.clone())],
        );
        assert_eq!(five_hour(&ctx, "parked"), Some(0.0));

        let laid_out = laid_out(&ctx);
        answered(&ctx, Permit::for_a_test(), &[("parked".into(), idle)]);
        assert_eq!(std::fs::read_to_string(path(&ctx)).unwrap(), laid_out);
    }
}
