//! What each account's limits have been doing, rather than only what they are now.
//!
//! The single decision pitboard exists to support is which account to use next, and it
//! answered with two instantaneous percentages and left the arithmetic to the person. 73%
//! of a weekly limit means nothing without knowing whether it was 40% this morning. An
//! account at 60% with four hours until its window resets is better than one at 40% with
//! twenty minutes, and better again than one at 20% whose weekly limit resets on Sunday.
//!
//! pitboard already had the data and threw it away: one snapshot per account in a map that
//! every write replaced. This keeps the series instead, one line per reading, and derives
//! from it the only number that answers the question: how long this account lasts.
//!
//! Nothing here is secret. It is percentages and times.
//!
//! Sized before it was written: one reading is 319 bytes, and a machine running `pitboard`
//! and a status line writes several an hour. So a reading is only appended when the last
//! one is old enough or the number has actually moved, and anything older than a fortnight
//! goes when the file is next written.

use crate::context::Context;
use crate::usage::Snapshot;
use crate::{atomic, home};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

/// Two weeks, which covers a weekly window twice over.
const KEEP_FOR: i64 = 14 * 86_400;

/// A reading closer than this to the last one is not worth a line unless something moved.
const APART: i64 = 300;

/// A move worth recording however recently the last reading was taken.
const MOVED: f64 = 0.5;

/// One reading of one window, flattened: the series is per account, and a line per reading
/// is smaller and easier to prune than a line per window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub at: i64,
    /// Window kind to percentage, as the reading had them.
    pub windows: Vec<(String, f64)>,
    /// When each window resets, where the reading said.
    #[serde(default)]
    pub resets: Vec<(String, i64)>,
}

fn dir(ctx: &Context) -> PathBuf {
    home::dir(ctx).join("readings")
}

fn path(ctx: &Context, account_uuid: &str) -> Option<PathBuf> {
    // The name goes into a file name, so anything that is not a plain identifier is refused
    // rather than escaped.
    let safe = !account_uuid.is_empty()
        && account_uuid
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    safe.then(|| dir(ctx).join(format!("{account_uuid}.ndjson")))
}

/// Everything known about this account, oldest first.
pub fn series(ctx: &Context, account_uuid: &str) -> Vec<Point> {
    let Some(path) = path(ctx, account_uuid) else {
        return Vec::new();
    };
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Point>(line).ok())
        .collect()
}

fn point_of(snapshot: &Snapshot, at: i64) -> Point {
    Point {
        at,
        windows: snapshot
            .windows
            .iter()
            .map(|w| (w.kind.clone(), w.percent))
            .collect(),
        resets: snapshot
            .windows
            .iter()
            .filter_map(|w| w.resets_at.map(|r| (w.kind.clone(), r)))
            .collect(),
    }
}

/// Whether this reading says anything the last one did not.
fn worth_keeping(last: Option<&Point>, next: &Point) -> bool {
    let Some(last) = last else {
        return true;
    };
    if next.at - last.at >= APART {
        return true;
    }
    next.windows.iter().any(|(kind, percent)| {
        last.windows
            .iter()
            .find(|(k, _)| k == kind)
            .is_none_or(|(_, was)| (percent - was).abs() >= MOVED)
    })
}

/// Record a reading, if it says anything.
pub fn record(ctx: &Context, account_uuid: &str, snapshot: &Snapshot) {
    let Some(path) = path(ctx, account_uuid) else {
        return;
    };
    let at = snapshot.observed_at.unwrap_or_else(|| ctx.now());
    let next = point_of(snapshot, at);
    let existing = series(ctx, account_uuid);
    if !worth_keeping(existing.last(), &next) {
        return;
    }
    if home::create_private(&dir(ctx)).is_err() {
        return;
    }
    let Ok(line) = serde_json::to_string(&next) else {
        return;
    };

    // Whole lines under O_APPEND, so the status line and a command writing at once
    // interleave lines rather than bytes.
    let stale = existing.iter().filter(|p| at - p.at > KEEP_FOR).count();
    if stale > 0 {
        let kept: Vec<String> = existing
            .iter()
            .filter(|p| at - p.at <= KEEP_FOR)
            .filter_map(|p| serde_json::to_string(p).ok())
            .chain(std::iter::once(line))
            .collect();
        let _ = atomic::write(
            &path,
            format!("{}\n", kept.join("\n")).as_bytes(),
            atomic::Perms::Secret,
        );
        return;
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&path)
    {
        let _ = writeln!(file, "{line}");
    }
}

