//! `pitboard statusline`: one line for Claude Code's status bar, naming the account in use
//! and what every enrolled account has left.
//!
//! Claude Code runs it after every message with its session as JSON on stdin, including the
//! limits of the account that session is using. So this reads only files: no keychain, no
//! network, and nothing written. The other accounts show pitboard's last reading of them,
//! with its age once that is worth knowing.

use crate::context::Context;
use crate::state::State;
use crate::usage::{Snapshot, Source, Window};
use serde_json::Value;
use std::collections::HashMap;

/// Older than this, a remembered reading shows its age.
const FRESH_FOR: i64 = 15 * 60;

/// The share of the five-hour and weekly limits already used, when known.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Shares {
    pub five_hour: Option<f64>,
    pub weekly: Option<f64>,
}

/// An enrolled account other than the one in use.
#[derive(Debug, PartialEq)]
pub struct Entry {
    pub label: String,
    pub shares: Shares,
    /// Seconds since this was measured, once that is worth knowing.
    pub age: Option<i64>,
}

#[derive(Debug, PartialEq)]
pub struct StatusLine {
    /// The account Claude Code's config names, or `None` when it is not enrolled.
    pub current: Option<String>,
    /// What Claude Code passed for the session's own account.
    pub session: Shares,
    pub others: Vec<Entry>,
}

/// A window whose reset has passed since counts as reset.
fn used_share(percent: f64, resets_at: Option<i64>, now: i64) -> f64 {
    if resets_at.is_some_and(|at| at <= now) {
        0.0
    } else {
        percent
    }
}

/// What Claude Code passes for the session's own account, under `rate_limits`.
fn session_shares(input: &Value, now: i64) -> Shares {
    let share = |window: &str| {
        let w = input.get("rate_limits")?.get(window)?;
        Some(used_share(
            w.get("used_percentage")?.as_f64()?,
            w.get("resets_at").and_then(Value::as_i64),
            now,
        ))
    };
    Shares {
        five_hour: share("five_hour"),
        weekly: share("seven_day"),
    }
}

/// A remembered reading.
fn remembered_shares(snapshot: &Snapshot, now: i64) -> Shares {
    let share = |kinds: &[&str]| {
        snapshot
            .windows
            .iter()
            .find(|w| w.scope.is_none() && kinds.contains(&w.kind.as_str()))
            .map(|w| used_share(w.percent, w.resets_at, now))
    };
    Shares {
        five_hour: share(&["session", "five_hour"]),
        weekly: share(&["weekly_all", "seven_day"]),
    }
}

/// `signed_in` is the account Claude Code's config names.
fn line(
    input: &Value,
    state: &State,
    signed_in: Option<&str>,
    remembered: &HashMap<String, Snapshot>,
    now: i64,
) -> StatusLine {
    let current = signed_in.and_then(|uuid| state.by_uuid(uuid));
    let others = state
        .accounts
        .iter()
        .filter(|a| current.is_none_or(|c| c.account_uuid != a.account_uuid))
        .map(|account| {
            let reading = remembered.get(&account.account_uuid);
            Entry {
                label: account.label.clone(),
                shares: reading.map_or_else(Shares::default, |r| remembered_shares(r, now)),
                age: reading
                    .and_then(|r| r.observed_at)
                    .map(|at| now - at)
                    .filter(|age| *age > FRESH_FOR),
            }
        })
        .collect();
    StatusLine {
        current: current.map(|a| a.label.clone()),
        session: session_shares(input, now),
        others,
    }
}

/// What Claude Code passed about the session's own account, as a reading worth keeping.
/// Free: these are numbers the session already had, not a question asked of anyone.
fn session_snapshot(input: &Value, uuid: &str, now: i64) -> Option<Snapshot> {
    let limits = input.get("rate_limits")?;
    let window = |name: &str| -> Option<Window> {
        let w = limits.get(name)?;
        Some(Window {
            kind: name.to_string(),
            scope: None,
            percent: w.get("used_percentage")?.as_f64()?,
            resets_at: w.get("resets_at").and_then(Value::as_i64),
            is_active: true,
        })
    };
    let windows: Vec<Window> = ["five_hour", "seven_day"]
        .iter()
        .filter_map(|name| window(name))
        .collect();
    (!windows.is_empty()).then(|| Snapshot {
        windows,
        observed_at: Some(now),
        account_uuid: Some(uuid.to_string()),
        source: Source::Live,
    })
}

