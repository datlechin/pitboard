//! Time through jiff: epoch seconds in everything pitboard stores, the local time zone only
//! in what it shows a person.

use jiff::Timestamp;
use jiff::tz::TimeZone;

pub fn now() -> i64 {
    Timestamp::now().as_second()
}

/// Park names carry this, so two parks of one account in the same second do not collide.
pub fn now_millis() -> i64 {
    Timestamp::now().as_millisecond()
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
    fn formats_in_the_local_zone() {
        assert_eq!(local(0, "%Y").len(), 4);
        assert_eq!(local(i64::MAX, "%Y"), "", "out of range is empty, not a panic");
    }
}
