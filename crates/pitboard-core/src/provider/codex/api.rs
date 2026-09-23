//! What pitboard asks OpenAI about a Codex login.
//!
//! Read from codex-cli 0.154.0. Two requests: one to exchange a refresh token, one to ask
//! how much of a plan is left. Neither is a model request and neither costs quota.

use crate::api::{agent, retry_after, server_time, test_base};
use crate::context::Context;
use crate::provider::{ProviderError, ProviderId};
use crate::usage::{Snapshot, Source, Window};
use serde_json::Value;

/// The client Codex renews as. Its own, not a first-party one: a login issued to this
/// client can only be renewed as it.
pub(crate) const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const USAGE_BASE: &str = "https://chatgpt.com/backend-api";

fn token_url(ctx: &Context) -> String {
    test_base(ctx).map_or_else(
        || TOKEN_URL.to_string(),
        |base| format!("{base}/oauth/token"),
    )
}

fn usage_url(ctx: &Context) -> String {
    test_base(ctx).map_or_else(
        || format!("{USAGE_BASE}/wham/usage"),
        |base| format!("{base}/wham/usage"),
    )
}

fn network(detail: String) -> ProviderError {
    ProviderError::Network {
        service: ProviderId::Codex.service(),
        detail,
    }
}

fn unexpected(status: u16) -> ProviderError {
    ProviderError::Unexpected {
        service: ProviderId::Codex.service(),
        status,
    }
}

/// Fresh tokens for a refresh chain.
///
/// The body is JSON rather than form-encoded, which is what Codex sends and is not what its
/// own authorisation-code exchange sends. Getting that wrong is a 400 with nothing useful
/// in it.
pub(crate) struct Fresh {
    pub id_token: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    /// The server's own clock, from the `Date` header, in epoch seconds.
    pub at: Option<i64>,
}

pub(crate) fn renew(ctx: &Context, refresh_token: &str) -> Result<Fresh, ProviderError> {
    let body = serde_json::json!({
        "client_id": CLIENT_ID,
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
    });
    let mut response = agent()
        .post(token_url(ctx))
        .header("Content-Type", "application/json")
        .send(body.to_string())
        .map_err(|e| network(e.to_string()))?;
    let status = response.status().as_u16();
    let at = server_time(response.headers());
    let wait = retry_after(response.headers());
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|e| network(e.to_string()))?;
    match status {
        200 => {
            let body: Value =
                serde_json::from_str(&text).map_err(|e| ProviderError::Malformed(e.to_string()))?;
            let string = |key: &str| body.get(key).and_then(Value::as_str).map(str::to_owned);
            let fresh = Fresh {
                id_token: string("id_token"),
                access_token: string("access_token"),
                refresh_token: string("refresh_token"),
                at,
            };
            if fresh.access_token.is_none() {
                return Err(ProviderError::Malformed(
                    "the answer carried no access token".into(),
                ));
            }
            Ok(fresh)
        }
        // A refresh token that has been spent, revoked, or signed out from elsewhere. The
        // body names which; the distinction does not change what pitboard can do about it.
        400 | 401 => Err(ProviderError::InvalidGrant),
        429 => Err(ProviderError::RateLimited { retry_after: wait }),
        other => Err(unexpected(other)),
    }
}

/// How much of the plan is left, for the account this access token belongs to.
///
/// A standalone GET: no model request, no quota spent. `ChatGPT-Account-ID` is required and
/// comes out of the login document, which is why usage takes the whole credential rather
/// than a bare token.
pub(crate) fn usage(
    ctx: &Context,
    access_token: &str,
    account_id: &str,
    now: i64,
) -> Result<Snapshot, ProviderError> {
    let mut response = agent()
        .get(usage_url(ctx))
        .header("Authorization", format!("Bearer {access_token}"))
        .header("ChatGPT-Account-ID", account_id)
        .call()
        .map_err(|e| network(e.to_string()))?;
    match response.status().as_u16() {
        200 => {
            let text = response
                .body_mut()
                .read_to_string()
                .map_err(|e| network(e.to_string()))?;
            let body: Value =
                serde_json::from_str(&text).map_err(|e| ProviderError::Malformed(e.to_string()))?;
            Ok(snapshot(&body, account_id, now))
        }
        401 | 403 => Err(ProviderError::Unauthorized),
        429 => Err(ProviderError::RateLimited {
            retry_after: retry_after(response.headers()),
        }),
        other => Err(unexpected(other)),
    }
}