/// Reads Claude Code's session JSON. Never fails: a status bar has nowhere to show an
/// error, so whatever cannot be read is left out.
///
/// It also keeps what the session told it about the account in use, so the accounts a
/// person actually works in stop reading as unknown without anyone running `pitboard` by
/// hand. It still asks nobody anything: no network, no credential.
pub fn read(ctx: &Context, input: &str) -> StatusLine {
    let input: Value = serde_json::from_str(input).unwrap_or(Value::Null);
    let state = crate::state::load(ctx).unwrap_or_default();
    let signed_in = crate::claude::load_config(ctx)
        .ok()
        .as_ref()
        .and_then(crate::claude::identity)
        .map(|id| id.account_uuid);
    let now = crate::time::now();
    let remembered = crate::readings::load(ctx);
    if let Some(uuid) = signed_in.as_deref() {
        let stale = remembered
            .get(uuid)
            .and_then(|r| r.observed_at)
            .is_none_or(|at| now - at > FRESH_FOR);
        if stale && let Some(snapshot) = session_snapshot(&input, uuid, now) {
            crate::readings::remember(ctx, &[(uuid.to_string(), snapshot)]);
        }
    }
    line(&input, &state, signed_in.as_deref(), &remembered, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Account;
    use crate::usage::{Source, Window};
    use serde_json::json;

    const NOW: i64 = 1_789_935_000;

    fn state() -> State {
        let account = |label: &str| Account {
            label: label.into(),
            account_uuid: format!("{label}-uuid"),
            email: format!("{label}@example.com"),
            organization_uuid: "o".into(),
            oauth_account: json!({}),
            parked: None,
        };
        State {
            accounts: vec![account("work"), account("personal"), account("side")],
            ..State::default()
        }
    }

    fn reading(five: f64, week: f64, observed_at: i64, resets_at: i64) -> Snapshot {
        let window = |kind: &str, percent| Window {
            kind: kind.into(),
            scope: None,
            percent,
            resets_at: Some(resets_at),
            is_active: false,
        };
        Snapshot {
            windows: vec![window("session", five), window("weekly_all", week)],
            observed_at: Some(observed_at),
            account_uuid: None,
            source: Source::Remembered,
        }
    }

    fn shares(five_hour: f64, weekly: f64) -> Shares {
        Shares {
            five_hour: Some(five_hour),
            weekly: Some(weekly),
        }
    }

    #[test]
    fn names_the_account_in_use_and_what_every_account_has_left() {
        let input = json!({"rate_limits": {
            "five_hour": {"used_percentage": 46.4, "resets_at": NOW + 600},
            "seven_day": {"used_percentage": 70.0, "resets_at": NOW + 86_400}
        }});
        let remembered = HashMap::from([
            (
                "personal-uuid".to_string(),
                reading(12.0, 40.0, NOW - 60, NOW + 600),
            ),
            (
                "side-uuid".to_string(),
                reading(90.0, 88.0, NOW - 3 * 3_600, NOW - 1),
            ),
        ]);
        let line = line(&input, &state(), Some("work-uuid"), &remembered, NOW);
        assert_eq!(line.current.as_deref(), Some("work"));
        assert_eq!(line.session, shares(46.4, 70.0));
        assert_eq!(
            line.others,
            [
                Entry {
                    label: "personal".into(),
                    shares: shares(12.0, 40.0),
                    age: None,
                },
                Entry {
                    label: "side".into(),
                    shares: shares(0.0, 0.0),
                    age: Some(3 * 3_600),
                },
            ],
            "a window past its reset counts as reset, and an old reading says how old"
        );
    }

    #[test]
    fn what_is_unknown_is_left_unknown_not_zero() {
        let line = line(&json!({}), &state(), None, &HashMap::new(), NOW);
        assert_eq!(line.current, None);
        assert_eq!(line.session, Shares::default());
        assert!(line.others.iter().all(|e| e.shares == Shares::default()));
        assert_eq!(line.others.len(), 3);
    }
}
