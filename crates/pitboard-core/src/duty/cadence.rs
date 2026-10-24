//! How often a front end looks, reads and decides by itself, and how long something that came
//! to nothing waits before it is tried again. One set of numbers for every duty and every front
//! end, so the app and `pitboard watch` running side by side never ask more often than either
//! would alone.

use std::time::Duration;

/// How long after an attempt that came to nothing Pitboard tries again: long enough for a
/// service that could not be reached, or a lock Claude Code held, to have come back. Twice
/// as long after each attempt in a row that failed for a reason trying again may mend.
pub(crate) const RETRY_SECONDS: i64 = 60;

/// The longest it waits between attempts, however long the outage: as long as a usage check
/// waits on a service that cannot be reached at most (`budget`).
pub(crate) const RETRY_MOST_SECONDS: i64 = 900;

/// Attempts at one thing before what it is about resets, of those that failed for a reason
/// trying again will not mend: something refused this often for such a reason is not going
/// to be done.
pub(crate) const ATTEMPTS: u32 = 3;

// When the index and the readings were last written: no network and no keychain.
pub const LOOK_EVERY_SECONDS: i64 = 2;

// Each account is still asked only as often as its budget allows.
pub const READ_EVERY_SECONDS: i64 = 300;

/// How often a front end that acts by itself decides though nothing was written: an account
/// put in use stops settling, and the wait after a failed attempt ends, with nobody writing
/// anything.
pub const DECIDE_EVERY_SECONDS: i64 = 30;

/// How long to wait after `waits` failures in a row for a reason trying again may mend: of
/// attempts, of asking whose login a tool has stored, or of a front end's refusals that came
/// before anything could record them.
pub(crate) fn retry_after(waits: u32) -> i64 {
    (RETRY_SECONDS << waits.saturating_sub(1).min(4)).min(RETRY_MOST_SECONDS)
}

/// A front end's refusals in a row and the wait after them, so its next decision under the
/// lock is no sooner than the core waits after as many failed attempts in a row. A refusal the
/// core raised before it could record anything would otherwise be asked again, and written in
/// the activity log, at every decision. Anything else a decision or a look comes to ends the
/// row.
///
/// Measured on the front end's own clock, as time since whatever it counts from: the app's
/// clock that runs while it does, the command line's since 1970.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Refusals {
    in_a_row: u32,
    /// When the last was.
    at: Duration,
    /// Until when the next decision under the lock waits.
    until: Duration,
}

impl Refusals {
    /// Whether a decision under the lock waits at `now`. A clock gone back before the last
    /// refusal waits on nothing.
    pub fn waits(&self, now: Duration) -> bool {
        (self.at..self.until).contains(&now)
    }

    /// When to decide next after deciding at `decided`: `every` later, or as the wait ends
    /// where that comes first.
    pub fn due(&self, decided: Duration, every: Duration) -> Duration {
        let next = decided + every;
        if self.waits(decided) {
            next.min(self.until)
        } else {
            next
        }
    }

    /// A decision at `at` was refused: the next waits longer than after the one before it.
    /// How long, in seconds.
    pub fn refused(&mut self, at: Duration) -> i64 {
        let in_a_row = self.in_a_row + 1;
        let wait = retry_after(in_a_row);
        *self = Refusals {
            in_a_row,
            at,
            until: at + Duration::from_secs(wait.unsigned_abs()),
        };
        wait
    }

    /// A decision or a look came to something other than a refusal, which ends the row.
    pub fn ended(&mut self) {
        *self = Refusals::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: Duration = Duration::from_secs(1_760_000_000);
    const EVERY: Duration = Duration::from_secs(30);

    fn secs(seconds: i64) -> Duration {
        Duration::from_secs(seconds.unsigned_abs())
    }

    #[test]
    fn the_wait_doubles_from_a_minute_to_a_quarter_of_an_hour() {
        let waits: Vec<i64> = (0..8).map(retry_after).collect();
        assert_eq!(waits, [60, 60, 120, 240, 480, 900, 900, 900]);
    }

    /// After a refusal the next decision under the lock waits as long as the core waits after
    /// as many failed attempts in a row. A look goes on meanwhile, and anything it or a
    /// decision comes to ends the row.
    #[test]
    fn a_refusal_is_tried_again_only_after_the_cores_wait() {
        let mut refusals = Refusals::default();
        assert!(!refusals.waits(AT));
        assert_eq!(refusals.due(AT, EVERY), AT + EVERY);

        assert_eq!(refusals.refused(AT), 60);
        assert!(refusals.waits(AT + secs(59)));
        assert!(!refusals.waits(AT + secs(60)));
        assert!(
            !refusals.waits(AT - secs(1)),
            "a clock gone backwards waits on nothing"
        );
        assert_eq!(refusals.due(AT, EVERY), AT + EVERY, "it looks meanwhile");
        assert_eq!(
            refusals.due(AT + secs(45), EVERY),
            AT + secs(60),
            "and decides as the wait ends"
        );

        assert_eq!(refusals.refused(AT + secs(60)), 120);
        assert!(refusals.waits(AT + secs(179)));
        assert!(!refusals.waits(AT + secs(180)));
        for _ in 0..8 {
            refusals.refused(AT);
        }
        assert!(refusals.waits(AT + secs(RETRY_MOST_SECONDS - 1)));
        assert!(
            !refusals.waits(AT + secs(RETRY_MOST_SECONDS)),
            "at most a quarter of an hour"
        );

        refusals.ended();
        assert!(!refusals.waits(AT + secs(200)));
        refusals.refused(AT + secs(200));
        assert!(
            !refusals.waits(AT + secs(260)),
            "a row ended starts again at a minute"
        );
    }
}
