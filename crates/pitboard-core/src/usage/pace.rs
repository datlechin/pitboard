//! How fast a limit is being used, against using it evenly across its window.
//!
//! A share on its own does not say whether it is a problem: 40% of a weekly limit is fine on
//! Saturday and a warning on Monday morning. What does is the share an even use would have
//! reached by then, which is the part of the window gone by, and how far the share used is
//! from it. Every surface asks this module, so the command line, the status line, the app
//! and `--json` say the same thing.
//!
//! Worked out from one reading: the share used, when it was taken, when the window resets,
//! and how long it runs. The pace is the reading's, as of when it was taken: the share it
//! holds was the share then, and set against a later moment it would drift towards under
//! pace for as long as nobody read the limit again, hiding a warning just when the numbers
//! are old. The rate is the window's own since it began, which is the share used over the
//! time gone by, so a limit faster than even always runs out before it resets, counted
//! down to the moment it is told, and one slower always lasts. It used to be a rate taken
//! across fourteen days of readings, through every reset in them, and on one machine it
//! said a weekly limit at 99% had a day and nine hours left when at its own rate it had one.

use crate::usage::Window;

/// How many points either side of an even pace still count as even: close enough that the
/// difference is noise early in a five-hour window, and far enough that a limit really
/// running ahead is said.
pub(crate) const MARGIN: f64 = 5.0;

/// The part of a window that has to have gone by before its pace means anything. A few
/// minutes into a five-hour window one request is a large share of the time gone by.
pub(crate) const TOO_EARLY: f64 = 0.03;

/// How a limit's use compared with an even pace when it was read.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Pace {
    /// The share an even use would have reached when the limit was read, in percent of the
    /// limit: the part of the window gone by then.
    pub expected: f64,
    /// The share used less `expected`, in percentage points. Above zero is faster than an
    /// even pace.
    pub delta: f64,
    #[serde(flatten)]
    pub standing: Standing,
}

/// Which side of an even pace a limit is on, beyond `MARGIN`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "standing", rename_all = "snake_case")]
pub enum Standing {
    /// Slower than even: at its own rate the limit lasts until it resets.
    Under,
    /// Within `MARGIN` of even.
    Even,
    /// Faster than even: at its own rate the limit runs out this many seconds from the
    /// moment this is told, before it resets.
    Over { runs_out_in: i64 },
}

/// The pace of a limit read at `at` with `used` percent used, whose window of
/// `length_seconds` resets at `resets_at`, told at `now`. A reading taken later than `now`,
/// by a clock ahead of this one, counts as taken now.
///
/// `None` where it means nothing: a window whose length or reset is not known, one that
/// has reset since, or that cannot have begun when it was read, one used up, which its
/// share says already, and one read less than `TOO_EARLY` in.
pub fn of(
    used: f64,
    resets_at: Option<i64>,
    length_seconds: Option<i64>,
    at: i64,
    now: i64,
) -> Option<Pace> {
    let length = length_seconds.filter(|length| *length > 0)?;
    let resets_at = resets_at?;
    let at = at.min(now);
    let left = resets_at - at;
    if resets_at <= now || left > length || used >= 100.0 {
        return None;
    }
    let gone = length - left;
    let part = gone as f64 / length as f64;
    if part < TOO_EARLY {
        return None;
    }
    let used = used.max(0.0);
    let expected = part * 100.0;
    let delta = used - expected;
    let standing = if delta > MARGIN {
        // What is left, at the share used per second gone by, less the time since the
        // reading. Past MARGIN the share used is above zero, and the time this gives is
        // short of the reset.
        let from_reading = ((100.0 - used) * gone as f64 / used) as i64;
        Standing::Over {
            runs_out_in: (from_reading - (now - at)).max(0),
        }
    } else if delta < -MARGIN {
        Standing::Under
    } else {
        Standing::Even
    };
    Some(Pace {
        expected,
        delta,
        standing,
    })
}

impl Window {
    /// This limit's pace as read at `at`, told at `now`.
    pub fn pace(&self, at: i64, now: i64) -> Option<Pace> {
        of(self.percent, self.resets_at, self.length_seconds, at, now)
    }
}

/// The limit that runs out first at its pace, and in how many seconds, of `limits` each
/// with its pace. `None` where none runs out before it resets.
pub fn first_to_run_out<T>(
    limits: impl IntoIterator<Item = (T, Option<Pace>)>,
) -> Option<(T, i64)> {
    limits
        .into_iter()
        .filter_map(|(limit, pace)| match pace?.standing {
            Standing::Over { runs_out_in } => Some((limit, runs_out_in)),
            Standing::Under | Standing::Even => None,
        })
        .min_by_key(|(_, runs_out_in)| *runs_out_in)
}

/// How long an account lasts, from its limits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Lasts<'a> {
    /// This limit runs out first, at its pace, this many seconds from now.
    RunsOut { limit: &'a Window, in_seconds: i64 },
    /// No limit runs out before it resets, and the first reset is this many seconds from now.
    UntilReset { in_seconds: i64 },
}

