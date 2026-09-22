//! How often pitboard is allowed to ask Anthropic about an account.
//!
//! `status` asked about every enrolled account plus the live login on every run, with no
//! memory of having just asked, and the menu bar app asked the same questions every five
//! minutes, on every wake, and on every panel open, from a process that knew nothing about
//! the command line's. Nothing honoured `Retry-After`: a 429 became a stale row and the
//! identical request went out on the next tick. Two accounts and a running app is on the
//! order of six hundred authenticated requests a day that nobody asked for.
//!
//! Beyond the cost, that is the part of pitboard's behaviour that reads least like a person
//! switching between their own accounts, which is the one appearance this project cannot
//! afford.
//!
//! So there is a floor, and it is derived rather than chosen: an account may be asked again
//! once its tightest limit could have moved by one percentage point. A five-hour window
//! moves at most 100% in five hours, so that is three minutes; a weekly window, a hundred.
//! Whatever the floor is, it is far below what changes anybody's decision, and it collapses
//! the repeated asking to one request per account per few minutes however many front ends
//! are running.
//!
//! Nothing here is secret. It is names, times and counts.

use crate::context::Context;
use crate::usage::Snapshot;
use crate::{atomic, home};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// How long each window kind covers, which is what makes the floor a derivation rather
/// than a preference. Measured from the reading itself where the reading says; these are
/// the fallbacks for a kind whose reset time is missing.
fn window_seconds(kind: &str) -> Option<i64> {
    match kind {
        "five_hour" => Some(5 * 3600),
        "seven_day" => Some(7 * 86_400),
        _ => None,
    }
}

/// One percentage point of the tightest window this account has, in seconds. The floor.
///
/// With no reading to derive it from, a minute: enough to collapse a burst of front ends
/// starting together, short enough that nobody notices.
const UNKNOWN_FLOOR: i64 = 60;

pub fn floor_for(reading: Option<&Snapshot>) -> i64 {
    let shortest = reading
        .map(|s| s.windows.as_slice())
        .unwrap_or_default()
        .iter()
        .filter_map(|w| window_seconds(&w.kind))
        .min();
    shortest.map_or(UNKNOWN_FLOOR, |seconds| (seconds / 100).max(1))
}

/// Anthropic asked for less traffic, or could not be reached. Both mean waiting, and how
/// long is the difference between them.
const RATE_LIMITED_FIRST: i64 = 60;
const RATE_LIMITED_MOST: i64 = 3600;
const UNREACHABLE_FIRST: i64 = 30;
const UNREACHABLE_MOST: i64 = 900;

/// What is known about asking one account.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Record {
    /// When Anthropic last answered about this account, in epoch seconds.
    #[serde(default)]
    pub answered_at: i64,
    /// Not before this, in epoch seconds. Set by a refusal, cleared by an answer.
    #[serde(default)]
    pub held_until: i64,
    /// Refusals in a row, which is what makes the wait grow.
    #[serde(default)]
    pub refusals: u32,
}

type Ledger = HashMap<String, Record>;

fn path(ctx: &Context) -> PathBuf {
    home::dir(ctx).join("asking.json")
}

fn load(ctx: &Context) -> Ledger {
    std::fs::read_to_string(path(ctx))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save(ctx: &Context, ledger: &Ledger) {
    if let Ok(body) = serde_json::to_string(ledger) {
        let _ = atomic::write(&path(ctx), body.as_bytes(), atomic::Perms::Secret);
    }
}

/// Why an account is not being asked about right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    /// Asked recently enough that the answer cannot have moved by a point.
    Fresh,
    /// Anthropic asked for less traffic.
    RateLimited,
    /// Anthropic could not be reached, and trying again at once helps nobody.
    Unreachable,
}

/// Whether to ask about this account now.
///
/// `None` means ask. `Some(held)` means do not, and says which of the three reasons it is,
/// so a front end can show the right thing and a person can tell a quiet answer from a
/// refused one.
pub fn may_ask(
    ctx: &Context,
    account_uuid: &str,
    last_reading: Option<&Snapshot>,
    forced: bool,
) -> Option<Held> {
    if forced {
        return None;
    }
    let ledger = load(ctx);
    let record = ledger.get(account_uuid)?;
    let now = ctx.now();
    if record.held_until > now {
        return Some(
            if record.refusals > 0 && record.held_until - record.answered_at > UNREACHABLE_MOST {
                Held::RateLimited
            } else {
                Held::Unreachable
            },
        );
    }
    (now - record.answered_at < floor_for(last_reading)).then_some(Held::Fresh)
}

