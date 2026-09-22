//! Time through jiff: epoch seconds in everything pitboard stores, the local time zone only
//! in what it shows a person.
//!
//! There is no free function that reads the clock. Everything that needs to know the time
//! asks its [`Context`](crate::context::Context), which holds a [`Clock`]. The expiry of a
//! parked login, whether a renewal is due, and how long doctor says is left are all
//! judgements about time, and none of them could be tested while the clock was a call into
//! the operating system made wherever it was needed.

use jiff::Timestamp;
use jiff::tz::TimeZone;

/// What pitboard reads the time from.
pub(crate) trait Clock: Send + Sync + std::fmt::Debug {
    /// Epoch seconds: what everything pitboard stores is measured in.
    fn now(&self) -> i64;

    /// Epoch milliseconds. Park names carry this, so two parks of one account in the same
    /// second do not collide.
    fn now_millis(&self) -> i64;
}

/// This machine's clock, which is what every real context uses.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> i64 {
        Timestamp::now().as_second()
    }

    fn now_millis(&self) -> i64 {
        Timestamp::now().as_millisecond()
    }
}

/// A clock that says what it is told, and can be moved. What the interesting judgements in
/// this crate are about is when something happens, so a test needs to say when.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub(crate) struct FixedClock(std::sync::atomic::AtomicI64);

#[cfg(any(test, feature = "test-support"))]
impl FixedClock {
    pub(crate) fn at(epoch_seconds: i64) -> FixedClock {
        FixedClock(std::sync::atomic::AtomicI64::new(epoch_seconds))
    }

    /// Move the clock forward, or back.
    pub(crate) fn advance(&self, seconds: i64) {
        self.0
            .fetch_add(seconds, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Clock for FixedClock {
    fn now(&self) -> i64 {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn now_millis(&self) -> i64 {
        self.now() * 1000
    }
}

/// An RFC 3339 instant such as `2026-09-20T22:20:00.095287+00:00`, as epoch seconds. `None`
/// for anything else, including a time without an offset, which names no instant.
pub fn parse(text: &str) -> Option<i64> {
    text.parse::<Timestamp>().ok().map(Timestamp::as_second)
}

/// `epoch` in this machine's time zone, formatted with `strftime` directives.
pub fn local(epoch: i64, pattern: &str) -> String {
    Timestamp::from_second(epoch)
        .map(|t| t.to_zoned(TimeZone::system()).strftime(pattern).to_string())
        .unwrap_or_default()
}

/// What a person reads for a moment: "14:02", or with the date once it is not today.
pub fn moment(epoch: i64, now: i64) -> String {
    let pattern = if local(epoch, "%F") == local(now, "%F") {
        "%H:%M"
    } else {
        "%b %-d %H:%M"
    };
    local(epoch, pattern)
}

/// A length of time to the precision a person reads: "6d 4h", "2h 05m", "47m", "<1m".
pub fn span(seconds: i64) -> String {
    let s = seconds.max(0);
    let (days, hours, minutes) = (s / 86_400, s % 86_400 / 3_600, s % 3_600 / 60);
    match (days, hours) {
        (0, 0) if minutes == 0 => "<1m".into(),
        (0, 0) => format!("{minutes}m"),
        (0, h) => format!("{h}h {minutes:02}m"),
        (d, h) => format!("{d}d {h}h"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_shapes_the_usage_api_actually_emits() {
        // Captured verbatim from a live /api/oauth/usage response.
        assert_eq!(parse("2026-09-20T22:20:00.095287+00:00"), Some(1789942800));
        assert_eq!(parse("2026-09-27T02:00:00.095306+00:00"), Some(1790474400));
        assert_eq!(parse("2026-09-20T22:20:00Z"), Some(1789942800));
        assert_eq!(parse("2026-09-21T05:20:00+07:00"), Some(1789942800));
    }

    #[test]
    fn refuses_rather_than_guesses() {
        for bad in [
            "",
            "not a date",
            "2026-09-20",
            "2026-09-20T22:20:00",
            "2026/09/20T22:20:00Z",
            "2026-13-01T00:00:00Z",
            "2026-01-32T00:00:00Z",
            "2026-01-01T24:00:00Z",
        ] {
            assert_eq!(parse(bad), None, "should refuse {bad:?}");
        }
    }

    #[test]
    fn spans_read_at_a_glance() {
        assert_eq!(span(-5), "<1m");
        assert_eq!(span(59), "<1m");
        assert_eq!(span(60 * 47), "47m");
        assert_eq!(span(3600 * 2 + 60 * 5), "2h 05m");
        assert_eq!(span(86_400 * 6 + 3600 * 4 + 59), "6d 4h");
    }

    #[test]
    fn a_moment_today_is_just_its_time() {
        let now = SystemClock.now();
        assert_eq!(moment(now, now).len(), 5);
        assert!(moment(now - 3 * 86_400, now).len() > 5);
    }

    #[test]
    fn formats_in_the_local_zone() {
        assert_eq!(local(0, "%Y").len(), 4);
        assert_eq!(
            local(i64::MAX, "%Y"),
            "",
            "out of range is empty, not a panic"
        );
    }
}
