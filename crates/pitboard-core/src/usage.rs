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
    /// pitboard's own, and absent in a reading taken before pitboard read this field.
    #[serde(default)]
    pub severity: Option<String>,
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Snapshot {
    pub windows: Vec<Window>,
    pub observed_at: Option<i64>,
    pub account_uuid: Option<String>,
    pub source: Source,
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
        "accountUuid": "9aeb9c89-316c-4344-84c5-603d71dc5c9a",
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
            Some("9aeb9c89-316c-4344-84c5-603d71dc5c9a")
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
}
