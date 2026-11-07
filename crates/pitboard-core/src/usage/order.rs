//! Which account Pitboard picks by itself: for the automatic switch, and for the app's offer.
//!
//! Resets soonest spends first what is lost first. Each account's week resets at a fixed time of
//! its own (`weekly_period_is_a_fixed_cycle`), and what it leaves unused is gone then. An account
//! without a weekly limit for all models, such as a Team seat, loses it at each five-hour reset,
//! so it comes first in either order.

use super::{LimitKind, Snapshot, Window, budget, same_reset};
use std::cmp::Ordering;

// A front end that acts by itself reads each account more often than a usage check waits at most
// on a service out of reach: an older reading is of an account nobody could ask about.
const FRESH: i64 = budget::UNREACHABLE_MOST;

/// The shares that trigger a switch, with no weekly trigger where `weekly` is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shares {
    /// The share for five-hour and unclassified limits.
    pub at: u8,
    pub weekly: Option<u8>,
}

impl From<u8> for Shares {
    fn from(at: u8) -> Self {
        Self {
            at,
            weekly: Some(at),
        }
    }
}

impl Shares {
    pub fn at(self, window: &Window) -> Option<u8> {
        match &window.kind {
            LimitKind::WeeklyAll | LimitKind::WeeklyScoped => self.weekly,
            LimitKind::Other(name) if name.starts_with("weekly_") => self.weekly,
            _ => Some(self.at),
        }
    }

    pub fn reached(self, window: &Window, now: i64) -> bool {
        self.at(window)
            .is_some_and(|share| window.reached(share, now))
    }

    /// Even a limit whose trigger is off leaves no room once the service says it is used up.
    pub fn excludes(self, window: &Window, now: i64) -> bool {
        window.used_up(now) || self.reached(window, now)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Order {
    #[default]
    MostLeft,
    ResetsSoonest,
}

impl Order {
    pub fn code(self) -> &'static str {
        match self {
            Order::MostLeft => "most-left",
            Order::ResetsSoonest => "resets-soonest",
        }
    }

    pub fn from_code(code: &str) -> Option<Order> {
        match code {
            "most-left" => Some(Order::MostLeft),
            "resets-soonest" => Some(Order::ResetsSoonest),
            _ => None,
        }
    }
}

/// When what an account has left of its week is lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Week {
    /// No weekly limit for all models.
    Free,
    /// In epoch seconds.
    Resets(i64),
    /// It has one, or may have, and no reading says when it resets next.
    Unknown,
}

impl Week {
    // A reading that may leave limits out says nothing of one it leaves out.
    pub fn of(reading: &Snapshot, now: i64) -> Week {
        let week = reading
            .windows
            .iter()
            .find(|window| window.scope.is_none() && window.kind == LimitKind::WeeklyAll);
        match week {
            None if reading.lists_every_limit => Week::Free,
            None => Week::Unknown,
            Some(week) => match week.resets_at {
                Some(at) if at > now => Week::Resets(at),
                _ => Week::Unknown,
            },
        }
    }

    pub fn resets_at(self) -> Option<i64> {
        match self {
            Week::Resets(at) => Some(at),
            Week::Free | Week::Unknown => None,
        }
    }

    /// For a move before the share: never between two without a week, which lose alike, nor
    /// where either reset is unknown.
    pub fn comes_before(self, other: Week) -> bool {
        match (self, other) {
            (Week::Free, Week::Resets(_) | Week::Unknown) => true,
            (Week::Resets(a), Week::Resets(b)) => a < b && !same_reset(a, b),
            _ => false,
        }
    }

    fn free_first(self, other: Week) -> Ordering {
        (self != Week::Free).cmp(&(other != Week::Free))
    }

    fn sooner(self, other: Week) -> Ordering {
        match (self, other) {
            (Week::Resets(a), Week::Resets(b)) if same_reset(a, b) => Ordering::Equal,
            (Week::Resets(a), Week::Resets(b)) => a.cmp(&b),
            (a, b) => a.rank().cmp(&b.rank()),
        }
    }

    fn rank(self) -> u8 {
        match self {
            Week::Free => 0,
            Week::Resets(_) => 1,
            Week::Unknown => 2,
        }
    }
}

/// What an account with room for a switch has used.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Room {
    /// Of the limit at the share, where it has that limit and there is one.
    pub used: Option<f64>,
    /// Of the fullest of its other limits.
    pub fullest: f64,
    pub week: Week,
}

