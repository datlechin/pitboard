//! When an account is due a warm-up, from what Pitboard already knows: the reading Anthropic
//! last gave of it, whether a login of it is at hand, the span of the day, and what was tried
//! today. Asks nobody and writes nothing.

use super::Range;
use crate::duty::ledger::{self, Tries};
use crate::home;
use crate::permit::Permit;
use crate::state::{Account, Key};
use crate::usage::{self, Snapshot, Window};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A five-hour period, in seconds.
const PERIOD: i64 = 5 * 3_600;

/// A period starts at the start of the ten minutes its first request fell in, as measured
/// (the register's `request_starts_the_five_hour_period`).
const START_STEP: i64 = 600;

/// Where an account stands as far as warming it up goes.
#[derive(Debug, Clone, PartialEq)]
pub enum Standing {
    /// Its five-hour period runs until `until`, in epoch seconds. The next warm-up is due at
    /// `next`: as the period resets, or as the span opens after that.
    Running { until: i64, next: Option<i64> },
    /// Its period is not running, and the span opens at `at`.
    Resting { at: i64 },
    /// A warm-up is due now.
    Due,
    /// A warm-up of it came to nothing a moment ago, and the next is not made before `until`.
    Waiting { until: i64 },
    /// It is not warmed up for now, for this reason.
    Held(Hold),
}

/// Why an account is not warmed up for now.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Hold {
    /// Anthropic gave no reading of it yet, so nothing says whether its period runs.
    NoReading,
    /// Its plan has no five-hour limit, so there is no period to start.
    NothingToStart,
    /// `limit` is used up until it resets, and Anthropic refuses every request till then:
    /// a warm-up would start nothing.
    LimitUsedUp { limit: Window },
    /// As many warm-ups of it as are ever made today came to nothing, and none is made
    /// again before `until`, the start of tomorrow.
    GaveUp { until: i64 },
    /// Its parked login has expired, and only signing in again mends it.
    LoginExpired,
    /// It is not in use and has nothing parked, so there is no login of it to warm it up with.
    NothingParked,
}

impl Hold {
    /// When it ends, where that is known: the limit's reset, or the start of tomorrow.
    pub fn until(&self) -> Option<i64> {
        match self {
            Hold::LimitUsedUp { limit } => limit.resets_at,
            Hold::GaveUp { until } => Some(*until),
            Hold::NoReading | Hold::NothingToStart | Hold::LoginExpired | Hold::NothingParked => {
                None
            }
        }
    }

    /// What tells this apart from another, for a front end that says each once: its code, and
    /// the limit or the day it names. Not the limit's reset, which each answer can give a second
    /// apart.
    pub fn told_apart(&self) -> String {
        match self {
            Hold::LimitUsedUp { limit } => format!(
                "{}/{}/{}",
                self.code(),
                limit.kind,
                limit.scope.as_deref().unwrap_or_default()
            ),
            Hold::GaveUp { until } => format!("{}/{until}", self.code()),
            Hold::NoReading | Hold::NothingToStart | Hold::LoginExpired | Hold::NothingParked => {
                self.code().to_owned()
            }
        }
    }

    /// A stable code, for `--json` and for telling one apart from another.
    pub fn code(&self) -> &'static str {
        match self {
            Hold::NoReading => "no_reading",
            Hold::NothingToStart => "nothing_to_start",
            Hold::LimitUsedUp { .. } => "limit_used_up",
            Hold::GaveUp { .. } => "attempts_spent",
            Hold::LoginExpired => "parked_login_expired",
            Hold::NothingParked => "nothing_parked",
        }
    }
}

/// Which login of an account there is to warm it up with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Login {
    /// Claude Code's own, the account being in use.
    InUse,
    /// A parked login, and whether its refresh token still renews it.
    Parked {
        restorable: bool,
    },
    Nothing,
}

