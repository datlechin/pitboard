//! One view of "how much is left", whatever shape it arrived in. Usage comes as a
//! `limits[]` array or as named `five_hour`/`seven_day` objects; both are normalised at the
//! boundary, and a value that fails to normalise is dropped rather than drawn.

use crate::time;
use serde_json::Value;
use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Window {
    pub kind: String,
    pub scope: Option<String>,
    pub percent: f64,
    pub resets_at: Option<i64>,
    pub is_active: bool,
    /// How Anthropic grades this row, when it grades it. Its word, not a threshold of
    /// pitboard's own, and absent in a reading taken before pitboard read this field.
    #[serde(default)]
    pub severity: Option<String>,
    /// How long the window runs, in seconds, where that is known.
    ///
    /// What makes a limit comparable to itself over time: a reset time alone cannot say
    /// how long a window is, because the time left shrinks as the window runs out. Stated
    /// outright by OpenAI, implied by the kind for Anthropic, and absent from a reading
    /// taken before pitboard kept it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_seconds: Option<i64>,
}

/// How long one of Anthropic's windows runs, from its kind.
///
/// Anthropic names its windows rather than timing them: `session` and the older
/// `five_hour` are the five-hour limit, and every `weekly_` kind, like the older
/// `seven_day`, runs a week. A kind not listed here has no length pitboard can vouch for.
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
    /// Copied from Claude Code's own cache, which it refreshes only when it asks.
    ClaudeCodeCache,
    /// The last live reading pitboard took itself.
    Remembered,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Snapshot {
    pub windows: Vec<Window>,
    pub observed_at: Option<i64>,
    pub account_uuid: Option<String>,
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
        limit(&self.kind) == limit(&other.kind) && self.scope == other.scope
    }

    /// Whether `other` is this very window: the same limit, resetting at the same time as
    /// far as sources agree on one. A window with no reset time is no window in particular.
    pub(crate) fn same_window(&self, other: &Window) -> bool {
        self.same_limit(other)
            && matches!(
                (self.resets_at, other.resets_at),
                (Some(x), Some(y)) if x.abs_diff(y) < SAME_RESET
            )
    }
}

/// A limit by one name. A Claude Code session, and Anthropic's answer when it has no
/// `limits`, use the older `five_hour` and `seven_day` for the limits `limits` calls
/// `session` and `weekly_all`. Codex's windows borrow the older names for their lengths,
/// which is harmless: one account's readings are only ever compared with each other.
fn limit(kind: &str) -> &str {
    match kind {
        "five_hour" => "session",
        "seven_day" => "weekly_all",
        other => other,
    }
}

/// Resets closer together than this are one reset.
///
/// Sources do not agree to the second on when a window resets: Anthropic's answer gives a
/// fraction of a second, which is dropped, and a Claude Code session is given whole seconds.
/// A limit's next window starts only once the last has reset, and the shortest window any
/// service has shown runs five hours, so resets a minute apart are rounding and never two
/// windows.
const SAME_RESET: u64 = 60;

/// Which of two measurements of one account's limit is the newer: `Greater` when `a` is.
///
/// A later reset is a later window, whatever its share. Within one window use only rises,
/// so while the limit stays the same the higher share was measured later. A window whose
/// reset has passed counts as reset, with nothing used, and one with no reset time is
/// compared by its share alone.
///
/// No timestamp is needed, which is the point: a Claude Code session passes its limits with
/// none. They are what its last response said, however long ago that was, and a session
/// left open passes the same old numbers every time its status line runs. The reset time is
/// the service's own, and use within a window only rises, so the numbers order themselves.
///
/// Both must be the account's own: another account's windows order against its own just as
/// readily. A session's numbers do not say whose they are, so the status line offers only
/// what moved between two of a session's runs with the same account named both times, and
/// leaves out any whose reset shows them to be another account's. And where the service
/// lowers a share within a window, as a plan upgraded in the middle of one does by raising
/// the limit, the old, higher share stands until the window resets.
pub(crate) fn recency(a: &Window, b: &Window, now: i64) -> Ordering {
    match (a.resets_at, b.resets_at) {
        (Some(x), Some(y)) if x.abs_diff(y) >= SAME_RESET => x.cmp(&y),
        _ => a.used(now).total_cmp(&b.used(now)),
    }
}

