//! Warming up an account: starting its next five-hour period as soon as the last one resets,
//! so that period resets as early as it can and no time between periods goes unused.
//!
//! A five-hour period starts with an account's first request after the last one reset, and
//! resets five hours after the start of the ten minutes that request fell in (the register's
//! `request_starts_the_five_hour_period`). An account nobody uses for a while after a
//! reset starts its next period only when somebody does, and that period resets that much
//! later. Where somebody asks for it, Pitboard starts the period at once instead, through
//! Claude Code itself: one short request with the account's own login, made by the tool's
//! own headless mode. Pitboard never sends a model request of its own.
//!
//! Asked for, account by account, and only within a span of each day somebody chose, 06:00
//! to 22:00 unless they chose otherwise, so periods run back to back through the hours
//! somebody works and none is started for a night nobody uses. The weekly limit is a fixed
//! cycle that runs whether or not anything is sent (`weekly_period_is_a_fixed_cycle`),
//! so there is nothing to start there. Claude Code only: when a Codex period starts is not
//! measured (docs/facts/not-measured.md).
//!
//! Which accounts, and the span, are kept in `warm.json` in Pitboard's directory, read by
//! every front end alike.

pub(crate) mod decide;
pub(crate) mod run;

pub use decide::{Hold, Standing};
pub use run::Ran;

use crate::context::Context;
use crate::error::{Error, Result};
use crate::permit::Permit;
use crate::provider::ProviderId;
use crate::state::State;
use crate::{atomic, home};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;

const DAY: u16 = 24 * 60;

/// A span of every day, in this machine's time zone, within which Pitboard warms up the
/// accounts somebody turned it on for. From `from` up to `to`, in minutes from midnight; a
/// span whose `from` is later than its `to` runs past midnight, and 00:00 to 24:00 is all day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    from: u16,
    to: u16,
}

impl Range {
    pub const DEFAULT: Range = Range {
        from: 6 * 60,
        to: 22 * 60,
    };

    /// `None` where the span is empty, or either end is not a time of day. `to` may be 24:00,
    /// and is where it is 00:00, as a clock shows the end of a day.
    pub fn new(from: u16, to: u16) -> Option<Range> {
        let to = if to == 0 { DAY } else { to };
        (from < DAY && to <= DAY && from != to).then_some(Range { from, to })
    }

    /// From two times of day as a person writes them, `06:00` and `22:00`.
    pub fn parse(from: &str, to: &str) -> std::result::Result<Range, String> {
        let from = minutes(from).filter(|&m| m < DAY);
        let to = minutes(to);
        match (from, to) {
            (Some(from), Some(to)) => Range::new(from, to)
                .ok_or_else(|| "the span starts and ends at the same time".to_owned()),
            _ => Err("give each time as hours and minutes, such as 06:00 or 22:30".to_owned()),
        }
    }

    pub fn from(self) -> u16 {
        self.from
    }

    pub fn to(self) -> u16 {
        self.to
    }

    /// `minute` of the day falls within it.
    pub(crate) fn contains(self, minute: u16) -> bool {
        if self.from < self.to {
            (self.from..self.to).contains(&minute)
        } else {
            minute >= self.from || minute < self.to
        }
    }

    /// Whether `now` falls within it, in `zone`.
    pub(crate) fn holds(self, now: i64, zone: &TimeZone) -> bool {
        local_minute(now, zone).is_some_and(|minute| self.contains(minute))
    }

    /// The next time it opens after `now`, in `zone`. On a day a clock change skips the
    /// opening time, it opens as the clock comes back to it, later that hour.
    pub(crate) fn opens_after(self, now: i64, zone: &TimeZone) -> Option<i64> {
        let today = Timestamp::from_second(now)
            .ok()?
            .to_zoned(zone.clone())
            .date();
        let at = |date: jiff::civil::Date| {
            date.at(
                i8::try_from(self.from / 60).ok()?,
                i8::try_from(self.from % 60).ok()?,
                0,
                0,
            )
            .to_zoned(zone.clone())
            .ok()
            .map(|zoned| zoned.timestamp().as_second())
        };
        at(today)
            .filter(|&opens| opens > now)
            .or_else(|| at(today.tomorrow().ok()?))
    }
}

