//! One view of "how much is left", whatever shape it arrived in. Usage comes as a
//! `limits[]` array or as named `five_hour`/`seven_day` objects; both are normalised at the
//! boundary, and a value that fails to normalise is dropped rather than drawn.

use crate::time;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Window {
    pub kind: String,
    pub scope: Option<String>,
    pub percent: f64,
    pub resets_at: Option<i64>,
    pub is_active: bool,
    /// How Anthropic grades this row, when it grades it. Its word, not a threshold of
    /// Pitboard's own, and absent in a reading taken before Pitboard read this field.
    #[serde(default)]
    pub severity: Option<String>,
    /// How long the window runs, in seconds, where that is known.
    ///
    /// What makes a limit comparable to itself over time: a reset time alone cannot say
    /// how long a window is, because the time left shrinks as the window runs out. Stated
    /// outright by OpenAI, implied by the kind for Anthropic, and absent from a reading
    /// taken before Pitboard kept it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_seconds: Option<i64>,
}

/// How long one of Anthropic's windows runs, from its kind.
///
/// Anthropic names its windows rather than timing them: `session` and the older
/// `five_hour` are the five-hour limit, and every `weekly_` kind, like the older
/// `seven_day`, runs a week. A kind not listed here has no length Pitboard can vouch for.
pub fn anthropic_window_length(kind: &str) -> Option<i64> {
    match kind {
        "session" | "five_hour" => Some(5 * 3600),
        "seven_day" => Some(7 * 86_400),
        weekly if weekly.starts_with("weekly_") => Some(7 * 86_400),
        _ => None,
    }
}

/// Where a measurement came from, so a stale number is never shown as a live one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Asked of Anthropic just now.
    Live,
    /// The last live reading Pitboard took itself.
    Remembered,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Snapshot {
    pub windows: Vec<Window>,
    pub observed_at: Option<i64>,
    /// When the service last answered for this account's own login. Absent from a reading
    /// an older Pitboard wrote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_at: Option<i64>,
    /// Whether `windows` are every limit the account has, so a limit they leave out is one
    /// it does not have: true of an answer read whole from a service whose answers list
    /// them all, and of the reading that answer set, since a session never adds a limit.
    #[serde(default)]
    pub lists_every_limit: bool,
    pub source: Source,
}

impl Window {
    /// The share used as of `now`. A window whose reset has passed counts as reset, though
    /// no reading has said so yet.
    pub(crate) fn used(&self, now: i64) -> f64 {
        if self.resets_at.is_some_and(|at| at <= now) {
            0.0
        } else {
            self.percent
        }
    }

    /// Whether `other` measures the same limit, whichever name its source gave it.
    pub(crate) fn same_limit(&self, other: &Window) -> bool {
        limit_name(&self.kind) == limit_name(&other.kind) && self.scope == other.scope
    }

    /// Whether `other` is this very window: the same limit, resetting at the same time as
    /// far as sources agree on one. A window with no reset time is no window in particular.
    pub(crate) fn same_window(&self, other: &Window) -> bool {
        self.same_limit(other)
            && matches!(
                (self.resets_at, other.resets_at),
                (Some(x), Some(y)) if same_reset(x, y)
            )
    }
}

/// A limit by one name. A Claude Code session, and Anthropic's answer when it has no
/// `limits`, use the older `five_hour` and `seven_day` for the limits `limits` calls
/// `session` and `weekly_all`. Codex's windows borrow the older names for their lengths,
/// which is harmless: one account's readings are only ever compared with each other.
pub(crate) fn limit_name(kind: &str) -> &str {
    match kind {
        "five_hour" => "session",
        "seven_day" => "weekly_all",
        other => other,
    }
}

/// A percentage as every front end draws it and the automatic switch judges it: a whole
/// one, a half rounded up, so a limit drawn at 95% is at 95%. Whether a limit is used up
/// is not judged by it: that is the service's 100, as read.
pub fn whole(percent: f64) -> i64 {
    percent.round() as i64
}