/// One account's reading with `offered` folded in, limit by limit: each limit keeps the
/// newer measurement by [`recency`], taken whole, and on a tie the one already known, so a
/// repeat changes nothing, whatever name or rounding it came with.
///
/// Except where the known window's reset has passed. A tie there is a reading that finds
/// nothing used since, which is what an account nobody has used since says, with no reset or
/// with the one that passed. Kept, the old share stood for as long as the account went
/// unused. So the offered window is taken, or where its reset has passed too, the limit is
/// recorded as nothing used and no reset, which a repeat then ties with and leaves alone.
/// Nothing new was measured, so this confirms the reading rather than advancing it.
///
/// A limit only one of them measured is kept, because a reading can speak for fewer limits
/// than there are: a session knows the five-hour and weekly limits and nothing scoped to a
/// model. One that only `known` has goes once its reset has passed and an answer the
/// service has just given leaves it out, so a limit the service stops reporting is not shown
/// for ever. Nothing else takes a limit away: a session leaves out a window whose reset has
/// passed, and taken for the service no longer reporting it, every reset took the five-hour
/// limit off the status line, `pitboard status` and the menu bar until the next answer.
/// Kept, the status line reads it as nothing used.
///
/// `observed_at` is the latest time anything confirmed or advanced the reading. One offered
/// without a time, which is what a session passes, is stamped `now` when it moves something
/// forward, and vouches for nothing when it only repeats what is known.
pub(crate) fn merge(
    known: Option<&Snapshot>,
    offered: Option<&Snapshot>,
    now: i64,
) -> Option<Snapshot> {
    let Some(offered) = offered else {
        return known.cloned();
    };
    let Some(known) = known else {
        let mut first = offered.clone();
        first.observed_at = first.observed_at.or(Some(now));
        return Some(first);
    };
    let answered = offered.source == Source::Live && offered.observed_at.is_some();
    let (mut advanced, mut confirmed) = (false, false);
    let mut windows = Vec::new();
    for had in &known.windows {
        match offered.windows.iter().find(|w| w.same_limit(had)) {
            Some(given) => match recency(given, had, now) {
                Ordering::Less => windows.push(had.clone()),
                Ordering::Equal if had.resets_at.is_some_and(|at| at <= now) => {
                    confirmed = true;
                    windows.push(if given.resets_at.is_none_or(|at| at > now) {
                        given.clone()
                    } else {
                        Window {
                            percent: 0.0,
                            resets_at: None,
                            ..had.clone()
                        }
                    });
                }
                Ordering::Equal => {
                    confirmed = true;
                    windows.push(had.clone());
                }
                Ordering::Greater => {
                    advanced = true;
                    windows.push(given.clone());
                }
            },
            None if answered && had.resets_at.is_some_and(|at| at <= now) => {}
            None => windows.push(had.clone()),
        }
    }
    for given in &offered.windows {
        if !known.windows.iter().any(|w| w.same_limit(given)) {
            advanced = true;
            windows.push(given.clone());
        }
    }
    let vouched = advanced || (confirmed && offered.observed_at.is_some());
    Some(Snapshot {
        windows,
        observed_at: if advanced {
            known
                .observed_at
                .max(Some(offered.observed_at.unwrap_or(now)))
        } else if confirmed {
            known.observed_at.max(offered.observed_at)
        } else {
            known.observed_at
        },
        account_uuid: known
            .account_uuid
            .clone()
            .or_else(|| offered.account_uuid.clone()),
        source: if vouched {
            offered.source
        } else {
            known.source
        },
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

/// The API answer and Claude Code's cached copy of it share this shape.
fn windows_of(u: &Value) -> Vec<Window> {
    let mut windows: Vec<Window> = u
        .get("limits")
        .and_then(Value::as_array)
        .map(|ls| ls.iter().filter_map(window_from_limit).collect())
        .unwrap_or_default();
    if windows.is_empty() {
        for kind in ["five_hour", "seven_day"] {
            if let Some(w) = u.get(kind).and_then(|v| window_from_named(kind, v)) {
                windows.push(w);
            }
        }
    }
    windows
}

/// A reading taken from Anthropic's usage endpoint just now.
pub fn from_usage_object(u: &Value, observed_at: i64) -> Snapshot {
    Snapshot {
        windows: windows_of(u),
        observed_at: Some(observed_at),
        account_uuid: None,
        source: Source::Live,
    }
}

/// Claude Code's own cache. It records the account it was measured for, so a reading for
/// another account can be told apart and ignored.
pub fn from_config_cache(config: &Value) -> Option<Snapshot> {
    let c = config.get("cachedUsageUtilization")?;
    Some(Snapshot {
        windows: windows_of(c.get("utilization")?),
        observed_at: c
            .get("fetchedAtMs")
            .and_then(Value::as_i64)
            .map(|ms| ms / 1000),
        account_uuid: c
            .get("accountUuid")
            .and_then(Value::as_str)
            .map(str::to_owned),
        source: Source::ClaudeCodeCache,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from this machine's real `~/.claude.json`.
    fn real_config() -> Value {
        serde_json::json!({"cachedUsageUtilization": {
        "fetchedAtMs": 1789933772292i64,
        "accountUuid": "1f0e2d3c-4b5a-4968-8776-a5b4c3d2e1f0",
        "utilization": {
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
            ]}}})
    }

    #[test]
    fn reads_the_real_cache_shape() {
        let s = from_config_cache(&real_config()).expect("should parse");
        assert_eq!(s.windows.len(), 3);
        assert_eq!(
            s.account_uuid.as_deref(),
            Some("1f0e2d3c-4b5a-4968-8776-a5b4c3d2e1f0")
        );
        assert_eq!(s.observed_at, Some(1789933772));
        let scoped = s
            .windows
            .iter()
            .find(|w| w.kind == "weekly_scoped")
            .unwrap();
        assert_eq!(scoped.scope.as_deref(), Some("Fable"));
    }

    #[test]
    fn falls_back_to_the_named_windows_when_limits_is_missing() {
        let mut c = real_config();
        c["cachedUsageUtilization"]["utilization"]
            .as_object_mut()
            .unwrap()
            .remove("limits");
        let s = from_config_cache(&c).unwrap();
        assert_eq!(s.windows.len(), 2);
        assert_eq!(s.windows[0].kind, "five_hour");
        assert_eq!(s.windows[0].percent, 62.0);
    }

    #[test]
    fn a_nonsense_percentage_is_dropped_rather_than_drawn() {
        for nonsense in [serde_json::json!(-5), serde_json::json!("75")] {
            let mut c = real_config();
            c["cachedUsageUtilization"]["utilization"]["limits"][0]["percent"] = nonsense;
            assert_eq!(from_config_cache(&c).unwrap().windows.len(), 2);
        }
    }

    #[test]
    fn an_exceeded_limit_is_kept_not_dropped() {
        let mut c = real_config();
        c["cachedUsageUtilization"]["utilization"]["limits"][0]["percent"] = serde_json::json!(104);
        let s = from_config_cache(&c).unwrap();
        assert_eq!(s.windows.len(), 3);
        assert_eq!(s.windows[0].percent, 104.0);
    }

    #[test]
    fn missing_cache_is_not_an_error() {
        assert!(from_config_cache(&serde_json::json!({})).is_none());
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

    fn reading(windows: Vec<Window>, observed_at: Option<i64>) -> Snapshot {
        Snapshot {
            windows,
            observed_at,
            account_uuid: None,
            source: Source::Live,
        }
    }

    fn shares(reading: &Snapshot) -> Vec<(&str, f64)> {
        reading
            .windows
            .iter()
            .map(|w| (w.kind.as_str(), w.percent))
            .collect()
    }

    #[test]
    fn a_later_reset_is_a_newer_window_whatever_its_share() {
        let full = measured("session", 90.0, Some(NOW + HOUR));
        let next = measured("session", 2.0, Some(NOW + 6 * HOUR));
        assert_eq!(recency(&next, &full, NOW), Ordering::Greater);
        assert_eq!(recency(&full, &next, NOW), Ordering::Less);
    }

    #[test]
    fn within_one_window_the_higher_share_is_the_newer() {
        let earlier = measured("session", 20.0, Some(NOW + HOUR));
        let later = measured("session", 22.0, Some(NOW + HOUR));
        assert_eq!(recency(&later, &earlier, NOW), Ordering::Greater);
        assert_eq!(recency(&earlier, &later, NOW), Ordering::Less);
    }

    /// Anthropic's answer gives a reset to a fraction of a second, which is dropped, and a
    /// session is given whole seconds. Taken for a newer window, the session's older 20%
    /// would win over the 22% the service has just measured.
    #[test]
    fn resets_a_second_apart_are_one_window() {
        let answered = measured("session", 22.0, Some(NOW + HOUR));
        let passed = measured("five_hour", 20.0, Some(NOW + HOUR + 1));
        assert_eq!(recency(&passed, &answered, NOW), Ordering::Less);
        assert_eq!(recency(&answered, &passed, NOW), Ordering::Greater);
    }

    /// A window that has reset has nothing used, however full it was, so any use of the
    /// limit since is newer, even from a reading that does not say when it resets.
    #[test]
    fn a_window_past_its_reset_counts_as_reset() {
        let over = measured("session", 90.0, Some(NOW - 1));
        let begun = measured("session", 5.0, None);
        assert_eq!(recency(&begun, &over, NOW), Ordering::Greater);
        assert_eq!(recency(&over, &begun, NOW), Ordering::Less);
    }

    #[test]
    fn a_reading_never_moves_a_limit_backwards() {
        let known = reading(vec![measured("session", 22.0, Some(NOW + HOUR))], Some(NOW));
        for behind in [
            measured("five_hour", 20.0, Some(NOW + HOUR)),
            measured("five_hour", 95.0, Some(NOW - 4 * HOUR)),
        ] {
            let merged = merge(Some(&known), Some(&reading(vec![behind], None)), NOW).unwrap();
            assert_eq!(shares(&merged), [("session", 22.0)]);
        }
        let ahead = reading(vec![measured("five_hour", 25.0, Some(NOW + HOUR))], None);
        let merged = merge(Some(&known), Some(&ahead), NOW).unwrap();
        assert_eq!(
            shares(&merged),
            [("five_hour", 25.0)],
            "under the name it came with"
        );
    }

    /// A session knows the five-hour and weekly limits and nothing scoped to a model, so a
    /// limit one reading leaves out is not a limit that has gone.
    #[test]
    fn a_limit_only_one_reading_measured_is_kept() {
        let scoped = Window {
            scope: Some("Fable".into()),
            ..measured("weekly_scoped", 5.0, Some(NOW + 50 * HOUR))
        };
        let known = reading(
            vec![measured("session", 22.0, Some(NOW + HOUR)), scoped],
            Some(NOW - 60),
        );
        let offered = reading(
            vec![
                measured("five_hour", 25.0, Some(NOW + HOUR)),
                measured("seven_day", 40.0, Some(NOW + 50 * HOUR)),
            ],
            None,
        );
        let merged = merge(Some(&known), Some(&offered), NOW).unwrap();
        assert_eq!(
            shares(&merged),
            [
                ("five_hour", 25.0),
                ("weekly_scoped", 5.0),
                ("seven_day", 40.0)
            ]
        );
    }

    /// Kept for ever, a limit the service stopped reporting would be shown for ever. Once
    /// its window is over, an answer that leaves it out is the service no longer reporting
    /// it, and there is nothing left in it to show.
    #[test]
    fn a_limit_an_answer_leaves_out_goes_once_its_reset_has_passed() {
        let known = reading(
            vec![
                measured("session", 22.0, Some(NOW + HOUR)),
                measured("weekly_scoped", 5.0, Some(NOW - 1)),
            ],
            Some(NOW - 60),
        );
        let answered = reading(vec![measured("session", 25.0, Some(NOW + HOUR))], Some(NOW));
        let merged = merge(Some(&known), Some(&answered), NOW).unwrap();
        assert_eq!(shares(&merged), [("session", 25.0)]);
    }

    /// A session says no time and leaves out a window whose reset has passed. Taken for the
    /// service no longer reporting it, every reset took the five-hour limit away until the
    /// next answer, where it reads as nothing used. Claude Code's cache is an answer as of
    /// whenever Claude Code last asked, so it takes nothing away either.
    #[test]
    fn a_reading_that_is_not_an_answer_just_now_never_takes_a_limit_away() {
        let known = reading(
            vec![
                measured("session", 40.0, Some(NOW - 10)),
                measured("weekly_all", 30.0, Some(NOW + 50 * HOUR)),
            ],
            Some(NOW - HOUR),
        );
        let passed = reading(
            vec![measured("seven_day", 31.0, Some(NOW + 50 * HOUR))],
            None,
        );
        let mut cached = reading(passed.windows.clone(), Some(NOW - 5));
        cached.source = Source::ClaudeCodeCache;
        for offered in [passed, cached] {
            let merged = merge(Some(&known), Some(&offered), NOW).unwrap();
            assert_eq!(
                shares(&merged),
                [("session", 40.0), ("seven_day", 31.0)],
                "{:?}",
                offered.source
            );
            assert_eq!(merged.windows[0].used(NOW), 0.0);
        }
    }

    #[test]
    fn with_one_side_absent_the_other_stands() {
        let known = reading(
            vec![measured("session", 22.0, Some(NOW + HOUR))],
            Some(NOW - 60),
        );
        assert_eq!(merge(Some(&known), None, NOW), Some(known.clone()));
        assert_eq!(merge(None, None, NOW), None);

        let first = merge(None, Some(&reading(known.windows.clone(), None)), NOW).unwrap();
        assert_eq!(shares(&first), [("session", 22.0)]);
        assert_eq!(first.observed_at, Some(NOW), "stamped when it arrived");
    }

    /// A session passes the numbers of its last response, however long ago that was, and
    /// says nothing about when. Repeating them vouches for nothing; moving one forward means
    /// the response that did it has only just come.
    #[test]
    fn a_reading_that_says_no_time_is_stamped_only_when_it_moves_something() {
        let mut known = reading(
            vec![measured("session", 22.0, Some(NOW + HOUR))],
            Some(NOW - 600),
        );
        known.source = Source::Remembered;
        let repeated = reading(vec![measured("five_hour", 22.0, Some(NOW + HOUR))], None);
        let merged = merge(Some(&known), Some(&repeated), NOW).unwrap();
        assert_eq!(
            merged.windows, known.windows,
            "a repeat changes nothing, name and all"
        );
        assert_eq!(merged.observed_at, Some(NOW - 600));
        assert_eq!(
            merged.source,
            Source::Remembered,
            "and it is still what was known"
        );

        let moved = reading(vec![measured("five_hour", 23.0, Some(NOW + HOUR))], None);
        assert_eq!(
            merge(Some(&known), Some(&moved), NOW).unwrap().observed_at,
            Some(NOW)
        );

        let mut confirmed = reading(known.windows.clone(), Some(NOW - 5));
        confirmed.source = Source::ClaudeCodeCache;
        let merged = merge(Some(&known), Some(&confirmed), NOW).unwrap();
        assert_eq!(
            merged.observed_at,
            Some(NOW - 5),
            "a measured repeat confirms it"
        );
        assert_eq!(merged.source, Source::ClaudeCodeCache);
    }

    /// Asked about an account that has done nothing since its window reset, Anthropic finds
    /// nothing used and gives no reset, or the one that passed, and Claude Code's cache says
    /// the same offline. Kept on that tie, a parked account that had run out read as full,
    /// marked live, until somebody used it again.
    #[test]
    fn a_window_past_its_reset_is_reset_by_a_reading_that_finds_nothing_used() {
        let known = reading(
            vec![measured("session", 100.0, Some(NOW - HOUR))],
            Some(NOW - 2 * HOUR),
        );
        for (said, source) in [
            (measured("session", 0.0, None), Source::Live),
            (measured("session", 0.0, Some(NOW - HOUR)), Source::Live),
            (measured("five_hour", 0.0, None), Source::ClaudeCodeCache),
        ] {
            let mut offered = reading(vec![said], Some(NOW - 5));
            offered.source = source;
            let merged = merge(Some(&known), Some(&offered), NOW).unwrap();
            assert_eq!(merged.windows[0].percent, 0.0, "{source:?}");
            assert_eq!(merged.windows[0].resets_at, None, "{source:?}");
            assert_eq!(merged.source, source, "and it is what said so");
            assert_eq!(
                merge(Some(&merged), Some(&offered), NOW).as_ref(),
                Some(&merged),
                "a repeat changes nothing"
            );
        }

        let untimed = reading(vec![measured("five_hour", 0.0, None)], None);
        let merged = merge(Some(&known), Some(&untimed), NOW).unwrap();
        assert_eq!(merged.windows[0].percent, 0.0);
        assert_eq!(
            merged.observed_at,
            Some(NOW - 2 * HOUR),
            "a reading that says no time moved nothing forward"
        );
    }
}