impl Default for Range {
    fn default() -> Range {
        Range::DEFAULT
    }
}

impl std::fmt::Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} to {}", clock(self.from), clock(self.to))
    }
}

/// `06:30` as minutes from midnight. `24:00` is a day's end.
fn minutes(text: &str) -> Option<u16> {
    let (hours, minutes) = text.trim().split_once(':')?;
    let all_digits = |part: &str| part.len() == 2 && part.bytes().all(|b| b.is_ascii_digit());
    if !all_digits(hours) || !all_digits(minutes) {
        return None;
    }
    let (hours, minutes): (u16, u16) = (hours.parse().ok()?, minutes.parse().ok()?);
    let total = hours * 60 + minutes;
    (minutes < 60 && total <= DAY).then_some(total)
}

/// Minutes from midnight as a clock shows them, `06:30`.
pub fn clock(minutes: u16) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// The minute of the day `now` falls in, in `zone`.
fn local_minute(now: i64, zone: &TimeZone) -> Option<u16> {
    let zoned = Timestamp::from_second(now).ok()?.to_zoned(zone.clone());
    u16::try_from(i32::from(zoned.hour()) * 60 + i32::from(zoned.minute())).ok()
}

/// Why a warm-up that ran, or was about to, started no period.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Failure {
    /// A limit of the account is used up, and Anthropic refused the request.
    LimitReached,
    /// Anthropic refused the login's access token.
    LoginRefused,
    /// The request was answered, and Anthropic's next reading still has no period running.
    NotStarted,
    /// Anthropic could not be reached, or was overloaded.
    Unreachable,
    /// The tool's run did not finish in time.
    TimedOut,
    /// The tool's run failed, for this reason in its own words.
    Failed(String),
}

impl Failure {
    /// A stable code, for `--json` and for telling one apart from another.
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Failure::LimitReached => "warm_up_limit_reached",
            Failure::LoginRefused => "warm_up_login_refused",
            Failure::NotStarted => "warm_up_not_started",
            Failure::Unreachable => "warm_up_unreachable",
            Failure::TimedOut => "warm_up_timed_out",
            Failure::Failed(_) => "warm_up_failed",
        }
    }

    /// Whether waiting may mend it, so it is not counted against the day's attempts: a
    /// service out of reach, a run cut short, or a limit used up until it resets.
    pub(crate) fn mended_by_waiting(&self) -> bool {
        matches!(
            self,
            Failure::LimitReached | Failure::Unreachable | Failure::TimedOut
        )
    }
}

/// Which accounts Pitboard warms up, by account id, and within which span of each day.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Setting {
    pub(crate) range: Range,
    accounts: BTreeSet<String>,
}

impl Setting {
    /// Whether the account with `id` is warmed up.
    pub(crate) fn is_on(&self, id: &str) -> bool {
        self.accounts.contains(id)
    }

    /// Whether any account is.
    pub(crate) fn any(&self) -> bool {
        !self.accounts.is_empty()
    }

    /// Warm up the account with `id`, or stop. Whether that changed anything.
    pub(crate) fn turn(&mut self, id: &str, on: bool) -> bool {
        if on {
            self.accounts.insert(id.to_owned())
        } else {
            self.accounts.remove(id)
        }
    }
}

/// Why no account is warmed up on this machine for now, whatever each one's period says.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Halt {
    /// A switch was interrupted, and the change that finishes it is somebody's to make.
    SwitchInterrupted,
    /// Claude Code keeps its login under another name, which Pitboard does not act on.
    CustomOauth,
    /// Claude Code's managed settings or its remote API key file make a warm-up's run sign in
    /// another way, set by these, so it would send a request on that rather than on an
    /// account's login.
    Overridden(Vec<String>),
    /// Claude Code is not installed where Pitboard looks.
    ProgramMissing,
}