/// The room for a switch away from `limit` of the account in use, whose limits are `mine`:
/// `None` where any of `theirs` has reached its share as drawn, or where nothing says what
/// it has used of one of `mine`.
///
/// A reading that lists every limit ([`Snapshot::lists_every_limit`]) does not have one it
/// leaves out, so a Team seat is a place to go from a weekly limit. Any other may leave out a
/// used-up limit; only a model's limit other than `limit` may be missing from one that gives the
/// five-hour and weekly limits, since plans limit models apart.
pub fn room(
    theirs: &Snapshot,
    mine: &[Window],
    limit: &Window,
    shares: impl Into<Shares>,
    now: i64,
) -> Option<Room> {
    below(theirs, mine, Some(limit), shares.into(), now)
}

/// The room for a move before the share away from the account in use, whose reading is `mine`:
/// `None` also where `theirs` has used more of its five-hour limit, or is old or not
/// Anthropic's.
pub fn resets_first(
    theirs: &Snapshot,
    mine: &Snapshot,
    shares: impl Into<Shares>,
    now: i64,
) -> Option<Room> {
    let fresh = theirs.answered_at.is_some_and(|at| now - at <= FRESH);
    let first = Week::of(theirs, now).comes_before(Week::of(mine, now));
    if !fresh || !first || five_hours(&theirs.windows, now) > five_hours(&mine.windows, now) {
        return None;
    }
    below(theirs, &mine.windows, None, shares.into(), now)
}

/// The candidate to switch to in `order`, the first of those alike. An account without the limit
/// at the share counts as having used none of it.
pub fn preferred<T>(
    order: Order,
    candidates: impl IntoIterator<Item = (T, Room)>,
) -> Option<(T, Room)> {
    candidates.into_iter().min_by(|(_, a), (_, b)| {
        match order {
            Order::MostLeft => a.week.free_first(b.week),
            Order::ResetsSoonest => a.week.sooner(b.week),
        }
        .then_with(|| a.used.unwrap_or(0.0).total_cmp(&b.used.unwrap_or(0.0)))
        .then_with(|| a.fullest.total_cmp(&b.fullest))
    })
}

/// Of its five-hour limit as of `now`, none where it has none.
pub fn five_hours(windows: &[Window], now: i64) -> f64 {
    windows
        .iter()
        .filter(|window| window.scope.is_none() && window.kind == LimitKind::Session)
        .map(|window| window.used(now))
        .fold(0.0, f64::max)
}

