//! The narrow slice of time handling this tool needs.
//!
//! We parse one fixed shape (RFC 3339 as the usage API emits it) and format through
//! the platform's own `strftime`, rather than pulling in a date library for two calls.

use std::ffi::CStr;

/// Seconds since the epoch, now.
pub fn now() -> i64 {
    since_epoch().as_secs() as i64
}

/// Milliseconds since the epoch. Park generations are named with this, because two
/// parks of one account inside the same second must not resolve to the same name.
/// Park generations are named with this: two parks of one account inside the same second
/// must not resolve to the same name.
pub fn now_millis() -> i64 {
    since_epoch().as_millis() as i64
}

fn since_epoch() -> std::time::Duration {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
}

/// Parse `2026-09-20T22:20:00.095287+00:00` (and the `Z` spelling) to epoch seconds.
///
/// Returns `None` rather than guessing when the shape is not what we expect: a wrong
/// number rendered confidently is worse than no number.
pub fn parse_rfc3339(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    // `timegm` normalises out-of-range fields rather than rejecting them, so each one is
    // checked here: a confidently wrong timestamp is worse than no timestamp.
    let field = |r: std::ops::Range<usize>, min: i32, max: i32| -> Option<i32> {
        let v = s.get(r)?.parse::<i32>().ok()?;
        (min..=max).contains(&v).then_some(v)
    };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = field(0..4, 1970, 9999)? - 1900;
    tm.tm_mon = field(5..7, 1, 12)? - 1;
    tm.tm_mday = field(8..10, 1, 31)?;
    tm.tm_hour = field(11..13, 0, 23)?;
    tm.tm_min = field(14..16, 0, 59)?;
    tm.tm_sec = field(17..19, 0, 60)?;

    // Offset: trailing `Z`, or `+HH:MM` / `-HH:MM` after any fractional seconds.
    let tail = &s[19..];
    let offset = if tail.ends_with('Z') || tail.is_empty() {
        0
    } else {
        let sign_at = tail.rfind(['+', '-'])?;
        let (sign, hm) = tail.split_at(sign_at);
        let _ = sign;
        let bytes = hm.as_bytes();
        if bytes.len() < 6 || bytes[3] != b':' {
            return None;
        }
        let h: i64 = hm.get(1..3)?.parse().ok()?;
        let m: i64 = hm.get(4..6)?.parse().ok()?;
        let magnitude = h * 3600 + m * 60;
        if bytes[0] == b'-' {
            -magnitude
        } else {
            magnitude
        }
    };
    let utc = unsafe { libc::timegm(&mut tm) };
    Some(utc as i64 - offset)
}

/// Format an epoch as local time with the given `strftime` pattern.
pub fn format_local(epoch: i64, pattern: &str) -> String {
    let t = epoch as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let mut buf = [0u8; 128];
    let pat = format!("{pattern}\0");
    unsafe {
        if libc::localtime_r(&t, &mut tm).is_null() {
            return String::new();
        }
        let n = libc::strftime(
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            pat.as_ptr() as *const libc::c_char,
            &tm,
        );
        if n == 0 {
            return String::new();
        }
        CStr::from_ptr(buf.as_ptr() as *const libc::c_char)
            .to_string_lossy()
            .into_owned()
    }
}

/// "in 6d 4h", "in 2h 14m", "in 47m", "now" — the only phrasing the UI needs.
pub fn humanise_until(target: i64, from: i64) -> String {
    let d = target - from;
    if d <= 0 {
        return "now".into();
    }
    let (days, hours, mins) = (d / 86_400, (d % 86_400) / 3600, (d % 3600) / 60);
    match (days, hours) {
        (0, 0) => format!("in {mins}m"),
        (0, h) => format!("in {h}h {mins:02}m"),
        (dd, h) => format!("in {dd}d {h}h"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_shapes_the_usage_api_actually_emits() {
        // Captured verbatim from a live /api/oauth/usage response.
        assert_eq!(
            parse_rfc3339("2026-09-20T22:20:00.095287+00:00"),
            Some(1789942800)
        );
        assert_eq!(
            parse_rfc3339("2026-09-27T02:00:00.095306+00:00"),
            Some(1790474400)
        );
        assert_eq!(parse_rfc3339("2026-09-20T22:20:00Z"), Some(1789942800));
    }

    #[test]
    fn honours_a_non_utc_offset() {
        // Same instant, written in Asia/Saigon.
        assert_eq!(parse_rfc3339("2026-09-21T05:20:00+07:00"), Some(1789942800));
    }

    /// `timegm` normalises out-of-range fields instead of rejecting them, so without an
    /// explicit range check a nonsense timestamp silently becomes a confident wrong answer.
    #[test]
    fn out_of_range_fields_are_refused_not_normalised() {
        for bad in [
            "2026-13-01T00:00:00Z",
            "2026-00-01T00:00:00Z",
            "2026-01-32T00:00:00Z",
            "2026-01-00T00:00:00Z",
            "2026-01-01T24:00:00Z",
            "2026-01-01T00:60:00Z",
            "2026-01-01T00:00:61Z",
            "2026-13-40T25:99:99Z",
        ] {
            assert_eq!(parse_rfc3339(bad), None, "{bad} should be refused");
        }
        assert!(
            parse_rfc3339("2026-12-31T23:59:60Z").is_some(),
            "a leap second is real"
        );
    }

    #[test]
    fn refuses_rather_than_guesses() {
        for bad in [
            "",
            "not a date",
            "2026-09-20",
            "2026/09/20T22:20:00Z",
            "2026-09-20T22:20",
        ] {
            assert_eq!(parse_rfc3339(bad), None, "should refuse {bad:?}");
        }
    }

    #[test]
    fn humanises_durations() {
        assert_eq!(humanise_until(100, 100), "now");
        assert_eq!(humanise_until(100, 200), "now");
        assert_eq!(humanise_until(60 * 47, 0), "in 47m");
        assert_eq!(humanise_until(3600 * 2 + 60 * 14, 0), "in 2h 14m");
        assert_eq!(humanise_until(86_400 * 6 + 3600 * 4, 0), "in 6d 4h");
    }
}