impl Login {
    /// Which login of `account` there is at `now`, where `in_use` is the account Claude Code
    /// has stored a login of, as Pitboard last knew.
    pub(crate) fn of(account: &Account, in_use: Option<&Key>, now: i64) -> Login {
        match &account.parked {
            Some(park) => Login::Parked {
                restorable: park.restorable_at(now),
            },
            None if in_use == Some(&account.key()) => Login::InUse,
            None => Login::Nothing,
        }
    }
}

/// What has been tried for one account today, and when a warm-up of it last started its
/// period, kept in `warmed.json` under the lock every change takes.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Warmed {
    /// The day its attempts are counted for, in this machine's time zone: `2026-10-12`.
    date: String,
    #[serde(flatten)]
    pub(crate) tries: Tries,
    /// When a warm-up of it last started its period, in epoch seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) started: Option<i64>,
}

impl Warmed {
    /// When the period the last warm-up started resets, where that may still be to come.
    fn started_until(&self, now: i64) -> Option<i64> {
        let started = self.started?;
        let until = started - started.rem_euclid(START_STEP) + PERIOD;
        (until > now).then_some(until)
    }
}

/// Every account's [`Warmed`], by account id.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Ledger {
    accounts: BTreeMap<String, Warmed>,
}

fn path(ctx: &crate::context::Context) -> PathBuf {
    home::dir(ctx).join("warmed.json")
}

/// `now`'s day in `zone`, as `2026-10-12`.
fn date(now: i64, zone: &TimeZone) -> String {
    Timestamp::from_second(now)
        .map(|t| t.to_zoned(zone.clone()).date().to_string())
        .unwrap_or_default()
}

/// The start of the day after `now`'s, in `zone`.
fn tomorrow(now: i64, zone: &TimeZone) -> i64 {
    Timestamp::from_second(now)
        .ok()
        .and_then(|t| t.to_zoned(zone.clone()).date().tomorrow().ok())
        .and_then(|day| day.to_zoned(zone.clone()).ok())
        .map_or(now + 86_400, |zoned| zoned.timestamp().as_second())
}

impl Ledger {
    /// What is kept, with each account's attempts counted afresh on a day after the one they
    /// were counted for, and without what no longer says anything.
    pub(crate) fn load(ctx: &crate::context::Context) -> Ledger {
        let mut kept: Ledger = ledger::load(&path(ctx));
        kept.prune(ctx.now(), &ctx.zone());
        kept
    }

    pub(crate) fn save(
        &mut self,
        ctx: &crate::context::Context,
        permit: Permit,
    ) -> crate::error::Result<()> {
        self.prune(ctx.now(), &ctx.zone());
        ledger::save(permit, &path(ctx), self)
    }

    fn prune(&mut self, now: i64, zone: &TimeZone) {
        let today = date(now, zone);
        self.accounts.retain(|_, warmed| {
            if warmed.date != today {
                warmed.date.clone_from(&today);
                warmed.tries = Tries::default();
            }
            warmed.tries != Tries::default() || warmed.started_until(now).is_some()
        });
    }

    /// What is kept of the account with `id`.
    pub(crate) fn get(&self, id: &str) -> Option<&Warmed> {
        self.accounts.get(id)
    }

    /// What is kept of the account with `id` today, begun afresh where nothing is.
    pub(crate) fn of(&mut self, id: &str, now: i64, zone: &TimeZone) -> &mut Warmed {
        let today = date(now, zone);
        let warmed = self.accounts.entry(id.to_owned()).or_default();
        if warmed.date != today {
            *warmed = Warmed {
                date: today,
                started: warmed.started,
                ..Warmed::default()
            };
        }
        warmed
    }

    /// Stop keeping anything of the account with `id`.
    pub(crate) fn forget(&mut self, id: &str) -> bool {
        self.accounts.remove(id).is_some()
    }
}

/// Whether a warm-up is one somebody asked for there and then, which the span of the day and
/// the attempts already spent today do not hold back, or one Pitboard makes by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Asked {
    Now,
    ByItself,
}