/// What one request turned out to be, for recording afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Answered,
    /// Anthropic asked for less traffic. `Some` is what it said to wait, in seconds.
    RateLimited(Option<i64>),
    /// Anthropic could not be reached, or answered in a way that trying again might fix.
    Unreachable,
}

/// Record what came back, for one account or for several.
///
/// Several at a time on purpose. Every front end asks about all its accounts at once, on a
/// thread each, and a record-as-you-go would have each thread read this file, change one
/// entry and write the whole thing back, so the last writer would erase what the others
/// learned. Which is exactly what happened: two accounts asked together, one budgeted and
/// one not, forever.
pub fn record(ctx: &Context, outcomes: &[(String, Outcome)]) {
    if outcomes.is_empty() {
        return;
    }
    let now = ctx.now();
    let mut ledger = load(ctx);
    for (account_uuid, outcome) in outcomes {
        let record = ledger.entry(account_uuid.clone()).or_default();
        match outcome {
            Outcome::Answered => {
                record.answered_at = now;
                record.held_until = 0;
                record.refusals = 0;
            }
            Outcome::RateLimited(retry_after) => {
                hold(
                    record,
                    now,
                    RATE_LIMITED_FIRST,
                    RATE_LIMITED_MOST,
                    *retry_after,
                );
            }
            Outcome::Unreachable => {
                hold(record, now, UNREACHABLE_FIRST, UNREACHABLE_MOST, None);
            }
        }
    }
    save(ctx, &ledger);
}

/// `retry_after` is what Anthropic said to wait, where it said anything, and is believed
/// over anything pitboard would pick.
fn hold(record: &mut Record, now: i64, first: i64, most: i64, retry_after: Option<i64>) {
    record.refusals = record.refusals.saturating_add(1);
    let backoff = retry_after
        .filter(|seconds| *seconds > 0)
        .unwrap_or_else(|| doubling(first, most, record.refusals - 1));
    record.held_until = now + backoff;
}

/// Doubling from `first`, capped at `most`. `before` is how many refusals came before this
/// one, so the first wait is `first` itself.
///
/// No jitter, because these waits are per account and per machine: there is no herd here to
/// spread out, and a jittered wait is one a person cannot predict from what doctor told
/// them.
fn doubling(first: i64, most: i64, before: u32) -> i64 {
    let shifted = first.saturating_mul(1i64 << before.min(16));
    shifted.clamp(first, most)
}

/// Every account currently being held off, for `doctor` to report.
pub fn holds(ctx: &Context) -> Vec<(String, i64)> {
    let now = ctx.now();
    let mut held: Vec<(String, i64)> = load(ctx)
        .into_iter()
        .filter(|(_, r)| r.held_until > now)
        .map(|(uuid, r)| (uuid, r.held_until - now))
        .collect();
    held.sort();
    held
}