/// Forget an account nobody is enrolled as any more.
pub fn forget(ctx: &Context, account_uuid: &str) {
    if let Some(path) = path(ctx, account_uuid) {
        let _ = std::fs::remove_file(path);
    }
}

/// How long an account lasts, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runway {
    /// Seconds until the tightest window fills at the rate it has been filling.
    Burning(i64),
    /// Nothing has been used lately, so the only thing that ends this window is its reset.
    Resting(i64),
    /// Not enough readings far enough apart to say anything. Better than a wrong number.
    Unknown,
}

impl Runway {
    /// One comparable number: how long this account lasts, in seconds. A rested account
    /// lasts until its window resets, which is when it becomes whole again.
    pub fn seconds(self) -> Option<i64> {
        match self {
            Runway::Burning(s) | Runway::Resting(s) => Some(s),
            Runway::Unknown => None,
        }
    }
}

/// Enough of a span to divide by, and enough points to believe. A rate taken from two
/// readings a minute apart is noise, and a wrong runway is worse than none: it tells
/// somebody to switch when they need not.
const ENOUGH_SPAN: i64 = 900;
const ENOUGH_POINTS: usize = 3;

/// How long this account lasts, from what its limits have been doing.
///
/// Per window: the rate it has been filling at, and how long until it is full at that rate,
/// or until it resets, whichever comes first. The account lasts as long as its tightest
/// window does.
///
/// A window that reset between two readings shows as filling backwards, which reads as not
/// burning rather than as a negative rate. That is also what a genuinely idle window looks
/// like, and both mean the same thing here.
pub fn runway(points: &[Point], now: i64) -> Runway {
    let recent: Vec<&Point> = points.iter().filter(|p| now - p.at <= KEEP_FOR).collect();
    if recent.len() < ENOUGH_POINTS {
        return Runway::Unknown;
    }
    let (first, last) = (recent[0], recent[recent.len() - 1]);
    let span = last.at - first.at;
    if span < ENOUGH_SPAN {
        return Runway::Unknown;
    }

    let mut shortest: Option<Runway> = None;
    for (kind, percent) in &last.windows {
        let was = first
            .windows
            .iter()
            .find(|(k, _)| k == kind)
            .map(|(_, p)| *p);
        let resets_in = last
            .resets
            .iter()
            .find(|(k, _)| k == kind)
            .map(|(_, at)| at - now)
            .filter(|left| *left > 0);

        let filling = was.map_or(0.0, |was| percent - was) / span as f64;
        let this = if filling > 0.0 && *percent < 100.0 {
            let until_full = ((100.0 - percent) / filling) as i64;
            match resets_in {
                Some(reset) if reset < until_full => Runway::Resting(reset),
                _ => Runway::Burning(until_full.max(0)),
            }
        } else {
            match resets_in {
                Some(reset) => Runway::Resting(reset),
                None => continue,
            }
        };
        if shortest
            .and_then(Runway::seconds)
            .is_none_or(|best| this.seconds().is_some_and(|s| s < best))
        {
            shortest = Some(this);
        }
    }
    shortest.unwrap_or(Runway::Unknown)
}

/// The same, for a whole account, read from disk.
pub fn runway_for(ctx: &Context, account_uuid: &str, now: i64) -> Runway {
    runway(&series(ctx, account_uuid), now)
}