impl Halt {
    /// A stable code, for `--json` and for telling one apart from another.
    pub fn code(&self) -> &'static str {
        match self {
            Halt::SwitchInterrupted => "switch_interrupted",
            Halt::CustomOauth => "custom_oauth_endpoint",
            Halt::Overridden(_) => "warm_up_auth_overridden",
            Halt::ProgramMissing => "claude_not_found",
        }
    }
}

/// Why nothing is warmed up here now, where something says so, asked in the order a change
/// would refuse over it.
pub(crate) fn halted(ctx: &Context) -> Option<Halt> {
    if crate::switch::unfinished(ctx) == Some(crate::switch::Unfinished::Interrupted) {
        return Some(Halt::SwitchInterrupted);
    }
    if crate::settings::custom_oauth(ctx) {
        return Some(Halt::CustomOauth);
    }
    let overridden = overridden(ctx);
    if !overridden.is_empty() {
        return Some(Halt::Overridden(overridden));
    }
    crate::provider::claude_code()
        .program(ctx)
        .is_none()
        .then_some(Halt::ProgramMissing)
}

/// What would make a warm-up's run sign in with something other than the access token it is
/// handed: the managed settings every run of Claude Code here reads, anything in its own
/// config directory, or an API key in Claude Code's remote token file. Not this process's
/// environment, which the run is never given.
pub(crate) fn overridden(ctx: &Context) -> Vec<String> {
    let config = run::home(ctx, ProviderId::Claude).join("config");
    crate::settings::overrides_of_a_run_in(ctx, &config)
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// Each Claude Code account, whether Pitboard warms it up and where it stands, and the span,
/// for a front end to show, with why nothing is warmed up here where something says so.
#[derive(Debug, Clone, PartialEq)]
pub struct Overview {
    pub range: Range,
    pub accounts: Vec<Choice>,
    pub halted: Option<Halt>,
}

/// One Claude Code account, as it is typed, whether Pitboard warms it up, and where it stands
/// where it does. `now` is where it stands for a warm-up somebody asks for now, which the span
/// of the day and today's attempts do not hold back: `Due` where one would start its period.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub account: String,
    /// Its label with its tool, `claude/work`, as a front end asks for it.
    pub qualified: String,
    pub on: bool,
    pub standing: Option<Standing>,
    pub now: Standing,
}

impl Overview {
    /// The soonest moment a warm-up is due, of every account turned on, and that account:
    /// now where one is due.
    pub fn next(&self, now: i64) -> Option<(&str, i64)> {
        self.accounts
            .iter()
            .filter_map(|choice| {
                let at = match choice.standing.as_ref()? {
                    Standing::Due => now,
                    Standing::Running { next, .. } => (*next)?,
                    Standing::Resting { at } => *at,
                    Standing::Waiting { until } => *until,
                    Standing::Held(_) => return None,
                };
                Some((choice.account.as_str(), at))
            })
            .min_by_key(|(_, at)| *at)
    }
}

/// The Claude Code account whose login Claude Code has stored, as Pitboard last knew it.
pub(crate) fn in_use(ctx: &Context, state: &State) -> Option<crate::state::Key> {
    crate::in_use::known(ctx, state, ProviderId::Claude)
        .owner()
        .and_then(|owner| state.account_of(ProviderId::Claude, owner))
        .map(crate::state::Account::key)
}

