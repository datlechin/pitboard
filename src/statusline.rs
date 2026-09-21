//! `pitboard statusline`: one line for Claude Code's status bar, naming the account in use
//! and what every enrolled account has left.
//!
//! Claude Code runs it after every message with its session as JSON on stdin, including the
//! limits of the account that session is using. So this reads only files: no keychain, no
//! network, and nothing written. The other accounts show pitboard's last reading of them,
//! with its age once that is worth knowing.

use crate::context::Context;
use crate::state::State;
use crate::ui::{self, BOLD, DIM, paint};
use crate::usage::Snapshot;
use serde_json::Value;
use std::collections::HashMap;

/// Older than this, a remembered reading shows its age.
const FRESH_FOR: i64 = 15 * 60;

/// The five-hour and weekly shares, in that order, when known.
type Shares = (Option<f64>, Option<f64>);

/// What Claude Code passes for the session's own account, under `rate_limits`.
fn session_shares(input: &Value, now: i64) -> Shares {
    let share = |window: &str| {
        let w = input.get("rate_limits")?.get(window)?;
        let resets_at = w.get("resets_at").and_then(Value::as_i64);
        let used = w.get("used_percentage")?.as_f64()?;
        Some(if resets_at.is_some_and(|at| at <= now) {
            0.0
        } else {
            used
        })
    };
    (share("five_hour"), share("seven_day"))
}

/// A remembered reading. A window whose reset has passed since is shown as reset.
fn remembered_shares(snapshot: &Snapshot, now: i64) -> Shares {
    let share = |kinds: &[&str]| {
        snapshot
            .windows
            .iter()
            .find(|w| w.scope.is_none() && kinds.contains(&w.kind.as_str()))
            .map(|w| {
                if w.resets_at.is_some_and(|at| at <= now) {
                    0.0
                } else {
                    w.percent
                }
            })
    };
    (
        share(&["session", "five_hour"]),
        share(&["weekly_all", "seven_day"]),
    )
}

fn shares(shares: Shares) -> String {
    let one = |share: Option<f64>| match share {
        Some(p) => paint(ui::level(p), format!("{p:.0}%")),
        None => paint(DIM, "?"),
    };
    format!("{}{}{}", one(shares.0), paint(DIM, "·"), one(shares.1))
}

/// The line itself. `signed_in` is the account Claude Code's config names.
pub fn render(
    input: &Value,
    state: &State,
    signed_in: Option<&str>,
    remembered: &HashMap<String, Snapshot>,
    now: i64,
) -> String {
    let current = signed_in.and_then(|uuid| state.by_uuid(uuid));
    let mut parts = vec![format!(
        "{} {}",
        paint(BOLD, current.map_or("unenrolled", |a| a.label.as_str())),
        shares(session_shares(input, now))
    )];
    for account in &state.accounts {
        if current.is_some_and(|c| c.account_uuid == account.account_uuid) {
            continue;
        }
        let reading = remembered.get(&account.account_uuid);
        let age = reading
            .and_then(|r| r.observed_at)
            .filter(|at| now - at > FRESH_FOR)
            .map(|at| paint(DIM, format!(" ({})", crate::time::span(now - at))))
            .unwrap_or_default();
        parts.push(format!(
            "{} {}{age}",
            paint(DIM, &account.label),
            shares(reading.map_or((None, None), |r| remembered_shares(r, now)))
        ));
    }
    parts.join("  ")
}

/// Reads Claude Code's JSON from `input`. Never fails: a status bar has nowhere to show an
/// error, so whatever cannot be read is left out.
pub fn run(ctx: &Context, input: &str) -> String {
    let input: Value = serde_json::from_str(input).unwrap_or(Value::Null);
    let state = crate::state::load(ctx).unwrap_or_default();
    let signed_in = crate::claude::load_config(ctx)
        .ok()
        .as_ref()
        .and_then(crate::claude::identity)
        .map(|id| id.account_uuid);
    render(
        &input,
        &state,
        signed_in.as_deref(),
        &crate::readings::load(ctx),
        crate::time::now(),
    )
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

    fn plain(styled: &str) -> String {
        anstream::adapter::strip_str(styled).to_string()
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
        let line = plain(&render(
            &input,
            &state(),
            Some("work-uuid"),
            &remembered,
            NOW,
        ));
        assert_eq!(line, "work 46%·70%  personal 12%·40%  side 0%·0% (3h 00m)");
    }

    #[test]
    fn what_is_unknown_is_shown_as_unknown_not_as_zero() {
        let line = plain(&render(&json!({}), &state(), None, &HashMap::new(), NOW));
        assert_eq!(line, "unenrolled ?·?  work ?·?  personal ?·?  side ?·?");
    }
}