use std::os::unix::fs::OpenOptionsExt;

#[cfg(test)]
mod tests {
    use super::*;

    /// A Codex account id carries the person inside the ChatGPT account, and its history
    /// has to be a file it can be kept in, or no Codex row ever says how long it lasts.
    #[test]
    fn a_real_codex_account_has_a_history() {
        let ctx = Context::new(std::path::PathBuf::from("/nowhere"))
            .with_pitboard_home(std::path::PathBuf::from("/nowhere/.pitboard"));
        assert!(
            path(
                &ctx,
                "8c3f0f86-0a7c-4d52-9b0e-1f2a3b4c5d6e_user-AbC123dEf456"
            )
            .is_some()
        );
    }
    use crate::time::{Clock, FixedClock};
    use crate::usage::{Source, Window};
    use std::sync::Arc;

    const NOW: i64 = 1_760_000_000;

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn machine(name: &str) -> (Context, Arc<FixedClock>, Scratch) {
        let root = std::env::temp_dir().join(format!(
            "pitboard-history-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let clock = Arc::new(FixedClock::at(NOW));
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root.clone())
            .with_clock(Arc::clone(&clock) as Arc<dyn Clock>);
        home::ensure(&ctx).expect("a home");
        (ctx, clock, Scratch(root))
    }

    fn reading(at: i64, five_hour: f64, resets_at: i64) -> Snapshot {
        Snapshot {
            observed_at: Some(at),
            account_uuid: None,
            source: Source::Live,
            windows: vec![Window {
                kind: "five_hour".into(),
                scope: None,
                percent: five_hour,
                resets_at: Some(resets_at),
                is_active: true,
                severity: None,
                length_seconds: None,
            }],
        }
    }

    fn points(samples: &[(i64, f64)], resets_at: i64) -> Vec<Point> {
        samples
            .iter()
            .map(|(at, percent)| Point {
                at: *at,
                windows: vec![("five_hour".into(), *percent)],
                resets: vec![("five_hour".into(), resets_at)],
            })
            .collect()
    }

    #[test]
    fn a_reading_is_kept_and_read_back() {
        let (ctx, _clock, _s) = machine("round-trip");
        record(&ctx, "acc", &reading(NOW, 10.0, NOW + 3600));
        record(&ctx, "acc", &reading(NOW + 600, 20.0, NOW + 3000));

        let kept = series(&ctx, "acc");
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].windows, vec![("five_hour".to_string(), 10.0)]);
        assert_eq!(kept[1].at, NOW + 600);
    }

    /// A machine running pitboard and a status line writes several readings an hour. One
    /// that says nothing the last one did not is not worth a line.
    #[test]
    fn a_reading_that_says_nothing_new_is_not_kept() {
        let (ctx, _clock, _s) = machine("quiet");
        record(&ctx, "acc", &reading(NOW, 10.0, NOW + 3600));
        record(&ctx, "acc", &reading(NOW + 10, 10.0, NOW + 3590));
        record(&ctx, "acc", &reading(NOW + 20, 10.2, NOW + 3580));
        assert_eq!(series(&ctx, "acc").len(), 1, "nothing moved");

        // A real move is kept however soon it comes.
        record(&ctx, "acc", &reading(NOW + 30, 11.0, NOW + 3570));
        assert_eq!(series(&ctx, "acc").len(), 2);

        // And so is a reading far enough from the last one.
        record(&ctx, "acc", &reading(NOW + 30 + APART, 11.0, NOW + 3000));
        assert_eq!(series(&ctx, "acc").len(), 3);
    }

    #[test]
    fn anything_older_than_a_fortnight_goes() {
        let (ctx, _clock, _s) = machine("pruned");
        for day in 0..20 {
            record(
                &ctx,
                "acc",
                &reading(NOW + day * 86_400, day as f64, NOW + day * 86_400 + 3600),
            );
        }
        let kept = series(&ctx, "acc");
        let newest = kept.last().expect("something").at;
        assert!(
            kept.iter().all(|p| newest - p.at <= KEEP_FOR),
            "kept {} readings, oldest {}s back",
            kept.len(),
            newest - kept[0].at
        );
        assert!(kept.len() < 20);
    }

    /// The question this exists to answer: how long does this account last.
    #[test]
    fn an_account_being_used_lasts_until_its_limit_fills() {
        // 20% to 50% over an hour is 30 points an hour, so the last 50 take 100 minutes.
        let series = points(
            &[(NOW, 20.0), (NOW + 1800, 35.0), (NOW + 3600, 50.0)],
            NOW + 86_400,
        );
        match runway(&series, NOW + 3600) {
            Runway::Burning(seconds) => {
                assert!(
                    (5_900..6_100).contains(&seconds),
                    "about a hundred minutes, got {seconds}"
                );
            }
            other => panic!("got {other:?}"),
        }
    }

    /// An account nobody is using lasts until its window resets, which is when it becomes
    /// whole again.
    #[test]
    fn an_account_nobody_is_using_lasts_until_its_window_resets() {
        let series = points(
            &[(NOW, 40.0), (NOW + 1800, 40.0), (NOW + 3600, 40.0)],
            NOW + 7200,
        );
        assert_eq!(runway(&series, NOW + 3600), Runway::Resting(3600));
    }

    /// A limit that resets before it could fill ends the window sooner, and that is the
    /// answer.
    #[test]
    fn a_reset_that_comes_first_is_what_the_account_lasts_until() {
        let series = points(
            &[(NOW, 20.0), (NOW + 1800, 25.0), (NOW + 3600, 30.0)],
            NOW + 4200,
        );
        assert_eq!(
            runway(&series, NOW + 3600),
            Runway::Resting(600),
            "it resets in ten minutes; it would take hours to fill"
        );
    }

    /// A wrong runway is worse than none: it tells somebody to switch when they need not.
    #[test]
    fn too_little_to_go_on_says_so_rather_than_guessing() {
        assert_eq!(runway(&[], NOW), Runway::Unknown);
        assert_eq!(
            runway(
                &points(&[(NOW, 10.0), (NOW + 600, 20.0)], NOW + 7200),
                NOW + 600
            ),
            Runway::Unknown,
            "two readings are not a rate"
        );
        assert_eq!(
            runway(
                &points(
                    &[(NOW, 10.0), (NOW + 60, 15.0), (NOW + 120, 20.0)],
                    NOW + 7200
                ),
                NOW + 120
            ),
            Runway::Unknown,
            "three readings two minutes apart are noise"
        );
    }

    /// A window that reset between two readings fills backwards. That is not a negative
    /// rate, it is a window that is no longer the one being measured.
    #[test]
    fn a_window_that_reset_in_the_middle_does_not_produce_a_nonsense_rate() {
        let series = points(
            &[(NOW, 90.0), (NOW + 1800, 95.0), (NOW + 3600, 5.0)],
            NOW + 7200,
        );
        assert_eq!(runway(&series, NOW + 3600), Runway::Resting(3600));
    }

    #[test]
    fn what_is_known_about_an_account_goes_when_the_account_does() {
        let (ctx, _clock, _s) = machine("forget");
        record(&ctx, "acc", &reading(NOW, 10.0, NOW + 3600));
        assert!(!series(&ctx, "acc").is_empty());
        forget(&ctx, "acc");
        assert!(series(&ctx, "acc").is_empty());
    }

    #[test]
    fn a_name_that_is_not_an_identifier_is_refused_rather_than_escaped() {
        let (ctx, _clock, _s) = machine("traversal");
        record(&ctx, "../../etc/passwd", &reading(NOW, 10.0, NOW + 3600));
        assert!(series(&ctx, "../../etc/passwd").is_empty());
        assert!(path(&ctx, "").is_none());
    }
}