/// What `setting` says of each Claude Code account `state` has, in the order they were
/// enrolled, and where each one turned on stands, from Pitboard's files alone.
pub(crate) fn overview(ctx: &Context, state: &State, setting: &Setting) -> Overview {
    let now = ctx.now();
    let zone = ctx.zone();
    let in_use = in_use(ctx, state);
    let readings = crate::usage::readings::load(ctx);
    let ledger = decide::Ledger::load(ctx);
    Overview {
        range: setting.range,
        halted: setting.any().then(|| halted(ctx)).flatten(),
        accounts: state
            .accounts
            .iter()
            .filter(|account| account.provider() == ProviderId::Claude)
            .map(|account| {
                let on = setting.is_on(&account.id);
                let standing = |asked| {
                    decide::decide(
                        decide::Login::of(account, in_use.as_ref(), now),
                        readings.get(&account.id),
                        ledger.get(&account.id),
                        setting.range,
                        asked,
                        now,
                        &zone,
                    )
                };
                Choice {
                    account: state.typed(&account.key()),
                    qualified: account.key().qualified(),
                    on,
                    standing: on.then(|| standing(decide::Asked::ByItself)),
                    now: standing(decide::Asked::Now),
                }
            })
            .collect(),
    }
}

/// What a warm-up would come to now, from Pitboard's files alone: no lock, no keychain and no
/// network, so a look that finds nothing due costs nobody anything. `Act` where an account
/// turned on is due, which only the decision under the lock says for certain.
pub(crate) fn look(ctx: &Context, state: &State) -> Result<crate::duty::Look<Overview>> {
    let setting = load(ctx)?;
    let overview = overview(ctx, state, &setting);
    let due = overview.halted.is_none()
        && overview
            .accounts
            .iter()
            .any(|choice| choice.standing == Some(Standing::Due));
    Ok(if due {
        crate::duty::Look::Act
    } else {
        crate::duty::Look::Stands(Box::new(overview))
    })
}

/// The format of `warm.json` this Pitboard writes.
const VERSION: u32 = 1;

/// `warm.json` as written.
#[derive(Debug, Serialize, Deserialize)]
struct Kept {
    version: u32,
    range: KeptRange,
    #[serde(default)]
    accounts: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct KeptRange {
    from: String,
    to: String,
}

pub(crate) fn path(ctx: &Context) -> PathBuf {
    home::dir(ctx).join("warm.json")
}

/// What `warm.json` says: nobody, and the default span, where it is not there.
pub(crate) fn load(ctx: &Context) -> Result<Setting> {
    let path = path(ctx);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Setting::default()),
        Err(source) => return Err(Error::WarmSettingUnreadable { path, source }),
    };
    let corrupt = |detail: String| Error::WarmSettingCorrupt {
        path: path.clone(),
        detail,
    };
    let version = serde_json::from_str::<serde_json::Value>(&raw)
        .map_err(|e| corrupt(e.to_string()))?
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| corrupt("it names no version".into()))?;
    if version > u64::from(VERSION) {
        return Err(Error::StateFromNewerVersion {
            path,
            found: u32::try_from(version).unwrap_or(u32::MAX),
            expected: VERSION,
        });
    }
    let kept: Kept = serde_json::from_str(&raw).map_err(|e| corrupt(e.to_string()))?;
    let range = Range::parse(&kept.range.from, &kept.range.to).map_err(corrupt)?;
    Ok(Setting {
        range,
        accounts: kept.accounts.into_iter().collect(),
    })
}

/// `setting`, written whole to `warm.json`. Only under the lock every change takes.
pub(crate) fn save(ctx: &Context, permit: Permit, setting: &Setting) -> Result<()> {
    let path = path(ctx);
    let kept = Kept {
        version: VERSION,
        range: KeptRange {
            from: clock(setting.range.from),
            to: clock(setting.range.to),
        },
        accounts: setting.accounts.iter().cloned().collect(),
    };
    let failed = |source| Error::StateWriteFailed {
        path: path.clone(),
        source,
    };
    let body = serde_json::to_vec(&kept).map_err(|e| failed(std::io::Error::other(e)))?;
    home::ensure(ctx, permit).map_err(failed)?;
    atomic::write(permit, &path, &body, atomic::Perms::Secret).map_err(failed)
}