/// Where `account` stands, with `login` of it at hand, `reading` the last Anthropic gave of it,
/// `warmed` what was tried today, within `range`, at `now` in `zone`.
pub(crate) fn decide(
    login: Login,
    reading: Option<&Snapshot>,
    warmed: Option<&Warmed>,
    range: Range,
    asked: Asked,
    now: i64,
    zone: &TimeZone,
) -> Standing {
    match login {
        Login::Nothing => return Standing::Held(Hold::NothingParked),
        Login::Parked { restorable: false } => return Standing::Held(Hold::LoginExpired),
        Login::Parked { .. } | Login::InUse => {}
    }
    let Some(reading) = reading.filter(|reading| reading.answered()) else {
        return Standing::Held(Hold::NoReading);
    };
    let Some(session) = startable(reading) else {
        return Standing::Held(if reading.lists_every_limit {
            Hold::NothingToStart
        } else {
            Hold::NoReading
        });
    };
    let next = |until: i64| {
        if range.holds(until, zone) {
            Some(until)
        } else {
            range.opens_after(until, zone)
        }
    };
    if let Some(until) = session.resets_at.filter(|&at| at > now) {
        return Standing::Running {
            until,
            next: next(until),
        };
    }
    // A warm-up the last reading came before: it started a period that reading cannot show.
    let answered_at = reading.answered_at.unwrap_or_default();
    if let Some(until) = warmed
        .filter(|warmed| warmed.started.is_some_and(|at| at >= answered_at))
        .and_then(|warmed| warmed.started_until(now))
    {
        return Standing::Running {
            until,
            next: next(until),
        };
    }
    if let Some(limit) = used_up(reading, now) {
        return Standing::Held(Hold::LimitUsedUp { limit });
    }
    if asked == Asked::Now {
        return Standing::Due;
    }
    if !range.holds(now, zone) {
        return match range.opens_after(now, zone) {
            Some(at) => Standing::Resting { at },
            None => Standing::Held(Hold::NoReading),
        };
    }
    if let Some(tries) = warmed.map(|warmed| &warmed.tries) {
        if tries.spent() {
            return Standing::Held(Hold::GaveUp {
                until: tomorrow(now, zone),
            });
        }
        // After any attempt, counted or one waiting may mend, the next waits its turn.
        if now < tries.waits_until() {
            return Standing::Waiting {
                until: tries.waits_until(),
            };
        }
    }
    Standing::Due
}

/// The limit a warm-up starts: the account's own five-hour limit, not one of a model's.
pub(crate) fn startable(reading: &Snapshot) -> Option<&Window> {
    reading
        .windows
        .iter()
        .find(|window| usage::limit_name(&window.kind) == "session" && window.scope.is_none())
}

