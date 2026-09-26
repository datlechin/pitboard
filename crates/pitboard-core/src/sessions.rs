//! What each Claude Code session passed its status line at its last run, and which account
//! Claude Code's config named then.
//!
//! A session's numbers do not say whose they are. Claude Code passes the limits of its last
//! response every time the status line runs, and a session left idle passes the same ones
//! for as long as it stays open, whichever account the config has named since. A change is
//! what says something: numbers that moved between two runs of one session came with a
//! response it got in between. So the status line keeps what each session passed last time,
//! and the account named then, to compare its next run with.
//!
//! Nothing here is secret: session and account identifiers, shares and times. A session not
//! seen for a week is dropped.

use crate::context::Context;
use crate::{atomic, home};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

/// A session not seen for this long is dropped: its numbers are past every reset by then.
const FORGET_AFTER: i64 = 7 * 86_400;

/// How old a session's `seen_at` may get before a run that changes nothing writes it again.
/// A status line can run every second in every open session, and what it passes mostly
/// changes nothing, so seeing a session is written down once a day rather than every time.
const SEEN_EVERY: i64 = 86_400;

/// One limit as a session passed it: the share used, and when it resets.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct Limit {
    pub used_percentage: f64,
    pub resets_at: Option<i64>,
}

/// What one run of a session's status line was given.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Run {
    /// The account Claude Code's config named, if it named one.
    pub account: Option<String>,
    /// Each limit the session passed, by the name Claude Code gives it.
    pub limits: BTreeMap<String, Limit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Seen {
    #[serde(flatten)]
    run: Run,
    /// When a run last found this session, in epoch seconds, to within [`SEEN_EVERY`].
    seen_at: i64,
}

fn path(ctx: &Context) -> PathBuf {
    home::dir(ctx).join("sessions.json")
}

fn load(ctx: &Context) -> HashMap<String, Seen> {
    std::fs::read_to_string(path(ctx))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Records `run` as session `id`'s latest and returns its run before this one, or `None`
/// when this session has not been seen before.
///
/// Written only when that changes something, under the readings' lock and to what is here
/// once the lock is held, so two sessions running at once cannot each write back the
/// other's older copy. A run that cannot be written is still compared, with nothing before
/// it.
pub(crate) fn exchange(ctx: &Context, id: &str, run: &Run) -> Option<Run> {
    let now = ctx.now();
    if let Some(seen) = load(ctx).remove(id)
        && seen.run == *run
        && now - seen.seen_at < SEEN_EVERY
    {
        return Some(seen.run);
    }
    let _held = crate::readings::exclusive(ctx)?;
    let mut all = load(ctx);
    let before = all.remove(id).map(|seen| seen.run);
    all.retain(|_, seen| now - seen.seen_at < FORGET_AFTER);
    all.insert(
        id.to_string(),
        Seen {
            run: run.clone(),
            seen_at: now,
        },
    );
    if let Ok(body) = serde_json::to_string(&all) {
        let _ = atomic::write(&path(ctx), body.as_bytes(), atomic::Perms::Secret);
    }
    before
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{Clock, FixedClock};
    use std::sync::Arc;

    const NOW: i64 = 1_789_935_000;

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn machine(name: &str) -> (Context, Scratch) {
        let root = std::env::temp_dir().join(format!(
            "pitboard-sessions-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let ctx = Context::new(root.clone()).with_pitboard_home(root.clone());
        home::ensure(&ctx).expect("a home");
        (ctx, Scratch(root))
    }

    fn at(ctx: &Context, now: i64) -> Context {
        ctx.clone()
            .with_clock(Arc::new(FixedClock::at(now)) as Arc<dyn Clock>)
    }

    fn run(account: &str, used_percentage: f64) -> Run {
        Run {
            account: Some(account.into()),
            limits: BTreeMap::from([(
                "five_hour".to_string(),
                Limit {
                    used_percentage,
                    resets_at: Some(NOW + 3_600),
                },
            )]),
        }
    }

    #[test]
    fn a_session_gets_back_what_it_passed_the_run_before() {
        let (ctx, _scratch) = machine("exchange");
        let ctx = at(&ctx, NOW);
        assert_eq!(exchange(&ctx, "pane", &run("work", 20.0)), None);
        assert_eq!(
            exchange(&ctx, "pane", &run("work", 22.0)),
            Some(run("work", 20.0))
        );
        assert_eq!(
            exchange(&ctx, "other", &run("personal", 5.0)),
            None,
            "each session its own"
        );
        assert_eq!(
            exchange(&ctx, "pane", &run("work", 22.0)),
            Some(run("work", 22.0))
        );
    }

    /// A status line can run every second in every open session, and what it passes is
    /// mostly what it passed the time before.
    #[test]
    fn a_run_that_changes_nothing_leaves_the_file_alone() {
        let (ctx, _scratch) = machine("unchanged");
        exchange(&at(&ctx, NOW), "pane", &run("work", 20.0));
        let written = std::fs::read_to_string(path(&ctx)).unwrap();
        std::fs::write(path(&ctx), format!("{written} ")).unwrap();

        exchange(&at(&ctx, NOW + 3_600), "pane", &run("work", 20.0));
        assert_eq!(
            std::fs::read_to_string(path(&ctx)).unwrap(),
            format!("{written} ")
        );
    }

    /// A session still open is seen again once a day, and one nobody has run for a week goes
    /// when anything is next written.
    #[test]
    fn a_session_not_seen_for_a_week_is_dropped() {
        let (ctx, _scratch) = machine("pruned");
        exchange(&at(&ctx, NOW), "gone", &run("work", 20.0));
        exchange(&at(&ctx, NOW), "open", &run("work", 30.0));
        for day in 1..=7 {
            exchange(&at(&ctx, NOW + day * 86_400), "open", &run("work", 30.0));
        }
        let kept: Vec<String> = load(&ctx).into_keys().collect();
        assert_eq!(kept, ["open"]);
    }
}