/// Stop warming up an account nobody is enrolled as any more, and drop what was tried. A file
/// that cannot be read or written keeps the id, which names no account and so warms up
/// nothing.
pub(crate) fn forget(ctx: &Context, permit: Permit, id: &str) {
    if let Ok(mut setting) = load(ctx)
        && setting.turn(id, false)
    {
        let _ = save(ctx, permit, &setting);
    }
    let mut ledger = decide::Ledger::load(ctx);
    if ledger.forget(id) {
        let _ = ledger.save(ctx, permit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600;

    /// 2026-10-12 00:00 at UTC+7.
    const MIDNIGHT: i64 = 1_791_738_000;

    fn saigon() -> TimeZone {
        TimeZone::fixed(jiff::tz::offset(7))
    }

    #[test]
    fn a_span_reads_as_a_person_writes_it() {
        let range = Range::parse("06:00", "22:00").expect("a span");
        assert_eq!(range, Range::DEFAULT);
        assert_eq!(range.to_string(), "06:00 to 22:00");
        assert_eq!(
            Range::parse("00:00", "24:00").map(|r| (r.from(), r.to())),
            Ok((0, 1440))
        );
        assert!(Range::parse("22:00", "06:00").is_ok(), "past midnight");
        assert_eq!(
            Range::parse("00:00", "00:00"),
            Range::parse("00:00", "24:00")
        );
        assert_eq!(
            Range::parse("22:00", "00:00").map(|r| r.to_string()),
            Ok("22:00 to 24:00".to_owned())
        );
        for (from, to) in [
            ("06:00", "06:00"),
            ("24:00", "06:00"),
            ("6:00", "22:00"),
            ("06:60", "22:00"),
            ("06:00", "24:01"),
            ("noon", "22:00"),
        ] {
            assert!(Range::parse(from, to).is_err(), "{from} to {to}");
        }
    }

    #[test]
    fn a_span_holds_from_its_start_up_to_its_end() {
        let day = Range::DEFAULT;
        assert!(!day.contains(359));
        assert!(day.contains(360));
        assert!(day.contains(1319));
        assert!(!day.contains(1320));
        let night = Range::parse("22:00", "06:00").expect("a span");
        assert!(night.contains(1320));
        assert!(night.contains(0));
        assert!(night.contains(359));
        assert!(!night.contains(360));
        let all = Range::parse("00:00", "24:00").expect("a span");
        assert!(all.contains(0) && all.contains(1439));
    }

    #[test]
    fn a_span_opens_today_or_else_tomorrow_in_the_zone_given() {
        let zone = saigon();
        let day = Range::DEFAULT;
        assert!(!day.holds(MIDNIGHT + 5 * HOUR, &zone));
        assert!(day.holds(MIDNIGHT + 6 * HOUR, &zone));
        assert_eq!(
            day.opens_after(MIDNIGHT + 5 * HOUR, &zone),
            Some(MIDNIGHT + 6 * HOUR)
        );
        assert_eq!(
            day.opens_after(MIDNIGHT + 6 * HOUR, &zone),
            Some(MIDNIGHT + 30 * HOUR),
            "once open, the next opening is tomorrow's"
        );
        assert_eq!(
            day.opens_after(MIDNIGHT + 23 * HOUR, &zone),
            Some(MIDNIGHT + 30 * HOUR)
        );
    }

    #[test]
    fn a_span_skipped_by_a_clock_change_opens_as_the_clock_comes_back_to_it() {
        // 2026-03-08 in New York's rules: 02:00 does not happen; clocks go from 01:59 to 03:00.
        let zone = TimeZone::posix("EST5EDT,M3.2.0,M11.1.0").expect("a rule jiff reads");
        let range = Range::parse("02:30", "05:00").expect("a span");
        let before = jiff::civil::date(2026, 3, 8)
            .at(1, 0, 0, 0)
            .to_zoned(zone.clone())
            .expect("a time")
            .timestamp()
            .as_second();
        let opens = range.opens_after(before, &zone).expect("it opens");
        assert_eq!(opens - before, HOUR + 30 * 60, "03:30 by the clock");
    }
}