/// Resets closer together than this are one reset.
///
/// Sources do not agree to the second on when a window resets: Anthropic's answer gives a
/// fraction of a second, which is dropped, and a Claude Code session is given whole seconds.
/// A limit's next window starts only once the last has reset, and the shortest window any
/// service has shown runs five hours, so resets a minute apart are rounding and never two
/// windows.
const SAME_RESET: u64 = 60;

/// Whether two resets of a limit are one: less than `SAME_RESET` apart. Public so that
/// whatever else tells one window of a limit from its next, such as the app deciding
/// whether a spent limit was already mentioned, counts them the way readings do.
pub fn same_reset(a: i64, b: i64) -> bool {
    a.abs_diff(b) < SAME_RESET
}

/// One account's reading once its service has answered `answer` for that account's own
/// login, as of the answer's `answered_at`.
///
/// Where the answer lists every limit the account has
/// ([`Snapshot::lists_every_limit`]), its windows are the reading's: a limit it leaves out
/// is gone at once, whatever its reset, and numbers filed under the wrong account go at
/// that account's first answer. Otherwise each limit it gives replaces the one known, and
/// one it leaves out stands where it was until an answer taken after its reset; a window
/// with no reset is not running. An answer taken after everything known is what the limits
/// are then, and may lower a share, as a banked reset on claude.ai or a plan upgraded in the
/// middle of a window does.
///
/// Where a session moved a window after the answer was taken, or in the same second, its
/// higher share stands: it came with a later response than the answer. An answer older
/// than the last one known changes nothing. A time later than `now` was stamped by a clock
/// that ran ahead and orders nothing: taken as one, it held back every answer until the
/// clock caught up.
pub(crate) fn answered(known: Option<&Snapshot>, answer: &Snapshot, now: i64) -> Snapshot {
    let Some(known) = known else {
        return answer.clone();
    };
    let stamped = |at: Option<i64>| at.filter(|at| *at <= now);
    if answer.answered_at < stamped(known.answered_at) {
        return known.clone();
    }
    let moved_since = stamped(known.observed_at) >= answer.answered_at;
    let higher = |given: &Window| {
        known
            .windows
            .iter()
            .find(|had| moved_since && had.same_window(given) && had.percent > given.percent)
    };
    let take = |given: &Window| match higher(given) {
        Some(had) => Window {
            percent: had.percent,
            severity: had.severity.clone(),
            ..given.clone()
        },
        None => given.clone(),
    };
    let windows = if answer.lists_every_limit {
        answer.windows.iter().map(take).collect()
    } else {
        let answer_of = |had: &Window| answer.windows.iter().find(|given| given.same_limit(had));
        let running = |had: &Window| {
            had.resets_at
                .zip(answer.answered_at)
                .is_some_and(|(reset, taken)| reset > taken)
        };
        let added = answer
            .windows
            .iter()
            .filter(|given| !known.windows.iter().any(|had| had.same_limit(given)));
        known
            .windows
            .iter()
            .filter_map(|had| match answer_of(had) {
                Some(given) => Some(take(given)),
                None => running(had).then(|| had.clone()),
            })
            .chain(added.map(take))
            .collect()
    };
    Snapshot {
        windows,
        observed_at: stamped(known.observed_at).max(answer.observed_at),
        answered_at: answer.answered_at,
        lists_every_limit: answer.lists_every_limit,
        source: answer.source,
    }
}

/// `known` with what a session passed its status line, or `None` where that moves nothing.
///
/// A session says how much of each limit is used and when it resets, and nothing else, so
/// it only moves a limit the service gave this account. In the same window, a higher share
/// is taken. Where the window's reset has passed, or none was given, the passed window is
/// the next one. A window of another reset still ahead is not this account's, and a limit
/// `known` does not list is not this session's to add: a session never adds a limit and
/// never founds a reading. A passed window whose reset has passed, or that gives none, says
/// nothing about now.
///
/// What only the service says stays as it said: the limit's name, whether the account is
/// working against it, and how long it runs. A share a session moved is one the service has
/// not graded. A session's numbers carry no time, and moving one means the response that
/// did it has only just come, so the reading is as of `now`.
pub(crate) fn moved(known: &Snapshot, passed: &[Window], now: i64) -> Option<Snapshot> {
    let mut windows = known.windows.clone();
    let mut changed = false;
    for given in passed
        .iter()
        .filter(|w| w.resets_at.is_some_and(|at| at > now))
    {
        let Some(had) = windows.iter_mut().find(|had| had.same_limit(given)) else {
            continue;
        };
        let running = had.resets_at.is_some_and(|at| at > now);
        had.resets_at = match (had.same_window(given), running) {
            (true, _) if given.percent > had.percent => had.resets_at,
            (false, false) => given.resets_at,
            _ => continue,
        };
        had.percent = given.percent;
        had.severity = None;
        changed = true;
    }
    changed.then(|| Snapshot {
        windows,
        observed_at: Some(now),
        ..known.clone()
    })
}

