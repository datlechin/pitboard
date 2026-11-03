//! Which account Pitboard picks when it picks one itself: the automatic switch, and the app's
//! offer once an account reaches the share or runs out.
//!
//! Most left, the order everyone has unless they choose otherwise, keeps the most room ahead.
//! Resets soonest spends first what is lost first. Anthropic gives each account a weekly reset
//! of its own, at a fixed time whatever is used (`weekly_period_is_a_fixed_cycle`), and what
//! an account leaves unused of its week is gone at that reset. So the account whose week
//! resets soonest is used first, and the automatic switch moves to it before the account in use
//! reaches the share. An account without a weekly limit for all models, such as a Team seat,
//! comes first in either order: what it leaves unused is lost at each five-hour reset.

use super::{LimitKind, Snapshot, Window, budget, reached, same_reset};
use std::cmp::Ordering;

/// How old a reading may be for a move before the share: as long as a usage check waits at
/// most on a service out of reach. Every front end that acts by itself reads each account
/// more often, so an older reading is of an account nobody could ask about, which may have
/// been used since.
const FRESH: i64 = budget::UNREACHABLE_MOST;

/// Which account goes first when Pitboard picks one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Order {
    /// The most room in the limit that reached the share, then the least full in the others.
    #[default]
    MostLeft,
    /// The account whose week resets soonest, which the automatic switch also moves to before
    /// the account in use reaches the share.
    ResetsSoonest,
}

impl Order {
    /// As the command line and the app's preferences write it.
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
    /// It has no weekly limit for all models, such as a Team seat: what it leaves unused is
    /// lost at each five-hour reset.
    Free,
    /// Its weekly limit for all models resets then, in epoch seconds.
    Resets(i64),
    /// It has one, or may have, and nothing says when it resets: none was given, or the one
    /// given has passed and no reading has said the next.
    Unknown,
}

impl Week {
    /// The week of the account whose reading is `reading`, as of `now`. A reading that may
    /// leave limits out says nothing of one it leaves out, as [`room`] takes it.
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

    /// When it resets, where it is known.
    pub fn resets_at(self) -> Option<i64> {
        match self {
            Week::Resets(at) => Some(at),
            Week::Free | Week::Unknown => None,
        }
    }

    /// Whether an account of this week comes before one of `other` for a move before the
    /// share. One without a weekly limit comes before one with, and of two that reset, the one
    /// that resets sooner, by more than [`same_reset`]. Never two without, which lose alike,
    /// nor where either reset is unknown.
    pub fn comes_before(self, other: Week) -> bool {
        match (self, other) {
            (Week::Free, Week::Resets(_) | Week::Unknown) => true,
            (Week::Resets(a), Week::Resets(b)) => a < b && !same_reset(a, b),
            _ => false,
        }
    }

    // Without a weekly limit first, in either order.
    fn free_first(self, other: Week) -> Ordering {
        (self != Week::Free).cmp(&(other != Week::Free))
    }

    // The sooner reset first, an unknown one last.
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
    /// Of the limit at the share. `None` where the account does not have it, and for a move
    /// before the share.
    pub used: Option<f64>,
    /// Of the fullest of its other limits.
    pub fullest: f64,
    /// When what it has left of its week is lost.
    pub week: Week,
}

/// The room an account whose reading is `theirs` has for a switch away from `limit` of the
/// account in use, whose limits are `mine`: `None` where any of `theirs` has reached `share` as
/// it is drawn, or where nothing says what the account has used of one of `mine`. The automatic
/// switch asks it with its share, and the app's advice with 100.
///
/// Where `theirs` lists every limit the account has ([`Snapshot::lists_every_limit`]), one it
/// leaves out is one it does not have, so a plan without the limit that ran out, such as a
/// Team seat with no weekly limit for all models, is a place to go. Any other reading says
/// nothing of a limit it leaves out, which may be used up, as with no reading at all. Only a
/// model's limit other than `limit` may be left out of one that gives the five-hour and weekly
/// limits: the older named shape names no model's, and plans limit models apart.
pub fn room(
    theirs: &Snapshot,
    mine: &[Window],
    limit: &Window,
    share: u8,
    now: i64,
) -> Option<Room> {
    below(theirs, mine, Some(limit), share, now)
}

/// The room an account whose reading is `theirs` has for a move before the share away from the
/// account in use, whose reading is `mine`, where it comes first by when its week resets
/// ([`Week::comes_before`]): `None` where it does not, where it has used more of its five-hour
/// limit, where its reading is older than `FRESH` or not Anthropic's, or where it has no room
/// below `share`, as [`room`] judges it. An account without a five-hour limit has used none.
pub fn resets_first(theirs: &Snapshot, mine: &Snapshot, share: u8, now: i64) -> Option<Room> {
    let fresh = theirs.answered_at.is_some_and(|at| now - at <= FRESH);
    let first = Week::of(theirs, now).comes_before(Week::of(mine, now));
    if !fresh || !first || five_hours(&theirs.windows, now) > five_hours(&mine.windows, now) {
        return None;
    }
    below(theirs, &mine.windows, None, share, now)
}