/// One of the account's own limits used up as of `now`, where one is: the one that resets
/// last, which is how long nothing could start a period. A model's limit holds back only that
/// model's requests, and a warm-up's is the smallest model's.
fn used_up(reading: &Snapshot, now: i64) -> Option<Window> {
    reading
        .windows
        .iter()
        .filter(|window| window.scope.is_none() && window.used_up(now))
        .max_by_key(|window| window.resets_at.unwrap_or(i64::MAX))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::Source;

    /// 2026-10-12 10:00 at UTC+7.
    const NOW: i64 = 1_791_774_000;
    const HOUR: i64 = 3_600;

    fn zone() -> TimeZone {
        TimeZone::fixed(jiff::tz::offset(7))
    }

    fn window(kind: &str, percent: f64, resets_at: Option<i64>) -> Window {
        Window {
            kind: kind.into(),
            scope: None,
            percent,
            resets_at,
            is_active: true,
            severity: None,
            length_seconds: None,
        }
    }

    fn answered(windows: Vec<Window>, at: i64) -> Snapshot {
        Snapshot {
            windows,
            observed_at: Some(at),
            answered_at: Some(at),
            lists_every_limit: true,
            source: Source::Live,
        }
    }

    /// The account's five-hour period not running, as Anthropic says it: no reset.
    fn idle() -> Snapshot {
        answered(
            vec![
                window("session", 0.0, None),
                window("weekly_all", 20.0, Some(NOW + 50 * HOUR)),
            ],
            NOW - 60,
        )
    }

    fn by_itself(reading: &Snapshot, warmed: Option<&Warmed>, now: i64) -> Standing {
        decide(
            Login::Parked { restorable: true },
            Some(reading),
            warmed,
            Range::DEFAULT,
            Asked::ByItself,
            now,
            &zone(),
        )
    }

    #[test]
    fn a_period_not_running_within_the_span_is_due() {
        assert_eq!(by_itself(&idle(), None, NOW), Standing::Due);
    }

    #[test]
    fn a_running_period_is_warmed_up_again_as_it_resets_or_as_the_span_opens() {
        let running = answered(vec![window("session", 30.0, Some(NOW + HOUR))], NOW - 60);
        assert_eq!(
            by_itself(&running, None, NOW),
            Standing::Running {
                until: NOW + HOUR,
                next: Some(NOW + HOUR)
            }
        );
        // Resetting at 23:00, after the span closes at 22:00: next at 06:00 tomorrow.
        let late = answered(vec![window("session", 30.0, Some(NOW + 13 * HOUR))], NOW);
        assert_eq!(
            by_itself(&late, None, NOW),
            Standing::Running {
                until: NOW + 13 * HOUR,
                next: Some(NOW + 20 * HOUR)
            }
        );
    }

    #[test]
    fn a_passed_reset_is_a_period_not_running() {
        let passed = answered(vec![window("session", 80.0, Some(NOW - 60))], NOW - HOUR);
        assert_eq!(by_itself(&passed, None, NOW), Standing::Due);
    }

    #[test]
    fn outside_the_span_it_waits_for_the_span_to_open() {
        let night = NOW + 13 * HOUR; // 23:00
        assert_eq!(
            by_itself(&idle(), None, night),
            Standing::Resting {
                at: NOW + 20 * HOUR
            }
        );
    }

    #[test]
    fn a_limit_used_up_holds_it_back_whatever_asks() {
        let out = answered(
            vec![
                window("session", 0.0, None),
                window("weekly_all", 100.0, Some(NOW + 40 * HOUR)),
            ],
            NOW - 60,
        );
        for asked in [Asked::ByItself, Asked::Now] {
            let standing = decide(
                Login::InUse,
                Some(&out),
                None,
                Range::DEFAULT,
                asked,
                NOW,
                &zone(),
            );
            assert!(
                matches!(&standing, Standing::Held(Hold::LimitUsedUp { limit }) if limit.kind == "weekly_all"),
                "{standing:?}"
            );
        }
    }

    #[test]
    fn a_model_limit_and_a_plan_without_a_five_hour_limit_start_nothing() {
        let mut fable = window("session", 0.0, None);
        fable.scope = Some("Fable".into());
        let team = answered(
            vec![fable, window("weekly_scoped", 0.0, Some(NOW + HOUR))],
            NOW,
        );
        assert_eq!(
            by_itself(&team, None, NOW),
            Standing::Held(Hold::NothingToStart)
        );
        let older_shape = Snapshot {
            lists_every_limit: false,
            ..team
        };
        assert_eq!(
            by_itself(&older_shape, None, NOW),
            Standing::Held(Hold::NoReading)
        );
    }

    #[test]
    fn no_reading_and_no_login_hold_it_back() {
        let unanswered = Snapshot {
            answered_at: None,
            ..idle()
        };
        assert_eq!(
            by_itself(&unanswered, None, NOW),
            Standing::Held(Hold::NoReading)
        );
        for (login, hold) in [
            (Login::Nothing, Hold::NothingParked),
            (Login::Parked { restorable: false }, Hold::LoginExpired),
        ] {
            let standing = decide(
                login,
                Some(&idle()),
                None,
                Range::DEFAULT,
                Asked::Now,
                NOW,
                &zone(),
            );
            assert_eq!(standing, Standing::Held(hold));
        }
    }

    #[test]
    fn an_attempt_waits_its_turn_and_three_failures_wait_for_tomorrow() {
        let mut warmed = Warmed::default();
        warmed.tries.attempt(NOW - 10);
        assert_eq!(
            by_itself(&idle(), Some(&warmed), NOW),
            Standing::Waiting { until: NOW + 50 }
        );
        assert_eq!(by_itself(&idle(), Some(&warmed), NOW + 50), Standing::Due);
        warmed.tries.attempt(NOW + 100);
        warmed.tries.attempt(NOW + 200);
        assert_eq!(
            by_itself(&idle(), Some(&warmed), NOW + 300),
            Standing::Held(Hold::GaveUp {
                until: NOW + 14 * HOUR
            })
        );
        let now = decide(
            Login::InUse,
            Some(&idle()),
            Some(&warmed),
            Range::DEFAULT,
            Asked::Now,
            NOW + 300,
            &zone(),
        );
        assert_eq!(now, Standing::Due, "asked for, it goes ahead");
    }

    /// A failure waiting may mend is not counted, but the next attempt still waits its turn,
    /// longer after each in a row.
    #[test]
    fn a_failure_waiting_may_mend_waits_its_turn_too() {
        let mut warmed = Warmed::default();
        warmed.tries.attempt(NOW);
        warmed.tries.waited();
        assert_eq!(
            by_itself(&idle(), Some(&warmed), NOW + 59),
            Standing::Waiting { until: NOW + 60 }
        );
        warmed.tries.attempt(NOW + 60);
        warmed.tries.waited();
        assert_eq!(
            by_itself(&idle(), Some(&warmed), NOW + 100),
            Standing::Waiting { until: NOW + 180 }
        );
        assert_eq!(by_itself(&idle(), Some(&warmed), NOW + 180), Standing::Due);
    }

    /// A model's own limit used up holds back that model alone, and a warm-up runs on the
    /// smallest.
    #[test]
    fn a_models_limit_used_up_holds_nothing_back() {
        let mut fable = window("weekly_scoped", 100.0, Some(NOW + 40 * HOUR));
        fable.scope = Some("Fable".into());
        let reading = answered(vec![window("session", 0.0, None), fable], NOW - 60);
        assert_eq!(by_itself(&reading, None, NOW), Standing::Due);
    }

    /// A warm-up the last reading did not see started a period at the start of the ten
    /// minutes it fell in, which runs five hours from there.
    #[test]
    fn a_warm_up_since_the_last_reading_counts_as_a_running_period() {
        let warmed = Warmed {
            started: Some(NOW + 125),
            ..Warmed::default()
        };
        assert_eq!(
            by_itself(&idle(), Some(&warmed), NOW + 200),
            Standing::Running {
                until: NOW + 5 * HOUR,
                next: Some(NOW + 5 * HOUR)
            }
        );
        let seen_after = answered(vec![window("session", 0.0, None)], NOW + 300);
        assert_eq!(
            by_itself(&seen_after, Some(&warmed), NOW + 400),
            Standing::Due,
            "a reading taken after it, and still not running, says it started nothing"
        );
    }

    #[test]
    fn attempts_are_counted_afresh_each_day() {
        let zone = zone();
        let mut ledger = Ledger::default();
        for at in 0..3 {
            ledger.of("a", NOW + at, &zone).tries.attempt(NOW + at);
        }
        assert!(ledger.get("a").is_some_and(|w| w.tries.spent()));
        ledger.prune(NOW + 15 * HOUR, &zone);
        assert!(ledger.get("a").is_none(), "nothing left to say tomorrow");

        ledger.of("b", NOW, &zone).started = Some(NOW);
        ledger.of("b", NOW, &zone).tries.attempt(NOW);
        ledger.prune(NOW + 14 * HOUR + 60, &zone);
        assert!(ledger.get("b").is_none(), "its period reset by then");
        ledger.of("c", NOW + 13 * HOUR, &zone).started = Some(NOW + 13 * HOUR);
        ledger.prune(NOW + 14 * HOUR + 60, &zone);
        let c = ledger.get("c").expect("its period still runs");
        assert_eq!(c.tries, Tries::default());
    }
}