/// A share of a limit. Past 100 is real, once a limit is exceeded; below zero is not.
fn percent(v: &Value) -> Option<f64> {
    let p = v.as_f64()?;
    (p.is_finite() && p >= 0.0).then_some(p)
}

fn window_from_limit(l: &Value) -> Option<Window> {
    // A row is scoped to a model or to a surface; either way the scope is what makes it
    // narrower than the account's own limit.
    let named = |what: &str| {
        l.get("scope")
            .and_then(|s| s.get(what))
            .and_then(|m| m.get("display_name"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    Some(Window {
        kind: l.get("kind")?.as_str()?.to_string(),
        scope: named("model").or_else(|| named("surface")),
        severity: l.get("severity").and_then(Value::as_str).map(str::to_owned),
        percent: percent(l.get("percent")?)?,
        resets_at: l
            .get("resets_at")
            .and_then(Value::as_str)
            .and_then(time::parse),
        is_active: l.get("is_active").and_then(Value::as_bool).unwrap_or(false),
        length_seconds: l
            .get("kind")
            .and_then(Value::as_str)
            .and_then(anthropic_window_length),
    })
}

fn window_from_named(kind: &str, v: &Value) -> Option<Window> {
    Some(Window {
        kind: kind.to_string(),
        scope: None,
        severity: None,
        percent: percent(v.get("utilization")?)?,
        resets_at: v
            .get("resets_at")
            .and_then(Value::as_str)
            .and_then(time::parse),
        is_active: false,
        length_seconds: anthropic_window_length(kind),
    })
}

/// A reading taken from Anthropic's usage endpoint just now.
///
/// Its `limits` are every limit the account has: read on 8 October 2026 from the answers
/// Claude Code 2.1.294 kept from `GET /api/oauth/usage` on one machine, where a Team seat's
/// holds `session` and one model's `weekly_scoped` and no `weekly_all`. The reading says so
/// only where `limits` held rows and every one was read. A row dropped for a value that
/// does not normalise is still a limit of the account's, and an answer without `limits`
/// names the five-hour and weekly limits alone.
pub fn from_usage_object(u: &Value, observed_at: i64) -> Snapshot {
    let limits = u.get("limits").and_then(Value::as_array);
    let mut windows: Vec<Window> = limits
        .into_iter()
        .flatten()
        .filter_map(window_from_limit)
        .collect();
    let lists_every_limit =
        !windows.is_empty() && limits.is_some_and(|rows| rows.len() == windows.len());
    if windows.is_empty() {
        for kind in ["five_hour", "seven_day"] {
            if let Some(w) = u.get(kind).and_then(|v| window_from_named(kind, v)) {
                windows.push(w);
            }
        }
    }
    Snapshot {
        windows,
        observed_at: Some(observed_at),
        answered_at: Some(observed_at),
        lists_every_limit,
        source: Source::Live,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An answer of Anthropic's usage endpoint as Claude Code kept it in this machine's
    /// `~/.claude.json`, trimmed.
    fn real_answer() -> Value {
        serde_json::json!({
        "five_hour": {"utilization": 62, "resets_at": "2026-09-20T22:20:00.095287+00:00"},
        "seven_day": {"utilization": 48, "resets_at": "2026-09-27T02:00:00.095306+00:00"},
        "limits": [
            {"kind": "session", "group": "session", "percent": 62,
             "resets_at": "2026-09-20T22:20:00.095287+00:00", "scope": null, "is_active": true},
            {"kind": "weekly_all", "group": "weekly", "percent": 48,
             "resets_at": "2026-09-27T02:00:00.095306+00:00", "scope": null, "is_active": false},
            {"kind": "weekly_scoped", "group": "weekly", "percent": 0,
             "resets_at": "2026-09-27T02:00:00+00:00",
             "scope": {"model": {"id": null, "display_name": "Fable"}}, "is_active": false}
        ]})
    }

    #[test]
    fn reads_the_real_answers_shape() {
        let s = from_usage_object(&real_answer(), 1789933772);
        assert_eq!(s.windows.len(), 3);
        assert_eq!(s.observed_at, Some(1789933772));
        assert_eq!(s.answered_at, Some(1789933772));
        assert!(s.lists_every_limit);
        assert_eq!(s.source, Source::Live);
        let scoped = s
            .windows
            .iter()
            .find(|w| w.kind == "weekly_scoped")
            .unwrap();
        assert_eq!(scoped.scope.as_deref(), Some("Fable"));
    }

    #[test]
    fn falls_back_to_the_named_windows_when_limits_is_missing() {
        let mut answer = real_answer();
        answer.as_object_mut().unwrap().remove("limits");
        let s = from_usage_object(&answer, 1789933772);
        assert_eq!(s.windows.len(), 2);
        assert_eq!(s.windows[0].kind, "five_hour");
        assert_eq!(s.windows[0].percent, 62.0);
        assert!(!s.lists_every_limit, "nothing scoped to a model is named");
    }

    #[test]
    fn a_nonsense_percentage_is_dropped_rather_than_drawn() {
        for nonsense in [serde_json::json!(-5), serde_json::json!("75")] {
            let mut answer = real_answer();
            answer["limits"][0]["percent"] = nonsense;
            let s = from_usage_object(&answer, 1789933772);
            assert_eq!(s.windows.len(), 2);
            assert!(
                !s.lists_every_limit,
                "the row dropped is a limit all the same"
            );
        }
    }

    #[test]
    fn an_exceeded_limit_is_kept_not_dropped() {
        let mut answer = real_answer();
        answer["limits"][0]["percent"] = serde_json::json!(104);
        let s = from_usage_object(&answer, 1789933772);
        assert_eq!(s.windows.len(), 3);
        assert_eq!(s.windows[0].percent, 104.0);
    }

    #[test]
    fn a_share_is_one_whole_percentage_a_half_rounded_up() {
        assert_eq!(whole(94.5), 95);
        assert_eq!(whole(94.49), 94);
        assert_eq!(whole(104.0), 104);
        assert_eq!(whole(0.4), 0);
    }

    const NOW: i64 = 1_789_935_000;
    const HOUR: i64 = 3_600;

    fn measured(kind: &str, percent: f64, resets_at: Option<i64>) -> Window {
        Window {
            kind: kind.into(),
            scope: None,
            percent,
            resets_at,
            is_active: true,
            severity: None,
            length_seconds: anthropic_window_length(kind),
        }
    }

    /// Anthropic's answer about an account, listing every limit it has, taken at `at`.
    fn answer(windows: Vec<Window>, at: i64) -> Snapshot {
        Snapshot {
            windows,
            observed_at: Some(at),
            answered_at: Some(at),
            lists_every_limit: true,
            source: Source::Live,
        }
    }

    /// What Pitboard holds of an account: last moved or confirmed at `observed_at`, and last
    /// answered for at `answered_at`.
    fn held(windows: Vec<Window>, observed_at: i64, answered_at: Option<i64>) -> Snapshot {
        Snapshot {
            windows,
            observed_at: Some(observed_at),
            answered_at,
            lists_every_limit: true,
            source: Source::Remembered,
        }
    }

    fn shares(reading: &Snapshot) -> Vec<(&str, f64)> {
        reading
            .windows
            .iter()
            .map(|w| (w.kind.as_str(), w.percent))
            .collect()
    }

    /// The rule the app keys what it has told somebody by: resets under a minute apart, in
    /// either order, are one, and a minute apart are two.
    #[test]
    fn resets_under_a_minute_apart_are_one_reset() {
        assert!(same_reset(NOW, NOW));
        assert!(same_reset(NOW, NOW + 59));
        assert!(same_reset(NOW + 59, NOW));
        assert!(!same_reset(NOW, NOW + 60));
        assert!(!same_reset(NOW + 60, NOW));
        assert!(
            !same_reset(0, NOW),
            "no reset known is no reset in particular"
        );
    }

    /// An answer lists every limit the account has. Kept until its reset, a weekly limit
    /// 0.9.0 filed under a Team seat, whose answer has no weekly limit for all models, stood
    /// over every answer about that seat for days.
    #[test]
    fn an_answer_replaces_every_limit_it_does_not_give() {
        let known = held(
            vec![
                measured("session", 10.0, Some(NOW + HOUR)),
                measured("weekly_all", 100.0, Some(NOW + 50 * HOUR)),
            ],
            NOW - 60,
            None,
        );
        let given = answer(
            vec![
                measured("session", 20.0, Some(NOW + HOUR)),
                Window {
                    scope: Some("Fable".into()),
                    ..measured("weekly_scoped", 0.0, Some(NOW + 50 * HOUR))
                },
            ],
            NOW,
        );
        let reading = answered(Some(&known), &given, NOW);
        assert_eq!(
            shares(&reading),
            [("session", 20.0), ("weekly_scoped", 0.0)]
        );
        assert_eq!(reading.answered_at, Some(NOW));
        assert_eq!(reading.observed_at, Some(NOW));
    }

    /// Anthropic's answer lists every limit, but Pitboard knows what one left out only where
    /// it read every row. Taken as the whole list, an answer with a row it could not read
    /// took a used-up weekly limit off the account.
    #[test]
    fn a_limit_whose_row_could_not_be_read_is_not_taken_away() {
        let mut spent = real_answer();
        spent["limits"][1]["percent"] = serde_json::json!(100);
        let known = from_usage_object(&spent, NOW - 600);
        let mut unread = spent.clone();
        unread["limits"][0]["percent"] = serde_json::json!(70);
        unread["limits"][1]["percent"] = Value::Null;
        let reading = answered(Some(&known), &from_usage_object(&unread, NOW), NOW);
        assert_eq!(
            shares(&reading),
            [
                ("session", 70.0),
                ("weekly_all", 100.0),
                ("weekly_scoped", 0.0)
            ]
        );
    }

    /// An answer with no `limits` names the five-hour and weekly limits and nothing scoped to
    /// a model, so a model's limit it leaves out is one it has no name for.
    #[test]
    fn an_answer_in_the_older_shape_takes_no_limit_away() {
        let mut spent = real_answer();
        spent["limits"][2]["percent"] = serde_json::json!(100);
        let known = from_usage_object(&spent, NOW - 600);
        let mut older = real_answer();
        older.as_object_mut().unwrap().remove("limits");
        let reading = answered(Some(&known), &from_usage_object(&older, NOW), NOW);
        assert_eq!(
            shares(&reading),
            [
                ("five_hour", 62.0),
                ("seven_day", 48.0),
                ("weekly_scoped", 100.0)
            ]
        );
    }

    /// A clock that ran ahead stamps a reading later than now. Taken as a later answer, that
    /// stamp held every answer back until the clock caught up, and a session's share with it.
    #[test]
    fn a_reading_stamped_ahead_of_the_clock_holds_no_answer_back() {
        let known = held(
            vec![measured("session", 80.0, Some(NOW + 2 * HOUR))],
            NOW + HOUR,
            Some(NOW + HOUR),
        );
        let given = answer(vec![measured("session", 20.0, Some(NOW + 2 * HOUR))], NOW);
        let reading = answered(Some(&known), &given, NOW);
        assert_eq!(shares(&reading), [("session", 20.0)]);
        assert_eq!(
            (reading.observed_at, reading.answered_at),
            (Some(NOW), Some(NOW))
        );
    }

    /// Whether OpenAI's null window is one not running is not measured, so a window its
    /// answer leaves out stands while it runs, and where it was: `pitboard status` and the
    /// menu list limits in a reading's order. One whose reset has passed, or that has none,
    /// is not running.
    #[test]
    fn a_window_openai_leaves_out_stands_until_its_reset() {
        let known = held(
            vec![
                measured("five_hour", 30.0, Some(NOW + HOUR)),
                measured("1_day", 60.0, Some(NOW - 60)),
                measured("2_day", 20.0, None),
                measured("seven_day", 40.0, Some(NOW + 50 * HOUR)),
            ],
            NOW - 600,
            Some(NOW - 600),
        );
        let whole = answer(
            vec![measured("seven_day", 45.0, Some(NOW + 50 * HOUR))],
            NOW,
        );
        let given = Snapshot {
            lists_every_limit: false,
            ..whole.clone()
        };
        let reading = answered(Some(&known), &given, NOW);
        assert_eq!(shares(&reading), [("five_hour", 30.0), ("seven_day", 45.0)]);
        assert!(!reading.lists_every_limit);
        assert_eq!(
            shares(&answered(Some(&known), &whole, NOW)),
            [("seven_day", 45.0)],
            "where the answer lists every limit"
        );
    }

    /// A session's numbers do not say whose they are. Within a window the service gave this
    /// account they can only be this account's, and anywhere else they can be anybody's.
    #[test]
    fn a_session_moves_a_share_only_inside_a_window_it_was_given() {
        let known = held(
            vec![measured("session", 22.0, Some(NOW + HOUR))],
            NOW - 60,
            Some(NOW - 60),
        );
        let higher = moved(
            &known,
            &[measured("five_hour", 25.0, Some(NOW + HOUR))],
            NOW,
        )
        .expect("a share moved");
        assert_eq!(
            shares(&higher),
            [("session", 25.0)],
            "under the name the service gave it"
        );
        assert_eq!(higher.answered_at, Some(NOW - 60), "and no answer since");
        assert_eq!(
            moved(
                &known,
                &[measured("five_hour", 5.0, Some(NOW + 6 * HOUR))],
                NOW
            ),
            None,
            "another reset still ahead"
        );
        assert_eq!(
            moved(
                &known,
                &[measured("seven_day", 40.0, Some(NOW + 50 * HOUR))],
                NOW
            ),
            None,
            "a limit the service did not give"
        );
    }

    /// A limit's next window starts only once the last has reset, so a session passing one
    /// after the reset is passing this account's next window. A window with no reset is one
    /// that is not running.
    #[test]
    fn a_session_takes_the_next_window_of_a_limit_whose_reset_has_passed() {
        let mut over = measured("session", 90.0, Some(NOW - 60));
        over.severity = Some("warning".into());
        let not_running = measured("session", 0.0, None);
        for had in [over, not_running] {
            let known = held(vec![had.clone()], NOW - HOUR, Some(NOW - HOUR));
            let next = moved(
                &known,
                &[measured("five_hour", 3.0, Some(NOW + 5 * HOUR))],
                NOW,
            )
            .expect("the next window");
            let window = &next.windows[0];
            assert_eq!(window.kind, "session", "{had:?}");
            assert_eq!(
                (window.percent, window.resets_at),
                (3.0, Some(NOW + 5 * HOUR))
            );
            assert_eq!(window.severity, None, "a share the service has not graded");
            assert!(window.is_active, "working against it as the service said");
            assert_eq!(next.observed_at, Some(NOW));
            assert_eq!(next.answered_at, Some(NOW - HOUR));
        }
    }

    /// A session passes the numbers of its last response, however long ago that was. An
    /// idle one goes on passing a lower share than a busy one has since recorded, and a pane
    /// left open past a reset goes on passing a window that is over.
    #[test]
    fn a_session_never_moves_a_share_back() {
        let known = held(
            vec![measured("session", 22.0, Some(NOW + HOUR))],
            NOW - 60,
            Some(NOW - 60),
        );
        for behind in [
            measured("five_hour", 20.0, Some(NOW + HOUR)),
            measured("five_hour", 22.0, Some(NOW + HOUR)),
            measured("five_hour", 95.0, Some(NOW - 4 * HOUR)),
        ] {
            assert_eq!(
                moved(&known, std::slice::from_ref(&behind), NOW),
                None,
                "{behind:?}"
            );
        }
    }

    /// Anthropic's answer gives a reset to a fraction of a second, which is dropped, and a
    /// session is given whole seconds. Taken for another window, a session's higher share
    /// would be taken for another account's.
    #[test]
    fn resets_a_second_apart_are_one_window() {
        let known = held(
            vec![measured("session", 22.0, Some(NOW + HOUR))],
            NOW - 60,
            Some(NOW - 60),
        );
        let ahead = moved(
            &known,
            &[measured("five_hour", 25.0, Some(NOW + HOUR + 1))],
            NOW,
        )
        .expect("a share moved");
        assert_eq!(shares(&ahead), [("session", 25.0)]);
        assert_eq!(
            ahead.windows[0].resets_at,
            Some(NOW + HOUR),
            "the reset is the service's"
        );
    }

    /// Measured on this machine on 2026-09-29: an account at 100% of its weekly limit,
    /// resetting at 20:00 UTC the next day, had a banked reset used on claude.ai. Its
    /// sessions then passed 1%, 13% and 14% of the same limit, with the same reset. Ordered
    /// by share, every lower answer lost to the 100%, and the account read as out for the
    /// day and a half until the reset.
    #[test]
    fn an_answer_taken_after_everything_known_is_what_the_limits_are_now() {
        let known = held(
            vec![
                measured("session", 0.0, None),
                measured("weekly_all", 100.0, Some(NOW + 33 * HOUR)),
            ],
            NOW - HOUR,
            Some(NOW - HOUR),
        );
        let given = answer(
            vec![
                measured("session", 5.0, Some(NOW + 5 * HOUR)),
                measured("weekly_all", 14.0, Some(NOW + 33 * HOUR)),
            ],
            NOW,
        );
        let reading = answered(Some(&known), &given, NOW);
        assert_eq!(shares(&reading), [("session", 5.0), ("weekly_all", 14.0)]);
        assert_eq!(reading.observed_at, Some(NOW));
        assert_eq!(reading.source, Source::Live);

        let unused = answer(vec![measured("weekly_all", 0.0, None)], NOW);
        assert_eq!(
            shares(&answered(Some(&known), &unused, NOW)),
            [("weekly_all", 0.0)],
            "and one that finds nothing used and no window running"
        );
    }

    /// An answer is what the limits were when it was taken. A share a session recorded
    /// since, or in the same second, may have come with a later response.
    #[test]
    fn an_answer_taken_before_a_session_moved_a_share_does_not_lower_it() {
        let known = held(
            vec![measured("weekly_all", 100.0, Some(NOW + 33 * HOUR))],
            NOW - 60,
            Some(NOW - 2 * HOUR),
        );
        for taken in [NOW - 60, NOW - HOUR] {
            let given = answer(
                vec![measured("weekly_all", 14.0, Some(NOW + 33 * HOUR))],
                taken,
            );
            let reading = answered(Some(&known), &given, NOW);
            assert_eq!(
                shares(&reading),
                [("weekly_all", 100.0)],
                "taken at {taken}"
            );
            assert_eq!(reading.observed_at, Some(NOW - 60));
            assert_eq!(reading.answered_at, Some(taken));
        }
    }

    /// Two front ends can each have a request out, and the older answer can land last.
    #[test]
    fn an_answer_older_than_the_last_changes_nothing() {
        let known = held(
            vec![measured("session", 40.0, Some(NOW + HOUR))],
            NOW,
            Some(NOW),
        );
        let late = answer(vec![measured("session", 30.0, Some(NOW + HOUR))], NOW - 60);
        assert_eq!(answered(Some(&known), &late, NOW), known);
    }

    /// A session knows the five-hour and weekly limits and nothing scoped to a model.
    #[test]
    fn a_session_leaves_the_limits_it_does_not_pass_as_they_were() {
        let scoped = Window {
            scope: Some("Fable".into()),
            ..measured("weekly_scoped", 5.0, Some(NOW + 50 * HOUR))
        };
        let known = held(
            vec![measured("session", 22.0, Some(NOW + HOUR)), scoped],
            NOW - 60,
            Some(NOW - 60),
        );
        let reading = moved(
            &known,
            &[measured("five_hour", 25.0, Some(NOW + HOUR))],
            NOW,
        )
        .expect("a share moved");
        assert_eq!(
            shares(&reading),
            [("session", 25.0), ("weekly_scoped", 5.0)]
        );
    }

    /// A session leaves out a window whose reset has passed. Taken for the service no
    /// longer reporting it, every reset took the five-hour limit away until the next answer,
    /// where it reads as nothing used.
    #[test]
    fn a_session_never_takes_a_limit_away() {
        let known = held(
            vec![
                measured("session", 40.0, Some(NOW - 10)),
                measured("weekly_all", 30.0, Some(NOW + 50 * HOUR)),
            ],
            NOW - HOUR,
            Some(NOW - HOUR),
        );
        let reading = moved(
            &known,
            &[measured("seven_day", 31.0, Some(NOW + 50 * HOUR))],
            NOW,
        )
        .expect("a share moved");
        assert_eq!(shares(&reading), [("session", 40.0), ("weekly_all", 31.0)]);
        assert_eq!(reading.windows[0].used(NOW), 0.0);
    }

    /// A reading is founded by the service's answer and by nothing else.
    #[test]
    fn a_first_answer_is_the_reading() {
        let given = answer(vec![measured("session", 22.0, Some(NOW + HOUR))], NOW);
        assert_eq!(answered(None, &given, NOW), given);
        assert_eq!(
            moved(&held(Vec::new(), NOW, None), &given.windows, NOW),
            None
        );
    }

    /// A session passes the numbers of its last response, however long ago that was, and
    /// says nothing about when. Repeating them vouches for nothing; moving one forward means
    /// the response that did it has only just come.
    #[test]
    fn a_reading_is_stamped_when_something_moves_or_the_service_answers() {
        let known = held(
            vec![measured("session", 22.0, Some(NOW + HOUR))],
            NOW - 600,
            Some(NOW - 600),
        );
        let repeated = measured("five_hour", 22.0, Some(NOW + HOUR));
        assert_eq!(moved(&known, &[repeated], NOW), None);

        let ahead = measured("five_hour", 23.0, Some(NOW + HOUR));
        assert_eq!(moved(&known, &[ahead], NOW).unwrap().observed_at, Some(NOW));

        let confirmed = answer(known.windows.clone(), NOW - 5);
        let reading = answered(Some(&known), &confirmed, NOW);
        assert_eq!(reading.windows, known.windows);
        assert_eq!(
            (reading.observed_at, reading.answered_at),
            (Some(NOW - 5), Some(NOW - 5)),
            "an answer that finds the same confirms it"
        );
        assert_eq!(reading.source, Source::Live);
    }

    /// Asked about an account that has done nothing since its window reset, Anthropic finds
    /// nothing used and gives no reset, or the one that passed. Kept, a parked account that
    /// had run out read as full, marked live, until somebody used it again.
    #[test]
    fn a_window_past_its_reset_is_reset_by_an_answer_that_finds_nothing_used() {
        let known = held(
            vec![measured("session", 100.0, Some(NOW - HOUR))],
            NOW - 2 * HOUR,
            Some(NOW - 2 * HOUR),
        );
        for said in [
            measured("session", 0.0, None),
            measured("session", 0.0, Some(NOW - HOUR)),
            measured("five_hour", 0.0, None),
        ] {
            let given = answer(vec![said.clone()], NOW - 5);
            let reading = answered(Some(&known), &given, NOW);
            assert_eq!(reading.windows, std::slice::from_ref(&said), "{said:?}");
            assert_eq!(
                answered(Some(&reading), &given, NOW),
                reading,
                "a repeat changes nothing"
            );
        }
        assert_eq!(
            moved(&known, &[measured("five_hour", 0.0, None)], NOW),
            None,
            "a session's window with no reset says nothing about now"
        );
    }
}
