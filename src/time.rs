//! The narrow slice of time handling this tool needs.
//!
//! We parse one fixed shape (RFC 3339 as the usage API emits it) and format through
//! the platform's own `strftime`, rather than pulling in a date library for two calls.

use std::ffi::CStr;

/// Seconds since the epoch, now.
pub fn now() -> i64 {
    unsafe { libc::time(std::ptr::null_mut()) }
}

/// Milliseconds since the epoch. Park generations are named with this, because two
/// parks of one account inside the same second must not resolve to the same name.
pub fn now_millis() -> i64 {
    let mut tv = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    unsafe { libc::gettimeofday(&mut tv, std::ptr::null_mut()) };
    tv.tv_sec * 1_000 + tv.tv_usec as i64 / 1_000
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
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i32>().ok();
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = num(0..4)? - 1900;
    tm.tm_mon = num(5..7)? - 1;
    tm.tm_mday = num(8..10)?;
    tm.tm_hour = num(11..13)?;
    tm.tm_min = num(14..16)?;
    tm.tm_sec = num(17..19)?;

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