impl Lasts<'_> {
    pub fn seconds(self) -> i64 {
        match self {
            Lasts::RunsOut { in_seconds, .. } | Lasts::UntilReset { in_seconds } => in_seconds,
        }
    }
}

/// How long an account in use with `windows`, read at `at`, lasts as of `now`: until the
/// first of them to run out at its pace does, or where none does, until the first of them
/// resets. `None` where no window says when it resets.
pub(crate) fn lasts(windows: &[Window], at: i64, now: i64) -> Option<Lasts<'_>> {
    let paced = windows.iter().map(|w| (w, w.pace(at, now)));
    if let Some((limit, in_seconds)) = first_to_run_out(paced) {
        return Some(Lasts::RunsOut { limit, in_seconds });
    }
    until_reset(windows, now)
}

/// How long an account not in use with `windows` lasts as of `now`: until the first of them
/// resets, since nothing of it is being used. `None` where no window says when it resets.
pub(crate) fn until_reset(windows: &[Window], now: i64) -> Option<Lasts<'_>> {
    windows
        .iter()
        .filter_map(|w| w.resets_at.map(|at| at - now))
        .filter(|left| *left > 0)
        .min()
        .map(|in_seconds| Lasts::UntilReset { in_seconds })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::LimitKind;

    const NOW: i64 = 1_789_935_000;
    const HOUR: i64 = 3_600;
    const DAY: i64 = 86_400;
    const WEEK: i64 = 7 * DAY;

    /// A window of `length` with `gone` of it gone by.
    fn pace(used: f64, gone: i64, length: i64) -> Option<Pace> {
        of(used, Some(NOW + length - gone), Some(length), NOW, NOW)
    }

    fn window(kind: &str, used: f64, gone: i64, length: i64) -> Window {
        Window {
            kind: kind.into(),
            scope: None,
            severity: None,
            percent: used,
            resets_at: Some(NOW + length - gone),
            is_active: true,
            length_seconds: Some(length),
        }
    }

    /// The share an even use reaches is the part of the window gone by, and the distance
    /// from it says which side of even the limit is on, past the margin.
    #[test]
    fn a_limit_is_measured_against_the_part_of_its_window_gone_by() {
        let half = pace(50.0, WEEK / 2, WEEK).unwrap();
        assert!((half.expected - 50.0).abs() < 1e-9);
        assert!(half.delta.abs() < 1e-9);
        assert_eq!(half.standing, Standing::Even);

        let gone = 10 * HOUR + 18 * 60;
        let busy = pace(33.0, gone, WEEK).unwrap();
        assert!((busy.expected - 6.13).abs() < 0.01, "{}", busy.expected);
        assert!(matches!(busy.standing, Standing::Over { .. }));

        assert_eq!(
            pace(10.0, WEEK / 2, WEEK).unwrap().standing,
            Standing::Under
        );
        assert_eq!(
            pace(54.9, WEEK / 2, WEEK).unwrap().standing,
            Standing::Even,
            "within the margin"
        );
        assert_eq!(
            pace(44.9, WEEK / 2, WEEK).unwrap().standing,
            Standing::Under
        );
    }

    /// Faster than even runs out before the reset, at the window's own rate since it
    /// began: 33% in ten hours runs out about twenty hours later, days before a week ends.
    #[test]
    fn a_limit_faster_than_even_says_when_it_runs_out() {
        let gone = 10 * HOUR;
        let Standing::Over { runs_out_in } = pace(33.0, gone, WEEK).unwrap().standing else {
            panic!("faster than even");
        };
        let at_its_rate = (67.0 * gone as f64 / 33.0) as i64;
        assert_eq!(runs_out_in, at_its_rate);
        assert!(runs_out_in < WEEK - gone, "before the reset");

        // Only just past the margin, still before the reset.
        let Standing::Over { runs_out_in } = pace(55.1, 5 * HOUR / 2, 5 * HOUR).unwrap().standing
        else {
            panic!("faster than even");
        };
        assert!(runs_out_in < 5 * HOUR / 2, "{runs_out_in}");
    }

    /// Nothing is said where nothing can be: a length or a reset not known, a reset passed,
    /// a reset further off than the window is long, a limit used up, and the first few
    /// minutes of a window.
    #[test]
    fn a_pace_is_said_only_where_it_means_something() {
        assert_eq!(of(40.0, None, Some(WEEK), NOW, NOW), None);
        assert_eq!(of(40.0, Some(NOW + DAY), None, NOW, NOW), None);
        assert_eq!(of(40.0, Some(NOW + DAY), Some(0), NOW, NOW), None);
        assert_eq!(of(40.0, Some(NOW), Some(WEEK), NOW, NOW), None);
        assert_eq!(of(40.0, Some(NOW - 60), Some(WEEK), NOW, NOW), None);
        assert_eq!(of(40.0, Some(NOW + WEEK + 60), Some(WEEK), NOW, NOW), None);
        assert_eq!(pace(100.0, DAY, WEEK), None, "used up says so itself");
        assert_eq!(pace(104.0, DAY, WEEK), None);

        let early = (TOO_EARLY * (5 * HOUR) as f64) as i64;
        assert_eq!(pace(20.0, early - 1, 5 * HOUR), None);
        assert!(pace(20.0, early + 1, 5 * HOUR).is_some());
    }

    /// A reading's pace is the pace when it was taken: an hour later its share is the share
    /// it was, and an even pace is where it was then, so a limit over pace stays over pace
    /// and only counts down to running out. Told against the later moment instead, the same
    /// reading drifted to under pace and hid the warning while nobody read it again.
    #[test]
    fn a_readings_pace_is_its_own_and_only_counts_down() {
        let read = NOW - HOUR;
        let resets = NOW + WEEK - 11 * HOUR;
        let then = of(33.0, Some(resets), Some(WEEK), read, read).unwrap();
        let later = of(33.0, Some(resets), Some(WEEK), read, NOW).unwrap();
        assert_eq!(later.expected, then.expected);
        assert_eq!(later.delta, then.delta);
        let (Standing::Over { runs_out_in: was }, Standing::Over { runs_out_in: is }) =
            (then.standing, later.standing)
        else {
            panic!("over pace both times");
        };
        assert_eq!(is, was - HOUR);

        let long_ago = of(90.0, Some(NOW + 3 * DAY), Some(WEEK), NOW - 3 * DAY, NOW).unwrap();
        assert_eq!(
            long_ago.standing,
            Standing::Over { runs_out_in: 0 },
            "projected to have run out already: about to, as far as anyone knows"
        );
        assert_eq!(
            of(33.0, Some(NOW - 1), Some(WEEK), NOW - DAY, NOW),
            None,
            "it has reset since it was read"
        );
        assert_eq!(
            of(33.0, Some(resets), Some(WEEK), NOW + HOUR, NOW),
            of(33.0, Some(resets), Some(WEEK), NOW, NOW),
            "a reading from a clock ahead of this one counts as taken now"
        );
    }

    /// A limit nobody has touched is under pace, and lasts.
    #[test]
    fn an_untouched_limit_is_under_pace() {
        assert_eq!(pace(0.0, DAY, WEEK).unwrap().standing, Standing::Under);
        assert_eq!(pace(-1.0, DAY, WEEK).unwrap().standing, Standing::Under);
    }

    /// An account in use lasts until its first limit to run out at its pace does, and
    /// otherwise until its first reset. One not in use is not being used, so it lasts until
    /// its first reset whatever its paces were.
    #[test]
    fn an_account_lasts_until_its_first_limit_runs_out_or_resets() {
        let session = window("session", 20.0, 4 * HOUR, 5 * HOUR);
        let week = window("weekly_all", 33.0, 10 * HOUR, WEEK);
        let busy = [session.clone(), week];
        let Some(Lasts::RunsOut { limit, in_seconds }) = lasts(&busy, NOW, NOW) else {
            panic!("the week runs out");
        };
        assert_eq!(limit.kind, LimitKind::WeeklyAll);
        assert_eq!(in_seconds, (67.0 * (10 * HOUR) as f64 / 33.0) as i64);

        let calm = window("weekly_all", 5.0, 3 * DAY, WEEK);
        assert_eq!(
            lasts(&[session, calm], NOW, NOW),
            Some(Lasts::UntilReset { in_seconds: HOUR })
        );

        let mut unknown = window("weekly_all", 5.0, 3 * DAY, WEEK);
        unknown.resets_at = None;
        assert_eq!(lasts(&[unknown], NOW, NOW), None);
        assert_eq!(
            until_reset(&busy, NOW),
            Some(Lasts::UntilReset { in_seconds: HOUR }),
            "the week's pace says nothing of an account nobody is using"
        );
    }

    /// Of two limits running out, the sooner is the one that counts.
    #[test]
    fn the_first_limit_to_run_out_is_the_soonest() {
        let paces = [
            ("week", pace(40.0, DAY, WEEK)),
            ("session", pace(90.0, 2 * HOUR, 5 * HOUR)),
            ("calm", pace(1.0, DAY, WEEK)),
            ("unknown", None),
        ];
        let (first, _) = first_to_run_out(paces).unwrap();
        assert_eq!(first, "session");
        assert_eq!(first_to_run_out([("calm", pace(1.0, DAY, WEEK))]), None);
    }

    /// In `--json`, a pace is its two figures and its standing, and an over-pace one when it
    /// runs out.
    #[test]
    fn a_pace_reads_in_json_as_its_figures_and_standing() {
        let over = Pace {
            expected: 6.0,
            delta: 27.0,
            standing: Standing::Over {
                runs_out_in: 72_000,
            },
        };
        assert_eq!(
            serde_json::to_value(over).unwrap(),
            serde_json::json!({
                "expected": 6.0,
                "delta": 27.0,
                "standing": "over",
                "runs_out_in": 72_000,
            })
        );
        let under = Pace {
            expected: 50.0,
            delta: -40.0,
            standing: Standing::Under,
        };
        assert_eq!(
            serde_json::to_value(under).unwrap(),
            serde_json::json!({"expected": 50.0, "delta": -40.0, "standing": "under"})
        );
    }
}