/// OpenAI's answer in pitboard's own shape.
///
/// A window that will not normalise is dropped rather than drawn, the same rule the Claude
/// Code side has always used: a number nobody can explain is worse than no number.
fn snapshot(body: &Value, account_id: &str, now: i64) -> Snapshot {
    let windows = ["primary", "secondary"]
        .into_iter()
        .filter_map(|which| window(body.get(which)?, which))
        .collect();
    Snapshot {
        windows,
        observed_at: Some(now),
        account_uuid: Some(account_id.to_string()),
        source: Source::Live,
    }
}

fn window(value: &Value, which: &str) -> Option<Window> {
    let percent = value.get("used_percent").and_then(Value::as_f64)?;
    if !percent.is_finite() || percent < 0.0 {
        return None;
    }
    let minutes = value.get("window_minutes").and_then(Value::as_i64);
    Some(Window {
        kind: kind(minutes, which),
        scope: None,
        percent,
        resets_at: value.get("resets_at").and_then(moment),
        is_active: true,
        severity: None,
    })
}

/// The same vocabulary the rest of pitboard already uses where the lengths match, so a
/// five-hour window reads as one whichever tool it came from. Anything else is named by its
/// own length rather than forced into a word that would be wrong.
fn kind(minutes: Option<i64>, which: &str) -> String {
    match minutes {
        Some(300) => "five_hour".into(),
        Some(10_080) => "seven_day".into(),
        Some(m) if m % (60 * 24) == 0 => format!("{}_day", m / (60 * 24)),
        Some(m) if m % 60 == 0 => format!("{}_hour", m / 60),
        Some(m) => format!("{m}_minute"),
        None => which.to_string(),
    }
}

/// Epoch seconds, however the answer put them.
///
/// Measured only as a number on this machine's account. The string form is read too because
/// the field is a timestamp and reading one shape and silently dropping the other would
/// show a limit with no reset time and no reason.
fn moment(value: &Value) -> Option<i64> {
    if let Some(seconds) = value.as_i64() {
        // A value in milliseconds would be around a thousand times too large; nothing
        // measured has sent one, and a date in the year 55000 is worth refusing.
        return (seconds > 0 && seconds < 100_000_000_000).then_some(seconds);
    }
    value
        .as_str()?
        .parse::<jiff::Timestamp>()
        .ok()
        .map(|t| t.as_second())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_windows_keep_the_vocabulary_the_rest_of_pitboard_uses() {
        assert_eq!(kind(Some(300), "primary"), "five_hour");
        assert_eq!(kind(Some(10_080), "secondary"), "seven_day");
        assert_eq!(kind(Some(1440), "primary"), "1_day");
        assert_eq!(kind(Some(180), "primary"), "3_hour");
        assert_eq!(kind(Some(90), "primary"), "90_minute");
        assert_eq!(
            kind(None, "primary"),
            "primary",
            "a window with no length is named by which one it is, not by a guess"
        );
    }

    #[test]
    fn a_reset_time_reads_as_seconds_or_as_a_timestamp() {
        assert_eq!(
            moment(&serde_json::json!(1_789_935_600)),
            Some(1_789_935_600)
        );
        assert_eq!(
            moment(&serde_json::json!("2026-09-21T10:30:00Z")),
            Some(1_789_986_600)
        );
        for bad in [
            serde_json::json!(null),
            serde_json::json!(0),
            serde_json::json!(-5),
            serde_json::json!(1_789_935_600_000i64),
            serde_json::json!("not a time"),
        ] {
            assert_eq!(moment(&bad), None, "{bad}");
        }
    }

    /// The shape OpenAI answers with, as measured, turned into the shape everything else
    /// here already draws.
    #[test]
    fn an_answer_becomes_windows_the_rest_of_pitboard_can_draw() {
        let body = serde_json::json!({
            "limit_name": "gpt-5",
            "plan_type": "pro",
            "primary": {"used_percent": 12.5, "window_minutes": 300, "resets_at": 1_789_935_600i64},
            "secondary": {"used_percent": 64.0, "window_minutes": 10_080, "resets_at": 1_790_000_000i64},
        });
        let snapshot = snapshot(&body, "acc-1", 1_789_900_000);
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.windows[0].kind, "five_hour");
        assert!((snapshot.windows[0].percent - 12.5).abs() < f64::EPSILON);
        assert_eq!(snapshot.windows[1].kind, "seven_day");
        assert_eq!(snapshot.account_uuid.as_deref(), Some("acc-1"));
    }

    /// A number nobody can explain is worse than no number, which is the rule the Claude
    /// Code side has always used.
    #[test]
    fn a_window_that_will_not_normalise_is_dropped_rather_than_drawn() {
        let body = serde_json::json!({
            "primary": {"used_percent": "quite a lot", "window_minutes": 300},
            "secondary": {"used_percent": -1.0, "window_minutes": 10_080},
        });
        assert!(snapshot(&body, "acc-1", 0).windows.is_empty());
    }
}