fn below(
    theirs: &Snapshot,
    mine: &[Window],
    limit: Option<&Window>,
    shares: Shares,
    now: i64,
) -> Option<Room> {
    let windows = &theirs.windows;
    if windows.is_empty() || windows.iter().any(|window| shares.excludes(window, now)) {
        return None;
    }
    let given = |had: &Window| windows.iter().find(|window| window.same_limit(had));
    let gives = |kind: LimitKind| {
        windows
            .iter()
            .any(|window| window.scope.is_none() && window.kind == kind)
    };
    let other = |window: &Window| limit.is_none_or(|limit| !window.same_limit(limit));
    if !theirs.lists_every_limit {
        let overall = gives(LimitKind::Session) && gives(LimitKind::WeeklyAll);
        let unknown =
            |had: &Window| given(had).is_none() && !(overall && had.scope.is_some() && other(had));
        if limit.into_iter().chain(mine).any(unknown) {
            return None;
        }
    }
    let fullest = windows
        .iter()
        .filter(|window| other(window))
        .map(|window| window.used(now))
        .fold(0.0, f64::max);
    Some(Room {
        used: limit.and_then(given).map(|window| window.used(now)),
        fullest,
        week: Week::of(theirs, now),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{DAY, HOUR};
    use crate::usage::Source;

    const NOW: i64 = 1_760_000_000;

    fn window(kind: &str, percent: f64, resets_in: i64) -> Window {
        Window {
            kind: kind.into(),
            scope: None,
            percent,
            resets_at: Some(NOW + resets_in),
            is_active: true,
            severity: None,
            length_seconds: LimitKind::from(kind).length(),
        }
    }

    fn scoped(kind: &str, model: &str, percent: f64) -> Window {
        Window {
            scope: Some(model.into()),
            ..window(kind, percent, 3 * DAY)
        }
    }

    fn reading(windows: Vec<Window>) -> Snapshot {
        Snapshot {
            windows,
            observed_at: Some(NOW),
            answered_at: Some(NOW),
            lists_every_limit: true,
            source: Source::Remembered,
        }
    }

    #[test]
    fn a_uniform_full_share_keeps_manual_advice_at_the_displayed_hundred() {
        let mine = [window("session", 100.0, HOUR)];
        let candidate = |used| reading(vec![window("session", used, HOUR)]);
        assert!(room(&candidate(99.4), &mine, &mine[0], 100u8, NOW).is_some());
        assert!(room(&candidate(99.5), &mine, &mine[0], 100u8, NOW).is_none());
        assert!(room(&candidate(100.0), &mine, &mine[0], 100u8, NOW).is_none());
    }

    /// An account with its five-hour limit `session` used and its week resetting in `days`.
    fn max(session: f64, days: i64) -> Snapshot {
        reading(vec![
            window("session", session, 2 * HOUR),
            window("weekly_all", 40.0, days * DAY),
        ])
    }

    fn seat(session: f64) -> Snapshot {
        reading(vec![
            window("session", session, 2 * HOUR),
            scoped("weekly_scoped", "Fable", 0.0),
        ])
    }

    #[test]
    fn an_order_reads_back_from_its_code() {
        for order in [Order::MostLeft, Order::ResetsSoonest] {
            assert_eq!(Order::from_code(order.code()), Some(order));
        }
        assert_eq!(Order::from_code("soonest"), None);
        assert_eq!(Order::default(), Order::MostLeft);
    }

    #[test]
    fn a_week_is_free_running_or_unknown_by_the_reading() {
        assert_eq!(Week::of(&seat(0.0), NOW), Week::Free);
        assert_eq!(Week::of(&max(0.0, 2), NOW), Week::Resets(NOW + 2 * DAY));
        let passed = reading(vec![window("weekly_all", 40.0, -HOUR)]);
        assert_eq!(Week::of(&passed, NOW), Week::Unknown, "a reset gone by");
        let mut unread = seat(0.0);
        unread.lists_every_limit = false;
        assert_eq!(Week::of(&unread, NOW), Week::Unknown, "it may have one");
        let model = reading(vec![scoped("weekly_all", "Fable", 10.0)]);
        assert_eq!(
            Week::of(&model, NOW),
            Week::Free,
            "a model's week is not the account's"
        );
    }

    #[test]
    fn a_week_that_resets_a_minute_sooner_comes_first_and_59_seconds_do_not() {
        let later = Week::Resets(NOW + DAY);
        assert!(Week::Resets(NOW + DAY - 60).comes_before(later));
        assert!(!Week::Resets(NOW + DAY - 59).comes_before(later));
        assert!(!later.comes_before(later));
        assert!(Week::Free.comes_before(later));
        assert!(Week::Free.comes_before(Week::Unknown));
        assert!(!Week::Free.comes_before(Week::Free), "two seats lose alike");
        assert!(!Week::Resets(NOW).comes_before(Week::Unknown));
        assert!(!Week::Unknown.comes_before(later));
        assert!(!later.comes_before(Week::Free));
    }

    #[test]
    fn most_left_keeps_the_most_room_and_resets_soonest_the_sooner_week() {
        let limit = window("session", 96.0, HOUR);
        let mine = [limit.clone()];
        let soon = max(60.0, 1);
        let roomy = max(10.0, 5);
        let candidates = || {
            [("soon", &soon), ("roomy", &roomy)]
                .into_iter()
                .filter_map(|(name, theirs)| Some((name, room(theirs, &mine, &limit, 95u8, NOW)?)))
        };
        assert_eq!(
            preferred(Order::MostLeft, candidates()).map(|(name, _)| name),
            Some("roomy")
        );
        assert_eq!(
            preferred(Order::ResetsSoonest, candidates()).map(|(name, _)| name),
            Some("soon")
        );
    }

    #[test]
    fn a_seat_comes_first_in_either_order_and_an_unknown_week_last() {
        let limit = window("session", 96.0, HOUR);
        let mine = [limit.clone()];
        let mut unread = max(0.0, 1);
        unread.windows[1].resets_at = None;
        let accounts = [
            ("unread", unread),
            ("soon", max(50.0, 1)),
            ("seat", seat(80.0)),
        ];
        let candidates = || {
            accounts
                .iter()
                .filter_map(|(name, theirs)| Some((*name, room(theirs, &mine, &limit, 95u8, NOW)?)))
        };
        for order in [Order::MostLeft, Order::ResetsSoonest] {
            assert_eq!(preferred(order, candidates()).map(|(n, _)| n), Some("seat"));
        }
        let weeks = || candidates().filter(|(name, _)| *name != "seat");
        assert_eq!(
            preferred(Order::ResetsSoonest, weeks()).map(|(n, _)| n),
            Some("soon")
        );
        assert_eq!(
            preferred(Order::MostLeft, weeks()).map(|(n, _)| n),
            Some("unread"),
            "most left looks at room alone"
        );
    }

    #[test]
    fn weeks_a_moment_apart_are_one_and_the_most_room_decides() {
        let limit = window("session", 96.0, HOUR);
        let mine = [limit.clone()];
        let mut tight = max(70.0, 1);
        tight.windows[1].resets_at = Some(NOW + DAY - 30);
        let roomy = max(20.0, 1);
        let candidates = [("tight", tight), ("roomy", roomy)];
        let chosen = preferred(
            Order::ResetsSoonest,
            candidates.iter().filter_map(|(name, theirs)| {
                Some((*name, room(theirs, &mine, &limit, 95u8, NOW)?))
            }),
        );
        assert_eq!(chosen.map(|(n, _)| n), Some("roomy"));
    }

    #[test]
    fn resets_first_needs_a_sooner_week_room_and_as_much_five_hour_room() {
        let mine = max(30.0, 5);
        assert!(resets_first(&max(30.0, 1), &mine, 95u8, NOW).is_some());
        assert!(resets_first(&max(0.0, 1), &mine, 95u8, NOW).is_some());
        assert!(
            resets_first(&max(31.0, 1), &mine, 95u8, NOW).is_none(),
            "less five-hour room"
        );
        assert!(
            resets_first(&max(0.0, 5), &mine, 95u8, NOW).is_none(),
            "the same week"
        );
        assert!(
            resets_first(&max(0.0, 6), &mine, 95u8, NOW).is_none(),
            "a later week"
        );
        let mut full = max(0.0, 1);
        full.windows[1].percent = 95.0;
        assert!(resets_first(&full, &mine, 95u8, NOW).is_none(), "no room");
    }

    #[test]
    fn a_seat_resets_first_from_a_week_and_never_from_another_seat() {
        assert!(resets_first(&seat(10.0), &max(30.0, 1), 95u8, NOW).is_some());
        assert!(resets_first(&seat(0.0), &seat(30.0), 95u8, NOW).is_none());
        assert!(resets_first(&max(0.0, 1), &seat(30.0), 95u8, NOW).is_none());
    }

    #[test]
    fn resets_first_wants_both_weeks_known() {
        let mut passed = max(30.0, 5);
        passed.windows[1].resets_at = Some(NOW - HOUR);
        assert!(resets_first(&max(0.0, 1), &passed, 95u8, NOW).is_none());
        let mut unread = max(0.0, 1);
        unread.windows[1].resets_at = None;
        assert!(resets_first(&unread, &max(30.0, 5), 95u8, NOW).is_none());
    }

    #[test]
    fn resets_first_wants_a_reading_anthropic_gave_lately() {
        let mine = max(30.0, 5);
        let mut old = max(0.0, 1);
        old.answered_at = Some(NOW - FRESH - 1);
        assert!(resets_first(&old, &mine, 95u8, NOW).is_none());
        old.answered_at = Some(NOW - FRESH);
        assert!(resets_first(&old, &mine, 95u8, NOW).is_some());
        old.answered_at = None;
        assert!(
            resets_first(&old, &mine, 95u8, NOW).is_none(),
            "not Anthropic's"
        );
    }

    #[test]
    fn without_a_five_hour_limit_none_of_it_is_used() {
        let ultra = |days| reading(vec![window("weekly_all", 40.0, days * DAY)]);
        assert!(resets_first(&ultra(1), &max(0.0, 5), 95u8, NOW).is_some());
        assert!(
            resets_first(&max(10.0, 1), &ultra(5), 95u8, NOW).is_none(),
            "from no five-hour limit, only to none used"
        );
        assert!(resets_first(&max(0.0, 1), &ultra(5), 95u8, NOW).is_some());
    }

    #[test]
    fn a_five_hour_limit_that_reset_counts_as_unused() {
        let mut reset = max(80.0, 1);
        reset.windows[0].resets_at = Some(NOW - 60);
        assert!(resets_first(&reset, &max(30.0, 5), 95u8, NOW).is_some());
    }

    #[test]
    fn a_reading_that_may_leave_limits_out_moves_early_only_with_the_limits_in_use() {
        let mine = reading(vec![
            window("session", 30.0, 2 * HOUR),
            window("weekly_all", 20.0, 5 * DAY),
            scoped("weekly_scoped", "Opus", 50.0),
        ]);
        let mut named = max(0.0, 1);
        named.lists_every_limit = false;
        assert!(
            resets_first(&named, &mine, 95u8, NOW).is_some(),
            "a model's limit may be left out of one with the five-hour and weekly limits"
        );
        let mut weekly_only = reading(vec![window("weekly_all", 10.0, DAY)]);
        weekly_only.lists_every_limit = false;
        assert!(resets_first(&weekly_only, &mine, 95u8, NOW).is_none());
    }
}