/// The candidate to switch to, in `order`. One without a weekly limit for all models comes
/// first in both. Under [`Order::ResetsSoonest`] the one whose week resets sooner then, as
/// [`same_reset`] tells resets apart, an unknown reset last. Then the least used of the limit
/// at the share, one without it as one with none used, then the least full in its others. The
/// first of those alike.
pub fn preferred<T>(
    order: Order,
    candidates: impl IntoIterator<Item = (T, Room)>,
) -> Option<(T, Room)> {
    candidates.into_iter().min_by(|(_, a), (_, b)| {
        a.week
            .free_first(b.week)
            .then_with(|| match order {
                Order::MostLeft => Ordering::Equal,
                Order::ResetsSoonest => a.week.sooner(b.week),
            })
            .then_with(|| a.used.unwrap_or(0.0).total_cmp(&b.used.unwrap_or(0.0)))
            .then_with(|| a.fullest.total_cmp(&b.fullest))
    })
}

/// What an account whose limits are `windows` has used of its five-hour limit as of `now`:
/// none where it has none, or it has reset.
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
    share: u8,
    now: i64,
) -> Option<Room> {
    let windows = &theirs.windows;
    if windows.is_empty()
        || windows
            .iter()
            .any(|window| reached(window.used(now), share))
    {
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
                .filter_map(|(name, theirs)| Some((name, room(theirs, &mine, &limit, 95, NOW)?)))
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
                .filter_map(|(name, theirs)| Some((*name, room(theirs, &mine, &limit, 95, NOW)?)))
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
            candidates
                .iter()
                .filter_map(|(name, theirs)| Some((*name, room(theirs, &mine, &limit, 95, NOW)?))),
        );
        assert_eq!(chosen.map(|(n, _)| n), Some("roomy"));
    }

    #[test]
    fn resets_first_needs_a_sooner_week_room_and_as_much_five_hour_room() {
        let mine = max(30.0, 5);
        assert!(resets_first(&max(30.0, 1), &mine, 95, NOW).is_some());
        assert!(resets_first(&max(0.0, 1), &mine, 95, NOW).is_some());
        assert!(
            resets_first(&max(31.0, 1), &mine, 95, NOW).is_none(),
            "less five-hour room"
        );
        assert!(
            resets_first(&max(0.0, 5), &mine, 95, NOW).is_none(),
            "the same week"
        );
        assert!(
            resets_first(&max(0.0, 6), &mine, 95, NOW).is_none(),
            "a later week"
        );
        let mut full = max(0.0, 1);
        full.windows[1].percent = 95.0;
        assert!(resets_first(&full, &mine, 95, NOW).is_none(), "no room");
    }

    #[test]
    fn a_seat_resets_first_from_a_week_and_never_from_another_seat() {
        assert!(resets_first(&seat(10.0), &max(30.0, 1), 95, NOW).is_some());
        assert!(resets_first(&seat(0.0), &seat(30.0), 95, NOW).is_none());
        assert!(resets_first(&max(0.0, 1), &seat(30.0), 95, NOW).is_none());
    }

    #[test]
    fn resets_first_wants_both_weeks_known() {
        let mut passed = max(30.0, 5);
        passed.windows[1].resets_at = Some(NOW - HOUR);
        assert!(resets_first(&max(0.0, 1), &passed, 95, NOW).is_none());
        let mut unread = max(0.0, 1);
        unread.windows[1].resets_at = None;
        assert!(resets_first(&unread, &max(30.0, 5), 95, NOW).is_none());
    }

    #[test]
    fn resets_first_wants_a_reading_anthropic_gave_lately() {
        let mine = max(30.0, 5);
        let mut old = max(0.0, 1);
        old.answered_at = Some(NOW - FRESH - 1);
        assert!(resets_first(&old, &mine, 95, NOW).is_none());
        old.answered_at = Some(NOW - FRESH);
        assert!(resets_first(&old, &mine, 95, NOW).is_some());
        old.answered_at = None;
        assert!(
            resets_first(&old, &mine, 95, NOW).is_none(),
            "not Anthropic's"
        );
    }

    #[test]
    fn without_a_five_hour_limit_none_of_it_is_used() {
        let ultra = |days| reading(vec![window("weekly_all", 40.0, days * DAY)]);
        assert!(resets_first(&ultra(1), &max(0.0, 5), 95, NOW).is_some());
        assert!(
            resets_first(&max(10.0, 1), &ultra(5), 95, NOW).is_none(),
            "from no five-hour limit, only to none used"
        );
        assert!(resets_first(&max(0.0, 1), &ultra(5), 95, NOW).is_some());
    }

    #[test]
    fn a_five_hour_limit_that_reset_counts_as_unused() {
        let mut reset = max(80.0, 1);
        reset.windows[0].resets_at = Some(NOW - 60);
        assert!(resets_first(&reset, &max(30.0, 5), 95, NOW).is_some());
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
            resets_first(&named, &mine, 95, NOW).is_some(),
            "a model's limit may be left out of one with the five-hour and weekly limits"
        );
        let mut weekly_only = reading(vec![window("weekly_all", 10.0, DAY)]);
        weekly_only.lists_every_limit = false;
        assert!(resets_first(&weekly_only, &mine, 95, NOW).is_none());
    }
}