/// Drop what is known about an account nobody is enrolled as any more.
pub fn forget(ctx: &Context, account_uuid: &str) {
    let mut ledger = load(ctx);
    if ledger.remove(account_uuid).is_some() {
        save(ctx, &ledger);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            "pitboard-budget-{name}-{}-{:?}",
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

    fn reading(kinds: &[&str]) -> Snapshot {
        Snapshot {
            observed_at: Some(NOW),
            account_uuid: None,
            source: Source::Live,
            windows: kinds
                .iter()
                .map(|kind| Window {
                    kind: (*kind).to_string(),
                    scope: None,
                    percent: 10.0,
                    resets_at: None,
                    is_active: true,
                    severity: None,
                })
                .collect(),
        }
    }

    /// The floor is a derivation, not a preference: how long the tightest limit takes to
    /// move by one percentage point.
    #[test]
    fn the_floor_comes_from_the_window_rather_than_from_taste() {
        assert_eq!(floor_for(Some(&reading(&["five_hour"]))), 180);
        assert_eq!(floor_for(Some(&reading(&["seven_day"]))), 6048);
        assert_eq!(
            floor_for(Some(&reading(&["seven_day", "five_hour"]))),
            180,
            "the tightest window is the one that decides"
        );
        assert_eq!(floor_for(None), UNKNOWN_FLOOR);
        assert_eq!(
            floor_for(Some(&reading(&["something_new"]))),
            UNKNOWN_FLOOR,
            "a window kind pitboard does not know is not a reason to ask forever"
        );
    }

    #[test]
    fn an_account_nobody_has_asked_about_is_asked_about() {
        let (ctx, _clock, _s) = machine("first");
        assert_eq!(may_ask(&ctx, "acc", None, false), None);
    }

    #[test]
    fn asking_again_inside_the_floor_serves_what_is_already_known() {
        let (ctx, clock, _s) = machine("floor");
        let five_hour = reading(&["five_hour"]);
        record(&ctx, &[("acc".into(), Outcome::Answered)]);

        assert_eq!(
            may_ask(&ctx, "acc", Some(&five_hour), false),
            Some(Held::Fresh)
        );
        clock.advance(179);
        assert_eq!(
            may_ask(&ctx, "acc", Some(&five_hour), false),
            Some(Held::Fresh)
        );
        clock.advance(2);
        assert_eq!(may_ask(&ctx, "acc", Some(&five_hour), false), None);
    }

    #[test]
    fn asking_for_it_is_always_allowed() {
        let (ctx, _clock, _s) = machine("forced");
        record(&ctx, &[("acc".into(), Outcome::Answered)]);
        assert_eq!(
            may_ask(&ctx, "acc", Some(&reading(&["five_hour"])), true),
            None
        );
    }

    /// What Anthropic said to wait is believed over anything pitboard would pick.
    #[test]
    fn a_retry_after_is_taken_at_its_word() {
        let (ctx, clock, _s) = machine("retry-after");
        record(&ctx, &[("acc".into(), Outcome::RateLimited(Some(300)))]);

        assert!(may_ask(&ctx, "acc", None, false).is_some());
        clock.advance(299);
        assert!(may_ask(&ctx, "acc", None, false).is_some());
        clock.advance(2);
        assert_eq!(may_ask(&ctx, "acc", None, false), None);
    }

    #[test]
    fn refusals_in_a_row_wait_longer_each_time_up_to_a_cap() {
        let (ctx, clock, _s) = machine("doubling");
        let waits: Vec<i64> = (0..8)
            .map(|_| {
                record(&ctx, &[("acc".into(), Outcome::RateLimited(None))]);
                let held = holds(&ctx);
                clock.advance(held.first().map_or(0, |(_, left)| *left));
                held.first().map_or(0, |(_, left)| *left)
            })
            .collect();
        assert_eq!(waits[0], RATE_LIMITED_FIRST);
        assert!(waits[1] > waits[0]);
        assert!(
            waits.iter().all(|w| *w <= RATE_LIMITED_MOST),
            "the wait has a ceiling: {waits:?}"
        );
        assert_eq!(*waits.last().expect("eight of them"), RATE_LIMITED_MOST);
    }

    #[test]
    fn an_answer_clears_a_wait_and_starts_the_floor_again() {
        let (ctx, _clock, _s) = machine("cleared");
        record(&ctx, &[("acc".into(), Outcome::RateLimited(Some(3600)))]);
        assert!(!holds(&ctx).is_empty());

        record(&ctx, &[("acc".into(), Outcome::Answered)]);
        assert!(holds(&ctx).is_empty());
        assert_eq!(may_ask(&ctx, "acc", None, false), Some(Held::Fresh));
    }

    #[test]
    fn what_is_known_about_an_account_goes_when_the_account_does() {
        let (ctx, _clock, _s) = machine("forget");
        record(&ctx, &[("acc".into(), Outcome::RateLimited(Some(3600)))]);
        forget(&ctx, "acc");
        assert!(holds(&ctx).is_empty());
        assert_eq!(may_ask(&ctx, "acc", None, false), None);
    }
}
